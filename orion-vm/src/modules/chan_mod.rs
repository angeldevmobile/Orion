//! Canales de comunicación entre tareas (`spawn` / `async fn`).
//!
//! Da a Orion paso de mensajes estilo Go: productor/consumidor, fan-out/fan-in,
//! y —vía `select` sobre un canal "done"— cancelación y concurrencia estructurada.
//!
//! Un canal se identifica con un handle entero (igual patrón que `cola`): así el
//! handle es un `Int` que cruza sin problemas a una tarea `spawn` (`to_send`).
//! Los valores viajan serializados a `serde_json::Value` para no chocar con las
//! restricciones de `Send` sobre `EvalValue`.
//!
//! Bloqueo con parking real (Condvar), nunca espera activa:
//!   - `recibir` se aparca hasta que llega un valor o el canal se cierra.
//!   - `enviar` sobre un canal con capacidad se aparca si está lleno.
//!   - `select` se aparca en un Condvar global que toda emisión/cierre despierta;
//!     un contador de generación cierra la ventana de "wakeup perdido".

use crate::eval_value::EvalValue;
use crate::modules::json_mod::{eval_to_json, json_to_eval};
use indexmap::IndexMap as HashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

struct ChanState {
    queue:  VecDeque<serde_json::Value>,
    closed: bool,
}

struct Chan {
    inner:     Mutex<ChanState>,
    not_empty: Condvar, // despierta a los `recibir` bloqueados
    not_full:  Condvar, // despierta a los `enviar` bloqueados (canal con capacidad)
    cap:       usize,   // 0 = ilimitado
}

static REGISTRY: OnceLock<Mutex<HashMap<i64, Arc<Chan>>>> = OnceLock::new();
static NEXT_ID:  AtomicI64 = AtomicI64::new(1);

/// Evento global para `select`: (contador de generación, condvar). Cada emisión
/// o cierre incrementa el contador y despierta; select re-escanea los canales.
static SELECT_EV: OnceLock<(Mutex<u64>, Condvar)> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<i64, Arc<Chan>>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn select_ev() -> &'static (Mutex<u64>, Condvar) {
    SELECT_EV.get_or_init(|| (Mutex::new(0), Condvar::new()))
}

/// Notifica a los `select` bloqueados que algo cambió (nuevo valor o cierre).
fn bump_select() {
    let (m, c) = select_ev();
    {
        let mut g = m.lock().unwrap();
        *g = g.wrapping_add(1);
    }
    c.notify_all();
}

fn get_chan(id: i64) -> Result<Arc<Chan>, String> {
    registry()
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("chan: canal {} no existe (¿cerrado y eliminado?)", id))
}

