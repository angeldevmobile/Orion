use crate::eval_value::EvalValue;
use std::collections::HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // pull(key, default?) → valor de la variable de entorno
        "pull" | "get" => {
            if args.is_empty() { return Err("env.pull requiere (key, default?)".into()); }
            let key = to_str(&args[0]);
            match std::env::var(&key) {
                Ok(v)  => Ok(EvalValue::Str(v)),
                Err(_) => {
                    if args.len() > 1 { Ok(args[1].clone()) }
                    else { Ok(EvalValue::Null) }
                }
            }
        }
        // push(key, value) → establece variable de entorno
        "push" | "set" => {
            if args.len() < 2 { return Err("env.push requiere (key, value)".into()); }
            let key = to_str(&args[0]);
            let val = to_str(&args[1]);
            std::env::set_var(&key, &val);
            let mut m = HashMap::new();
            m.insert("key".into(),   EvalValue::Str(key));
            m.insert("value".into(), EvalValue::Str(val));
            Ok(EvalValue::Dict(m))
        }
        // load(path?) → carga .env file
        "load" => {
            let path = if args.is_empty() { ".env".into() } else { to_str(&args[0]) };
            load_dotenv(&path)
        }
        // reveal() → dict con todas las variables de entorno
        "reveal" | "all" => {
            let map: HashMap<String, EvalValue> = std::env::vars()
                .map(|(k, v)| (k, EvalValue::Str(v)))
                .collect();
            Ok(EvalValue::Dict(map))
        }
        // has(key) → bool
        "has" => {
            if args.is_empty() { return Err("env.has requiere (key)".into()); }
            let key = to_str(&args[0]);
            Ok(EvalValue::Bool(std::env::var(&key).is_ok()))
        }
        // remove(key)
        "remove" => {
            if args.is_empty() { return Err("env.remove requiere (key)".into()); }
            let key = to_str(&args[0]);
            std::env::remove_var(&key);
            Ok(EvalValue::Null)
        }

        f => Err(format!("env.{}() no existe", f)),
    }
}

fn load_dotenv(path: &str) -> Result<EvalValue, String> {
    if !std::path::Path::new(path).exists() {
        return Ok(EvalValue::Bool(false));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("env.load: {}", e))?;
    let mut count = 0i64;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if let Some((key, val)) = line.split_once('=') {
            std::env::set_var(key.trim(), val.trim());
            count += 1;
        }
    }
    Ok(EvalValue::Int(count))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
