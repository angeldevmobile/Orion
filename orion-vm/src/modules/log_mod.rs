use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// 0=debug  1=info  2=warn  3=error
static LOG_LEVEL: AtomicU8 = AtomicU8::new(1);
static LOG_FILE:  Mutex<Option<String>>                = Mutex::new(None);
static TIMERS:    Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // ── Niveles estándar ────────────────────────────────────────────────────
        "info" | "warn" | "err" | "debug" | "ok" => {
            if args.is_empty() {
                return Err(format!("log.{} requiere (msg, tag?)", function));
            }
            let msg = to_str(&args[0]);
            let tag = if args.len() > 1 { Some(to_str(&args[1])) } else { None };
            let level = if function == "err" { "error" } else { function };
            write_log(level, &msg, tag.as_deref());
            Ok(EvalValue::Null)
        }

        // ── Configuración ───────────────────────────────────────────────────────
        "level" => {
            if args.is_empty() {
                return Err("log.level requiere (\"debug\"|\"info\"|\"warn\"|\"error\")".into());
            }
            LOG_LEVEL.store(level_num(&to_str(&args[0])), Ordering::Relaxed);
            Ok(EvalValue::Null)
        }
        "to_file" | "file" => {
            let mut guard = LOG_FILE.lock().unwrap();
            *guard = if args.is_empty() { None } else { Some(to_str(&args[0])) };
            Ok(EvalValue::Null)
        }

        // ── Separador visual ────────────────────────────────────────────────────
        // divider(label?) → imprime una línea separadora
        "divider" | "separator" => {
            let label = if args.is_empty() { String::new() } else { to_str(&args[0]) };
            print_divider(&label);
            Ok(EvalValue::Null)
        }

        // ── Timers ──────────────────────────────────────────────────────────────
        // timer(name) → inicia un temporizador nombrado
        "timer" | "start" => {
            if args.is_empty() { return Err("log.timer requiere (name)".into()); }
            let name = to_str(&args[0]);
            let mut guard = TIMERS.lock().unwrap();
            guard.get_or_insert_with(HashMap::new).insert(name.clone(), Instant::now());
            write_log("debug", &format!("timer '{}' iniciado", name), None);
            Ok(EvalValue::Null)
        }
        // elapsed(name, msg?) → muestra el tiempo transcurrido desde timer(name)
        "elapsed" | "stop" => {
            if args.is_empty() { return Err("log.elapsed requiere (name, msg?)".into()); }
            let name  = to_str(&args[0]);
            let label = if args.len() > 1 { to_str(&args[1]) } else { format!("'{}'", name) };
            let guard = TIMERS.lock().unwrap();
            match guard.as_ref().and_then(|m| m.get(&name)) {
                Some(start) => {
                    let ms = start.elapsed().as_millis();
                    let msg = format!("{} completado en {}ms", label, ms);
                    drop(guard);
                    write_log("ok", &msg, Some(&name));
                    Ok(EvalValue::Int(ms as i64))
                }
                None => Err(format!("log.elapsed: timer '{}' no existe — usa log.timer(name) primero", name)),
            }
        }

        // ── Impresión explícita ─────────────────────────────────────────────────
        "print" => {
            if args.len() < 2 { return Err("log.print requiere (level, msg, tag?)".into()); }
            let level = to_str(&args[0]);
            let msg   = to_str(&args[1]);
            let tag   = if args.len() > 2 { Some(to_str(&args[2])) } else { None };
            write_log(&level, &msg, tag.as_deref());
            Ok(EvalValue::Null)
        }

        f => Err(format!("log.{}() no existe", f)),
    }
}

// ── Render ────────────────────────────────────────────────────────────────────

fn write_log(level: &str, msg: &str, tag: Option<&str>) {
    if level_num(level) < LOG_LEVEL.load(Ordering::Relaxed) { return; }

    let ts    = full_timestamp();
    let badge = level_badge(level);
    let color = level_color(level);
    let reset = "\x1b[0m";
    let bold  = "\x1b[1m";

    // Tag formateado con color suave
    let tag_part = match tag {
        Some(t) => format!(" \x1b[90m[{}]\x1b[0m", t),
        None    => String::new(),
    };

    let line = format!(
        "{ts}  {color}{bold}{badge}{reset}{tag_part}  {msg}",
        ts    = ts,
        color = color,
        bold  = bold,
        badge = badge,
        reset = reset,
    );
    println!("{}", line);

    // Guardar en archivo sin ANSI
    if let Ok(guard) = LOG_FILE.lock() {
        if let Some(path) = guard.as_ref() {
            let tag_plain = tag.map(|t| format!(" [{}]", t)).unwrap_or_default();
            let plain = format!("{ts}  {badge}{tag_plain}  {msg}\n");
            let _ = std::fs::OpenOptions::new()
                .create(true).append(true).open(path)
                .and_then(|mut f| f.write_all(plain.as_bytes()));
        }
    }
}

fn print_divider(label: &str) {
    let width = 60usize;
    let line = if label.is_empty() {
        "\x1b[90m".to_string() + &"─".repeat(width) + "\x1b[0m"
    } else {
        let pad = width.saturating_sub(label.len() + 2);
        let left = pad / 2;
        let right = pad - left;
        format!(
            "\x1b[90m{} {} {}\x1b[0m",
            "─".repeat(left),
            label,
            "─".repeat(right)
        )
    };
    println!("{}", line);
    if let Ok(guard) = LOG_FILE.lock() {
        if let Some(path) = guard.as_ref() {
            let plain = if label.is_empty() {
                format!("{}\n", "─".repeat(width))
            } else {
                format!("── {} {}\n", label, "─".repeat(width.saturating_sub(label.len() + 4)))
            };
            let _ = std::fs::OpenOptions::new()
                .create(true).append(true).open(path)
                .and_then(|mut f| f.write_all(plain.as_bytes()));
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn level_badge(level: &str) -> &'static str {
    match level {
        "info"  => "INFO ",
        "warn"  => "WARN ",
        "error" => "ERROR",
        "debug" => "DEBUG",
        "ok"    => "OK   ",
        _       => "LOG  ",
    }
}

fn level_color(level: &str) -> &'static str {
    match level {
        "info"  => "\x1b[34m",    // azul
        "warn"  => "\x1b[33m",    // amarillo
        "error" => "\x1b[31m",    // rojo
        "debug" => "\x1b[36m",    // cyan
        "ok"    => "\x1b[32m",    // verde
        _       => "\x1b[37m",
    }
}

fn level_num(level: &str) -> u8 {
    match level { "debug" => 0, "info" | "ok" => 1, "warn" => 2, "error" => 3, _ => 1 }
}

fn full_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_s = secs.as_secs();
    let ms      = secs.subsec_millis();

    // Fecha simple desde epoch (sin chrono para no añadir dep)
    let days   = total_s / 86400;
    let time_s = total_s % 86400;
    let h = time_s / 3600;
    let m = (time_s % 3600) / 60;
    let s = time_s % 60;

    // Algoritmo de conversión epoch→fecha (calendario gregoriano)
    let (year, month, day) = days_to_date(days);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}", year, month, day, h, m, s, ms)
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Algoritmo de Richards (variante pública)
    let z = days + 719468;
    let era = z / 146097;
    let doe = z % 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y   = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = doy - (153 * mp + 2) / 5 + 1;
    let m   = if mp < 10 { mp + 3 } else { mp - 9 };
    let y   = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
