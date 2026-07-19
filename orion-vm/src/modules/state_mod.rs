//! Módulo `state`: estado compartido, thread-safe y opcionalmente persistente.
//!
//! Resuelve el punto débil clásico de los servidores en Orion: las variables
//! globales crean LOCALES dentro de cada handler y no sobreviven entre requests.
//! `state` mantiene un único store en memoria protegido por `Mutex` —seguro bajo
//! el pool de hilos de `serve`— con:
//!   • operaciones atómicas (`incr`/`decr`): get-modify-set bajo un solo lock,
//!     sin condiciones de carrera aunque dos requests lleguen a la vez;
//!   • persistencia opcional a disco (`persist`): sobrevive reinicios sin que el
//!     usuario tenga que serializar a JSON a mano.
//!
//! ```orion
//! use state
//! state.persist("app.db")        -- respalda a disco (y carga lo existente)
//! state.set("visitas", 0)
//! fn router(req) {
//!     n = state.incr("visitas")  -- atómico y seguro con N hilos
//!     return { "status": 200, "body": "visita #" + str(n) }
//! }
//! ```

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use indexmap::IndexMap;
use crate::eval_value::EvalValue;
use crate::modules::json_mod::{eval_to_json, json_to_eval};

struct Store {
    data: IndexMap<String, EvalValue>,
    /// Si está definido, cada escritura se vuelca a este archivo JSON.
    persist_path: Option<PathBuf>,
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Store { data: IndexMap::new(), persist_path: None }))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

/// Vuelca el store a disco si hay ruta de persistencia configurada.
fn flush(s: &Store) {
    if let Some(path) = &s.persist_path {
        let obj: serde_json::Map<String, serde_json::Value> = s.data.iter()
            .map(|(k, v)| (k.clone(), eval_to_json(v.clone())))
            .collect();
        let json = serde_json::Value::Object(obj);
        let _ = std::fs::write(path, serde_json::to_string_pretty(&json).unwrap_or_default());
    }
}

/// Extrae el componente numérico de un EvalValue (para incr/decr).
fn as_number(v: &EvalValue) -> Option<f64> {
    match v {
        EvalValue::Int(n)   => Some(*n as f64),
        EvalValue::Float(f) => Some(*f),
        _ => None,
    }
}

/// Valor actual para incr/decr: inexistente → 0; no numérico → error claro.
fn numeric_current(op: &str, s: &Store, key: &str) -> Result<f64, String> {
    match s.data.get(key) {
        None => Ok(0.0),
        Some(v) => as_number(v).ok_or_else(|| format!(
            "state.{}: la clave '{}' contiene un valor no numérico ({}); usa set para reemplazarla",
            op, key, v.type_name()
        )),
    }
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let mut s = store().lock().unwrap();
    match function {
        // set(clave, valor) → valor guardado
        "set" | "put" | "guardar" => {
            if args.len() < 2 { return Err("state.set requiere (clave, valor)".into()); }
            let key = to_str(&args[0]);
            let val = args[1].clone();
            s.data.insert(key, val.clone());
            flush(&s);
            Ok(val)
        }
        // get(clave [, default]) → valor o default/Null
        "get" | "obtener" => {
            if args.is_empty() { return Err("state.get requiere (clave)".into()); }
            let key = to_str(&args[0]);
            Ok(s.data.get(&key).cloned()
                .unwrap_or_else(|| args.get(1).cloned().unwrap_or(EvalValue::Null)))
        }
        // has(clave) → Bool
        "has" | "exists" | "existe" => {
            if args.is_empty() { return Err("state.has requiere (clave)".into()); }
            Ok(EvalValue::Bool(s.data.contains_key(&to_str(&args[0]))))
        }
        // incr(clave [, delta=1]) → nuevo valor (ATÓMICO: get+set bajo un lock)
        // Clave inexistente arranca en 0; clave con valor NO numérico → error
        // claro (antes se pisaba en silencio).
        "incr" | "increment" => {
            if args.is_empty() { return Err("state.incr requiere (clave)".into()); }
            let key = to_str(&args[0]);
            let delta = args.get(1).and_then(as_number).unwrap_or(1.0);
            let current = numeric_current("incr", &s, &key)?;
            let next = current + delta;
            // Si tanto el valor previo como el delta son enteros, mantener Int.
            let is_int = delta.fract() == 0.0 && current.fract() == 0.0;
            let val = if is_int { EvalValue::Int(next as i64) } else { EvalValue::Float(next) };
            s.data.insert(key, val.clone());
            flush(&s);
            Ok(val)
        }
        // decr(clave [, delta=1]) → nuevo valor (atómico)
        "decr" | "decrement" => {
            if args.is_empty() { return Err("state.decr requiere (clave)".into()); }
            let key = to_str(&args[0]);
            let delta = args.get(1).and_then(as_number).unwrap_or(1.0);
            let current = numeric_current("decr", &s, &key)?;
            let next = current - delta;
            let is_int = delta.fract() == 0.0 && current.fract() == 0.0;
            let val = if is_int { EvalValue::Int(next as i64) } else { EvalValue::Float(next) };
            s.data.insert(key, val.clone());
            flush(&s);
            Ok(val)
        }
        // setnx(clave, valor) → Bool: guarda SOLO si la clave no existe.
        // Atómico bajo el mismo lock — sirve como candado simple entre requests.
        "setnx" | "set_if_absent" => {
            if args.len() < 2 { return Err("state.setnx requiere (clave, valor)".into()); }
            let key = to_str(&args[0]);
            if s.data.contains_key(&key) { return Ok(EvalValue::Bool(false)); }
            s.data.insert(key, args[1].clone());
            flush(&s);
            Ok(EvalValue::Bool(true))
        }
        // delete(clave) → Bool (true si existía)
        "delete" | "remove" | "del" | "eliminar" => {
            if args.is_empty() { return Err("state.delete requiere (clave)".into()); }
            let existed = s.data.shift_remove(&to_str(&args[0])).is_some();
            flush(&s);
            Ok(EvalValue::Bool(existed))
        }
        // keys() → List<Str>
        "keys" | "claves" => {
            Ok(EvalValue::List(s.data.keys().map(|k| EvalValue::Str(k.clone())).collect()))
        }
        // all() / snapshot() → Dict con todo el estado
        "all" | "snapshot" | "todo" => {
            let map = s.data.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            Ok(EvalValue::Dict(map))
        }
        // len() / size() → Int
        "len" | "size" | "tamaño" => Ok(EvalValue::Int(s.data.len() as i64)),
        // clear() → Bool
        "clear" | "limpiar" => {
            s.data.clear();
            flush(&s);
            Ok(EvalValue::Bool(true))
        }
        // persist(ruta) → Bool. Activa el respaldo a disco y carga lo existente.
        "persist" | "persistir" => {
            if args.is_empty() { return Err("state.persist requiere (ruta)".into()); }
            let path = PathBuf::from(to_str(&args[0]));
            // Si el archivo ya existe, sembrar el store con su contenido.
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str::<serde_json::Value>(&content) {
                    for (k, v) in obj {
                        s.data.insert(k, json_to_eval(v));
                    }
                }
            }
            s.persist_path = Some(path);
            Ok(EvalValue::Bool(true))
        }
        f => Err(format!("state.{}() no existe", f)),
    }
}
