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
            if args.is_empty() { return Err("datetime.format requires (dt_str, fmt?)".into()); }
            let dt_str = to_str(&args[0]);
            let fmt = if args.len() > 1 { to_str(&args[1]) } else { "%Y-%m-%d %H:%M:%S".into() };
            let dt = parse_dt(&dt_str)?;
            Ok(EvalValue::Str(dt.format(&fmt).to_string()))
        }
        // parse(s, fmt?) → string ISO normalizado
        "parse" => {
            if args.is_empty() { return Err("datetime.parse requires (s, fmt?)".into()); }
            let s   = to_str(&args[0]);
            let fmt = if args.len() > 1 { to_str(&args[1]) } else { "%Y-%m-%d %H:%M:%S".into() };
            let dt = NaiveDateTime::parse_from_str(&s, &fmt)
                .map_err(|e| format!("datetime.parse: {}", e))?;
            Ok(EvalValue::Str(dt.format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // add_days(dt_str, n) → nuevo string de fecha
        "add_days" => {
            if args.len() < 2 { return Err("datetime.add_days requires (dt_str, days)".into()); }
            let dt_str = to_str(&args[0]);
            let days   = to_i64(&args[1])?;
            let dt = parse_dt(&dt_str)?;
            let new_dt = dt + Duration::days(days);
            Ok(EvalValue::Str(new_dt.format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // add_hours(dt_str, n)
        "add_hours" => {
            if args.len() < 2 { return Err("datetime.add_hours requires (dt_str, hours)".into()); }
            let dt_str = to_str(&args[0]);
            let hours  = to_i64(&args[1])?;
            let dt = parse_dt(&dt_str)?;
            let new_dt = dt + Duration::hours(hours);
            Ok(EvalValue::Str(new_dt.format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // add_minutes(dt_str, n)
        "add_minutes" => {
            if args.len() < 2 { return Err("datetime.add_minutes requires (dt_str, minutes)".into()); }
            let dt = parse_dt(&to_str(&args[0]))?;
            let new_dt = dt + Duration::minutes(to_i64(&args[1])?);
            Ok(EvalValue::Str(new_dt.format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // add_months(dt_str, n) → ajusta el fin de mes (31 ene + 1 mes = 28/29 feb)
        "add_months" => {
            if args.len() < 2 { return Err("datetime.add_months requires (dt_str, months)".into()); }
            let dt = parse_dt(&to_str(&args[0]))?;
            let new_dt = shift_months(dt, to_i64(&args[1])?)?;
            Ok(EvalValue::Str(new_dt.format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // add_years(dt_str, n) → ajusta bisiestos (29 feb + 1 año = 28 feb)
        "add_years" => {
            if args.len() < 2 { return Err("datetime.add_years requires (dt_str, years)".into()); }
            let dt = parse_dt(&to_str(&args[0]))?;
            let new_dt = shift_months(dt, to_i64(&args[1])? * 12)?;
            Ok(EvalValue::Str(new_dt.format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // diff_days(a, b) → días entre dos fechas
        "diff_days" => {
            if args.len() < 2 { return Err("datetime.diff_days requires (a, b)".into()); }
            let a = parse_dt(&to_str(&args[0]))?;
            let b = parse_dt(&to_str(&args[1]))?;
            let diff = (b - a).num_days();
            Ok(EvalValue::Int(diff))
        }
        // diff_seconds(a, b)
        "diff_seconds" => {
            if args.len() < 2 { return Err("datetime.diff_seconds requires (a, b)".into()); }
            let a = parse_dt(&to_str(&args[0]))?;
            let b = parse_dt(&to_str(&args[1]))?;
            let diff = (b - a).num_seconds();
            Ok(EvalValue::Int(diff))
        }
        // diff_hours(a, b) → horas completas entre dos fechas
        "diff_hours" => {
            if args.len() < 2 { return Err("datetime.diff_hours requires (a, b)".into()); }
            let a = parse_dt(&to_str(&args[0]))?;
            let b = parse_dt(&to_str(&args[1]))?;
            Ok(EvalValue::Int((b - a).num_hours()))
        }
        // diff_minutes(a, b) → minutos completos entre dos fechas
        "diff_minutes" => {
            if args.len() < 2 { return Err("datetime.diff_minutes requires (a, b)".into()); }
            let a = parse_dt(&to_str(&args[0]))?;
            let b = parse_dt(&to_str(&args[1]))?;
            Ok(EvalValue::Int((b - a).num_minutes()))
        }
        // to_timestamp(dt_str) → unix segundos (la fecha se interpreta en hora local)
        "to_timestamp" => {
            use chrono::TimeZone;
            if args.is_empty() { return Err("datetime.to_timestamp requires (dt_str)".into()); }
            let dt = parse_dt(&to_str(&args[0]))?;
            let local = Local.from_local_datetime(&dt).single()
                .or_else(|| Local.from_local_datetime(&dt).earliest())
                .ok_or("datetime.to_timestamp: fecha ambigua en hora local")?;
            Ok(EvalValue::Int(local.timestamp()))
        }
        // from_timestamp(ts_segundos) → string ISO en hora local
        "from_timestamp" => {
            use chrono::TimeZone;
            if args.is_empty() { return Err("datetime.from_timestamp requires (ts_segundos)".into()); }
            let ts = to_i64(&args[0])?;
            let dt = Local.timestamp_opt(ts, 0).single()
                .ok_or("datetime.from_timestamp: timestamp inválido")?;
            Ok(EvalValue::Str(dt.format("%Y-%m-%dT%H:%M:%S").to_string()))
        }
        // start_of_month(dt_str) → "YYYY-MM-01"
        "start_of_month" => {
            if args.is_empty() { return Err("datetime.start_of_month requires (dt_str)".into()); }
            let dt = parse_dt(&to_str(&args[0]))?;
            let date = NaiveDate::from_ymd_opt(dt.year(), dt.month(), 1)
                .ok_or("datetime.start_of_month: fecha inválida")?;
            Ok(EvalValue::Str(date.format("%Y-%m-%d").to_string()))
        }
        // end_of_month(dt_str) → último día del mes ("YYYY-MM-28/29/30/31")
        "end_of_month" => {
            if args.is_empty() { return Err("datetime.end_of_month requires (dt_str)".into()); }
            let dt = parse_dt(&to_str(&args[0]))?;
            let last = last_day_of_month(dt.year(), dt.month());
            let date = NaiveDate::from_ymd_opt(dt.year(), dt.month(), last)
                .ok_or("datetime.end_of_month: fecha inválida")?;
            Ok(EvalValue::Str(date.format("%Y-%m-%d").to_string()))
        }
        // parts(dt_str) → dict con year, month, day, hour, minute, second
        "parts" => {
            if args.is_empty() { return Err("datetime.parts requires (dt_str)".into()); }
            let dt = parse_dt(&to_str(&args[0]))?;
            let mut m = indexmap::IndexMap::new();
            m.insert("year".into(),   EvalValue::Int(dt.year() as i64));
            m.insert("month".into(),  EvalValue::Int(dt.month() as i64));
            m.insert("day".into(),    EvalValue::Int(dt.day() as i64));
            m.insert("hour".into(),   EvalValue::Int(dt.hour() as i64));
            m.insert("minute".into(), EvalValue::Int(dt.minute() as i64));
            m.insert("second".into(), EvalValue::Int(dt.second() as i64));
            Ok(EvalValue::Dict(m))
        }
        // weekday(dt_str) → "Monday"…; weekday(dt_str, "es") → "lunes"…
        "weekday" => {
            if args.is_empty() { return Err("datetime.weekday requires (dt_str [, idioma])".into()); }
            let dt = parse_dt(&to_str(&args[0]))?;
            let idx = dt.weekday().num_days_from_monday() as usize;
            let es = matches!(args.get(1), Some(EvalValue::Str(s)) if s == "es");
            let name = if es {
                ["lunes","martes","miércoles","jueves","viernes","sábado","domingo"][idx]
            } else {
                ["Monday","Tuesday","Wednesday","Thursday","Friday","Saturday","Sunday"][idx]
            };
            Ok(EvalValue::Str(name.to_string()))
        }
        // is_past / is_future
        "is_past" => {
            if args.is_empty() { return Err("datetime.is_past requires (dt_str)".into()); }
            let dt  = parse_dt(&to_str(&args[0]))?;
            let now = Local::now().naive_local();
            Ok(EvalValue::Bool(dt < now))
        }
        "is_future" => {
            if args.is_empty() { return Err("datetime.is_future requires (dt_str)".into()); }
            let dt  = parse_dt(&to_str(&args[0]))?;
            let now = Local::now().naive_local();
            Ok(EvalValue::Bool(dt > now))
        }
        // from_date(year, month, day) → string ISO
        "from_date" => {
            if args.len() < 3 { return Err("datetime.from_date requires (year, month, day)".into()); }
            let y = to_i64(&args[0])? as i32;
            let m = to_i64(&args[1])? as u32;
            let d = to_i64(&args[2])? as u32;
            let date = NaiveDate::from_ymd_opt(y, m, d)
                .ok_or("datetime.from_date: fecha inválida")?;
            Ok(EvalValue::Str(date.format("%Y-%m-%d").to_string()))
        }

        f => Err(format!("datetime.{}() does not exist", f)),
    }
}

fn parse_dt(s: &str) -> Result<chrono::NaiveDateTime, String> {
    // Formatos comunes; día-primero antes que mes-primero en los ambiguos
    // (misma convención que table.cast(col, "date"))
    let formats = [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d",
        "%d/%m/%Y %H:%M:%S",
        "%d/%m/%Y",
        "%d-%m-%Y",
        "%d.%m.%Y",
        "%m/%d/%Y",
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
    Err(format!("datetime: could not parse the date '{}'", s))
}

fn last_day_of_month(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    (NaiveDate::from_ymd_opt(ny, nm, 1).unwrap() - Duration::days(1)).day()
}

// Suma meses ajustando el día al fin de mes destino (31 ene + 1 mes = 28/29 feb)
fn shift_months(dt: NaiveDateTime, months: i64) -> Result<NaiveDateTime, String> {
    let total = dt.year() as i64 * 12 + (dt.month() as i64 - 1) + months;
    let y = total.div_euclid(12) as i32;
    let m = (total.rem_euclid(12) + 1) as u32;
    let d = dt.day().min(last_day_of_month(y, m));
    let date = NaiveDate::from_ymd_opt(y, m, d)
        .ok_or("datetime: fecha resultante inválida")?;
    Ok(date.and_hms_opt(dt.hour(), dt.minute(), dt.second()).unwrap())
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("datetime: expected a number, got {}", other.type_name())),
    }
}
