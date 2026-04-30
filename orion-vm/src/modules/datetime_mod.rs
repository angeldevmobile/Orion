use crate::eval_value::EvalValue;
use chrono::{Local, NaiveDateTime, NaiveDate, Duration, Datelike, Timelike};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // now() → "2024-01-15T10:30:00"
        "now" => {
            Ok(EvalValue::Str(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // today() → "2024-01-15"
        "today" => {
            Ok(EvalValue::Str(Local::now().format("%Y-%m-%d").to_string()))
        }
        // timestamp() → unix timestamp en segundos
        "timestamp" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs();
            Ok(EvalValue::Int(ts as i64))
        }
        // timestamp_ms() → unix timestamp en milisegundos
        "timestamp_ms" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis();
            Ok(EvalValue::Int(ts as i64))
        }
        // format(dt_str, fmt) → string formateado
        "format" => {
            if args.is_empty() { return Err("datetime.format requiere (dt_str, fmt?)".into()); }
            let dt_str = to_str(&args[0]);
            let fmt = if args.len() > 1 { to_str(&args[1]) } else { "%Y-%m-%d %H:%M:%S".into() };
            let dt = parse_dt(&dt_str)?;
            Ok(EvalValue::Str(dt.format(&fmt).to_string()))
        }
        // parse(s, fmt?) → string ISO normalizado
        "parse" => {
            if args.is_empty() { return Err("datetime.parse requiere (s, fmt?)".into()); }
            let s   = to_str(&args[0]);
            let fmt = if args.len() > 1 { to_str(&args[1]) } else { "%Y-%m-%d %H:%M:%S".into() };
            let dt = NaiveDateTime::parse_from_str(&s, &fmt)
                .map_err(|e| format!("datetime.parse: {}", e))?;
            Ok(EvalValue::Str(dt.format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // add_days(dt_str, n) → nuevo string de fecha
        "add_days" => {
            if args.len() < 2 { return Err("datetime.add_days requiere (dt_str, days)".into()); }
            let dt_str = to_str(&args[0]);
            let days   = to_i64(&args[1])?;
            let dt = parse_dt(&dt_str)?;
            let new_dt = dt + Duration::days(days);
            Ok(EvalValue::Str(new_dt.format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // add_hours(dt_str, n)
        "add_hours" => {
            if args.len() < 2 { return Err("datetime.add_hours requiere (dt_str, hours)".into()); }
            let dt_str = to_str(&args[0]);
            let hours  = to_i64(&args[1])?;
            let dt = parse_dt(&dt_str)?;
            let new_dt = dt + Duration::hours(hours);
            Ok(EvalValue::Str(new_dt.format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // diff_days(a, b) → días entre dos fechas
        "diff_days" => {
            if args.len() < 2 { return Err("datetime.diff_days requiere (a, b)".into()); }
            let a = parse_dt(&to_str(&args[0]))?;
            let b = parse_dt(&to_str(&args[1]))?;
            let diff = (b - a).num_days();
            Ok(EvalValue::Int(diff))
        }
        // diff_seconds(a, b)
        "diff_seconds" => {
            if args.len() < 2 { return Err("datetime.diff_seconds requiere (a, b)".into()); }
            let a = parse_dt(&to_str(&args[0]))?;
            let b = parse_dt(&to_str(&args[1]))?;
            let diff = (b - a).num_seconds();
            Ok(EvalValue::Int(diff))
        }
        // parts(dt_str) → dict con year, month, day, hour, minute, second
        "parts" => {
            if args.is_empty() { return Err("datetime.parts requiere (dt_str)".into()); }
            let dt = parse_dt(&to_str(&args[0]))?;
            let mut m = std::collections::HashMap::new();
            m.insert("year".into(),   EvalValue::Int(dt.year() as i64));
            m.insert("month".into(),  EvalValue::Int(dt.month() as i64));
            m.insert("day".into(),    EvalValue::Int(dt.day() as i64));
            m.insert("hour".into(),   EvalValue::Int(dt.hour() as i64));
            m.insert("minute".into(), EvalValue::Int(dt.minute() as i64));
            m.insert("second".into(), EvalValue::Int(dt.second() as i64));
            Ok(EvalValue::Dict(m))
        }
        // weekday(dt_str) → "Monday", "Tuesday", etc.
        "weekday" => {
            if args.is_empty() { return Err("datetime.weekday requiere (dt_str)".into()); }
            let dt = parse_dt(&to_str(&args[0]))?;
            let names = ["Monday","Tuesday","Wednesday","Thursday","Friday","Saturday","Sunday"];
            let idx = dt.weekday().num_days_from_monday() as usize;
            Ok(EvalValue::Str(names[idx].to_string()))
        }
        // is_past / is_future
        "is_past" => {
            if args.is_empty() { return Err("datetime.is_past requiere (dt_str)".into()); }
            let dt  = parse_dt(&to_str(&args[0]))?;
            let now = Local::now().naive_local();
            Ok(EvalValue::Bool(dt < now))
        }
        "is_future" => {
            if args.is_empty() { return Err("datetime.is_future requiere (dt_str)".into()); }
            let dt  = parse_dt(&to_str(&args[0]))?;
            let now = Local::now().naive_local();
            Ok(EvalValue::Bool(dt > now))
        }
        // from_date(year, month, day) → string ISO
        "from_date" => {
            if args.len() < 3 { return Err("datetime.from_date requiere (year, month, day)".into()); }
            let y = to_i64(&args[0])? as i32;
            let m = to_i64(&args[1])? as u32;
            let d = to_i64(&args[2])? as u32;
            let date = NaiveDate::from_ymd_opt(y, m, d)
                .ok_or("datetime.from_date: fecha inválida")?;
            Ok(EvalValue::Str(date.format("%Y-%m-%d").to_string()))
        }

        f => Err(format!("datetime.{}() no existe", f)),
    }
}

fn parse_dt(s: &str) -> Result<chrono::NaiveDateTime, String> {
    // Intenta varios formatos comunes
    let formats = [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d",
    ];
    for fmt in formats {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Ok(dt);
        }
        // Intenta como solo fecha
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return Ok(d.and_hms_opt(0, 0, 0).unwrap());
        }
    }
    Err(format!("datetime: no se pudo parsear la fecha '{}'", s))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("datetime: esperaba número, recibió {}", other.type_name())),
    }
}
