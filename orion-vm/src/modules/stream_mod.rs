use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        //    Fuentes                                                              
        // from(list) → devuelve la lista tal cual (documenta intención de pipeline)
        "from" => {
            if args.is_empty() { return Err("stream.from requires (list)".into()); }
            match &args[0] {
                EvalValue::List(_) => Ok(args[0].clone()),
                _ => Err("stream.from: the argument must be a list".into()),
            }
        }
        // range(start, end, step?) → [start, start+step, ..., end-1]
        "range" => {
            if args.len() < 2 { return Err("stream.range requires (start, end, step?)".into()); }
            let start = args[0].to_i64()?;
            let end   = args[1].to_i64()?;
            let step  = if args.len() > 2 { args[2].to_i64()? } else { 1 };
            if step == 0 { return Err("stream.range: step cannot be 0".into()); }
            let mut out = Vec::new();
            let mut i = start;
            while if step > 0 { i < end } else { i > end } {
                out.push(EvalValue::Int(i));
                i += step;
            }
            Ok(EvalValue::List(out))
        }

        //    Filtros                                                               
        // where(s, key, val) / filter_eq → filtra dicts donde dict[key] == val
        "where" | "filter_eq" | "where_" => {
            if args.len() < 3 { return Err("stream.where requires (list, key, value)".into()); }
            let key = to_str(&args[1]);
            let target = &args[2];
            filter_list(&args[0], |item| {
                if let EvalValue::Dict(m) = item {
                    m.get(&key).map(|v| eval_eq(v, target)).unwrap_or(false)
                } else { false }
            })
        }
        // filter_gt(s, n) → mantiene números > n (o dicts donde el valor es > n no aplica)
        "filter_gt" => {
            if args.len() < 2 { return Err("stream.filter_gt requires (list, n)".into()); }
            let n = args[1].to_f64()?;
            filter_list(&args[0], |item| item.to_f64().map(|v| v > n).unwrap_or(false))
        }
        // filter_lt(s, n) → mantiene números < n
        "filter_lt" => {
            if args.len() < 2 { return Err("stream.filter_lt requires (list, n)".into()); }
            let n = args[1].to_f64()?;
            filter_list(&args[0], |item| item.to_f64().map(|v| v < n).unwrap_or(false))
        }
        // filter_gte(s, n)
        "filter_gte" => {
            if args.len() < 2 { return Err("stream.filter_gte requires (list, n)".into()); }
            let n = args[1].to_f64()?;
            filter_list(&args[0], |item| item.to_f64().map(|v| v >= n).unwrap_or(false))
        }
        // filter_lte(s, n)
        "filter_lte" => {
            if args.len() < 2 { return Err("stream.filter_lte requires (list, n)".into()); }
            let n = args[1].to_f64()?;
            filter_list(&args[0], |item| item.to_f64().map(|v| v <= n).unwrap_or(false))
        }

        //    Proyecciones                                                          
        // pluck(s, key) → extrae un campo de cada dict
        "pluck" => {
            if args.len() < 2 { return Err("stream.pluck requires (list, key)".into()); }
            let key = to_str(&args[1]);
            map_list(&args[0], |item| {
                if let EvalValue::Dict(m) = item {
                    m.get(&key).cloned().unwrap_or(EvalValue::Null)
                } else { EvalValue::Null }
            })
        }
        // keep(s, keys) → deja solo las claves indicadas en cada dict
        "keep" => {
            if args.len() < 2 { return Err("stream.keep requires (list, [keys])".into()); }
            let keys = match &args[1] {
                EvalValue::List(v) => v.iter().map(|k| to_str(k)).collect::<Vec<_>>(),
                _ => return Err("stream.keep: the second argument must be a list of strings".into()),
            };
            map_list(&args[0], |item| {
                if let EvalValue::Dict(m) = item {
                    let d = keys.iter()
                        .filter_map(|k| m.get(k).map(|v| (k.clone(), v.clone())))
                        .collect();
                    EvalValue::Dict(d)
                } else { item.clone() }
            })
        }

        //    Slicing                                                               
        // take(s, n) → primeros N elementos
        "take" => {
            if args.len() < 2 { return Err("stream.take requires (list, n)".into()); }
            let n = args[1].to_i64()? as usize;
            match &args[0] {
                EvalValue::List(v) => Ok(EvalValue::List(v.iter().take(n).cloned().collect())),
                _ => Err("stream.take: the first argument must be a list".into()),
            }
        }
        // skip(s, n) → descarta los primeros N
        "skip" => {
            if args.len() < 2 { return Err("stream.skip requires (list, n)".into()); }
            let n = args[1].to_i64()? as usize;
            match &args[0] {
                EvalValue::List(v) => Ok(EvalValue::List(v.iter().skip(n).cloned().collect())),
                _ => Err("stream.skip: the first argument must be a list".into()),
            }
        }

        //    Transformaciones                                                      
        // reverse(s) → lista invertida
        "reverse" => {
            if args.is_empty() { return Err("stream.reverse requires (list)".into()); }
            match &args[0] {
                EvalValue::List(v) => {
                    let mut r = v.clone();
                    r.reverse();
                    Ok(EvalValue::List(r))
                }
                _ => Err("stream.reverse: the argument must be a list".into()),
            }
        }
        // unique(s) → elimina duplicados (preserva orden de primera aparición)
        "unique" => {
            if args.is_empty() { return Err("stream.unique requires (list)".into()); }
            match &args[0] {
                EvalValue::List(v) => {
                    let mut seen = std::collections::HashSet::new();
                    let unique: Vec<EvalValue> = v.iter()
                        .filter(|item| seen.insert(format!("{}", item)))
                        .cloned()
                        .collect();
                    Ok(EvalValue::List(unique))
                }
                _ => Err("stream.unique: the argument must be a list".into()),
            }
        }
        // flatten(s) → aplana un nivel de listas anidadas
        "flatten" => {
            if args.is_empty() { return Err("stream.flatten requires (list)".into()); }
            match &args[0] {
                EvalValue::List(v) => {
                    let flat: Vec<EvalValue> = v.iter().flat_map(|item| {
                        if let EvalValue::List(inner) = item { inner.clone() }
                        else { vec![item.clone()] }
                    }).collect();
                    Ok(EvalValue::List(flat))
                }
                _ => Err("stream.flatten: the argument must be a list".into()),
            }
        }
        // zip_lists(s1, s2) → [{a: s1[0], b: s2[0]}, ...]
        "zip_lists" | "zip_" => {
            if args.len() < 2 { return Err("stream.zip_ requires (list1, list2)".into()); }
            match (&args[0], &args[1]) {
                (EvalValue::List(a), EvalValue::List(b)) => {
                    let zipped = a.iter().zip(b.iter()).map(|(x, y)| {
                        let mut m = HashMap::new();
                        m.insert("a".into(), x.clone());
                        m.insert("b".into(), y.clone());
                        EvalValue::Dict(m)
                    }).collect();
                    Ok(EvalValue::List(zipped))
                }
                _ => Err("stream.zip_: both arguments must be lists".into()),
            }
        }

        //    Agregaciones                                                          
        // collect(s) → materializa el stream (passthrough)
        "collect" => {
            if args.is_empty() { return Err("stream.collect requires (list)".into()); }
            Ok(args[0].clone())
        }
        "count" => {
            if args.is_empty() { return Err("stream.count requires (list)".into()); }
            match &args[0] {
                EvalValue::List(v) => Ok(EvalValue::Int(v.len() as i64)),
                _ => Err("stream.count: the argument must be a list".into()),
            }
        }
        "sum" => {
            if args.is_empty() { return Err("stream.sum requires (list)".into()); }
            numeric_fold(&args[0], 0.0, |acc, x| acc + x)
        }
        "avg" => {
            if args.is_empty() { return Err("stream.avg requires (list)".into()); }
            match &args[0] {
                EvalValue::List(v) if v.is_empty() => Ok(EvalValue::Null),
                EvalValue::List(v) => {
                    let sum: f64 = v.iter().filter_map(|x| x.to_f64().ok()).sum();
                    let cnt = v.iter().filter(|x| x.to_f64().is_ok()).count();
                    if cnt == 0 { Ok(EvalValue::Null) }
                    else { Ok(EvalValue::Float(sum / cnt as f64)) }
                }
                _ => Err("stream.avg: the argument must be a list".into()),
            }
        }
        "min" => {
            if args.is_empty() { return Err("stream.min requires (list)".into()); }
            match &args[0] {
                EvalValue::List(v) => {
                    v.iter().filter_map(|x| x.to_f64().ok())
                        .reduce(f64::min)
                        .map(wrap_number)
                        .ok_or_else(|| "stream.min: empty list, or no numbers in it".into())
                }
                _ => Err("stream.min: the argument must be a list".into()),
            }
        }
        "max" => {
            if args.is_empty() { return Err("stream.max requires (list)".into()); }
            match &args[0] {
                EvalValue::List(v) => {
                    v.iter().filter_map(|x| x.to_f64().ok())
                        .reduce(f64::max)
                        .map(wrap_number)
                        .ok_or_else(|| "stream.max: empty list, or no numbers in it".into())
                }
                _ => Err("stream.max: the argument must be a list".into()),
            }
        }

        f => Err(format!("stream.{}() does not exist", f)),
    }
}

