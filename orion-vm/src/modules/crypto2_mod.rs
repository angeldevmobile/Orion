use crate::eval_value::EvalValue;
use std::collections::HashMap;

// AES-256-GCM
use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, KeyInit}};
// RSA (API de bajo nivel — evita conflictos de trait bounds con signature crate)
use rsa::{RsaPrivateKey, RsaPublicKey, Oaep};
use rsa::pkcs1v15::Pkcs1v15Sign;
use rsa::pkcs8::{EncodePrivateKey, DecodePrivateKey, EncodePublicKey, DecodePublicKey, LineEnding};
// SHA-256
use sha2::{Sha256, Digest as _};
// Base64
use base64::{engine::general_purpose::STANDARD as B64, Engine};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // ── AES-256-GCM ─────────────────────────────────────────────────────────
        "aes_encrypt" => {
            if args.len() < 2 { return Err("crypto2.aes_encrypt requiere (plaintext, password)".into()); }
            aes_encrypt(&to_str(&args[0]), &to_str(&args[1]))
        }
        "aes_decrypt" => {
            if args.len() < 2 { return Err("crypto2.aes_decrypt requiere (ciphertext_b64, password)".into()); }
            aes_decrypt(&to_str(&args[0]), &to_str(&args[1]))
        }
        // ── RSA ──────────────────────────────────────────────────────────────────
        "rsa_keygen" => {
            let bits = if args.is_empty() { 2048 } else { args[0].to_i64()? as usize };
            rsa_keygen(bits)
        }
        "rsa_encrypt" => {
            if args.len() < 2 { return Err("crypto2.rsa_encrypt requiere (plaintext, public_key_pem)".into()); }
            rsa_encrypt(&to_str(&args[0]), &to_str(&args[1]))
        }
        "rsa_decrypt" => {
            if args.len() < 2 { return Err("crypto2.rsa_decrypt requiere (ciphertext_b64, private_key_pem)".into()); }
            rsa_decrypt(&to_str(&args[0]), &to_str(&args[1]))
        }
        "rsa_sign" => {
            if args.len() < 2 { return Err("crypto2.rsa_sign requiere (data, private_key_pem)".into()); }
            rsa_sign(&to_str(&args[0]), &to_str(&args[1]))
        }
        "rsa_verify" => {
            if args.len() < 3 { return Err("crypto2.rsa_verify requiere (data, signature_b64, public_key_pem)".into()); }
            rsa_verify(&to_str(&args[0]), &to_str(&args[1]), &to_str(&args[2]))
        }
        f => Err(format!("crypto2.{}() no existe", f)),
    }
}

// ── AES-256-GCM ──────────────────────────────────────────────────────────────

fn derive_aes_key(password: &str) -> Key<Aes256Gcm> {
    let hash = Sha256::digest(password.as_bytes());
    *Key::<Aes256Gcm>::from_slice(&hash)
}

fn aes_encrypt(plaintext: &str, password: &str) -> Result<EvalValue, String> {
    let key    = derive_aes_key(password);
    let cipher = Aes256Gcm::new(&key);

    let mut nonce_bytes = [0u8; 12];
    rand_fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("crypto2.aes_encrypt: {}", e))?;

    // Formato final: base64(nonce‖ciphertext)
    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(EvalValue::Str(B64.encode(&combined)))
}

fn aes_decrypt(encoded: &str, password: &str) -> Result<EvalValue, String> {
    let raw = B64.decode(encoded)
        .map_err(|e| format!("crypto2.aes_decrypt base64: {}", e))?;
    if raw.len() < 12 {
        return Err("crypto2.aes_decrypt: datos corruptos (demasiado cortos)".into());
    }
    let nonce      = Nonce::from_slice(&raw[..12]);
    let ciphertext = &raw[12..];
    let key        = derive_aes_key(password);
    let cipher     = Aes256Gcm::new(&key);
    let plain      = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "crypto2.aes_decrypt: clave incorrecta o datos corruptos".to_string())?;
    String::from_utf8(plain)
        .map(EvalValue::Str)
        .map_err(|e| format!("crypto2.aes_decrypt UTF-8: {}", e))
}

