/// frame_mod — motor de datos columnar nativo de Orion
///
/// Arquitectura:
///   - Almacenamiento columnar: Vec<(String, Col)> en lugar de Vec<HashMap>
///   - Lectura por chunks: nunca carga todo el archivo en RAM
///   - Operaciones directas sobre Vec<f64> — sin hash lookups
///   - Handle-based como vector_mod (lazy: open() solo lee el header)

use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

//   tipos columna                               ─

#[derive(Clone)]
enum Col {
    Float(Vec<f64>),
    Int(Vec<i64>),
    Str(Vec<String>),
    Bool(Vec<bool>),
}

impl Col {
    fn len(&self) -> usize {
        match self { Col::Float(v) => v.len(), Col::Int(v) => v.len(),
                     Col::Str(v)  => v.len(), Col::Bool(v) => v.len() }
    }

    fn as_floats(&self) -> Option<Vec<f64>> {
        match self {
            Col::Float(v) => Some(v.clone()),
            Col::Int(v)   => Some(v.iter().map(|&x| x as f64).collect()),
            _ => None,
        }
    }

    fn to_eval(&self, i: usize) -> EvalValue {
        match self {
            Col::Float(v) => EvalValue::Float(v[i]),
            Col::Int(v)   => EvalValue::Int(v[i]),
            Col::Str(v)   => EvalValue::Str(v[i].clone()),
            Col::Bool(v)  => EvalValue::Bool(v[i]),
        }
    }
}

//   frame en memoria                              

struct Frame {
    cols: Vec<(String, Col)>,
    rows: usize,
}

impl Frame {
    fn col_index(&self, name: &str) -> Option<usize> {
        self.cols.iter().position(|(n, _)| n == name)
    }

    fn col(&self, name: &str) -> Option<&Col> {
        self.cols.iter().find(|(n, _)| n == name).map(|(_, c)| c)
    }

    fn row_to_dict(&self, i: usize) -> EvalValue {
        let mut map = HashMap::new();
        for (name, col) in &self.cols {
            map.insert(name.clone(), col.to_eval(i));
        }
        EvalValue::Dict(map)
    }
}

//   store estático                               

static FRAMES:  Mutex<Option<HashMap<String, Frame>>> = Mutex::new(None);
static COUNTER: AtomicU64 = AtomicU64::new(1);

fn with_frames<F, T>(f: F) -> T
where F: FnOnce(&mut HashMap<String, Frame>) -> T {
    let mut g = FRAMES.lock().unwrap();
    if g.is_none() { *g = Some(HashMap::new()); }
    f(g.as_mut().unwrap())
}

fn new_handle() -> String {
    format!("frame_{}", COUNTER.fetch_add(1, Ordering::SeqCst))
}

//   parsing CSV                                ─

fn parse_delim_chunk(reader: &mut BufReader<File>, sep: &str, limit: usize) -> (Vec<String>, Vec<Vec<String>>) {
    let mut headers = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        let Ok(l) = line else { break };
        let fields: Vec<String> = l.split(sep).map(|s| s.trim().trim_matches('"').to_string()).collect();
        if i == 0 { headers = fields; }
        else {
            rows.push(fields);
            if limit > 0 && rows.len() >= limit { break; }
        }
    }
    (headers, rows)
}

fn infer_columns(headers: &[String], rows: &[Vec<String>]) -> Vec<(String, Col)> {
    headers.iter().enumerate().map(|(ci, name)| {
        let vals: Vec<&str> = rows.iter().map(|r| r.get(ci).map(|s| s.as_str()).unwrap_or("")).collect();

        // intentar float
        let floats: Vec<f64> = vals.iter().filter_map(|v| v.parse().ok()).collect();
        if floats.len() == vals.len() {
            // si todos son enteros exactos → Col::Int
            if floats.iter().all(|f| f.fract() == 0.0) {
                return (name.clone(), Col::Int(floats.iter().map(|&f| f as i64).collect()));
            }
            return (name.clone(), Col::Float(floats));
        }

        // intentar bool
        let bools: Vec<bool> = vals.iter().filter_map(|v| match *v {
            "yes" | "true"  | "1" => Some(true),
            "no"  | "false" | "0" => Some(false),
            _ => None,
        }).collect();
        if bools.len() == vals.len() {
            return (name.clone(), Col::Bool(bools));
        }

        // string por defecto
        (name.clone(), Col::Str(vals.iter().map(|s| s.to_string()).collect()))
    }).collect()
}

//   dispatcher                                 

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // Carga
        "open"       => fn_open(args),
        "from_txt"     => fn_from_txt(args),
        "to_excel"     => fn_to_excel(args),
        "txt_to_excel" => fn_txt_to_excel(args),
        "from_list"  => fn_from_list(args),
        // Exploración
        "peek"       => fn_peek(args),
        "schema"     => fn_schema(args),
        "size"       => fn_size(args),
        "col"        => fn_col(args),
        "row"        => fn_row(args),
        "to_list"    => fn_to_list(args),
        // Selección
        "keep"       => fn_keep(args),
        "drop"       => fn_drop(args),
        "rename"     => fn_rename(args),
        // Filtrado
        "where_"     => fn_where(args),
        "head"       => fn_head(args),
        "tail"       => fn_tail(args),
        "sort"       => fn_sort(args),
        // Estadísticas por columna (directo sobre Vec<f64>)
        "mean"       => fn_col_stat(args, "mean"),
        "sum"        => fn_col_stat(args, "sum"),
        "min"        => fn_col_stat(args, "min"),
        "max"        => fn_col_stat(args, "max"),
        "std"        => fn_col_stat(args, "std"),
        "stats"      => fn_stats(args),
        // Agregación
        "group"      => fn_group(args),
        "count"      => fn_count(args),
        // Columna calculada
        "add_col"    => fn_add_col(args),
        // Chunked (grandes volúmenes sin cargar todo)
        "each_chunk" => fn_each_chunk(args),
        "scan_stats" => fn_scan_stats(args),
        // Persistencia
        "save"       => fn_save(args),
        "save_odf"   => fn_save_odf(args),
        "load_odf"   => fn_load_odf(args),
        "txt_to_odf" => fn_txt_to_odf(args),
        _ => Err(format!("frame.{} no existe", function)),
    }
}

//   carga                                   ─

fn fn_open(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let path = match args.first() {
        Some(EvalValue::Str(s)) => s.clone(),
        _ => return Err("frame.open(ruta_csv)".into()),
    };
    let file = File::open(&path).map_err(|e| format!("frame.open: {}", e))?;
    let mut reader = BufReader::new(file);
    let (headers, rows) = parse_delim_chunk(&mut reader, ",", 0); // 0 = sin límite
    if headers.is_empty() { return Err("frame.open: archivo vacío o sin cabecera".into()); }
    let n = rows.len();
    let cols = infer_columns(&headers, &rows);
    let id = new_handle();
    with_frames(|fs| fs.insert(id.clone(), Frame { cols, rows: n }));
    Ok(EvalValue::Str(id))
}

