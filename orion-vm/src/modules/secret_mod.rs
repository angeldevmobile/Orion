use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::sync::Mutex;

static SECRETS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // load(path?) → carga el .env y devuelve cantidad de variables cargadas
        "load" => {
            let path = if args.is_empty() { ".env".to_string() } else { to_str(&args[0]) };
            let map  = parse_dotenv(&path)?;
            let n    = map.len() as i64;
            *SECRETS.lock().unwrap() = Some(map);
            Ok(EvalValue::Int(n))
        }
        // get(key, default?) → valor del secret o default/null
        "get" => {
            if args.is_empty() { return Err("secret.get requiere (key, default?)".into()); }
            let key = to_str(&args[0]);
            if let Some(val) = secrets_lookup(&key) {
                return Ok(EvalValue::Str(val));
            }
            if args.len() > 1 { Ok(args[1].clone()) } else { Ok(EvalValue::Null) }
        }
        // require(key) → valor o error claro
        "require" => {
            if args.is_empty() { return Err("secret.require requiere (key)".into()); }
            let key = to_str(&args[0]);
            match secrets_lookup(&key) {
                Some(val) => Ok(EvalValue::Str(val)),
                None => Err(format!(
                    "secret.require: '{}' no encontrado — agrega esta variable a tu .env",
                    key
                )),
            }
        }
        // mask(value) → "ab***cd" (oculta parte central)
        "mask" => {
            if args.is_empty() { return Err("secret.mask requiere (value)".into()); }
            let val = to_str(&args[0]);
            let masked = if val.len() <= 4 {
                "*".repeat(val.len())
            } else {
                format!("{}***{}", &val[..2], &val[val.len() - 2..])
            };
            Ok(EvalValue::Str(masked))
        }
        // has(key) → bool
        "has" => {
            if args.is_empty() { return Err("secret.has requiere (key)".into()); }
            let key = to_str(&args[0]);
            Ok(EvalValue::Bool(secrets_lookup(&key).is_some()))
        }
        // all() → dict con todos los secrets cargados
        "all" => {
            let guard = SECRETS.lock().unwrap();
            match guard.as_ref() {
                None => Ok(EvalValue::Dict(HashMap::new())),
                Some(m) => {
                    let d = m.iter()
                        .map(|(k, v)| (k.clone(), EvalValue::Str(v.clone())))
                        .collect();
                    Ok(EvalValue::Dict(d))
                }
            }
        }
        f => Err(format!("secret.{}() no existe", f)),
    }
}

// Busca primero en secrets cargados, luego en variables de entorno del proceso
fn secrets_lookup(key: &str) -> Option<String> {
    let guard = SECRETS.lock().unwrap();
    if let Some(map) = guard.as_ref() {
        if let Some(v) = map.get(key) {
            return Some(v.clone());
        }
    }
    drop(guard);
    std::env::var(key).ok()
}

fn parse_dotenv(path: &str) -> Result<HashMap<String, String>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("secret.load '{}': {}", path, e))?;

    let mut map = HashMap::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim().to_string();
            // quitar comillas simples o dobles del valor
            let v = v.trim().trim_matches('"').trim_matches('\'').to_string();
            map.insert(k, v);
        }
    }
    Ok(map)
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