//    Helpers                                                                   

fn filter_list<F>(val: &EvalValue, pred: F) -> Result<EvalValue, String>
where F: Fn(&EvalValue) -> bool
{
    match val {
        EvalValue::List(v) => Ok(EvalValue::List(v.iter().filter(|x| pred(x)).cloned().collect())),
        _ => Err("stream: the first argument must be a list".into()),
    }
}

fn map_list<F>(val: &EvalValue, f: F) -> Result<EvalValue, String>
where F: Fn(&EvalValue) -> EvalValue
{
    match val {
        EvalValue::List(v) => Ok(EvalValue::List(v.iter().map(|x| f(x)).collect())),
        _ => Err("stream: the first argument must be a list".into()),
    }
}

fn numeric_fold(val: &EvalValue, init: f64, f: fn(f64, f64) -> f64) -> Result<EvalValue, String> {
    match val {
        EvalValue::List(v) => {
            let result = v.iter().filter_map(|x| x.to_f64().ok()).fold(init, f);
            Ok(wrap_number(result))
        }
        _ => Err("stream: the argument must be a list".into()),
    }
}

fn wrap_number(f: f64) -> EvalValue {
    if f.fract() == 0.0 && f.abs() < 9e18 { EvalValue::Int(f as i64) }
    else { EvalValue::Float(f) }
}

fn eval_eq(a: &EvalValue, b: &EvalValue) -> bool {
    match (a, b) {
        (EvalValue::Int(x),   EvalValue::Int(y))   => x == y,
        (EvalValue::Float(x), EvalValue::Float(y)) => x == y,
        (EvalValue::Int(x),   EvalValue::Float(y)) => (*x as f64) == *y,
        (EvalValue::Float(x), EvalValue::Int(y))   => *x == (*y as f64),
        (EvalValue::Str(x),   EvalValue::Str(y))   => x == y,
        (EvalValue::Bool(x),  EvalValue::Bool(y))  => x == y,
        (EvalValue::Null,     EvalValue::Null)      => true,
        _ => false,
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