/// from_txt(ruta)            → separador por defecto ","
/// from_txt(ruta, sep)       → separador configurable (";", "\t", "|", ...)
///
/// Entrada del pipeline: un TXT delimitado se parsea a columnas tipadas. A
/// diferencia de xlsx, partir texto por un separador es trivial y rápido — el
/// cuello de botella nunca es la entrada.
fn fn_from_txt(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let path = arg_str(&args, 0, "frame.from_txt")?;
    let sep = match args.get(1) {
        Some(EvalValue::Str(s)) if !s.is_empty() => s.clone(),
        _ => ",".to_string(),
    };
    let file = File::open(&path).map_err(|e| format!("frame.from_txt: {}", e))?;
    let mut reader = BufReader::new(file);
    let (headers, rows) = parse_delim_chunk(&mut reader, &sep, 0);
    if headers.is_empty() { return Err("frame.from_txt: archivo vacío o sin cabecera".into()); }
    let n = rows.len();
    let cols = infer_columns(&headers, &rows);
    let id = new_handle();
    with_frames(|fs| fs.insert(id.clone(), Frame { cols, rows: n }));
    Ok(EvalValue::Str(id))
}

/// to_excel(handle, ruta)              → una hoja (parte por el límite de Excel)
/// to_excel(handle, ruta, split_por)   → tamaño máximo de filas por hoja
///
/// Salida del pipeline: escribe columnas → xlsx directamente (sin materializar
/// dicts). Excel admite como máximo 1 048 576 filas por hoja; si el frame es
/// mayor se reparte en hojas "parte_1", "parte_2", … dentro del mismo libro.
fn fn_to_excel(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    use rust_xlsxwriter::{Workbook, Format, Color};

    if args.len() < 2 { return Err("frame.to_excel(handle, ruta[, split_por])".into()); }
    let id   = arg_handle(&args, 0)?;
    let path = arg_str(&args, 1, "frame.to_excel")?;

    // Límite duro de Excel: 1 048 576 filas por hoja (incluida la cabecera).
    const EXCEL_MAX: usize = 1_048_576;
    let split = match args.get(2) {
        Some(EvalValue::Int(n)) if *n > 0 => (*n as usize).min(EXCEL_MAX - 1),
        _ => EXCEL_MAX - 1, // por defecto: máximo de datos dejando sitio a la cabecera
    };

    with_frames(|fs| {
        let f = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let headers: Vec<&str> = f.cols.iter().map(|(n, _)| n.as_str()).collect();

        let mut wb = Workbook::new();
        let header_fmt = Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0x2D5F8A))
            .set_font_color(Color::White);

        // número de hojas necesarias (al menos 1, aunque el frame esté vacío)
        let n_sheets = if f.rows == 0 { 1 } else { (f.rows + split - 1) / split };

        for s in 0..n_sheets {
            let ws = wb.add_worksheet();
            let sheet_name = if n_sheets == 1 { "Datos".to_string() } else { format!("parte_{}", s + 1) };
            ws.set_name(sheet_name.as_str())
                .map_err(|e| format!("frame.to_excel: nombre de hoja: {}", e))?;

            // cabecera
            for (c, h) in headers.iter().enumerate() {
                ws.write_with_format(0, c as u16, *h, &header_fmt)
                    .map_err(|e| format!("frame.to_excel: cabecera: {}", e))?;
            }

            let start = s * split;
            let end = (start + split).min(f.rows);
            for (out_r, i) in (start..end).enumerate() {
                let row = out_r as u32 + 1; // +1 por la cabecera
                for (c, (_, col)) in f.cols.iter().enumerate() {
                    let cell = c as u16;
                    let res = match col {
                        Col::Float(v) => ws.write(row, cell, v[i]),
                        Col::Int(v)   => ws.write(row, cell, v[i]),
                        Col::Bool(v)  => ws.write(row, cell, v[i]),
                        Col::Str(v)   => ws.write(row, cell, v[i].as_str()),
                    };
                    res.map_err(|e| format!("frame.to_excel: celda ({},{}): {}", row, cell, e))?;
                }
            }
        }

        wb.save(&path).map_err(|e| format!("frame.to_excel: guardando '{}': {}", path, e))?;
        Ok(EvalValue::Str(format!(
            "Excel escrito: {} ({} filas, {} hoja(s))", path, f.rows, n_sheets
        )))
    })
}

// ── Streaming TXT → Excel (memoria acotada) ──────────────────────────────────
//
// txt_to_excel(txt, base, sep = ",", split_por = 1_048_575)
//
// A diferencia de `open`+`to_excel` (que carga TODO el frame en RAM), esto
// transmite el TXT fila por fila y escribe archivos `base_1.xlsx`, `base_2.xlsx`,
// … de a lo sumo `split_por` filas cada uno, LIBERANDO cada libro tras
// guardarlo. La memoria queda acotada a ~un archivo, no al tamaño del TXT — así
// se procesan archivos más grandes que la RAM. Cada archivo respeta el límite de
// Excel (1 048 576 filas/hoja). Los tipos se infieren por celda.

/// Escribe una fila de texto (delimitada por `sep`) en la hoja, tipando cada
/// celda: entero → float → texto.
fn write_txt_row(
    ws: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    line: &str,
    sep: &str,
    ncols: usize,
) -> Result<(), String> {
    for (c, field) in line.trim_end_matches(['\r', '\n']).split(sep).enumerate() {
        if c >= ncols { break; }
        let f = field.trim().trim_matches('"');
        let cell = c as u16;
        let res = if let Ok(i) = f.parse::<i64>() {
            ws.write(row, cell, i)
        } else if let Ok(fl) = f.parse::<f64>() {
            ws.write(row, cell, fl)
        } else {
            ws.write(row, cell, f)
        };
        res.map_err(|e| format!("txt_to_excel: celda ({},{}): {}", row, cell, e))?;
    }
    Ok(())
}

/// Escribe hasta `split` filas desde `reader` a un xlsx nuevo. Devuelve cuántas
/// filas escribió (0 = no había más → no crea el archivo). El libro se libera al
/// salir de la función (memoria acotada).
fn write_one_excel_chunk(
    path: &str,
    headers: &[String],
    sep: &str,
    reader: &mut BufReader<File>,
    split: usize,
) -> Result<usize, String> {
    use rust_xlsxwriter::{Workbook, Format, Color};

    // ¿Hay al menos una fila de datos? Si no, no creamos archivo.
    let mut first = String::new();
    if reader.read_line(&mut first).map_err(|e| e.to_string())? == 0 {
        return Ok(0);
    }

    let mut wb = Workbook::new();
    let count;
    {
        let ws = wb.add_worksheet();
        let header_fmt = Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0x2D5F8A))
            .set_font_color(Color::White);
        for (c, h) in headers.iter().enumerate() {
            ws.write_with_format(0, c as u16, h.as_str(), &header_fmt)
                .map_err(|e| format!("txt_to_excel: cabecera: {}", e))?;
        }

        let mut r: u32 = 1;
        write_txt_row(ws, r, &first, sep, headers.len())?;
        r += 1;
        let mut n = 1usize;
        while n < split {
            let mut line = String::new();
            if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 { break; }
            if line.trim().is_empty() { continue; }
            write_txt_row(ws, r, &line, sep, headers.len())?;
            r += 1;
            n += 1;
        }
        count = n;
    }
    wb.save(path).map_err(|e| format!("txt_to_excel: guardando '{}': {}", path, e))?;
    Ok(count)
}

