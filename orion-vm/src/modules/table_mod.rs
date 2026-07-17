use crate::eval_value::EvalValue;
use std::collections::HashMap;
use calamine::{Reader, open_workbook_auto, Data};
use rust_xlsxwriter::{Workbook, Format, Color};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {

        //    Carga                                                             

        // load(path) → table   auto-detecta .csv .xlsx .xls .ods .json
        "load" => {
            let path = str_arg("load", &args, 0)?;
            let ext = std::path::Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            match ext.as_str() {
                "csv" | "tsv" | "txt" => load_csv(&path),
                "xlsx" | "xls" | "ods" | "xlsm" => load_excel(&path, None),
                "json" => load_json(&path),
                _ => Err(format!("table.load: formato '{}' no reconocido", ext)),
            }
        }

        // load_sheet(path, sheet) → table   hoja específica de Excel
        "load_sheet" => {
            let path  = str_arg("load_sheet", &args, 0)?;
            let sheet = str_arg("load_sheet", &args, 1)?;
            load_excel(&path, Some(&sheet.clone()))
        }

        // from(list_of_dicts) → table   convierte lista existente en tabla
        "from" => {
            let list = list_arg("from", &args, 0)?;
            // valida que todos sean dicts
            for (i, row) in list.iter().enumerate() {
                if !matches!(row, EvalValue::Dict(_)) {
                    return Err(format!("table.from: elemento [{}] no es un dict", i));
                }
            }
            Ok(EvalValue::List(list))
        }

        // rows(table) → list_of_dicts   extrae las filas
        "rows" => {
            let t = list_arg("rows", &args, 0)?;
            Ok(EvalValue::List(t))
        }

        //    Exploración                                                       

        // peek(table) o peek(table, n) → null  imprime tabla bonita en consola
        "peek" => {
            let rows = list_arg("peek", &args, 0)?;
            let n = args.get(1).and_then(|v| v.to_i64().ok()).unwrap_or(10) as usize;
            pretty_print(&rows, n);
            Ok(EvalValue::Null)
        }

        // size(table) → dict { rows, cols }
        "size" => {
            let rows = list_arg("size", &args, 0)?;
            let cols = infer_headers(&rows).len();
            let mut m = HashMap::new();
            m.insert("rows".into(), EvalValue::Int(rows.len() as i64));
            m.insert("cols".into(), EvalValue::Int(cols as i64));
            Ok(EvalValue::Dict(m))
        }

        // schema(table) → null  imprime tipos + estadísticas por columna
        "schema" => {
            let rows = list_arg("schema", &args, 0)?;
            print_schema(&rows);
            Ok(EvalValue::Null)
        }

        // profile(table) → dict  estadísticas completas por columna (retorna valor)
        "profile" => {
            let rows = list_arg("profile", &args, 0)?;
            let headers = infer_headers(&rows);
            let mut profile = HashMap::new();
            for col in &headers {
                let vals: Vec<EvalValue> = column_values(&rows, col);
                let mut info = HashMap::new();
                let nulls = vals.iter().filter(|v| matches!(v, EvalValue::Null)).count();
                info.insert("total".into(),  EvalValue::Int(vals.len() as i64));
                info.insert("nulls".into(),  EvalValue::Int(nulls as i64));
                info.insert("filled".into(), EvalValue::Int((vals.len() - nulls) as i64));

                let non_null: Vec<&EvalValue> =
                    vals.iter().filter(|v| !matches!(v, EvalValue::Null)).collect();
                let nums: Vec<f64> = vals.iter().filter_map(|v| v.to_f64().ok()).collect();
                if !non_null.is_empty() && non_null.iter().all(|v| matches!(v, EvalValue::Bool(_))) {
                    let trues = non_null.iter().filter(|v| matches!(v, EvalValue::Bool(true))).count();
                    info.insert("type".into(),   EvalValue::Str("bool".into()));
                    info.insert("trues".into(),  EvalValue::Int(trues as i64));
                    info.insert("falses".into(), EvalValue::Int((non_null.len() - trues) as i64));
                } else if !nums.is_empty() {
                    let stats = compute_stats(&nums);
                    info.insert("type".into(),    EvalValue::Str("number".into()));
                    info.insert("min".into(),     EvalValue::Float(stats.min));
                    info.insert("max".into(),     EvalValue::Float(stats.max));
                    info.insert("avg".into(),     EvalValue::Float(stats.avg));
                    info.insert("std".into(),     EvalValue::Float(stats.std));
                    info.insert("p25".into(),     EvalValue::Float(stats.p25));
                    info.insert("median".into(),  EvalValue::Float(stats.median));
                    info.insert("p75".into(),     EvalValue::Float(stats.p75));
                } else {
                    let uniq: std::collections::HashSet<String> =
                        vals.iter().map(|v| v.to_string()).collect();
                    info.insert("type".into(),   EvalValue::Str("string".into()));
                    info.insert("unique".into(), EvalValue::Int(uniq.len() as i64));
                }
                profile.insert(col.clone(), EvalValue::Dict(info));
            }
            Ok(EvalValue::Dict(profile))
        }

        //    Selección de columnas                                             

        // keep(table, ["col1", "col2"]) → table con solo esas columnas
        "keep" => {
            let rows = list_arg("keep", &args, 0)?;
            let cols = str_list_arg("keep", &args, 1)?;
            let result = rows.into_iter().map(|row| {
                if let EvalValue::Dict(m) = row {
                    let mut new = HashMap::new();
                    for c in &cols { if let Some(v) = m.get(c) { new.insert(c.clone(), v.clone()); } }
                    EvalValue::Dict(new)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        // drop(table, ["col1", "col2"]) → table sin esas columnas
        "drop" => {
            let rows = list_arg("drop", &args, 0)?;
            let cols = str_list_arg("drop", &args, 1)?;
            let result = rows.into_iter().map(|row| {
                if let EvalValue::Dict(mut m) = row {
                    for c in &cols { m.remove(c); }
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        // rename(table, "viejo", "nuevo") → table con columna renombrada
        "rename" => {
            if args.len() < 3 { return Err("table.rename requiere (tabla, viejo, nuevo)".into()); }
            let rows  = list_arg("rename", &args, 0)?;
            let old   = str_arg("rename", &args, 1)?;
            let new   = str_arg("rename", &args, 2)?;
            let result = rows.into_iter().map(|row| {
                if let EvalValue::Dict(mut m) = row {
                    if let Some(v) = m.remove(&old) { m.insert(new.clone(), v); }
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        // cast(table, "col", "int"|"float"|"string"|"bool") → tabla con columna convertida
        "cast" => {
            if args.len() < 3 { return Err("table.cast requiere (tabla, columna, tipo)".into()); }
            let rows  = list_arg("cast", &args, 0)?;
            let col   = str_arg("cast", &args, 1)?;
            let to    = str_arg("cast", &args, 2)?;
            let result = rows.into_iter().map(|row| {
                if let EvalValue::Dict(mut m) = row {
                    if let Some(v) = m.get(&col).cloned() {
                        m.insert(col.clone(), cast_value(v, &to));
                    }
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        //    Filtros y ordenación                                              

        // where(table, condicion) → table filtrada
        // Soporta: comparadores (== != > >= < <= contains starts_with ends_with),
        // lógica (&& || !), paréntesis, aritmética, columna vs columna y funciones.
        // Ej: "(region == 'Norte' || region == 'Sur') && venta * 1.19 > meta"
        "where" => {
            if args.len() < 2 { return Err("table.where requiere (tabla, condicion)".into()); }
            let rows = list_arg("where", &args, 0)?;
            let cond = str_arg("where", &args, 1)?;
            let ast = parse_table_expr(&cond).map_err(|e| format!("table.where: {}", e))?;
            let result: Vec<EvalValue> = rows.into_iter()
                .filter(|row| {
                    if let EvalValue::Dict(m) = row {
                        eval_texpr(m, &ast).is_truthy()
                    } else { false }
                })
                .collect();
            Ok(EvalValue::List(result))
        }

        // sort(table, "col") o sort(table, "col", "desc") → table ordenada
        "sort" => {
            if args.len() < 2 { return Err("table.sort requiere (tabla, columna)".into()); }
            let mut rows = list_arg("sort", &args, 0)?;
            let col = str_arg("sort", &args, 1)?;
            let desc = args.get(2)
                .and_then(|v| if let EvalValue::Str(s) = v { Some(s == "desc") } else { None })
                .unwrap_or(false);
            rows.sort_by(|a, b| {
                let va = dict_get_val(a, &col);
                let vb = dict_get_val(b, &col);
                let ord = eval_ord(&va, &vb);
                if desc { ord.reverse() } else { ord }
            });
            Ok(EvalValue::List(rows))
        }

        // top(table, "col", n) → las n filas con mayor valor en col
        "top" => {
            if args.len() < 3 { return Err("table.top requiere (tabla, columna, n)".into()); }
            let mut rows = list_arg("top", &args, 0)?;
            let col = str_arg("top", &args, 1)?;
            let n   = int_arg("top", &args, 2)?.max(0) as usize;
            rows.sort_by(|a, b| eval_ord(&dict_get_val(b, &col), &dict_get_val(a, &col)));
            Ok(EvalValue::List(rows.into_iter().take(n).collect()))
        }

        // bottom(table, "col", n) → las n filas con menor valor en col
        "bottom" => {
            if args.len() < 3 { return Err("table.bottom requiere (tabla, columna, n)".into()); }
            let mut rows = list_arg("bottom", &args, 0)?;
            let col = str_arg("bottom", &args, 1)?;
            let n   = int_arg("bottom", &args, 2)?.max(0) as usize;
            rows.sort_by(|a, b| eval_ord(&dict_get_val(a, &col), &dict_get_val(b, &col)));
            Ok(EvalValue::List(rows.into_iter().take(n).collect()))
        }

        // sample(table, n) → n filas aleatorias sin reemplazo
        "sample" => {
            if args.len() < 2 { return Err("table.sample requiere (tabla, n)".into()); }
            let mut rows = list_arg("sample", &args, 0)?;
            let n = (int_arg("sample", &args, 1)?.max(0) as usize).min(rows.len());
            // Fisher-Yates parcial
            use rand::Rng;
            let mut rng = rand::thread_rng();
            for i in 0..n {
                let j = rng.gen_range(i..rows.len());
                rows.swap(i, j);
            }
            Ok(EvalValue::List(rows.into_iter().take(n).collect()))
        }

        // dedupe(table, "col") → sin duplicados por columna
        "dedupe" => {
            if args.len() < 2 { return Err("table.dedupe requiere (tabla, columna)".into()); }
            let rows = list_arg("dedupe", &args, 0)?;
            let col  = str_arg("dedupe", &args, 1)?;
            let mut seen = std::collections::HashSet::new();
            let result = rows.into_iter().filter(|row| {
                let key = if let EvalValue::Dict(m) = row {
                    m.get(&col).map(|v| v.to_string()).unwrap_or_default()
                } else { row.to_string() };
                seen.insert(key)
            }).collect();
            Ok(EvalValue::List(result))
        }

        //    Transformación                                                    

        // add(table, "nueva_col", "expresion") → table con columna calculada
        // Aritmética completa (+ - * / % con precedencia, paréntesis, negativos),
        // concatenación de texto con +, comparadores (producen columna booleana)
        // y funciones: upper lower trim len abs round floor ceil sqrt min max pow.
        // Ej: "round((venta - costo) / venta * 100, 2)", "upper(nombre) + ' (' + region + ')'"
        "add" => {
            if args.len() < 3 { return Err("table.add requiere (tabla, nombre_col, expresion)".into()); }
            let rows = list_arg("add", &args, 0)?;
            let col  = str_arg("add", &args, 1)?;
            let expr = str_arg("add", &args, 2)?;
            let ast = parse_table_expr(&expr).map_err(|e| format!("table.add: {}", e))?;
            let result = rows.into_iter().map(|row| {
                if let EvalValue::Dict(mut m) = row {
                    let val = eval_texpr(&m, &ast);
                    m.insert(col.clone(), val);
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        //    Agregación                                                        

        // group(table, "por_col", "valor_col", "sum"|"avg"|"count"|"min"|"max")
        // → list of dicts {by_col, result}
        "group" => {
            if args.len() < 4 { return Err("table.group requiere (tabla, por, valor, operacion)".into()); }
            let rows  = list_arg("group", &args, 0)?;
            let by    = str_arg("group", &args, 1)?;
            let val   = str_arg("group", &args, 2)?;
            let op    = str_arg("group", &args, 3)?;
            if !matches!(op.as_str(), "sum" | "avg" | "count" | "min" | "max") {
                return Err(format!("table.group: operación '{}' desconocida (usa sum|avg|count|min|max)", op));
            }

            let mut buckets: HashMap<String, Vec<f64>> = HashMap::new();
            let mut key_order: Vec<String> = Vec::new();

            for row in &rows {
                if let EvalValue::Dict(m) = row {
                    let key = m.get(&by).map(|v| v.to_string()).unwrap_or_default();
                    let num = m.get(&val).and_then(|v| v.to_f64().ok()).unwrap_or(0.0);
                    if !buckets.contains_key(&key) { key_order.push(key.clone()); }
                    buckets.entry(key).or_default().push(num);
                }
            }

            let result: Vec<EvalValue> = key_order.iter().map(|k| {
                let nums = &buckets[k];
                let agg_val = match op.as_str() {
                    "sum"   => nums.iter().sum(),
                    "avg"   => nums.iter().sum::<f64>() / nums.len() as f64,
                    "count" => nums.len() as f64,
                    "min"   => nums.iter().cloned().fold(f64::INFINITY, f64::min),
                    _       => nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                };
                let mut m = HashMap::new();
                m.insert(by.clone(),  EvalValue::Str(k.clone()));
                m.insert(val.clone(), smart_num(agg_val));
                EvalValue::Dict(m)
            }).collect();

            Ok(EvalValue::List(result))
        }

        // agg(table, "col", "sum"|"avg"|"count"|"min"|"max") → número
        // count cuenta valores no nulos de cualquier tipo; el resto opera sobre números.
        "agg" => {
            if args.len() < 3 { return Err("table.agg requiere (tabla, columna, operacion)".into()); }
            let rows = list_arg("agg", &args, 0)?;
            let col  = str_arg("agg", &args, 1)?;
            let op   = str_arg("agg", &args, 2)?;
            if op == "count" {
                let n = rows.iter().filter(|r| {
                    if let EvalValue::Dict(m) = r {
                        !matches!(m.get(&col).unwrap_or(&EvalValue::Null), EvalValue::Null)
                    } else { false }
                }).count();
                return Ok(EvalValue::Int(n as i64));
            }
            let nums: Vec<f64> = rows.iter()
                .filter_map(|r| if let EvalValue::Dict(m) = r { m.get(&col)?.to_f64().ok() } else { None })
                .collect();
            if nums.is_empty() { return Ok(EvalValue::Null); }
            let result = match op.as_str() {
                "sum"   => nums.iter().sum(),
                "avg"   => nums.iter().sum::<f64>() / nums.len() as f64,
                "min"   => nums.iter().cloned().fold(f64::INFINITY, f64::min),
                "max"   => nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                _ => return Err(format!("table.agg: operación '{}' desconocida (usa sum|avg|count|min|max)", op)),
            };
            Ok(smart_num(result))
        }

        // stats(table, "col") → dict completo: min/max/avg/std/p25/median/p75/count/nulls
        "stats" => {
            if args.len() < 2 { return Err("table.stats requiere (tabla, columna)".into()); }
            let rows = list_arg("stats", &args, 0)?;
            let col  = str_arg("stats", &args, 1)?;
            let all_vals = column_values(&rows, &col);
            let nulls = all_vals.iter().filter(|v| matches!(v, EvalValue::Null)).count();
            let nums: Vec<f64> = all_vals.iter().filter_map(|v| v.to_f64().ok()).collect();
            if nums.is_empty() {
                return Err(format!("table.stats: columna '{}' no tiene valores numéricos", col));
            }
            let s = compute_stats(&nums);
            let mut result = HashMap::new();
            result.insert("count".into(),  EvalValue::Int(nums.len() as i64));
            result.insert("nulls".into(),  EvalValue::Int(nulls as i64));
            result.insert("min".into(),    EvalValue::Float(s.min));
            result.insert("max".into(),    EvalValue::Float(s.max));
            result.insert("avg".into(),    EvalValue::Float(s.avg));
            result.insert("std".into(),    EvalValue::Float(s.std));
            result.insert("p25".into(),    EvalValue::Float(s.p25));
            result.insert("median".into(), EvalValue::Float(s.median));
            result.insert("p75".into(),    EvalValue::Float(s.p75));
            Ok(EvalValue::Dict(result))
        }

        // column(table, "col") → list de valores de esa columna
        "column" => {
            if args.len() < 2 { return Err("table.column requiere (tabla, columna)".into()); }
            let rows = list_arg("column", &args, 0)?;
            let col  = str_arg("column", &args, 1)?;
            Ok(EvalValue::List(column_values(&rows, &col)))
        }

        // count(table) o count(table, "condicion") → int
        "count" => {
            let rows = list_arg("count", &args, 0)?;
            if let Some(cond_arg) = args.get(1) {
                let cond = cond_arg.to_string();
                let ast = parse_table_expr(&cond).map_err(|e| format!("table.count: {}", e))?;
                let n = rows.iter().filter(|row| {
                    if let EvalValue::Dict(m) = row { eval_texpr(m, &ast).is_truthy() } else { false }
                }).count();
                Ok(EvalValue::Int(n as i64))
            } else {
                Ok(EvalValue::Int(rows.len() as i64))
            }
        }

        //    Combinación                                                       

        // join(table1, table2, "key_col") → inner join por columna clave
        // join(table1, table2, "key_col", "left") → conserva todas las filas de
        // table1 y rellena con null las columnas del lado derecho sin match.
        "join" => {
            if args.len() < 3 { return Err("table.join requiere (tabla1, tabla2, clave)".into()); }
            let left  = list_arg("join", &args, 0)?;
            let right = list_arg("join", &args, 1)?;
            let key   = str_arg("join", &args, 2)?;
            let mode  = args.get(3).map(|v| v.to_string()).unwrap_or_else(|| "inner".to_string());
            if mode != "inner" && mode != "left" {
                return Err(format!("table.join: modo '{}' desconocido (usa inner|left)", mode));
            }
            let right_cols: Vec<String> = infer_headers(&right)
                .into_iter().filter(|c| c != &key).collect();

            // Índice del lado derecho
            let mut right_index: HashMap<String, Vec<HashMap<String, EvalValue>>> = HashMap::new();
            for row in &right {
                if let EvalValue::Dict(m) = row {
                    let k = m.get(&key).map(|v| v.to_string()).unwrap_or_default();
                    right_index.entry(k).or_default().push(m.clone());
                }
            }

            let mut result = Vec::new();
            for row in &left {
                if let EvalValue::Dict(lm) = row {
                    let k = lm.get(&key).map(|v| v.to_string()).unwrap_or_default();
                    if let Some(right_rows) = right_index.get(&k) {
                        for rm in right_rows {
                            let mut merged = lm.clone();
                            for (rk, rv) in rm {
                                if rk != &key { merged.insert(rk.clone(), rv.clone()); }
                            }
                            result.push(EvalValue::Dict(merged));
                        }
                    } else if mode == "left" {
                        let mut merged = lm.clone();
                        for rc in &right_cols {
                            merged.entry(rc.clone()).or_insert(EvalValue::Null);
                        }
                        result.push(EvalValue::Dict(merged));
                    }
                }
            }
            Ok(EvalValue::List(result))
        }

        // concat(table1, table2) → apila las filas
        "concat" => {
            if args.len() < 2 { return Err("table.concat requiere (tabla1, tabla2)".into()); }
            let mut t1 = list_arg("concat", &args, 0)?;
            let t2     = list_arg("concat", &args, 1)?;
            t1.extend(t2);
            Ok(EvalValue::List(t1))
        }

        //    Analítica avanzada                                                

        // forecast(table, "col", n) → list de n valores futuros (regresión lineal)
        "forecast" => {
            if args.len() < 3 { return Err("table.forecast requiere (tabla, columna, n)".into()); }
            let rows = list_arg("forecast", &args, 0)?;
            let col  = str_arg("forecast", &args, 1)?;
            let n    = int_arg("forecast", &args, 2)? as usize;
            let nums: Vec<f64> = column_values(&rows, &col)
                .iter().filter_map(|v| v.to_f64().ok()).collect();
            if nums.len() < 2 {
                return Err("table.forecast: se necesitan al menos 2 valores para proyectar".into());
            }
            let predictions = linear_forecast(&nums, n);
            Ok(EvalValue::List(predictions.into_iter().map(EvalValue::Float).collect()))
        }

        // anomalies(table, "col") → list de dicts de las filas anómalas (IQR)
        "anomalies" => {
            if args.len() < 2 { return Err("table.anomalies requiere (tabla, columna)".into()); }
            let rows = list_arg("anomalies", &args, 0)?;
            let col  = str_arg("anomalies", &args, 1)?;
            let nums: Vec<f64> = column_values(&rows, &col)
                .iter().filter_map(|v| v.to_f64().ok()).collect();
            let flags = detect_anomalies_iqr(&nums);
            let mut num_idx = 0;
            let mut anomalous = Vec::new();
            for row in &rows {
                if let EvalValue::Dict(m) = row {
                    if m.get(&col).and_then(|v| v.to_f64().ok()).is_some() {
                        if flags.get(num_idx).copied().unwrap_or(false) {
                            anomalous.push(row.clone());
                        }
                        num_idx += 1;
                    }
                }
            }
            Ok(EvalValue::List(anomalous))
        }

        // anomalies_mark(table, "col") → table con columna "_anomaly" yes/no
        "anomalies_mark" => {
            if args.len() < 2 { return Err("table.anomalies_mark requiere (tabla, columna)".into()); }
            let rows = list_arg("anomalies_mark", &args, 0)?;
            let col  = str_arg("anomalies_mark", &args, 1)?;
            let nums: Vec<f64> = column_values(&rows, &col)
                .iter().filter_map(|v| v.to_f64().ok()).collect();
            let flags = detect_anomalies_iqr(&nums);
            let mut num_idx = 0;
            let result = rows.into_iter().map(|row| {
                if let EvalValue::Dict(mut m) = row {
                    let is_anom = if m.get(&col).and_then(|v| v.to_f64().ok()).is_some() {
                        let a = flags.get(num_idx).copied().unwrap_or(false);
                        num_idx += 1;
                        a
                    } else { false };
                    m.insert("_anomaly".into(), EvalValue::Bool(is_anom));
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        // correlate(table, "col1", "col2") → float coeficiente de Pearson (-1 a 1)
        "correlate" => {
            if args.len() < 3 { return Err("table.correlate requiere (tabla, col1, col2)".into()); }
            let rows = list_arg("correlate", &args, 0)?;
            let c1   = str_arg("correlate", &args, 1)?;
            let c2   = str_arg("correlate", &args, 2)?;
            let pairs: Vec<(f64, f64)> = rows.iter().filter_map(|row| {
                if let EvalValue::Dict(m) = row {
                    let a = m.get(&c1)?.to_f64().ok()?;
                    let b = m.get(&c2)?.to_f64().ok()?;
                    Some((a, b))
                } else { None }
            }).collect();
            if pairs.len() < 2 {
                return Err("table.correlate: se necesitan al menos 2 pares de valores".into());
            }
            let xs: Vec<f64> = pairs.iter().map(|p| p.0).collect();
            let ys: Vec<f64> = pairs.iter().map(|p| p.1).collect();
            Ok(EvalValue::Float(pearson_correlation(&xs, &ys)))
        }

        // rank(table, "col") → table con columna "_rank" (1=mayor) y "_pct" (percentil 0-1)
        "rank" => {
            if args.len() < 2 { return Err("table.rank requiere (tabla, columna)".into()); }
            let rows = list_arg("rank", &args, 0)?;
            let col  = str_arg("rank", &args, 1)?;

            // Extraer (índice, valor) y ordenar desc para ranking
            let mut indexed: Vec<(usize, f64)> = rows.iter().enumerate()
                .filter_map(|(i, row)| {
                    if let EvalValue::Dict(m) = row {
                        m.get(&col)?.to_f64().ok().map(|v| (i, v))
                    } else { None }
                }).collect();
            indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let total = indexed.len() as f64;
            let mut rank_map: HashMap<usize, (i64, f64)> = HashMap::new();
            for (rank, (orig_idx, _)) in indexed.iter().enumerate() {
                let pct = if total > 1.0 { 1.0 - rank as f64 / (total - 1.0) } else { 1.0 };
                rank_map.insert(*orig_idx, (rank as i64 + 1, pct));
            }

            let result = rows.into_iter().enumerate().map(|(i, row)| {
                if let EvalValue::Dict(mut m) = row {
                    if let Some((r, pct)) = rank_map.get(&i) {
                        m.insert("_rank".into(), EvalValue::Int(*r));
                        m.insert("_pct".into(),  EvalValue::Float(*pct));
                    }
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        // moving_avg(table, "col", window) → table con columna "_mavg"
        "moving_avg" => {
            if args.len() < 3 { return Err("table.moving_avg requiere (tabla, columna, ventana)".into()); }
            let rows   = list_arg("moving_avg", &args, 0)?;
            let col    = str_arg("moving_avg", &args, 1)?;
            let window = int_arg("moving_avg", &args, 2)? as usize;
            let nums: Vec<Option<f64>> = rows.iter().map(|row| {
                if let EvalValue::Dict(m) = row { m.get(&col)?.to_f64().ok() } else { None }
            }).collect();

            let result = rows.into_iter().enumerate().map(|(i, row)| {
                if let EvalValue::Dict(mut m) = row {
                    let mavg = if i + 1 >= window {
                        let slice: Vec<f64> = nums[i + 1 - window..=i]
                            .iter().filter_map(|v| *v).collect();
                        if slice.len() == window {
                            Some(slice.iter().sum::<f64>() / window as f64)
                        } else { None }
                    } else { None };
                    m.insert("_mavg".into(), mavg.map(EvalValue::Float).unwrap_or(EvalValue::Null));
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        // normalize(table, "col") → table con columna "_norm" (0-1, min-max)
        "normalize" => {
            if args.len() < 2 { return Err("table.normalize requiere (tabla, columna)".into()); }
            let rows = list_arg("normalize", &args, 0)?;
            let col  = str_arg("normalize", &args, 1)?;
            let nums: Vec<f64> = column_values(&rows, &col)
                .iter().filter_map(|v| v.to_f64().ok()).collect();
            if nums.is_empty() { return Ok(EvalValue::List(rows)); }
            let min = nums.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let range = max - min;

            let result = rows.into_iter().map(|row| {
                if let EvalValue::Dict(mut m) = row {
                    let norm = if let Some(v) = m.get(&col).and_then(|v| v.to_f64().ok()) {
                        if range.abs() < f64::EPSILON { EvalValue::Float(0.0) }
                        else { EvalValue::Float((v - min) / range) }
                    } else { EvalValue::Null };
                    m.insert("_norm".into(), norm);
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        //    Streaming para archivos enormes                                   

        // stream(path, condicion) o stream(path, condicion, limite)
        // → filas que cumplen la condición, sin cargar todo en RAM.
        // Auto-detecta el delimitador (coma, punto y coma o tab) igual que load.
        "stream" => {
            if args.len() < 2 { return Err("table.stream requiere (path, condicion)".into()); }
            let path = str_arg("stream", &args, 0)?;
            let cond = str_arg("stream", &args, 1)?;
            let limit = args.get(2).and_then(|v| v.to_i64().ok()).unwrap_or(i64::MAX) as usize;
            let ast = parse_table_expr(&cond).map_err(|e| format!("table.stream: {}", e))?;
            let delimiter = detect_delimiter(&path).map_err(|e| format!("table.stream: {}", e))?;

            let mut rdr = csv::ReaderBuilder::new()
                .delimiter(delimiter)
                .has_headers(true)
                .flexible(true)
                .from_path(&path)
                .map_err(|e| format!("table.stream: no se pudo abrir '{}': {}", path, e))?;

            let headers: Vec<String> = rdr.headers()
                .map_err(|e| format!("table.stream: {}", e))?
                .iter().map(|s| s.trim().to_string()).collect();

            let mut result = Vec::new();
            for record in rdr.records() {
                if result.len() >= limit { break; }
                let record = record.map_err(|e| format!("table.stream: {}", e))?;
                let mut m = HashMap::new();
                for (i, field) in record.iter().enumerate() {
                    let key = headers.get(i).cloned().unwrap_or_else(|| format!("col_{}", i));
                    m.insert(key, infer_csv_value(field.trim()));
                }
                if eval_texpr(&m, &ast).is_truthy() {
                    result.push(EvalValue::Dict(m));
                }
            }
            Ok(EvalValue::List(result))
        }

        //    IA nativa                                                         

        // describe_ai(table) → string descripción en lenguaje natural
        "describe_ai" => {
            let rows = list_arg("describe_ai", &args, 0)?;
            let summary = build_table_summary(&rows);
            let prompt = format!(
                "Analiza esta tabla de datos y describe en 2-3 oraciones qué contiene, \
                 cuáles son los patrones más importantes y qué insights clave se pueden extraer.\n\n{}",
                summary
            );
            ai_call(&prompt)
        }

        // ask(table, "pregunta") → string respuesta IA sobre los datos
        "ask" => {
            if args.len() < 2 { return Err("table.ask requiere (tabla, pregunta)".into()); }
            let rows      = list_arg("ask", &args, 0)?;
            let question  = str_arg("ask", &args, 1)?;
            let summary   = build_table_summary(&rows);
            let prompt = format!(
                "Tienes acceso a esta tabla de datos:\n\n{}\n\nPregunta: {}\n\n\
                 Responde de forma concisa y basándote solo en los datos disponibles.",
                summary, question
            );
            ai_call(&prompt)
        }

        // suggest(table) → list de strings con sugerencias de análisis
        "suggest" => {
            let rows    = list_arg("suggest", &args, 0)?;
            let summary = build_table_summary(&rows);
            let prompt = format!(
                "Dado este resumen de una tabla de datos:\n\n{}\n\n\
                 Sugiere 5 análisis o visualizaciones concretas que serían más valiosas. \
                 Responde como una lista numerada, cada ítem en una línea.",
                summary
            );
            let response = ai_call(&prompt)?;
            if let EvalValue::Str(s) = response {
                let items: Vec<EvalValue> = s.lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .map(EvalValue::Str)
                    .collect();
                Ok(EvalValue::List(items))
            } else {
                Ok(EvalValue::Null)
            }
        }

        //    Exportación                                                       

        // save(table, path) → null   auto-detecta formato por extensión
        "save" => {
            if args.len() < 2 { return Err("table.save requiere (tabla, path)".into()); }
            let rows = list_arg("save", &args, 0)?;
            let path = str_arg("save", &args, 1)?;
            let ext  = std::path::Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            match ext.as_str() {
                "csv" | "tsv" => save_csv(&rows, &path),
                "xlsx"        => save_excel(&rows, &path),
                "json"        => save_json(&rows, &path),
                _ => Err(format!("table.save: formato '{}' no soportado", ext)),
            }
        }

        f => Err(format!("table.{}: función no encontrada", f)),
    }
}

//    Loaders                                                                   

// Lee SOLO la primera línea para detectar el separador (no carga el archivo entero)
fn detect_delimiter(path: &str) -> Result<u8, String> {
    use std::io::BufRead;
    let f = std::fs::File::open(path)
        .map_err(|e| format!("no se pudo abrir '{}': {}", path, e))?;
    let mut first = String::new();
    std::io::BufReader::new(f).read_line(&mut first)
        .map_err(|e| format!("no se pudo leer '{}': {}", path, e))?;
    Ok(if first.contains('\t') { b'\t' }
       else if first.contains(';') { b';' }
       else { b',' })
}

fn load_csv(path: &str) -> Result<EvalValue, String> {
    let delimiter = detect_delimiter(path).map_err(|e| format!("table.load: {}", e))?;

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| format!("table.load: {}", e))?;

    let headers: Vec<String> = rdr.headers()
        .map_err(|e| format!("table.load: {}", e))?
        .iter().map(|s| s.trim().to_string()).collect();

    let mut rows = Vec::new();
    for record in rdr.records() {
        let record = record.map_err(|e| format!("table.load: {}", e))?;
        let mut m = HashMap::new();
        for (i, field) in record.iter().enumerate() {
            let key = headers.get(i).cloned().unwrap_or_else(|| format!("col_{}", i));
            m.insert(key, infer_csv_value(field.trim()));
        }
        rows.push(EvalValue::Dict(m));
    }
    Ok(EvalValue::List(rows))
}

fn load_excel(path: &str, sheet: Option<&str>) -> Result<EvalValue, String> {
    let mut wb: calamine::Sheets<std::io::BufReader<std::fs::File>> =
        open_workbook_auto(path).map_err(|e| format!("table.load: {}", e))?;

    let target = match sheet {
        Some(s) => s.to_string(),
        None => wb.sheet_names().first().cloned()
            .ok_or("table.load: el archivo no tiene hojas")?,
    };

    let range = wb.worksheet_range(&target)
        .map_err(|e| format!("table.load: hoja '{}': {}", target, e))?;

    let mut iter = range.rows();
    let headers: Vec<String> = match iter.next() {
        Some(row) => row.iter().map(|c| excel_cell_str(c)).collect(),
        None => return Ok(EvalValue::List(vec![])),
    };

    let rows = iter.map(|row| {
        let mut m = HashMap::new();
        for (i, cell) in row.iter().enumerate() {
            let key = headers.get(i).cloned().unwrap_or_else(|| format!("col_{}", i));
            m.insert(key, excel_cell_eval(cell));
        }
        EvalValue::Dict(m)
    }).collect();

    Ok(EvalValue::List(rows))
}

fn load_json(path: &str) -> Result<EvalValue, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("table.load: {}", e))?;
    let json: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("table.load: JSON inválido: {}", e))?;
    match json {
        serde_json::Value::Array(arr) => {
            let rows = arr.into_iter().map(json_to_eval).collect();
            Ok(EvalValue::List(rows))
        }
        _ => Err("table.load: el JSON debe ser una lista de objetos".into()),
    }
}

//    Savers                                                                    

fn save_csv(rows: &[EvalValue], path: &str) -> Result<EvalValue, String> {
    let headers = infer_headers(rows);
    let mut wtr = csv::WriterBuilder::new()
        .from_path(path)
        .map_err(|e| format!("table.save: {}", e))?;
    if !headers.is_empty() {
        wtr.write_record(&headers).map_err(|e| format!("table.save: {}", e))?;
    }
    for row in rows {
        if let EvalValue::Dict(m) = row {
            let record: Vec<String> = headers.iter()
                .map(|k| eval_to_str(m.get(k).unwrap_or(&EvalValue::Null)))
                .collect();
            wtr.write_record(&record).map_err(|e| format!("table.save: {}", e))?;
        }
    }
    wtr.flush().map_err(|e| format!("table.save: {}", e))?;
    Ok(EvalValue::Null)
}

fn save_excel(rows: &[EvalValue], path: &str) -> Result<EvalValue, String> {
    let headers = infer_headers(rows);
    let mut wb = Workbook::new();
    {
        let ws = wb.add_worksheet();
        let hfmt = Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0x1A3A5C))
            .set_font_color(Color::White);

        for (c, h) in headers.iter().enumerate() {
            ws.write_with_format(0, c as u16, h.as_str(), &hfmt)
                .map_err(|e| format!("table.save: {}", e))?;
        }
        for (r, row) in rows.iter().enumerate() {
            if let EvalValue::Dict(m) = row {
                for (c, k) in headers.iter().enumerate() {
                    let v = m.get(k).unwrap_or(&EvalValue::Null);
                    let res = match v {
                        EvalValue::Int(n)   => ws.write(r as u32 + 1, c as u16, *n),
                        EvalValue::Float(f) => ws.write(r as u32 + 1, c as u16, *f),
                        EvalValue::Bool(b)  => ws.write(r as u32 + 1, c as u16, *b),
                        EvalValue::Null     => ws.write(r as u32 + 1, c as u16, ""),
                        other               => ws.write(r as u32 + 1, c as u16, other.to_string().as_str()),
                    };
                    res.map(|_| ()).map_err(|e| format!("table.save: {}", e))?;
                }
            }
        }
    }
    wb.save(path).map_err(|e| format!("table.save: {}", e))?;
    Ok(EvalValue::Null)
}

fn save_json(rows: &[EvalValue], path: &str) -> Result<EvalValue, String> {
    let json: Vec<serde_json::Value> = rows.iter().map(eval_to_json).collect();
    let s = serde_json::to_string_pretty(&json)
        .map_err(|e| format!("table.save: {}", e))?;
    std::fs::write(path, s).map_err(|e| format!("table.save: {}", e))?;
    Ok(EvalValue::Null)
}

//    Display bonito                                                             

fn pretty_print(rows: &[EvalValue], n: usize) {
    if rows.is_empty() { println!("  tabla vacía"); return; }
    let headers = infer_headers(rows);
    let show = rows.iter().take(n).collect::<Vec<_>>();

    // Calcular anchos de columna
    let col_widths: Vec<usize> = headers.iter().enumerate().map(|(_i, h)| {
        let max_data = show.iter().map(|row| {
            if let EvalValue::Dict(m) = row {
                m.get(h).map(|v| v.to_string().len()).unwrap_or(0)
            } else { 0 }
        }).max().unwrap_or(0);
        h.len().max(max_data).min(24)
    }).collect();

    let total_w: usize = col_widths.iter().sum::<usize>() + col_widths.len() * 3 + 1;

    // Cabecera
    println!();
    println!("  ┌{: <width$}┐", "", width = total_w - 2);
    println!("  │ {:width$} │",
        format!("tabla  ·  {} filas  ×  {} columnas", rows.len(), headers.len()),
        width = total_w - 4);
    println!("  ├{}┤", headers.iter().enumerate()
        .map(|(i, _)| format!("{: <w$}", "", w = col_widths[i] + 2))
        .collect::<Vec<_>>().join("┬"));

    // Cabeceras
    let header_row: String = headers.iter().enumerate()
        .map(|(i, h)| format!(" {:<w$} ", trunc(h, col_widths[i]), w = col_widths[i]))
        .collect::<Vec<_>>().join("│");
    println!("  │{}│", header_row);

    println!("  ├{}┤", headers.iter().enumerate()
        .map(|(i, _)| format!("{: <w$}", "", w = col_widths[i] + 2))
        .collect::<Vec<_>>().join("┼"));

    // Datos
    for row in &show {
        if let EvalValue::Dict(m) = row {
            let data_row: String = headers.iter().enumerate()
                .map(|(i, h)| {
                    let val = m.get(h).map(|v| v.to_string()).unwrap_or_default();
                    format!(" {:<w$} ", trunc(&val, col_widths[i]), w = col_widths[i])
                })
                .collect::<Vec<_>>().join("│");
            println!("  │{}│", data_row);
        }
    }

    if rows.len() > n {
        println!("  │ {:width$} │",
            format!("... {} filas más", rows.len() - n),
            width = total_w - 4);
    }
    println!("  └{}┘", headers.iter().enumerate()
        .map(|(i, _)| format!("{: <w$}", "", w = col_widths[i] + 2))
        .collect::<Vec<_>>().join("┴"));
    println!();
}

fn print_schema(rows: &[EvalValue]) {
    if rows.is_empty() { println!("  tabla vacía"); return; }
    let headers = infer_headers(rows);

    println!();
    println!("  ┌{: <14}┬{: <10}┬{: <8}┬{: <8}┬{: <26}┐", "", "", "", "", "");
    println!("  │ {:<12} │ {:<8} │ {:<6} │ {:<6} │ {:<24} │",
        "columna", "tipo", "nulos", "únicos", "muestra");
    println!("  ├{: <14}┼{: <10}┼{: <8}┼{: <8}┼{: <26}┤", "", "", "", "", "");

    for col in &headers {
        let vals = column_values(rows, col);
        let nulls = vals.iter().filter(|v| matches!(v, EvalValue::Null)).count();
        let uniq: std::collections::HashSet<String> =
            vals.iter().filter(|v| !matches!(v, EvalValue::Null))
                .map(|v| v.to_string()).collect();
        let non_null_count = vals.len() - nulls;
        let all_bool = non_null_count > 0 &&
            vals.iter().all(|v| matches!(v, EvalValue::Bool(_) | EvalValue::Null));
        let nums: Vec<f64> = vals.iter().filter_map(|v| v.to_f64().ok()).collect();
        let tipo = if all_bool { "bool" }
                   else if nums.len() > vals.len() / 2 { "number" }
                   else { "string" };
        let sample: String = vals.iter()
            .filter(|v| !matches!(v, EvalValue::Null))
            .take(3)
            .map(|v| match v {
                EvalValue::Str(s) => format!("\"{}\"", trunc(s, 8)),
                other => trunc(&other.to_string(), 8).to_string(),
            })
            .collect::<Vec<_>>().join(", ");

        println!("  │ {:<12} │ {:<8} │ {:<6} │ {:<6} │ {:<24} │",
            trunc(col, 12), tipo, nulls, uniq.len(), trunc(&sample, 24));
    }
    println!("  └{: <14}┴{: <10}┴{: <8}┴{: <8}┴{: <26}┘", "", "", "", "", "");
    println!();
}

//    Motor de expresiones (where / add / count / stream)
//
//    Gramática única para condiciones y columnas calculadas; se parsea UNA vez
//    por llamada y se evalúa por fila. Precedencia de menor a mayor:
//      or      := and ('||' and)*
//      and     := not ('&&' not)*
//      not     := '!' not | cmp
//      cmp     := sum (('=='|'!='|'>='|'<='|'>'|'<'|contains|starts_with|ends_with) sum)?
//      sum     := term (('+'|'-') term)*
//      term    := factor (('*'|'/'|'%') factor)*
//      factor  := '-' factor | primary
//      primary := número | 'texto' | true/yes | false/no | null | columna
//              | función '(' expr (',' expr)* ')' | '(' or ')'

#[derive(Debug, Clone)]
enum Tok {
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),
    Sym(&'static str),
}

// (nombre, mínimo de argumentos, máximo de argumentos)
const EXPR_FNS: &[(&str, usize, usize)] = &[
    ("upper", 1, 1), ("lower", 1, 1), ("trim", 1, 1), ("len", 1, 1),
    ("abs", 1, 1), ("round", 1, 2), ("floor", 1, 1), ("ceil", 1, 1),
    ("sqrt", 1, 1), ("min", 1, usize::MAX), ("max", 1, usize::MAX), ("pow", 2, 2),
];

fn tokenize_expr(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() { i += 1; continue; }
        if c == '\'' || c == '"' {
            let quote = c;
            let mut s = String::new();
            i += 1;
            while i < chars.len() && chars[i] != quote { s.push(chars[i]); i += 1; }
            if i >= chars.len() { return Err(format!("literal de texto sin cerrar: {}{}", quote, s)); }
            i += 1;
            toks.push(Tok::Str(s));
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            let mut is_float = false;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                if chars[i] == '.' {
                    if is_float { break; }
                    is_float = true;
                }
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            if is_float {
                let f = text.parse::<f64>().map_err(|_| format!("número inválido '{}'", text))?;
                toks.push(Tok::Float(f));
            } else {
                let n = text.parse::<i64>().map_err(|_| format!("número inválido '{}'", text))?;
                toks.push(Tok::Int(n));
            }
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
            toks.push(Tok::Ident(chars[start..i].iter().collect()));
            continue;
        }
        if i + 1 < chars.len() {
            let two: String = chars[i..i+2].iter().collect();
            let sym2 = match two.as_str() {
                "&&" => Some("&&"), "||" => Some("||"), "==" => Some("=="),
                "!=" => Some("!="), ">=" => Some(">="), "<=" => Some("<="),
                _ => None,
            };
            if let Some(s) = sym2 { toks.push(Tok::Sym(s)); i += 2; continue; }
        }
        let sym1 = match c {
            '>' => ">", '<' => "<", '!' => "!", '+' => "+", '-' => "-",
            '*' => "*", '/' => "/", '%' => "%", '(' => "(", ')' => ")", ',' => ",",
            _ => return Err(format!("carácter inesperado '{}'", c)),
        };
        toks.push(Tok::Sym(sym1));
        i += 1;
    }
    Ok(toks)
}

#[derive(Debug, Clone)]
enum TExpr {
    Lit(EvalValue),
    Col(String),
    Neg(Box<TExpr>),
    Not(Box<TExpr>),
    Bin(&'static str, Box<TExpr>, Box<TExpr>),
    Call(&'static str, Vec<TExpr>),
}

struct ExprParser { toks: Vec<Tok>, pos: usize }

impl ExprParser {
    fn peek(&self) -> Option<&Tok> { self.toks.get(self.pos) }

    fn eat_sym(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Sym(x)) if *x == s) { self.pos += 1; true } else { false }
    }

    fn parse_or(&mut self) -> Result<TExpr, String> {
        let mut left = self.parse_and()?;
        while self.eat_sym("||") {
            let right = self.parse_and()?;
            left = TExpr::Bin("||", Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<TExpr, String> {
        let mut left = self.parse_not()?;
        while self.eat_sym("&&") {
            let right = self.parse_not()?;
            left = TExpr::Bin("&&", Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<TExpr, String> {
        if self.eat_sym("!") {
            return Ok(TExpr::Not(Box::new(self.parse_not()?)));
        }
        self.parse_cmp()
    }

    fn parse_cmp(&mut self) -> Result<TExpr, String> {
        let left = self.parse_sum()?;
        let op: Option<&'static str> = match self.peek() {
            Some(Tok::Sym(s)) if ["==", "!=", ">=", "<=", ">", "<"].contains(s) => Some(*s),
            Some(Tok::Ident(w)) if w.as_str() == "contains"    => Some("contains"),
            Some(Tok::Ident(w)) if w.as_str() == "starts_with" => Some("starts_with"),
            Some(Tok::Ident(w)) if w.as_str() == "ends_with"   => Some("ends_with"),
            _ => None,
        };
        if let Some(op) = op {
            self.pos += 1;
            let right = self.parse_sum()?;
            return Ok(TExpr::Bin(op, Box::new(left), Box::new(right)));
        }
        Ok(left)
    }

    fn parse_sum(&mut self) -> Result<TExpr, String> {
        let mut left = self.parse_term()?;
        loop {
            let op = if self.eat_sym("+") { "+" }
                     else if self.eat_sym("-") { "-" }
                     else { break };
            let right = self.parse_term()?;
            left = TExpr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<TExpr, String> {
        let mut left = self.parse_factor()?;
        loop {
            let op = if self.eat_sym("*") { "*" }
                     else if self.eat_sym("/") { "/" }
                     else if self.eat_sym("%") { "%" }
                     else { break };
            let right = self.parse_factor()?;
            left = TExpr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_factor(&mut self) -> Result<TExpr, String> {
        if self.eat_sym("-") {
            return Ok(TExpr::Neg(Box::new(self.parse_factor()?)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<TExpr, String> {
        match self.toks.get(self.pos).cloned() {
            Some(Tok::Int(n))   => { self.pos += 1; Ok(TExpr::Lit(EvalValue::Int(n))) }
            Some(Tok::Float(f)) => { self.pos += 1; Ok(TExpr::Lit(EvalValue::Float(f))) }
            Some(Tok::Str(s))   => { self.pos += 1; Ok(TExpr::Lit(EvalValue::Str(s))) }
            Some(Tok::Sym("(")) => {
                self.pos += 1;
                let e = self.parse_or()?;
                if !self.eat_sym(")") { return Err("falta ')' de cierre".into()); }
                Ok(e)
            }
            Some(Tok::Ident(w)) => {
                self.pos += 1;
                match w.as_str() {
                    "true" | "yes" => return Ok(TExpr::Lit(EvalValue::Bool(true))),
                    "false" | "no" => return Ok(TExpr::Lit(EvalValue::Bool(false))),
                    "null"         => return Ok(TExpr::Lit(EvalValue::Null)),
                    _ => {}
                }
                if self.eat_sym("(") {
                    let spec = EXPR_FNS.iter().find(|(name, _, _)| *name == w.as_str());
                    let (name, min_a, max_a) = match spec {
                        Some(s) => *s,
                        None => return Err(format!(
                            "función desconocida '{}'. Disponibles: {}",
                            w, EXPR_FNS.iter().map(|(n, _, _)| *n).collect::<Vec<_>>().join(", ")
                        )),
                    };
                    let mut fn_args = Vec::new();
                    if !self.eat_sym(")") {
                        loop {
                            fn_args.push(self.parse_or()?);
                            if self.eat_sym(",") { continue; }
                            if self.eat_sym(")") { break; }
                            return Err(format!("falta ')' en la llamada a {}", name));
                        }
                    }
                    if fn_args.len() < min_a || fn_args.len() > max_a {
                        return Err(format!("{}: número de argumentos inválido ({})", name, fn_args.len()));
                    }
                    return Ok(TExpr::Call(name, fn_args));
                }
                Ok(TExpr::Col(w))
            }
            Some(other) => Err(format!("token inesperado: {:?}", other)),
            None => Err("expresión incompleta".into()),
        }
    }
}

fn parse_table_expr(src: &str) -> Result<TExpr, String> {
    let toks = tokenize_expr(src)?;
    if toks.is_empty() { return Err("expresión vacía".into()); }
    let mut p = ExprParser { toks, pos: 0 };
    let e = p.parse_or()?;
    if p.pos < p.toks.len() {
        return Err(format!("expresión inválida cerca de {:?}", p.toks[p.pos]));
    }
    Ok(e)
}

fn eval_texpr(row: &HashMap<String, EvalValue>, e: &TExpr) -> EvalValue {
    match e {
        TExpr::Lit(v) => v.clone(),
        TExpr::Col(name) => row.get(name).cloned().unwrap_or(EvalValue::Null),
        TExpr::Neg(x) => match eval_texpr(row, x) {
            EvalValue::Int(n)   => EvalValue::Int(-n),
            EvalValue::Float(f) => EvalValue::Float(-f),
            other => match other.to_f64() {
                Ok(f) => EvalValue::Float(-f),
                Err(_) => EvalValue::Null,
            },
        },
        TExpr::Not(x) => EvalValue::Bool(!eval_texpr(row, x).is_truthy()),
        TExpr::Bin("&&", l, r) => {
            if !eval_texpr(row, l).is_truthy() { return EvalValue::Bool(false); }
            EvalValue::Bool(eval_texpr(row, r).is_truthy())
        }
        TExpr::Bin("||", l, r) => {
            if eval_texpr(row, l).is_truthy() { return EvalValue::Bool(true); }
            EvalValue::Bool(eval_texpr(row, r).is_truthy())
        }
        TExpr::Bin(op, l, r) => {
            let lv = eval_texpr(row, l);
            let rv = eval_texpr(row, r);
            eval_binop(op, &lv, &rv)
        }
        TExpr::Call(name, args) => {
            let vals: Vec<EvalValue> = args.iter().map(|a| eval_texpr(row, a)).collect();
            eval_expr_fn(name, &vals)
        }
    }
}

fn eval_binop(op: &str, l: &EvalValue, r: &EvalValue) -> EvalValue {
    match op {
        "+" => {
            if matches!(l, EvalValue::Str(_)) || matches!(r, EvalValue::Str(_)) {
                return EvalValue::Str(format!("{}{}", eval_to_str(l), eval_to_str(r)));
            }
            num_binop(l, r, i64::checked_add, |a, b| a + b)
        }
        "-" => num_binop(l, r, i64::checked_sub, |a, b| a - b),
        "*" => num_binop(l, r, i64::checked_mul, |a, b| a * b),
        "/" => match (l.to_f64(), r.to_f64()) {
            (Ok(a), Ok(b)) if b != 0.0 => EvalValue::Float(a / b),
            _ => EvalValue::Null,
        },
        "%" => match (l, r) {
            (EvalValue::Int(a), EvalValue::Int(b)) if *b != 0 => EvalValue::Int(a % b),
            _ => match (l.to_f64(), r.to_f64()) {
                (Ok(a), Ok(b)) if b != 0.0 => EvalValue::Float(a % b),
                _ => EvalValue::Null,
            },
        },
        "==" => EvalValue::Bool(eval_eq(l, r)),
        "!=" => EvalValue::Bool(!eval_eq(l, r)),
        ">" | ">=" | "<" | "<=" => {
            if matches!(l, EvalValue::Null) || matches!(r, EvalValue::Null) {
                return EvalValue::Bool(false);
            }
            EvalValue::Bool(match op {
                ">"  => eval_gt(l, r),
                ">=" => !eval_lt(l, r),
                "<"  => eval_lt(l, r),
                _    => !eval_gt(l, r),
            })
        }
        "contains" | "starts_with" | "ends_with" => match (l, r) {
            (EvalValue::Str(a), EvalValue::Str(b)) => EvalValue::Bool(match op {
                "contains"    => a.contains(b.as_str()),
                "starts_with" => a.starts_with(b.as_str()),
                _             => a.ends_with(b.as_str()),
            }),
            _ => EvalValue::Bool(false),
        },
        _ => EvalValue::Null,
    }
}

// Int⊕Int se mantiene entero (con chequeo de overflow); si algo es float,
// o el entero desborda, se pasa a f64.
fn num_binop(
    l: &EvalValue, r: &EvalValue,
    int_op: fn(i64, i64) -> Option<i64>,
    f_op: fn(f64, f64) -> f64,
) -> EvalValue {
    if let (EvalValue::Int(a), EvalValue::Int(b)) = (l, r) {
        if let Some(n) = int_op(*a, *b) { return EvalValue::Int(n); }
    }
    match (l.to_f64(), r.to_f64()) {
        (Ok(a), Ok(b)) => EvalValue::Float(f_op(a, b)),
        _ => EvalValue::Null,
    }
}

fn eval_expr_fn(name: &str, args: &[EvalValue]) -> EvalValue {
    match name {
        "upper" | "lower" | "trim" => match &args[0] {
            EvalValue::Null => EvalValue::Null,
            v => {
                let s = match v { EvalValue::Str(s) => s.clone(), other => other.to_string() };
                EvalValue::Str(match name {
                    "upper" => s.to_uppercase(),
                    "lower" => s.to_lowercase(),
                    _       => s.trim().to_string(),
                })
            }
        },
        "len" => match &args[0] {
            EvalValue::Str(s)  => EvalValue::Int(s.chars().count() as i64),
            EvalValue::List(v) => EvalValue::Int(v.len() as i64),
            EvalValue::Dict(m) => EvalValue::Int(m.len() as i64),
            _ => EvalValue::Null,
        },
        "abs" => match &args[0] {
            EvalValue::Int(n) => EvalValue::Int(n.abs()),
            v => v.to_f64().map(|f| EvalValue::Float(f.abs())).unwrap_or(EvalValue::Null),
        },
        "round" => {
            let dec = args.get(1).and_then(|v| v.to_i64().ok()).unwrap_or(0) as i32;
            match args[0].to_f64() {
                Ok(f) => {
                    let factor = 10f64.powi(dec);
                    EvalValue::Float((f * factor).round() / factor)
                }
                Err(_) => EvalValue::Null,
            }
        }
        "floor" => args[0].to_f64().map(|f| EvalValue::Float(f.floor())).unwrap_or(EvalValue::Null),
        "ceil"  => args[0].to_f64().map(|f| EvalValue::Float(f.ceil())).unwrap_or(EvalValue::Null),
        "sqrt"  => match args[0].to_f64() {
            Ok(f) if f >= 0.0 => EvalValue::Float(f.sqrt()),
            _ => EvalValue::Null,
        },
        "min" | "max" => {
            let all_int = args.iter().all(|v| matches!(v, EvalValue::Int(_)));
            let nums: Vec<f64> = args.iter().filter_map(|v| v.to_f64().ok()).collect();
            if nums.len() != args.len() { return EvalValue::Null; }
            let best = if name == "min" {
                nums.iter().cloned().fold(f64::INFINITY, f64::min)
            } else {
                nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            };
            if all_int { EvalValue::Int(best as i64) } else { EvalValue::Float(best) }
        }
        "pow" => match (args[0].to_f64(), args[1].to_f64()) {
            (Ok(a), Ok(b)) => EvalValue::Float(a.powf(b)),
            _ => EvalValue::Null,
        },
        _ => EvalValue::Null,
    }
}

fn smart_num(f: f64) -> EvalValue {
    if f.fract() == 0.0 && f.abs() < 1e15 { EvalValue::Int(f as i64) }
    else { EvalValue::Float(f) }
}

//    Algoritmos de analítica                                                    

struct Stats { min: f64, max: f64, avg: f64, std: f64, p25: f64, median: f64, p75: f64 }

fn compute_stats(nums: &[f64]) -> Stats {
    let mut sorted = nums.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let min = sorted[0];
    let max = sorted[n - 1];
    let sum: f64 = sorted.iter().sum();
    let avg = sum / n as f64;
    let variance = sorted.iter().map(|x| (x - avg).powi(2)).sum::<f64>() / n as f64;
    let std = variance.sqrt();
    // Percentil con interpolación lineal (paridad con frame.stats)
    let percentile = |p: f64| -> f64 {
        if n == 1 { return sorted[0]; }
        let idx = p / 100.0 * (n - 1) as f64;
        let lo = idx.floor() as usize;
        let hi = (lo + 1).min(n - 1);
        let frac = idx - lo as f64;
        sorted[lo] + (sorted[hi] - sorted[lo]) * frac
    };
    Stats { min, max, avg, std, p25: percentile(25.0), median: percentile(50.0), p75: percentile(75.0) }
}

fn linear_forecast(values: &[f64], n: usize) -> Vec<f64> {
    let len = values.len() as f64;
    let sum_x: f64 = (0..values.len()).map(|i| i as f64).sum();
    let sum_y: f64 = values.iter().sum();
    let sum_xy: f64 = values.iter().enumerate().map(|(i, y)| i as f64 * y).sum();
    let sum_xx: f64 = (0..values.len()).map(|i| (i as f64).powi(2)).sum();
    let denom = len * sum_xx - sum_x.powi(2);
    if denom.abs() < f64::EPSILON {
        return vec![values.last().copied().unwrap_or(0.0); n];
    }
    let slope = (len * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / len;
    (0..n).map(|i| intercept + slope * (values.len() + i) as f64).collect()
}

fn detect_anomalies_iqr(values: &[f64]) -> Vec<bool> {
    if values.len() < 4 { return vec![false; values.len()]; }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let q1 = sorted[n / 4];
    let q3 = sorted[3 * n / 4];
    let iqr = q3 - q1;
    let lower = q1 - 1.5 * iqr;
    let upper = q3 + 1.5 * iqr;
    values.iter().map(|&v| v < lower || v > upper).collect()
}

fn pearson_correlation(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let mx = x.iter().sum::<f64>() / n;
    let my = y.iter().sum::<f64>() / n;
    let num: f64 = x.iter().zip(y).map(|(xi, yi)| (xi - mx) * (yi - my)).sum();
    let dx:  f64 = x.iter().map(|xi| (xi - mx).powi(2)).sum::<f64>().sqrt();
    let dy:  f64 = y.iter().map(|yi| (yi - my).powi(2)).sum::<f64>().sqrt();
    if dx == 0.0 || dy == 0.0 { 0.0 } else { num / (dx * dy) }
}

//    IA                                                                         

fn build_table_summary(rows: &[EvalValue]) -> String {
    let headers = infer_headers(rows);
    let mut lines = vec![
        format!("Tabla: {} filas, {} columnas", rows.len(), headers.len()),
        format!("Columnas: {}", headers.join(", ")),
    ];
    for col in &headers {
        let vals = column_values(rows, col);
        let nums: Vec<f64> = vals.iter().filter_map(|v| v.to_f64().ok()).collect();
        if !nums.is_empty() {
            let s = compute_stats(&nums);
            lines.push(format!(
                "  {} (número): min={:.2}, max={:.2}, avg={:.2}, std={:.2}",
                col, s.min, s.max, s.avg, s.std
            ));
        } else {
            let uniq: std::collections::HashSet<String> =
                vals.iter().filter(|v| !matches!(v, EvalValue::Null))
                    .map(|v| v.to_string()).collect();
            let sample: Vec<&String> = {
                let mut sv: Vec<&String> = uniq.iter().collect();
                sv.sort();
                sv.into_iter().take(5).collect()
            };
            lines.push(format!(
                "  {} (texto): {} valores únicos — ej: {}",
                col, uniq.len(), sample.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    lines.join("\n")
}

fn ai_call(prompt: &str) -> Result<EvalValue, String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .map_err(|_| "table (IA): falta ANTHROPIC_API_KEY o OPENAI_API_KEY en el entorno".to_string())?;

    let is_anthropic = std::env::var("ANTHROPIC_API_KEY").is_ok();

    if is_anthropic {
        let model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-haiku-4-5".to_string());
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 512,
            "messages": [{ "role": "user", "content": prompt }]
        });
        let resp = ureq::post("https://api.anthropic.com/v1/messages")
            .set("x-api-key", &api_key)
            .set("anthropic-version", "2023-06-01")
            .set("content-type", "application/json")
            .send_json(body)
            .map_err(|e| format!("table (IA): error de red: {}", e))?;
        let json: serde_json::Value = resp.into_json()
            .map_err(|e| format!("table (IA): respuesta inválida: {}", e))?;
        let text = json["content"][0]["text"].as_str().unwrap_or("").to_string();
        Ok(EvalValue::Str(text))
    } else {
        let body = serde_json::json!({
            "model": "gpt-4o-mini",
            "max_tokens": 512,
            "messages": [{ "role": "user", "content": prompt }]
        });
        let resp = ureq::post("https://api.openai.com/v1/chat/completions")
            .set("Authorization", &format!("Bearer {}", api_key))
            .set("content-type", "application/json")
            .send_json(body)
            .map_err(|e| format!("table (IA): error de red: {}", e))?;
        let json: serde_json::Value = resp.into_json()
            .map_err(|e| format!("table (IA): respuesta inválida: {}", e))?;
        let text = json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
        Ok(EvalValue::Str(text))
    }
}

//    Helpers generales                                                         

// Unión de las claves de TODAS las filas (no solo la primera), para que las
// tablas heterogéneas no pierdan columnas al guardar/imprimir.
fn infer_headers(rows: &[EvalValue]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut headers = Vec::new();
    for row in rows {
        if let EvalValue::Dict(m) = row {
            for k in m.keys() {
                if seen.insert(k.clone()) { headers.push(k.clone()); }
            }
        }
    }
    headers.sort();
    headers
}

fn column_values(rows: &[EvalValue], col: &str) -> Vec<EvalValue> {
    rows.iter().filter_map(|row| {
        if let EvalValue::Dict(m) = row { Some(m.get(col).cloned().unwrap_or(EvalValue::Null)) }
        else { None }
    }).collect()
}

fn infer_csv_value(s: &str) -> EvalValue {
    if s.is_empty() || s.eq_ignore_ascii_case("null") { return EvalValue::Null; }
    if let Ok(n) = s.parse::<i64>()  { return EvalValue::Int(n); }
    if let Ok(f) = s.parse::<f64>()  { return EvalValue::Float(f); }
    if s.eq_ignore_ascii_case("true")  || s == "yes" { return EvalValue::Bool(true); }
    if s.eq_ignore_ascii_case("false") || s == "no"  { return EvalValue::Bool(false); }
    EvalValue::Str(s.to_string())
}

fn excel_cell_eval(cell: &Data) -> EvalValue {
    match cell {
        Data::Int(n)    => EvalValue::Int(*n),
        Data::Float(f)  => EvalValue::Float(*f),
        Data::String(s) => EvalValue::Str(s.clone()),
        Data::Bool(b)   => EvalValue::Bool(*b),
        Data::Empty     => EvalValue::Null,
        _               => EvalValue::Null,
    }
}

fn excel_cell_str(cell: &Data) -> String {
    match cell {
        Data::String(s) => s.trim().to_string(),
        Data::Empty     => String::new(),
        other           => other.to_string(),
    }
}

fn eval_to_str(v: &EvalValue) -> String {
    match v {
        EvalValue::Null  => String::new(),
        EvalValue::Bool(b) => if *b { "true".into() } else { "false".into() },
        other => other.to_string(),
    }
}

fn eval_to_json(v: &EvalValue) -> serde_json::Value {
    match v {
        EvalValue::Int(n)   => serde_json::Value::Number((*n).into()),
        EvalValue::Float(f) => serde_json::json!(f),
        EvalValue::Str(s)   => serde_json::Value::String(s.clone()),
        EvalValue::Bool(b)  => serde_json::Value::Bool(*b),
        EvalValue::Null     => serde_json::Value::Null,
        EvalValue::List(v)  => serde_json::Value::Array(v.iter().map(eval_to_json).collect()),
        EvalValue::Dict(m)  => {
            let obj: serde_json::Map<String, serde_json::Value> =
                m.iter().map(|(k, v)| (k.clone(), eval_to_json(v))).collect();
            serde_json::Value::Object(obj)
        }
        other => serde_json::Value::String(other.to_string()),
    }
}

fn json_to_eval(v: serde_json::Value) -> EvalValue {
    match v {
        serde_json::Value::Null        => EvalValue::Null,
        serde_json::Value::Bool(b)     => EvalValue::Bool(b),
        serde_json::Value::Number(n)   => {
            if let Some(i) = n.as_i64() { EvalValue::Int(i) }
            else { EvalValue::Float(n.as_f64().unwrap_or(0.0)) }
        }
        serde_json::Value::String(s)   => EvalValue::Str(s),
        serde_json::Value::Array(arr)  => EvalValue::List(arr.into_iter().map(json_to_eval).collect()),
        serde_json::Value::Object(obj) => {
            let m = obj.into_iter().map(|(k, v)| (k, json_to_eval(v))).collect();
            EvalValue::Dict(m)
        }
    }
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
        (EvalValue::Null, _) | (_, EvalValue::Null) => false,
        _ => a.to_string() == b.to_string(),
    }
}

fn eval_gt(a: &EvalValue, b: &EvalValue) -> bool {
    match (a.to_f64(), b.to_f64()) {
        (Ok(x), Ok(y)) => x > y,
        _ => a.to_string() > b.to_string(),
    }
}

fn eval_lt(a: &EvalValue, b: &EvalValue) -> bool {
    match (a.to_f64(), b.to_f64()) {
        (Ok(x), Ok(y)) => x < y,
        _ => a.to_string() < b.to_string(),
    }
}

fn eval_ord(a: &EvalValue, b: &EvalValue) -> std::cmp::Ordering {
    match (a.to_f64(), b.to_f64()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.to_string().cmp(&b.to_string()),
    }
}

fn dict_get_val(row: &EvalValue, col: &str) -> EvalValue {
    if let EvalValue::Dict(m) = row { m.get(col).cloned().unwrap_or(EvalValue::Null) }
    else { EvalValue::Null }
}

fn cast_value(v: EvalValue, to: &str) -> EvalValue {
    match to {
        "int"    => v.to_i64().map(EvalValue::Int).unwrap_or(EvalValue::Null),
        "float"  => v.to_f64().map(EvalValue::Float).unwrap_or(EvalValue::Null),
        "string" => EvalValue::Str(v.to_string()),
        // "true"/"yes"/"false"/"no" se reconocen como texto; el resto por truthiness
        "bool"   => match &v {
            EvalValue::Bool(_) => v,
            EvalValue::Null    => EvalValue::Null,
            EvalValue::Str(s)  => {
                let t = s.trim().to_lowercase();
                if t == "true" || t == "yes" { EvalValue::Bool(true) }
                else if t == "false" || t == "no" { EvalValue::Bool(false) }
                else { EvalValue::Bool(!t.is_empty()) }
            }
            other => EvalValue::Bool(other.is_truthy()),
        },
        _ => v,
    }
}

fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() }
    else { format!("{}…", s.chars().take(max - 1).collect::<String>()) }
}

fn str_arg(fn_name: &str, args: &[EvalValue], idx: usize) -> Result<String, String> {
    match args.get(idx) {
        Some(EvalValue::Str(s)) => Ok(s.clone()),
        Some(other) => Ok(other.to_string()),
        None => Err(format!("table.{}: argumento {} requerido", fn_name, idx + 1)),
    }
}

fn int_arg(fn_name: &str, args: &[EvalValue], idx: usize) -> Result<i64, String> {
    match args.get(idx) {
        Some(v) => v.to_i64().map_err(|e| format!("table.{}: {}", fn_name, e)),
        None => Err(format!("table.{}: argumento {} requerido", fn_name, idx + 1)),
    }
}

fn list_arg(fn_name: &str, args: &[EvalValue], idx: usize) -> Result<Vec<EvalValue>, String> {
    match args.get(idx) {
        Some(EvalValue::List(v)) => Ok(v.clone()),
        Some(other) => Err(format!("table.{}: se esperaba tabla (lista), se recibió {}", fn_name, other.type_name())),
        None => Err(format!("table.{}: argumento {} requerido", fn_name, idx + 1)),
    }
}

fn str_list_arg(fn_name: &str, args: &[EvalValue], idx: usize) -> Result<Vec<String>, String> {
    match args.get(idx) {
        Some(EvalValue::List(v)) => Ok(v.iter().map(|x| x.to_string()).collect()),
        Some(EvalValue::Str(s)) => Ok(vec![s.clone()]),
        _ => Err(format!("table.{}: argumento {} debe ser lista de strings", fn_name, idx + 1)),
    }
}
