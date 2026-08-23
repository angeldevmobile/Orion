//! Recorrido paralelo con reanudación.
//!
//! `extract_to` recorre una lista de URLs **con una sola pestaña, en serie**.
//! Sirve, pero deja la máquina a un octavo de gas: mientras una página carga
//! —que es esperar a la red, no calcular— el resto del navegador está parado.
//!
//! `crawl` abre **N pestañas y las conduce en paralelo desde N hilos de
//! sistema**. Ahí está el músculo que Orion tiene y un scraper de Python no:
//! hilos de verdad sobre el mismo socket CDP, que el transporte ya multiplexa
//! (cada respuesta vuelve a su emisor por `id`). No es `asyncio` cooperativo, son
//! núcleos trabajando a la vez mientras otras pestañas esperan la red.
//!
//! Y **reanuda**. Un recorrido de diez mil páginas que se corta en la siete mil
//! no puede empezar de cero: se anota cada URL terminada en un archivo de
//! progreso y, al volver a arrancar, las hechas se saltan. Es lo que separa un
//! juguete de algo que corre de noche en un servidor.
//!
//! ```orion
//! r = web.crawl(b, {
//!     urls:    urls,              -- la lista de páginas
//!     row:     ".card",
//!     schema:  { nombre: ".title", precio: ".price|num" },
//!     out:     "catalogo.csv",
//!     workers: 8,                 -- 8 pestañas en paralelo
//!     resume:  yes                -- retoma donde se cortó
//! })
//! ```
//!
//! En Python esto es Scrapy: un framework entero, otro fichero de settings, otra
//! mentalidad. Aquí es una llamada, apoyada en piezas que ya existen —el pool de
//! pestañas, `extract`, el volcador en streaming de `extract_to`—.

use std::collections::{HashSet, VecDeque};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::cdp::Conn;
use super::extract::{self, Campo, Volcador};
use super::launch::Tuning;

pub struct Opciones {
    pub urls:     Vec<String>,
    pub fila_sel: String,
    pub campos:   Vec<Campo>,
    pub salida:   String,
    pub workers:  usize,
    pub chunk:    usize,
    pub wait_ms:  u64,
    pub resume:   bool,
}

pub struct Resumen {
    pub filas:    usize,
    pub ok:       usize,
    pub vacias:   Vec<String>,
    pub errores:  Vec<String>,
    pub archivos: Vec<String>,
    pub saltadas: usize,
    pub muertos:  Vec<String>,
    pub workers:  usize,
}

/// Abre una pestaña nueva sin registrarla en el mapa global de handles: la
/// posee el recorrido y la cierra al terminar. Devuelve `(targetId, sessionId)`.
fn abrir_pestaña(conn: &Conn, timeout: Duration) -> Result<(String, String), String> {
    let creado = conn.call("Target.createTarget",
        serde_json::json!({ "url": "about:blank" }), None, timeout)?;
    let target = creado["targetId"].as_str()
        .ok_or("crawl: the browser returned no targetId")?.to_string();
    let adj = conn.call("Target.attachToTarget",
        serde_json::json!({ "targetId": target, "flatten": true }), None, timeout)?;
    let session = adj["sessionId"].as_str()
        .ok_or("crawl: the browser returned no sessionId")?.to_string();
    conn.call("Page.enable", serde_json::json!({}), Some(&session), timeout)?;
    Ok((target, session))
}

fn navegar(conn: &Conn, session: &str, url: &str, timeout: Duration) -> Result<(), String> {
    let marca = conn.event_mark();
    let r = conn.call("Page.navigate", serde_json::json!({ "url": url }), Some(session), timeout)?;
    if let Some(err) = r.get("errorText").and_then(|e| e.as_str()) {
        return Err(err.to_string());
    }
    let _ = conn.wait_event("Page.loadEventFired", Some(session), marca, timeout)?;
    Ok(())
}

#[derive(Default)]
struct Parcial {
    ok:      usize,
    vacias:  Vec<String>,
    errores: Vec<String>,
}