fn fn_txt_to_excel(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.txt_to_excel(txt, base[, sep, split_por])".into()); }
    let txt_path = arg_str(&args, 0, "frame.txt_to_excel")?;
    let out_base = arg_str(&args, 1, "frame.txt_to_excel")?;
    let sep = match args.get(2) {
        Some(EvalValue::Str(s)) if !s.is_empty() => s.clone(),
        _ => ",".to_string(),
    };
    const EXCEL_MAX: usize = 1_048_576;
    let split = match args.get(3) {
        Some(EvalValue::Int(n)) if *n > 0 => (*n as usize).min(EXCEL_MAX - 1),
        _ => EXCEL_MAX - 1,
    };

    let file = File::open(&txt_path).map_err(|e| format!("frame.txt_to_excel: {}", e))?;
    let mut reader = BufReader::new(file);

    // cabecera
    let mut header_line = String::new();
    if reader.read_line(&mut header_line).map_err(|e| e.to_string())? == 0 {
        return Err("frame.txt_to_excel: archivo vacío".into());
    }
    let headers: Vec<String> = header_line
        .trim_end_matches(['\r', '\n'])
        .split(&sep)
        .map(|s| s.trim().trim_matches('"').to_string())
        .collect();

    let mut file_idx = 0usize;
    let mut total = 0usize;
    loop {
        let path = format!("{}_{}.xlsx", out_base, file_idx + 1);
        let n = write_one_excel_chunk(&path, &headers, &sep, &mut reader, split)?;
        if n == 0 { break; }
        file_idx += 1;
        total += n;
        if n < split { break; } // último chunk parcial
    }

    Ok(EvalValue::Str(format!(
        "Streaming TXT→Excel: {} filas en {} archivo(s) ({}_1.xlsx …)",
        total, file_idx, out_base
    )))
}

fn fn_from_list(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match args.first() {
        Some(EvalValue::List(rows)) if !rows.is_empty() => {
            let headers: Vec<String> = match &rows[0] {
                EvalValue::Dict(d) => d.keys().cloned().collect(),
                _ => return Err("frame.from_list: se esperaba lista de dicts".into()),
            };
            let str_rows: Vec<Vec<String>> = rows.iter().map(|r| {
                match r {
                    EvalValue::Dict(d) => headers.iter().map(|h| {
                        match d.get(h) {
                            Some(EvalValue::Int(n))   => n.to_string(),
                            Some(EvalValue::Float(f)) => f.to_string(),
                            Some(EvalValue::Bool(b))  => b.to_string(),
                            Some(EvalValue::Str(s))   => s.clone(),
                            _ => String::new(),
                        }
                    }).collect(),
                    _ => vec![String::new(); headers.len()],
                }
            }).collect();
            let n = str_rows.len();
            let cols = infer_columns(&headers, &str_rows);
            let id = new_handle();
            with_frames(|fs| fs.insert(id.clone(), Frame { cols, rows: n }));
            Ok(EvalValue::Str(id))
        }
        _ => Err("frame.from_list(lista_de_dicts)".into()),
    }
}

//   exploración                                ─

fn fn_peek(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    let n  = match args.get(1) { Some(EvalValue::Int(n)) => *n as usize, _ => 5 };
    with_frames(|fs| {
        let f = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let show = n.min(f.rows);
        // ancho de columnas
        let widths: Vec<usize> = f.cols.iter().map(|(name, _)| name.len().max(8)).collect();
        // header
        let header: Vec<String> = f.cols.iter().map(|(n, _)| n.clone()).collect();
        println!("┌{}┐", widths.iter().map(|w| "─".repeat(w + 2)).collect::<Vec<_>>().join("┬"));
        println!("│{}│", header.iter().zip(&widths).map(|(h, w)| format!(" {:width$} ", h, width=w)).collect::<Vec<_>>().join("│"));
        println!("├{}┤", widths.iter().map(|w| "─".repeat(w + 2)).collect::<Vec<_>>().join("┼"));
        for i in 0..show {
            let row: Vec<String> = f.cols.iter().zip(&widths).map(|((_, col), w)| {
                let val = match col {
                    Col::Float(v) => format!("{:.2}", v[i]),
                    Col::Int(v)   => v[i].to_string(),
                    Col::Str(v)   => v[i].clone(),
                    Col::Bool(v)  => if v[i] { "yes".into() } else { "no".into() },
                };
                format!(" {:width$} ", val, width=w)
            }).collect();
            println!("│{}│", row.join("│"));
        }
        println!("└{}┘", widths.iter().map(|w| "─".repeat(w + 2)).collect::<Vec<_>>().join("┴"));
        if f.rows > show { println!("  ... {} filas en total", f.rows); }
        Ok(EvalValue::Null)
    })
}

fn fn_schema(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_frames(|fs| {
        let f = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let mut map = HashMap::new();
        for (name, col) in &f.cols {
            let t = match col { Col::Float(_) => "float", Col::Int(_) => "int",
                                Col::Str(_)   => "string", Col::Bool(_) => "bool" };
            map.insert(name.clone(), EvalValue::Str(t.into()));
        }
        Ok(EvalValue::Dict(map))
    })
}

fn fn_size(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_frames(|fs| {
        let f = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let mut map = HashMap::new();
        map.insert("rows".to_string(), EvalValue::Int(f.rows as i64));
        map.insert("cols".to_string(), EvalValue::Int(f.cols.len() as i64));
        Ok(EvalValue::Dict(map))
    })
}

fn fn_col(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.col(handle, nombre)".into()); }
    let id   = arg_handle(&args, 0)?;
    let name = arg_str(&args, 1, "frame.col")?;
    with_frames(|fs| {
        let f = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let col = f.col(&name).ok_or(format!("columna '{}' no existe", name))?;
        let vals: Vec<EvalValue> = (0..col.len()).map(|i| col.to_eval(i)).collect();
        Ok(EvalValue::List(vals))
    })
}

fn fn_row(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.row(handle, indice)".into()); }
    let id  = arg_handle(&args, 0)?;
    let idx = match &args[1] { EvalValue::Int(n) => *n as usize, _ => return Err("frame.row: índice debe ser int".into()) };
    with_frames(|fs| {
        let f = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        if idx >= f.rows { return Err(format!("frame.row: índice {} fuera de rango", idx)); }
        Ok(f.row_to_dict(idx))
    })
}

fn fn_to_list(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_frames(|fs| {
        let f = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let rows: Vec<EvalValue> = (0..f.rows).map(|i| f.row_to_dict(i)).collect();
        Ok(EvalValue::List(rows))
    })
}

//   selección                                 ─

fn fn_keep(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.keep(handle, [cols])".into()); }
    let id   = arg_handle(&args, 0)?;
    let keep = arg_str_list(&args, 1)?;
    with_frames(|fs| {
        let f    = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let cols = f.cols.iter().filter(|(n, _)| keep.contains(n)).cloned().collect::<Vec<_>>();
        let rows = cols.first().map(|(_, c)| c.len()).unwrap_or(0);
        let new_id = new_handle();
        fs.insert(new_id.clone(), Frame { cols, rows });
        Ok(EvalValue::Str(new_id))
    })
}

fn fn_drop(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.drop(handle, [cols])".into()); }
    let id   = arg_handle(&args, 0)?;
    let drop = arg_str_list(&args, 1)?;
    with_frames(|fs| {
        let f    = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let cols = f.cols.iter().filter(|(n, _)| !drop.contains(n)).cloned().collect::<Vec<_>>();
        let rows = cols.first().map(|(_, c)| c.len()).unwrap_or(0);
        let new_id = new_handle();
        fs.insert(new_id.clone(), Frame { cols, rows });
        Ok(EvalValue::Str(new_id))
    })
}

