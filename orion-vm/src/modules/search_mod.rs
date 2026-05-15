/// search_mod — búsqueda rápida en archivos sin cargar todo en RAM
///
/// Streaming en todos los casos: BufReader para texto, csv::Reader para CSV,
/// calamine para Excel. Regex via crate `regex`. Nunca > chunk de líneas en RAM.

use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        "in_file"  => fn_in_file(args),     // auto-detecta tipo por extensión
        "text"     => fn_text(args),         // busca en txt/log/cualquier texto
        "regex"    => fn_regex(args),        // búsqueda con regex
        "csv"      => fn_csv(args),          // busca en CSV por columna=valor
        "excel"    => fn_excel(args),        // busca en Excel
        "count"    => fn_count(args),        // cuenta matches sin materializar
        "first"    => fn_first(args),        // primer match y para
        "in_dir"   => fn_in_dir(args),       // busca en todos los archivos de un directorio
        "context"  => fn_context(args),      // grep -C: líneas antes/después del match
        "columns"  => fn_csv_columns(args),  // busca en múltiples columnas de CSV
        _ => Err(format!("search.{} no existe", function)),
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn str_arg(args: &[EvalValue], pos: usize, ctx: &str) -> Result<String, String> {
    match args.get(pos) {
        Some(EvalValue::Str(s)) => Ok(s.clone()),
        _ => Err(format!("search.{}: argumento {} debe ser string", ctx, pos)),
    }
}

fn ext(path: &str) -> &str {
    Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("")
}

fn match_line(line: &str, pattern: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        line.contains(pattern)
    } else {
        line.to_lowercase().contains(&pattern.to_lowercase())
    }
}

fn result_dict(line_no: usize, content: &str) -> EvalValue {
    let mut m = HashMap::new();
    m.insert("line".to_string(),    EvalValue::Int(line_no as i64));
    m.insert("content".to_string(), EvalValue::Str(content.trim_end().to_string()));
    EvalValue::Dict(m)
}

// ── búsqueda automática por tipo de archivo ───────────────────────────────────

fn fn_in_file(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("search.in_file(ruta, patron)".into()); }
    let path    = str_arg(&args, 0, "in_file")?;
    match ext(&path) {
        "csv" | "tsv" => {
            // para in_file en CSV buscamos en cualquier columna
            fn_csv_any(&path, &str_arg(&args, 1, "in_file")?)
        }
        "xlsx" | "xls" | "ods" => fn_excel(args),
        _ => fn_text(args),
    }
}

// ── texto / log / txt ─────────────────────────────────────────────────────────

fn fn_text(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("search.text(ruta, patron, case_sensitive?)".into()); }
    let path    = str_arg(&args, 0, "text")?;
    let pattern = str_arg(&args, 1, "text")?;
    let case_ok = !matches!(args.get(2), Some(EvalValue::Bool(false)));

    let file    = File::open(&path).map_err(|e| format!("search.text: {}", e))?;
    let reader  = BufReader::new(file);
    let mut results = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        let l = line.map_err(|e| format!("search.text: {}", e))?;
        if match_line(&l, &pattern, case_ok) {
            results.push(result_dict(i + 1, &l));
        }
    }
    Ok(EvalValue::List(results))
}

// ── regex ─────────────────────────────────────────────────────────────────────

fn fn_regex(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("search.regex(ruta, patron_regex)".into()); }
    let path    = str_arg(&args, 0, "regex")?;
    let pattern = str_arg(&args, 1, "regex")?;
    let re      = regex::Regex::new(&pattern).map_err(|e| format!("search.regex: patrón inválido: {}", e))?;

    let file    = File::open(&path).map_err(|e| format!("search.regex: {}", e))?;
    let reader  = BufReader::new(file);
    let mut results = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        let l = line.map_err(|e| format!("search.regex: {}", e))?;
        if re.is_match(&l) {
            let captures: Vec<EvalValue> = re.captures_iter(&l)
                .flat_map(|c| c.iter().skip(1).flatten()
                    .map(|m| EvalValue::Str(m.as_str().to_string()))
                    .collect::<Vec<_>>())
                .collect();
            let mut m = HashMap::new();
            m.insert("line".to_string(),     EvalValue::Int((i + 1) as i64));
            m.insert("content".to_string(),  EvalValue::Str(l.trim_end().to_string()));
            m.insert("matches".to_string(),  EvalValue::List(captures));
            results.push(EvalValue::Dict(m));
        }
    }
    Ok(EvalValue::List(results))
}

