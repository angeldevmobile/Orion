use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

struct Serie {
    values:     Vec<f64>,
    timestamps: Option<Vec<String>>,
}

static SERIES:  Mutex<Option<HashMap<String, Serie>>> = Mutex::new(None);
static COUNTER: AtomicU64 = AtomicU64::new(1);

fn with_series<F, T>(f: F) -> T
where
    F: FnOnce(&mut HashMap<String, Serie>) -> T,
{
    let mut guard = SERIES.lock().unwrap();
    if guard.is_none() { *guard = Some(HashMap::new()); }
    f(guard.as_mut().unwrap())
}

fn new_handle() -> String {
    format!("serie_{}", COUNTER.fetch_add(1, Ordering::SeqCst))
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn arg_handle(args: &[EvalValue], pos: usize) -> Result<String, String> {
    match args.get(pos) {
        Some(EvalValue::Str(s)) => Ok(s.clone()),
        _ => Err("serie: se esperaba un handle de serie (string)".into()),
    }
}

fn arg_window(args: &[EvalValue], pos: usize, name: &str) -> Result<usize, String> {
    match args.get(pos) {
        Some(EvalValue::Int(n)) if *n > 0 => Ok(*n as usize),
        _ => Err(format!("serie.{}: window debe ser un entero positivo", name)),
    }
}

fn list_to_floats(list: &[EvalValue]) -> Result<Vec<f64>, String> {
    list.iter().map(|v| match v {
        EvalValue::Int(n)   => Ok(*n as f64),
        EvalValue::Float(f) => Ok(*f),
        EvalValue::Dict(d)  => match d.get("v").or_else(|| d.get("value")) {
            Some(EvalValue::Int(n))   => Ok(*n as f64),
            Some(EvalValue::Float(f)) => Ok(*f),
            _ => Err("serie: dict debe tener campo 'v' numérico".into()),
        },
        _ => Err("serie: la lista debe contener números o {t, v}".into()),
    }).collect()
}

fn list_to_timestamps(list: &[EvalValue]) -> Option<Vec<String>> {
    let ts: Vec<String> = list.iter().filter_map(|v| match v {
        EvalValue::Dict(d) => {
            ["t", "timestamp", "date", "fecha", "time"].iter()
                .find_map(|k| match d.get(*k) {
                    Some(EvalValue::Str(s)) => Some(s.clone()),
                    _ => None,
                })
        }
        _ => None,
    }).collect();
    if ts.len() == list.len() { Some(ts) } else { None }
}

fn mean_of(data: &[f64]) -> f64 {
    if data.is_empty() { return 0.0; }
    data.iter().sum::<f64>() / data.len() as f64
}

fn std_of(data: &[f64]) -> f64 {
    if data.len() < 2 { return 0.0; }
    let m = mean_of(data);
    let var = data.iter().map(|x| (x - m).powi(2)).sum::<f64>() / data.len() as f64;
    var.sqrt()
}

fn percentile_of(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let idx = p / 100.0 * (sorted.len() - 1) as f64;
    let lo  = idx.floor() as usize;
    let hi  = (idx.ceil() as usize).min(sorted.len() - 1);
    if lo == hi { return sorted[lo]; }
    sorted[lo] + (sorted[hi] - sorted[lo]) * (idx - lo as f64)
}

fn linear_regression(vals: &[f64]) -> (f64, f64) {
    let n = vals.len();
    let x_mean = (n - 1) as f64 / 2.0;
    let y_mean = mean_of(vals);
    let num: f64 = vals.iter().enumerate().map(|(i, &y)| (i as f64 - x_mean) * (y - y_mean)).sum();
    let den: f64 = (0..n).map(|i| (i as f64 - x_mean).powi(2)).sum();
    let slope = if den == 0.0 { 0.0 } else { num / den };
    let intercept = y_mean - slope * x_mean;
    (slope, intercept)
}

fn store_serie(values: Vec<f64>, timestamps: Option<Vec<String>>) -> String {
    let id = new_handle();
    with_series(|s| s.insert(id.clone(), Serie { values, timestamps }));
    id
}

// ── dispatcher ────────────────────────────────────────────────────────────────

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        "new"         => fn_new(args),
        "from_table"  => fn_from_table(args),
        "values"      => fn_values(args),
        "timestamps"  => fn_timestamps(args),
        "len"         => fn_len(args),
        "add"         => fn_add(args),
        "slice"       => fn_slice(args),
        "peek"        => fn_peek(args),
        "moving_avg"  => fn_moving_avg(args),
        "rolling_std" => fn_rolling_std(args),
        "diff"        => fn_diff(args),
        "pct_change"  => fn_pct_change(args),
        "cumsum"      => fn_cumsum(args),
        "smooth"      => fn_smooth(args),
        "forecast"    => fn_forecast(args),
        "trend"       => fn_trend(args),
        "anomalies"   => fn_anomalies(args),
        "describe"    => fn_describe(args),
        _ => Err(format!("serie.{} no existe", function)),
    }
}