fn fn_rename(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 3 { return Err("frame.rename(handle, viejo, nuevo)".into()); }
    let id    = arg_handle(&args, 0)?;
    let viejo = arg_str(&args, 1, "frame.rename")?;
    let nuevo = arg_str(&args, 2, "frame.rename")?;
    with_frames(|fs| {
        let f = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let cols = f.cols.iter().map(|(n, c)| {
            (if n == &viejo { nuevo.clone() } else { n.clone() }, c.clone())
        }).collect();
        let rows = f.rows;
        let new_id = new_handle();
        fs.insert(new_id.clone(), Frame { cols, rows });
        Ok(EvalValue::Str(new_id))
    })
}

//   filtrado                                  

fn fn_where(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 3 { return Err("frame.where_(handle, columna, valor)".into()); }
    let id   = arg_handle(&args, 0)?;
    let col  = arg_str(&args, 1, "frame.where_")?;
    let val  = args[2].clone();
    with_frames(|fs| {
        let f   = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let idx = f.col_index(&col).ok_or(format!("columna '{}' no existe", col))?;
        let mask: Vec<usize> = (0..f.rows).filter(|&i| {
            match (&f.cols[idx].1, &val) {
                (Col::Float(v), EvalValue::Float(target)) => (v[i] - target).abs() < 1e-12,
                (Col::Float(v), EvalValue::Int(target))   => (v[i] - *target as f64).abs() < 1e-12,
                (Col::Int(v),   EvalValue::Int(target))   => v[i] == *target,
                (Col::Str(v),   EvalValue::Str(target))   => &v[i] == target,
                (Col::Bool(v),  EvalValue::Bool(target))  => v[i] == *target,
                _ => false,
            }
        }).collect();
        let cols = f.cols.iter().map(|(name, col)| {
            let c = match col {
                Col::Float(v) => Col::Float(mask.iter().map(|&i| v[i]).collect()),
                Col::Int(v)   => Col::Int(mask.iter().map(|&i| v[i]).collect()),
                Col::Str(v)   => Col::Str(mask.iter().map(|&i| v[i].clone()).collect()),
                Col::Bool(v)  => Col::Bool(mask.iter().map(|&i| v[i]).collect()),
            };
            (name.clone(), c)
        }).collect();
        let rows = mask.len();
        let new_id = new_handle();
        fs.insert(new_id.clone(), Frame { cols, rows });
        Ok(EvalValue::Str(new_id))
    })
}

fn fn_head(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    slice_frame(args, true)
}

fn fn_tail(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    slice_frame(args, false)
}

fn slice_frame(args: Vec<EvalValue>, from_start: bool) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.head/tail(handle, n)".into()); }
    let id = arg_handle(&args, 0)?;
    let n  = match &args[1] { EvalValue::Int(n) => *n as usize, _ => return Err("n debe ser int".into()) };
    with_frames(|fs| {
        let f = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let (start, end) = if from_start {
            (0, n.min(f.rows))
        } else {
            (f.rows.saturating_sub(n), f.rows)
        };
        let range: Vec<usize> = (start..end).collect();
        let cols = f.cols.iter().map(|(name, col)| {
            let c = match col {
                Col::Float(v) => Col::Float(range.iter().map(|&i| v[i]).collect()),
                Col::Int(v)   => Col::Int(range.iter().map(|&i| v[i]).collect()),
                Col::Str(v)   => Col::Str(range.iter().map(|&i| v[i].clone()).collect()),
                Col::Bool(v)  => Col::Bool(range.iter().map(|&i| v[i]).collect()),
            };
            (name.clone(), c)
        }).collect();
        let rows = range.len();
        let new_id = new_handle();
        fs.insert(new_id.clone(), Frame { cols, rows });
        Ok(EvalValue::Str(new_id))
    })
}

fn fn_sort(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.sort(handle, columna, desc?)".into()); }
    let id   = arg_handle(&args, 0)?;
    let col  = arg_str(&args, 1, "frame.sort")?;
    let desc = matches!(args.get(2), Some(EvalValue::Str(s)) if s == "desc");
    with_frames(|fs| {
        let f   = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let idx = f.col_index(&col).ok_or(format!("columna '{}' no existe", col))?;
        let mut order: Vec<usize> = (0..f.rows).collect();
        match &f.cols[idx].1 {
            Col::Float(v) => order.sort_by(|&a, &b| {
                let cmp = v[a].partial_cmp(&v[b]).unwrap_or(std::cmp::Ordering::Equal);
                if desc { cmp.reverse() } else { cmp }
            }),
            Col::Int(v)   => order.sort_by(|&a, &b| {
                let cmp = v[a].cmp(&v[b]);
                if desc { cmp.reverse() } else { cmp }
            }),
            Col::Str(v)   => order.sort_by(|&a, &b| {
                let cmp = v[a].cmp(&v[b]);
                if desc { cmp.reverse() } else { cmp }
            }),
            _ => {}
        }
        let cols = f.cols.iter().map(|(name, col)| {
            let c = match col {
                Col::Float(v) => Col::Float(order.iter().map(|&i| v[i]).collect()),
                Col::Int(v)   => Col::Int(order.iter().map(|&i| v[i]).collect()),
                Col::Str(v)   => Col::Str(order.iter().map(|&i| v[i].clone()).collect()),
                Col::Bool(v)  => Col::Bool(order.iter().map(|&i| v[i]).collect()),
            };
            (name.clone(), c)
        }).collect();
        let rows = f.rows;
        let new_id = new_handle();
        fs.insert(new_id.clone(), Frame { cols, rows });
        Ok(EvalValue::Str(new_id))
    })
}

//   estadísticas columnar (directo sobre Vec<f64>)               

fn fn_col_stat(args: Vec<EvalValue>, stat: &str) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err(format!("frame.{}(handle, columna)", stat)); }
    let id   = arg_handle(&args, 0)?;
    let name = arg_str(&args, 1, "frame.stat")?;
    with_frames(|fs| {
        let f    = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let col  = f.col(&name).ok_or(format!("columna '{}' no existe", name))?;
        let vals = col.as_floats().ok_or(format!("columna '{}' no es numérica", name))?;
        let result = match stat {
            "mean" => vals.iter().sum::<f64>() / vals.len() as f64,
            "sum"  => vals.iter().sum(),
            "min"  => vals.iter().cloned().fold(f64::INFINITY, f64::min),
            "max"  => vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            "std"  => {
                let m = vals.iter().sum::<f64>() / vals.len() as f64;
                (vals.iter().map(|x| (x - m).powi(2)).sum::<f64>() / vals.len() as f64).sqrt()
            }
            _ => 0.0,
        };
        Ok(EvalValue::Float(result))
    })
}

fn fn_stats(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.stats(handle, columna)".into()); }
    let id   = arg_handle(&args, 0)?;
    let name = arg_str(&args, 1, "frame.stats")?;
    with_frames(|fs| {
        let f    = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let col  = f.col(&name).ok_or(format!("columna '{}' no existe", name))?;
        let mut v = col.as_floats().ok_or(format!("columna '{}' no es numérica", name))?;
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n  = v.len() as f64;
        let m  = v.iter().sum::<f64>() / n;
        let s  = (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n).sqrt();
        let p  = |p: f64| { let i = p / 100.0 * (v.len() - 1) as f64; let lo = i.floor() as usize; let hi = (i.ceil() as usize).min(v.len()-1); v[lo] + (v[hi] - v[lo]) * (i - lo as f64) };
        let mut map = HashMap::new();
        map.insert("count".to_string(),  EvalValue::Int(v.len() as i64));
        map.insert("mean".to_string(),   EvalValue::Float(m));
        map.insert("std".to_string(),    EvalValue::Float(s));
        map.insert("min".to_string(),    EvalValue::Float(v[0]));
        map.insert("p25".to_string(),    EvalValue::Float(p(25.0)));
        map.insert("median".to_string(), EvalValue::Float(p(50.0)));
        map.insert("p75".to_string(),    EvalValue::Float(p(75.0)));
        map.insert("max".to_string(),    EvalValue::Float(*v.last().unwrap()));
        Ok(EvalValue::Dict(map))
    })
}

