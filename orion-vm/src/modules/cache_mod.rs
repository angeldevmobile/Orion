use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static CACHE: OnceLock<Mutex<HashMap<String, serde_json::Value>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, serde_json::Value>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // guardar(clave, valor) → Bool
        "guardar" | "set" => {
            if args.len() < 2 { return Err("cache.guardar requiere (clave, valor)".into()); }
            let key = to_str(&args[0]);
            let val = crate::modules::json_mod::eval_to_json(args[1].clone());
            cache().lock().unwrap().insert(key, val);
            Ok(EvalValue::Bool(true))
        }
        // obtener(clave) → valor o Null
        "obtener" | "get" => {
            if args.is_empty() { return Err("cache.obtener requiere (clave)".into()); }
            let key = to_str(&args[0]);
            Ok(cache().lock().unwrap().get(&key)
                .map(|v| crate::modules::json_mod::json_to_eval(v.clone()))
                .unwrap_or(EvalValue::Null))
        }
        // eliminar(clave) → Bool
        "eliminar" | "del" => {
            if args.is_empty() { return Err("cache.eliminar requiere (clave)".into()); }
            cache().lock().unwrap().remove(&to_str(&args[0]));
            Ok(EvalValue::Bool(true))
        }
        // existe(clave) → Bool
        "existe" | "has" => {
            if args.is_empty() { return Err("cache.existe requiere (clave)".into()); }
            Ok(EvalValue::Bool(cache().lock().unwrap().contains_key(&to_str(&args[0]))))
        }
        // limpiar() → Bool
        "limpiar" | "clear" => {
            cache().lock().unwrap().clear();
            Ok(EvalValue::Bool(true))
        }
        // claves() → List<Str>
        "claves" | "keys" => {
            let keys: Vec<EvalValue> = cache().lock().unwrap()
                .keys().map(|k| EvalValue::Str(k.clone())).collect();
            Ok(EvalValue::List(keys))
        }
        // tamaño() → Int
        "tamaño" | "size" | "len" => {
            Ok(EvalValue::Int(cache().lock().unwrap().len() as i64))
        }
        f => Err(format!("cache.{}() no existe", f)),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
