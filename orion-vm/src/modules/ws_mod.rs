use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};
use tungstenite::{connect, Message, WebSocket, stream::MaybeTlsStream};
use std::net::TcpStream;

type WsConn = WebSocket<MaybeTlsStream<TcpStream>>;

static CONNS: OnceLock<Mutex<HashMap<u64, WsConn>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn conns() -> &'static Mutex<HashMap<u64, WsConn>> {
    CONNS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // conectar(url) → id Int
        "conectar" | "connect" => {
            let url = one_str("ws.conectar", &args)?;
            let (socket, _) = connect(&url)
                .map_err(|e| format!("ws.conectar '{}': {}", url, e))?;
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            conns().lock().unwrap().insert(id, socket);
            Ok(EvalValue::Int(id as i64))
        }
        // enviar(id, mensaje) → Bool
        "enviar" | "send" => {
            if args.len() < 2 { return Err("ws.enviar requiere (id, mensaje)".into()); }
            let id  = to_u64(&args[0])?;
            let msg = to_str(&args[1]);
            conns().lock().unwrap()
                .get_mut(&id)
                .ok_or_else(|| format!("ws: conexión {} no existe", id))?
                .write_message(Message::Text(msg))
                .map_err(|e| format!("ws.enviar: {}", e))?;
            Ok(EvalValue::Bool(true))
        }
        // recibir(id) → String o Null
        "recibir" | "recv" => {
            let id = to_u64(args.first().ok_or("ws.recibir requiere (id)")?)?;
            match conns().lock().unwrap()
                .get_mut(&id)
                .ok_or_else(|| format!("ws: conexión {} no existe", id))?
                .read_message()
                .map_err(|e| format!("ws.recibir: {}", e))?
            {
                Message::Text(t)   => Ok(EvalValue::Str(t)),
                Message::Binary(b) => Ok(EvalValue::Str(String::from_utf8_lossy(&b).into_owned())),
                _                  => Ok(EvalValue::Null),
            }
        }
        // cerrar(id) → Bool
        "cerrar" | "close" => {
            let id = to_u64(args.first().ok_or("ws.cerrar requiere (id)")?)?;
            if let Some(mut conn) = conns().lock().unwrap().remove(&id) {
                let _ = conn.close(None);
            }
            Ok(EvalValue::Bool(true))
        }
        // conexiones() → List<Int> de ids activos
        "conexiones" | "connections" => {
            let ids: Vec<EvalValue> = conns().lock().unwrap()
                .keys().map(|k| EvalValue::Int(*k as i64)).collect();
            Ok(EvalValue::List(ids))
        }
        f => Err(format!("ws.{}() no existe", f)),
    }
}

fn one_str(fn_name: &str, args: &[EvalValue]) -> Result<String, String> {
    if args.is_empty() { return Err(format!("{} requiere argumento", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_u64(v: &EvalValue) -> Result<u64, String> {
    match v {
        EvalValue::Int(n) if *n > 0 => Ok(*n as u64),
        other => Err(format!("ws: esperaba id positivo, recibió {}", other.type_name())),
    }
}