// ── construcción ─────────────────────────────────────────────────────────────

fn fn_new(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match args.first().ok_or("serie.new(lista)")? {
        EvalValue::List(items) => {
            let values     = list_to_floats(items)?;
            let timestamps = list_to_timestamps(items);
            Ok(EvalValue::Str(store_serie(values, timestamps)))
        }
        _ => Err("serie.new: se esperaba una lista".into()),
    }
}

fn fn_from_table(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("serie.from_table(tabla, columna)".into()); }
    let col = match &args[1] {
        EvalValue::Str(s) => s.clone(),
        _ => return Err("serie.from_table: columna debe ser string".into()),
    };
    match &args[0] {
        EvalValue::List(rows) => {
            let mut values     = Vec::new();
            let mut timestamps = Vec::new();
            let mut has_ts     = false;
            for row in rows {
                if let EvalValue::Dict(d) = row {
                    let v = match d.get(&col) {
                        Some(EvalValue::Int(n))   => *n as f64,
                        Some(EvalValue::Float(f)) => *f,
                        _ => continue,
                    };
                    values.push(v);
                    for k in &["fecha", "date", "t", "timestamp", "time"] {
                        if let Some(EvalValue::Str(ts)) = d.get(*k) {
                            timestamps.push(ts.clone());
                            has_ts = true;
                            break;
                        }
                    }
                }
            }
            let ts = if has_ts && timestamps.len() == values.len() { Some(timestamps) } else { None };
            Ok(EvalValue::Str(store_serie(values, ts)))
        }
        _ => Err("serie.from_table: se esperaba una lista de dicts".into()),
    }
}

// ── lectura ───────────────────────────────────────────────────────────────────

fn fn_values(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_series(|s| match s.get(&id) {
        Some(sr) => Ok(EvalValue::List(sr.values.iter().map(|&v| EvalValue::Float(v)).collect())),
        None => Err(format!("serie '{}' no existe", id)),
    })
}

fn fn_timestamps(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_series(|s| match s.get(&id) {
        Some(sr) => Ok(EvalValue::List(
            sr.timestamps.as_deref().unwrap_or(&[]).iter()
                .map(|t| EvalValue::Str(t.clone())).collect()
        )),
        None => Err(format!("serie '{}' no existe", id)),
    })
}

fn fn_len(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_series(|s| match s.get(&id) {
        Some(sr) => Ok(EvalValue::Int(sr.values.len() as i64)),
        None => Err(format!("serie '{}' no existe", id)),
    })
}

fn fn_add(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("serie.add(handle, valor)".into()); }
    let id = arg_handle(&args, 0)?;
    let v  = match &args[1] {
        EvalValue::Int(n)   => *n as f64,
        EvalValue::Float(f) => *f,
        _ => return Err("serie.add: valor debe ser número".into()),
    };
    with_series(|s| match s.get_mut(&id) {
        Some(sr) => { sr.values.push(v); Ok(EvalValue::Int(sr.values.len() as i64)) }
        None => Err(format!("serie '{}' no existe", id)),
    })
}