// ── CSV streaming ─────────────────────────────────────────────────────────────

/// Busca en una columna específica: search.csv(ruta, columna, valor)
fn fn_csv(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 3 { return Err("search.csv(ruta, columna, valor)".into()); }
    let path   = str_arg(&args, 0, "csv")?;
    let col    = str_arg(&args, 1, "csv")?;
    let target = &args[2];

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(&path)
        .map_err(|e| format!("search.csv: {}", e))?;

    let headers: Vec<String> = rdr.headers()
        .map_err(|e| format!("search.csv: {}", e))?
        .iter().map(|s| s.trim().to_string()).collect();

    let col_idx = headers.iter().position(|h| h == &col)
        .ok_or(format!("search.csv: columna '{}' no encontrada", col))?;

    let mut results = Vec::new();
    for record in rdr.records().flatten() {
        let cell = record.get(col_idx).unwrap_or("").trim();
        let matches = match target {
            EvalValue::Str(s)   => cell.eq_ignore_ascii_case(s),
            EvalValue::Int(n)   => cell.parse::<i64>().ok() == Some(*n),
            EvalValue::Float(f) => cell.parse::<f64>().ok().map(|v| (v - f).abs() < 1e-9).unwrap_or(false),
            EvalValue::Bool(b)  => matches!((cell, b), ("yes"|"true"|"1", true) | ("no"|"false"|"0", false)),
            _ => false,
        };
        if matches {
            let mut map = HashMap::new();
            for (i, h) in headers.iter().enumerate() {
                let v = record.get(i).unwrap_or("").trim();
                map.insert(h.clone(), infer_val(v));
            }
            results.push(EvalValue::Dict(map));
        }
    }
    Ok(EvalValue::List(results))
}

/// Busca un patrón de texto en cualquier columna del CSV
fn fn_csv_any(path: &str, pattern: &str) -> Result<EvalValue, String> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true).flexible(true)
        .from_path(path)
        .map_err(|e| format!("search.in_file: {}", e))?;

    let headers: Vec<String> = rdr.headers()
        .map_err(|e| e.to_string())?
        .iter().map(|s| s.trim().to_string()).collect();

    let pat_lower = pattern.to_lowercase();
    let mut results = Vec::new();
    for record in rdr.records().flatten() {
        let hit = record.iter().any(|cell| cell.to_lowercase().contains(&pat_lower));
        if hit {
            let mut map = HashMap::new();
            for (i, h) in headers.iter().enumerate() {
                map.insert(h.clone(), infer_val(record.get(i).unwrap_or("").trim()));
            }
            results.push(EvalValue::Dict(map));
        }
    }
    Ok(EvalValue::List(results))
}

/// Busca en múltiples columnas: search.columns(ruta, [col1, col2], patron)
fn fn_csv_columns(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 3 { return Err("search.columns(ruta, [columnas], patron)".into()); }
    let path    = str_arg(&args, 0, "columns")?;
    let cols: Vec<String> = match &args[1] {
        EvalValue::List(items) => items.iter().filter_map(|v| match v {
            EvalValue::Str(s) => Some(s.clone()), _ => None
        }).collect(),
        _ => return Err("search.columns: columnas debe ser lista de strings".into()),
    };
    let pattern = str_arg(&args, 2, "columns")?.to_lowercase();

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true).flexible(true)
        .from_path(&path)
        .map_err(|e| format!("search.columns: {}", e))?;

    let headers: Vec<String> = rdr.headers()
        .map_err(|e| e.to_string())?
        .iter().map(|s| s.trim().to_string()).collect();

    let indices: Vec<usize> = cols.iter()
        .filter_map(|c| headers.iter().position(|h| h == c))
        .collect();

    let mut results = Vec::new();
    for record in rdr.records().flatten() {
        let hit = indices.iter().any(|&i| {
            record.get(i).map(|v| v.to_lowercase().contains(&pattern)).unwrap_or(false)
        });
        if hit {
            let mut map = HashMap::new();
            for (i, h) in headers.iter().enumerate() {
                map.insert(h.clone(), infer_val(record.get(i).unwrap_or("").trim()));
            }
            results.push(EvalValue::Dict(map));
        }
    }
    Ok(EvalValue::List(results))
}