// ── RSA ──────────────────────────────────────────────────────────────────────

fn rsa_keygen(bits: usize) -> Result<EvalValue, String> {
    let mut rng  = rand::thread_rng();
    let priv_key = RsaPrivateKey::new(&mut rng, bits)
        .map_err(|e| format!("crypto2.rsa_keygen: {}", e))?;
    let pub_key  = RsaPublicKey::from(&priv_key);

    let priv_pem = priv_key.to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| format!("crypto2.rsa_keygen (priv PEM): {}", e))?
        .to_string();
    let pub_pem = pub_key.to_public_key_pem(LineEnding::LF)
        .map_err(|e| format!("crypto2.rsa_keygen (pub PEM): {}", e))?;

    let mut map = HashMap::new();
    map.insert("private_key".into(), EvalValue::Str(priv_pem));
    map.insert("public_key".into(),  EvalValue::Str(pub_pem));
    Ok(EvalValue::Dict(map))
}

fn rsa_encrypt(plaintext: &str, pub_pem: &str) -> Result<EvalValue, String> {
    let pub_key = RsaPublicKey::from_public_key_pem(pub_pem)
        .map_err(|e| format!("crypto2.rsa_encrypt (clave): {}", e))?;
    let mut rng = rand::thread_rng();
    let cipher  = pub_key
        .encrypt(&mut rng, Oaep::new::<Sha256>(), plaintext.as_bytes())
        .map_err(|e| format!("crypto2.rsa_encrypt: {}", e))?;
    Ok(EvalValue::Str(B64.encode(&cipher)))
}

fn rsa_decrypt(encoded: &str, priv_pem: &str) -> Result<EvalValue, String> {
    let priv_key = RsaPrivateKey::from_pkcs8_pem(priv_pem)
        .map_err(|e| format!("crypto2.rsa_decrypt (clave): {}", e))?;
    let cipher   = B64.decode(encoded)
        .map_err(|e| format!("crypto2.rsa_decrypt base64: {}", e))?;
    let plain    = priv_key
        .decrypt(Oaep::new::<Sha256>(), &cipher)
        .map_err(|e| format!("crypto2.rsa_decrypt: {}", e))?;
    String::from_utf8(plain)
        .map(EvalValue::Str)
        .map_err(|e| format!("crypto2.rsa_decrypt UTF-8: {}", e))
}

// Firma PKCS#1 v1.5 con SHA-256 — usa API de bajo nivel de rsa directamente
// para evitar conflictos de trait bounds del signature crate
fn rsa_sign(data: &str, priv_pem: &str) -> Result<EvalValue, String> {
    let priv_key  = RsaPrivateKey::from_pkcs8_pem(priv_pem)
        .map_err(|e| format!("crypto2.rsa_sign (clave): {}", e))?;
    let hash      = Sha256::digest(data.as_bytes());
    let mut rng   = rand::thread_rng();
    let signature = priv_key
        .sign_with_rng(&mut rng, Pkcs1v15Sign::new::<Sha256>(), &hash)
        .map_err(|e| format!("crypto2.rsa_sign: {}", e))?;
    Ok(EvalValue::Str(B64.encode(&signature)))
}

fn rsa_verify(data: &str, sig_b64: &str, pub_pem: &str) -> Result<EvalValue, String> {
    let pub_key = RsaPublicKey::from_public_key_pem(pub_pem)
        .map_err(|e| format!("crypto2.rsa_verify (clave): {}", e))?;
    let sig     = B64.decode(sig_b64)
        .map_err(|e| format!("crypto2.rsa_verify base64: {}", e))?;
    let hash    = Sha256::digest(data.as_bytes());
    let ok      = pub_key
        .verify(Pkcs1v15Sign::new::<Sha256>(), &hash, &sig)
        .is_ok();
    Ok(EvalValue::Bool(ok))
}

// ── Utilidad ─────────────────────────────────────────────────────────────────

fn rand_fill(buf: &mut [u8]) {
    use rand::RngCore;
    rand::thread_rng().fill_bytes(buf);
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
