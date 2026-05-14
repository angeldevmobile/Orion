use crate::eval_value::EvalValue;
use std::collections::HashMap;
use calamine::{Reader, open_workbook_auto, Data};
use rust_xlsxwriter::{Workbook, Format, Color};

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