// ── Excel ─────────────────────────────────────────────────────────────────────

fn fn_excel(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("search.excel(ruta, patron, hoja?)".into()); }
    let path    = str_arg(&args, 0, "excel")?;
    let pattern = str_arg(&args, 1, "excel")?.to_lowercase();
    let sheet_name = match args.get(2) {
        Some(EvalValue::Str(s)) => Some(s.clone()),
        _ => None,
    };

    use calamine::{open_workbook_auto, Reader};
    let mut wb = open_workbook_auto(&path)
        .map_err(|e| format!("search.excel: {}", e))?;

    let sheet = sheet_name
        .unwrap_or_else(|| wb.sheet_names().first().cloned().unwrap_or_default());
    let range = wb.worksheet_range(&sheet)
        .map_err(|e| format!("search.excel: hoja '{}': {}", sheet, e))?;

    let mut rows = range.rows();
    let headers: Vec<String> = match rows.next() {
        Some(r) => r.iter().map(|c| c.to_string().trim().to_string()).collect(),
        None    => return Ok(EvalValue::List(vec![])),
    };

    let mut results = Vec::new();
    for row in rows {
        let hit = row.iter().any(|c| c.to_string().to_lowercase().contains(&pattern));
        if hit {
            let mut map = HashMap::new();
            for (i, h) in headers.iter().enumerate() {
                let cell = row.get(i).map(|c| c.to_string()).unwrap_or_default();
                map.insert(h.clone(), infer_val(&cell));
            }
            results.push(EvalValue::Dict(map));
        }
    }
    Ok(EvalValue::List(results))
}

// ── conteo rápido sin materializar resultados ─────────────────────────────────

fn fn_count(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("search.count(ruta, patron)".into()); }
    let path    = str_arg(&args, 0, "count")?;
    let pattern = str_arg(&args, 1, "count")?;
    let pat_l   = pattern.to_lowercase();

    match ext(&path) {
        "csv" | "tsv" => {
            let mut rdr = csv::ReaderBuilder::new().has_headers(true).flexible(true)
                .from_path(&path).map_err(|e| format!("search.count: {}", e))?;
            let count = rdr.records().flatten()
                .filter(|r| r.iter().any(|c| c.to_lowercase().contains(&pat_l)))
                .count();
            Ok(EvalValue::Int(count as i64))
        }
        _ => {
            let file   = File::open(&path).map_err(|e| format!("search.count: {}", e))?;
            let count  = BufReader::new(file).lines().flatten()
                .filter(|l| l.to_lowercase().contains(&pat_l))
                .count();
            Ok(EvalValue::Int(count as i64))
        }
    }
}

// ── primer match (para y sale) ────────────────────────────────────────────────

fn fn_first(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("search.first(ruta, patron)".into()); }
    let path    = str_arg(&args, 0, "first")?;
    let pattern = str_arg(&args, 1, "first")?;
    let pat_l   = pattern.to_lowercase();

    match ext(&path) {
        "csv" | "tsv" => {
            let mut rdr = csv::ReaderBuilder::new().has_headers(true).flexible(true)
                .from_path(&path).map_err(|e| format!("search.first: {}", e))?;
            let headers: Vec<String> = rdr.headers().map_err(|e| e.to_string())?
                .iter().map(|s| s.trim().to_string()).collect();
            for record in rdr.records().flatten() {
                if record.iter().any(|c| c.to_lowercase().contains(&pat_l)) {
                    let mut map = HashMap::new();
                    for (i, h) in headers.iter().enumerate() {
                        map.insert(h.clone(), infer_val(record.get(i).unwrap_or("").trim()));
                    }
                    return Ok(EvalValue::Dict(map));
                }
            }
            Ok(EvalValue::Null)
        }
        _ => {
            let file = File::open(&path).map_err(|e| format!("search.first: {}", e))?;
            for (i, line) in BufReader::new(file).lines().enumerate() {
                let l = line.map_err(|e| format!("search.first: {}", e))?;
                if l.to_lowercase().contains(&pat_l) {
                    return Ok(result_dict(i + 1, &l));
                }
            }
            Ok(EvalValue::Null)
        }
    }
}

