use crate::eval_value::EvalValue;
use indexmap::IndexMap;
use std::collections::HashMap as StdHashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

// Sesiones server-side: un store global (compartido entre los workers de serve)
// mapea un session-id a un Dict de datos arbitrarios. Los valores se guardan
// como JSON por el puente Send, igual que cache. La caducidad es perezosa:
// se poda al acceder o vía session.sweep(max_edad).

struct Session {
    data:        IndexMap<String, serde_json::Value>,
    last_access: u64,
}

static STORE: OnceLock<Mutex<StdHashMap<String, Session>>> = OnceLock::new();

fn store() -> &'static Mutex<StdHashMap<String, Session>> {
    STORE.get_or_init(|| Mutex::new(StdHashMap::new()))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // new() → String  — genera un session-id nuevo (no crea la sesión aún)
        "new" | "id" => Ok(EvalValue::Str(gen_sid())),

        // set(sid, clave, valor) → Bool  — crea la sesión si no existía
        "set" | "guardar" => {
            if args.len() < 3 { return Err("session.set requires (sid, key, value)".into()); }
            let sid = to_str(&args[0]);
            let key = to_str(&args[1]);
            let val = crate::modules::json_mod::eval_to_json(args[2].clone());
            let mut s = store().lock().unwrap();
            let sess = s.entry(sid).or_insert_with(|| Session {
                data: IndexMap::new(), last_access: now(),
            });
            sess.data.insert(key, val);
            sess.last_access = now();
            Ok(EvalValue::Bool(true))
        }

        // get(sid, clave, default?) → valor | default | Null
        "get" | "obtener" => {
            if args.len() < 2 { return Err("session.get requires (sid, key [, default])".into()); }
            let sid = to_str(&args[0]);
            let key = to_str(&args[1]);
            let default = args.get(2).cloned().unwrap_or(EvalValue::Null);
            let mut s = store().lock().unwrap();
            Ok(match s.get_mut(&sid) {
                Some(sess) => {
                    sess.last_access = now();
                    sess.data.get(&key)
                        .map(|v| crate::modules::json_mod::json_to_eval(v.clone()))
                        .unwrap_or(default)
                }
                None => default,
            })
        }

        // all(sid) → Dict con todos los datos de la sesión
        "all" | "todo" => {
            if args.is_empty() { return Err("session.all requires (sid)".into()); }
            let sid = to_str(&args[0]);
            let s = store().lock().unwrap();
            Ok(match s.get(&sid) {
                Some(sess) => {
                    let mut m = IndexMap::new();
                    for (k, v) in &sess.data {
                        m.insert(k.clone(), crate::modules::json_mod::json_to_eval(v.clone()));
                    }
                    EvalValue::Dict(m)
                }
                None => EvalValue::Dict(IndexMap::new()),
            })
        }

        // has(sid, clave) → Bool
        "has" | "existe" => {
            if args.len() < 2 { return Err("session.has requires (sid, key)".into()); }
            let s = store().lock().unwrap();
            Ok(EvalValue::Bool(
                s.get(&to_str(&args[0])).map(|se| se.data.contains_key(&to_str(&args[1]))).unwrap_or(false)
            ))
        }

        // delete(sid, clave) → Bool  — borra una clave de la sesión
        "delete" | "del" | "eliminar" => {
            if args.len() < 2 { return Err("session.delete requires (sid, key)".into()); }
            let mut s = store().lock().unwrap();
            let removed = s.get_mut(&to_str(&args[0]))
                .map(|se| se.data.shift_remove(&to_str(&args[1])).is_some())
                .unwrap_or(false);
            Ok(EvalValue::Bool(removed))
        }

        // destroy(sid) → Bool  — elimina la sesión entera (logout)
        "destroy" | "destruir" => {
            if args.is_empty() { return Err("session.destroy requires (sid)".into()); }
            let removed = store().lock().unwrap().remove(&to_str(&args[0])).is_some();
            Ok(EvalValue::Bool(removed))
        }

        // count() → Int  — sesiones activas
        "count" | "cuenta" => Ok(EvalValue::Int(store().lock().unwrap().len() as i64)),

        // sweep(max_edad_secs) → Int  — poda sesiones inactivas, devuelve cuántas
        "sweep" | "podar" => {
            let max_age = args.first().and_then(|v| v.to_i64().ok()).unwrap_or(3600).max(0) as u64;
            let cutoff = now().saturating_sub(max_age);
            let mut s = store().lock().unwrap();
            let before = s.len();
            s.retain(|_, se| se.last_access >= cutoff);
            Ok(EvalValue::Int((before - s.len()) as i64))
        }

        f => Err(format!("session.{}() does not exist", f)),
    }
}

/// ¿Existe una sesión con datos para este sid? Lo usa serve para decidir si
/// emite el Set-Cookie del session-id automáticamente.
pub fn exists(sid: &str) -> bool {
    store().lock().unwrap().get(sid).map(|s| !s.data.is_empty()).unwrap_or(false)
}

/// Genera un session-id aleatorio (128 bits en hex).
pub fn gen_sid() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
