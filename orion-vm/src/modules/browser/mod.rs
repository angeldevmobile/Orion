//! Módulo `browser` — automatización web sobre CDP.
//!
//! API en inglés y de un solo nivel: funciones del módulo con el handle como
//! primer argumento, igual que `db`, `frame` o `ws`. No hay builders, ni
//! cadenas de acciones, ni contextos anidados.
//!
//! ```orion
//! use "browser" as web
//!
//! with b = web.open() {
//!     p = web.page(b)
//!     web.goto(p, "https://ejemplo.dev")
//!     show(web.title(p))
//! }
//! ```
//!
//! El `with` funciona sin nada extra: desugara a `web.free(b)` incluso si el
//! cuerpo lanza un error, y `free` cierra en cascada las pestañas del navegador.
//!
//! Esta es la primera entrega: transporte, arranque y navegación. La extracción
//! declarativa, la interacción (click/type) y las ventanas emergentes se apoyan
//! sobre esto y llegan después.

pub mod capture;
pub mod cdp;
pub mod crawl;
pub mod discover;
pub mod dom;
pub mod extract;
pub mod files;
pub mod form;
pub mod input;
pub mod launch;
pub mod state;

use crate::eval_value::EvalValue;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use cdp::Conn;
use launch::{LaunchOpts, Launched, Tuning};

//    Registro de handles
//
// Navegadores y pestañas comparten numeración y mapa: así `free(h)` funciona
// con cualquiera de los dos, que es lo que necesita `with` para no obligar al
// programa a saber qué clase de cosa está soltando.

struct BrowserState {
    conn:      Arc<Conn>,
    /// El proceso, si lo arrancamos nosotros. `None` con `attach`: ese
    /// navegador es del usuario y decide qué hace `free` — el nuestro se mata,
    /// al ajeno solo se le suelta el socket.
    proc:      Option<std::process::Child>,
    exe:       String,
    user_data: std::path::PathBuf,
    temporal:  bool,
    timeout:   Duration,
    tuning:    Tuning,
    pages:     Vec<u64>,
}

struct PageState {
    browser:   u64,
    target_id: String,
    session:   String,
    /// Escucha de red armada: patrón de URL y desde qué evento mirar.
    ///
    /// La marca se guarda al armar y no al recoger porque la petición ocurre
    /// entre las dos llamadas: buscar solo hacia delante desde el momento de
    /// recoger encontraría siempre una lista vacía.
    watch:     Option<(String, u64)>,
}

enum Handle {
    Browser(BrowserState),
    Page(PageState),
}

static HANDLES: OnceLock<Mutex<HashMap<u64, Handle>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn handles() -> &'static Mutex<HashMap<u64, Handle>> {
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn new_id() -> u64 { NEXT_ID.fetch_add(1, Ordering::SeqCst) }

/// Datos de una pestaña más la conexión de su navegador.
///
/// Se devuelven copiados en vez de mantener el candado: una llamada CDP puede
/// tardar segundos y bloquear el registro mientras tanto dejaría clavado a
/// cualquier otra tarea que solo quisiera abrir una pestaña.
fn page_ctx(h: u64) -> Result<(Arc<Conn>, String, Duration, Tuning), String> {
    let reg = handles().lock().unwrap();
    let Some(Handle::Page(p)) = reg.get(&h) else {
        return Err(match reg.get(&h) {
            Some(Handle::Browser(_)) => format!("browser: {h} es un navegador, no una pestaña"),
            _ => format!("browser: la pestaña {h} no existe (¿ya se cerró?)"),
        });
    };
    let session = p.session.clone();
    let Some(Handle::Browser(b)) = reg.get(&p.browser) else {
        return Err(format!("browser: el navegador de la pestaña {h} ya no existe"));
    };
    Ok((Arc::clone(&b.conn), session, b.timeout, b.tuning.clone()))
}

fn browser_ctx(h: u64) -> Result<(Arc<Conn>, Duration, Tuning), String> {
    let reg = handles().lock().unwrap();
    match reg.get(&h) {
        Some(Handle::Browser(b)) => Ok((Arc::clone(&b.conn), b.timeout, b.tuning.clone())),
        Some(Handle::Page(_))    => Err(format!("browser: {h} es una pestaña, no un navegador")),
        None                     => Err(format!("browser: el navegador {h} no existe")),
    }
}

//    Dispatch

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // open(opts?: dict) -> handle → arranca un navegador propio y lo cierra al salir del `with`. opts: { headless, images, gpu, width, height, chrome, user_data, allow, args }
        "open"    => open(&args),
        // attach(puerto|url|opts) → navegador ya abierto; no se cierra al salir
        "attach"  => attach(&args),
        // page(navegador: handle) -> handle → abre una pestaña nueva y devuelve su handle
        "page"    => page(&args),
        // goto(pestaña: handle, url: string) -> nada → navega y espera a que la página cargue
        "goto"    => goto(&args),
        // title(pestaña: handle) -> string → el título de la página
        "title"   => read_str(&args, "document.title", "browser.title"),
        // url(pestaña: handle) -> string → la URL actual, ya con las redirecciones aplicadas
        "url"     => read_str(&args, "location.href", "browser.url"),
        // eval(pestaña: handle, js: string) -> any → ejecuta JavaScript en la página y devuelve el resultado
        "eval"    => eval(&args),
        // reload(pestaña, opts?) → recarga; { cache: no } fuerza traerlo todo del servidor
        "reload"  => do_history(&args, 0),
        // back(pestaña) → vuelve a la página anterior; falla claro si no hay ninguna
        "back"    => do_history(&args, -1),
        // forward(pestaña) → avanza en el historial
        "forward" => do_history(&args, 1),
        // content(pestaña: handle) -> string → el HTML entero tal y como está AHORA, ya con lo que haya pintado el JavaScript
        "content" => read_str(&args, "document.documentElement.outerHTML", "browser.content"),
        // pages(navegador: handle) -> list → los handles de las pestañas abiertas
        "pages"   => pages(&args),

        // Interacción. Todas esperan a que el elemento se pueda usar de verdad
        // (que exista, se vea y nada lo tape) antes de tocarlo.
        //
        // click(pestaña: handle, selector: string, opts?: dict) -> nada → clic real; `force` atraviesa lo que tape sin clicar a ciegas
        "click"      => do_click(&args, "left", 1),
        // dblclick(pestaña: handle, selector: string, opts?: dict) -> nada → doble clic
        "dblclick"   => do_click(&args, "left", 2),
        // rightclick(pestaña: handle, selector: string, opts?: dict) -> nada → clic derecho, para menús contextuales
        "rightclick" => do_click(&args, "right", 1),
        // hover(pestaña: handle, selector: string) -> nada → deja el ratón encima; despliega los menús que reaccionan al paso
        "hover"      => do_hover(&args),
        // drag(pestaña: handle, origen: string, destino: string) -> nada → arrastra de un elemento a otro con eventos de ratón reales
        "drag"       => do_drag(&args),
        // scroll(pestaña: handle, selector_o_y: any) -> nada → desplaza hasta un elemento o hasta una posición
        "scroll"     => do_scroll(&args),
        // type(pestaña: handle, selector: string, texto: string) -> nada → escribe tecla a tecla, disparando los eventos que espera la página
        "type"       => do_type(&args),
        // press(pestaña: handle, tecla: string) -> nada → pulsa una tecla suelta ("Enter", "Tab", "Escape"…)
        "press"      => do_press(&args),
        // select(pestaña: handle, selector: string, valor: string) -> nada → elige una opción de un desplegable
        "select"     => do_select(&args),
        // fill(pestaña, campos, opts?) → rellena un formulario entero en UNA llamada; detecta si cada control es texto, desplegable o casilla
        "fill"       => do_fill(&args),
        // check(pestaña, selector) → marca una casilla con un clic real; no hace nada si ya estaba marcada
        "check"      => do_check(&args, true),
        // uncheck(pestaña, selector) → desmarca una casilla; un radio no se puede desmarcar
        "uncheck"    => do_check(&args, false),

        // Modales y ventanas
        // dialogs(pestaña: handle, politica: string) -> nada → qué hacer con alert/confirm/prompt: "accept", "dismiss"… Se fija ANTES de provocarlos, o el diálogo bloquea la página
        "dialogs"     => do_dialogs(&args),
        // click_opens(pestaña: handle, selector: string) -> handle → clic que abre una pestaña nueva; devuelve el handle de la que se abrió
        "click_opens" => do_click_opens(&args),

        // Lectura del DOM.
        //
        // Las que devuelven contenido esperan a que lo haya; las que informan
        // del estado responden sobre el instante actual y no esperan nunca.
        "wait"    => do_wait(&args),
        // text(pestaña: handle, selector: string) -> string → el texto visible del primer elemento que case, sin espacios sobrantes. ESPERA a que aparezca
        "text"    => query(&args, "browser.text", Espera::Si,
                        "const e = __find(sel); return e ? (e.innerText || e.textContent || '').trim() : null;"),
        // html(pestaña: handle, selector: string) -> string → el HTML de dentro del elemento. ESPERA a que aparezca
        "html"    => query(&args, "browser.html", Espera::Si,
                        "const e = __find(sel); return e ? e.innerHTML : null;"),
        // texts(pestaña: handle, selector: string) -> list → el texto de TODOS los que casen. ESPERA a que haya alguno
        "texts"   => query(&args, "browser.texts", Espera::Si,
                        "return __findAll(sel).map(e => (e.innerText || e.textContent || '').trim());"),
        // exists(pestaña: handle, selector: string) -> bool → si está ahora mismo. NO espera: para preguntar sin bloquear
        "exists"  => query(&args, "browser.exists", Espera::No, "return !!__find(sel);"),
        // count(pestaña: handle, selector: string) -> int → cuántos hay ahora mismo. NO espera
        "count"   => query(&args, "browser.count", Espera::No, "return __findAll(sel).length;"),
        // visible(pestaña: handle, selector: string) -> bool → si existe Y se ve de verdad (con tamaño, sin display:none ni opacidad 0). NO espera
        "visible" => query(&args, "browser.visible", Espera::No, r#"
                        const e = __find(sel);
                        if (!e) return false;
                        const r = e.getBoundingClientRect();
                        const s = getComputedStyle(e);
                        return r.width > 0 && r.height > 0
                            && s.display !== 'none' && s.visibility !== 'hidden' && s.opacity !== '0';
                     "#),
        // attr(pestaña: handle, selector: string, nombre: string) -> string → un atributo del HTML. Para lo que el usuario ha escrito en un campo usa value()
        "attr"    => do_attr(&args),
        // value(pestaña, selector) → lo que un campo contiene AHORA; distinto de attr("value"), que lee el atributo del HTML y no cambia al escribir
        "value"   => do_value(&args),
        // table(pestaña, selector, opts?) → lee una <table> entera como lista de registros, con la cabecera deducida y las celdas combinadas expandidas
        "table"   => do_table(&args),
        // extract(pestaña: handle, selector: string, esquema: dict, espera?: int) -> list → una fila por cada elemento que case `selector`, con los campos del esquema { nombre: "selector" }. Un campo con "|list" recoge todas las coincidencias
        "extract"    => do_extract(&args),
        // extract_to(pestaña: handle, urls: list, selector: string, esquema: dict, salida: string) -> dict → como extract() pero recorriendo varias urls y escribiendo a disco (.csv o .odf) según lee, sin acumular en memoria
        "extract_to" => do_extract_to(&args),
        // discover(pestaña, opts?) → deduce el esquema solo: {row, count, fields, sample}
        "discover"   => do_discover(&args),
        // crawl(navegador, opts) → recorre urls en paralelo con N pestañas, vuelca a disco y reanuda
        "crawl"      => do_crawl(&args),

        // Archivos. Las tres ventanas del sistema operativo que el navegador
        // abriría por su cuenta se interceptan antes de que existan.
        //
        // upload(pestaña, selector, archivos) → adjunta sin que se abra la ventana del sistema; el selector puede ser el <input type=file> o el botón que lo abre
        "upload"   => do_upload(&args),
        // download(pestaña, selector, opts?) → pulsa y espera a que la descarga TERMINE; devuelve {path, name, bytes, url} y no hay diálogo "Guardar como"
        "download" => do_download(&args),
        // pdf(pestaña, ruta, opts?) → imprime la página a PDF sin abrir el diálogo de impresión
        "pdf"      => do_pdf(&args),

        // Captura de red: leer el JSON que la página le pide a su propia API,
        // en vez de deshacer el HTML que ese JSON produjo.
        //
        // watch(pestaña, patrón) → arma la escucha; hay que llamarlo ANTES de provocar la petición
        "watch"   => do_watch(&args),
        // capture(pestaña, opts?) → devuelve lo que la página pidió y casó, con el JSON ya parseado
        "capture" => do_capture(&args),

        // Sesión reutilizable: loguearse una vez en vez de en cada ejecución.
        //
        // save_state(pestaña, ruta) → guarda cookies y almacenamiento en un JSON; ese archivo VALE COMO CREDENCIAL
        "save_state" => do_save_state(&args),
        // load_state(pestaña, ruta) → restaura la sesión guardada; el almacenamiento solo se aplica estando en su origen
        "load_state" => do_load_state(&args),
        // blocked(navegador) → URLs que la lista blanca de open({allow}) ha cortado
        "blocked"    => do_blocked(&args),

        // Captura
        "screenshot" => do_screenshot(&args),
        // `close` y `free` son lo mismo: `free` existe porque es el nombre que
        // invoca el desugar de `with`, y `close` porque es el que la gente
        // escribe cuando cierra a mano.
        "free" | "close" => free(&args),
        // info() -> dict → diagnóstico sin abrir nada: { found, path, env, open_browsers, in_use, open_pages }. Lo primero que mirar cuando "no me funciona"
        "info"    => info(),
        f => Err(format!("browser.{f}() no existe")),
    }
}

