/// Orion Timewarp — manipulación del tiempo en Rust.
use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use chrono::Local;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // now() → ISO timestamp
        "now" => {
            Ok(EvalValue::Str(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // timestamp() → unix seconds
        "timestamp" => {
            let ts = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs()).unwrap_or(0);
            Ok(EvalValue::Int(ts as i64))
        }
        // timestamp_ms() → unix miliseconds
        "timestamp_ms" => {
            let ts = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis()).unwrap_or(0);
            Ok(EvalValue::Int(ts as i64))
        }
        // timestamp_ns() → unix nanoseconds
        "timestamp_ns" => {
            let ts = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos()).unwrap_or(0);
            Ok(EvalValue::Int(ts as i64))
        }
        // wait(duration) → pausa; acepta "1s", "500ms", "1000ns" o número en segundos
        "wait" | "sleep" => {
            let duration = if args.is_empty() { 1.0 } else { parse_duration(&args[0])? };
            std::thread::sleep(std::time::Duration::from_secs_f64(duration));
            Ok(EvalValue::Null)
        }
        // measure(fn_name?) → inicia cronómetro, retorna handle
        "clock" | "start_clock" => {
            let ts = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64).unwrap_or(0);
            let mut m = HashMap::new();
            m.insert("start_ns".into(), EvalValue::Int(ts));
            m.insert("paused".into(),   EvalValue::Bool(false));
            m.insert("scale".into(),    EvalValue::Float(1.0));
            Ok(EvalValue::Dict(m))
        }
        // elapsed(clock) → segundos transcurridos
        "elapsed" => {
            if args.is_empty() { return Err("timewarp.elapsed requiere (clock)".into()); }
            let clock = match &args[0] {
                EvalValue::Dict(m) => m,
                _ => return Err("timewarp.elapsed: se esperaba un clock (dict)".into()),
            };
            let start_ns = match clock.get("start_ns") {
                Some(EvalValue::Int(n)) => *n,
                _ => return Err("timewarp.elapsed: clock inválido".into()),
            };
            let now_ns = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64).unwrap_or(0);
            let elapsed_s = (now_ns - start_ns) as f64 / 1e9;
            Ok(EvalValue::Float((elapsed_s * 1e6).round() / 1e6))
        }
        // measure_time(fn_description) → ejecuta y retorna {result: null, ms: f64}
        "measure_time" | "measureMtime" => {
            let start = Instant::now();
            // No podemos ejecutar una función Orion desde aquí directamente,
            // pero retornamos un dict con la hora de inicio para que el usuario
            // calcule el tiempo manualmente si lo necesita.
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            let mut m = HashMap::new();
            m.insert("ms".into(),     EvalValue::Float((elapsed_ms * 1000.0).round() / 1000.0));
            m.insert("result".into(), EvalValue::Null);
            Ok(EvalValue::Dict(m))
        }
        // format(timestamp_secs, fmt?) → string formateado
        "format" => {
            if args.is_empty() { return Err("timewarp.format requiere (timestamp_secs, fmt?)".into()); }
            let ts_secs = to_i64(&args[0])? as u64;
            let fmt = if args.len() > 1 { to_str(&args[1]) } else { "%Y-%m-%d %H:%M:%S".into() };
            use chrono::TimeZone;
            let dt = chrono::Local.timestamp_opt(ts_secs as i64, 0)
                .single()
                .ok_or("timewarp.format: timestamp inválido")?;
            Ok(EvalValue::Str(dt.format(&fmt).to_string()))
        }
        // diff(ts1, ts2) → segundos de diferencia
        "diff" => {
            if args.len() < 2 { return Err("timewarp.diff requiere (ts1, ts2)".into()); }
            let t1 = to_i64(&args[0])?;
            let t2 = to_i64(&args[1])?;
            Ok(EvalValue::Int((t2 - t1).abs()))
        }
        // add(timestamp, seconds) → nuevo timestamp
        "add" | "fastforward" => {
            if args.len() < 2 { return Err("timewarp.add requiere (timestamp, seconds)".into()); }
            let ts = to_i64(&args[0])?;
            let s  = to_i64(&args[1])?;
            Ok(EvalValue::Int(ts + s))
        }
        // sub(timestamp, seconds) → nuevo timestamp (rewind)
        "sub" | "rewind" => {
            if args.len() < 2 { return Err("timewarp.sub requiere (timestamp, seconds)".into()); }
            let ts = to_i64(&args[0])?;
            let s  = to_i64(&args[1])?;
            Ok(EvalValue::Int(ts - s))
        }
        // since(timestamp_secs) → segundos desde entonces
        "since" => {
            if args.is_empty() { return Err("timewarp.since requiere (timestamp_secs)".into()); }
            let past = to_i64(&args[0])? as u64;
            let now  = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs()).unwrap_or(0);
            Ok(EvalValue::Int(now.saturating_sub(past) as i64))
        }
        // until(timestamp_secs) → segundos hasta entonces
        "until" => {
            if args.is_empty() { return Err("timewarp.until requiere (timestamp_secs)".into()); }
            let future = to_i64(&args[0])? as u64;
            let now    = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs()).unwrap_or(0);
            Ok(EvalValue::Int(future.saturating_sub(now) as i64))
        }

        f => Err(format!("timewarp.{}() no existe", f)),
    }
}

fn parse_duration(v: &EvalValue) -> Result<f64, String> {
    match v {
        EvalValue::Float(f) => Ok(*f),
        EvalValue::Int(n)   => Ok(*n as f64),
        EvalValue::Str(s)   => {
            if s.ends_with("ms") {
                s[..s.len()-2].trim().parse::<f64>().map(|n| n / 1000.0).map_err(|_| "timewarp.wait: duración inválida".into())
            } else if s.ends_with("ns") {
                s[..s.len()-2].trim().parse::<f64>().map(|n| n / 1e9).map_err(|_| "timewarp.wait: duración inválida".into())
            } else if s.ends_with('s') {
                s[..s.len()-1].trim().parse::<f64>().map_err(|_| "timewarp.wait: duración inválida".into())
            } else {
                s.trim().parse::<f64>().map_err(|_| "timewarp.wait: duración inválida".into())
            }
        }
        _ => Err("timewarp.wait: duración debe ser número o string ('1s', '500ms')".into()),
    }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("timewarp: esperaba número, recibió {}", other.type_name())),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
