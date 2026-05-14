use crate::eval_value::EvalValue;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::io::Write;

// 0=debug  1=info  2=warn  3=error
static LOG_LEVEL: AtomicU8 = AtomicU8::new(1);
static LOG_FILE:  Mutex<Option<String>> = Mutex::new(None);

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        "info" | "warn" | "error" | "debug" => {
            if args.is_empty() {
                return Err(format!("log.{} requiere (msg)", function));
            }
            write_log(function, &to_str(&args[0]));
            Ok(EvalValue::Null)
        }
        "level" => {
            if args.is_empty() {
                return Err("log.level requiere (\"debug\"|\"info\"|\"warn\"|\"error\")".into());
            }
            LOG_LEVEL.store(level_num(&to_str(&args[0])), Ordering::Relaxed);
            Ok(EvalValue::Null)
        }
        "to_file" | "file" => {
            let mut guard = LOG_FILE.lock().unwrap();
            if args.is_empty() {
                *guard = None;
            } else {
                *guard = Some(to_str(&args[0]));
            }
            Ok(EvalValue::Null)
        }
        "print" => {
            if args.len() < 2 {
                return Err("log.print requiere (level, msg)".into());
            }
            write_log(&to_str(&args[0]), &to_str(&args[1]));
            Ok(EvalValue::Null)
        }
        f => Err(format!("log.{}() no existe", f)),
    }
}

fn level_num(level: &str) -> u8 {
    match level { "debug" => 0, "info" => 1, "warn" => 2, "error" => 3, _ => 1 }
}

fn write_log(level: &str, msg: &str) {
    if level_num(level) < LOG_LEVEL.load(Ordering::Relaxed) { return; }

    let ts  = simple_time();
    let tag = match level {
        "info"  => "\x1b[32m[INFO]\x1b[0m",
        "warn"  => "\x1b[33m[WARN]\x1b[0m",
        "error" => "\x1b[31m[ERROR]\x1b[0m",
        "debug" => "\x1b[36m[DEBUG]\x1b[0m",
        _       => "[LOG]",
    };
    println!("{} {} {}", ts, tag, msg);

    if let Ok(guard) = LOG_FILE.lock() {
        if let Some(path) = guard.as_ref() {
            let line = format!("{} [{}] {}\n", ts, level.to_uppercase(), msg);
            let _ = std::fs::OpenOptions::new()
                .create(true).append(true).open(path)
                .and_then(|mut f| f.write_all(line.as_bytes()));
        }
    }
}

fn simple_time() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{:02}:{:02}:{:02}", (s % 86400) / 3600, (s % 3600) / 60, s % 60)
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
