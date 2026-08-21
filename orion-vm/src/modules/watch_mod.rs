use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

// Mapa path → última mtime conocida (segundos Unix).
static WATCHED: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

fn watched() -> &'static Mutex<HashMap<String, u64>> {
    WATCHED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn mtime(path: &str) -> u64 {
    std::fs::metadata(path).ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // watch(path) → Bool  — registra el estado actual del archivo
        "watch" | "observar" => {
            let path = one_str("watch.observar", &args)?;
            watched().lock().unwrap().insert(path.clone(), mtime(&path));
            Ok(EvalValue::Bool(true))
        }
        // changed(path) → Bool  — true si cambió desde la última observación
        "changed" | "modificado" => {
            let path    = one_str("watch.modificado", &args)?;
            let current = mtime(&path);
            let mut map = watched().lock().unwrap();
            let prev    = *map.get(&path).unwrap_or(&0);
            if current != prev {
                map.insert(path, current);
                Ok(EvalValue::Bool(true))
            } else {
                Ok(EvalValue::Bool(false))
            }
        }
        // stat(path) → Dict {existe, tamaño, modificado_unix, es_directorio}
        "stat" | "estado" => {
            let path = one_str("watch.estado", &args)?;
            let mut m = HashMap::new();
            match std::fs::metadata(&path) {
                Ok(meta) => {
                    let ts = meta.modified().ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    m.insert("existe".into(),          EvalValue::Bool(true));
                    m.insert("tamaño".into(),          EvalValue::Int(meta.len() as i64));
                    m.insert("modificado_unix".into(), EvalValue::Int(ts));
                    m.insert("es_directorio".into(),   EvalValue::Bool(meta.is_dir()));
                }
                Err(_) => { m.insert("existe".into(), EvalValue::Bool(false)); }
            }
            Ok(EvalValue::Dict(m))
        }
        // unwatch(path) → Bool  — deja de rastrear el archivo
        "unwatch" | "dejar" => {
            let path = one_str("watch.dejar", &args)?;
            watched().lock().unwrap().shift_remove(&path);
            Ok(EvalValue::Bool(true))
        }
        // list() → List<Str> de paths observados
        "list" | "lista" => {
            let paths: Vec<EvalValue> = watched().lock().unwrap()
                .keys().map(|k| EvalValue::Str(k.clone())).collect();
            Ok(EvalValue::List(paths))
        }
        f => Err(format!("watch.{}() no existe", f)),
    }
}

fn one_str(fn_name: &str, args: &[EvalValue]) -> Result<String, String> {
    if args.is_empty() { return Err(format!("{} requiere (path)", fn_name)); }
    Ok(match &args[0] { EvalValue::Str(s) => s.clone(), other => format!("{}", other) })
}