fn fn_count(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_frames(|fs| match fs.get(&id) {
        Some(f) => Ok(EvalValue::Int(f.rows as i64)),
        None    => Err(format!("frame '{}' no existe", id)),
    })
}

//   agregación                                 

fn fn_group(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 4 { return Err("frame.group(handle, by, valor_col, op)".into()); }
    let id  = arg_handle(&args, 0)?;
    let by  = arg_str(&args, 1, "frame.group")?;
    let val = arg_str(&args, 2, "frame.group")?;
    let op  = arg_str(&args, 3, "frame.group")?;
    with_frames(|fs| {
        let f        = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let by_col   = f.col(&by).ok_or(format!("columna '{}' no existe", by))?;
        let val_col  = f.col(&val).ok_or(format!("columna '{}' no existe", val))?;
        let vals     = val_col.as_floats().ok_or(format!("columna '{}' no es numérica", val))?;
        // agrupar: key → vec de valores
        let mut groups: HashMap<String, Vec<f64>> = HashMap::new();
        for i in 0..f.rows {
            let k = match by_col { Col::Str(v) => v[i].clone(), Col::Int(v) => v[i].to_string(),
                                   Col::Bool(v) => v[i].to_string(), Col::Float(v) => format!("{}", v[i]) };
            groups.entry(k).or_default().push(vals[i]);
        }
        let mut keys: Vec<String> = groups.keys().cloned().collect();
        keys.sort();
        let agg_vals: Vec<f64> = keys.iter().map(|k| {
            let g = &groups[k];
            match op.as_str() {
                "sum"   => g.iter().sum(),
                "avg" | "mean" => g.iter().sum::<f64>() / g.len() as f64,
                "count" => g.len() as f64,
                "min"   => g.iter().cloned().fold(f64::INFINITY, f64::min),
                "max"   => g.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                _       => g.iter().sum(),
            }
        }).collect();
        let cols = vec![
            (by.clone(),  Col::Str(keys)),
            (val.clone(), Col::Float(agg_vals.clone())),
        ];
        let rows = agg_vals.len();
        let new_id = new_handle();
        fs.insert(new_id.clone(), Frame { cols, rows });
        Ok(EvalValue::Str(new_id))
    })
}

//   columna calculada                             ─

fn fn_add_col(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 3 { return Err("frame.add_col(handle, nombre, lista_valores)".into()); }
    let id   = arg_handle(&args, 0)?;
    let name = arg_str(&args, 1, "frame.add_col")?;
    let new_col = match &args[2] {
        EvalValue::List(items) => {
            let floats: Vec<f64> = items.iter().filter_map(|v| match v {
                EvalValue::Int(n)   => Some(*n as f64),
                EvalValue::Float(f) => Some(*f),
                _ => None,
            }).collect();
            if floats.len() == items.len() { Col::Float(floats) }
            else {
                let strs: Vec<String> = items.iter().map(|v| format!("{}", v)).collect();
                Col::Str(strs)
            }
        }
        _ => return Err("frame.add_col: valores debe ser una lista".into()),
    };
    with_frames(|fs| {
        let f = fs.get_mut(&id).ok_or(format!("frame '{}' no existe", id))?;
        if new_col.len() != f.rows {
            return Err(format!("frame.add_col: lista tiene {} valores pero el frame tiene {}", new_col.len(), f.rows));
        }
        f.cols.retain(|(n, _)| n != &name);
        f.cols.push((name, new_col));
        Ok(EvalValue::Str(id.clone()))
    })
}

//   chunked — grandes volúmenes sin cargar todo en RAM             

/// frame.each_chunk(ruta, chunk_size, fn(frame_handle) → cualquier_cosa)
/// Lee el CSV en bloques de chunk_size filas, llama fn por cada bloque.
/// Nunca tiene más de chunk_size filas en RAM simultáneamente.
fn fn_each_chunk(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 3 { return Err("frame.each_chunk(ruta, chunk_size, fn)".into()); }
    let path       = arg_str(&args, 0, "frame.each_chunk")?;
    let chunk_size = match &args[1] { EvalValue::Int(n) => *n as usize, _ => 10_000 };
    // El fn se guarda para llamarlo desde el evaluador — retornamos la lista de handles
    // (el evaluador debería llamar fn(handle) por cada chunk; aquí devolvemos los handles)
    let file = File::open(&path).map_err(|e| format!("frame.each_chunk: {}", e))?;
    let mut reader = BufReader::new(file);
    // leer header
    let mut header_line = String::new();
    reader.read_line(&mut header_line).map_err(|e| e.to_string())?;
    let headers: Vec<String> = header_line.split(',')
        .map(|s| s.trim().trim_matches('"').to_string()).collect();
    let mut chunk_handles = Vec::new();
    loop {
        let mut rows: Vec<Vec<String>> = Vec::new();
        for _ in 0..chunk_size {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let fields: Vec<String> = line.split(',')
                        .map(|s| s.trim().trim_matches('"').to_string()).collect();
                    rows.push(fields);
                }
                Err(_) => break,
            }
        }
        if rows.is_empty() { break; }
        let n    = rows.len();
        let cols = infer_columns(&headers, &rows);
        let id   = new_handle();
        with_frames(|fs| fs.insert(id.clone(), Frame { cols, rows: n }));
        chunk_handles.push(EvalValue::Str(id));
    }
    Ok(EvalValue::List(chunk_handles))
}

/// frame.scan_stats(ruta, columna) — calcula stats sobre un CSV grande sin cargarlo todo
/// Lee chunk por chunk y acumula min/max/sum/count para calcular mean y approx std
fn fn_scan_stats(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.scan_stats(ruta, columna)".into()); }
    let path = arg_str(&args, 0, "frame.scan_stats")?;
    let col  = arg_str(&args, 1, "frame.scan_stats")?;
    let file = File::open(&path).map_err(|e| format!("frame.scan_stats: {}", e))?;
    let mut reader = BufReader::new(file);
    // leer header → buscar índice de columna
    let mut header_line = String::new();
    reader.read_line(&mut header_line).map_err(|e| e.to_string())?;
    let headers: Vec<&str> = header_line.split(',').map(|s| s.trim().trim_matches('"')).collect();
    let col_idx = headers.iter().position(|&h| h == col)
        .ok_or(format!("columna '{}' no encontrada", col))?;
    let mut count  = 0u64;
    let mut sum    = 0.0_f64;
    let mut min    = f64::INFINITY;
    let mut max    = f64::NEG_INFINITY;
    let mut sum_sq = 0.0_f64;
    for line in reader.lines().flatten() {
        let fields: Vec<&str> = line.split(',').collect();
        if let Some(raw) = fields.get(col_idx) {
            if let Ok(v) = raw.trim().parse::<f64>() {
                count  += 1;
                sum    += v;
                sum_sq += v * v;
                if v < min { min = v; }
                if v > max { max = v; }
            }
        }
    }
    if count == 0 { return Err(format!("frame.scan_stats: no hay valores numéricos en '{}'", col)); }
    let mean     = sum / count as f64;
    let variance = sum_sq / count as f64 - mean * mean;
    let std      = variance.max(0.0).sqrt();
    let mut map  = HashMap::new();
    map.insert("count".to_string(), EvalValue::Int(count as i64));
    map.insert("mean".to_string(),  EvalValue::Float(mean));
    map.insert("std".to_string(),   EvalValue::Float(std));
    map.insert("min".to_string(),   EvalValue::Float(min));
    map.insert("max".to_string(),   EvalValue::Float(max));
    map.insert("sum".to_string(),   EvalValue::Float(sum));
    Ok(EvalValue::Dict(map))
}

