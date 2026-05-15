use crate::eval_value::EvalValue;
use std::collections::HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        "mean"        => fn_mean(args),
        "median"      => fn_median(args),
        "mode"        => fn_mode(args),
        "std"         => fn_std(args),
        "variance"    => fn_variance(args),
        "min"         => fn_min(args),
        "max"         => fn_max(args),
        "range"       => fn_range(args),
        "sum"         => fn_sum(args),
        "percentile"  => fn_percentile(args),
        "iqr"         => fn_iqr(args),
        "zscore"      => fn_zscore(args),
        "normalize"   => fn_normalize(args),
        "correlation" => fn_correlation(args),
        "regression"  => fn_regression(args),
        "outliers"    => fn_outliers(args),
        "histogram"   => fn_histogram(args),
        "describe"    => fn_describe(args),
        _ => Err(format!("stat.{} no existe", function)),
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn to_floats(val: &EvalValue) -> Result<Vec<f64>, String> {
    match val {
        EvalValue::List(items) => items.iter().map(|v| match v {
            EvalValue::Int(n)   => Ok(*n as f64),
            EvalValue::Float(f) => Ok(*f),
            _ => Err("stat: la lista debe contener números".into()),
        }).collect(),
        _ => Err("stat: se esperaba una lista de números".into()),
    }
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
    let lo = idx.floor() as usize;
    let hi = (idx.ceil() as usize).min(sorted.len() - 1);
    if lo == hi { return sorted[lo]; }
    sorted[lo] + (sorted[hi] - sorted[lo]) * (idx - lo as f64)
}

// ── funciones ─────────────────────────────────────────────────────────────────

fn fn_mean(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let data = to_floats(args.first().ok_or("stat.mean(lista)")?)?;
    Ok(EvalValue::Float(mean_of(&data)))
}

fn fn_median(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let mut data = to_floats(args.first().ok_or("stat.median(lista)")?)?;
    data.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Ok(EvalValue::Float(percentile_of(&data, 50.0)))
}

fn fn_mode(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match args.first().ok_or("stat.mode(lista)")? {
        EvalValue::List(items) => {
            let mut counts: HashMap<String, (EvalValue, usize)> = HashMap::new();
            for item in items {
                let key = format!("{:?}", item);
                let entry = counts.entry(key).or_insert((item.clone(), 0));
                entry.1 += 1;
            }
            let best = counts.values().max_by_key(|(_, c)| *c);
            Ok(best.map(|(v, _)| v.clone()).unwrap_or(EvalValue::Null))
        }
        _ => Err("stat.mode(lista)".into()),
    }
}

fn fn_std(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let data = to_floats(args.first().ok_or("stat.std(lista)")?)?;
    Ok(EvalValue::Float(std_of(&data)))
}

fn fn_variance(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let data = to_floats(args.first().ok_or("stat.variance(lista)")?)?;
    if data.len() < 2 { return Ok(EvalValue::Float(0.0)); }
    let m = mean_of(&data);
    let var = data.iter().map(|x| (x - m).powi(2)).sum::<f64>() / data.len() as f64;
    Ok(EvalValue::Float(var))
}

fn fn_min(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let data = to_floats(args.first().ok_or("stat.min(lista)")?)?;
    Ok(EvalValue::Float(data.iter().cloned().fold(f64::INFINITY, f64::min)))
}

fn fn_max(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let data = to_floats(args.first().ok_or("stat.max(lista)")?)?;
    Ok(EvalValue::Float(data.iter().cloned().fold(f64::NEG_INFINITY, f64::max)))
}

fn fn_range(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let data = to_floats(args.first().ok_or("stat.range(lista)")?)?;
    let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    Ok(EvalValue::Float(max - min))
}

fn fn_sum(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let data = to_floats(args.first().ok_or("stat.sum(lista)")?)?;
    Ok(EvalValue::Float(data.iter().sum()))
}

fn fn_percentile(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("stat.percentile(lista, p)".into()); }
    let mut data = to_floats(&args[0])?;
    let p = match &args[1] {
        EvalValue::Int(n)   => *n as f64,
        EvalValue::Float(f) => *f,
        _ => return Err("stat.percentile: p debe ser número (0-100)".into()),
    };
    data.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Ok(EvalValue::Float(percentile_of(&data, p)))
}

fn fn_iqr(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let mut data = to_floats(args.first().ok_or("stat.iqr(lista)")?)?;
    data.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Ok(EvalValue::Float(percentile_of(&data, 75.0) - percentile_of(&data, 25.0)))
}

fn fn_zscore(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let data = to_floats(args.first().ok_or("stat.zscore(lista)")?)?;
    let m = mean_of(&data);
    let s = std_of(&data);
    let scores: Vec<EvalValue> = if s == 0.0 {
        vec![EvalValue::Float(0.0); data.len()]
    } else {
        data.iter().map(|x| EvalValue::Float((x - m) / s)).collect()
    };
    Ok(EvalValue::List(scores))
}

fn fn_normalize(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let data = to_floats(args.first().ok_or("stat.normalize(lista)")?)?;
    let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    let normed: Vec<EvalValue> = if range == 0.0 {
        vec![EvalValue::Float(0.0); data.len()]
    } else {
        data.iter().map(|x| EvalValue::Float((x - min) / range)).collect()
    };
    Ok(EvalValue::List(normed))
}