//    open(opts?) → handle del navegador

fn open(args: &[EvalValue]) -> Result<EvalValue, String> {
    let opts = parse_opts(args.first())?;
    let tuning = parse_tuning(args.first());
    let timeout = opts.timeout;

    // La lista blanca se valida ANTES de arrancar nada. Comprobarla después
    // dejaba un navegador vivo al que ya nadie tenía handle: ni `with` ni
    // `free` pueden cerrar algo que nunca llegó a registrarse, así que el
    // proceso se quedaba suelto comiendo memoria hasta el reinicio.
    let allow: Vec<String> = match args.first() {
        Some(EvalValue::Dict(m)) => match m.get("allow") {
            Some(EvalValue::List(l)) => l.iter().map(to_str)
                .filter(|s| !s.trim().is_empty()).collect(),
            Some(otro) => vec![to_str(otro)].into_iter()
                .filter(|s| !s.trim().is_empty()).collect(),
            None => Vec::new(),
        },
        _ => Vec::new(),
    };
    if let Some(EvalValue::Dict(m)) = args.first() {
        if m.contains_key("allow") && allow.is_empty() {
            return Err("browser.open: `allow` está vacío. Quítalo si no quieres restringir; \
                        una lista vacía bloquearía todo y es casi seguro un descuido.".into());
        }
    }

    let Launched { child, ws_url, exe, user_data, temporal } = launch::launch(&opts, &tuning)?;
    let limits = cdp::Limits {
        max_events: tuning.max_events,
        idle_poll:  Duration::from_millis(tuning.idle_poll_ms),
        send:       Duration::from_millis(tuning.send_ms),
        nav_settle: Duration::from_millis(tuning.nav_settle_ms),
        retry:      Duration::from_millis(tuning.retry_ms),
    };
    let conn = match Conn::connect(&ws_url, limits) {
        Ok(c) => c,
        Err(e) => {
            // Si no se puede hablar con él, no dejarlo suelto comiendo memoria.
            let mut c = child;
            let _ = c.kill();
            let _ = c.wait();
            if temporal { let _ = std::fs::remove_dir_all(&user_data); }
            return Err(e);
        }
    };

    // Se fija antes de que exista ninguna pestaña: puesta después, la primera
    // navegación ya habría salido sin control.
    if !allow.is_empty() {
        conn.set_allowlist(allow);
    }

    let id = new_id();
    handles().lock().unwrap().insert(id, Handle::Browser(BrowserState {
        conn, proc: Some(child), exe, user_data, temporal, timeout, tuning, pages: Vec::new(),
    }));
    Ok(EvalValue::Int(id as i64))
}

//    attach(puerto | url | opts) → handle de un navegador YA abierto

/// Se engancha a un navegador ya abierto, en vez de arrancar uno.
///
/// Cubre lo que `open` no puede: reutilizar el navegador del usuario, con su
/// sesión y su SSO. Chrome se niega a abrir un perfil que otro Chrome ya tiene
/// bloqueado, de ahí el "cierra tu ventana" de las automatizaciones de
/// escritorio. Requiere que ese navegador se arrancara con
/// `--remote-debugging-port=N`; uno abierto normal no expone CDP.
///
/// ```orion
/// b = web.attach(9222)
/// b = web.attach("ws://127.0.0.1:9222/...")
/// b = web.attach({ port: 9222, timeout: 5000 })
/// ```
///
/// Al cerrar NO se cierra el navegador: ver la rama `None` de `free`.
fn attach(args: &[EvalValue]) -> Result<EvalValue, String> {
    let uso = "browser.attach(puerto | \"ws://...\" | { port: 9222 })";

    let (endpoint, port, opts_dict) = match args.first() {
        Some(EvalValue::Int(p))   => (None, Some(*p as u64), None),
        Some(EvalValue::Str(s))   => (Some(s.clone()), None, None),
        Some(EvalValue::Dict(m))  => {
            let url  = m.get("url").or_else(|| m.get("endpoint")).map(to_str);
            let port = m.get("port").and_then(|v| to_u64(v).ok());
            if url.is_none() && port.is_none() {
                return Err(format!("{uso}: hace falta `port` o `url`"));
            }
            (url, port, Some(m.clone()))
        }
        _ => return Err(format!("{uso}: falta el puerto o la url")),
    };

    let tuning = Tuning::default();
    let timeout = opts_dict
        .as_ref()
        .and_then(|m| m.get("timeout"))
        .and_then(|v| to_u64(v).ok())
        .map(|ms| Duration::from_millis(ms.max(1_000)))
        .unwrap_or_else(|| Duration::from_millis(30_000));

    // Un puerto no es un endpoint: hay que preguntarle al navegador cuál es el
    // suyo, porque lleva un identificador de sesión que cambia en cada arranque.
    let ws_url = match endpoint {
        Some(u) if u.starts_with("ws://") || u.starts_with("wss://") => u,
        Some(u) => descubrir_endpoint(&u, timeout)?,
        None => descubrir_endpoint(&format!("127.0.0.1:{}", port.unwrap()), timeout)?,
    };

    let limits = cdp::Limits {
        max_events: tuning.max_events,
        idle_poll:  Duration::from_millis(tuning.idle_poll_ms),
        send:       Duration::from_millis(tuning.send_ms),
        nav_settle: Duration::from_millis(tuning.nav_settle_ms),
        retry:      Duration::from_millis(tuning.retry_ms),
    };
    let conn = Conn::connect(&ws_url, limits)
        .map_err(|e| format!("browser.attach: no se pudo hablar con el navegador en {ws_url}: {e}"))?;

    // La lista blanca de dominios vale igual estando enganchados.
    if let Some(m) = &opts_dict {
        if let Some(EvalValue::List(xs)) = m.get("allow") {
            let allow: Vec<String> = xs.iter().map(to_str).collect();
            if allow.is_empty() {
                return Err("browser.attach: `allow` está vacío. Quítalo si no quieres \
                            restringir; una lista vacía bloquearía todo.".into());
            }
            conn.set_allowlist(allow);
        }
    }

    let id = new_id();
    handles().lock().unwrap().insert(id, Handle::Browser(BrowserState {
        conn,
        proc: None,                            // no es nuestro: ver `free`
        exe: ws_url,
        user_data: std::path::PathBuf::new(),  // su perfil no se toca
        temporal: false,
        timeout,
        tuning,
        pages: Vec::new(),
    }));
    Ok(EvalValue::Int(id as i64))
}

/// Pregunta a `http://host:puerto/json/version` cuál es su endpoint CDP.
///
/// El error se redacta con cuidado porque este es EL punto donde falla el caso
/// del equipo gestionado, y "conexión rechazada" a secas no dice qué hacer.
fn descubrir_endpoint(host_port: &str, timeout: Duration) -> Result<String, String> {
    let url = format!("http://{host_port}/json/version");
    let resp = ureq::get(&url)
        .timeout(timeout)
        .call()
        .map_err(|e| format!(
            "browser.attach: no hay ningún navegador escuchando en {host_port} ({e}).\n  \
             El navegador tiene que estar arrancado con --remote-debugging-port={}.\n  \
             Uno abierto de la forma normal no expone CDP.",
            host_port.rsplit(':').next().unwrap_or("9222")
        ))?;

    let v: serde_json::Value = resp.into_json()
        .map_err(|e| format!("browser.attach: respuesta ilegible de {url}: {e}"))?;

    v.get("webSocketDebuggerUrl")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!(
            "browser.attach: {host_port} respondió, pero sin endpoint de depuración. \
             ¿Es de verdad un navegador con CDP abierto?"
        ))
}

//    page(browser) → handle de pestaña

fn page(args: &[EvalValue]) -> Result<EvalValue, String> {
    let b = arg_handle(args, 0, "browser.page(navegador)")?;
    let (conn, timeout, _t) = browser_ctx(b)?;

    let creado = conn.call(
        "Target.createTarget",
        serde_json::json!({ "url": "about:blank" }),
        None, timeout,
    )?;
    let target_id = creado["targetId"].as_str()
        .ok_or("browser.page: el navegador no devolvió targetId")?
        .to_string();

    // `flatten` multiplexa la sesión sobre el mismo socket; sin él haría falta
    // una conexión por pestaña.
    let adjunto = conn.call(
        "Target.attachToTarget",
        serde_json::json!({ "targetId": target_id, "flatten": true }),
        None, timeout,
    )?;
    let session = adjunto["sessionId"].as_str()
        .ok_or("browser.page: el navegador no devolvió sessionId")?
        .to_string();

    // Los eventos de página hacen falta para saber cuándo terminó de cargar.
    conn.call("Page.enable", serde_json::json!({}), Some(&session), timeout)?;

    // La interceptación solo se activa si hay lista blanca. Encenderla siempre
    // saldría caro para nada: con `Fetch` puesto, el navegador para en CADA
    // petición y espera una respuesta por el socket.
    if conn.hay_allowlist() {
        conn.call("Fetch.enable", serde_json::json!({}), Some(&session), timeout)?;
    }

    let id = new_id();
    let mut reg = handles().lock().unwrap();
    reg.insert(id, Handle::Page(PageState { browser: b, target_id, session, watch: None }));
    if let Some(Handle::Browser(bs)) = reg.get_mut(&b) {
        bs.pages.push(id);
    }
    Ok(EvalValue::Int(id as i64))
}

//    goto(page, url) → url final

fn goto(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.goto(pestaña, url)")?;
    let url = args.get(1).map(to_str)
        .ok_or("browser.goto(pestaña, url): falta la url")?;
    let (conn, session, timeout, t) = page_ctx(p)?;

    // La marca se toma ANTES de navegar: si no, una carga anterior podría
    // hacerse pasar por esta y `goto` volvería antes de tiempo.
    let marca = conn.event_mark();

    let r = conn.call(
        "Page.navigate",
        serde_json::json!({ "url": url }),
        Some(&session), timeout,
    )?;
    if let Some(err) = r.get("errorText").and_then(|e| e.as_str()) {
        return Err(format!("browser.goto '{url}': {err}"));
    }

    // Que la carga no termine no es un error por sí solo: hay páginas que dejan
    // peticiones abiertas para siempre y su contenido ya está ahí.
    let cargo = conn.wait_event("Page.loadEventFired", Some(&session), marca, timeout)?;

    // Pero hay una causa concreta que sí conviene delatar: un alert/confirm
    // durante la carga deja la página congelada y sin nadie que lo atienda. Sin
    // este aviso, el síntoma es un timeout genérico imposible de diagnosticar.
    if cargo.is_none() {
        if let Some(ev) = conn.wait_event(
            "Page.javascriptDialogOpening", Some(&session), marca, Duration::from_millis(0)
        )? {
            let texto = ev.params.get("message").and_then(|m| m.as_str()).unwrap_or("");
            let tipo  = ev.params.get("type").and_then(|t| t.as_str()).unwrap_or("dialog");
            // Se descarta para no dejar el navegador clavado esperando a nadie.
            let _ = conn.call(
                "Page.handleJavaScriptDialog",
                serde_json::json!({ "accept": false }),
                Some(&session), Duration::from_millis(t.close_ms),
            );
            return Err(format!(
                "browser.goto '{url}': la página abrió un {tipo} ({texto:?}) y quedó bloqueada."
            ));
        }
    }

    match evaluate(&conn, &session, "location.href", timeout) {
        Ok(EvalValue::Str(s)) => Ok(EvalValue::Str(s)),
        _ => Ok(EvalValue::Str(url)),
    }
}

