use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static TIMERS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

fn timers() -> &'static Mutex<HashMap<String, Instant>> {
    TIMERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // dormir(ms) → Null  — pausa la ejecución
        "dormir" | "sleep" => {
            let ms = to_i64(args.first().ok_or("tarea.dormir requiere (ms)")?)? as u64;
            std::thread::sleep(Duration::from_millis(ms));
            Ok(EvalValue::Null)
        }
        // ahora() → timestamp Unix en ms
        "ahora" | "now" => {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| format!("tarea.ahora: {}", e))?
                .as_millis() as i64;
            Ok(EvalValue::Int(ms))
        }
        // iniciar(nombre) → Null  — arranca un cronómetro
        "iniciar" | "start" => {
            let name = one_str("tarea.iniciar", &args)?;
            timers().lock().unwrap().insert(name, Instant::now());
            Ok(EvalValue::Null)
        }
        // medir(nombre) → ms transcurridos desde iniciar()
        "medir" | "elapsed" => {
            let name = one_str("tarea.medir", &args)?;
            let ms = timers().lock().unwrap()
                .get(&name)
                .map(|t| t.elapsed().as_millis() as i64)
                .ok_or_else(|| format!("tarea.medir: timer '{}' no iniciado", name))?;
            Ok(EvalValue::Int(ms))
        }
        // reiniciar(nombre) → Null
        "reiniciar" | "reset" => {
            let name = one_str("tarea.reiniciar", &args)?;
            timers().lock().unwrap().insert(name, Instant::now());
            Ok(EvalValue::Null)
        }
        // repetir(n, intervalo_ms) → Null  — espera n veces con intervalo (bloqueante)
        "repetir" | "repeat" => {
            if args.len() < 2 { return Err("tarea.repetir requiere (n, intervalo_ms)".into()); }
            let n        = to_i64(&args[0])? as u64;
            let interval = to_i64(&args[1])? as u64;
            for _ in 0..n {
                std::thread::sleep(Duration::from_millis(interval));
            }
            Ok(EvalValue::Null)
        }
        f => Err(format!("tarea.{}() no existe", f)),
    }
}

fn one_str(fn_name: &str, args: &[EvalValue]) -> Result<String, String> {
    if args.is_empty() { return Err(format!("{} requiere (nombre)", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("tarea: esperaba número, recibió {}", other.type_name())),
    }
}
