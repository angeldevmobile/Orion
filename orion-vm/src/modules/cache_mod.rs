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
        // set(clave, valor) o set(clave, valor, ttl_segundos) → Bool
        // Con ttl la entrada expira sola; acepta segundos con decimales.
        "set" | "guardar" => {
            if args.len() < 2 { return Err("cache.guardar requires (key, value [, ttl_seconds])".into()); }
            let key = to_str(&args[0]);
            let val = crate::modules::json_mod::eval_to_json(args[1].clone());
            let expires = match args.get(2) {
                Some(v) => {
                    let secs = v.to_f64().map_err(|e| format!("cache.guardar: invalid ttl: {}", e))?;
                    Some(Instant::now() + Duration::from_secs_f64(secs.max(0.0)))
                }
                None => None,
            };
            cache().lock().unwrap().insert(key, (val, expires));
            Ok(EvalValue::Bool(true))
        }
        // get(clave) o get(clave, default) → valor, default o Null
        "get" | "obtener" => {
            if args.is_empty() { return Err("cache.obtener requires (key [, default])".into()); }
            let key = to_str(&args[0]);
            let default = args.get(1).cloned().unwrap_or(EvalValue::Null);
            let mut m = cache().lock().unwrap();
            let is_expired = m.get(&key).map(expired).unwrap_or(false);
            if is_expired { m.shift_remove(&key); }
            Ok(m.get(&key)
                .map(|e| crate::modules::json_mod::json_to_eval(e.0.clone()))
                .unwrap_or(default))
        }
        // del(clave) → Bool (true si la clave existía y estaba viva)
        "del" | "eliminar" => {
            if args.is_empty() { return Err("cache.eliminar requires (key)".into()); }
            let removed = cache().lock().unwrap().shift_remove(&to_str(&args[0]));
            Ok(EvalValue::Bool(removed.map(|e| !expired(&e)).unwrap_or(false)))
        }
        // has(clave) → Bool (una entrada expirada ya no existe)
        "has" | "existe" => {
            if args.is_empty() { return Err("cache.existe requires (key)".into()); }
            let key = to_str(&args[0]);
            let mut m = cache().lock().unwrap();
            let is_expired = m.get(&key).map(expired).unwrap_or(false);
            if is_expired { m.shift_remove(&key); }
            Ok(EvalValue::Bool(m.contains_key(&key)))
        }
        // ttl(clave) → segundos restantes (Float), o Null si no existe o no tiene ttl
        "ttl" => {
            if args.is_empty() { return Err("cache.ttl requires (key)".into()); }
            let key = to_str(&args[0]);
            let mut m = cache().lock().unwrap();
            let is_expired = m.get(&key).map(expired).unwrap_or(false);
            if is_expired { m.shift_remove(&key); }
            Ok(match m.get(&key) {
                Some((_, Some(t))) => EvalValue::Float((*t - Instant::now()).as_secs_f64()),
                _ => EvalValue::Null,
            })
        }
        // clear() → Bool
        "clear" | "limpiar" => {
            cache().lock().unwrap().clear();
            Ok(EvalValue::Bool(true))
        }
        // keys() → List<Str> (solo entradas vivas)
        "keys" | "claves" => {
            let mut m = cache().lock().unwrap();
            purge(&mut m);
            let keys: Vec<EvalValue> = m.keys().map(|k| EvalValue::Str(k.clone())).collect();
            Ok(EvalValue::List(keys))
        }
        // size() → Int (solo entradas vivas)
        "size" | "tamaño" | "len" => {
            let mut m = cache().lock().unwrap();
            purge(&mut m);
            Ok(EvalValue::Int(m.len() as i64))
        }
        f => Err(format!("cache.{}() does not exist", f)),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
