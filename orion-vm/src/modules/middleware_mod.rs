use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct RateLimiter {
    max_req:     u64,
    window_secs: u64,
    clients:     HashMap<String, (u64, u64)>, // ip → (count, window_start_secs)
}

static LIMITERS: OnceLock<Mutex<HashMap<u64, RateLimiter>>> = OnceLock::new();
static NEXT_ID:  AtomicU64 = AtomicU64::new(1);

fn limiters() -> &'static Mutex<HashMap<u64, RateLimiter>> {
    LIMITERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // rate_limit(max_req, window_secs) → Int (limiter ID)
        "rate_limit" => {
            if args.len() < 2 { return Err("middleware.rate_limit requiere (max_req, window_secs)".into()); }
            let max_req     = args[0].to_i64()? as u64;
            let window_secs = args[1].to_i64()? as u64;
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            limiters().lock().unwrap().insert(id, RateLimiter {
                max_req,
                window_secs,
                clients: HashMap::new(),
            });
            Ok(EvalValue::Int(id as i64))
        }

        // check_rate(limiter_id, client_ip) → Bool (true = permitido)
        "check_rate" => {
            if args.len() < 2 { return Err("middleware.check_rate requiere (limiter_id, client_ip)".into()); }
            let id  = to_u64(&args[0])?;
            let ip  = to_str(&args[1]);
            let now = now_secs();
            let mut store = limiters().lock().unwrap();
            let lim = store.get_mut(&id)
                .ok_or_else(|| format!("middleware: limiter {} no existe", id))?;
            let entry = lim.clients.entry(ip).or_insert((0, now));
            if now - entry.1 >= lim.window_secs {
                *entry = (1, now);
                Ok(EvalValue::Bool(true))
            } else if entry.0 < lim.max_req {
                entry.0 += 1;
                Ok(EvalValue::Bool(true))
            } else {
                Ok(EvalValue::Bool(false))
            }
        }

        // reset_rate(limiter_id, client_ip?) → Bool  (resetea contadores)
        "reset_rate" => {
            if args.is_empty() { return Err("middleware.reset_rate requiere (limiter_id, ip?)".into()); }
            let id = to_u64(&args[0])?;
            let mut store = limiters().lock().unwrap();
            let lim = store.get_mut(&id)
                .ok_or_else(|| format!("middleware: limiter {} no existe", id))?;
            if args.len() > 1 {
                lim.clients.remove(&to_str(&args[1]));
            } else {
                lim.clients.clear();
            }
            Ok(EvalValue::Bool(true))
        }

        // cors(origins?, methods?, headers?) → Dict de headers HTTP
        "cors" => {
            let origins = args.first().map(to_str).unwrap_or_else(|| "*".into());
            let methods = args.get(1).map(to_str)
                .unwrap_or_else(|| "GET, POST, PUT, DELETE, PATCH, OPTIONS".into());
            let headers = args.get(2).map(to_str)
                .unwrap_or_else(|| "Content-Type, Authorization".into());
            let mut d = HashMap::new();
            d.insert("Access-Control-Allow-Origin".into(),   EvalValue::Str(origins));
            d.insert("Access-Control-Allow-Methods".into(),  EvalValue::Str(methods));
            d.insert("Access-Control-Allow-Headers".into(),  EvalValue::Str(headers));
            d.insert("Access-Control-Max-Age".into(),        EvalValue::Str("86400".into()));
            d.insert("Vary".into(),                          EvalValue::Str("Origin".into()));
            Ok(EvalValue::Dict(d))
        }

        // auth_bearer(token, secret) → Dict {valid, sub?, payload?, error?}
        "auth_bearer" => {
            if args.len() < 2 { return Err("middleware.auth_bearer requiere (token, secret)".into()); }
            validate_jwt(&to_str(&args[0]), &to_str(&args[1]))
        }

        // log_req(method, path, status, ms) → Bool
        "log_req" => {
            if args.len() < 4 { return Err("middleware.log_req requiere (method, path, status, ms)".into()); }
            let method = to_str(&args[0]);
            let path   = to_str(&args[1]);
            let status = args[2].to_i64().unwrap_or(0);
            let ms     = args[3].to_i64().unwrap_or(0);
            let color  = if status < 300 { "\x1b[32m" }
                         else if status < 400 { "\x1b[33m" }
                         else { "\x1b[31m" };
            println!("  \x1b[2m{}\x1b[0m  {}{:<7}\x1b[0m  \x1b[97m{}\x1b[0m  {}{}  \x1b[2m{}ms\x1b[0m",
                now_str(), color, method, path, color, status, ms);
            Ok(EvalValue::Bool(true))
        }

        // drop_rate(limiter_id) → Bool
        "drop_rate" => {
            if args.is_empty() { return Err("middleware.drop_rate requiere (limiter_id)".into()); }
            limiters().lock().unwrap().remove(&to_u64(&args[0])?);
            Ok(EvalValue::Bool(true))
        }

        f => Err(format!("middleware.{}() no existe", f)),
    }
}

//    JWT validation                                                             

fn validate_jwt(token: &str, secret: &str) -> Result<EvalValue, String> {
    use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};

    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = false;

    match decode::<serde_json::Map<String, serde_json::Value>>(token, &key, &validation) {
        Ok(data) => {
            let mut result: HashMap<String, EvalValue> = HashMap::new();

            // Verificar expiración manualmente
            if let Some(exp) = data.claims.get("exp").and_then(|v| v.as_i64()) {
                if exp < now_secs() as i64 {
                    result.insert("valid".into(), EvalValue::Bool(false));
                    result.insert("error".into(), EvalValue::Str("Token expirado".into()));
                    return Ok(EvalValue::Dict(result));
                }
            }

            result.insert("valid".into(), EvalValue::Bool(true));
            if let Some(sub) = data.claims.get("sub").and_then(|v| v.as_str()) {
                result.insert("sub".into(), EvalValue::Str(sub.into()));
            }
            let payload: HashMap<String, EvalValue> = data.claims.into_iter()
                .map(|(k, v)| (k, json_to_eval(v)))
                .collect();
            result.insert("payload".into(), EvalValue::Dict(payload));
            Ok(EvalValue::Dict(result))
        }
        Err(e) => {
            let mut d: HashMap<String, EvalValue> = HashMap::new();
            d.insert("valid".into(), EvalValue::Bool(false));
            d.insert("error".into(), EvalValue::Str(format!("{}", e)));
            Ok(EvalValue::Dict(d))
        }
    }
}

//    Helpers                                                                    

fn json_to_eval(v: serde_json::Value) -> EvalValue {
    match v {
        serde_json::Value::Null      => EvalValue::Null,
        serde_json::Value::Bool(b)   => EvalValue::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { EvalValue::Int(i) }
            else { n.as_f64().map(EvalValue::Float).unwrap_or(EvalValue::Null) }
        }
        serde_json::Value::String(s) => EvalValue::Str(s),
        serde_json::Value::Array(a)  => EvalValue::List(a.into_iter().map(json_to_eval).collect()),
        serde_json::Value::Object(o) => EvalValue::Dict(
            o.into_iter().map(|(k, v)| (k, json_to_eval(v))).collect()
        ),
    }
}

fn now_str() -> String {
    let s = now_secs();
    format!("{:02}:{:02}:{:02}", (s % 86400) / 3600, (s % 3600) / 60, s % 60)
}

fn to_u64(v: &EvalValue) -> Result<u64, String> {
    match v {
        EvalValue::Int(n) if *n > 0 => Ok(*n as u64),
        _ => Err("middleware: ID debe ser un Int positivo".into()),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