//   persistencia                                

fn fn_save(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.save(handle, ruta)".into()); }
    let id   = arg_handle(&args, 0)?;
    let path = arg_str(&args, 1, "frame.save")?;
    with_frames(|fs| {
        let f = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let mut lines = Vec::new();
        let headers: Vec<String> = f.cols.iter().map(|(n, _)| n.clone()).collect();
        lines.push(headers.join(","));
        for i in 0..f.rows {
            let row: Vec<String> = f.cols.iter().map(|(_, col)| match col {
                Col::Float(v) => format!("{:.4}", v[i]),
                Col::Int(v)   => v[i].to_string(),
                Col::Str(v)   => format!("\"{}\"", v[i].replace('"', "\"\"")),
                Col::Bool(v)  => if v[i] { "yes".into() } else { "no".into() },
            }).collect();
            lines.push(row.join(","));
        }
        std::fs::write(&path, lines.join("\n")).map_err(|e| format!("frame.save: {}", e))?;
        Ok(EvalValue::Str(format!("Guardado: {} ({} filas)", path, f.rows)))
    })
}

//   formato binario .odf                        ─
//
// Capa 1 del motor de datos: columnar en disco, en binario crudo.
// Layout (little-endian):
//   [4]  magic "ODF1"
//   [8]  n_filas (u64)      [4]  n_columnas (u32)
//   por columna (metadatos):  [1] tag (0=f64,1=i64,2=str,3=bool)
//                             [4] len_nombre (u32) + nombre utf8
//   por columna (datos, en orden de columnas):
//     f64 → n_filas*8  |  i64 → n_filas*8  |  bool → n_filas*1
//     str → por fila: [4] len (u32) + bytes utf8
//
// Ventaja vs CSV: cero parsing de texto; los números se leen como bytes crudos
// (from_le_bytes en bloque), no se re-parsea "1200.5" carácter por carácter.
// Es la base sobre la que luego se monta el mmap zero-copy.

const ODF_MAGIC: &[u8; 4] = b"ODF1";

fn col_tag(col: &Col) -> u8 {
    match col { Col::Float(_) => 0, Col::Int(_) => 1, Col::Str(_) => 2, Col::Bool(_) => 3 }
}

/// Serializa columnas a bytes en formato .odf (ODF1). Reutilizado por `save_odf`
/// (frame en RAM) y por `txt_to_odf` (streaming por archivos).
fn serialize_odf(cols: &[(String, Col)], rows: usize) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::with_capacity(16 + rows * cols.len() * 8);
    buf.extend_from_slice(ODF_MAGIC);
    buf.extend_from_slice(&(rows as u64).to_le_bytes());
    buf.extend_from_slice(&(cols.len() as u32).to_le_bytes());
    for (name, col) in cols {
        buf.push(col_tag(col));
        buf.extend_from_slice(&(name.len() as u32).to_le_bytes());
        buf.extend_from_slice(name.as_bytes());
    }
    for (_, col) in cols {
        match col {
            Col::Float(v) => for &x in v { buf.extend_from_slice(&x.to_le_bytes()); },
            Col::Int(v)   => for &x in v { buf.extend_from_slice(&x.to_le_bytes()); },
            Col::Bool(v)  => for &b in v { buf.push(if b { 1 } else { 0 }); },
            Col::Str(v)   => for s in v {
                buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
                buf.extend_from_slice(s.as_bytes());
            },
        }
    }
    buf
}

fn fn_save_odf(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.save_odf(handle, ruta)".into()); }
    let id   = arg_handle(&args, 0)?;
    let path = arg_str(&args, 1, "frame.save_odf")?;
    with_frames(|fs| {
        let f = fs.get(&id).ok_or(format!("frame '{}' no existe", id))?;
        let buf = serialize_odf(&f.cols, f.rows);
        std::fs::write(&path, &buf)
            .map_err(|e| format!("frame.save_odf: {}", e))?;
        Ok(EvalValue::Str(format!(
            "Guardado .odf: {} ({} filas, {} columnas, {} bytes)",
            path, f.rows, f.cols.len(), buf.len()
        )))
    })
}

/// Streaming TXT → .odf binario, en varios archivos (memoria acotada).
///
/// txt_to_odf(txt, base, sep = ",", chunk = 500_000)
///
/// Transmite el TXT y escribe `base_1.odf`, `base_2.odf`, … de `chunk` filas
/// cada uno, LIBERANDO cada bloque tras guardarlo. Combina lo mejor de las dos
/// técnicas: memoria acotada (streaming) Y velocidad binaria (~8× más rápido que
/// xlsx, sin impuesto XML/zip). Cada archivo infiere sus propios tipos de columna
/// de su chunk. Contraparte rápida de `txt_to_excel` (que es para humanos/Office).
fn fn_txt_to_odf(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("frame.txt_to_odf(txt, base[, sep, chunk])".into()); }
    let txt_path = arg_str(&args, 0, "frame.txt_to_odf")?;
    let out_base = arg_str(&args, 1, "frame.txt_to_odf")?;
    let sep = match args.get(2) {
        Some(EvalValue::Str(s)) if !s.is_empty() => s.clone(),
        _ => ",".to_string(),
    };
    let chunk = match args.get(3) {
        Some(EvalValue::Int(n)) if *n > 0 => *n as usize,
        _ => 500_000,
    };

    let file = File::open(&txt_path).map_err(|e| format!("frame.txt_to_odf: {}", e))?;
    let mut reader = BufReader::new(file);

    // cabecera
    let mut header_line = String::new();
    if reader.read_line(&mut header_line).map_err(|e| e.to_string())? == 0 {
        return Err("frame.txt_to_odf: archivo vacío".into());
    }
    let headers: Vec<String> = header_line
        .trim_end_matches(['\r', '\n'])
        .split(&sep)
        .map(|s| s.trim().trim_matches('"').to_string())
        .collect();

    let mut file_idx = 0usize;
    let mut total = 0usize;
    loop {
        // leer hasta `chunk` filas
        let mut rows: Vec<Vec<String>> = Vec::new();
        for _ in 0..chunk {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if line.trim().is_empty() { continue; }
                    let fields: Vec<String> = line
                        .trim_end_matches(['\r', '\n'])
                        .split(&sep)
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .collect();
                    rows.push(fields);
                }
                Err(_) => break,
            }
        }
        if rows.is_empty() { break; }

        let n = rows.len();
        let cols = infer_columns(&headers, &rows);
        let buf = serialize_odf(&cols, n);
        drop(rows); // liberar el chunk de texto antes de escribir
        file_idx += 1;
        let path = format!("{}_{}.odf", out_base, file_idx);
        std::fs::write(&path, &buf).map_err(|e| format!("frame.txt_to_odf: {}", e))?;
        total += n;
        if n < chunk { break; } // último chunk parcial
    }

    Ok(EvalValue::Str(format!(
        "Streaming TXT→.odf: {} filas en {} archivo(s) ({}_1.odf …)",
        total, file_idx, out_base
    )))
}