//    title / url / content

fn read_str(args: &[EvalValue], expr: &str, quien: &str) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, &format!("{quien}(pestaña)"))?;
    let (conn, session, timeout, _t) = page_ctx(p)?;
    evaluate(&conn, &session, expr, timeout)
}

//    eval(page, js)

fn eval(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.eval(pestaña, js)")?;
    let js = args.get(1).map(to_str)
        .ok_or("browser.eval(pestaña, js): falta el código")?;
    let (conn, session, timeout, _t) = page_ctx(p)?;
    evaluate(&conn, &session, &js, timeout)
}

/// Evalúa JavaScript en la página y trae el resultado ya convertido.
///
/// `returnByValue` es lo que evita traerse referencias a objetos del DOM: solo
/// cruza el socket el valor final, serializado. Es la decisión que mantiene la
/// memoria de Orion proporcional a los datos pedidos y no al peso de la página.
fn evaluate(conn: &Conn, session: &str, expr: &str, timeout: Duration) -> Result<EvalValue, String> {
    evaluate_awaiting(conn, session, expr, timeout, true)
}

fn evaluate_awaiting(
    conn: &Conn, session: &str, expr: &str, timeout: Duration, await_promise: bool,
) -> Result<EvalValue, String> {
    let r = conn.call(
        "Runtime.evaluate",
        serde_json::json!({
            "expression": expr,
            "returnByValue": true,
            "awaitPromise": await_promise,
        }),
        Some(session), timeout,
    )?;

    // Una excepción del JS llega como resultado correcto de CDP con un
    // `exceptionDetails` dentro; si no se mira, los errores del usuario se
    // convierten en `null` silenciosos.
    if let Some(ex) = r.get("exceptionDetails") {
        let msg = ex.get("exception").and_then(|e| e.get("description")).and_then(|d| d.as_str())
            .or_else(|| ex.get("text").and_then(|t| t.as_str()))
            .unwrap_or("error de JavaScript");
        return Err(format!("browser.eval: {msg}"));
    }

    Ok(json_to_eval(r.get("result").and_then(|x| x.get("value")).cloned()
        .unwrap_or(serde_json::Value::Null)))
}

//    Interacción
//
// Todas esperan al selector antes de actuar. La espera implícita no es una
// comodidad: un scraper que exige acordarse de poner `wait` es un scraper que
// falla de forma intermitente, y esos son los que nadie consigue depurar.

/// Milisegundos de espera de un selector.
///
/// Tres niveles, del más concreto al más general: lo que diga esta llamada, lo
/// que se fijó al abrir el navegador, y el default. Antes solo existía el
/// primero y el último, así que quien quisiera otro plazo tenía que repetirlo en
/// **cada** llamada — que es la forma más cansada de tener una constante fijada.
///
/// El argumento admite un número (los milisegundos) o un dict de opciones, para
/// que el caso simple siga siendo `click(p, sel)` y el complicado no necesite
/// una función aparte.
fn espera_de(args: &[EvalValue], i: usize, t: &Tuning) -> u64 {
    match args.get(i) {
        Some(EvalValue::Dict(m)) => m.get("wait").and_then(|v| to_u64(v).ok()).unwrap_or(t.wait_ms),
        Some(v) => to_u64(v).unwrap_or(t.wait_ms),
        None => t.wait_ms,
    }
}

/// Afinado leído de las opciones de `open`.
///
/// Los parámetros de política viven en la raíz (`wait`, `drag_steps`…) porque se
/// tocan de verdad; los de mecanismo van bajo `tuning` para no ensuciar la API
/// de uso diario. Lo que no se indique conserva su default.
fn parse_tuning(v: Option<&EvalValue>) -> Tuning {
    let mut t = Tuning::default();
    let Some(EvalValue::Dict(m)) = v else { return t };

    let u64_de = |d: &IndexMap<String, EvalValue>, k: &str| -> Option<u64> {
        d.get(k).and_then(|x| to_u64(x).ok())
    };

    // Política — en la raíz de las opciones.
    if let Some(x) = u64_de(m, "wait")          { t.wait_ms       = x; }
    if let Some(x) = u64_de(m, "retry")         { t.retry_ms      = x.max(1); }
    if let Some(x) = u64_de(m, "cdp_margin")    { t.cdp_margin_ms = x; }
    if let Some(x) = u64_de(m, "drag_steps")    { t.drag_steps    = x.max(1) as u32; }
    if let Some(x) = u64_de(m, "force_layers")  { t.force_layers  = x as u32; }
    if let Some(x) = u64_de(m, "iframe_depth")  { t.iframe_depth  = x as u32; }
    if let Some(x) = u64_de(m, "nav_settle")    { t.nav_settle_ms = x; }
    if let Some(x) = m.get("hit_inset").and_then(to_f64) { t.hit_inset = x.max(0.0); }

    // Mecanismo — bajo `tuning`.
    if let Some(EvalValue::Dict(g)) = m.get("tuning") {
        if let Some(x) = u64_de(g, "max_events")    { t.max_events    = x.max(1) as usize; }
        if let Some(x) = u64_de(g, "idle_poll")     { t.idle_poll_ms  = x.max(1); }
        if let Some(x) = u64_de(g, "close_timeout") { t.close_ms      = x; }
        if let Some(x) = u64_de(g, "send_timeout")  { t.send_ms       = x; }
        if let Some(x) = u64_de(g, "cleanup_tries") { t.cleanup_tries = x as u32; }
        if let Some(x) = u64_de(g, "body_buffer")   { t.body_buffer   = x; }
        if let Some(x) = u64_de(g, "total_buffer")  { t.total_buffer  = x; }
    }
    t
}

/// ¿Se pidió atravesar lo que tape el elemento?
fn force_de(args: &[EvalValue], i: usize) -> input::Force {
    match args.get(i) {
        Some(EvalValue::Dict(m)) => match m.get("force").map(truthy) {
            Some(true) => input::Force::Si,
            _ => input::Force::No,
        },
        _ => input::Force::No,
    }
}

fn do_click(args: &[EvalValue], boton: &str, veces: i64) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.click(pestaña, selector)")?;
    let sel = args.get(1).map(to_str)
        .ok_or("browser.click(pestaña, selector): falta el selector")?;
    let (conn, session, timeout, t) = page_ctx(p)?;
    input::click(&conn, &session, &sel, boton, veces,
                 espera_de(args, 2, &t), force_de(args, 2), &t, timeout)?;
    Ok(EvalValue::Bool(true))
}

fn do_hover(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.hover(pestaña, selector)")?;
    let sel = args.get(1).map(to_str)
        .ok_or("browser.hover(pestaña, selector): falta el selector")?;
    let (conn, session, timeout, t) = page_ctx(p)?;
    input::hover(&conn, &session, &sel, espera_de(args, 2, &t), &t, timeout)?;
    Ok(EvalValue::Bool(true))
}

fn do_drag(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.drag(pestaña, origen, destino)")?;
    let a = args.get(1).map(to_str).ok_or("browser.drag: falta el origen")?;
    let z = args.get(2).map(to_str).ok_or("browser.drag: falta el destino")?;
    let (conn, session, timeout, t) = page_ctx(p)?;
    input::drag(&conn, &session, &a, &z, espera_de(args, 3, &t), &t, timeout)?;
    Ok(EvalValue::Bool(true))
}

fn do_scroll(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.scroll(pestaña, dx, dy)")?;
    let dx = args.get(1).and_then(|v| to_f64(v)).unwrap_or(0.0);
    let dy = args.get(2).and_then(|v| to_f64(v)).unwrap_or(0.0);
    let (conn, session, timeout, _t) = page_ctx(p)?;
    input::scroll(&conn, &session, dx, dy, timeout)?;
    Ok(EvalValue::Bool(true))
}

fn do_type(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.type(pestaña, selector, texto)")?;
    let sel = args.get(1).map(to_str).ok_or("browser.type: falta el selector")?;
    let txt = args.get(2).map(to_str).ok_or("browser.type: falta el texto")?;
    // Limpiar antes de escribir es lo que casi siempre se quiere; lo contrario
    // se pide con `{ clear: no }`.
    let limpiar = match args.get(3) {
        Some(EvalValue::Dict(m)) => m.get("clear").map(truthy).unwrap_or(true),
        Some(v) => truthy(v),
        None => true,
    };
    let (conn, session, timeout, t) = page_ctx(p)?;
    input::type_text(&conn, &session, &sel, &txt, limpiar,
                     espera_de(args, 3, &t), force_de(args, 3), &t, timeout)?;
    Ok(EvalValue::Bool(true))
}

fn do_press(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.press(pestaña, tecla)")?;
    let k = args.get(1).map(to_str)
        .ok_or_else(|| format!("browser.press(pestaña, tecla): falta la tecla.\n  Admitidas: {}", input::TECLAS))?;
    let (conn, session, timeout, _t) = page_ctx(p)?;
    input::press(&conn, &session, &k, timeout)?;
    Ok(EvalValue::Bool(true))
}

