use crate::eval_value::EvalValue;
use std::collections::HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // read(path) → list of dicts
        // read(path, delimiter) → list of dicts con delimitador custom
        "read" => {
            if args.is_empty() {
                return Err("csv.read requiere (path) o (path, delimiter)".into());
            }
            let path = str_arg("read", &args, 0)?;
            let delimiter = if args.len() >= 2 {
                let d = str_arg("read", &args, 1)?;
                d.chars().next().unwrap_or(',') as u8
            } else {
                b','
            };

            let mut rdr = csv::ReaderBuilder::new()
                .delimiter(delimiter)
                .has_headers(true)
                .flexible(true)
                .from_path(&path)
                .map_err(|e| format!("csv.read: no se pudo abrir '{}': {}", path, e))?;

            let headers: Vec<String> = rdr.headers()
                .map_err(|e| format!("csv.read: error leyendo cabeceras: {}", e))?
                .iter()
                .map(|s| s.trim().to_string())
                .collect();

            let mut rows = Vec::new();
            for result in rdr.records() {
                let record = result.map_err(|e| format!("csv.read: error en fila: {}", e))?;
                let mut map = HashMap::new();
                for (i, field) in record.iter().enumerate() {
                    let key = headers.get(i).cloned().unwrap_or_else(|| format!("col_{}", i));
                    map.insert(key, infer_value(field.trim()));
                }
                rows.push(EvalValue::Dict(map));
            }
            Ok(EvalValue::List(rows))
        }

        // read_raw(path) → list of lists (sin cabeceras como dict)
        "read_raw" => {
            let path = str_arg("read_raw", &args, 0)?;
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(false)
                .flexible(true)
                .from_path(&path)
                .map_err(|e| format!("csv.read_raw: {}", e))?;

            let mut rows = Vec::new();
            for result in rdr.records() {
                let record = result.map_err(|e| format!("csv.read_raw: {}", e))?;
                let row: Vec<EvalValue> = record.iter()
                    .map(|f| infer_value(f.trim()))
                    .collect();
                rows.push(EvalValue::List(row));
            }
            Ok(EvalValue::List(rows))
        }

        // write(path, list_of_dicts) → null
        // write(path, list_of_dicts, delimiter) → null
        "write" => {
            if args.len() < 2 {
                return Err("csv.write requiere (path, datos) o (path, datos, delimiter)".into());
            }
            let path = str_arg("write", &args, 0)?;
            let rows = list_arg("write", &args, 1)?;
            let delimiter = if args.len() >= 3 {
                let d = str_arg("write", &args, 2)?;
                d.chars().next().unwrap_or(',') as u8
            } else {
                b','
            };

            let mut wtr = csv::WriterBuilder::new()
                .delimiter(delimiter)
                .from_path(&path)
                .map_err(|e| format!("csv.write: no se pudo crear '{}': {}", path, e))?;

            // Extraer cabeceras del primer dict
            let headers = match rows.first() {
                Some(EvalValue::Dict(m)) => {
                    let mut h: Vec<String> = m.keys().cloned().collect();
                    h.sort();
                    h
                }
                Some(EvalValue::List(_)) => vec![],
                _ => return Err("csv.write: los datos deben ser una lista de dicts o listas".into()),
            };

            if !headers.is_empty() {
                wtr.write_record(&headers)
                    .map_err(|e| format!("csv.write: error escribiendo cabeceras: {}", e))?;
            }

            for row in &rows {
                match row {
                    EvalValue::Dict(m) => {
                        let record: Vec<String> = headers.iter()
                            .map(|k| eval_to_csv_str(m.get(k).unwrap_or(&EvalValue::Null)))
                            .collect();
                        wtr.write_record(&record)
                            .map_err(|e| format!("csv.write: error en fila: {}", e))?;
                    }
                    EvalValue::List(fields) => {
                        let record: Vec<String> = fields.iter().map(eval_to_csv_str).collect();
                        wtr.write_record(&record)
                            .map_err(|e| format!("csv.write: error en fila: {}", e))?;
                    }
                    _ => return Err("csv.write: cada fila debe ser un dict o lista".into()),
                }
            }
            wtr.flush().map_err(|e| format!("csv.write: {}", e))?;
            Ok(EvalValue::Null)
        }

        // headers(list_of_dicts) → list of strings
        "headers" => {
            let rows = list_arg("headers", &args, 0)?;
            match rows.first() {
                Some(EvalValue::Dict(m)) => {
                    let mut h: Vec<EvalValue> = m.keys()
                        .map(|k| EvalValue::Str(k.clone()))
                        .collect();
                    h.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
                    Ok(EvalValue::List(h))
                }
                _ => Ok(EvalValue::List(vec![])),
            }
        }

        // column(list_of_dicts, "col_name") → list of values
        "column" => {
            if args.len() < 2 {
                return Err("csv.column requiere (datos, columna)".into());
            }
            let rows = list_arg("column", &args, 0)?;
            let col = str_arg("column", &args, 1)?;
            let values: Vec<EvalValue> = rows.iter()
                .filter_map(|row| {
                    if let EvalValue::Dict(m) = row {
                        m.get(&col).cloned()
                    } else {
                        None
                    }
                })
                .collect();
            Ok(EvalValue::List(values))
        }

        // filter(list_of_dicts, "col", value) → list of dicts donde col == value
        "filter" => {
            if args.len() < 3 {
                return Err("csv.filter requiere (datos, columna, valor)".into());
            }
            let rows = list_arg("filter", &args, 0)?;
            let col = str_arg("filter", &args, 1)?;
            let target = args[2].clone();
            let filtered: Vec<EvalValue> = rows.into_iter()
                .filter(|row| {
                    if let EvalValue::Dict(m) = row {
                        m.get(&col).map(|v| eval_eq(v, &target)).unwrap_or(false)
                    } else {
                        false
                    }
                })
                .collect();
            Ok(EvalValue::List(filtered))
        }

        // select(list_of_dicts, ["col1", "col2"]) → list of dicts solo con esas columnas
        "select" => {
            if args.len() < 2 {
                return Err("csv.select requiere (datos, [columnas])".into());
            }
            let rows = list_arg("select", &args, 0)?;
            let cols_raw = list_arg("select", &args, 1)?;
            let cols: Vec<String> = cols_raw.iter()
                .map(|v| v.to_string())
                .collect();
            let result: Vec<EvalValue> = rows.into_iter()
                .map(|row| {
                    if let EvalValue::Dict(m) = row {
                        let mut new_map = HashMap::new();
                        for c in &cols {
                            if let Some(v) = m.get(c) {
                                new_map.insert(c.clone(), v.clone());
                            }
                        }
                        EvalValue::Dict(new_map)
                    } else {
                        row
                    }
                })
                .collect();
            Ok(EvalValue::List(result))
        }

        // sort(list_of_dicts, "col") → list ordenada ascendente
        // sort(list_of_dicts, "col", "desc") → descendente
        "sort" => {
            if args.len() < 2 {
                return Err("csv.sort requiere (datos, columna) o (datos, columna, \"desc\")".into());
            }
            let mut rows = list_arg("sort", &args, 0)?;
            let col = str_arg("sort", &args, 1)?;
            let desc = args.get(2)
                .and_then(|v| if let EvalValue::Str(s) = v { Some(s.as_str()) } else { None })
                .map(|s| s == "desc")
                .unwrap_or(false);

            rows.sort_by(|a, b| {
                let va = dict_get(a, &col);
                let vb = dict_get(b, &col);
                let ord = eval_cmp(&va, &vb);
                if desc { ord.reverse() } else { ord }
            });
            Ok(EvalValue::List(rows))
        }

        // group_by(list_of_dicts, "col") → dict { valor → list of dicts }
        "group_by" => {
            if args.len() < 2 {
                return Err("csv.group_by requiere (datos, columna)".into());
            }
            let rows = list_arg("group_by", &args, 0)?;
            let col = str_arg("group_by", &args, 1)?;
            let mut groups: HashMap<String, Vec<EvalValue>> = HashMap::new();
            for row in rows {
                let key = if let EvalValue::Dict(ref m) = row {
                    m.get(&col).map(|v| v.to_string()).unwrap_or_else(|| "null".into())
                } else {
                    "null".into()
                };
                groups.entry(key).or_default().push(row);
            }
            let result: HashMap<String, EvalValue> = groups.into_iter()
                .map(|(k, v)| (k, EvalValue::List(v)))
                .collect();
            Ok(EvalValue::Dict(result))
        }

        // count(list_of_dicts) → int
        "count" => {
            let rows = list_arg("count", &args, 0)?;
            Ok(EvalValue::Int(rows.len() as i64))
        }

        // stats(list_of_dicts, "col") → dict { min, max, sum, avg, std, median, p25, p75, count }
        "stats" => {
            if args.len() < 2 {
                return Err("csv.stats requiere (datos, columna)".into());
            }
            let rows = list_arg("stats", &args, 0)?;
            let col = str_arg("stats", &args, 1)?;
            let mut nums: Vec<f64> = Vec::new();
            for row in &rows {
                if let EvalValue::Dict(m) = row {
                    if let Some(v) = m.get(&col) {
                        if let Ok(n) = v.to_f64() {
                            nums.push(n);
                        }
                    }
                }
            }
            if nums.is_empty() {
                return Err(format!("csv.stats: columna '{}' no tiene valores numéricos", col));
            }
            let mut sorted = nums.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let n = sorted.len();
            let sum: f64 = sorted.iter().sum();
            let avg = sum / n as f64;
            let variance = sorted.iter().map(|x| (x - avg).powi(2)).sum::<f64>() / n as f64;
            let std = variance.sqrt();
            let percentile = |p: f64| -> f64 {
                let idx = (p / 100.0 * (n - 1) as f64) as usize;
                sorted[idx.min(n - 1)]
            };

            let mut result = HashMap::new();
            result.insert("min".into(),    EvalValue::Float(sorted[0]));
            result.insert("max".into(),    EvalValue::Float(sorted[n - 1]));
            result.insert("sum".into(),    EvalValue::Float(sum));
            result.insert("avg".into(),    EvalValue::Float(avg));
            result.insert("std".into(),    EvalValue::Float(std));
            result.insert("median".into(), EvalValue::Float(percentile(50.0)));
            result.insert("p25".into(),    EvalValue::Float(percentile(25.0)));
            result.insert("p75".into(),    EvalValue::Float(percentile(75.0)));
            result.insert("count".into(),  EvalValue::Int(n as i64));
            Ok(EvalValue::Dict(result))
        }

        // dedupe(list_of_dicts, "col") → list sin duplicados por columna
        "dedupe" => {
            if args.len() < 2 {
                return Err("csv.dedupe requiere (datos, columna)".into());
            }
            let rows = list_arg("dedupe", &args, 0)?;
            let col = str_arg("dedupe", &args, 1)?;
            let mut seen = std::collections::HashSet::new();
            let result: Vec<EvalValue> = rows.into_iter()
                .filter(|row| {
                    let key = if let EvalValue::Dict(m) = row {
                        m.get(&col).map(|v| v.to_string()).unwrap_or_default()
                    } else {
                        row.to_string()
                    };
                    seen.insert(key)
                })
                .collect();
            Ok(EvalValue::List(result))
        }

        // rename(list_of_dicts, "viejo", "nuevo") → list con columna renombrada
        "rename" => {
            if args.len() < 3 {
                return Err("csv.rename requiere (datos, viejo, nuevo)".into());
            }
            let rows = list_arg("rename", &args, 0)?;
            let old_col = str_arg("rename", &args, 1)?;
            let new_col = str_arg("rename", &args, 2)?;
            let result: Vec<EvalValue> = rows.into_iter()
                .map(|row| {
                    if let EvalValue::Dict(mut m) = row {
                        if let Some(v) = m.remove(&old_col) {
                            m.insert(new_col.clone(), v);
                        }
                        EvalValue::Dict(m)
                    } else {
                        row
                    }
                })
                .collect();
            Ok(EvalValue::List(result))
        }

        // slice(list, start, end) → subconjunto de filas
        "slice" => {
            if args.len() < 3 {
                return Err("csv.slice requiere (datos, inicio, fin)".into());
            }
            let rows = list_arg("slice", &args, 0)?;
            let start = int_arg("slice", &args, 1)? as usize;
            let end   = (int_arg("slice", &args, 2)? as usize).min(rows.len());
            Ok(EvalValue::List(rows[start.min(rows.len())..end].to_vec()))
        }

        // to_json(list_of_dicts) → string JSON
        "to_json" => {
            let rows = list_arg("to_json", &args, 0)?;
            let json_rows: Vec<serde_json::Value> = rows.iter().map(eval_to_json).collect();
            serde_json::to_string_pretty(&json_rows)
                .map(EvalValue::Str)
                .map_err(|e| format!("csv.to_json: {}", e))
        }

        f => Err(format!("csv.{}: función no encontrada", f)),
    }
}

