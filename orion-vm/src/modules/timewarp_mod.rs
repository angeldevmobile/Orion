/// Orion Timewarp — manipulación del tiempo en Rust.
use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
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
        // clock() → cronómetro (dict con start_ns); medir con elapsed(clock)
        "clock" | "start_clock" => {
            let ts = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64).unwrap_or(0);
            let mut m = HashMap::new();
            m.insert("start_ns".into(), EvalValue::Int(ts));
            Ok(EvalValue::Dict(m))
        }
        // elapsed(clock) → segundos transcurridos
        "elapsed" => {
            if args.is_empty() { return Err("timewarp.elapsed requires (clock)".into()); }
            let clock = match &args[0] {
                EvalValue::Dict(m) => m,
                _ => return Err("timewarp.elapsed: expected a clock (dict)".into()),
            };
            let start_ns = match clock.get("start_ns") {
                Some(EvalValue::Int(n)) => *n,
                _ => return Err("timewarp.elapsed: invalid clock".into()),
            };
            let now_ns = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64).unwrap_or(0);
            let elapsed_s = (now_ns - start_ns) as f64 / 1e9;
            Ok(EvalValue::Float((elapsed_s * 1e6).round() / 1e6))
        }
        // measure_time fue retirado: devolvía siempre ~0 ms (no puede ejecutar
        // una fn de Orion desde el módulo). El patrón honesto es clock/elapsed.
        "measure_time" | "measureMtime" => {
            Err("timewarp.measure_time fue retirado (medía siempre 0). Usa:\n  \
                 t = timewarp.clock()\n  ... tu código ...\n  \
                 segundos = timewarp.elapsed(t)".into())
        }
        // format(timestamp_secs, fmt?) → string formateado
        "format" => {
            if args.is_empty() { return Err("timewarp.format requires (timestamp_secs, fmt?)".into()); }
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
            if args.len() < 2 { return Err("timewarp.diff requires (ts1, ts2)".into()); }
            let t1 = to_i64(&args[0])?;
            let t2 = to_i64(&args[1])?;
            Ok(EvalValue::Int((t2 - t1).abs()))
        }
        // add(timestamp, seconds) → nuevo timestamp
        "add" | "fastforward" => {
            if args.len() < 2 { return Err("timewarp.add requires (timestamp, seconds)".into()); }
            let ts = to_i64(&args[0])?;
            let s  = to_i64(&args[1])?;
            Ok(EvalValue::Int(ts + s))
        }
        // sub(timestamp, seconds) → nuevo timestamp (rewind)
        "sub" | "rewind" => {
            if args.len() < 2 { return Err("timewarp.sub requires (timestamp, seconds)".into()); }
            let ts = to_i64(&args[0])?;
            let s  = to_i64(&args[1])?;
            Ok(EvalValue::Int(ts - s))
        }
        // since(timestamp_secs) → segundos desde entonces
        "since" => {
            if args.is_empty() { return Err("timewarp.since requires (timestamp_secs)".into()); }
            let past = to_i64(&args[0])? as u64;
            let now  = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs()).unwrap_or(0);
            Ok(EvalValue::Int(now.saturating_sub(past) as i64))
        }
        // until(timestamp_secs) → segundos hasta entonces
        "until" => {
            if args.is_empty() { return Err("timewarp.until requires (timestamp_secs)".into()); }
            let future = to_i64(&args[0])? as u64;
            let now    = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs()).unwrap_or(0);
            Ok(EvalValue::Int(future.saturating_sub(now) as i64))
        }

        f => Err(format!("timewarp.{}() does not exist", f)),
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
        _ => Err("timewarp.wait: the duration must be a number or a string ('1s', '500ms')".into()),
    }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("timewarp: expected a number, got {}", other.type_name())),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