/// Elige una opción de un `<select>` nativo.
///
/// Un `<select>` abre un desplegable del **sistema operativo**, fuera del DOM:
/// ningún clic sintético ni real puede navegarlo, y por eso Selenium tiene una
/// clase `Select` aparte. Aquí se asigna el valor y se emiten `input` y `change`
/// como haría el navegador, que es lo que el sitio está escuchando.
///
/// Acepta el `value`, el texto visible o el índice: quien escribe el scraper ve
/// el texto en pantalla, no el `value` del HTML.
fn do_select(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.select(pestaña, selector, opción)")?;
    let sel = args.get(1).map(to_str).ok_or("browser.select: falta el selector")?;
    let opt = args.get(2).map(to_str).ok_or("browser.select: falta la opción")?;
    let (conn, session, timeout, t) = page_ctx(p)?;

    let quiere = serde_json::Value::String(opt.clone()).to_string();
    let cuerpo = format!(r#"
    const el = __find(sel);
    if (!el) return {{ ok: false, why: 'no existe' }};
    if (el.tagName !== 'SELECT') return {{ ok: false, why: 'no es un <select> (es <' + el.tagName.toLowerCase() + '>)' }};
    const q = {quiere};
    const ops = Array.from(el.options);
    let i = ops.findIndex(o => o.value === q);
    if (i < 0) i = ops.findIndex(o => (o.textContent || '').trim() === q);
    if (i < 0 && /^[0-9]+$/.test(q)) i = parseInt(q, 10);
    if (i < 0 || i >= ops.length) {{
      return {{ ok: false, why: 'no hay opción ' + JSON.stringify(q),
                opciones: ops.map(o => (o.textContent || '').trim()) }};
    }}
    el.selectedIndex = i;
    el.dispatchEvent(new Event('input',  {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
    return {{ ok: true, value: ops[i].value, text: (ops[i].textContent || '').trim() }};
    "#);

    let ms = espera_de(args, 3, &t);
    let v = evaluate_awaiting(
        &conn, &session,
        &dom::expr_waiting(&sel, &cuerpo, ms, &t),
        Duration::from_millis(ms + t.cdp_margin_ms), true,
    )?;
    let _ = timeout;

    if let EvalValue::Dict(m) = &v {
        if !matches!(m.get("ok"), Some(EvalValue::Bool(true))) {
            let why = m.get("why").map(to_str).unwrap_or_default();
            let ops = match m.get("opciones") {
                Some(EvalValue::List(l)) if !l.is_empty() => format!(
                    "\n  Opciones: {}",
                    l.iter().map(to_str).collect::<Vec<_>>().join(", ")
                ),
                _ => String::new(),
            };
            return Err(format!("browser.select '{sel}': {why}{ops}"));
        }
        return Ok(m.get("value").cloned().unwrap_or(EvalValue::Bool(true)));
    }
    Ok(v)
}

/// Rellena un formulario entero en una sola llamada.
///
/// El tipo de cada control lo decide la página: un texto, un desplegable y una
/// casilla se escriben igual. Obligar a elegir la función según de qué está
/// hecho el campo significa mirar el HTML antes de poder escribir una línea.
fn do_fill(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.fill(pestaña, campos, opts?)")?;
    let EvalValue::Dict(campos) = args.get(1)
        .ok_or("browser.fill(pestaña, campos, opts?): faltan los campos")?
    else {
        return Err("browser.fill: los campos son un diccionario { selector: valor }".into());
    };
    if campos.is_empty() {
        return Err("browser.fill: el diccionario de campos está vacío".into());
    }
    let (conn, session, _timeout, t) = page_ctx(p)?;
    let ms = espera_de(args, 2, &t);

    let opts = match args.get(2) { Some(EvalValue::Dict(m)) => Some(m), _ => None };
    let estricto = opts.and_then(|m| m.get("strict")).map(truthy).unwrap_or(true);
    let por_teclas = opts.and_then(|m| m.get("keys")).map(truthy).unwrap_or(false);

    // El orden del diccionario se conserva (es un IndexMap) y hace falta: un
    // desplegable de provincia que solo se llena al elegir el país tiene que ir
    // después del país.
    let lista: Vec<(String, serde_json::Value)> = campos.iter()
        .map(|(k, v)| (k.clone(), json_de(v)))
        .collect();

    if por_teclas {
        return fill_con_teclas(&conn, &session, &lista, ms, &t, estricto);
    }

    let mut r = form::fill(&conn, &session, &lista, ms, &t)?;

    // Un error de `fill` puede repetir el valor que no se admitió —el de un
    // desplegable, por ejemplo— y ese error acaba en un log o en una consola
    // compartida. Los campos marcados como secretos no cuentan lo suyo.
    if let Some(secretos) = opts.and_then(|m| m.get("secret")) {
        let marcados: Vec<String> = match secretos {
            EvalValue::List(l) => l.iter().map(to_str).collect(),
            // `{ secret: yes }` tapa todos los campos de la llamada.
            EvalValue::Bool(true) => lista.iter().map(|(s, _)| s.clone()).collect(),
            otro => vec![to_str(otro)],
        };
        for (sel, why) in r.fallidos.iter_mut() {
            if marcados.iter().any(|m| m == sel) {
                *why = "el valor no fue admitido (oculto por secret)".into();
            }
        }
    }

    if estricto {
        if let Some(q) = form::queja(&r) { return Err(q); }
    }
    Ok(EvalValue::Int(r.puestos as i64))
}

/// Variante lenta y fiel: teclas de verdad, campo a campo.
///
/// Existe porque hay sitios que solo reaccionan a las pulsaciones —
/// autocompletados, máscaras, buscadores que filtran mientras escribes—, y en
/// esos la asignación directa deja el campo relleno y la aplicación sin
/// enterarse. Cuesta unas 4 ms por carácter, así que es la excepción.
fn fill_con_teclas(
    conn: &Conn, session: &str, lista: &[(String, serde_json::Value)],
    ms: u64, t: &Tuning, estricto: bool,
) -> Result<EvalValue, String> {
    let mut puestos = 0usize;
    let mut ausentes = Vec::new();
    for (sel, val) in lista {
        let texto = match val {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Bool(b) => {
                // Una casilla no se escribe, se pulsa.
                match form::estado_casilla(conn, session, sel, ms, t) {
                    Ok((actual, _)) => {
                        if actual != *b {
                            input::click(conn, session, sel, "left", 1, ms,
                                         input::Force::No, t, Duration::from_millis(ms + t.cdp_margin_ms))?;
                        }
                        puestos += 1;
                        continue;
                    }
                    Err(e) => { ausentes.push(format!("{sel}  ->  {e}")); continue; }
                }
            }
            otro => otro.to_string(),
        };
        match input::type_text(conn, session, sel, &texto, true, ms,
                               input::Force::No, t, Duration::from_millis(ms + t.cdp_margin_ms)) {
            Ok(()) => puestos += 1,
            Err(e) => ausentes.push(format!("{sel}  ->  {e}")),
        }
    }
    if estricto && !ausentes.is_empty() {
        return Err(format!("browser.fill: no se pudieron rellenar {} campo(s):\n    {}",
                           ausentes.len(), ausentes.join("\n    ")));
    }
    Ok(EvalValue::Int(puestos as i64))
}

/// Marca o desmarca una casilla, con un clic real y sin repetirlo si ya estaba.
///
/// Es idempotente a propósito: `check` sobre una casilla ya marcada la dejaría
/// desmarcada si se limitase a pulsar, que es el error que convierte un
/// reintento inocente en el contrario de lo que se pedía.
fn do_check(args: &[EvalValue], querer: bool) -> Result<EvalValue, String> {
    let quien = if querer { "check" } else { "uncheck" };
    let p = arg_handle(args, 0, &format!("browser.{quien}(pestaña, selector)"))?;
    let sel = args.get(1).map(to_str)
        .ok_or_else(|| format!("browser.{quien}(pestaña, selector): falta el selector"))?;
    let (conn, session, timeout, t) = page_ctx(p)?;
    let ms = espera_de(args, 2, &t);

    let (actual, tipo) = form::estado_casilla(&conn, &session, &sel, ms, &t)
        .map_err(|e| format!("browser.{quien}: {e}"))?;

    if tipo == "radio" && !querer {
        return Err(format!(
            "browser.uncheck: '{sel}' es un radio y un radio no se desmarca; marca otro del grupo"
        ));
    }
    if actual != querer {
        input::click(&conn, &session, &sel, "left", 1, ms, force_de(args, 2), &t, timeout)
            .map_err(|e| e.replace("browser.click", &format!("browser.{quien}")))?;
    }
    Ok(EvalValue::Bool(querer))
}

/// Lo que un campo contiene ahora mismo.
///
/// Hace falta aparte de `attr` porque no son lo mismo, y confundirlos es un
/// clásico: `attr(p, sel, "value")` lee el **atributo** del HTML —el que venía
/// escrito en la página— y ese no cambia cuando alguien escribe en el campo.
/// Un `<input>` sin `value=` en el HTML devuelve `null` por ahí aunque tenga
/// texto dentro, que es exactamente el momento en el que uno cree que su
/// `fill` no funcionó.
///
/// Devuelve además lo que corresponde a cada control: el texto de un `<select>`
/// no está en el elemento sino en la opción elegida, y una casilla no tiene
/// texto sino estado.
fn do_value(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.value(pestaña, selector)")?;
    let sel = args.get(1).map(to_str)
        .ok_or("browser.value(pestaña, selector): falta el selector")?;
    let (conn, session, _timeout, t) = page_ctx(p)?;
    let ms = espera_de(args, 2, &t);

    let cuerpo = r#"
    const e = __find(sel);
    if (!e) return null;
    const tipo = (e.type || '').toLowerCase();
    if (tipo === 'checkbox' || tipo === 'radio') return !!e.checked;
    if (e.tagName === 'SELECT') {
      const o = e.options[e.selectedIndex];
      return o ? o.value : null;
    }
    if (e.isContentEditable) return e.textContent;
    if ('value' in e) return e.value;
    return (e.innerText || e.textContent || '');
    "#;

    evaluate_awaiting(
        &conn, &session,
        &dom::expr_waiting(&sel, cuerpo, ms, &t),
        Duration::from_millis(ms + t.cdp_margin_ms), true,
    )
}

/// Lee una `<table>` completa.
fn do_table(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.table(pestaña, selector, opts?)")?;
    let sel = args.get(1).map(to_str)
        .ok_or("browser.table(pestaña, selector, opts?): falta el selector")?;
    let (conn, session, _timeout, t) = page_ctx(p)?;
    let ms = espera_de(args, 2, &t);

    // Con `{ header: no }` no se interpreta ninguna fila como cabecera y todo
    // sale como datos con columnas `col_1`, `col_2`… Hace falta para las tablas
    // que se usan como maquetación, donde la primera fila ya es un dato.
    let con_cabecera = match args.get(2) {
        Some(EvalValue::Dict(m)) => m.get("header").map(truthy).unwrap_or(true),
        _ => true,
    };

    let v = form::table(&conn, &session, &sel, con_cabecera, ms, &t)?;
    Ok(json_to_eval(v.get("filas").cloned().unwrap_or(serde_json::Value::Null)))
}

/// Convierte un valor de Orion a JSON para mandarlo a la página.
fn json_de(v: &EvalValue) -> serde_json::Value {
    match v {
        EvalValue::Str(s)   => serde_json::Value::String(s.clone()),
        EvalValue::Int(i)   => serde_json::Value::from(*i),
        EvalValue::Float(f) => serde_json::Value::from(*f),
        EvalValue::Bool(b)  => serde_json::Value::Bool(*b),
        EvalValue::Null     => serde_json::Value::Null,
        otro                => serde_json::Value::String(to_str(otro)),
    }
}

/// Espera a que una pestaña termine de cargar.
///
/// Se pregunta por `readyState` en vez de esperar el evento de carga porque el
/// evento puede haber pasado ya antes de que nos adjuntáramos, y entonces la
/// espera no terminaría nunca.
fn wait_ready(conn: &Conn, session: &str, ms: u64, t: &Tuning) -> Result<(), String> {
    let reintento = t.retry_ms;
    let js = format!(r#"(() => {{
      return new Promise((resolve) => {{
        if (document.readyState === 'complete') return resolve(true);
        const limite = Date.now() + {ms};
        const iv = setInterval(() => {{
          if (document.readyState === 'complete' || Date.now() >= limite) {{
            clearInterval(iv); resolve(true);
          }}
        }}, {reintento});
      }});
    }})()"#);
    conn.call(
        "Runtime.evaluate",
        serde_json::json!({ "expression": js, "returnByValue": true, "awaitPromise": true }),
        Some(session), Duration::from_millis(ms + t.cdp_margin_ms),
    )?;
    Ok(())
}

/// Política para los diálogos nativos de una pestaña.
///
/// `accept`, `dismiss`, o `answer:<texto>` para un `prompt`. Se declara una vez
/// y vale para toda la sesión, en vez del registro previo a cada acción que usan
/// Playwright y Selenium: un diálogo abierto por un temporizador de la página no
/// tiene ninguna llamada tuya a la que engancharse.
fn do_dialogs(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.dialogs(pestaña, política)")?;
    let (conn, session, _to, _t) = page_ctx(p)?;

    let pol = args.get(1).map(to_str);
    match pol.as_deref() {
        None | Some("") | Some("off") | Some("none") => {
            conn.set_dialog_policy(&session, None);
        }
        Some(v) => {
            let valido = matches!(v, "accept" | "aceptar" | "dismiss" | "rechazar" | "yes" | "ok")
                || v.starts_with("answer:") || v.starts_with("responder:");
            if !valido {
                return Err(format!(
                    "browser.dialogs: política '{v}' desconocida.\n  Admitidas: accept, dismiss, answer:<texto>, off"
                ));
            }
            conn.set_dialog_policy(&session, Some(v.to_string()));
        }
    }
    Ok(EvalValue::Bool(true))
}

/// Clic que abre una pestaña nueva; devuelve el handle de la que se abrió.
///
/// Playwright necesita envolver el clic en `expect_popup` **antes** de hacerlo, y
/// Selenium te hace listar los handles de ventana y adivinar cuál es la nueva.
/// Aquí es una llamada, y la carrera entre el clic y la aparición de la pestaña
/// la resuelve el módulo: la marca de eventos se toma antes de clicar.
fn do_click_opens(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.click_opens(pestaña, selector)")?;
    let sel = args.get(1).map(to_str).ok_or("browser.click_opens: falta el selector")?;
    let (conn, session, timeout, t) = page_ctx(p)?;
    let browser = match handles().lock().unwrap().get(&p) {
        Some(Handle::Page(ps)) => ps.browser,
        _ => return Err(format!("browser.click_opens: la pestaña {p} no existe")),
    };

    // Descubrir targets antes de clicar: si se activa después, el evento de la
    // pestaña recién creada ya habrá pasado.
    conn.call("Target.setDiscoverTargets", serde_json::json!({ "discover": true }),
              None, timeout)?;
    let marca = conn.event_mark();

    input::click(&conn, &session, &sel, "left", 1,
                 espera_de(args, 2, &t), force_de(args, 2), &t, timeout)?;

    let ms = espera_de(args, 2, &t);
    let ev = conn.wait_event("Target.targetCreated", None, marca, Duration::from_millis(ms))?
        .filter(|e| e.params.get("targetInfo")
            .and_then(|t| t.get("type")).and_then(|t| t.as_str()) == Some("page"))
        .ok_or_else(|| format!(
            "browser.click_opens '{sel}': el clic no abrió ninguna pestaña en {ms} ms"
        ))?;

    let target_id = ev.params["targetInfo"]["targetId"].as_str()
        .ok_or("browser.click_opens: la pestaña nueva no trae targetId")?
        .to_string();

    let adjunto = conn.call(
        "Target.attachToTarget",
        serde_json::json!({ "targetId": target_id, "flatten": true }),
        None, timeout,
    )?;
    let nueva_ses = adjunto["sessionId"].as_str()
        .ok_or("browser.click_opens: no se pudo adjuntar a la pestaña nueva")?
        .to_string();
    conn.call("Page.enable", serde_json::json!({}), Some(&nueva_ses), timeout)?;

    // Nos enganchamos en `targetCreated`, que ocurre antes de que la pestaña
    // tenga contenido. Sin esperar a que termine de cargar, el primer `title`
    // o `text` sobre ella devolvería vacío y parecería que la página está mal.
    wait_ready(&conn, &nueva_ses, ms, &t)?;

    let id = new_id();
    let mut reg = handles().lock().unwrap();
    reg.insert(id, Handle::Page(PageState {
        browser, target_id, session: nueva_ses, watch: None,
    }));
    if let Some(Handle::Browser(bs)) = reg.get_mut(&browser) {
        bs.pages.push(id);
    }
    Ok(EvalValue::Int(id as i64))
}

//    Lectura del DOM

fn do_wait(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.wait(pestaña, selector, ms?)")?;

    // `wait` con un diccionario espera una condición de la página en vez de un
    // selector. Va aquí y no en una función aparte porque es la misma idea
    // —parar hasta que algo esté listo— y separarlas obligaría a recordar dos
    // nombres para lo mismo.
    if let Some(EvalValue::Dict(m)) = args.get(1) {
        let (conn, session, _to, t) = page_ctx(p)?;
        if let Some(v) = m.get("idle") {
            let quieto = to_u64(v).unwrap_or(t.retry_ms.max(500));
            let tope = m.get("wait").and_then(|x| to_u64(x).ok()).unwrap_or(t.wait_ms);
            return do_wait_idle(&conn, &session, quieto, tope, &t);
        }
        return Err("browser.wait: el diccionario admite { idle: ms }".into());
    }

    let sel = args.get(1).map(to_str).ok_or("browser.wait: falta el selector")?;
    let (conn, session, _to, t) = page_ctx(p)?;
    let ms = espera_de(args, 2, &t);
    if !dom::wait_for(&conn, &session, &sel, ms, &t)? {
        return Err(format!("browser.wait: '{sel}' no apareció en {ms} ms"));
    }
    Ok(EvalValue::Bool(true))
}

/// Espera a que la red se calme.
///
/// Sirve para lo que ningún selector resuelve: no sabes **qué** va a aparecer,
/// solo que la página aún está trayendo cosas. Es el caso del panel que se monta
/// con tres llamadas encadenadas, o del listado que se recarga al filtrar.
///
/// La alternativa que usa todo el mundo es dormir dos segundos, y tiene los dos
/// defectos a la vez: si la red va lenta se lee a medias, y si va rápida se
/// tiran dos segundos por petición.
///
/// Se cuentan las peticiones en vuelo **dentro de la página**, interceptando
/// `fetch` y `XMLHttpRequest`, en vez de contar eventos CDP desde fuera. Así es
/// una sola llamada y no depende de que el historial de eventos —que está
/// acotado— haya conservado los que hacían falta.
fn do_wait_idle(
    conn: &Conn, session: &str, quieto_ms: u64, tope_ms: u64, t: &Tuning,
) -> Result<EvalValue, String> {
    let js = format!(r#"(() => {{
      // El contador se instala una sola vez y sobrevive a varias llamadas; en
      // una navegación la página nueva lo pierde y se vuelve a instalar sola.
      if (!window.__orionRed) {{
        const R = {{ vuelo: 0, ultima: Date.now() }};
        window.__orionRed = R;
        const marca = () => {{ R.ultima = Date.now(); }};
        const f = window.fetch;
        if (f) {{
          window.fetch = function (...a) {{
            R.vuelo++; marca();
            return f.apply(this, a).finally(() => {{ R.vuelo--; marca(); }});
          }};
        }}
        const abrir = XMLHttpRequest.prototype.open;
        XMLHttpRequest.prototype.open = function (...a) {{
          this.addEventListener('loadstart', () => {{ R.vuelo++; marca(); }});
          const fin = () => {{ R.vuelo--; marca(); }};
          this.addEventListener('loadend', fin);
          return abrir.apply(this, a);
        }};
      }}
      const R = window.__orionRed;
      return new Promise((resolve) => {{
        const limite = Date.now() + {tope_ms};
        const mira = () => {{
          if (R.vuelo <= 0 && Date.now() - R.ultima >= {quieto_ms}) return resolve(true);
          if (Date.now() >= limite) return resolve(false);
          setTimeout(mira, {retry});
        }};
        mira();
      }});
    }})()"#, retry = t.retry_ms);

    let v = evaluate_awaiting(
        conn, session, &js,
        Duration::from_millis(tope_ms + t.cdp_margin_ms), true,
    )?;
    if matches!(v, EvalValue::Bool(false)) {
        return Err(format!(
            "browser.wait: la red no se quedó {quieto_ms} ms sin actividad en {tope_ms} ms.\n  \
             Hay páginas que sondean el servidor para siempre; en esas, espera por un selector."
        ));
    }
    Ok(EvalValue::Bool(true))
}