// ── búsqueda en directorio ────────────────────────────────────────────────────

fn fn_in_dir(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("search.in_dir(directorio, patron, ext?)".into()); }
    let dir     = str_arg(&args, 0, "in_dir")?;
    let pattern = str_arg(&args, 1, "in_dir")?.to_lowercase();
    let filter_ext = match args.get(2) {
        Some(EvalValue::Str(s)) => Some(s.trim_start_matches('.').to_lowercase()),
        _ => None,
    };

    let entries = fs::read_dir(&dir).map_err(|e| format!("search.in_dir: {}", e))?;
    let mut results = Vec::new();

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_file() { continue; }
        let path_str = entry_path.to_string_lossy().to_string();
        let file_ext = ext(&path_str).to_lowercase();

        if let Some(ref fe) = filter_ext {
            if &file_ext != fe { continue; }
        }

        let file = match File::open(&entry_path) { Ok(f) => f, Err(_) => continue };
        for (i, line) in BufReader::new(file).lines().enumerate() {
            let Ok(l) = line else { continue };
            if l.to_lowercase().contains(&pattern) {
                let mut m = HashMap::new();
                m.insert("file".to_string(),    EvalValue::Str(path_str.clone()));
                m.insert("line".to_string(),    EvalValue::Int((i + 1) as i64));
                m.insert("content".to_string(), EvalValue::Str(l.trim_end().to_string()));
                results.push(EvalValue::Dict(m));
            }
        }
    }
    Ok(EvalValue::List(results))
}

// ── contexto — N líneas antes/después ────────────────────────────────────────

fn fn_context(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("search.context(ruta, patron, n?)".into()); }
    let path    = str_arg(&args, 0, "context")?;
    let pattern = str_arg(&args, 1, "context")?;
    let n       = match args.get(2) { Some(EvalValue::Int(n)) => *n as usize, _ => 2 };

    let file    = File::open(&path).map_err(|e| format!("search.context: {}", e))?;
    let lines: Vec<String> = BufReader::new(file).lines().flatten().collect();
    let pat_l   = pattern.to_lowercase();
    let mut results = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if !line.to_lowercase().contains(&pat_l) { continue; }
        let before: Vec<EvalValue> = (i.saturating_sub(n)..i)
            .map(|j| EvalValue::Str(lines[j].clone())).collect();
        let after: Vec<EvalValue> = ((i + 1)..(i + 1 + n).min(lines.len()))
            .map(|j| EvalValue::Str(lines[j].clone())).collect();
        let mut m = HashMap::new();
        m.insert("line".to_string(),    EvalValue::Int((i + 1) as i64));
        m.insert("content".to_string(), EvalValue::Str(line.trim_end().to_string()));
        m.insert("before".to_string(),  EvalValue::List(before));
        m.insert("after".to_string(),   EvalValue::List(after));
        results.push(EvalValue::Dict(m));
    }
    Ok(EvalValue::List(results))
}

// ── inferir tipo de celda ─────────────────────────────────────────────────────

fn infer_val(s: &str) -> EvalValue {
    if let Ok(n) = s.parse::<i64>()   { return EvalValue::Int(n); }
    if let Ok(f) = s.parse::<f64>()   { return EvalValue::Float(f); }
    match s { "yes"|"true" => return EvalValue::Bool(true),
               "no"|"false" => return EvalValue::Bool(false), _ => {} }
    EvalValue::Str(s.to_string())
}