pub fn crawl(
    conn: Arc<Conn>, timeout: Duration, tuning: Tuning, o: Opciones,
) -> Result<Resumen, String> {
    let headers: Vec<String> = o.campos.iter().map(|c| c.nombre.clone()).collect();
    let progreso_ruta = format!("{}.progress", o.salida);

    let hechas: HashSet<String> = if o.resume {
        std::fs::read_to_string(&progreso_ruta).unwrap_or_default()
            .lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
    } else {
        // Sin reanudar se empieza limpio: el progreso viejo mentiría.
        let _ = std::fs::remove_file(&progreso_ruta);
        HashSet::new()
    };
    let saltadas = o.urls.iter().filter(|u| hechas.contains(*u)).count();
    let pendientes: VecDeque<String> = o.urls.iter()
        .filter(|u| !hechas.contains(*u)).cloned().collect();

    let volcador = if o.resume && !hechas.is_empty() {
        Volcador::continuar(&o.salida, headers.clone(), o.chunk)
    } else {
        Volcador::nuevo(&o.salida, headers.clone(), o.chunk)
    }.map_err(|e| format!("browser.crawl: {e}"))?;

    let progreso = std::fs::OpenOptions::new()
        .create(true).append(true).open(&progreso_ruta)
        .map_err(|e| format!("browser.crawl: could not open the progress file: {e}"))?;

    // Cuántas pestañas de verdad: ni más que URLs pendientes, ni cero.
    let n = o.workers.clamp(1, pendientes.len().max(1));

    let cola     = Arc::new(Mutex::new(pendientes));
    let escritor = Arc::new(Mutex::new(volcador));
    let progreso = Arc::new(Mutex::new(progreso));
    let vistos   = Arc::new(Mutex::new(HashSet::<String>::new()));

    let mut hilos = Vec::with_capacity(n);
    for _ in 0..n {
        let conn     = Arc::clone(&conn);
        let cola     = Arc::clone(&cola);
        let escritor = Arc::clone(&escritor);
        let progreso = Arc::clone(&progreso);
        let vistos   = Arc::clone(&vistos);
        let headers  = headers.clone();
        let campos   = o.campos.clone();
        let fila_sel = o.fila_sel.clone();
        let t        = tuning.clone();
        let ms       = o.wait_ms;

        hilos.push(std::thread::spawn(move || -> Parcial {
            let mut par = Parcial::default();

            // Cada hilo abre su propia pestaña. Si no puede, no participa: el
            // resto del recorrido sigue con menos manos, que es mejor que abortar.
            let (target, session) = match abrir_pestaña(&conn, timeout) {
                Ok(x) => x,
                Err(e) => { par.errores.push(format!("(page) {e}")); return par; }
            };

            loop {
                let url = { cola.lock().unwrap().pop_front() };
                let Some(url) = url else { break };

                if let Err(e) = navegar(&conn, &session, &url, timeout) {
                    par.errores.push(format!("{url}: {e}"));
                    continue;
                }
                match extract::extract(&conn, &session, &fila_sel, &campos, ms, &t) {
                    Ok(r) => {
                        if r.filas == 0 {
                            par.vacias.push(url.clone());
                        } else if let Some(arr) = r.json.as_array() {
                            let mut w = escritor.lock().unwrap();
                            let mut vis = vistos.lock().unwrap();
                            for reg in arr {
                                let fila: Vec<String> = headers.iter().map(|h| {
                                    let v = reg.get(h).unwrap_or(&serde_json::Value::Null);
                                    if !v.is_null() && v != &serde_json::Value::String(String::new()) {
                                        vis.insert(h.clone());
                                    }
                                    extract::a_texto(v)
                                }).collect();
                                if let Err(e) = w.escribir(fila) {
                                    par.errores.push(format!("(escritura) {e}"));
                                    drop(w); drop(vis);
                                    let _ = conn.call("Target.closeTarget",
                                        serde_json::json!({ "targetId": target }), None, timeout);
                                    return par;
                                }
                            }
                        }
                        par.ok += 1;
                        if let Ok(mut pf) = progreso.lock() {
                            let _ = writeln!(pf, "{url}");
                            let _ = pf.flush();
                        }
                    }
                    Err(e) => par.errores.push(format!("{url}: {e}")),
                }
            }

            let _ = conn.call("Target.closeTarget",
                serde_json::json!({ "targetId": target }), None, timeout);
            par
        }));
    }

    // Fundir los parciales.
    let mut ok = 0usize;
    let mut vacias = Vec::new();
    let mut errores = Vec::new();
    for h in hilos {
        match h.join() {
            Ok(par) => { ok += par.ok; vacias.extend(par.vacias); errores.extend(par.errores); }
            Err(_)  => errores.push("a crawl worker thread died".into()),
        }
    }

    let volcador = Arc::try_unwrap(escritor)
        .map_err(|_| "browser.crawl: the writer was still shared".to_string())?
        .into_inner().unwrap();
    let (filas, archivos) = volcador.cerrar().map_err(|e| format!("browser.crawl: {e}"))?;

    let vis = Arc::try_unwrap(vistos).map(|m| m.into_inner().unwrap()).unwrap_or_default();
    let muertos: Vec<String> = if filas > 0 {
        headers.iter().filter(|h| !vis.contains(*h)).cloned().collect()
    } else {
        Vec::new()
    };

    Ok(Resumen { filas, ok, vacias, errores, archivos, saltadas, muertos, workers: n })
}