fn as_int(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("chan: se esperaba un handle de canal (int), no {}", other)),
    }
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // crear(cap?) → Int (handle). cap=0 o ausente → ilimitado.
        "crear" | "create" | "new" => {
            let cap = args.get(0).and_then(|v| match v {
                EvalValue::Int(n)   => Some(*n as usize),
                EvalValue::Float(f) => Some(*f as usize),
                _ => None,
            }).unwrap_or(0);
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            let ch = Arc::new(Chan {
                inner: Mutex::new(ChanState { queue: VecDeque::new(), closed: false }),
                not_empty: Condvar::new(),
                not_full:  Condvar::new(),
                cap,
            });
            registry().lock().unwrap().insert(id, ch);
            Ok(EvalValue::Int(id))
        }

        // enviar(id, valor) → Bool. Bloquea si el canal tiene capacidad y está
        // lleno. Error si el canal está cerrado.
        "enviar" | "send" | "push" => {
            if args.len() < 2 { return Err("chan.enviar requiere (canal, valor)".into()); }
            let id  = as_int(&args[0])?;
            let val = eval_to_json(args[1].clone());
            let ch  = get_chan(id)?;

            let mut st = ch.inner.lock().unwrap();
            if st.closed { return Err(format!("chan.enviar: el canal {} está cerrado", id)); }
            if ch.cap > 0 {
                while st.queue.len() >= ch.cap && !st.closed {
                    st = ch.not_full.wait(st).unwrap();
                }
                if st.closed { return Err(format!("chan.enviar: el canal {} se cerró", id)); }
            }
            st.queue.push_back(val);
            ch.not_empty.notify_one();
            drop(st);
            bump_select();
            Ok(EvalValue::Bool(true))
        }

        // recibir(id) → valor | Null. Bloquea (parking) hasta que haya un valor.
        // Devuelve Null si el canal se cierra y ya no quedan valores.
        "recibir" | "recv" | "pop" => {
            let id = as_int(args.get(0).ok_or("chan.recibir requiere (canal)")?)?;
            let ch = get_chan(id)?;
            let mut st = ch.inner.lock().unwrap();
            loop {
                if let Some(v) = st.queue.pop_front() {
                    ch.not_full.notify_one();
                    return Ok(json_to_eval(v));
                }
                if st.closed {
                    return Ok(EvalValue::Null);
                }
                st = ch.not_empty.wait(st).unwrap();
            }
        }

        // try_recibir(id) → valor | Null (no bloquea; Null si vacío).
        "try_recibir" | "try_recv" | "intentar_recibir" => {
            let id = as_int(args.get(0).ok_or("chan.try_recibir requiere (canal)")?)?;
            let ch = get_chan(id)?;
            let mut st = ch.inner.lock().unwrap();
            match st.queue.pop_front() {
                Some(v) => { ch.not_full.notify_one(); Ok(json_to_eval(v)) }
                None    => Ok(EvalValue::Null),
            }
        }

        // cerrar(id) → Bool. Despierta a todos los bloqueados.
        "cerrar" | "close" => {
            let id = as_int(args.get(0).ok_or("chan.cerrar requiere (canal)")?)?;
            let ch = get_chan(id)?;
            {
                let mut st = ch.inner.lock().unwrap();
                st.closed = true;
            }
            ch.not_empty.notify_all();
            ch.not_full.notify_all();
            bump_select();
            Ok(EvalValue::Bool(true))
        }

        // cerrada(id) → Bool
        "cerrada" | "is_closed" | "closed" => {
            let id = as_int(args.get(0).ok_or("chan.cerrada requiere (canal)")?)?;
            let ch = get_chan(id)?;
            let st = ch.inner.lock().unwrap();
            Ok(EvalValue::Bool(st.closed))
        }

        // len(id) / tamaño(id) → Int (valores en cola ahora mismo)
        "len" | "tamaño" | "size" => {
            let id = as_int(args.get(0).ok_or("chan.len requiere (canal)")?)?;
            let ch = get_chan(id)?;
            let st = ch.inner.lock().unwrap();
            Ok(EvalValue::Int(st.queue.len() as i64))
        }

        // eliminar(id) → Bool. Libera el canal del registro.
        "eliminar" | "delete" | "free" => {
            let id = as_int(args.get(0).ok_or("chan.eliminar requiere (canal)")?)?;
            let existed = registry().lock().unwrap().shift_remove(&id).is_some();
            Ok(EvalValue::Bool(existed))
        }

        // select([id1, id2, ...]) → Dict {canal, valor} | Null.
        // Bloquea hasta que ALGÚN canal tenga un valor y lo devuelve junto con su
        // handle. Devuelve Null si todos los canales están cerrados y vacíos.
        // Es la base de la cancelación (canal "done") y del fan-in estructurado.
        "select" | "seleccionar" => {
            let ids: Vec<i64> = match args.get(0) {
                Some(EvalValue::List(items)) => items.iter()
                    .map(as_int).collect::<Result<Vec<_>, _>>()?,
                _ => return Err("chan.select requiere una lista de canales".into()),
            };
            if ids.is_empty() { return Ok(EvalValue::Null); }

            loop {
                // Leer la generación ANTES de escanear: si algo cambia mientras
                // escaneamos, la generación diferirá y no nos dormiremos.
                let gen_before = *select_ev().0.lock().unwrap();

                let mut all_done = true;
                for &id in &ids {
                    let ch = match get_chan(id) { Ok(c) => c, Err(_) => continue };
                    let mut st = ch.inner.lock().unwrap();
                    if let Some(v) = st.queue.pop_front() {
                        ch.not_full.notify_one();
                        let mut d = HashMap::new();
                        d.insert("canal".to_string(), EvalValue::Int(id));
                        d.insert("valor".to_string(), json_to_eval(v));
                        return Ok(EvalValue::Dict(d));
                    }
                    if !st.closed { all_done = false; }
                }
                if all_done { return Ok(EvalValue::Null); }

                // Nada listo: aparcar en el condvar global hasta el próximo evento.
                let (m, c) = select_ev();
                let g = m.lock().unwrap();
                if *g == gen_before {
                    let _ = c.wait_timeout(g, Duration::from_millis(100)).unwrap();
                }
            }
        }

        // lista() → List<Int> de handles de canales vivos
        "lista" | "list" => {
            let ids: Vec<EvalValue> = registry().lock().unwrap()
                .keys().map(|k| EvalValue::Int(*k)).collect();
            Ok(EvalValue::List(ids))
        }

        f => Err(format!("chan.{}() no existe", f)),
    }
}
