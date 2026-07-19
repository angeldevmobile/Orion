use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// Cada entrada guarda el valor (JSON, por Send) y su expiración opcional.
// La limpieza es PEREZOSA: se purga al leer/listar, sin hilos de fondo.
type Entry = (serde_json::Value, Option<Instant>);

static CACHE: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, Entry>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn expired(entry: &Entry) -> bool {
    entry.1.map(|t| Instant::now() >= t).unwrap_or(false)
}

fn purge(map: &mut HashMap<String, Entry>) {
    map.retain(|_, e| !expired(e));
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // guardar(clave, valor) o guardar(clave, valor, ttl_segundos) → Bool
        // Con ttl la entrada expira sola; acepta segundos con decimales.
        "guardar" | "set" => {
            if args.len() < 2 { return Err("cache.guardar requiere (clave, valor [, ttl_segundos])".into()); }
            let key = to_str(&args[0]);
            let val = crate::modules::json_mod::eval_to_json(args[1].clone());
            let expires = match args.get(2) {
                Some(v) => {
                    let secs = v.to_f64().map_err(|e| format!("cache.guardar: ttl inválido: {}", e))?;
                    Some(Instant::now() + Duration::from_secs_f64(secs.max(0.0)))
                }
                None => None,
            };
            cache().lock().unwrap().insert(key, (val, expires));
            Ok(EvalValue::Bool(true))
        }
        // obtener(clave) o obtener(clave, default) → valor, default o Null
        "obtener" | "get" => {
            if args.is_empty() { return Err("cache.obtener requiere (clave [, default])".into()); }
            let key = to_str(&args[0]);
            let default = args.get(1).cloned().unwrap_or(EvalValue::Null);
            let mut m = cache().lock().unwrap();
            let is_expired = m.get(&key).map(expired).unwrap_or(false);
            if is_expired { m.shift_remove(&key); }
            Ok(m.get(&key)
                .map(|e| crate::modules::json_mod::json_to_eval(e.0.clone()))
                .unwrap_or(default))
        }
        // eliminar(clave) → Bool (true si la clave existía y estaba viva)
        "eliminar" | "del" => {
            if args.is_empty() { return Err("cache.eliminar requiere (clave)".into()); }
            let removed = cache().lock().unwrap().shift_remove(&to_str(&args[0]));
            Ok(EvalValue::Bool(removed.map(|e| !expired(&e)).unwrap_or(false)))
        }
        // existe(clave) → Bool (una entrada expirada ya no existe)
        "existe" | "has" => {
            if args.is_empty() { return Err("cache.existe requiere (clave)".into()); }
            let key = to_str(&args[0]);
            let mut m = cache().lock().unwrap();
            let is_expired = m.get(&key).map(expired).unwrap_or(false);
            if is_expired { m.shift_remove(&key); }
            Ok(EvalValue::Bool(m.contains_key(&key)))
        }
        // ttl(clave) → segundos restantes (Float), o Null si no existe o no tiene ttl
        "ttl" => {
            if args.is_empty() { return Err("cache.ttl requiere (clave)".into()); }
            let key = to_str(&args[0]);
            let mut m = cache().lock().unwrap();
            let is_expired = m.get(&key).map(expired).unwrap_or(false);
            if is_expired { m.shift_remove(&key); }
            Ok(match m.get(&key) {
                Some((_, Some(t))) => EvalValue::Float((*t - Instant::now()).as_secs_f64()),
                _ => EvalValue::Null,
            })
        }
        // limpiar() → Bool
        "limpiar" | "clear" => {
            cache().lock().unwrap().clear();
            Ok(EvalValue::Bool(true))
        }
        // claves() → List<Str> (solo entradas vivas)
        "claves" | "keys" => {
            let mut m = cache().lock().unwrap();
            purge(&mut m);
            let keys: Vec<EvalValue> = m.keys().map(|k| EvalValue::Str(k.clone())).collect();
            Ok(EvalValue::List(keys))
        }
        // tamaño() → Int (solo entradas vivas)
        "tamaño" | "size" | "len" => {
            let mut m = cache().lock().unwrap();
            purge(&mut m);
            Ok(EvalValue::Int(m.len() as i64))
        }
        f => Err(format!("cache.{}() no existe", f)),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
