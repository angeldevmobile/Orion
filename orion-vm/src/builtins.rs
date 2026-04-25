use crate::eval_value::EvalValue;

/// Llama una función built-in por nombre.
/// Retorna Ok(EvalValue) si el nombre existe, Err si no.
pub fn call_builtin(name: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match name {
        //   I/O                               
        "show" | "print" => {
            let parts: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
            println!("{}", parts.join(" "));
            Ok(EvalValue::Null)
        }

        //   Tipo / conversión                        
        "type" => {
            let v = one_arg(name, args)?;
            Ok(EvalValue::Str(v.type_name().to_string()))
        }
        "str" => {
            let v = one_arg(name, args)?;
            Ok(EvalValue::Str(format!("{}", v)))
        }
        "int" => {
            let v = one_arg(name, args)?;
            Ok(EvalValue::Int(v.to_i64()?))
        }
        "float" => {
            let v = one_arg(name, args)?;
            Ok(EvalValue::Float(v.to_f64()?))
        }
        "bool" => {
            let v = one_arg(name, args)?;
            Ok(EvalValue::Bool(v.is_truthy()))
        }

        //   Colecciones                           
        "len" => {
            let v = one_arg(name, args)?;
            let n = match &v {
                EvalValue::Str(s)  => s.chars().count() as i64,
                EvalValue::List(v) => v.len() as i64,
                EvalValue::Dict(m) => m.len() as i64,
                other => return Err(format!("len() no aplica a {}", other.type_name())),
            };
            Ok(EvalValue::Int(n))
        }
        "keys" => {
            let v = one_arg(name, args)?;
            match v {
                EvalValue::Dict(m) =>
                    Ok(EvalValue::List(m.into_keys().map(EvalValue::Str).collect())),
                other => Err(format!("keys() no aplica a {}", other.type_name())),
            }
        }
        "values" => {
            let v = one_arg(name, args)?;
            match v {
                EvalValue::Dict(m) =>
                    Ok(EvalValue::List(m.into_values().collect())),
                other => Err(format!("values() no aplica a {}", other.type_name())),
            }
        }
        "range" => {
            match args.len() {
                1 => {
                    let n = args[0].to_i64()?;
                    Ok(EvalValue::List((0..n).map(EvalValue::Int).collect()))
                }
                2 => {
                    let start = args[0].to_i64()?;
                    let end   = args[1].to_i64()?;
                    Ok(EvalValue::List((start..end).map(EvalValue::Int).collect()))
                }
                3 => {
                    let start = args[0].to_i64()?;
                    let end   = args[1].to_i64()?;
                    let step  = args[2].to_i64()?;
                    if step == 0 { return Err("range(): step no puede ser 0".into()); }
                    let mut v = Vec::new();
                    let mut i = start;
                    while (step > 0 && i < end) || (step < 0 && i > end) {
                        v.push(EvalValue::Int(i));
                        i += step;
                    }
                    Ok(EvalValue::List(v))
                }
                n => Err(format!("range() requiere 1-3 argumentos, recibió {}", n)),
            }
        }
        "list" => {
            let v = one_arg(name, args)?;
            match v {
                EvalValue::List(_) => Ok(v),
                EvalValue::Str(s)  =>
                    Ok(EvalValue::List(s.chars().map(|c| EvalValue::Str(c.to_string())).collect())),
                other => Err(format!("list() no puede convertir {}", other.type_name())),
            }
        }

        //   Matemáticas                           
        "abs" => {
            let v = one_arg(name, args)?;
            match v {
                EvalValue::Int(n)   => Ok(EvalValue::Int(n.abs())),
                EvalValue::Float(f) => Ok(EvalValue::Float(f.abs())),
                other => Err(format!("abs() no aplica a {}", other.type_name())),
            }
        }
        "max" => {
            let items = flatten_or_direct(args);
            if items.is_empty() { return Err("max(): lista vacía".into()); }
            let mut best: Option<EvalValue> = None;
            for v in items {
                best = Some(match best {
                    None      => v,
                    Some(cur) => if cmp_values(&v, &cur)? > 0 { v } else { cur },
                });
            }
            best.ok_or_else(|| "max(): sin resultado".into())
        }
        "min" => {
            let items = flatten_or_direct(args);
            if items.is_empty() { return Err("min(): lista vacía".into()); }
            let mut best: Option<EvalValue> = None;
            for v in items {
                best = Some(match best {
                    None      => v,
                    Some(cur) => if cmp_values(&v, &cur)? < 0 { v } else { cur },
                });
            }
            best.ok_or_else(|| "min(): sin resultado".into())
        }
        "sum" => {
            let items = flatten_or_direct(args);
            let mut total = 0.0_f64;
            let mut all_int = true;
            for item in &items {
                match item {
                    EvalValue::Int(n)   => total += *n as f64,
                    EvalValue::Float(f) => { total += f; all_int = false; }
                    other => return Err(format!("sum(): tipo {} no soportado", other.type_name())),
                }
            }
            if all_int { Ok(EvalValue::Int(total as i64)) }
            else       { Ok(EvalValue::Float(total)) }
        }
        "round" => {
            match args.len() {
                1 => {
                    let f = args[0].to_f64()?;
                    Ok(EvalValue::Int(f.round() as i64))
                }
                2 => {
                    let f      = args[0].to_f64()?;
                    let digits = args[1].to_i64()? as u32;
                    let factor = 10_f64.powi(digits as i32);
                    Ok(EvalValue::Float((f * factor).round() / factor))
                }
                n => Err(format!("round() requiere 1-2 argumentos, recibió {}", n)),
            }
        }

        //   Matemáticas extendidas
        "floor" => {
            let v = one_arg(name, args)?;
            Ok(EvalValue::Int(v.to_f64()?.floor() as i64))
        }
        "ceil" => {
            let v = one_arg(name, args)?;
            Ok(EvalValue::Int(v.to_f64()?.ceil() as i64))
        }
        "sqrt" => {
            let v = one_arg(name, args)?;
            let f = v.to_f64()?;
            if f < 0.0 { return Err("sqrt() de número negativo".into()); }
            Ok(EvalValue::Float(f.sqrt()))
        }
        "pow" => {
            if args.len() != 2 { return Err(format!("pow() requiere 2 argumentos, recibió {}", args.len())); }
            let base = args[0].to_f64()?;
            let exp  = args[1].to_f64()?;
            Ok(EvalValue::Float(base.powf(exp)))
        }

        //   I/O: input desde stdin
        "input" => {
            use std::io::{self, BufRead, Write};
            if let Some(prompt) = args.into_iter().next() {
                print!("{}", prompt);
                io::stdout().flush().ok();
            }
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line).map_err(|e| e.to_string())?;
            Ok(EvalValue::Str(line.trim_end_matches(['\n', '\r']).to_string()))
        }

        //   Strings
        "upper"   => { let v = one_arg(name, args)?; str_method(v, "upper") }
        "lower"   => { let v = one_arg(name, args)?; str_method(v, "lower") }
        "strip"   => { let v = one_arg(name, args)?; str_method(v, "strip") }
        "reverse" => {
            let v = one_arg(name, args)?;
            match v {
                EvalValue::Str(s)  =>
                    Ok(EvalValue::Str(s.chars().rev().collect())),
                EvalValue::List(mut v) => { v.reverse(); Ok(EvalValue::List(v)) }
                other => Err(format!("reverse() no aplica a {}", other.type_name())),
            }
        }

        _ => Err(format!("__not_found__:{}", name)),
    }
}

