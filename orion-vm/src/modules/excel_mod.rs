use crate::eval_value::EvalValue;
use std::collections::HashMap;
use calamine::{Reader, open_workbook_auto, Data};
use rust_xlsxwriter::{Workbook, Format, Color, Formula};
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

        // ── write_styled ─────────────────────────────────────────────────────────
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

        // ── Data pipeline ─────────────────────────────────────────────────────────

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
        "agrupar" | "group" => {
            if args.len() < 2 {
                return Err("excel.agrupar requiere (datos, campo, config?)".into());
            }
            let rows  = list_arg("agrupar", &args, 0)?;
            let campo = str_arg("agrupar", &args, 1)?;
            let cfg   = match args.get(2) {
                Some(EvalValue::Dict(m)) => m.clone(),
                _ => HashMap::new(),
            };
            group_by(rows, campo, cfg)
        }

        // ordenar(datos, campo, dir?) → sorted — dir: "asc" (default) | "desc"
        "ordenar" | "sort_by" => {
            if args.len() < 2 {
                return Err("excel.ordenar requiere (datos, campo, dir?)".into());
            }
            let mut rows = list_arg("ordenar", &args, 0)?;
            let campo    = str_arg("ordenar", &args, 1)?;
            let desc     = matches!(args.get(2), Some(EvalValue::Str(s)) if s == "desc");
            rows.sort_by(|a, b| {
                let va = dict_get(a, &campo);
                let vb = dict_get(b, &campo);
                let ord = compare_eval_order(&va, &vb);
                if desc { ord.reverse() } else { ord }
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

// ─── write_styled ────────────────────────────────────────────────────────────

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

        // ── Título (fila mergeada)
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
        cur += 1;
        let data_start = cur;

        // ── Filas de datos
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
                cur += 1;
            }
        }
        let data_end = cur.saturating_sub(1);

        // ── Formato condicional por columna
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

        // ── Filas alternadas vía conditional format (no pisa formato de número)
        if alternar && data_end >= data_start {
            let alt_fmt = Format::new().set_background_color(Color::RGB(0xF2F7FC));
            let formula = format!("=MOD(ROW()-{},2)=0", header_row + 1);
            let cf = ConditionalFormatFormula::new()
                .set_rule(Formula::new(formula))
                .set_format(alt_fmt);
            ws.add_conditional_format(data_start, 0, data_end, last_col, &cf)
                .map_err(|e| format!("excel.write_styled alternar: {}", e))?;
        }

        // ── Fila de totales
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

        // ── Freeze (congela hasta la fila de cabecera)
        if do_freeze {
            ws.set_freeze_panes(header_row + 1, 0)
                .map_err(|e| format!("excel.write_styled freeze: {}", e))?;
        }

        // ── Autofilter en cabecera
        if do_filter && data_end >= data_start {
            ws.autofilter(header_row, 0, data_end, last_col)
                .map_err(|e| format!("excel.write_styled autofilter: {}", e))?;
        }

        // ── Anchos de columna
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

// ─── Data pipeline helpers ────────────────────────────────────────────────────

fn group_by(
    rows: Vec<EvalValue>,
    campo: String,
    cfg: HashMap<String, EvalValue>,
) -> Result<EvalValue, String> {
    let do_conteo  = cfg_bool(&cfg, "conteo");
    let cols_suma: Vec<String> = match cfg.get("suma") {
        Some(EvalValue::List(l)) => l.iter().map(|v| to_str_val(v)).collect(),
        _ => vec![],
    };
    let cols_prom: Vec<String> = match cfg.get("promedio") {
        Some(EvalValue::List(l)) => l.iter().map(|v| to_str_val(v)).collect(),
        _ => vec![],
    };

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
        if do_conteo {
            m.insert("conteo".to_string(), EvalValue::Int(group.len() as i64));
        }
        for col in &cols_suma {
            let s: f64 = group.iter()
                .map(|r| to_f64_val(&dict_get(r, col)).unwrap_or(0.0))
                .sum();
            m.insert(format!("{}_suma", col), EvalValue::Float(s));
        }
        for col in &cols_prom {
            let vals: Vec<f64> = group.iter()
                .filter_map(|r| to_f64_val(&dict_get(r, col)))
                .collect();
            let avg = if vals.is_empty() { 0.0 }
                      else { vals.iter().sum::<f64>() / vals.len() as f64 };
            m.insert(format!("{}_promedio", col), EvalValue::Float(avg));
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

// ─── Tiny utilities ───────────────────────────────────────────────────────────

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
