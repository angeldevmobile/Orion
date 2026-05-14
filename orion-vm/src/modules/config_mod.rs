use crate::eval_value::EvalValue;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // load(path) → dict con la configuración
        "load" => {
            if args.is_empty() { return Err("config.load requiere (path)".into()); }
            load_config(&to_str(&args[0]))
        }
        // get(dict, key) → valor o null
        "get" => {
            if args.len() < 2 { return Err("config.get requiere (dict, key)".into()); }
            let key = to_str(&args[1]);
            match &args[0] {
                EvalValue::Dict(m) => Ok(m.get(&key).cloned().unwrap_or(EvalValue::Null)),
                _ => Err("config.get: primer argumento debe ser un dict".into()),
            }
        }
        // merge(base_dict, path) → dict fusionado (extra sobreescribe base)
        "merge" => {
            if args.len() < 2 { return Err("config.merge requiere (base_dict, path)".into()); }
            let extra = load_config(&to_str(&args[1]))?;
            match (&args[0], extra) {
                (EvalValue::Dict(base), EvalValue::Dict(ext)) => {
                    let mut merged = base.clone();
                    for (k, v) in ext { merged.insert(k, v); }
                    Ok(EvalValue::Dict(merged))
                }
                _ => Err("config.merge: ambos valores deben ser dicts".into()),
            }
        }
        // keys(dict) → lista de claves
        "keys" => {
            if args.is_empty() { return Err("config.keys requiere (dict)".into()); }
            match &args[0] {
                EvalValue::Dict(m) => {
                    let keys: Vec<EvalValue> = m.keys().map(|k| EvalValue::Str(k.clone())).collect();
                    Ok(EvalValue::List(keys))
                }
                _ => Err("config.keys: argumento debe ser un dict".into()),
            }
        }
        f => Err(format!("config.{}() no existe", f)),
    }
}

fn load_config(path: &str) -> Result<EvalValue, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("config.load '{}': {}", path, e))?;

    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "json" => {
            let v: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| format!("config.load JSON: {}", e))?;
            Ok(json_to_eval(v))
        }
        "toml" => {
            let v: toml::Value = toml::from_str(&content)
                .map_err(|e| format!("config.load TOML: {}", e))?;
            Ok(toml_to_eval(v))
        }
        other => Err(format!(
            "config.load: formato '{}' no soportado — usa .json o .toml",
            other
        )),
    }
}

fn json_to_eval(v: serde_json::Value) -> EvalValue {
    match v {
        serde_json::Value::Null      => EvalValue::Null,
        serde_json::Value::Bool(b)   => EvalValue::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { EvalValue::Int(i) }
            else { EvalValue::Float(n.as_f64().unwrap_or(0.0)) }
        }
        serde_json::Value::String(s) => EvalValue::Str(s),
        serde_json::Value::Array(a)  => EvalValue::List(a.into_iter().map(json_to_eval).collect()),
        serde_json::Value::Object(o) => {
            EvalValue::Dict(o.into_iter().map(|(k, v)| (k, json_to_eval(v))).collect())
        }
    }
}

fn toml_to_eval(v: toml::Value) -> EvalValue {
    match v {
        toml::Value::String(s)   => EvalValue::Str(s),
        toml::Value::Integer(i)  => EvalValue::Int(i),
        toml::Value::Float(f)    => EvalValue::Float(f),
        toml::Value::Boolean(b)  => EvalValue::Bool(b),
        toml::Value::Datetime(d) => EvalValue::Str(d.to_string()),
        toml::Value::Array(a)    => EvalValue::List(a.into_iter().map(toml_to_eval).collect()),
        toml::Value::Table(t)    => {
            EvalValue::Dict(t.into_iter().map(|(k, v)| (k, toml_to_eval(v))).collect())
        }
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