//   helpers                                 ─

fn one_arg(name: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() != 1 {
        Err(format!("{}() requiere 1 argumento, recibió {}", name, args.len()))
    } else {
        Ok(args.into_iter().next().unwrap())
    }
}

fn flatten_or_direct(mut args: Vec<EvalValue>) -> Vec<EvalValue> {
    if args.len() == 1 {
        let first = args.remove(0);
        if let EvalValue::List(v) = first {
            return v;
        } else {
            return vec![first];
        }
    }
    args
}

pub fn cmp_values(a: &EvalValue, b: &EvalValue) -> Result<i64, String> {
    match (a, b) {
        (EvalValue::Int(x),   EvalValue::Int(y))   => Ok(x.cmp(y) as i64),
        (EvalValue::Float(x), EvalValue::Float(y)) => Ok(x.partial_cmp(y).map(|o| o as i64).unwrap_or(0)),
        (EvalValue::Int(x),   EvalValue::Float(y)) => Ok((*x as f64).partial_cmp(y).map(|o| o as i64).unwrap_or(0)),
        (EvalValue::Float(x), EvalValue::Int(y))   => Ok(x.partial_cmp(&(*y as f64)).map(|o| o as i64).unwrap_or(0)),
        (EvalValue::Str(x),   EvalValue::Str(y))   => Ok(x.cmp(y) as i64),
        _ => Err(format!("No se puede comparar {} con {}", a.type_name(), b.type_name())),
    }
}

fn str_method(v: EvalValue, method: &str) -> Result<EvalValue, String> {
    match v {
        EvalValue::Str(s) => Ok(EvalValue::Str(match method {
            "upper" => s.to_uppercase(),
            "lower" => s.to_lowercase(),
            "strip" => s.trim().to_string(),
            _       => s,
        })),
        other => Err(format!("{}() no aplica a {}", method, other.type_name())),
    }
}

/// Retorna true si el nombre es una función built-in conocida.
pub fn is_builtin(name: &str) -> bool {
    matches!(name,
        "show" | "print" | "type" | "str" | "int" | "float" | "bool" |
        "len" | "keys" | "values" | "range" | "list" |
        "abs" | "max" | "min" | "sum" | "round" |
        "upper" | "lower" | "strip" | "reverse"
    )
}