fn fn_slice(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 3 { return Err("serie.slice(handle, desde, hasta)".into()); }
    let id    = arg_handle(&args, 0)?;
    let desde = match &args[1] { EvalValue::Int(n) => *n as usize, _ => return Err("serie.slice: índices deben ser int".into()) };
    let hasta = match &args[2] { EvalValue::Int(n) => *n as usize, _ => return Err("serie.slice: índices deben ser int".into()) };
    with_series(|s| {
        let sr  = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        let end = hasta.min(sr.values.len());
        let ini = desde.min(end);
        let values     = sr.values[ini..end].to_vec();
        let timestamps = sr.timestamps.as_ref().map(|ts| ts[ini..end.min(ts.len())].to_vec());
        Ok(EvalValue::Str(store_serie(values, timestamps)))
    })
}

fn fn_peek(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    let n  = match args.get(1) { Some(EvalValue::Int(n)) => *n as usize, _ => 10 };
    with_series(|s| {
        let sr    = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        let total = sr.values.len();
        let show  = n.min(total);
        println!("Serie ({} valores{}):", total,
            if sr.timestamps.is_some() { " con timestamps" } else { "" });
        for i in 0..show {
            match sr.timestamps.as_ref().and_then(|ts| ts.get(i)) {
                Some(ts) => println!("  [{}] {}  →  {:.4}", i, ts, sr.values[i]),
                None     => println!("  [{}]  {:.4}", i, sr.values[i]),
            }
        }
        if total > show { println!("  ... ({} más)", total - show); }
        Ok(EvalValue::Null)
    })
}

// ── transformaciones ──────────────────────────────────────────────────────────

fn fn_moving_avg(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id     = arg_handle(&args, 0)?;
    let window = arg_window(&args, 1, "moving_avg")?;
    with_series(|s| {
        let sr     = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        let vals   = &sr.values;
        let result: Vec<f64> = (0..vals.len()).map(|i| {
            let start = i.saturating_sub(window - 1);
            let slice = &vals[start..=i];
            slice.iter().sum::<f64>() / slice.len() as f64
        }).collect();
        let ts = sr.timestamps.clone();
        Ok(EvalValue::Str(store_serie(result, ts)))
    })
}

fn fn_rolling_std(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id     = arg_handle(&args, 0)?;
    let window = arg_window(&args, 1, "rolling_std")?;
    with_series(|s| {
        let sr   = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        let vals = &sr.values;
        let result: Vec<f64> = (0..vals.len()).map(|i| {
            let start = i.saturating_sub(window - 1);
            let slice = &vals[start..=i];
            std_of(slice)
        }).collect();
        let ts = sr.timestamps.clone();
        Ok(EvalValue::Str(store_serie(result, ts)))
    })
}

fn fn_diff(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_series(|s| {
        let sr = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        if sr.values.len() < 2 { return Ok(EvalValue::Str(id.clone())); }
        let result: Vec<f64> = sr.values.windows(2).map(|w| w[1] - w[0]).collect();
        let ts = sr.timestamps.as_ref().map(|ts| ts[1..].to_vec());
        Ok(EvalValue::Str(store_serie(result, ts)))
    })
}

fn fn_pct_change(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_series(|s| {
        let sr = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        if sr.values.len() < 2 { return Ok(EvalValue::Str(id.clone())); }
        let result: Vec<f64> = sr.values.windows(2).map(|w| {
            if w[0] == 0.0 { 0.0 } else { (w[1] - w[0]) / w[0] * 100.0 }
        }).collect();
        let ts = sr.timestamps.as_ref().map(|ts| ts[1..].to_vec());
        Ok(EvalValue::Str(store_serie(result, ts)))
    })
}

fn fn_cumsum(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_series(|s| {
        let sr = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        let mut acc = 0.0_f64;
        let result: Vec<f64> = sr.values.iter().map(|&v| { acc += v; acc }).collect();
        let ts = sr.timestamps.clone();
        Ok(EvalValue::Str(store_serie(result, ts)))
    })
}