//   Helpers                       

fn infer_value(s: &str) -> EvalValue {
    if let Ok(n) = s.parse::<i64>() { return EvalValue::Int(n); }
    if let Ok(f) = s.parse::<f64>() { return EvalValue::Float(f); }
    match s {
        "true" | "yes" | "si" | "sí" | "1" => return EvalValue::Bool(true),
        "false" | "no" | "0"                => return EvalValue::Bool(false),
        "null" | "NULL" | ""               => return EvalValue::Null,
        _ => {}
    }
    EvalValue::Str(s.to_string())
}

fn eval_to_csv_str(v: &EvalValue) -> String {
    match v {
        EvalValue::Null  => String::new(),
        EvalValue::Bool(b) => if *b { "true".into() } else { "false".into() },
        other => other.to_string(),
    }
}

fn eval_to_json(v: &EvalValue) -> serde_json::Value {
    match v {
        EvalValue::Int(n)    => serde_json::Value::Number((*n).into()),
        EvalValue::Float(f)  => serde_json::json!(f),
        EvalValue::Str(s)    => serde_json::Value::String(s.clone()),
        EvalValue::Bool(b)   => serde_json::Value::Bool(*b),
        EvalValue::Null      => serde_json::Value::Null,
        EvalValue::List(v)   => serde_json::Value::Array(v.iter().map(eval_to_json).collect()),
        EvalValue::Dict(m)   => {
            let obj: serde_json::Map<String, serde_json::Value> =
                m.iter().map(|(k, v)| (k.clone(), eval_to_json(v))).collect();
            serde_json::Value::Object(obj)
        }
        other => serde_json::Value::String(other.to_string()),
    }
}