/// Recarga, atrás y adelante.
///
/// El historial se lee del navegador en vez de contar pasos: `history.back()`
/// desde JavaScript no dice si había algo a lo que volver, y sin eso un `back`
/// al principio del historial se queda callado y el programa sigue creyendo que
/// cambió de página.
fn do_history(args: &[EvalValue], salto: i64) -> Result<EvalValue, String> {
    let quien = match salto { 0 => "reload", -1 => "back", _ => "forward" };
    let p = arg_handle(args, 0, &format!("browser.{quien}(pestaña)"))?;
    let (conn, session, timeout, t) = page_ctx(p)?;
    let ms = espera_de(args, 1, &t);

    // De dónde se viene, para saber cuándo se ha llegado.
    let anterior = match evaluate(&conn, &session, "location.href", timeout) {
        Ok(EvalValue::Str(s)) => s,
        _ => String::new(),
    };

    if salto == 0 {
        // `ignoreCache` deja recargar de verdad cuando hace falta; por defecto
        // se respeta la caché, que es lo que hace F5.
        let duro = match args.get(1) {
            Some(EvalValue::Dict(m)) => m.get("cache").map(|v| !truthy(v)).unwrap_or(false),
            _ => false,
        };
        conn.call("Page.reload", serde_json::json!({ "ignoreCache": duro }),
                  Some(&session), timeout)?;
    } else {
        let h = conn.call("Page.getNavigationHistory", serde_json::json!({}),
                          Some(&session), timeout)?;
        let actual = h.get("currentIndex").and_then(|x| x.as_i64()).unwrap_or(0);
        let entradas = h.get("entries").and_then(|x| x.as_array())
            .map(|a| a.len() as i64).unwrap_or(0);
        let destino = actual + salto;
        if destino < 0 || destino >= entradas {
            return Err(format!(
                "browser.{quien}: no hay {} en el historial de esta pestaña",
                if salto < 0 { "página anterior" } else { "página siguiente" }
            ));
        }
        let id = h["entries"][destino as usize]["id"].clone();
        conn.call("Page.navigateToHistoryEntry", serde_json::json!({ "entryId": id }),
                  Some(&session), timeout)?;
    }

    // Aquí NO se espera `Page.loadEventFired`, y es deliberado: al volver
    // atrás, Chrome suele restaurar la página desde su caché de retroceso sin
    // recargarla, y entonces **no hay evento de carga**. Esperarlo dejaba cada
    // `back` clavado el plazo entero —treinta segundos— para acabar
    // continuando igual. Se mira la página, que es quien sabe dónde está.
    let condicion = if salto == 0 {
        // Recargar lleva a la misma URL, así que el criterio es que el
        // documento vuelva a estar completo.
        "document.readyState === 'complete'".to_string()
    } else {
        format!(
            "location.href !== {} && document.readyState !== 'loading'",
            serde_json::Value::String(anterior.clone())
        )
    };
    let js = format!(r#"(() => new Promise((resolve) => {{
      const limite = Date.now() + {ms};
      const mira = () => {{
        if ({condicion}) return resolve(location.href);
        if (Date.now() >= limite) return resolve(null);
        setTimeout(mira, {retry});
      }};
      mira();
    }}))()"#, retry = t.retry_ms);

    let v = evaluate_awaiting(
        &conn, &session, &js, Duration::from_millis(ms + t.cdp_margin_ms), true,
    )?;
    match v {
        EvalValue::Str(s) => Ok(EvalValue::Str(s)),
        _ => Err(format!(
            "browser.{quien}: la página no terminó de cambiar en {ms} ms"
        )),
    }
}

/// ¿Esta lectura debe esperar a que haya contenido?
#[derive(Clone, Copy, PartialEq)]
enum Espera { Si, No }

/// Ejecuta una consulta sobre el DOM con el selector ya inyectado.
///
/// Toda la lectura pasa por aquí y por tanto por **una** evaluación: es lo que
/// evita el patrón de Selenium de una petición HTTP por cada atributo leído.
fn query(args: &[EvalValue], quien: &str, espera: Espera, cuerpo: &str) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, &format!("{quien}(pestaña, selector)"))?;
    let sel = args.get(1).map(to_str)
        .ok_or_else(|| format!("{quien}(pestaña, selector): falta el selector"))?;
    let (conn, session, timeout, t) = page_ctx(p)?;

    let (js, limite) = match espera {
        Espera::No => (dom::expr(&sel, cuerpo, &t), timeout),
        Espera::Si => {
            let ms = espera_de(args, 2, &t);
            (dom::expr_waiting(&sel, cuerpo, ms, &t), Duration::from_millis(ms + t.cdp_margin_ms))
        }
    };
    evaluate_awaiting(&conn, &session, &js, limite, espera == Espera::Si)
}

fn do_attr(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.attr(pestaña, selector, atributo)")?;
    let sel = args.get(1).map(to_str).ok_or("browser.attr: falta el selector")?;
    let at  = args.get(2).map(to_str).ok_or("browser.attr: falta el nombre del atributo")?;
    let (conn, session, timeout, t) = page_ctx(p)?;

    // El nombre del atributo también se serializa: puede traer comillas.
    let a = serde_json::Value::String(at).to_string();
    let cuerpo = format!("const e = __find(sel); return e ? e.getAttribute({a}) : null;");

    let ms = espera_de(args, 3, &t);
    let _ = timeout;
    evaluate_awaiting(
        &conn, &session,
        &dom::expr_waiting(&sel, &cuerpo, ms, &t),
        Duration::from_millis(ms + t.cdp_margin_ms),
        true,
    )
}

//    Extracción declarativa

/// `extract(pestaña, selector_de_fila, esquema, opts?)` → lista de dicts.
///
/// El esquema entero se compila a una sola evaluación dentro de la página. Ver
/// `extract.rs` para la gramática de las especificaciones.
fn do_extract(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.extract(pestaña, selector, esquema)")?;
    let fila_sel = args.get(1).map(to_str)
        .ok_or("browser.extract(pestaña, selector, esquema): falta el selector de fila")?;

    let Some(EvalValue::Dict(esquema)) = args.get(2) else {
        return Err(concat!(
            "browser.extract: el esquema debe ser un diccionario campo → especificación.
",
            "  Ejemplo: { nombre: \".title\", precio: \".price|num\" }"
        ).to_string());
    };
    if esquema.is_empty() {
        return Err("browser.extract: el esquema está vacío".into());
    }

    let campos: Vec<extract::Campo> = esquema.iter()
        .map(|(k, v)| extract::parse_campo(k, &to_str(v)))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("browser.extract: {e}"))?;

    let (conn, session, _to, t) = page_ctx(p)?;
    let ms = espera_de(args, 3, &t);
    let r = extract::extract(&conn, &session, &fila_sel, &campos, ms, &t)?;

    // Un campo vacío en TODAS las filas es casi siempre un selector equivocado,
    // no un dato ausente. Callarlo devolvería una lista de nulls que parece
    // buena y revienta mucho más adelante, que es el fallo clásico de
    // BeautifulSoup. Con `strict: no` se acepta y se sigue.
    let estricto = match args.get(3) {
        Some(EvalValue::Dict(m)) => m.get("strict").map(truthy).unwrap_or(true),
        _ => true,
    };
    if estricto && !r.muertos.is_empty() {
        let detalle: Vec<String> = r.muertos.iter()
            .map(|(campo, spec)| format!("    {campo}  ←  {spec}"))
            .collect();
        return Err(format!(
            "browser.extract: {} campo(s) no encontraron nada en ninguna de las {} filas:
{}
  Revisa esos selectores, o usa {{ strict: no }} si de verdad pueden faltar.",
            r.muertos.len(), r.filas, detalle.join("
")
        ));
    }

    Ok(json_to_eval(r.json))
}