fn fn_correlation(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("stat.correlation(lista1, lista2)".into()); }
    let x = to_floats(&args[0])?;
    let y = to_floats(&args[1])?;
    if x.len() != y.len() || x.is_empty() {
        return Err("stat.correlation: listas deben tener la misma longitud".into());
    }
    let mx = mean_of(&x);
    let my = mean_of(&y);
    let num: f64 = x.iter().zip(y.iter()).map(|(a, b)| (a - mx) * (b - my)).sum();
    let den_x: f64 = x.iter().map(|a| (a - mx).powi(2)).sum::<f64>().sqrt();
    let den_y: f64 = y.iter().map(|b| (b - my).powi(2)).sum::<f64>().sqrt();
    if den_x * den_y == 0.0 { return Ok(EvalValue::Float(0.0)); }
    Ok(EvalValue::Float(num / (den_x * den_y)))
}

fn fn_regression(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.len() < 2 { return Err("stat.regression(x, y)".into()); }
    let x = to_floats(&args[0])?;
    let y = to_floats(&args[1])?;
    if x.len() < 2 || x.len() != y.len() {
        return Err("stat.regression: se necesitan al menos 2 puntos y misma longitud".into());
    }
    let mx = mean_of(&x);
    let my = mean_of(&y);
    let num: f64 = x.iter().zip(y.iter()).map(|(a, b)| (a - mx) * (b - my)).sum();
    let den: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
    let slope = if den == 0.0 { 0.0 } else { num / den };
    let intercept = my - slope * mx;
    let ss_res: f64 = x.iter().zip(y.iter()).map(|(a, b)| (b - (slope * a + intercept)).powi(2)).sum();
    let ss_tot: f64 = y.iter().map(|b| (b - my).powi(2)).sum();
    let r2 = if ss_tot == 0.0 { 1.0 } else { 1.0 - ss_res / ss_tot };
    let mut map = HashMap::new();
    map.insert("slope".to_string(),     EvalValue::Float(slope));
    map.insert("intercept".to_string(), EvalValue::Float(intercept));
    map.insert("r2".to_string(),        EvalValue::Float(r2));
    Ok(EvalValue::Dict(map))
}

fn fn_outliers(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let mut data = to_floats(args.first().ok_or("stat.outliers(lista)")?)?;
    data.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q1  = percentile_of(&data, 25.0);
    let q3  = percentile_of(&data, 75.0);
    let iqr = q3 - q1;
    let lo  = q1 - 1.5 * iqr;
    let hi  = q3 + 1.5 * iqr;
    let out: Vec<EvalValue> = data.iter()
        .filter(|&&x| x < lo || x > hi)
        .map(|&x| EvalValue::Float(x))
        .collect();
    Ok(EvalValue::List(out))
}

fn fn_histogram(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    if args.is_empty() { return Err("stat.histogram(lista, bins?)".into()); }
    let data = to_floats(&args[0])?;
    let bins = match args.get(1) {
        Some(EvalValue::Int(n)) => *n as usize,
        _ => 10,
    };
    if data.is_empty() || bins == 0 {
        return Err("stat.histogram: datos o bins inválidos".into());
    }
    let min = data.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let width = if max == min { 1.0 } else { (max - min) / bins as f64 };
    let mut counts = vec![0i64; bins];
    let mut edges: Vec<EvalValue> = (0..=bins)
        .map(|i| EvalValue::Float(min + i as f64 * width))
        .collect();
    // último borde es exactamente max
    if let Some(last) = edges.last_mut() { *last = EvalValue::Float(max); }
    for &v in &data {
        let idx = ((v - min) / width).floor() as usize;
        counts[idx.min(bins - 1)] += 1;
    }
    let mut map = HashMap::new();
    map.insert("bins".to_string(),   EvalValue::List(edges));
    map.insert("counts".to_string(), EvalValue::List(counts.into_iter().map(EvalValue::Int).collect()));
    Ok(EvalValue::Dict(map))
}

fn fn_describe(args: Vec<EvalValue>) -> Result<EvalValue, String> {
    let mut data = to_floats(args.first().ok_or("stat.describe(lista)")?)?;
    if data.is_empty() { return Err("stat.describe: lista vacía".into()); }
    data.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = data.len();
    let mut map = HashMap::new();
    map.insert("count".to_string(),  EvalValue::Int(n as i64));
    map.insert("mean".to_string(),   EvalValue::Float(mean_of(&data)));
    map.insert("std".to_string(),    EvalValue::Float(std_of(&data)));
    map.insert("min".to_string(),    EvalValue::Float(data[0]));
    map.insert("p25".to_string(),    EvalValue::Float(percentile_of(&data, 25.0)));
    map.insert("median".to_string(), EvalValue::Float(percentile_of(&data, 50.0)));
    map.insert("p75".to_string(),    EvalValue::Float(percentile_of(&data, 75.0)));
    map.insert("max".to_string(),    EvalValue::Float(data[n - 1]));
    Ok(EvalValue::Dict(map))
}
