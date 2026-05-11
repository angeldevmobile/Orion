use crate::eval_value::EvalValue;
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

// Los valores se serializan a JSON para evitar restricciones de Send sobre EvalValue.
static QUEUES: OnceLock<Mutex<HashMap<String, VecDeque<serde_json::Value>>>> = OnceLock::new();

fn queues() -> &'static Mutex<HashMap<String, VecDeque<serde_json::Value>>> {
    QUEUES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // crear(nombre) → Bool
        "crear" | "create" => {
            let name = one_str("cola.crear", &args)?;
            queues().lock().unwrap().entry(name).or_insert_with(VecDeque::new);
            Ok(EvalValue::Bool(true))
        }
        // enviar(nombre, valor) → Bool  — agrega al final
        "enviar" | "push" => {
            if args.len() < 2 { return Err("cola.enviar requiere (nombre, valor)".into()); }
            let name = to_str(&args[0]);
            let val  = crate::modules::json_mod::eval_to_json(args[1].clone());
            queues().lock().unwrap()
                .entry(name).or_insert_with(VecDeque::new)
                .push_back(val);
            Ok(EvalValue::Bool(true))
        }
        // recibir(nombre) → valor o Null  — extrae del frente (FIFO)
        "recibir" | "pop" => {
            let name = one_str("cola.recibir", &args)?;
            Ok(queues().lock().unwrap()
                .get_mut(&name)
                .and_then(|q| q.pop_front())
                .map(crate::modules::json_mod::json_to_eval)
                .unwrap_or(EvalValue::Null))
        }
        // espiar(nombre) → ver el frente sin extraer
        "espiar" | "peek" => {
            let name = one_str("cola.espiar", &args)?;
            Ok(queues().lock().unwrap()
                .get(&name)
                .and_then(|q| q.front().cloned())
                .map(crate::modules::json_mod::json_to_eval)
                .unwrap_or(EvalValue::Null))
        }
        // tamaño(nombre) → Int
        "tamaño" | "size" | "len" => {
            let name = one_str("cola.tamaño", &args)?;
            Ok(EvalValue::Int(
                queues().lock().unwrap().get(&name).map(|q| q.len()).unwrap_or(0) as i64,
            ))
        }
        // vaciar(nombre) → Bool
        "vaciar" | "clear" => {
            let name = one_str("cola.vaciar", &args)?;
            if let Some(q) = queues().lock().unwrap().get_mut(&name) { q.clear(); }
            Ok(EvalValue::Bool(true))
        }
        // eliminar(nombre) → Bool  — elimina la cola entera
        "eliminar" | "delete" => {
            let name = one_str("cola.eliminar", &args)?;
            queues().lock().unwrap().remove(&name);
            Ok(EvalValue::Bool(true))
        }
        // lista() → List<Str> de nombres de colas existentes
        "lista" | "list" => {
            let names: Vec<EvalValue> = queues().lock().unwrap()
                .keys().map(|k| EvalValue::Str(k.clone())).collect();
            Ok(EvalValue::List(names))
        }
        f => Err(format!("cola.{}() no existe", f)),
    }
}

fn one_str(fn_name: &str, args: &[EvalValue]) -> Result<String, String> {
    if args.is_empty() { return Err(format!("{} requiere (nombre)", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
