use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;

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
            if args.len() < 2 { return Err("crypto2.aes_encrypt requires (plaintext, password)".into()); }
            aes_encrypt(&to_str(&args[0]), &to_str(&args[1]))
        }
        "aes_decrypt" => {
            if args.len() < 2 { return Err("crypto2.aes_decrypt requires (ciphertext_b64, password)".into()); }
            aes_decrypt(&to_str(&args[0]), &to_str(&args[1]))
        }
        // ── RSA ──────────────────────────────────────────────────────────────────
        "rsa_keygen" => {
            let bits = if args.is_empty() { 2048 } else { args[0].to_i64()? as usize };
            rsa_keygen(bits)
        }
        "rsa_encrypt" => {
            if args.len() < 2 { return Err("crypto2.rsa_encrypt requires (plaintext, public_key_pem)".into()); }
            rsa_encrypt(&to_str(&args[0]), &to_str(&args[1]))
        }
        "rsa_decrypt" => {
            if args.len() < 2 { return Err("crypto2.rsa_decrypt requires (ciphertext_b64, private_key_pem)".into()); }
            rsa_decrypt(&to_str(&args[0]), &to_str(&args[1]))
        }
        "rsa_sign" => {
            if args.len() < 2 { return Err("crypto2.rsa_sign requires (data, private_key_pem)".into()); }
            rsa_sign(&to_str(&args[0]), &to_str(&args[1]))
        }
        "rsa_verify" => {
            if args.len() < 3 { return Err("crypto2.rsa_verify requires (data, signature_b64, public_key_pem)".into()); }
            rsa_verify(&to_str(&args[0]), &to_str(&args[1]), &to_str(&args[2]))
        }
        f => Err(format!("crypto2.{}() does not exist", f)),
    }
}

// ── AES-256-GCM ──────────────────────────────────────────────────────────────
//
// La clave AES se deriva del password con Argon2id + salt aleatorio de 16 bytes
// (memory-hard: resiste fuerza bruta en GPU). Un SHA-256 plano del password —el
// esquema anterior— es rapidísimo de romper y sin salt permite rainbow tables
// compartidas entre todos los usuarios. Formato versionado:
//   v1 (actual):  base64( 0x01 ‖ salt[16] ‖ nonce[12] ‖ ciphertext )
//   legacy:       base64(                    nonce[12] ‖ ciphertext )  (SHA-256)
// Al descifrar se intenta v1 y, si el tag GCM no valida, se cae a legacy: el tag
// autenticado desambigua sin riesgo de descifrar basura.

const AES_V1: u8 = 0x01;

/// Argon2id(password, salt) → 32 bytes de clave AES-256.
fn kdf_argon2(password: &str, salt: &[u8]) -> Result<Key<Aes256Gcm>, String> {
    let mut key = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| format!("crypto2: key derivation: {}", e))?;
    Ok(*Key::<Aes256Gcm>::from_slice(&key))
}

/// KDF legacy (SHA-256 plano) — SOLO para descifrar datos del formato viejo.
fn kdf_legacy(password: &str) -> Key<Aes256Gcm> {
    let hash = Sha256::digest(password.as_bytes());
    *Key::<Aes256Gcm>::from_slice(&hash)
}

fn aes_encrypt(plaintext: &str, password: &str) -> Result<EvalValue, String> {
    let mut salt = [0u8; 16];
    rand_fill(&mut salt);
    let key    = kdf_argon2(password, &salt)?;
    let cipher = Aes256Gcm::new(&key);

    let mut nonce_bytes = [0u8; 12];
    rand_fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("crypto2.aes_encrypt: {}", e))?;

    // Formato v1: base64(0x01 ‖ salt ‖ nonce ‖ ciphertext)
    let mut combined = Vec::with_capacity(1 + 16 + 12 + ciphertext.len());
    combined.push(AES_V1);
    combined.extend_from_slice(&salt);
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    Ok(EvalValue::Str(B64.encode(&combined)))
}

fn aes_decrypt(encoded: &str, password: &str) -> Result<EvalValue, String> {
    let raw = B64.decode(encoded)
        .map_err(|e| format!("crypto2.aes_decrypt base64: {}", e))?;

    // Intento formato v1 (Argon2id + salt).
    if raw.first() == Some(&AES_V1) && raw.len() >= 1 + 16 + 12 {
        let salt  = &raw[1..17];
        let nonce = Nonce::from_slice(&raw[17..29]);
        let ct    = &raw[29..];
        let key   = kdf_argon2(password, salt)?;
        if let Ok(plain) = Aes256Gcm::new(&key).decrypt(nonce, ct) {
            return String::from_utf8(plain)
                .map(EvalValue::Str)
                .map_err(|e| format!("crypto2.aes_decrypt UTF-8: {}", e));
        }
        // Si el tag no valida, puede ser un dato legacy que empieza por 0x01:
        // caemos al intento legacy antes de rendirnos.
    }

    // Formato legacy (SHA-256, sin salt): nonce[12] ‖ ct.
    if raw.len() >= 12 {
        let nonce = Nonce::from_slice(&raw[..12]);
        let ct    = &raw[12..];
        let key   = kdf_legacy(password);
        if let Ok(plain) = Aes256Gcm::new(&key).decrypt(nonce, ct) {
            return String::from_utf8(plain)
                .map(EvalValue::Str)
                .map_err(|e| format!("crypto2.aes_decrypt UTF-8: {}", e));
        }
    }

    Err("crypto2.aes_decrypt: wrong key, or corrupt data".into())
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
        .map_err(|e| format!("crypto2.rsa_encrypt (key): {}", e))?;
    let mut rng = rand::thread_rng();
    let cipher  = pub_key
        .encrypt(&mut rng, Oaep::new::<Sha256>(), plaintext.as_bytes())
        .map_err(|e| format!("crypto2.rsa_encrypt: {}", e))?;
    Ok(EvalValue::Str(B64.encode(&cipher)))
}

fn rsa_decrypt(encoded: &str, priv_pem: &str) -> Result<EvalValue, String> {
    let priv_key = RsaPrivateKey::from_pkcs8_pem(priv_pem)
        .map_err(|e| format!("crypto2.rsa_decrypt (key): {}", e))?;
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
        .map_err(|e| format!("crypto2.rsa_sign (key): {}", e))?;
    let hash      = Sha256::digest(data.as_bytes());
    let mut rng   = rand::thread_rng();
    let signature = priv_key
        .sign_with_rng(&mut rng, Pkcs1v15Sign::new::<Sha256>(), &hash)
        .map_err(|e| format!("crypto2.rsa_sign: {}", e))?;
    Ok(EvalValue::Str(B64.encode(&signature)))
}

fn rsa_verify(data: &str, sig_b64: &str, pub_pem: &str) -> Result<EvalValue, String> {
    let pub_key = RsaPublicKey::from_public_key_pem(pub_pem)
        .map_err(|e| format!("crypto2.rsa_verify (key): {}", e))?;
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
