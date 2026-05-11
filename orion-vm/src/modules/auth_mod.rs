use crate::eval_value::EvalValue;
use std::collections::HashMap;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::{SaltString, rand_core::OsRng};
use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // hash(password) → String  — Argon2id con salt aleatorio
        "hash" => {
            let pass = one_str("auth.hash", &args)?;
            let salt = SaltString::generate(&mut OsRng);
            let hash = Argon2::default()
                .hash_password(pass.as_bytes(), &salt)
                .map_err(|e| format!("auth.hash: {}", e))?
                .to_string();
            Ok(EvalValue::Str(hash))
        }
        // verificar(password, hash) → Bool
        "verificar" | "verify" => {
            if args.len() < 2 { return Err("auth.verificar requiere (password, hash)".into()); }
            let pass   = to_str(&args[0]);
            let hash   = to_str(&args[1]);
            let parsed = PasswordHash::new(&hash)
                .map_err(|e| format!("auth.verificar: hash inválido: {}", e))?;
            Ok(EvalValue::Bool(
                Argon2::default().verify_password(pass.as_bytes(), &parsed).is_ok()
            ))
        }
        // token(payload_dict, secret, exp_secs?) → String JWT (HS256)
        "token" => {
            if args.len() < 2 { return Err("auth.token requiere (payload, secret, exp_secs?)".into()); }
            let mut claims = match crate::modules::json_mod::eval_to_json(args[0].clone()) {
                serde_json::Value::Object(m) => m,
                _ => return Err("auth.token: payload debe ser un Dict".into()),
            };
            let secret = to_str(&args[1]);
            if let Some(exp_arg) = args.get(2) {
                let secs = to_i64(exp_arg)?;
                let exp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64 + secs;
                claims.insert("exp".into(), serde_json::json!(exp));
            }
            let token = encode(
                &Header::default(),
                &serde_json::Value::Object(claims),
                &EncodingKey::from_secret(secret.as_bytes()),
            ).map_err(|e| format!("auth.token: {}", e))?;
            Ok(EvalValue::Str(token))
        }
        // verificar_token(token, secret) → Dict con payload o {error, valido:false}
        "verificar_token" | "decode_token" => {
            if args.len() < 2 { return Err("auth.verificar_token requiere (token, secret)".into()); }
            let token  = to_str(&args[0]);
            let secret = to_str(&args[1]);
            let mut validation = Validation::new(Algorithm::HS256);
            // No requerir 'exp' para tokens sin fecha de expiración
            validation.required_spec_claims = std::collections::HashSet::new();
            match decode::<serde_json::Value>(
                &token,
                &DecodingKey::from_secret(secret.as_bytes()),
                &validation,
            ) {
                Ok(data)  => Ok(crate::modules::json_mod::json_to_eval(data.claims)),
                Err(e) => {
                    let mut m = HashMap::new();
                    m.insert("error".into(),  EvalValue::Str(format!("{}", e)));
                    m.insert("valido".into(), EvalValue::Bool(false));
                    Ok(EvalValue::Dict(m))
                }
            }
        }
        f => Err(format!("auth.{}() no existe", f)),
    }
}

fn one_str(fn_name: &str, args: &[EvalValue]) -> Result<String, String> {
    if args.is_empty() { return Err(format!("{} requiere argumento", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("auth: esperaba número, recibió {}", other.type_name())),
    }
}
