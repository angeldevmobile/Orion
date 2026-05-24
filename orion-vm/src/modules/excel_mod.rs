use crate::eval_value::EvalValue;
use std::collections::HashMap;
use chrono::{Datelike, NaiveDate};
use calamine::{Reader, open_workbook_auto, Data};
use rust_xlsxwriter::{Workbook, Format, Color, Formula,
    Chart, ChartType, ChartFormat, ChartSolidFill,
    ChartLine, ChartLineDashType, ChartLegendPosition};
use rust_xlsxwriter::conditional_format::{
    ConditionalFormatCell, ConditionalFormatCellRule,
    ConditionalFormatFormula,
    ConditionalFormatText, ConditionalFormatTextRule,
};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // sheets(path) → list of sheet names
        "sheets" => {
            let path = str_arg("sheets", &args, 0)?;
            let wb: calamine::Sheets<std::io::BufReader<std::fs::File>> =
                open_workbook_auto(&path)
                    .map_err(|e| format!("excel.sheets: no se pudo abrir '{}': {}", path, e))?;
            let names: Vec<EvalValue> = wb.sheet_names()
                .iter()
                .map(|n| EvalValue::Str(n.clone()))
                .collect();
            Ok(EvalValue::List(names))
        }

        // read(path) → list of dicts usando la primera hoja
        // read(path, sheet) → list of dicts de la hoja especificada
        "read" => {
            let path = str_arg("read", &args, 0)?;
            let sheet_name: Option<String> = args.get(1)
                .and_then(|v| if let EvalValue::Str(s) = v { Some(s.clone()) } else { None });

            let mut wb: calamine::Sheets<std::io::BufReader<std::fs::File>> =
                open_workbook_auto(&path)
                    .map_err(|e| format!("excel.read: no se pudo abrir '{}': {}", path, e))?;

            let target_sheet = match sheet_name {
                Some(n) => n,
                None => wb.sheet_names().first()
                    .cloned()
                    .ok_or_else(|| "excel.read: el archivo no tiene hojas".to_string())?,
            };

            let range = wb.worksheet_range(&target_sheet)
                .map_err(|e| format!("excel.read: hoja '{}' no encontrada: {}", target_sheet, e))?;

            let mut rows_iter = range.rows();

            // Primera fila → cabeceras
            let headers: Vec<String> = match rows_iter.next() {
                Some(row) => row.iter().map(|c| cell_to_string(c)).collect(),
                None => return Ok(EvalValue::List(vec![])),
            };

            let mut result = Vec::new();
            for row in rows_iter {
                let mut map = HashMap::new();
                for (i, cell) in row.iter().enumerate() {
                    let key = headers.get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("col_{}", i));
                    map.insert(key, cell_to_eval(cell));
                }
                result.push(EvalValue::Dict(map));
            }
            Ok(EvalValue::List(result))
        }

        // read_raw(path) → list of lists (todas las filas incluyendo cabecera)
        // read_raw(path, sheet) → de la hoja especificada
        "read_raw" => {
            let path = str_arg("read_raw", &args, 0)?;
            let sheet_name: Option<String> = args.get(1)
                .and_then(|v| if let EvalValue::Str(s) = v { Some(s.clone()) } else { None });

            let mut wb: calamine::Sheets<std::io::BufReader<std::fs::File>> =
                open_workbook_auto(&path)
                    .map_err(|e| format!("excel.read_raw: {}", e))?;

            let target = match sheet_name {
                Some(n) => n,
                None => wb.sheet_names().first().cloned()
                    .ok_or("excel.read_raw: sin hojas")?,
            };

            let range = wb.worksheet_range(&target)
                .map_err(|e| format!("excel.read_raw: {}", e))?;

            let result: Vec<EvalValue> = range.rows()
                .map(|row| {
                    EvalValue::List(row.iter().map(cell_to_eval).collect())
                })
                .collect();
            Ok(EvalValue::List(result))
        }

        // write(path, list_of_dicts) → escribe .xlsx con cabeceras automáticas
        // write(path, list_of_dicts, sheet_name) → con nombre de hoja
        "write" => {
            if args.len() < 2 {
                return Err("excel.write requiere (path, datos) o (path, datos, nombre_hoja)".into());
            }
            let path = str_arg("write", &args, 0)?;
            let rows = list_arg("write", &args, 1)?;
            let sheet_name = args.get(2)
                .and_then(|v| if let EvalValue::Str(s) = v { Some(s.clone()) } else { None })
                .unwrap_or_else(|| "Datos".to_string());

            // Extraer cabeceras antes de crear el workbook
            let has_dicts = matches!(rows.first(), Some(EvalValue::Dict(_)));
            let headers: Vec<String> = if has_dicts {
                if let Some(EvalValue::Dict(m)) = rows.first() {
                    let mut h: Vec<String> = m.keys().cloned().collect();
                    h.sort();
                    h
                } else {
                    vec![]
                }
            } else {
                match rows.first() {
                    Some(EvalValue::List(_)) | None => vec![],
                    _ => return Err("excel.write: los datos deben ser lista de dicts o listas".into()),
                }
            };

            let mut wb = Workbook::new();

            {
                let ws = wb.add_worksheet();
                ws.set_name(sheet_name.as_str())
                    .map_err(|e| format!("excel.write: nombre de hoja inválido: {}", e))?;

                let header_fmt = Format::new()
                    .set_bold()
                    .set_background_color(Color::RGB(0x2D5F8A))
                    .set_font_color(Color::White);

                if !headers.is_empty() {
                    for (col, h) in headers.iter().enumerate() {
                        ws.write_with_format(0, col as u16, h.as_str(), &header_fmt)
                            .map_err(|e| format!("excel.write: cabecera col {}: {}", col, e))?;
                    }
                }

                for (row_idx, row) in rows.iter().enumerate() {
                    let data_row = row_idx as u32 + if headers.is_empty() { 0 } else { 1 };
                    match row {
                        EvalValue::Dict(m) => {
                            for (col_idx, key) in headers.iter().enumerate() {
                                let v = m.get(key).unwrap_or(&EvalValue::Null);
                                write_cell(ws, data_row, col_idx as u16, v)?;
                            }
                        }
                        EvalValue::List(fields) => {
                            for (col_idx, v) in fields.iter().enumerate() {
                                write_cell(ws, data_row, col_idx as u16, v)?;
                            }
                        }
                        _ => {}
                    }
                }

                if !headers.is_empty() {
                    for col_idx in 0..headers.len() {
                        ws.set_column_width(col_idx as u16, 18.0)
                            .map_err(|e| format!("excel.write: ancho col {}: {}", col_idx, e))?;
                    }
                }
            } // ws borrow termina aquí

            wb.save(&path).map_err(|e| format!("excel.write: error guardando '{}': {}", path, e))?;
            Ok(EvalValue::Null)
        }

        // write_multi(path, dict { sheet_name → list_of_dicts }) → xlsx con múltiples hojas
        "write_multi" => {
            if args.len() < 2 {
                return Err("excel.write_multi requiere (path, dict_de_hojas)".into());
            }
            let path = str_arg("write_multi", &args, 0)?;
            let sheets_map = match &args[1] {
                EvalValue::Dict(m) => m.clone(),
                other => return Err(format!("excel.write_multi: se esperaba dict, se recibió {}", other.type_name())),
            };

            let mut wb = Workbook::new();
            let header_fmt = Format::new()
                .set_bold()
                .set_background_color(Color::RGB(0x2D5F8A))
                .set_font_color(Color::White);

            let mut sheet_names: Vec<String> = sheets_map.keys().cloned().collect();
            sheet_names.sort();

            for sheet_name in &sheet_names {
                let rows = match sheets_map.get(sheet_name) {
                    Some(EvalValue::List(v)) => v.clone(),
                    _ => continue,
                };

                let headers: Vec<String> = match rows.first() {
                    Some(EvalValue::Dict(m)) => {
                        let mut h: Vec<String> = m.keys().cloned().collect();
                        h.sort();
                        h
                    }
                    _ => vec![],
                };

                {
                    let ws = wb.add_worksheet();
                    ws.set_name(sheet_name.as_str())
                        .map_err(|e| format!("excel.write_multi: {}", e))?;

                    if !headers.is_empty() {
                        for (col, h) in headers.iter().enumerate() {
                            ws.write_with_format(0, col as u16, h.as_str(), &header_fmt)
                                .map_err(|e| format!("excel.write_multi: {}", e))?;
                        }
                    }

                    for (row_idx, row) in rows.iter().enumerate() {
                        let data_row = row_idx as u32 + if headers.is_empty() { 0 } else { 1 };
                        match row {
                            EvalValue::Dict(m) => {
                                for (col_idx, key) in headers.iter().enumerate() {
                                    let v = m.get(key).unwrap_or(&EvalValue::Null);
                                    write_cell(ws, data_row, col_idx as u16, v)?;
                                }
                            }
                            EvalValue::List(fields) => {
                                for (col_idx, v) in fields.iter().enumerate() {
                                    write_cell(ws, data_row, col_idx as u16, v)?;
                                }
                            }
                            _ => {}
                        }
                    }
                } // ws borrow termina aquí
            }

            wb.save(&path).map_err(|e| format!("excel.write_multi: {}", e))?;
            Ok(EvalValue::Null)
        }

        // info(path) → dict { sheets, rows, cols } info básica
        "info" => {
            let path = str_arg("info", &args, 0)?;
            let mut wb: calamine::Sheets<std::io::BufReader<std::fs::File>> =
                open_workbook_auto(&path)
                    .map_err(|e| format!("excel.info: {}", e))?;

            let sheet_names = wb.sheet_names().to_vec();
            let mut sheets_info = Vec::new();

            for name in &sheet_names {
                if let Ok(range) = wb.worksheet_range(name) {
                    let mut info = HashMap::new();
                    info.insert("name".into(),  EvalValue::Str(name.clone()));
                    info.insert("rows".into(),  EvalValue::Int(range.height() as i64));
                    info.insert("cols".into(),  EvalValue::Int(range.width() as i64));
                    sheets_info.push(EvalValue::Dict(info));
                }
            }

            let mut result = HashMap::new();
            result.insert("file".into(),   EvalValue::Str(path));
            result.insert("sheets".into(), EvalValue::Int(sheet_names.len() as i64));
            result.insert("detail".into(), EvalValue::List(sheets_info));
            Ok(EvalValue::Dict(result))
        }

        //   write_styled                              
        // write_styled(path, datos, config?) → xlsx con formato avanzado por columna
        // config: {
        //   hoja, titulo, cabecera:{fondo,texto}, alternar, freeze, autofilter,
        //   anchos:{col:n}, totales:[cols], formatos:{col:{numero,bold,fondo,texto,condicional:[...]}}
        // }
        "write_styled" => {
            if args.len() < 2 {
                return Err("excel.write_styled requiere (path, datos, config?)".into());
            }
            let path = str_arg("write_styled", &args, 0)?;
            let rows = list_arg("write_styled", &args, 1)?;
            let config = match args.get(2) {
                Some(EvalValue::Dict(m)) => m.clone(),
                _ => HashMap::new(),
            };
            write_styled_impl(&path, rows, config)
        }

        //   Data pipeline                              

        // filtrar(datos, campo, op, valor) → lista filtrada
        // op: ">" | "<" | ">=" | "<=" | "==" | "!=" | "contiene" | "empieza" | "termina"
        "filtrar" | "filter" => {
            if args.len() < 4 {
                return Err("excel.filtrar requiere (datos, campo, op, valor)".into());
            }
            let rows  = list_arg("filtrar", &args, 0)?;
            let campo = str_arg("filtrar", &args, 1)?;
            let op    = str_arg("filtrar", &args, 2)?;
            let valor = args[3].clone();
            let result: Vec<EvalValue> = rows.into_iter()
                .filter(|row| match row {
                    EvalValue::Dict(m) => m.get(&campo).map_or(false, |v| compare_values(v, &op, &valor)),
                    _ => false,
                })
                .collect();
            Ok(EvalValue::List(result))
        }

        // agrupar(datos, campo, config?) → list agrupada
        // config: {"suma": ["col1","col2"], "conteo": yes, "promedio": ["col1"]}
        // group(data, campo, spec) — multi-agg
        // spec: { "col": ["sum","avg","max","min","count","first","last","std","median"],
        //         "count": yes }
        "agrupar" | "group" => {
            if args.len() < 2 {
                return Err("excel.group requiere (datos, campo, spec?)".into());
            }
            let rows  = list_arg("group", &args, 0)?;
            let campo = str_arg("group", &args, 1)?;
            let spec  = match args.get(2) {
                Some(EvalValue::Dict(m)) => m.clone(),
                _ => HashMap::new(),
            };
            group_by_multi(rows, campo, spec)
        }

        // ordenar(datos, campo, dir?) → sorted — dir: "asc" (default) | "desc"
        // sort(data, criterios...) — multi-columna
        // Formas:
        //   excel.sort(data, "col+")              → col asc
        //   excel.sort(data, "col-")              → col desc
        //   excel.sort(data, "region+", "sales-") → multi-col shorthand
        //   excel.sort(data, [{by:"col",dir:"asc"}, ...]) → explícito
        //   excel.sort(data, "col", "asc"|"desc") → compat. 1-col anterior
        "ordenar" | "sort_by" | "sort" => {
            if args.len() < 2 {
                return Err("excel.sort requiere (datos, criterio...)".into());
            }
            let mut rows = list_arg("sort", &args, 0)?;
            // Construir lista de (campo, desc)
            let criteria: Vec<(String, bool)> = match &args[1] {
                // Estilo explícito: lista de dicts [{by, dir}]
                EvalValue::List(specs) => {
                    specs.iter().map(|s| match s {
                        EvalValue::Dict(m) => {
                            let col  = cfg_str(m, "by").unwrap_or_default();
                            let desc = cfg_str(m, "dir").map(|d| d == "desc").unwrap_or(false);
                            (col, desc)
                        }
                        EvalValue::Str(s) => parse_sort_key(s),
                        _ => (String::new(), false),
                    }).filter(|(c, _)| !c.is_empty()).collect()
                }
                // Estilo corto: uno o más strings "col+" / "col-"
                EvalValue::Str(s) => {
                    // Si el siguiente arg es "asc"/"desc" → modo compat 1-col
                    let is_dir_arg = matches!(args.get(2),
                        Some(EvalValue::Str(d)) if d == "asc" || d == "desc");
                    if is_dir_arg {
                        let desc = matches!(args.get(2), Some(EvalValue::Str(d)) if d == "desc");
                        vec![(s.clone(), desc)]
                    } else {
                        // Recoger todos los strings restantes como criterios
                        let mut crit = vec![parse_sort_key(s)];
                        for extra in args[2..].iter() {
                            if let EvalValue::Str(k) = extra { crit.push(parse_sort_key(k)); }
                        }
                        crit
                    }
                }
                other => return Err(format!(
                    "excel.sort: criterio inválido ({})", other.type_name()
                )),
            };

            rows.sort_by(|a, b| {
                for (col, desc) in &criteria {
                    let va = dict_get(a, col);
                    let vb = dict_get(b, col);
                    let ord = compare_eval_order(&va, &vb);
                    let ord = if *desc { ord.reverse() } else { ord };
                    if ord != std::cmp::Ordering::Equal { return ord; }
                }
                std::cmp::Ordering::Equal
            });
            Ok(EvalValue::List(rows))
        }

        // columna(datos, campo) → lista de valores de esa columna
        "columna" | "column" => {
            if args.len() < 2 {
                return Err("excel.columna requiere (datos, campo)".into());
            }
            let rows  = list_arg("columna", &args, 0)?;
            let campo = str_arg("columna", &args, 1)?;
            Ok(EvalValue::List(rows.into_iter().map(|r| dict_get(&r, &campo)).collect()))
        }

        // sumar(datos, campo) → Float — suma de columna numérica
        "sumar" | "sum_col" => {
            if args.len() < 2 {
                return Err("excel.sumar requiere (datos, campo)".into());
            }
            let rows  = list_arg("sumar", &args, 0)?;
            let campo = str_arg("sumar", &args, 1)?;
            let total: f64 = rows.iter()
                .map(|r| to_f64_val(&dict_get(r, &campo)).unwrap_or(0.0))
                .sum();
            Ok(EvalValue::Float(total))
        }

        // promedio(datos, campo) → Float — promedio de columna numérica
        "promedio" | "avg_col" => {
            if args.len() < 2 {
                return Err("excel.promedio requiere (datos, campo)".into());
            }
            let rows  = list_arg("promedio", &args, 0)?;
            let campo = str_arg("promedio", &args, 1)?;
            let vals: Vec<f64> = rows.iter()
                .filter_map(|r| to_f64_val(&dict_get(r, &campo)))
                .collect();
            let avg = if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 };
            Ok(EvalValue::Float(avg))
        }

        // pivot(datos, campo_fila, campo_col, campo_valor) → lista formato ancho
        "pivot" => {
            if args.len() < 4 {
                return Err("excel.pivot requiere (datos, campo_fila, campo_col, campo_valor)".into());
            }
            let rows        = list_arg("pivot", &args, 0)?;
            let campo_fila  = str_arg("pivot", &args, 1)?;
            let campo_col   = str_arg("pivot", &args, 2)?;
            let campo_valor = str_arg("pivot", &args, 3)?;
            pivot_table(rows, campo_fila, campo_col, campo_valor)
        }

        // long(datos, keep: [campos], var: "nombre_col", val: "nombre_val")
        // Convierte formato ancho a largo (unpivot / melt).
        // Las columnas no listadas en `keep` se convierten en filas.
        "long" | "melt" | "unpivot" => {
            if args.len() < 4 {
                return Err("excel.long requiere (datos, keep:[cols], var:\"nombre\", val:\"nombre\")".into());
            }
            let rows = list_arg("long", &args, 0)?;
            let keep_cols: Vec<String> = match &args[1] {
                EvalValue::List(l) => l.iter().map(|v| to_str_val(v)).collect(),
                EvalValue::Str(s)  => vec![s.clone()],
                _ => return Err("excel.long: `keep` debe ser lista de columnas o string".into()),
            };
            let var_col = str_arg("long", &args, 2)?;
            let val_col = str_arg("long", &args, 3)?;

            let mut result: Vec<EvalValue> = Vec::new();
            for row in &rows {
                if let EvalValue::Dict(m) = row {
                    // Columnas que se convierten en filas = todas menos las de keep
                    let value_cols: Vec<String> = m.keys()
                        .filter(|k| !keep_cols.contains(k))
                        .cloned()
                        .collect();

                    for col in &value_cols {
                        let mut new_row: HashMap<String, EvalValue> = HashMap::new();
                        // Copiar las columnas fijas
                        for k in &keep_cols {
                            if let Some(v) = m.get(k) {
                                new_row.insert(k.clone(), v.clone());
                            }
                        }
                        // Insertar nombre de la columna y su valor
                        new_row.insert(var_col.clone(), EvalValue::Str(col.clone()));
                        new_row.insert(val_col.clone(), m.get(col).cloned().unwrap_or(EvalValue::Null));
                        result.push(EvalValue::Dict(new_row));
                    }
                } else {
                    return Err("excel.long: cada fila debe ser un dict".into());
                }
            }
            Ok(EvalValue::List(result))
        }

        // seleccionar(datos, [campos]) → lista con solo esas columnas
        "seleccionar" | "select_cols" => {
            if args.len() < 2 {
                return Err("excel.seleccionar requiere (datos, [campos])".into());
            }
            let rows   = list_arg("seleccionar", &args, 0)?;
            let campos: Vec<String> = match &args[1] {
                EvalValue::List(l) => l.iter().map(|v| to_str_val(v)).collect(),
                _ => return Err("excel.seleccionar: segundo arg debe ser lista de campos".into()),
            };
            let result: Vec<EvalValue> = rows.into_iter().map(|row| {
                if let EvalValue::Dict(m) = row {
                    let mut new_m = HashMap::new();
                    for c in &campos {
                        if let Some(v) = m.get(c) { new_m.insert(c.clone(), v.clone()); }
                    }
                    EvalValue::Dict(new_m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        // unir(datos1, datos2, ...) → lista concatenada (más claro que +)
        "unir" | "concat" => {
            let mut result = Vec::new();
            for arg in &args {
                match arg {
                    EvalValue::List(l) => result.extend_from_slice(l),
                    _ => return Err("excel.unir: todos los argumentos deben ser listas".into()),
                }
            }
            Ok(EvalValue::List(result))
        }

        // cruzar / join — una o múltiples claves
        // join(data1, data2, "clave", tipo?)
        // join(data1, data2, ["clave1","clave2"], tipo?)
        // tipo: "inner" (default) | "left" | "right" | "full"
        "cruzar" | "join" => {
            if args.len() < 3 {
                return Err("excel.join requiere (data1, data2, clave|[claves], tipo?)".into());
            }
            let left  = list_arg("join", &args, 0)?;
            let right = list_arg("join", &args, 1)?;
            let claves: Vec<String> = match &args[2] {
                EvalValue::Str(s)  => vec![s.clone()],
                EvalValue::List(l) => l.iter().map(|v| to_str_val(v)).collect(),
                other => return Err(format!(
                    "excel.join: clave debe ser string o lista, se recibió {}", other.type_name()
                )),
            };
            let tipo = args.get(3)
                .and_then(|v| if let EvalValue::Str(s) = v { Some(s.clone()) } else { None })
                .unwrap_or_else(|| "inner".to_string());
            join_multi(left, right, claves, &tipo)
        }

        // deduplicar(data, campos?) → filas únicas
        // campos: lista de campos clave — si se omite, usa todos los campos
        "deduplicar" | "dedupe" => {
            if args.is_empty() {
                return Err("excel.deduplicar requiere (data, [campos]?)".into());
            }
            let rows = list_arg("deduplicar", &args, 0)?;
            let campos: Option<Vec<String>> = match args.get(1) {
                Some(EvalValue::List(l)) => Some(l.iter().map(|v| to_str_val(v)).collect()),
                _ => None,
            };
            deduplicate(rows, campos)
        }

        // estadisticas(data, campo) → dict { min, max, sum, avg, count, std, mediana }
        "estadisticas" | "stats" => {
            if args.len() < 2 {
                return Err("excel.estadisticas requiere (data, campo)".into());
            }
            let rows  = list_arg("estadisticas", &args, 0)?;
            let campo = str_arg("estadisticas", &args, 1)?;
            compute_stats(rows, campo)
        }

        // renombrar_col(data, viejo, nuevo) → lista con columna renombrada
        "renombrar_col" | "rename_col" => {
            if args.len() < 3 {
                return Err("excel.renombrar_col requiere (data, viejo, nuevo)".into());
            }
            let rows  = list_arg("renombrar_col", &args, 0)?;
            let viejo = str_arg("renombrar_col", &args, 1)?;
            let nuevo = str_arg("renombrar_col", &args, 2)?;
            let result: Vec<EvalValue> = rows.into_iter().map(|row| {
                if let EvalValue::Dict(mut m) = row {
                    if let Some(v) = m.remove(&viejo) {
                        m.insert(nuevo.clone(), v);
                    }
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        // rellenar(data, campo, valor) → reemplaza nulos/vacíos en campo con valor
        "rellenar" | "fill_null" => {
            if args.len() < 3 {
                return Err("excel.rellenar requiere (data, campo, valor)".into());
            }
            let rows  = list_arg("rellenar", &args, 0)?;
            let campo = str_arg("rellenar", &args, 1)?;
            let valor = args[2].clone();
            let result: Vec<EvalValue> = rows.into_iter().map(|row| {
                if let EvalValue::Dict(mut m) = row {
                    let is_empty = match m.get(&campo) {
                        None | Some(EvalValue::Null) => true,
                        Some(EvalValue::Str(s)) if s.trim().is_empty() => true,
                        _ => false,
                    };
                    if is_empty { m.insert(campo.clone(), valor.clone()); }
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        // F-5: dates(data, col, format?) → normaliza strings de fecha a YYYY-MM-DD
        // Formatos: "DD/MM/YYYY" | "MM/DD/YYYY" | "YYYY-MM-DD" | "auto"
        "dates" => {
            if args.len() < 2 {
                return Err("excel.dates requiere (data, col, formato?)".into());
            }
            let rows = list_arg("dates", &args, 0)?;
            let col  = str_arg("dates", &args, 1)?;
            let fmt  = match args.get(2) {
                Some(EvalValue::Str(s)) => s.clone(),
                _ => "auto".to_string(),
            };

            let result: Vec<EvalValue> = rows.into_iter().map(|row| {
                if let EvalValue::Dict(mut m) = row {
                    if let Some(val) = m.get(&col).cloned() {
                        let raw = to_str_val(&val);
                        if let Some(iso) = parse_date_str(&raw, &fmt) {
                            m.insert(col.clone(), EvalValue::Str(iso));
                        }
                    }
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        // F-5: date_parts(data, col, [partes]) → agrega columnas col_year, col_month, etc.
        // Partes: "year" | "month" | "day" | "quarter" | "weekday" | "week" | "hour"
        "date_parts" => {
            if args.len() < 3 {
                return Err("excel.date_parts requiere (data, col, [partes])".into());
            }
            let rows  = list_arg("date_parts", &args, 0)?;
            let col   = str_arg("date_parts", &args, 1)?;
            let parts: Vec<String> = match &args[2] {
                EvalValue::List(l) => l.iter().map(|v| to_str_val(v)).collect(),
                EvalValue::Str(s)  => vec![s.clone()],
                other => return Err(format!(
                    "excel.date_parts: partes debe ser lista, se recibió {}", other.type_name()
                )),
            };

            let result: Vec<EvalValue> = rows.into_iter().map(|row| {
                if let EvalValue::Dict(mut m) = row {
                    if let Some(val) = m.get(&col).cloned() {
                        let raw = to_str_val(&val);
                        if let Ok(date) = NaiveDate::parse_from_str(&raw, "%Y-%m-%d") {
                            for part in &parts {
                                let key = format!("{}_{}", col, part);
                                let v = match part.as_str() {
                                    "year"    => EvalValue::Int(date.year() as i64),
                                    "month"   => EvalValue::Int(date.month() as i64),
                                    "day"     => EvalValue::Int(date.day() as i64),
                                    "quarter" => EvalValue::Int(((date.month() - 1) / 3 + 1) as i64),
                                    "weekday" => EvalValue::Str(format!("{}", date.weekday())),
                                    "week"    => EvalValue::Int(date.iso_week().week() as i64),
                                    "hour"    => EvalValue::Int(0),
                                    _         => EvalValue::Null,
                                };
                                m.insert(key, v);
                            }
                        }
                    }
                    EvalValue::Dict(m)
                } else { row }
            }).collect();
            Ok(EvalValue::List(result))
        }

        // F-9: sheet(path) | sheet(path, name) → { name, rows, cols, headers, data }
        // Lectura completa de una hoja: metadata + datos en un solo dict.
        "sheet" => {
            let path = str_arg("sheet", &args, 0)?;
            let sheet_name: Option<String> = args.get(1)
                .and_then(|v| if let EvalValue::Str(s) = v { Some(s.clone()) } else { None });

            let mut wb: calamine::Sheets<std::io::BufReader<std::fs::File>> =
                open_workbook_auto(&path)
                    .map_err(|e| format!("excel.sheet: no se pudo abrir '{}': {}", path, e))?;

            let target = match sheet_name {
                Some(n) => n,
                None => wb.sheet_names().first().cloned()
                    .ok_or_else(|| "excel.sheet: el archivo no tiene hojas".to_string())?,
            };

            let range = wb.worksheet_range(&target)
                .map_err(|e| format!("excel.sheet: hoja '{}' no encontrada: {}", target, e))?;

            let mut rows_iter = range.rows();

            let headers: Vec<String> = match rows_iter.next() {
                Some(row) => row.iter().map(|c| cell_to_string(c)).collect(),
                None => {
                    let mut m = HashMap::new();
                    m.insert("name".into(),    EvalValue::Str(target));
                    m.insert("rows".into(),    EvalValue::Int(0));
                    m.insert("cols".into(),    EvalValue::Int(0));
                    m.insert("headers".into(), EvalValue::List(vec![]));
                    m.insert("data".into(),    EvalValue::List(vec![]));
                    return Ok(EvalValue::Dict(m));
                }
            };

            let mut data: Vec<EvalValue> = Vec::new();
            for row in rows_iter {
                let mut map = HashMap::new();
                for (i, cell) in row.iter().enumerate() {
                    let key = headers.get(i).cloned()
                        .unwrap_or_else(|| format!("col_{}", i));
                    map.insert(key, cell_to_eval(cell));
                }
                data.push(EvalValue::Dict(map));
            }

            let nrows   = data.len() as i64;
            let ncols   = headers.len() as i64;
            let hdrs_v: Vec<EvalValue> = headers.iter()
                .map(|h| EvalValue::Str(h.clone()))
                .collect();

            let mut m = HashMap::new();
            m.insert("name".into(),    EvalValue::Str(target));
            m.insert("rows".into(),    EvalValue::Int(nrows));
            m.insert("cols".into(),    EvalValue::Int(ncols));
            m.insert("headers".into(), EvalValue::List(hdrs_v));
            m.insert("data".into(),    EvalValue::List(data));
            Ok(EvalValue::Dict(m))
        }

        // f → retorna el sub-módulo formula builder (excel.f.pct, .ratio, .rank, ...)
        "f" => Ok(EvalValue::Module("excel_f".to_string())),

        // chart(path, datos, config) → genera xlsx con gráfico
        // config: { type, x, y, title, x_title, y_title,
        //           palette, colors, sheet, data_sheet,
        //           stacked, smooth, show_values, goal,
        //           width, height }
        "chart" => {
            if args.len() < 2 {
                return Err("excel.chart requiere (path, datos, config?)".into());
            }
            let path   = str_arg("chart", &args, 0)?;
            let rows   = list_arg("chart", &args, 1)?;
            let config = match args.get(2) {
                Some(EvalValue::Dict(m)) => m.clone(),
                _ => HashMap::new(),
            };
            excel_chart_impl(&path, rows, config)
        }

        f => Err(format!("excel.{}: función no encontrada", f)),
    }
}

//   Helpers                       

fn cell_to_eval(cell: &Data) -> EvalValue {
    match cell {
        Data::Int(n)    => EvalValue::Int(*n),
        Data::Float(f)  => EvalValue::Float(*f),
        Data::String(s) => EvalValue::Str(s.clone()),
        Data::Bool(b)   => EvalValue::Bool(*b),
        Data::Empty     => EvalValue::Null,
        Data::Error(_)  => EvalValue::Null,
        other           => EvalValue::Str(other.to_string()),
    }
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::String(s) => s.trim().to_string(),
        Data::Empty     => String::new(),
        other           => other.to_string(),
    }
}

fn write_cell(
    ws: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    v: &EvalValue,
) -> Result<(), String> {
    let result = match v {
        EvalValue::Int(n)   => ws.write(row, col, *n),
        EvalValue::Float(f) => ws.write(row, col, *f),
        EvalValue::Bool(b)  => ws.write(row, col, *b),
        EvalValue::Null     => ws.write(row, col, ""),
        other               => ws.write(row, col, other.to_string().as_str()),
    };
    result.map(|_| ()).map_err(|e| format!("excel: error en celda ({}, {}): {}", row, col, e))
}

fn str_arg(fn_name: &str, args: &[EvalValue], idx: usize) -> Result<String, String> {
    match args.get(idx) {
        Some(EvalValue::Str(s)) => Ok(s.clone()),
        Some(other) => Ok(other.to_string()),
        None => Err(format!("excel.{}: argumento {} requerido", fn_name, idx + 1)),
    }
}

fn list_arg(fn_name: &str, args: &[EvalValue], idx: usize) -> Result<Vec<EvalValue>, String> {
    match args.get(idx) {
        Some(EvalValue::List(v)) => Ok(v.clone()),
        Some(other) => Err(format!("excel.{}: se esperaba lista, se recibió {}", fn_name, other.type_name())),
        None => Err(format!("excel.{}: argumento {} requerido", fn_name, idx + 1)),
    }
}

//    write_styled                               

fn write_styled_impl(
    path: &str,
    rows: Vec<EvalValue>,
    config: HashMap<String, EvalValue>,
) -> Result<EvalValue, String> {
    let sheet_name = cfg_str(&config, "hoja").unwrap_or_else(|| "Datos".to_string());
    let titulo     = cfg_str(&config, "titulo");
    let alternar   = cfg_bool(&config, "alternar");
    let do_freeze  = cfg_bool(&config, "freeze");
    let do_filter  = cfg_bool(&config, "autofilter");

    let formatos: HashMap<String, EvalValue> = match config.get("formatos") {
        Some(EvalValue::Dict(m)) => m.clone(),
        _ => HashMap::new(),
    };
    let totales_cols: Vec<String> = match config.get("totales") {
        Some(EvalValue::List(l)) => l.iter().map(|v| to_str_val(v)).collect(),
        _ => vec![],
    };
    let anchos: HashMap<String, EvalValue> = match config.get("anchos") {
        Some(EvalValue::Dict(m)) => m.clone(),
        _ => HashMap::new(),
    };

    let (hdr_bg, hdr_fg) = match config.get("cabecera") {
        Some(EvalValue::Dict(m)) => (
            cfg_color(m, "fondo").unwrap_or(0x2D5F8A),
            cfg_color(m, "texto").unwrap_or(0xFFFFFF),
        ),
        _ => (0x2D5F8A, 0xFFFFFF),
    };

    let headers: Vec<String> = match rows.first() {
        Some(EvalValue::Dict(m)) => {
            let mut h: Vec<String> = m.keys().cloned().collect();
            h.sort();
            h
        }
        _ => return Err("excel.write_styled: los datos deben ser lista de dicts".into()),
    };
    if headers.is_empty() {
        return Err("excel.write_styled: el primer dict está vacío".into());
    }

    // Columnas de fórmulas vivas: { "col_name": descriptor_dict, ... }
    let formulas: Vec<(String, HashMap<String, EvalValue>)> = {
        let mut v: Vec<(String, HashMap<String, EvalValue>)> = match config.get("formulas") {
            Some(EvalValue::Dict(m)) => m.iter().filter_map(|(name, val)| {
                if let EvalValue::Dict(desc) = val {
                    if desc.contains_key("_f") {
                        return Some((name.clone(), desc.clone()));
                    }
                }
                None
            }).collect(),
            _ => vec![],
        };
        v.sort_by(|a, b| a.0.cmp(&b.0)); // orden determinístico
        v
    };

    // Gráficos embebidos: lista de configs de gráfico que referenciarán la hoja de datos
    let charts_cfgs: Vec<HashMap<String, EvalValue>> = match config.get("charts") {
        Some(EvalValue::List(l)) => l.iter().filter_map(|v| {
            if let EvalValue::Dict(m) = v { Some(m.clone()) } else { None }
        }).collect(),
        Some(EvalValue::Dict(m)) => vec![m.clone()],
        _ => vec![],
    };

    // Posiciones de filas — las calculamos antes del bloque para usarlas al crear charts
    let header_row_pos: u32 = if titulo.is_some() { 1 } else { 0 };
    let data_start_pos: u32 = header_row_pos + 1;
    let data_end_pos:   u32 = data_start_pos + rows.len() as u32 - 1;

    let mut wb = Workbook::new();

    // Build per-column base formats ahead of time
    let col_fmts: Vec<Format> = headers.iter().map(|key| {
        let mut f = Format::new();
        if let Some(EvalValue::Dict(cfg)) = formatos.get(key) {
            if cfg_bool(cfg, "bold") { f = f.set_bold(); }
            if let Some(n) = cfg_str(cfg, "numero") { f = f.set_num_format(&n); }
            if let Some(bg) = cfg_color(cfg, "fondo") { f = f.set_background_color(Color::RGB(bg)); }
            if let Some(fg) = cfg_color(cfg, "texto") { f = f.set_font_color(Color::RGB(fg)); }
        }
        f
    }).collect();

    // Fetch column num-formats for totals row
    let col_num_fmts: Vec<Option<String>> = headers.iter().map(|key| {
        if let Some(EvalValue::Dict(cfg)) = formatos.get(key) {
            cfg_str(cfg, "numero")
        } else {
            None
        }
    }).collect();

    {
        let ws = wb.add_worksheet();
        ws.set_name(sheet_name.as_str())
            .map_err(|e| format!("excel.write_styled: {}", e))?;

        let last_col = (headers.len() - 1) as u16;
        let mut cur: u32 = 0;

        //   Título (fila mergeada)
        if let Some(ref t) = titulo {
            let title_fmt = Format::new()
                .set_bold()
                .set_font_size(14.0)
                .set_background_color(Color::RGB(hdr_bg))
                .set_font_color(Color::RGB(hdr_fg));
            ws.merge_range(cur, 0, cur, last_col, t.as_str(), &title_fmt)
                .map_err(|e| format!("excel.write_styled titulo: {}", e))?;
            cur += 1;
        }

        let header_row = cur;
        let hdr_fmt = Format::new()
            .set_bold()
            .set_background_color(Color::RGB(hdr_bg))
            .set_font_color(Color::RGB(hdr_fg));

        for (col, h) in headers.iter().enumerate() {
            ws.write_with_format(cur, col as u16, h.as_str(), &hdr_fmt)
                .map_err(|e| format!("excel.write_styled header: {}", e))?;
        }
        // Encabezados de columnas de fórmula
        let formula_col_start = headers.len() as u16;
        for (fi, (fname, _)) in formulas.iter().enumerate() {
            ws.write_with_format(cur, formula_col_start + fi as u16, fname.as_str(), &hdr_fmt)
                .map_err(|e| format!("excel.write_styled formula header: {}", e))?;
        }
        cur += 1;
        let data_start = cur;

        // Mapa col_name → índice de columna (para generar fórmulas)
        let col_map: HashMap<String, u16> = headers.iter().enumerate()
            .map(|(i, h)| (h.clone(), i as u16))
            .collect();

        //   Filas de datos
        let mut totals: HashMap<String, f64> = HashMap::new();
        for row in &rows {
            if let EvalValue::Dict(m) = row {
                for (ci, key) in headers.iter().enumerate() {
                    let v = m.get(key).unwrap_or(&EvalValue::Null);
                    write_cell_fmt(ws, cur, ci as u16, v, &col_fmts[ci])?;
                    if totales_cols.contains(key) {
                        *totals.entry(key.clone()).or_insert(0.0) +=
                            to_f64_val(v).unwrap_or(0.0);
                    }
                }
                // Fórmulas vivas por fila
                if !formulas.is_empty() {
                    let excel_row = cur + 1; // xlsxwriter es 0-based; Excel es 1-based
                    let ds = data_start + 1; // primera fila de datos en Excel (1-based)
                    // data_end_pos ya está calculado fuera del bloque
                    let de = data_end_pos + 1;
                    for (fi, (_, desc)) in formulas.iter().enumerate() {
                        match generate_formula_str(desc, excel_row, &col_map, ds, de) {
                            Ok(fstr) => {
                                ws.write_formula(cur, formula_col_start + fi as u16,
                                    Formula::new(&fstr))
                                    .map_err(|e| format!("excel.write_styled fórmula: {}", e))?;
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
                cur += 1;
            }
        }
        let data_end = cur.saturating_sub(1);

        //   Formato condicional por columna
        for (ci, key) in headers.iter().enumerate() {
            if let Some(EvalValue::Dict(cfg)) = formatos.get(key) {
                if let Some(EvalValue::List(conds)) = cfg.get("condicional") {
                    let col_letter = col_to_letter(ci as u16);
                    for cond in conds {
                        if let EvalValue::Dict(c) = cond {
                            apply_conditional_fmt(
                                ws, data_start, ci as u16, data_end, ci as u16,
                                c, &col_letter, data_start,
                            )?;
                        }
                    }
                }
            }
        }

        //   Filas alternadas vía conditional format (no pisa formato de número)
        if alternar && data_end >= data_start {
            let alt_fmt = Format::new().set_background_color(Color::RGB(0xF2F7FC));
            let formula = format!("=MOD(ROW()-{},2)=0", header_row + 1);
            let cf = ConditionalFormatFormula::new()
                .set_rule(Formula::new(formula))
                .set_format(alt_fmt);
            ws.add_conditional_format(data_start, 0, data_end, last_col, &cf)
                .map_err(|e| format!("excel.write_styled alternar: {}", e))?;
        }

        //   Fila de totales
        if !totales_cols.is_empty() && data_end >= data_start {
            let totals_base = Format::new()
                .set_bold()
                .set_background_color(Color::RGB(0xE0E0E0));
            for (ci, key) in headers.iter().enumerate() {
                if let Some(&val) = totals.get(key) {
                    let mut tf = Format::new()
                        .set_bold()
                        .set_background_color(Color::RGB(0xE0E0E0));
                    if let Some(ref n) = col_num_fmts[ci] { tf = tf.set_num_format(n); }
                    write_cell_fmt(ws, cur, ci as u16, &EvalValue::Float(val), &tf)?;
                } else if ci == 0 {
                    ws.write_with_format(cur, 0, "TOTAL", &totals_base)
                        .map_err(|e| format!("excel.write_styled totals: {}", e))?;
                } else {
                    ws.write_with_format(cur, ci as u16, "", &totals_base)
                        .map_err(|e| format!("excel.write_styled totals: {}", e))?;
                }
            }
        }

        //   Freeze (congela hasta la fila de cabecera)
        if do_freeze {
            ws.set_freeze_panes(header_row + 1, 0)
                .map_err(|e| format!("excel.write_styled freeze: {}", e))?;
        }

        //   Autofilter en cabecera
        if do_filter && data_end >= data_start {
            ws.autofilter(header_row, 0, data_end, last_col)
                .map_err(|e| format!("excel.write_styled autofilter: {}", e))?;
        }

        //   Anchos de columna
        for (ci, key) in headers.iter().enumerate() {
            let w = match anchos.get(key) {
                Some(EvalValue::Int(n))   => *n as f64,
                Some(EvalValue::Float(f)) => *f,
                _ => 18.0,
            };
            ws.set_column_width(ci as u16, w)
                .map_err(|e| format!("excel.write_styled width: {}", e))?;
        }
    } // ws borrow termina aquí

    // ── Hojas de gráfico embebidas ────────────────────────────────────────────
    for (i, chart_cfg) in charts_cfgs.iter().enumerate() {
        let chart_sheet = cfg_str(chart_cfg, "sheet")
            .unwrap_or_else(|| format!("Gráfico {}", i + 1));
        let mut chart = build_chart_from_cfg(
            chart_cfg,
            &sheet_name,
            &headers,
            data_start_pos,
            data_end_pos,
            None,
        )?;
        let ws_c = wb.add_worksheet();
        ws_c.set_name(&chart_sheet)
            .map_err(|e| format!("excel.write_styled chart sheet: {}", e))?;
        ws_c.insert_chart(0, 0, &mut chart)
            .map_err(|e| format!("excel.write_styled chart insertar: {}", e))?;
    }

    wb.save(path).map_err(|e| format!("excel.write_styled: error guardando '{}': {}", path, e))?;
    Ok(EvalValue::Null)
}

fn apply_conditional_fmt(
    ws: &mut rust_xlsxwriter::Worksheet,
    r1: u32, c1: u16, r2: u32, c2: u16,
    cond: &HashMap<String, EvalValue>,
    col_letter: &str,
    data_start: u32,
) -> Result<(), String> {
    let op    = cfg_str(cond, "op").unwrap_or_else(|| "==".to_string());
    let valor = cond.get("valor").cloned().unwrap_or(EvalValue::Null);

    let mut fmt = Format::new();
    if let Some(bg) = cfg_color(cond, "fondo") { fmt = fmt.set_background_color(Color::RGB(bg)); }
    if let Some(fg) = cfg_color(cond, "texto") { fmt = fmt.set_font_color(Color::RGB(fg)); }
    if cfg_bool(cond, "bold") { fmt = fmt.set_bold(); }

    match &valor {
        EvalValue::Int(n)   => apply_numeric_cf(ws, r1, c1, r2, c2, &op, *n as f64, fmt),
        EvalValue::Float(f) => apply_numeric_cf(ws, r1, c1, r2, c2, &op, *f, fmt),
        EvalValue::Str(s)   => apply_text_cf(ws, r1, c1, r2, c2, &op, s, fmt, col_letter, data_start),
        _ => Ok(()),
    }
}

fn apply_numeric_cf(
    ws: &mut rust_xlsxwriter::Worksheet,
    r1: u32, c1: u16, r2: u32, c2: u16,
    op: &str, val: f64, fmt: Format,
) -> Result<(), String> {
    let rule = match op {
        "<"        => ConditionalFormatCellRule::LessThan(val),
        "<="       => ConditionalFormatCellRule::LessThanOrEqualTo(val),
        ">"        => ConditionalFormatCellRule::GreaterThan(val),
        ">="       => ConditionalFormatCellRule::GreaterThanOrEqualTo(val),
        "==" | "=" => ConditionalFormatCellRule::EqualTo(val),
        "!="       => ConditionalFormatCellRule::NotEqualTo(val),
        _          => return Ok(()),
    };
    ws.add_conditional_format(r1, c1, r2, c2,
        &ConditionalFormatCell::new().set_rule(rule).set_format(fmt))
        .map(|_| ())
        .map_err(|e| format!("excel: cf numérico: {}", e))
}

fn apply_text_cf(
    ws: &mut rust_xlsxwriter::Worksheet,
    r1: u32, c1: u16, r2: u32, c2: u16,
    op: &str, val: &str, fmt: Format,
    col_letter: &str, data_start: u32,
) -> Result<(), String> {
    match op {
        "contiene" | "contains" => {
            ws.add_conditional_format(r1, c1, r2, c2,
                &ConditionalFormatText::new()
                    .set_rule(ConditionalFormatTextRule::Contains(val.to_string()))
                    .set_format(fmt))
                .map(|_| ())
                .map_err(|e| format!("excel: cf texto: {}", e))
        }
        "empieza" | "starts_with" => {
            ws.add_conditional_format(r1, c1, r2, c2,
                &ConditionalFormatText::new()
                    .set_rule(ConditionalFormatTextRule::BeginsWith(val.to_string()))
                    .set_format(fmt))
                .map(|_| ())
                .map_err(|e| format!("excel: cf texto: {}", e))
        }
        "termina" | "ends_with" => {
            ws.add_conditional_format(r1, c1, r2, c2,
                &ConditionalFormatText::new()
                    .set_rule(ConditionalFormatTextRule::EndsWith(val.to_string()))
                    .set_format(fmt))
                .map(|_| ())
                .map_err(|e| format!("excel: cf texto: {}", e))
        }
        _ => {
            let esc = val.replace('"', "\"\"");
            let row_ref = data_start + 1;
            let formula = if op == "!=" {
                format!("=${}{}!=\"{}\"", col_letter, row_ref, esc)
            } else {
                format!("=${}{}=\"{}\"", col_letter, row_ref, esc)
            };
            ws.add_conditional_format(r1, c1, r2, c2,
                &ConditionalFormatFormula::new()
                    .set_rule(Formula::new(formula))
                    .set_format(fmt))
                .map(|_| ())
                .map_err(|e| format!("excel: cf fórmula: {}", e))
        }
    }
}

//    Data pipeline helpers                           

fn group_by_multi(
    rows: Vec<EvalValue>,
    campo: String,
    spec: HashMap<String, EvalValue>,
) -> Result<EvalValue, String> {
    // Agrupar filas por clave
    let mut groups: HashMap<String, Vec<EvalValue>> = HashMap::new();
    let mut key_order: Vec<String> = Vec::new();
    for row in rows {
        let k = to_str_val(&dict_get(&row, &campo));
        if !groups.contains_key(&k) { key_order.push(k.clone()); }
        groups.entry(k).or_default().push(row);
    }

    let result: Vec<EvalValue> = key_order.iter().map(|k| {
        let group = &groups[k];
        let mut m = HashMap::new();
        m.insert(campo.clone(), EvalValue::Str(k.clone()));

        for (col, agg_spec) in &spec {
            // "count": yes  →  añade "count"
            if col == "count" && matches!(agg_spec, EvalValue::Bool(true)) {
                m.insert("count".to_string(), EvalValue::Int(group.len() as i64));
                continue;
            }

            // recoger funciones pedidas
            let fns: Vec<String> = match agg_spec {
                EvalValue::List(l) => l.iter().map(|v| to_str_val(v)).collect(),
                EvalValue::Str(s)  => vec![s.clone()],
                EvalValue::Bool(true) if col != "count" => vec!["count".to_string()],
                _ => continue,
            };

            let vals: Vec<f64> = group.iter()
                .filter_map(|r| to_f64_val(&dict_get(r, col)))
                .collect();

            for f in &fns {
                let out_key = format!("{}_{}", col, f);
                let v = match f.as_str() {
                    "sum" => EvalValue::Float(vals.iter().sum()),
                    "avg" => {
                        if vals.is_empty() { EvalValue::Null }
                        else { EvalValue::Float(vals.iter().sum::<f64>() / vals.len() as f64) }
                    }
                    "max" => vals.iter().cloned().reduce(f64::max)
                        .map(EvalValue::Float).unwrap_or(EvalValue::Null),
                    "min" => vals.iter().cloned().reduce(f64::min)
                        .map(EvalValue::Float).unwrap_or(EvalValue::Null),
                    "count" => EvalValue::Int(vals.len() as i64),
                    "first" => group.first()
                        .map(|r| dict_get(r, col)).unwrap_or(EvalValue::Null),
                    "last"  => group.last()
                        .map(|r| dict_get(r, col)).unwrap_or(EvalValue::Null),
                    "std" => {
                        if vals.len() < 2 { EvalValue::Float(0.0) }
                        else {
                            let mean = vals.iter().sum::<f64>() / vals.len() as f64;
                            let var  = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                                       / vals.len() as f64;
                            EvalValue::Float(var.sqrt())
                        }
                    }
                    "median" => {
                        if vals.is_empty() { EvalValue::Null }
                        else {
                            let mut s = vals.clone();
                            s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                            let mid = s.len() / 2;
                            let med = if s.len() % 2 == 0 { (s[mid-1] + s[mid]) / 2.0 } else { s[mid] };
                            EvalValue::Float(med)
                        }
                    }
                    _ => EvalValue::Null,
                };
                m.insert(out_key, v);
            }
        }
        EvalValue::Dict(m)
    }).collect();

    Ok(EvalValue::List(result))
}

fn pivot_table(
    rows: Vec<EvalValue>,
    campo_fila: String,
    campo_col: String,
    campo_valor: String,
) -> Result<EvalValue, String> {
    let mut col_vals: Vec<String> = Vec::new();
    for row in &rows {
        let cv = to_str_val(&dict_get(row, &campo_col));
        if !col_vals.contains(&cv) { col_vals.push(cv); }
    }
    col_vals.sort();

    let mut pivot: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut row_order: Vec<String> = Vec::new();

    for row in &rows {
        let fv = to_str_val(&dict_get(row, &campo_fila));
        let cv = to_str_val(&dict_get(row, &campo_col));
        let vv = to_f64_val(&dict_get(row, &campo_valor)).unwrap_or(0.0);
        if !pivot.contains_key(&fv) { row_order.push(fv.clone()); }
        *pivot.entry(fv).or_default().entry(cv).or_insert(0.0) += vv;
    }

    let result: Vec<EvalValue> = row_order.iter().map(|fv| {
        let mut m = HashMap::new();
        m.insert(campo_fila.clone(), EvalValue::Str(fv.clone()));
        let col_data = pivot.get(fv).cloned().unwrap_or_default();
        for cv in &col_vals {
            m.insert(cv.clone(), EvalValue::Float(col_data.get(cv).copied().unwrap_or(0.0)));
        }
        EvalValue::Dict(m)
    }).collect();

    Ok(EvalValue::List(result))
}

//    join / dedupe / stats                           

fn join_multi(
    left: Vec<EvalValue>,
    right: Vec<EvalValue>,
    claves: Vec<String>,
    tipo: &str,
) -> Result<EvalValue, String> {
    // Clave compuesta: concat de valores separados por '\x00'
    let composite_key = |m: &HashMap<String, EvalValue>| -> String {
        claves.iter()
            .map(|c| to_str_val(m.get(c).unwrap_or(&EvalValue::Null)))
            .collect::<Vec<_>>()
            .join("\x00")
    };

    let mut right_map: HashMap<String, Vec<HashMap<String, EvalValue>>> = HashMap::new();
    for row in &right {
        if let EvalValue::Dict(m) = row {
            right_map.entry(composite_key(m)).or_default().push(m.clone());
        }
    }

    let mut result: Vec<EvalValue> = Vec::new();
    let mut matched_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    for lrow in &left {
        if let EvalValue::Dict(lm) = lrow {
            let k = composite_key(lm);
            match right_map.get(&k) {
                Some(rrows) => {
                    matched_keys.insert(k.clone());
                    for rm in rrows {
                        let mut merged = lm.clone();
                        for (rk, rv) in rm {
                            if !claves.contains(rk) { merged.insert(rk.clone(), rv.clone()); }
                        }
                        result.push(EvalValue::Dict(merged));
                    }
                }
                None => {
                    if tipo == "left" || tipo == "full" {
                        result.push(EvalValue::Dict(lm.clone()));
                    }
                }
            }
        }
    }

    if tipo == "right" || tipo == "full" {
        for row in &right {
            if let EvalValue::Dict(rm) = row {
                if !matched_keys.contains(&composite_key(rm)) {
                    result.push(EvalValue::Dict(rm.clone()));
                }
            }
        }
    }

    Ok(EvalValue::List(result))
}

fn deduplicate(rows: Vec<EvalValue>, campos: Option<Vec<String>>) -> Result<EvalValue, String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut result: Vec<EvalValue> = Vec::new();
    for row in rows {
        let key = match &row {
            EvalValue::Dict(m) => {
                match &campos {
                    Some(cols) => {
                        let mut parts: Vec<String> = cols.iter()
                            .map(|c| to_str_val(m.get(c).unwrap_or(&EvalValue::Null)))
                            .collect();
                        parts.join("||")
                    }
                    None => {
                        let mut pairs: Vec<String> = m.iter()
                            .map(|(k, v)| format!("{}={}", k, to_str_val(v)))
                            .collect();
                        pairs.sort();
                        pairs.join("||")
                    }
                }
            }
            other => to_str_val(other),
        };
        if seen.insert(key) {
            result.push(row);
        }
    }
    Ok(EvalValue::List(result))
}

fn compute_stats(rows: Vec<EvalValue>, campo: String) -> Result<EvalValue, String> {
    let mut vals: Vec<f64> = rows.iter()
        .filter_map(|r| to_f64_val(&dict_get(r, &campo)))
        .collect();

    if vals.is_empty() {
        let mut m = HashMap::new();
        m.insert("count".into(), EvalValue::Int(0));
        m.insert("sum".into(),   EvalValue::Float(0.0));
        m.insert("min".into(),   EvalValue::Null);
        m.insert("max".into(),   EvalValue::Null);
        m.insert("avg".into(),   EvalValue::Null);
        m.insert("std".into(),   EvalValue::Null);
        m.insert("mediana".into(), EvalValue::Null);
        return Ok(EvalValue::Dict(m));
    }

    let count = vals.len() as f64;
    let sum: f64 = vals.iter().sum();
    let avg = sum / count;
    let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let variance = vals.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / count;
    let std = variance.sqrt();

    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = vals.len() / 2;
    let mediana = if vals.len() % 2 == 0 {
        (vals[mid - 1] + vals[mid]) / 2.0
    } else {
        vals[mid]
    };

    let mut m = HashMap::new();
    m.insert("count".into(),   EvalValue::Int(vals.len() as i64));
    m.insert("sum".into(),     EvalValue::Float(sum));
    m.insert("min".into(),     EvalValue::Float(min));
    m.insert("max".into(),     EvalValue::Float(max));
    m.insert("avg".into(),     EvalValue::Float(avg));
    m.insert("std".into(),     EvalValue::Float(std));
    m.insert("mediana".into(), EvalValue::Float(mediana));
    Ok(EvalValue::Dict(m))
}

//    Tiny utilities                               

fn write_cell_fmt(
    ws: &mut rust_xlsxwriter::Worksheet,
    row: u32, col: u16,
    v: &EvalValue,
    fmt: &Format,
) -> Result<(), String> {
    let r = match v {
        EvalValue::Int(n)   => ws.write_with_format(row, col, *n, fmt),
        EvalValue::Float(f) => ws.write_with_format(row, col, *f, fmt),
        EvalValue::Bool(b)  => ws.write_with_format(row, col, *b, fmt),
        EvalValue::Null     => ws.write_with_format(row, col, "", fmt),
        other               => ws.write_with_format(row, col, other.to_string().as_str(), fmt),
    };
    r.map(|_| ()).map_err(|e| format!("excel: celda ({}, {}): {}", row, col, e))
}

fn compare_values(a: &EvalValue, op: &str, b: &EvalValue) -> bool {
    if let (Some(va), Some(vb)) = (to_f64_val(a), to_f64_val(b)) {
        return match op {
            ">"        => va > vb,
            "<"        => va < vb,
            ">="       => va >= vb,
            "<="       => va <= vb,
            "==" | "=" => (va - vb).abs() < f64::EPSILON,
            "!="       => (va - vb).abs() >= f64::EPSILON,
            _          => false,
        };
    }
    let (sa, sb) = (to_str_val(a), to_str_val(b));
    match op {
        "==" | "="               => sa == sb,
        "!="                     => sa != sb,
        ">"                      => sa > sb,
        "<"                      => sa < sb,
        "contiene" | "contains"  => sa.contains(&sb),
        "empieza" | "starts_with"=> sa.starts_with(&sb),
        "termina" | "ends_with"  => sa.ends_with(&sb),
        _                        => false,
    }
}

// "col+" → (col, false)  "col-" → (col, true)  "col" → (col, false)
fn parse_sort_key(s: &str) -> (String, bool) {
    if let Some(col) = s.strip_suffix('+') { (col.to_string(), false) }
    else if let Some(col) = s.strip_suffix('-') { (col.to_string(), true) }
    else { (s.to_string(), false) }
}

fn compare_eval_order(a: &EvalValue, b: &EvalValue) -> std::cmp::Ordering {
    if let (Some(va), Some(vb)) = (to_f64_val(a), to_f64_val(b)) {
        return va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal);
    }
    to_str_val(a).cmp(&to_str_val(b))
}

fn dict_get(row: &EvalValue, key: &str) -> EvalValue {
    match row {
        EvalValue::Dict(m) => m.get(key).cloned().unwrap_or(EvalValue::Null),
        _ => EvalValue::Null,
    }
}

fn col_to_letter(col: u16) -> String {
    let mut result = String::new();
    let mut c = col as u32 + 1;
    while c > 0 {
        let rem = (c - 1) % 26;
        result.insert(0, (b'A' + rem as u8) as char);
        c = (c - 1) / 26;
    }
    result
}

fn cfg_str(m: &HashMap<String, EvalValue>, key: &str) -> Option<String> {
    match m.get(key) { Some(EvalValue::Str(s)) => Some(s.clone()), _ => None }
}

fn cfg_bool(m: &HashMap<String, EvalValue>, key: &str) -> bool {
    matches!(m.get(key), Some(EvalValue::Bool(true)))
}

// Acepta "#RRGGBB" o [R, G, B] (0–255 cada canal)
fn cfg_color(m: &HashMap<String, EvalValue>, key: &str) -> Option<u32> {
    match m.get(key) {
        Some(EvalValue::Str(s)) => parse_hex_color(s),
        Some(EvalValue::List(l)) if l.len() == 3 => {
            let r = to_f64_val(l.get(0)?)? as u32;
            let g = to_f64_val(l.get(1)?)? as u32;
            let b = to_f64_val(l.get(2)?)? as u32;
            if r <= 255 && g <= 255 && b <= 255 { Some((r << 16) | (g << 8) | b) } else { None }
        }
        _ => None,
    }
}

fn parse_hex_color(s: &str) -> Option<u32> {
    let hex = s.trim_start_matches('#');
    if hex.len() == 6 { u32::from_str_radix(hex, 16).ok() } else { None }
}

fn to_str_val(v: &EvalValue) -> String {
    match v {
        EvalValue::Str(s)   => s.clone(),
        EvalValue::Int(n)   => n.to_string(),
        EvalValue::Float(f) => f.to_string(),
        EvalValue::Bool(b)  => b.to_string(),
        EvalValue::Null     => String::new(),
        other               => format!("{}", other),
    }
}

fn to_f64_val(v: &EvalValue) -> Option<f64> {
    match v {
        EvalValue::Int(n)   => Some(*n as f64),
        EvalValue::Float(f) => Some(*f),
        EvalValue::Str(s)   => s.parse::<f64>().ok(),
        _                   => None,
    }
}

// ─── excel.chart ─────────────────────────────────────────────────────────────
fn excel_chart_impl(
    path:   &str,
    rows:   Vec<EvalValue>,
    config: HashMap<String, EvalValue>,
) -> Result<EvalValue, String> {
    if rows.is_empty() {
        return Err("excel.chart: datos vacíos".into());
    }

    let data_sht  = cfg_str(&config, "data_sheet").unwrap_or_else(|| "Datos".to_string());
    let chart_sht = cfg_str(&config, "sheet").unwrap_or_else(|| "Gráfico".to_string());

    let goal = parse_goal(&config);

    let headers: Vec<String> = match rows.first() {
        Some(EvalValue::Dict(m)) => m.keys().cloned().collect(),
        _ => return Err("excel.chart: cada elemento debe ser un dict".into()),
    };

    let goal_col_idx = headers.len() as u16;
    let nrows        = rows.len() as u32;
    let first_row: u32 = 1;
    let last_row:  u32 = nrows;

    let mut wb = Workbook::new();

    {
        let ws = wb.add_worksheet();
        ws.set_name(&data_sht).map_err(|e| format!("excel.chart: {}", e))?;

        for (ci, h) in headers.iter().enumerate() {
            ws.write(0, ci as u16, h.as_str())
                .map_err(|e| format!("excel.chart encabezado: {}", e))?;
        }
        if let Some((_, ref lbl)) = goal {
            ws.write(0, goal_col_idx, lbl.as_str())
                .map_err(|e| format!("excel.chart encabezado meta: {}", e))?;
        }
        for (ri, row) in rows.iter().enumerate() {
            let cur = (ri as u32) + 1;
            if let EvalValue::Dict(m) = row {
                for (ci, key) in headers.iter().enumerate() {
                    write_cell(ws, cur, ci as u16, m.get(key).unwrap_or(&EvalValue::Null))?;
                }
                if let Some((gv, _)) = goal {
                    ws.write(cur, goal_col_idx, gv)
                        .map_err(|e| format!("excel.chart valor meta: {}", e))?;
                }
            }
        }
    }

    let goal_ref = goal.as_ref().map(|_| goal_col_idx);
    let mut chart = build_chart_from_cfg(&config, &data_sht, &headers, first_row, last_row, goal_ref)?;

    {
        let ws_c = wb.add_worksheet();
        ws_c.set_name(&chart_sht).map_err(|e| format!("excel.chart hoja: {}", e))?;
        ws_c.insert_chart(0, 0, &mut chart).map_err(|e| format!("excel.chart insertar: {}", e))?;
    }

    wb.save(path).map_err(|e| format!("excel.chart guardar '{}': {}", path, e))?;
    Ok(EvalValue::Str(path.to_string()))
}

// ─── Constructor de gráfico compartido ───────────────────────────────────────
// Usado por excel.chart y por el key `charts` de excel.write_styled.
//
// Parámetros:
//   cfg          — config del gráfico (type, x, y, title, palette, style, ...)
//   data_sheet   — nombre de la hoja donde están los datos
//   headers      — nombres de columnas en orden
//   first_row    — primera fila de datos (1-based, fila 0 = encabezados)
//   last_row     — última fila de datos  (inclusive)
//   goal_col_idx — Some(col) si hay línea de meta ya escrita en esa columna
fn build_chart_from_cfg(
    cfg:          &HashMap<String, EvalValue>,
    data_sheet:   &str,
    headers:      &[String],
    first_row:    u32,
    last_row:     u32,
    goal_col_idx: Option<u16>,
) -> Result<Chart, String> {
    let type_str = cfg_str(cfg, "type").unwrap_or_else(|| "bars".to_string());
    let x_col    = cfg_str(cfg, "x").ok_or("excel.chart: falta campo 'x'")?;
    let y_cols: Vec<String> = match cfg.get("y") {
        Some(EvalValue::List(l)) => l.iter().map(|v| to_str_val(v)).collect(),
        Some(EvalValue::Str(s))  => vec![s.clone()],
        _ => return Err("excel.chart: falta campo 'y'".into()),
    };
    if y_cols.is_empty() {
        return Err("excel.chart: 'y' no puede estar vacío".into());
    }

    let title     = cfg_str(cfg, "title").unwrap_or_default();
    let x_title   = cfg_str(cfg, "x_title").unwrap_or_default();
    let y_title   = cfg_str(cfg, "y_title").unwrap_or_default();
    let stacked   = cfg_bool(cfg, "stacked");
    let smooth    = cfg_bool(cfg, "smooth");
    let show_val  = cfg_bool(cfg, "show_values");
    let style_str = cfg_str(cfg, "style").unwrap_or_else(|| "minimal".to_string());

    let width_px: u32 = match cfg.get("width") {
        Some(EvalValue::Int(n)) => *n as u32, _ => 640,
    };
    let height_px: u32 = match cfg.get("height") {
        Some(EvalValue::Int(n)) => *n as u32, _ => 400,
    };

    let palette_name = cfg_str(cfg, "palette").unwrap_or_else(|| "orion".to_string());
    let custom_colors: Vec<u32> = match cfg.get("colors") {
        Some(EvalValue::List(l)) => l.iter().filter_map(|v| {
            if let EvalValue::Str(s) = v { parse_hex_color(s) } else { None }
        }).collect(),
        _ => vec![],
    };
    let colors = if !custom_colors.is_empty() { custom_colors } else { chart_palette(&palette_name) };

    // Índices de columnas
    let x_idx = headers.iter().position(|h| h == &x_col)
        .ok_or_else(|| format!("excel.chart: columna x '{}' no encontrada", x_col))? as u16;
    let y_indices: Vec<u16> = y_cols.iter().map(|col| {
        headers.iter().position(|h| h == col)
            .ok_or_else(|| format!("excel.chart: columna y '{}' no encontrada", col))
            .map(|i| i as u16)
    }).collect::<Result<Vec<_>, _>>()?;

    // Tipo de gráfico
    let ctype = match type_str.as_str() {
        "bars"    | "column"   | "columnas" => if stacked { ChartType::ColumnStacked } else { ChartType::Column },
        "hbars"   | "bar"      | "barras"   => if stacked { ChartType::BarStacked    } else { ChartType::Bar    },
        "lines"   | "line"     | "lineas"   => if stacked { ChartType::LineStacked   } else { ChartType::Line   },
        "area"                              => if stacked { ChartType::AreaStacked   } else { ChartType::Area   },
        "pie"     | "pastel"                => ChartType::Pie,
        "donut"   | "doughnut" | "dona"     => ChartType::Doughnut,
        "scatter" | "puntos"                => ChartType::Scatter,
        "radar"                             => ChartType::Radar,
        _                                   => ChartType::Column,
    };

    let mut chart = Chart::new(ctype);

    if !title.is_empty()   { chart.title().set_name(&title); }
    if !x_title.is_empty() { chart.x_axis().set_name(&x_title); }
    if !y_title.is_empty() { chart.y_axis().set_name(&y_title); }

    // Series de datos
    for (si, &y_idx) in y_indices.iter().enumerate() {
        let color = colors.get(si % colors.len()).copied().unwrap_or(0x2D5F8A);
        let series = chart.add_series()
            .set_categories((data_sheet, first_row, x_idx, last_row, x_idx))
            .set_values((data_sheet, first_row, y_idx, last_row, y_idx))
            .set_name(y_cols[si].as_str());
        series.set_format(
            ChartFormat::new().set_solid_fill(ChartSolidFill::new().set_color(Color::RGB(color)))
        );
        if smooth  { series.set_smooth(true); }
        if show_val { series.set_data_label(rust_xlsxwriter::ChartDataLabel::new().show_value()); }
    }

    // Línea de meta — línea roja punteada referenciando la columna goal ya escrita
    if let Some(gcol) = goal_col_idx {
        let goal_lbl = match cfg.get("goal") {
            Some(EvalValue::Dict(m)) => cfg_str(m, "label").unwrap_or_else(|| "Meta".to_string()),
            _ => "Meta".to_string(),
        };
        chart.add_series()
            .set_categories((data_sheet, first_row, x_idx,  last_row, x_idx))
            .set_values(    (data_sheet, first_row, gcol, last_row, gcol))
            .set_name(goal_lbl.as_str())
            .set_format(
                ChartFormat::new().set_line(
                    ChartLine::new()
                        .set_color(Color::RGB(0xE74C3C))
                        .set_dash_type(ChartLineDashType::Dash)
                )
            );
    }

    // Estilo Excel integrado (1–48) y dimensiones
    chart.set_style(chart_style_num(&style_str));
    chart.set_width(width_px).set_height(height_px);
    chart.legend().set_position(ChartLegendPosition::Bottom);

    Ok(chart)
}

// ─── Helpers de chart ─────────────────────────────────────────────────────────

// Parsea el campo `goal` del config en (valor_f64, etiqueta)
fn parse_goal(cfg: &HashMap<String, EvalValue>) -> Option<(f64, String)> {
    match cfg.get("goal") {
        Some(EvalValue::Dict(m)) => {
            let lbl = cfg_str(m, "label").unwrap_or_else(|| "Meta".to_string());
            match m.get("value") {
                Some(EvalValue::Float(f)) => Some((*f, lbl)),
                Some(EvalValue::Int(i))   => Some((*i as f64, lbl)),
                _ => None,
            }
        }
        Some(EvalValue::Float(f)) => Some((*f, "Meta".to_string())),
        Some(EvalValue::Int(i))   => Some((*i as f64, "Meta".to_string())),
        _ => None,
    }
}

// Mapea nombres de estilo a números de estilo Excel integrado (1–48)
fn chart_style_num(name: &str) -> u8 {
    match name {
        "minimal"    | "clean"    => 2,
        "light"                   => 3,
        "neutral"                 => 4,
        "corporate"               => 26,
        "dark"                    => 8,
        "vivid"      | "colorful" => 34,
        "monochrome"              => 20,
        _                         => 2,
    }
}

// Paletas de colores predefinidas — 6 colores por paleta
fn chart_palette(name: &str) -> Vec<u32> {
    match name {
        "orion"     => vec![0x2D5F8A, 0xE67E22, 0x27AE60, 0x8E44AD, 0xE74C3C, 0x1ABC9C],
        "ocean"     => vec![0x023E8A, 0x0077B6, 0x00B4D8, 0x48CAE4, 0x90E0EF, 0x0096C7],
        "vivid"     => vec![0xFF6B6B, 0xFFBF00, 0x4ECDC4, 0x45B7D1, 0xFF9F43, 0x96CEB4],
        "corporate" => vec![0x003F5C, 0x2F4B7C, 0x665191, 0xA05195, 0xD45087, 0xF95D6A],
        "sunset"    => vec![0xF72585, 0xB5179E, 0x7209B7, 0x3A0CA3, 0x4361EE, 0x4CC9F0],
        "pastel"    => vec![0xFFB3BA, 0xFFDFBA, 0xBAFFBA, 0xBAE1FF, 0xD4BAFF, 0xFFBAD2],
        "dark"      => vec![0x2C3E50, 0xE74C3C, 0x3498DB, 0x2ECC71, 0xF39C12, 0x9B59B6],
        _           => vec![0x2D5F8A, 0xE67E22, 0x27AE60, 0x8E44AD, 0xE74C3C, 0x1ABC9C],
    }
}

// ─── Generador de fórmulas Excel ─────────────────────────────────────────────
// Convierte un descriptor { _f, col, ... } en una string de fórmula Excel.
//
// excel_row     — fila actual en notación Excel (1-based)
// col_map       — nombre de columna → índice de columna (0-based xlsxwriter)
// data_start    — primera fila de datos en Excel (1-based)
// data_end      — última fila de datos en Excel  (1-based)
fn generate_formula_str(
    desc:       &HashMap<String, EvalValue>,
    excel_row:  u32,
    col_map:    &HashMap<String, u16>,
    data_start: u32,
    data_end:   u32,
) -> Result<String, String> {
    let f_type = match desc.get("_f") {
        Some(EvalValue::Str(s)) => s.as_str(),
        _ => return Err("Descriptor de fórmula inválido: falta '_f'".into()),
    };

    // Resuelve nombre de columna → letra Excel
    let col_letter = |key: &str| -> Result<String, String> {
        let name = match desc.get(key) {
            Some(EvalValue::Str(s)) => s.clone(),
            _ => return Err(format!("Fórmula '{}': campo '{}' requerido", f_type, key)),
        };
        let idx = col_map.get(&name)
            .ok_or_else(|| format!("Fórmula '{}': columna '{}' no encontrada", f_type, name))?;
        Ok(col_to_letter(*idx))
    };

    let get_num = |key: &str| -> Option<f64> {
        match desc.get(key) {
            Some(EvalValue::Float(f)) => Some(*f),
            Some(EvalValue::Int(i))   => Some(*i as f64),
            _ => None,
        }
    };

    // Formatea un EvalValue como literal Excel (string con comillas, número, bool)
    let lit = |v: &EvalValue| -> String {
        match v {
            EvalValue::Str(s)        => format!("\"{}\"", s.replace('"', "\"\"")),
            EvalValue::Int(i)        => i.to_string(),
            EvalValue::Float(f)      => format!("{}", f),
            EvalValue::Bool(true)    => "TRUE".into(),
            EvalValue::Bool(false)   => "FALSE".into(),
            _                        => "\"\"".into(),
        }
    };

    match f_type {
        // =B6*0.05
        "pct" => {
            let cl  = col_letter("col")?;
            let pct = get_num("val").unwrap_or(100.0);
            Ok(format!("={}{}*{}", cl, excel_row, pct / 100.0))
        }
        // =B6/C6
        "ratio" => {
            let cl1 = col_letter("col")?;
            let cl2 = col_letter("col2")?;
            Ok(format!("={}{}/{}{}", cl1, excel_row, cl2, excel_row))
        }
        // =B6-C6
        "diff" => {
            let cl1 = col_letter("col")?;
            let cl2 = col_letter("col2")?;
            Ok(format!("={}{}-{}{}", cl1, excel_row, cl2, excel_row))
        }
        // =B6*1.19
        "mul" => {
            let cl     = col_letter("col")?;
            let factor = get_num("val").unwrap_or(1.0);
            Ok(format!("={}{}*{}", cl, excel_row, factor))
        }
        // =SUM($B$2:$B$10)  — igual en todas las filas
        "sum" => {
            let cl = col_letter("col")?;
            Ok(format!("=SUM(${0}${1}:${0}${2})", cl, data_start, data_end))
        }
        // =AVERAGE($B$2:$B$10)
        "avg" => {
            let cl = col_letter("col")?;
            Ok(format!("=AVERAGE(${0}${1}:${0}${2})", cl, data_start, data_end))
        }
        // =SUM($B$2:B6)  — se expande fila a fila
        "cumulative" => {
            let cl = col_letter("col")?;
            Ok(format!("=SUM(${0}${1}:{0}{2})", cl, data_start, excel_row))
        }
        // =RANK(B6,$B$2:$B$10,0)
        "rank" => {
            let cl    = col_letter("col")?;
            let order = match desc.get("dir") {
                Some(EvalValue::Str(s)) if s == "asc" => 1,
                _ => 0,
            };
            Ok(format!("=RANK({0}{1},${0}${2}:${0}${3},{4})",
                cl, excel_row, data_start, data_end, order))
        }
        // =IF(B6>80000,"A","B")
        "if" => {
            let cl   = col_letter("col")?;
            let op   = match desc.get("op") {
                Some(EvalValue::Str(s)) => s.replace("==", "=").replace("!=", "<>"),
                _ => ">".into(),
            };
            let val   = desc.get("val").unwrap_or(&EvalValue::Null);
            let then  = desc.get("then").unwrap_or(&EvalValue::Null);
            let else_ = desc.get("else_").unwrap_or(&EvalValue::Null);
            Ok(format!("=IF({}{}{}{},{},{})",
                cl, excel_row, op, lit(val), lit(then), lit(else_)))
        }
        t => Err(format!("Tipo de fórmula '{}' no reconocido internamente", t)),
    }
}

// Parsea un string de fecha en el formato dado y devuelve ISO "YYYY-MM-DD".
// Formatos soportados: "DD/MM/YYYY" "MM/DD/YYYY" "YYYY-MM-DD" "auto" + formato genérico.
fn parse_date_str(s: &str, fmt: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() { return None; }

    let date = match fmt {
        "DD/MM/YYYY" => NaiveDate::parse_from_str(s, "%d/%m/%Y").ok(),
        "MM/DD/YYYY" => NaiveDate::parse_from_str(s, "%m/%d/%Y").ok(),
        "YYYY-MM-DD" => NaiveDate::parse_from_str(s, "%Y-%m-%d").ok(),
        "auto" => {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .or_else(|_| NaiveDate::parse_from_str(s, "%d/%m/%Y"))
                .or_else(|_| NaiveDate::parse_from_str(s, "%m/%d/%Y"))
                .or_else(|_| NaiveDate::parse_from_str(s, "%d-%m-%Y"))
                .or_else(|_| NaiveDate::parse_from_str(s, "%Y/%m/%d"))
                .or_else(|_| NaiveDate::parse_from_str(s, "%d.%m.%Y"))
                .ok()
        }
        // Formato genérico: convierte notación Orion a strftime
        other => {
            let chrono_fmt = other
                .replace("DD", "%d")
                .replace("MM", "%m")
                .replace("YYYY", "%Y")
                .replace("YY", "%y");
            NaiveDate::parse_from_str(s, &chrono_fmt).ok()
        }
    };

    date.map(|d| d.format("%Y-%m-%d").to_string())
}