fn eval_eq(a: &EvalValue, b: &EvalValue) -> bool {
    match (a, b) {
        (EvalValue::Int(x),   EvalValue::Int(y))   => x == y,
        (EvalValue::Float(x), EvalValue::Float(y)) => x == y,
        (EvalValue::Str(x),   EvalValue::Str(y))   => x == y,
        (EvalValue::Bool(x),  EvalValue::Bool(y))  => x == y,
        (EvalValue::Null,     EvalValue::Null)      => true,
        _ => a.to_string() == b.to_string(),
    }
}

fn eval_cmp(a: &EvalValue, b: &EvalValue) -> std::cmp::Ordering {
    match (a, b) {
        (EvalValue::Int(x),   EvalValue::Int(y))   => x.cmp(y),
        (EvalValue::Float(x), EvalValue::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
        (EvalValue::Int(x),   EvalValue::Float(y)) => (*x as f64).partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
        (EvalValue::Float(x), EvalValue::Int(y))   => x.partial_cmp(&(*y as f64)).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.to_string().cmp(&b.to_string()),
    }
}

fn dict_get(row: &EvalValue, col: &str) -> EvalValue {
    if let EvalValue::Dict(m) = row {
        m.get(col).cloned().unwrap_or(EvalValue::Null)
    } else {
        EvalValue::Null
    }
}

fn str_arg(fn_name: &str, args: &[EvalValue], idx: usize) -> Result<String, String> {
    match args.get(idx) {
        Some(EvalValue::Str(s)) => Ok(s.clone()),
        Some(other) => Ok(other.to_string()),
        None => Err(format!("csv.{}: argumento {} requerido", fn_name, idx + 1)),
    }
}

fn int_arg(fn_name: &str, args: &[EvalValue], idx: usize) -> Result<i64, String> {
    match args.get(idx) {
        Some(v) => v.to_i64().map_err(|e| format!("csv.{}: {}", fn_name, e)),
        None => Err(format!("csv.{}: argumento {} requerido", fn_name, idx + 1)),
    }
}

fn list_arg(fn_name: &str, args: &[EvalValue], idx: usize) -> Result<Vec<EvalValue>, String> {
    match args.get(idx) {
        Some(EvalValue::List(v)) => Ok(v.clone()),
        Some(other) => Err(format!("csv.{}: se esperaba lista, se recibió {}", fn_name, other.type_name())),
        None => Err(format!("csv.{}: argumento {} requerido", fn_name, idx + 1)),
    }
}