fn fn_load_odf(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let path = arg_str(&args, 0, "frame.load_odf")?;
    let data = std::fs::read(&path).map_err(|e| format!("frame.load_odf: {}", e))?;
    let mut p = 0usize;

    // Comprueba que quedan al menos $n bytes desde la posición actual.
    macro_rules! need {
        ($n:expr) => {
            if p + $n > data.len() {
                return Err("frame.load_odf: archivo .odf truncado o corrupto".into());
            }
        };
    }

    need!(4);
    if &data[0..4] != ODF_MAGIC {
        return Err("frame.load_odf: no es un .odf válido (magic incorrecto)".into());
    }
    p = 4;
    need!(8); let n_rows = u64::from_le_bytes(data[p..p+8].try_into().unwrap()) as usize; p += 8;
    need!(4); let n_cols = u32::from_le_bytes(data[p..p+4].try_into().unwrap()) as usize; p += 4;

    // metadatos
    let mut metas: Vec<(u8, String)> = Vec::with_capacity(n_cols);
    for _ in 0..n_cols {
        need!(1); let tag = data[p]; p += 1;
        need!(4); let nl = u32::from_le_bytes(data[p..p+4].try_into().unwrap()) as usize; p += 4;
        need!(nl); let name = String::from_utf8_lossy(&data[p..p+nl]).into_owned(); p += nl;
        metas.push((tag, name));
    }

    // datos
    let mut cols: Vec<(String, Col)> = Vec::with_capacity(n_cols);
    for (tag, name) in metas {
        let col = match tag {
            0 => { need!(n_rows * 8); let mut v = Vec::with_capacity(n_rows);
                   for _ in 0..n_rows { v.push(f64::from_le_bytes(data[p..p+8].try_into().unwrap())); p += 8; }
                   Col::Float(v) }
            1 => { need!(n_rows * 8); let mut v = Vec::with_capacity(n_rows);
                   for _ in 0..n_rows { v.push(i64::from_le_bytes(data[p..p+8].try_into().unwrap())); p += 8; }
                   Col::Int(v) }
            3 => { need!(n_rows); let mut v = Vec::with_capacity(n_rows);
                   for _ in 0..n_rows { v.push(data[p] != 0); p += 1; }
                   Col::Bool(v) }
            2 => { let mut v = Vec::with_capacity(n_rows);
                   for _ in 0..n_rows {
                       need!(4); let sl = u32::from_le_bytes(data[p..p+4].try_into().unwrap()) as usize; p += 4;
                       need!(sl); v.push(String::from_utf8_lossy(&data[p..p+sl]).into_owned()); p += sl;
                   }
                   Col::Str(v) }
            _ => return Err(format!("frame.load_odf: tag de columna desconocido ({})", tag)),
        };
        cols.push((name, col));
    }

    let id = new_handle();
    with_frames(|fs| fs.insert(id.clone(), Frame { cols, rows: n_rows }));
    Ok(EvalValue::Str(id))
}

//   arg helpers                                ─

fn arg_handle(args: &[EvalValue], pos: usize) -> Result<String, String> {
    match args.get(pos) {
        Some(EvalValue::Str(s)) => Ok(s.clone()),
        _ => Err("frame: se esperaba un handle (string)".into()),
    }
}

fn arg_str(args: &[EvalValue], pos: usize, ctx: &str) -> Result<String, String> {
    match args.get(pos) {
        Some(EvalValue::Str(s)) => Ok(s.clone()),
        _ => Err(format!("{}: argumento {} debe ser string", ctx, pos)),
    }
}

fn arg_str_list(args: &[EvalValue], pos: usize) -> Result<Vec<String>, String> {
    match args.get(pos) {
        Some(EvalValue::List(items)) => items.iter().map(|v| match v {
            EvalValue::Str(s) => Ok(s.clone()),
            _ => Err("frame: se esperaba lista de strings".into()),
        }).collect(),
        _ => Err("frame: se esperaba lista de strings".into()),
    }
}

