use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

struct VectorEntry {
    id:        String,
    embedding: Vec<f64>,
    metadata:  String,
}

struct VectorDb {
    entries: Vec<VectorEntry>,
}

static DBS: Mutex<Option<HashMap<String, VectorDb>>> = Mutex::new(None);
static COUNTER: AtomicU64 = AtomicU64::new(1);

fn with_dbs<F, T>(f: F) -> T
where
    F: FnOnce(&mut HashMap<String, VectorDb>) -> T,
{
    let mut guard = DBS.lock().unwrap();
    if guard.is_none() { *guard = Some(HashMap::new()); }
    f(guard.as_mut().unwrap())
}

fn new_handle() -> String {
    format!("vdb_{}", COUNTER.fetch_add(1, Ordering::SeqCst))
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // vector.new() → handle (string)
        "new" => {
            let handle = new_handle();
            with_dbs(|dbs| { dbs.insert(handle.clone(), VectorDb { entries: Vec::new() }); });
            Ok(EvalValue::Str(handle))
        }

        // vector.add(handle, id, embedding, metadata?) → int (total de entradas)
        "add" => {
            if args.len() < 3 { return Err("vector.add requiere (handle, id, embedding, metadata?)".into()); }
            let handle = to_str(&args[0]);
            let id     = to_str(&args[1]);
            let emb    = to_float_vec(&args[2])?;
            let meta   = if args.len() > 3 { format!("{}", &args[3]) } else { String::new() };
            let count  = with_dbs(|dbs| {
                let db = dbs.entry(handle.clone()).or_insert_with(|| VectorDb { entries: Vec::new() });
                db.entries.retain(|e| e.id != id);  // upsert: elimina duplicado si existe
                db.entries.push(VectorEntry { id, embedding: emb, metadata: meta });
                db.entries.len()
            });
            Ok(EvalValue::Int(count as i64))
        }

        // vector.buscar(handle, embedding, top?) → List<{id, score, metadata?}>
        "buscar" | "search" => {
            if args.len() < 2 { return Err("vector.buscar requiere (handle, embedding, top?)".into()); }
            let handle = to_str(&args[0]);
            let query  = to_float_vec(&args[1])?;
            let top    = if args.len() > 2 { to_usize(&args[2]).unwrap_or(5) } else { 5 };

            let results: Vec<(String, f64, String)> = with_dbs(|dbs| {
                let Some(db) = dbs.get(&handle) else { return Vec::new(); };
                let mut scored: Vec<(String, f64, String)> = db.entries.iter()
                    .map(|e| (e.id.clone(), cosine_similarity(&query, &e.embedding), e.metadata.clone()))
                    .collect();
                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                scored.truncate(top);
                scored
            });

            let out: Vec<EvalValue> = results.into_iter().map(|(id, score, meta)| {
                let mut m = HashMap::new();
                m.insert("id".into(),    EvalValue::Str(id));
                m.insert("score".into(), EvalValue::Float(round6(score)));
                if !meta.is_empty() { m.insert("metadata".into(), EvalValue::Str(meta)); }
                EvalValue::Dict(m)
            }).collect();
            Ok(EvalValue::List(out))
        }

        // vector.remove(handle, id) → bool
        "remove" | "eliminar" => {
            if args.len() < 2 { return Err("vector.remove requiere (handle, id)".into()); }
            let handle = to_str(&args[0]);
            let id     = to_str(&args[1]);
            let removed = with_dbs(|dbs| {
                let Some(db) = dbs.get_mut(&handle) else { return false; };
                let before = db.entries.len();
                db.entries.retain(|e| e.id != id);
                db.entries.len() < before
            });
            Ok(EvalValue::Bool(removed))
        }

        // vector.size(handle) → int
        "size" | "tamaño" => {
            if args.is_empty() { return Err("vector.size requiere (handle)".into()); }
            let handle = to_str(&args[0]);
            let n = with_dbs(|dbs| dbs.get(&handle).map(|db| db.entries.len()).unwrap_or(0));
            Ok(EvalValue::Int(n as i64))
        }

        // vector.clear(handle) → int (entradas eliminadas)
        "clear" | "limpiar" => {
            if args.is_empty() { return Err("vector.clear requiere (handle)".into()); }
            let handle = to_str(&args[0]);
            let n = with_dbs(|dbs| {
                let Some(db) = dbs.get_mut(&handle) else { return 0; };
                let n = db.entries.len();
                db.entries.clear();
                n
            });
            Ok(EvalValue::Int(n as i64))
        }

        // vector.ids(handle) → List<string>
        "ids" => {
            if args.is_empty() { return Err("vector.ids requiere (handle)".into()); }
            let handle = to_str(&args[0]);
            let ids = with_dbs(|dbs| {
                dbs.get(&handle)
                    .map(|db| db.entries.iter().map(|e| EvalValue::Str(e.id.clone())).collect::<Vec<_>>())
                    .unwrap_or_default()
            });
            Ok(EvalValue::List(ids))
        }

        // vector.save(handle, path) → string
        "save" | "guardar" => {
            if args.len() < 2 { return Err("vector.save requiere (handle, path)".into()); }
            let handle = to_str(&args[0]);
            let path   = to_str(&args[1]);
            let json   = with_dbs(|dbs| {
                let Some(db) = dbs.get(&handle) else { return "[]".to_string(); };
                let entries: Vec<serde_json::Value> = db.entries.iter().map(|e| serde_json::json!({
                    "id": e.id, "embedding": e.embedding, "metadata": e.metadata
                })).collect();
                serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".into())
            });
            std::fs::write(&path, json).map_err(|e| format!("vector.save: {}", e))?;
            Ok(EvalValue::Str(format!("[guardado: {}]", path)))
        }

        // vector.load(path) → handle
        "load" | "cargar" => {
            if args.is_empty() { return Err("vector.load requiere (path)".into()); }
            let path    = to_str(&args[0]);
            let content = std::fs::read_to_string(&path).map_err(|e| format!("vector.load: {}", e))?;
            let json: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| format!("vector.load: JSON inválido: {}", e))?;
            let arr = json.as_array().ok_or("vector.load: se esperaba un array JSON")?;
            let handle  = new_handle();
            let mut entries = Vec::new();
            for item in arr {
                let id   = item["id"].as_str().unwrap_or("").to_string();
                let emb  = item["embedding"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>())
                    .unwrap_or_default();
                let meta = item["metadata"].as_str().unwrap_or("").to_string();
                entries.push(VectorEntry { id, embedding: emb, metadata: meta });
            }
            with_dbs(|dbs| { dbs.insert(handle.clone(), VectorDb { entries }); });
            Ok(EvalValue::Str(handle))
        }

        f => Err(format!("vector.{}() no existe", f)),
    }
}

//   Matemática                                 

fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() { return 0.0; }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f64  = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb: f64  = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 { return 0.0; }
    (dot / (na * nb)).clamp(-1.0, 1.0)
}

//   Helpers                                  ─

fn to_float_vec(v: &EvalValue) -> Result<Vec<f64>, String> {
    match v {
        EvalValue::List(items) => items.iter().map(|x| match x {
            EvalValue::Float(f) => Ok(*f),
            EvalValue::Int(i)   => Ok(*i as f64),
            other => Err(format!("vector: embedding debe contener números, encontró {}", other.type_name())),
        }).collect(),
        _ => Err("vector: se esperaba un embedding (lista de números)".into()),
    }
}

fn to_usize(v: &EvalValue) -> Result<usize, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n as usize),
        EvalValue::Float(f) => Ok(*f as usize),
        _ => Err("vector: se esperaba un número entero".into()),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn round6(x: f64) -> f64 { (x * 1_000_000.0).round() / 1_000_000.0 }