fn fn_smooth(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("serie.smooth(handle, alpha)  — alpha entre 0.0 y 1.0".into()); }
    let id    = arg_handle(&args, 0)?;
    let alpha = match &args[1] {
        EvalValue::Float(f) => *f,
        EvalValue::Int(n)   => *n as f64,
        _ => return Err("serie.smooth: alpha debe ser número".into()),
    };
    with_series(|s| {
        let sr = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        if sr.values.is_empty() { return Ok(EvalValue::Str(id.clone())); }
        let mut result = Vec::with_capacity(sr.values.len());
        result.push(sr.values[0]);
        for &v in &sr.values[1..] {
            let prev = *result.last().unwrap();
            result.push(alpha * v + (1.0 - alpha) * prev);
        }
        let ts = sr.timestamps.clone();
        Ok(EvalValue::Str(store_serie(result, ts)))
    })
}

// ── análisis ──────────────────────────────────────────────────────────────────

fn fn_forecast(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("serie.forecast(handle, n)".into()); }
    let id = arg_handle(&args, 0)?;
    let n  = match &args[1] {
        EvalValue::Int(n) if *n > 0 => *n as usize,
        _ => return Err("serie.forecast: n debe ser entero positivo".into()),
    };
    with_series(|s| {
        let sr = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        if sr.values.len() < 2 { return Err("serie.forecast: necesita al menos 2 puntos".into()); }
        let (slope, intercept) = linear_regression(&sr.values);
        let base = sr.values.len();
        let preds: Vec<EvalValue> = (0..n)
            .map(|i| EvalValue::Float(slope * (base + i) as f64 + intercept))
            .collect();
        Ok(EvalValue::List(preds))
    })
}

fn fn_trend(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_series(|s| {
        let sr = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        if sr.values.len() < 2 { return Err("serie.trend: necesita al menos 2 puntos".into()); }
        let (slope, intercept) = linear_regression(&sr.values);
        let my     = mean_of(&sr.values);
        let ss_res: f64 = sr.values.iter().enumerate()
            .map(|(i, &y)| (y - (slope * i as f64 + intercept)).powi(2)).sum();
        let ss_tot: f64 = sr.values.iter().map(|&y| (y - my).powi(2)).sum();
        let r2 = if ss_tot == 0.0 { 1.0 } else { 1.0 - ss_res / ss_tot };
        let direction = if slope > 0.001 { "up" } else if slope < -0.001 { "down" } else { "flat" };
        let mut map = HashMap::new();
        map.insert("direction".to_string(), EvalValue::Str(direction.into()));
        map.insert("slope".to_string(),     EvalValue::Float(slope));
        map.insert("r2".to_string(),        EvalValue::Float(r2));
        Ok(EvalValue::Dict(map))
    })
}

fn fn_anomalies(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_series(|s| {
        let sr = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        let mut sorted = sr.values.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let q1  = percentile_of(&sorted, 25.0);
        let q3  = percentile_of(&sorted, 75.0);
        let iqr = q3 - q1;
        let lo  = q1 - 1.5 * iqr;
        let hi  = q3 + 1.5 * iqr;
        let indices: Vec<EvalValue> = sr.values.iter().enumerate()
            .filter(|(_, &v)| v < lo || v > hi)
            .map(|(i, _)| EvalValue::Int(i as i64))
            .collect();
        Ok(EvalValue::List(indices))
    })
}

fn fn_describe(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let id = arg_handle(&args, 0)?;
    with_series(|s| {
        let sr = s.get(&id).ok_or(format!("serie '{}' no existe", id))?;
        if sr.values.is_empty() { return Err("serie.describe: serie vacía".into()); }
        let mut sorted = sr.values.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = sorted.len();
        let mut map = HashMap::new();
        map.insert("count".to_string(),  EvalValue::Int(n as i64));
        map.insert("mean".to_string(),   EvalValue::Float(mean_of(&sorted)));
        map.insert("std".to_string(),    EvalValue::Float(std_of(&sorted)));
        map.insert("min".to_string(),    EvalValue::Float(sorted[0]));
        map.insert("p25".to_string(),    EvalValue::Float(percentile_of(&sorted, 25.0)));
        map.insert("median".to_string(), EvalValue::Float(percentile_of(&sorted, 50.0)));
        map.insert("p75".to_string(),    EvalValue::Float(percentile_of(&sorted, 75.0)));
        map.insert("max".to_string(),    EvalValue::Float(sorted[n - 1]));
        Ok(EvalValue::Dict(map))
    })
}
