use crate::eval_value::EvalValue;
use regex::Regex;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // is_match(text, pattern) → bool
        "is_match" => {
            let text    = str_arg("is_match", &args, 0)?;
            let pattern = str_arg("is_match", &args, 1)?;
            let re = compile(&pattern, "is_match")?;
            Ok(EvalValue::Bool(re.is_match(&text)))
        }

        // find(text, pattern) → string (primera coincidencia) o null
        "find" => {
            let text    = str_arg("find", &args, 0)?;
            let pattern = str_arg("find", &args, 1)?;
            let re = compile(&pattern, "find")?;
            match re.find(&text) {
                Some(m) => Ok(EvalValue::Str(m.as_str().to_string())),
                None    => Ok(EvalValue::Null),
            }
        }

        // find_all(text, pattern) → list of strings
        "find_all" => {
            let text    = str_arg("find_all", &args, 0)?;
            let pattern = str_arg("find_all", &args, 1)?;
            let re = compile(&pattern, "find_all")?;
            let matches: Vec<EvalValue> = re.find_iter(&text)
                .map(|m| EvalValue::Str(m.as_str().to_string()))
                .collect();
            Ok(EvalValue::List(matches))
        }

        // replace(text, pattern, replacement) → string
        // replace(text, pattern, replacement, count) → reemplaza solo N veces
        "replace" => {
            if args.len() < 3 {
                return Err("regex.replace requiere (texto, patron, reemplazo)".into());
            }
            let text        = str_arg("replace", &args, 0)?;
            let pattern     = str_arg("replace", &args, 1)?;
            let replacement = str_arg("replace", &args, 2)?;
            let re = compile(&pattern, "replace")?;

            match args.get(3) {
                Some(v) => {
                    let count = v.to_i64().map_err(|e| format!("regex.replace: {}", e))? as usize;
                    Ok(EvalValue::Str(re.replacen(&text, count, replacement.as_str()).to_string()))
                }
                None => Ok(EvalValue::Str(re.replace_all(&text, replacement.as_str()).to_string())),
            }
        }

        // split(text, pattern) → list of strings
        "split" => {
            let text    = str_arg("split", &args, 0)?;
            let pattern = str_arg("split", &args, 1)?;
            let re = compile(&pattern, "split")?;
            let parts: Vec<EvalValue> = re.split(&text)
                .map(|s| EvalValue::Str(s.to_string()))
                .collect();
            Ok(EvalValue::List(parts))
        }

        // groups(text, pattern) → list of strings (grupos de captura)
        "groups" => {
            let text    = str_arg("groups", &args, 0)?;
            let pattern = str_arg("groups", &args, 1)?;
            let re = compile(&pattern, "groups")?;
            match re.captures(&text) {
                None => Ok(EvalValue::Null),
                Some(caps) => {
                    let groups: Vec<EvalValue> = caps.iter()
                        .skip(1) // omite el match completo (grupo 0)
                        .map(|m| match m {
                            Some(m) => EvalValue::Str(m.as_str().to_string()),
                            None    => EvalValue::Null,
                        })
                        .collect();
                    Ok(EvalValue::List(groups))
                }
            }
        }

        // groups_all(text, pattern) → list of lists (todos los matches con sus grupos)
        "groups_all" => {
            let text    = str_arg("groups_all", &args, 0)?;
            let pattern = str_arg("groups_all", &args, 1)?;
            let re = compile(&pattern, "groups_all")?;
            let all: Vec<EvalValue> = re.captures_iter(&text)
                .map(|caps| {
                    let groups: Vec<EvalValue> = caps.iter()
                        .skip(1)
                        .map(|m| match m {
                            Some(m) => EvalValue::Str(m.as_str().to_string()),
                            None    => EvalValue::Null,
                        })
                        .collect();
                    EvalValue::List(groups)
                })
                .collect();
            Ok(EvalValue::List(all))
        }

        // named(text, pattern_with_named_groups) → dict { nombre → valor }
        "named" => {
            let text    = str_arg("named", &args, 0)?;
            let pattern = str_arg("named", &args, 1)?;
            let re = compile(&pattern, "named")?;
            match re.captures(&text) {
                None => Ok(EvalValue::Null),
                Some(caps) => {
                    let mut map = std::collections::HashMap::new();
                    for name in re.capture_names().flatten() {
                        let v = caps.name(name)
                            .map(|m| EvalValue::Str(m.as_str().to_string()))
                            .unwrap_or(EvalValue::Null);
                        map.insert(name.to_string(), v);
                    }
                    Ok(EvalValue::Dict(map))
                }
            }
        }

        // count(text, pattern) → int (número de coincidencias no superpuestas)
        "count" => {
            let text    = str_arg("count", &args, 0)?;
            let pattern = str_arg("count", &args, 1)?;
            let re = compile(&pattern, "count")?;
            Ok(EvalValue::Int(re.find_iter(&text).count() as i64))
        }

        // test(pattern) → bool (si el patrón es válido)
        "test" => {
            let pattern = str_arg("test", &args, 0)?;
            Ok(EvalValue::Bool(Regex::new(&pattern).is_ok()))
        }

        // escape(text) → string con caracteres especiales escapados
        "escape" => {
            let text = str_arg("escape", &args, 0)?;
            Ok(EvalValue::Str(regex::escape(&text)))
        }

        f => Err(format!("regex.{}: función no encontrada", f)),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn compile(pattern: &str, fn_name: &str) -> Result<Regex, String> {
    Regex::new(pattern)
        .map_err(|e| format!("regex.{}: patrón inválido '{}': {}", fn_name, pattern, e))
}

fn str_arg(fn_name: &str, args: &[EvalValue], idx: usize) -> Result<String, String> {
    match args.get(idx) {
        Some(EvalValue::Str(s)) => Ok(s.clone()),
        Some(other) => Ok(other.to_string()),
        None => Err(format!("regex.{}: argumento {} requerido", fn_name, idx + 1)),
    }
}