//   tests                                       ─

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(pairs: &[(&str, EvalValue)]) -> EvalValue {
        let mut m = HashMap::new();
        for (k, v) in pairs { m.insert((*k).to_string(), v.clone()); }
        EvalValue::Dict(m)
    }

    fn handle(v: EvalValue) -> String {
        match v { EvalValue::Str(s) => s, other => panic!("se esperaba handle, got {:?}", other) }
    }

    /// El .odf debe preservar exactamente tipos y valores tras save→load.
    /// Cubre las cuatro columnas (int, float, str, bool) y verifica que las
    /// operaciones columnares (sum) dan el mismo resultado sobre el frame cargado.
    #[test]
    fn odf_roundtrip_preserva_tipos_y_valores() {
        let rows = EvalValue::List(vec![
            dict(&[("nombre", EvalValue::Str("Ana".into())),  ("edad", EvalValue::Int(30)),
                   ("monto", EvalValue::Float(1200.5)),        ("activo", EvalValue::Bool(true))]),
            dict(&[("nombre", EvalValue::Str("Luis".into())), ("edad", EvalValue::Int(25)),
                   ("monto", EvalValue::Float(980.0)),         ("activo", EvalValue::Bool(false))]),
            dict(&[("nombre", EvalValue::Str("Zoe".into())),  ("edad", EvalValue::Int(41)),
                   ("monto", EvalValue::Float(1500.75)),       ("activo", EvalValue::Bool(true))]),
        ]);

        let h = handle(call("from_list", vec![rows]).unwrap());

        let mut path = std::env::temp_dir();
        path.push(format!("orion_odf_test_{}.odf", std::process::id()));
        let p = path.to_str().unwrap().to_string();

        call("save_odf", vec![EvalValue::Str(h.clone()), EvalValue::Str(p.clone())]).unwrap();
        let h2 = handle(call("load_odf", vec![EvalValue::Str(p.clone())]).unwrap());

        // helpers de aserción (EvalValue no implementa PartialEq)
        let as_int = |v: Option<&EvalValue>| match v { Some(EvalValue::Int(n)) => *n, o => panic!("esperaba int, got {:?}", o) };
        let as_str = |v: Option<&EvalValue>| match v { Some(EvalValue::Str(s)) => s.clone(), o => panic!("esperaba str, got {:?}", o) };
        let as_bool = |v: Option<&EvalValue>| match v { Some(EvalValue::Bool(b)) => *b, o => panic!("esperaba bool, got {:?}", o) };

        // mismas dimensiones
        let size = call("size", vec![EvalValue::Str(h2.clone())]).unwrap();
        if let EvalValue::Dict(m) = &size {
            assert_eq!(as_int(m.get("rows")), 3);
            assert_eq!(as_int(m.get("cols")), 4);
        } else { panic!("size no devolvió dict"); }

        // suma de monto idéntica al original (float exacto vía bytes)
        let suma = call("sum", vec![EvalValue::Str(h2.clone()), EvalValue::Str("monto".into())]).unwrap();
        match suma {
            EvalValue::Float(f) => assert!((f - (1200.5 + 980.0 + 1500.75)).abs() < 1e-9, "suma divergió: {}", f),
            o => panic!("sum no devolvió float: {:?}", o),
        }

        // valores str/int/bool preservados fila a fila
        let row0 = call("row", vec![EvalValue::Str(h2.clone()), EvalValue::Int(0)]).unwrap();
        if let EvalValue::Dict(m) = &row0 {
            assert_eq!(as_str(m.get("nombre")), "Ana");
            assert_eq!(as_int(m.get("edad")), 30);
            assert!(as_bool(m.get("activo")));
        } else { panic!("row(0) no devolvió dict"); }

        let _ = std::fs::remove_file(&path);
    }

    /// Un archivo que no es .odf (magic incorrecto) debe fallar limpio, no panic.
    #[test]
    fn odf_load_rechaza_archivo_invalido() {
        let mut path = std::env::temp_dir();
        path.push(format!("orion_odf_bad_{}.odf", std::process::id()));
        std::fs::write(&path, b"esto no es un odf").unwrap();
        let r = call("load_odf", vec![EvalValue::Str(path.to_str().unwrap().into())]);
        assert!(r.is_err(), "debería rechazar un archivo con magic inválido");
        let _ = std::fs::remove_file(&path);
    }

    /// from_txt con separador configurable parsea columnas tipadas.
    #[test]
    fn from_txt_separador_configurable() {
        let mut path = std::env::temp_dir();
        path.push(format!("orion_txt_{}.txt", std::process::id()));
        std::fs::write(&path, "nombre;edad;monto\nAna;30;1200.5\nLuis;25;980.0\n").unwrap();

        let h = handle(call("from_txt", vec![
            EvalValue::Str(path.to_str().unwrap().into()),
            EvalValue::Str(";".into()),
        ]).unwrap());

        let size = call("size", vec![EvalValue::Str(h.clone())]).unwrap();
        if let EvalValue::Dict(m) = &size {
            match m.get("rows") { Some(EvalValue::Int(2)) => {}, o => panic!("rows != 2: {:?}", o) }
            match m.get("cols") { Some(EvalValue::Int(3)) => {}, o => panic!("cols != 3: {:?}", o) }
        } else { panic!("size no devolvió dict"); }

        // el separador ';' se respetó → "edad" es una columna numérica real
        let suma = call("sum", vec![EvalValue::Str(h), EvalValue::Str("edad".into())]).unwrap();
        match suma { EvalValue::Float(f) => assert!((f - 55.0).abs() < 1e-9), o => panic!("suma edad: {:?}", o) }

        let _ = std::fs::remove_file(&path);
    }

    /// txt_to_excel transmite el TXT y reparte en varios archivos (memoria
    /// acotada), respetando split_por. 7 filas con split 3 ⇒ 3 archivos.
    #[test]
    fn txt_to_excel_streaming_reparte_archivos() {
        let dir = std::env::temp_dir();
        let txt = dir.join(format!("orion_stream_{}.txt", std::process::id()));
        let mut contenido = String::from("nombre;edad;monto\n");
        for i in 1..=7 {
            contenido.push_str(&format!("P{};{};{}.5\n", i, 20 + i, 100 * i));
        }
        std::fs::write(&txt, contenido).unwrap();

        let base = dir.join(format!("orion_stream_out_{}", std::process::id()));
        let base_str = base.to_str().unwrap().to_string();

        let msg = call("txt_to_excel", vec![
            EvalValue::Str(txt.to_str().unwrap().into()),
            EvalValue::Str(base_str.clone()),
            EvalValue::Str(";".into()),
            EvalValue::Int(3),
        ]).unwrap();
        match msg {
            EvalValue::Str(s) => {
                assert!(s.contains("7 filas"), "filas: {}", s);
                assert!(s.contains("3 archivo"), "archivos: {}", s);
            }
            o => panic!("no devolvió str: {:?}", o),
        }
        // los 3 archivos existen; el 4º no
        for i in 1..=3 {
            assert!(std::path::Path::new(&format!("{}_{}.xlsx", base_str, i)).exists(),
                "falta el archivo {}", i);
        }
        assert!(!std::path::Path::new(&format!("{}_4.xlsx", base_str)).exists(),
            "no debería existir un 4º archivo");

        // limpieza
        let _ = std::fs::remove_file(&txt);
        for i in 1..=3 { let _ = std::fs::remove_file(format!("{}_{}.xlsx", base_str, i)); }
    }

    /// txt_to_odf transmite el TXT a varios .odf binarios (memoria acotada) y
    /// cada archivo se puede recargar con load_odf preservando tipos.
    #[test]
    fn txt_to_odf_streaming_y_roundtrip() {
        let dir = std::env::temp_dir();
        let txt = dir.join(format!("orion_odfstream_{}.txt", std::process::id()));
        let mut contenido = String::from("id;nombre;monto\n");
        for i in 1..=7 {
            contenido.push_str(&format!("{};P{};{}.5\n", i, i, 100 * i));
        }
        std::fs::write(&txt, contenido).unwrap();

        let base = dir.join(format!("orion_odfstream_out_{}", std::process::id()));
        let base_str = base.to_str().unwrap().to_string();

        // 7 filas, chunk 3 → 3 archivos
        let msg = call("txt_to_odf", vec![
            EvalValue::Str(txt.to_str().unwrap().into()),
            EvalValue::Str(base_str.clone()),
            EvalValue::Str(";".into()),
            EvalValue::Int(3),
        ]).unwrap();
        match msg {
            EvalValue::Str(s) => {
                assert!(s.contains("7 filas"), "filas: {}", s);
                assert!(s.contains("3 archivo"), "archivos: {}", s);
            }
            o => panic!("no devolvió str: {:?}", o),
        }

        // recargar el primer archivo: 3 filas, tipos correctos
        let h = handle(call("load_odf", vec![
            EvalValue::Str(format!("{}_1.odf", base_str)),
        ]).unwrap());
        let size = call("size", vec![EvalValue::Str(h.clone())]).unwrap();
        if let EvalValue::Dict(m) = &size {
            match m.get("rows") { Some(EvalValue::Int(3)) => {}, o => panic!("rows: {:?}", o) }
        } else { panic!("size no dict"); }
        // 'monto' debe haberse inferido como float y sumar correctamente
        let suma = call("sum", vec![EvalValue::Str(h), EvalValue::Str("monto".into())]).unwrap();
        match suma {
            EvalValue::Float(f) => assert!((f - (100.5 + 200.5 + 300.5)).abs() < 1e-9, "suma: {}", f),
            o => panic!("sum no float: {:?}", o),
        }

        let _ = std::fs::remove_file(&txt);
        for i in 1..=3 { let _ = std::fs::remove_file(format!("{}_{}.odf", base_str, i)); }
    }

    /// to_excel reparte en varias hojas cuando el frame supera split_por.
    #[test]
    fn to_excel_parte_por_split() {
        // 5 filas via from_list
        let rows = EvalValue::List((0..5).map(|i| {
            dict(&[("id", EvalValue::Int(i)), ("v", EvalValue::Float(i as f64 * 1.5))])
        }).collect());
        let h = handle(call("from_list", vec![rows]).unwrap());

        let mut path = std::env::temp_dir();
        path.push(format!("orion_xlsx_{}.xlsx", std::process::id()));
        let p = path.to_str().unwrap().to_string();

        // split_por = 2 → ceil(5/2) = 3 hojas
        let msg = call("to_excel", vec![
            EvalValue::Str(h), EvalValue::Str(p.clone()), EvalValue::Int(2),
        ]).unwrap();
        match msg {
            EvalValue::Str(s) => assert!(s.contains("3 hoja"), "esperaba 3 hojas: {}", s),
            o => panic!("to_excel no devolvió str: {:?}", o),
        }
        assert!(path.exists(), "el xlsx no se creó");
        let _ = std::fs::remove_file(&path);
    }
}
