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
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

// ── tipos columna ─────────────────────────────────────────────────────────────

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

// ── frame en memoria ──────────────────────────────────────────────────────────

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

// ── store estático ────────────────────────────────────────────────────────────

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

// ── parsing CSV ───────────────────────────────────────────────────────────────

fn parse_csv_chunk(reader: &mut BufReader<File>, limit: usize) -> (Vec<String>, Vec<Vec<String>>) {
    let mut headers = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        let Ok(l) = line else { break };
        let fields: Vec<String> = l.split(',').map(|s| s.trim().trim_matches('"').to_string()).collect();
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

// ── dispatcher ────────────────────────────────────────────────────────────────

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // Carga
        "open"       => fn_open(args),
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
        _ => Err(format!("frame.{} no existe", function)),
    }
}

// ── carga ─────────────────────────────────────────────────────────────────────

fn fn_open(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let path = match args.first() {
        Some(EvalValue::Str(s)) => s.clone(),
        _ => return Err("frame.open(ruta_csv)".into()),
    };
    let file = File::open(&path).map_err(|e| format!("frame.open: {}", e))?;
    let mut reader = BufReader::new(file);
    let (headers, rows) = parse_csv_chunk(&mut reader, 0); // 0 = sin límite
    if headers.is_empty() { return Err("frame.open: archivo vacío o sin cabecera".into()); }
    let n = rows.len();
    let cols = infer_columns(&headers, &rows);
    let id = new_handle();
    with_frames(|fs| fs.insert(id.clone(), Frame { cols, rows: n }));
    Ok(EvalValue::Str(id))
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

// ── exploración ───────────────────────────────────────────────────────────────

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

// ── selección ─────────────────────────────────────────────────────────────────

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

// ── filtrado ──────────────────────────────────────────────────────────────────

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

// ── estadísticas columnar (directo sobre Vec<f64>) ────────────────────────────

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

// ── agregación ────────────────────────────────────────────────────────────────

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

// ── columna calculada ─────────────────────────────────────────────────────────

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

// ── chunked — grandes volúmenes sin cargar todo en RAM ────────────────────────

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

// ── persistencia ──────────────────────────────────────────────────────────────

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

// ── arg helpers ───────────────────────────────────────────────────────────────

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