/// `discover(pestaña, opts?)` → esquema propuesto.
///
/// Analiza la página y propone el selector de fila y un selector por campo, más
/// una muestra ya extraída. No sustituye a `extract`: te lleva hasta su puerta.
fn do_discover(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.discover(pestaña, opts?)")?;
    let (conn, session, _to, t) = page_ctx(p)?;
    let ms = espera_de(args, 1, &t);

    // Cuántos elementos hermanos hacen falta para considerarlo un "listado".
    // Tres es lo mínimo para hablar de repetición; algunos sitios lo necesitan
    // más alto para no enganchar una fila de tres iconos de cabecera.
    let min = match args.get(1) {
        Some(EvalValue::Dict(m)) => m.get("min").and_then(|v| to_u64(v).ok()).unwrap_or(3).max(2),
        _ => 3,
    };
    let js = discover::DISCOVER_JS.replace("__MIN__", &min.to_string());

    // Se espera por si el listado llega tras una acción, igual que `extract`.
    let cuerpo = format!(r#"
    return new Promise((resolve) => {{
      const limite = Date.now() + {ms};
      const intenta = () => {{
        const r = {js};
        if (!r.error || Date.now() >= limite) return resolve(r);
        setTimeout(intenta, {retry});
      }};
      intenta();
    }});
    "#, retry = t.retry_ms);

    let expr = format!("(() => {{ {cuerpo} }})()");
    let r = conn.call(
        "Runtime.evaluate",
        serde_json::json!({
            "expression": expr,
            "returnByValue": true,
            "awaitPromise": true,
        }),
        Some(&session), Duration::from_millis(ms + t.cdp_margin_ms),
    )?;

    if let Some(ex) = r.get("exceptionDetails") {
        let msg = ex.get("exception").and_then(|e| e.get("description")).and_then(|d| d.as_str())
            .or_else(|| ex.get("text").and_then(|x| x.as_str()))
            .unwrap_or("error de JavaScript");
        return Err(format!("browser.discover: {msg}"));
    }

    let v = r.get("result").and_then(|x| x.get("value")).cloned().unwrap_or(serde_json::Value::Null);
    if let Some(e) = v.get("error").and_then(|x| x.as_str()) {
        return Err(format!("browser.discover: {e}"));
    }
    Ok(json_to_eval(v))
}

/// `crawl(navegador, opts)` → resumen del recorrido paralelo.
///
/// Toma el **navegador**, no una pestaña: abre N por su cuenta y las conduce en
/// paralelo. Es la versión con músculo de `extract_to` — hilos de sistema de
/// verdad, streaming a disco y reanudación.
fn do_crawl(args: &[EvalValue]) -> Result<EvalValue, String> {
    let nav = arg_handle(args, 0, "browser.crawl(navegador, opts)")?;
    let Some(EvalValue::Dict(o)) = args.get(1) else {
        return Err("browser.crawl(navegador, opts): las opciones son un diccionario \
                    con al menos { urls, row, schema, out }".into());
    };

    let urls: Vec<String> = match o.get("urls") {
        Some(EvalValue::List(l)) => l.iter().map(to_str).collect(),
        Some(v) => vec![to_str(v)],
        None => return Err("browser.crawl: falta `urls` (la lista de páginas a recorrer)".into()),
    };
    if urls.is_empty() {
        return Err("browser.crawl: la lista `urls` está vacía".into());
    }
    let fila_sel = o.get("row").map(to_str)
        .ok_or("browser.crawl: falta `row` (el selector de la fila que se repite)")?;
    let Some(EvalValue::Dict(esquema)) = o.get("schema") else {
        return Err("browser.crawl: falta `schema` (el diccionario campo → especificación)".into());
    };
    let salida = o.get("out").map(to_str)
        .ok_or("browser.crawl: falta `out` (la ruta de salida, .csv o .odf)")?;

    let campos: Vec<extract::Campo> = esquema.iter()
        .map(|(k, v)| extract::parse_campo(k, &to_str(v)))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("browser.crawl: {e}"))?;

    let (conn, timeout, t) = browser_ctx(nav)?;

    // Reanudar sobre `.odf` no está soportado (numera bloques y lleva el conteo
    // en la cabecera); se avisa antes de arrancar el navegador en vez de fallar a
    // la mitad de un recorrido largo.
    let resume = o.get("resume").map(truthy).unwrap_or(false);
    if resume && !salida.to_lowercase().ends_with(".csv") {
        return Err("browser.crawl: `resume` solo funciona con salida .csv; \
                    el .odf obliga a empezar de cero".into());
    }

    let opts = crawl::Opciones {
        urls,
        fila_sel,
        campos,
        salida: salida.clone(),
        workers: o.get("workers").and_then(|v| to_u64(v).ok()).unwrap_or(4).max(1) as usize,
        chunk:   o.get("chunk").and_then(|v| to_u64(v).ok()).unwrap_or(50_000) as usize,
        wait_ms: o.get("wait").and_then(|v| to_u64(v).ok()).unwrap_or(t.wait_ms),
        resume,
    };

    let r = crawl::crawl(conn, timeout, t, opts)?;

    // Selectores muertos: como en `extract`, callarlos devolvería columnas
    // vacías que parecen buenas. Con `{ strict: no }` se acepta.
    let estricto = o.get("strict").map(truthy).unwrap_or(true);
    if estricto && !r.muertos.is_empty() {
        return Err(format!(
            "browser.crawl: {} campo(s) no trajeron valor en ninguna página:\n    {}\n  \
             Los datos ya se escribieron en {salida}. Revisa esos selectores, o usa {{ strict: no }}.",
            r.muertos.len(), r.muertos.join("\n    ")
        ));
    }

    let mut m: IndexMap<String, EvalValue> = IndexMap::new();
    m.insert("rows".into(),     EvalValue::Int(r.filas as i64));
    m.insert("ok".into(),       EvalValue::Int(r.ok as i64));
    m.insert("failed".into(),   EvalValue::Int(r.errores.len() as i64));
    m.insert("skipped".into(),  EvalValue::Int(r.saltadas as i64));
    m.insert("workers".into(),  EvalValue::Int(r.workers as i64));
    m.insert("empty".into(),    EvalValue::List(r.vacias.into_iter().map(EvalValue::Str).collect()));
    m.insert("files".into(),    EvalValue::List(r.archivos.into_iter().map(EvalValue::Str).collect()));
    m.insert("errors".into(),   EvalValue::List(r.errores.into_iter().map(EvalValue::Str).collect()));
    Ok(EvalValue::Dict(m))
}

/// `extract_to(pestaña, urls, selector, esquema, salida, opts?)` → resumen.
///
/// Recorre las URLs **reutilizando una sola pestaña** y vuelca lo extraído a
/// disco según se obtiene. Las dos cosas son deliberadas: abrir una pestaña por
/// URL multiplica la memoria del navegador, y acumular el listado entero antes
/// de guardar es lo que hace que un scraper de Python se coma la RAM en cuanto
/// el volumen crece.
fn do_extract_to(args: &[EvalValue]) -> Result<EvalValue, String> {
    const USO: &str = "browser.extract_to(pestaña, urls, selector, esquema, salida)";
    let p = arg_handle(args, 0, USO)?;

    // Una sola URL también vale: obligar a envolverla en una lista sería
    // ceremonia por nada.
    let urls: Vec<String> = match args.get(1) {
        Some(EvalValue::List(l)) => l.iter().map(to_str).collect(),
        Some(v) => vec![to_str(v)],
        None => return Err(format!("{USO}: faltan las urls")),
    };
    if urls.is_empty() { return Err("browser.extract_to: la lista de urls está vacía".into()); }

    let fila_sel = args.get(2).map(to_str).ok_or(format!("{USO}: falta el selector de fila"))?;
    let Some(EvalValue::Dict(esquema)) = args.get(3) else {
        return Err(format!("{USO}: el esquema debe ser un diccionario campo → especificación"));
    };
    let salida = args.get(4).map(to_str).ok_or(format!("{USO}: falta la ruta de salida"))?;

    let campos: Vec<extract::Campo> = esquema.iter()
        .map(|(k, v)| extract::parse_campo(k, &to_str(v)))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("browser.extract_to: {e}"))?;

    let (conn, session, timeout, t) = page_ctx(p)?;
    let ms = espera_de(args, 5, &t);
    let chunk = match args.get(5) {
        Some(EvalValue::Dict(m)) => m.get("chunk").and_then(|v| to_u64(v).ok()).unwrap_or(50_000),
        _ => 50_000,
    } as usize;

    let headers: Vec<String> = campos.iter().map(|c| c.nombre.clone()).collect();
    let mut volcador = extract::Volcador::nuevo(&salida, headers.clone(), chunk)
        .map_err(|e| format!("browser.extract_to: {e}"))?;

    let mut errores: Vec<EvalValue> = Vec::new();
    let mut vacias: Vec<EvalValue> = Vec::new();
    let mut ok = 0usize;
    let mut muertos_globales: Vec<(String, String)> = Vec::new();

    for url in &urls {
        // Un fallo en una URL no aborta el recorrido: en una tanda de veinte,
        // morir por un 404 tira el trabajo de las diecinueve buenas. Se anota y
        // se sigue, y el resumen dice exactamente qué pasó.
        if let Err(e) = navegar(&conn, &session, url, timeout) {
            errores.push(EvalValue::Str(format!("{url}: {e}")));
            continue;
        }
        match extract::extract(&conn, &session, &fila_sel, &campos, ms, &t) {
            Ok(r) => {
                if r.filas > 0 && !r.muertos.is_empty() && muertos_globales.is_empty() {
                    muertos_globales = r.muertos.clone();
                }
                // Una página que carga pero no da filas no es un error de red y
                // por eso pasa desapercibida: un 404 con plantilla, un redirect
                // al login, o el selector que dejó de valer en esa sección. Se
                // anota aparte para que un recorrido no pierda páginas en
                // silencio, que es el fallo que nadie detecta hasta que faltan
                // datos en el informe.
                if r.filas == 0 { vacias.push(EvalValue::Str(url.clone())); }
                if let Some(arr) = r.json.as_array() {
                    for reg in arr {
                        let fila: Vec<String> = headers.iter()
                            .map(|h| extract::a_texto(reg.get(h).unwrap_or(&serde_json::Value::Null)))
                            .collect();
                        volcador.escribir(fila).map_err(|e| format!("browser.extract_to: {e}"))?;
                    }
                }
                ok += 1;
            }
            Err(e) => errores.push(EvalValue::Str(format!("{url}: {e}"))),
        }
    }

    let (filas, archivos) = volcador.cerrar().map_err(|e| format!("browser.extract_to: {e}"))?;

    let estricto = match args.get(5) {
        Some(EvalValue::Dict(m)) => m.get("strict").map(truthy).unwrap_or(true),
        _ => true,
    };
    if estricto && !muertos_globales.is_empty() {
        let detalle: Vec<String> = muertos_globales.iter()
            .map(|(campo, spec)| format!("    {campo}  ←  {spec}"))
            .collect();
        return Err(format!(
            "browser.extract_to: {} campo(s) no encontraron nada en ninguna fila:
{}
  Los datos ya se escribieron en {salida}. Revisa esos selectores, o usa {{ strict: no }}.",
            muertos_globales.len(), detalle.join("
")
        ));
    }

    let mut m: IndexMap<String, EvalValue> = IndexMap::new();
    m.insert("rows".into(),   EvalValue::Int(filas as i64));
    m.insert("urls".into(),   EvalValue::Int(urls.len() as i64));
    m.insert("ok".into(),     EvalValue::Int(ok as i64));
    m.insert("failed".into(), EvalValue::Int(errores.len() as i64));
    m.insert("empty".into(),  EvalValue::List(vacias));
    m.insert("files".into(),  EvalValue::List(
        archivos.into_iter().map(EvalValue::Str).collect()));
    m.insert("errors".into(), EvalValue::List(errores));
    Ok(EvalValue::Dict(m))
}

/// Navega y espera la carga. Extraído de `goto` para poder reutilizarlo en el
/// recorrido sin pasar por la conversión de argumentos de Orion.
fn navegar(conn: &Conn, session: &str, url: &str, timeout: Duration) -> Result<(), String> {
    let marca = conn.event_mark();
    let r = conn.call("Page.navigate", serde_json::json!({ "url": url }), Some(session), timeout)?;
    if let Some(err) = r.get("errorText").and_then(|e| e.as_str()) {
        return Err(err.to_string());
    }
    let _ = conn.wait_event("Page.loadEventFired", Some(session), marca, timeout)?;
    Ok(())
}

//    Captura de red

