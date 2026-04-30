use crate::eval_value::EvalValue;
use std::collections::HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // parse(string) → valor Orion
        "parse" => {
            let s = one_str("parse", args)?;
            let v: serde_json::Value = serde_json::from_str(&s)
                .map_err(|e| format!("json.parse: {}", e))?;
            Ok(json_to_eval(v))
        }
        // forge(value) → string JSON
        "forge" => {
            let v = one_arg("forge", args)?;
            let j = eval_to_json(v);
            serde_json::to_string(&j).map(EvalValue::Str)
                .map_err(|e| format!("json.forge: {}", e))
        }
        // forge_pretty / forge(value, beauty=true)
        "forge_pretty" => {
            let v = one_arg("forge_pretty", args)?;
            let j = eval_to_json(v);
            serde_json::to_string_pretty(&j).map(EvalValue::Str)
                .map_err(|e| format!("json.forge_pretty: {}", e))
        }
        // absorb(path) → valor Orion
        "absorb" => {
            let path = one_str("absorb", args)?;
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("json.absorb: {}", e))?;
            let v: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| format!("json.absorb: {}", e))?;
            Ok(json_to_eval(v))
        }
        // emit(path, value) → escribe JSON al archivo
        "emit" => {
            if args.len() < 2 { return Err("json.emit requiere (path, value)".into()); }
            let path = match &args[0] { EvalValue::Str(s) => s.clone(), v => format!("{}", v) };
            let j = eval_to_json(args[1].clone());
            let s = serde_json::to_string_pretty(&j)
                .map_err(|e| format!("json.emit: {}", e))?;
            std::fs::write(&path, s).map_err(|e| format!("json.emit: {}", e))?;
            Ok(EvalValue::Null)
        }
        // trace(obj, "user.name") → valor anidado
        "trace" => {
            if args.len() < 2 { return Err("json.trace requiere (obj, path)".into()); }
            let path = match &args[1] { EvalValue::Str(s) => s.clone(), v => format!("{}", v) };
            let mut cur = args[0].clone();
            for key in path.split('.') {
                cur = match cur {
                    EvalValue::Dict(m) => m.get(key).cloned().unwrap_or(EvalValue::Null),
                    _ => return Ok(EvalValue::Null),
                };
            }
            Ok(cur)
        }
        // fuse(a, b) → merge superficial de dos dicts
        "fuse" => {
            if args.len() < 2 { return Err("json.fuse requiere (a, b)".into()); }
            let mut result: HashMap<String, EvalValue> = HashMap::new();
            for arg in args {
                if let EvalValue::Dict(m) = arg {
                    result.extend(m);
                }
            }
            Ok(EvalValue::Dict(result))
        }
        // purify(obj) → elimina nulls y vacíos
        "purify" => {
            let v = one_arg("purify", args)?;
            Ok(purify(v))
        }
        // validate(obj, schema) → bool
        "validate" => {
            if args.len() < 2 { return Err("json.validate requiere (obj, schema)".into()); }
            let obj    = args[0].clone();
            let schema = args[1].clone();
            Ok(EvalValue::Bool(validate(&obj, &schema)))
        }
        // keys, values ya están en builtins pero los exponemos aquí también
        "keys" => {
            let v = one_arg("keys", args)?;
            match v {
                EvalValue::Dict(m) =>
                    Ok(EvalValue::List(m.into_keys().map(EvalValue::Str).collect())),
                _ => Err("json.keys requiere un objeto".into()),
            }
        }
        "values" => {
            let v = one_arg("values", args)?;
            match v {
                EvalValue::Dict(m) =>
                    Ok(EvalValue::List(m.into_values().collect())),
                _ => Err("json.values requiere un objeto".into()),
            }
        }

        f => Err(format!("json.{}() no existe", f)),
    }
}

// serde_json::Value → EvalValue
pub fn json_to_eval(v: serde_json::Value) -> EvalValue {
    match v {
        serde_json::Value::Null       => EvalValue::Null,
        serde_json::Value::Bool(b)    => EvalValue::Bool(b),
        serde_json::Value::Number(n)  => {
            if let Some(i) = n.as_i64() { EvalValue::Int(i) }
            else { EvalValue::Float(n.as_f64().unwrap_or(0.0)) }
        }
        serde_json::Value::String(s)  => EvalValue::Str(s),
        serde_json::Value::Array(arr) => EvalValue::List(arr.into_iter().map(json_to_eval).collect()),
        serde_json::Value::Object(m)  => {
            EvalValue::Dict(m.into_iter().map(|(k, v)| (k, json_to_eval(v))).collect())
        }
    }
}

// EvalValue → serde_json::Value
pub fn eval_to_json(v: EvalValue) -> serde_json::Value {
    match v {
        EvalValue::Null        => serde_json::Value::Null,
        EvalValue::Bool(b)     => serde_json::Value::Bool(b),
        EvalValue::Int(n)      => serde_json::json!(n),
        EvalValue::Float(f)    => serde_json::json!(f),
        EvalValue::Str(s)      => serde_json::Value::String(s),
        EvalValue::List(arr)   => serde_json::Value::Array(arr.into_iter().map(eval_to_json).collect()),
        EvalValue::Dict(m)     => {
            let obj: serde_json::Map<String, serde_json::Value> =
                m.into_iter().map(|(k, v)| (k, eval_to_json(v))).collect();
            serde_json::Value::Object(obj)
        }
        other => serde_json::Value::String(format!("{}", other)),
    }
}

fn purify(v: EvalValue) -> EvalValue {
    match v {
        EvalValue::Null => EvalValue::Null,
        EvalValue::Str(ref s) if s.is_empty() => EvalValue::Null,
        EvalValue::List(arr) => {
            let cleaned: Vec<EvalValue> = arr.into_iter()
                .map(purify)
                .filter(|x| !matches!(x, EvalValue::Null))
                .collect();
            EvalValue::List(cleaned)
        }
        EvalValue::Dict(m) => {
            let cleaned: HashMap<String, EvalValue> = m.into_iter()
                .filter_map(|(k, v)| {
                    let p = purify(v);
                    if matches!(p, EvalValue::Null) { None } else { Some((k, p)) }
                })
                .collect();
            EvalValue::Dict(cleaned)
        }
        other => other,
    }
}

fn validate(obj: &EvalValue, schema: &EvalValue) -> bool {
    let EvalValue::Dict(schema_map) = schema else { return false; };
    let EvalValue::Dict(obj_map)    = obj    else { return false; };
    for (key, expected_type) in schema_map {
        let EvalValue::Str(type_name) = expected_type else { return false; };
        let Some(val) = obj_map.get(key) else { return false; };
        let matches = match type_name.as_str() {
            "str"   => matches!(val, EvalValue::Str(_)),
            "int"   => matches!(val, EvalValue::Int(_)),
            "float" => matches!(val, EvalValue::Float(_)),
            "bool"  => matches!(val, EvalValue::Bool(_)),
            "list"  => matches!(val, EvalValue::List(_)),
            "dict"  => matches!(val, EvalValue::Dict(_)),
            _       => false,
        };
        if !matches { return false; }
    }
    true
}

fn one_str(fn_name: &str, args: Vec<EvalValue>) -> Result<String, String> {
    let v = one_arg(fn_name, args)?;
    match v {
        EvalValue::Str(s) => Ok(s),
        other => Ok(format!("{}", other)),
    }
}

fn one_arg(fn_name: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.is_empty() {
        return Err(format!("json.{}() requiere al menos 1 argumento", fn_name));
    }
    Ok(args.into_iter().next().unwrap())
}