/// Arma la escucha: a partir de aquí se anota lo que la página pida y case.
///
/// Va antes de provocar la petición, no después. Si se encendiera al recoger,
/// la llamada ya habría pasado y no quedaría nada que leer — es el mismo
/// motivo por el que `click_opens` toma la marca de eventos antes de pulsar.
fn do_watch(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.watch(pestaña, patrón)")?;
    let patron = args.get(1).map(to_str).unwrap_or_default();
    let (conn, session, timeout, t) = page_ctx(p)?;

    // El dominio de red solo se enciende si alguien va a escuchar: con él
    // puesto el navegador emite varios eventos por cada petición, y una página
    // con cien recursos son cientos de mensajes por el socket que nadie
    // consumiría.
    //
    // Los tamaños de buffer no están fijados en el código: el cuerpo de una
    // respuesta se lee del búfer del navegador, así que un listado grande
    // necesita más sitio y quien ejecuta es el que sabe cuánto.
    conn.call(
        "Network.enable",
        serde_json::json!({
            "maxResourceBufferSize": t.body_buffer,
            "maxTotalBufferSize":    t.total_buffer,
        }),
        Some(&session), timeout,
    )?;

    let marca = conn.event_mark();
    let mut reg = handles().lock().unwrap();
    if let Some(Handle::Page(ps)) = reg.get_mut(&p) {
        ps.watch = Some((patron, marca));
    }
    Ok(EvalValue::Bool(true))
}

/// Recoge lo que la página pidió desde que se armó la escucha.
///
/// Devuelve una lista de `{url, status, method, json}` — el JSON **ya
/// parseado**, que es el sentido de todo esto: los datos llegan tipados y con
/// los campos que el sitio no llega a pintar. Si el cuerpo no era JSON, `json`
/// viene nulo y el texto crudo va en `text`.
fn do_capture(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.capture(pestaña, opts?)")?;
    let (conn, session, timeout, t) = page_ctx(p)?;
    let ms = espera_de(args, 1, &t);

    let (patron, marca) = {
        let reg = handles().lock().unwrap();
        match reg.get(&p) {
            Some(Handle::Page(ps)) => ps.watch.clone().ok_or(
                "browser.capture: no hay ninguna escucha armada.\n  \
                 Llama antes a browser.watch(pestaña, patrón), y después provoca la petición."
            )?,
            _ => return Err(format!("browser.capture: la pestaña {p} no existe")),
        }
    };

    // Se espera a que llegue algo que case, en vez de mirar una vez y volver
    // vacío: la petición sale después de la acción que la provoca, y devolver
    // una lista vacía convertiría un problema de tiempo en "este sitio no usa
    // API", que es una conclusión falsa y difícil de deshacer.
    let pat = patron.clone();
    let _ = conn.wait_event_where(
        "Network.responseReceived", Some(&session), marca,
        Duration::from_millis(ms),
        move |e| e.params.get("response").and_then(|r| r.get("url"))
                  .and_then(|u| u.as_str())
                  .map(|u| capture::casa(u, &pat))
                  .unwrap_or(false),
    )?;

    // Un margen para que terminen de llegar las que salieron a la vez: un
    // panel suele pedir tres o cuatro cosas de golpe, y quedarse con la
    // primera daría un resultado incompleto que parece completo.
    let respuestas = conn.events_where("Network.responseReceived", Some(&session), marca, |e| {
        e.params.get("response").and_then(|r| r.get("url")).and_then(|u| u.as_str())
            .map(|u| capture::casa(u, &patron)).unwrap_or(false)
    });

    let mut salida = Vec::new();
    for ev in respuestas {
        let resp = ev.params.get("response").cloned().unwrap_or(serde_json::Value::Null);
        let url = resp.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
        let status = resp.get("status").and_then(|s| s.as_i64()).unwrap_or(0);
        let req_id = ev.params.get("requestId").and_then(|r| r.as_str()).unwrap_or("").to_string();

        let mut m: IndexMap<String, EvalValue> = IndexMap::new();
        m.insert("url".into(), EvalValue::Str(url));
        m.insert("status".into(), EvalValue::Int(status));

        // El cuerpo se pide aparte porque no viaja en el evento. Puede haber
        // desaparecido ya del búfer del navegador —una respuesta enorme, o
        // muchas peticiones después—: eso no es un error del programa, así que
        // se anota y se sigue en vez de tirar la captura entera.
        match conn.call(
            "Network.getResponseBody",
            serde_json::json!({ "requestId": req_id }),
            Some(&session), timeout,
        ) {
            Ok(b) => {
                let cuerpo = b.get("body").and_then(|x| x.as_str()).unwrap_or("");
                match serde_json::from_str::<serde_json::Value>(cuerpo) {
                    Ok(v) => { m.insert("json".into(), json_to_eval(v)); }
                    Err(_) => {
                        m.insert("json".into(), EvalValue::Null);
                        m.insert("text".into(), EvalValue::Str(cuerpo.to_string()));
                    }
                }
            }
            Err(e) => {
                m.insert("json".into(), EvalValue::Null);
                m.insert("error".into(), EvalValue::Str(format!(
                    "el navegador ya no tenía el cuerpo ({e}). Sube el búfer con \
                     open({{ tuning: {{ body_buffer: bytes }} }}) o captura antes."
                )));
            }
        }
        salida.push(EvalValue::Dict(m));
    }

    Ok(EvalValue::List(salida))
}

//    Sesión

fn do_save_state(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.save_state(pestaña, ruta)")?;
    let ruta = args.get(1).map(to_str)
        .ok_or("browser.save_state(pestaña, ruta): falta la ruta")?;
    let (conn, session, timeout, _t) = page_ctx(p)?;

    let g = state::save(&conn, &session, &ruta, timeout)?;
    let mut r: IndexMap<String, EvalValue> = IndexMap::new();
    r.insert("path".into(),    EvalValue::Str(ruta));
    r.insert("cookies".into(), EvalValue::Int(g.cookies as i64));
    r.insert("local".into(),   EvalValue::Int(g.local as i64));
    r.insert("session".into(), EvalValue::Int(g.session as i64));
    r.insert("origin".into(),  EvalValue::Str(g.origin));
    Ok(EvalValue::Dict(r))
}

fn do_load_state(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.load_state(pestaña, ruta)")?;
    let ruta = args.get(1).map(to_str)
        .ok_or("browser.load_state(pestaña, ruta): falta la ruta")?;
    let (conn, session, timeout, _t) = page_ctx(p)?;

    let c = state::load(&conn, &session, &ruta, timeout)?;
    let mut r: IndexMap<String, EvalValue> = IndexMap::new();
    r.insert("cookies".into(), EvalValue::Int(c.cookies as i64));
    r.insert("local".into(),   EvalValue::Int(c.local as i64));
    r.insert("session".into(), EvalValue::Int(c.session as i64));
    // Se informa de los orígenes que NO se aplicaron en vez de callarlos: el
    // navegador no deja escribir el almacenamiento de otro dominio, así que
    // cargar el estado estando en la página equivocada restaura las cookies y
    // deja fuera lo demás — y el síntoma sería una sesión a medias sin motivo
    // aparente.
    r.insert("skipped".into(), EvalValue::List(
        c.omitidos.into_iter().map(EvalValue::Str).collect()
    ));
    Ok(EvalValue::Dict(r))
}

fn do_blocked(args: &[EvalValue]) -> Result<EvalValue, String> {
    let h = arg_handle(args, 0, "browser.blocked(navegador)")?;
    // Vale con el handle del navegador o el de una pestaña: quien depura no
    // tiene por qué acordarse de cuál de los dos tenía a mano.
    let conn = match browser_ctx(h) {
        Ok((c, _, _)) => c,
        Err(_) => page_ctx(h)?.0,
    };
    Ok(EvalValue::List(conn.bloqueadas().into_iter().map(EvalValue::Str).collect()))
}

//    Archivos

/// Rutas de archivo de un argumento: admite una sola o una lista.
///
/// Adjuntar un archivo es lo normal y adjuntar varios la excepción, así que
/// obligar a escribir `["f.pdf"]` en el caso común sería cobrar a todos el
/// precio de unos pocos.
fn rutas_de(v: Option<&EvalValue>) -> Vec<String> {
    match v {
        Some(EvalValue::List(l)) => l.iter().map(to_str).collect(),
        Some(x) => vec![to_str(x)],
        None => vec![],
    }
}

fn do_upload(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.upload(pestaña, selector, archivos)")?;
    let sel = args.get(1).map(to_str)
        .ok_or("browser.upload(pestaña, selector, archivos): falta el selector")?;
    let rutas = rutas_de(args.get(2));
    if rutas.is_empty() {
        return Err("browser.upload(pestaña, selector, archivos): falta el archivo".into());
    }
    let (conn, session, timeout, t) = page_ctx(p)?;
    let puestos = files::upload(&conn, &session, &sel, &rutas,
                                espera_de(args, 3, &t), &t, timeout)?;
    // Se devuelven las rutas absolutas de verdad: la conversión de relativa a
    // absoluta ocurre por dentro, y sin verla es imposible entender por qué el
    // navegador dice que un archivo que existe no existe.
    Ok(EvalValue::List(puestos.into_iter().map(EvalValue::Str).collect()))
}

fn do_download(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.download(pestaña, selector, opts?)")?;
    let sel = args.get(1).map(to_str)
        .ok_or("browser.download(pestaña, selector, opts?): falta el selector")?;
    let (conn, session, timeout, t) = page_ctx(p)?;

    let m = match args.get(2) { Some(EvalValue::Dict(d)) => Some(d), _ => None };
    let o = files::DescargaOpts {
        dir:       m.and_then(|d| d.get("dir")).map(to_str),
        name:      m.and_then(|d| d.get("name")).map(to_str),
        overwrite: m.and_then(|d| d.get("overwrite")).map(truthy).unwrap_or(false),
        wait_ms:   espera_de(args, 2, &t),
    };

    let d = files::download(&conn, &session, &sel, &o, espera_de(args, 2, &t), &t, timeout)?;
    let mut r: IndexMap<String, EvalValue> = IndexMap::new();
    r.insert("path".into(),  EvalValue::Str(d.path));
    r.insert("name".into(),  EvalValue::Str(d.name));
    r.insert("bytes".into(), EvalValue::Int(d.bytes as i64));
    r.insert("url".into(),   EvalValue::Str(d.url));
    Ok(EvalValue::Dict(r))
}

fn do_pdf(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.pdf(pestaña, ruta, opts?)")?;
    let ruta = args.get(1).map(to_str)
        .ok_or("browser.pdf(pestaña, ruta, opts?): falta la ruta")?;
    let (conn, session, timeout, _t) = page_ctx(p)?;

    // Nada fijado: lo que no se indique lo decide el navegador con su propio
    // default, que es el mismo que aplica el diálogo de impresión.
    let mut o = serde_json::Map::new();
    if let Some(EvalValue::Dict(m)) = args.get(2) {
        if let Some(v) = m.get("landscape")  { o.insert("landscape".into(), truthy(v).into()); }
        if let Some(v) = m.get("background") { o.insert("printBackground".into(), truthy(v).into()); }
        if let Some(v) = m.get("headers")    { o.insert("displayHeaderFooter".into(), truthy(v).into()); }
        if let Some(x) = m.get("scale").and_then(to_f64)  { o.insert("scale".into(), x.into()); }
        // En pulgadas, que es la unidad de CDP; A4 son 8,27 × 11,69.
        if let Some(x) = m.get("width").and_then(to_f64)  { o.insert("paperWidth".into(), x.into()); }
        if let Some(x) = m.get("height").and_then(to_f64) { o.insert("paperHeight".into(), x.into()); }
        if let Some(x) = m.get("margin").and_then(to_f64) {
            for lado in ["marginTop", "marginBottom", "marginLeft", "marginRight"] {
                o.insert(lado.into(), x.into());
            }
        }
        if let Some(v) = m.get("pages") { o.insert("pageRanges".into(), to_str(v).into()); }
    }
    // Sin esto, un fondo de color o una tabla con filas alternas salen en
    // blanco: el navegador los quita al imprimir para ahorrar tinta, que en un
    // PDF que nadie va a imprimir es justo lo contrario de lo que se quiere.
    o.entry("printBackground").or_insert(true.into());

    let escrito = files::pdf(&conn, &session, &ruta, serde_json::Value::Object(o), timeout)?;
    Ok(EvalValue::Str(escrito))
}

//    Captura

fn do_screenshot(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.screenshot(pestaña, ruta)")?;
    let ruta = args.get(1).map(to_str)
        .ok_or("browser.screenshot(pestaña, ruta): falta la ruta")?;
    let (conn, session, timeout, _t) = page_ctx(p)?;

    let r = conn.call(
        "Page.captureScreenshot",
        serde_json::json!({ "format": "png", "captureBeyondViewport": false }),
        Some(&session), timeout,
    )?;
    let b64 = r.get("data").and_then(|d| d.as_str())
        .ok_or("browser.screenshot: el navegador no devolvió imagen")?;

    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let bytes = B64.decode(b64)
        .map_err(|e| format!("browser.screenshot: imagen ilegible ({e})"))?;
    std::fs::write(&ruta, &bytes)
        .map_err(|e| format!("browser.screenshot: no se pudo escribir '{ruta}': {e}"))?;

    Ok(EvalValue::Str(ruta))
}

//    pages(browser) → lista de handles

fn pages(args: &[EvalValue]) -> Result<EvalValue, String> {
    let b = arg_handle(args, 0, "browser.pages(navegador)")?;
    let reg = handles().lock().unwrap();
    match reg.get(&b) {
        Some(Handle::Browser(bs)) => Ok(EvalValue::List(
            bs.pages.iter().map(|p| EvalValue::Int(*p as i64)).collect()
        )),
        _ => Err(format!("browser.pages: el navegador {b} no existe")),
    }
}

//    free(handle) — cierra pestaña o navegador

fn free(args: &[EvalValue]) -> Result<EvalValue, String> {
    let h = match args.first() {
        Some(v) => match to_u64(v) { Ok(h) => h, Err(_) => return Ok(EvalValue::Bool(false)) },
        None => return Ok(EvalValue::Bool(false)),
    };

    let sacado = handles().lock().unwrap().remove(&h);
    match sacado {
        None => Ok(EvalValue::Bool(false)),

        Some(Handle::Page(p)) => {
            // Cerrar la pestaña es lo que libera la memoria del renderer, que
            // es la parte cara: dejarla abierta y olvidada es RAM que nadie
            // reclama hasta que muere el navegador.
            let mut reg = handles().lock().unwrap();
            if let Some(Handle::Browser(b)) = reg.get_mut(&p.browser) {
                b.pages.retain(|x| *x != h);
                let conn = Arc::clone(&b.conn);
                let timeout = b.timeout;
                drop(reg);
                let _ = conn.call(
                    "Target.closeTarget",
                    serde_json::json!({ "targetId": p.target_id }),
                    None, timeout,
                );
            }
            Ok(EvalValue::Bool(true))
        }

        Some(Handle::Browser(mut b)) => {
            // Las pestañas se van con él. Se guardan sus targetId antes de
            // tirarlas del registro porque un navegador ajeno (`attach`) NO va a
            // morir: sin esto, cada ejecución le dejaba una pestaña huérfana.
            let targets: Vec<String> = {
                let mut reg = handles().lock().unwrap();
                b.pages.iter()
                    .filter_map(|p| match reg.remove(p) {
                        Some(Handle::Page(ps)) => Some(ps.target_id),
                        _ => None,
                    })
                    .collect()
            };
            match b.proc.as_mut() {
                // Navegador nuestro: se cierra y, si hace falta, se remata.
                Some(proc) => {
                    let _ = b.conn.call("Browser.close", serde_json::json!({}), None,
                                        Duration::from_millis(b.tuning.close_ms));
                    b.conn.close();
                    // `Browser.close` es una petición amable; si no la atendió,
                    // se termina el proceso a mano para no dejarlo huérfano.
                    let _ = proc.kill();
                    let _ = proc.wait();
                    if b.temporal {
                        remove_profile(&b.user_data, b.tuning.cleanup_tries);
                    }
                }
                // Ajeno: se recoge lo que abrimos y se suelta el socket. Ni
                // `Browser.close` ni `kill` ni tocar su perfil.
                None => {
                    for t in targets {
                        let _ = b.conn.call(
                            "Target.closeTarget",
                            serde_json::json!({ "targetId": t }),
                            None,
                            Duration::from_millis(b.tuning.close_ms),
                        );
                    }
                    b.conn.close();
                }
            }
            Ok(EvalValue::Bool(true))
        }
    }
}

/// Borra el perfil temporal, reintentando.
///
/// `wait()` solo espera al proceso principal, pero Chrome deja hijos (renderer,
/// GPU, red) que tardan un instante en soltar los archivos del perfil. En
/// Windows eso hace fallar el borrado inmediato, y un intento único dejaba
/// varios MB por sesión tirados en el temporal.
fn remove_profile(dir: &std::path::Path, intentos: u32) {
    for intento in 0..intentos.max(1) as u64 {
        if !dir.exists() || std::fs::remove_dir_all(dir).is_ok() { return; }
        std::thread::sleep(Duration::from_millis(50 + intento * 25));
    }
    // Si tras un segundo largo sigue bloqueado, no se insiste: perder el
    // directorio temporal es mucho menos grave que colgar el programa del
    // usuario al cerrar un navegador.
}

//    info() — diagnóstico

/// Qué navegador se usaría y de dónde sale. Sin esto, "no me funciona" es
/// indepurable: no se sabe si falta el navegador, si se eligió otro, o si el
/// problema está en la página.
fn info() -> Result<EvalValue, String> {
    let mut m: IndexMap<String, EvalValue> = IndexMap::new();
    match launch::resolve_browser(None) {
        Ok(p) => {
            m.insert("found".into(), EvalValue::Bool(true));
            m.insert("path".into(), EvalValue::Str(p.display().to_string()));
        }
        Err(e) => {
            m.insert("found".into(), EvalValue::Bool(false));
            m.insert("error".into(), EvalValue::Str(e));
        }
    }
    m.insert("env".into(), EvalValue::Str(
        std::env::var("ORION_CHROME").unwrap_or_else(|_| String::new())
    ));
    let abiertos = handles().lock().unwrap();
    let en_uso: Vec<EvalValue> = abiertos.values()
        .filter_map(|h| match h {
            // Se informa del ejecutable REAL de cada navegador abierto, no del
            // que se resolvería ahora: pueden no coincidir si el programa pasó
            // una ruta distinta en `open`, y ahí es donde está el malentendido.
            Handle::Browser(b) => Some(EvalValue::Str(b.exe.clone())),
            Handle::Page(_) => None,
        })
        .collect();
    m.insert("open_browsers".into(), EvalValue::Int(en_uso.len() as i64));
    m.insert("in_use".into(), EvalValue::List(en_uso));
    m.insert("open_pages".into(), EvalValue::Int(
        abiertos.values().filter(|h| matches!(h, Handle::Page(_))).count() as i64
    ));
    Ok(EvalValue::Dict(m))
}

//    Conversión de argumentos

fn parse_opts(v: Option<&EvalValue>) -> Result<LaunchOpts, String> {
    let mut o = LaunchOpts::default();
    let Some(EvalValue::Dict(m)) = v else { return Ok(o) };

    if let Some(x) = m.get("chrome").or_else(|| m.get("browser")) {
        o.chrome = Some(to_str(x));
    }
    if let Some(x) = m.get("user_data") { o.user_data = Some(to_str(x)); }
    if let Some(x) = m.get("headless")  { o.headless = truthy(x); }
    if let Some(x) = m.get("images")    { o.images   = truthy(x); }
    if let Some(x) = m.get("gpu")       { o.gpu      = truthy(x); }
    if let Some(x) = m.get("width")     { o.width    = to_u64(x).unwrap_or(1280) as u32; }
    if let Some(x) = m.get("height")    { o.height   = to_u64(x).unwrap_or(800) as u32; }
    if let Some(x) = m.get("timeout") {
        let ms = to_u64(x).unwrap_or(30_000);
        o.timeout = Duration::from_millis(ms.max(1_000));
    }
    if let Some(EvalValue::List(xs)) = m.get("args") {
        o.extra = xs.iter().map(to_str).collect();
    }
    Ok(o)
}

fn arg_handle(args: &[EvalValue], i: usize, uso: &str) -> Result<u64, String> {
    let v = args.get(i).ok_or_else(|| format!("{uso}: falta el handle"))?;
    to_u64(v).map_err(|_| format!("{uso}: esperaba un handle, recibió {}", v.type_name()))
}

fn to_u64(v: &EvalValue) -> Result<u64, String> {
    match v {
        EvalValue::Int(n) if *n > 0 => Ok(*n as u64),
        other => Err(format!("esperaba un handle positivo, recibió {}", other.type_name())),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{other}") }
}

fn to_f64(v: &EvalValue) -> Option<f64> {
    match v {
        EvalValue::Int(n) => Some(*n as f64),
        EvalValue::Float(f) => Some(*f),
        _ => None,
    }
}

fn truthy(v: &EvalValue) -> bool {
    match v {
        EvalValue::Bool(b) => *b,
        EvalValue::Int(n)  => *n != 0,
        EvalValue::Str(s)  => matches!(s.to_lowercase().as_str(), "yes" | "true" | "si" | "sí" | "1"),
        _ => false,
    }
}

fn json_to_eval(v: serde_json::Value) -> EvalValue {
    match v {
        serde_json::Value::Null => EvalValue::Null,
        serde_json::Value::Bool(b) => EvalValue::Bool(b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => EvalValue::Int(i),
            None => EvalValue::Float(n.as_f64().unwrap_or(0.0)),
        },
        serde_json::Value::String(s) => EvalValue::Str(s),
        serde_json::Value::Array(a) => EvalValue::List(a.into_iter().map(json_to_eval).collect()),
        serde_json::Value::Object(o) => {
            let mut m: IndexMap<String, EvalValue> = IndexMap::new();
            for (k, val) in o { m.insert(k, json_to_eval(val)); }
            EvalValue::Dict(m)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_opciones_por_defecto_ahorran_memoria() {
        let o = parse_opts(None).unwrap();
        assert!(o.headless, "por defecto debería ser headless");
        assert!(!o.images, "las imágenes deberían venir desactivadas");
        assert!(!o.gpu);
    }

    #[test]
    fn las_opciones_se_leen_del_dict() {
        let mut m: IndexMap<String, EvalValue> = IndexMap::new();
        m.insert("headless".into(), EvalValue::Bool(false));
        m.insert("images".into(),   EvalValue::Bool(true));
        m.insert("width".into(),    EvalValue::Int(1920));
        m.insert("timeout".into(),  EvalValue::Int(5_000));
        let o = parse_opts(Some(&EvalValue::Dict(m))).unwrap();
        assert!(!o.headless);
        assert!(o.images);
        assert_eq!(o.width, 1920);
        assert_eq!(o.timeout, Duration::from_millis(5_000));
    }

    #[test]
    fn yes_del_lenguaje_se_entiende_como_cierto() {
        // En Orion los booleanos se escriben yes/no; si además llegan como
        // texto desde un config, deben significar lo mismo.
        assert!(truthy(&EvalValue::Str("yes".into())));
        assert!(truthy(&EvalValue::Bool(true)));
        assert!(!truthy(&EvalValue::Str("no".into())));
    }

    #[test]
    fn un_timeout_ridiculo_se_eleva_a_un_minimo_util() {
        let mut m: IndexMap<String, EvalValue> = IndexMap::new();
        m.insert("timeout".into(), EvalValue::Int(5));
        let o = parse_opts(Some(&EvalValue::Dict(m))).unwrap();
        assert!(o.timeout >= Duration::from_secs(1), "un timeout de 5 ms no deja arrancar nada");
    }

    #[test]
    fn free_de_un_handle_inexistente_no_revienta() {
        // `with` llama a free en el camino de error: si ahí explotara, taparía
        // el error original del usuario.
        for entrada in [vec![EvalValue::Int(999_999)], vec![EvalValue::Null], vec![]] {
            assert!(matches!(free(&entrada), Ok(EvalValue::Bool(false))),
                    "free debería devolver no, sin fallar");
        }
    }

    #[test]
    fn un_handle_desconocido_da_un_error_legible() {
        let e = page_ctx(424_242).err().expect("debería fallar");
        assert!(e.contains("no existe"), "{e}");
    }

    #[test]
    fn info_responde_aunque_no_haya_navegador() {
        let EvalValue::Dict(m) = info().unwrap() else { panic!("info debería devolver un dict") };
        assert!(m.contains_key("found"));
        assert!(m.contains_key("open_browsers"));
    }

    #[test]
    fn json_se_convierte_a_valores_de_orion() {
        let v = serde_json::json!({ "a": 1, "b": [true, null, "x"], "c": 1.5 });
        let EvalValue::Dict(m) = json_to_eval(v) else { panic!("esperaba dict") };
        assert!(matches!(m["a"], EvalValue::Int(1)));
        assert!(matches!(m["c"], EvalValue::Float(f) if (f - 1.5).abs() < 1e-9));
        let EvalValue::List(l) = &m["b"] else { panic!("esperaba lista") };
        assert!(matches!(l[0], EvalValue::Bool(true)));
        assert!(matches!(l[1], EvalValue::Null));
        assert!(matches!(&l[2], EvalValue::Str(s) if s == "x"));
    }
}
