use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Digest};
use rand::Rng;
use subtle::ConstantTimeEq;
use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, KeyInit}};

type HmacSha256 = Hmac<Sha256>;

/// Compara dos secretos en TIEMPO CONSTANTE. Un `==` normal corta en el primer
/// byte distinto y filtra, por diferencia de tiempo, cuántos bytes acertó el
/// atacante — suficiente para reconstruir una firma byte a byte.
fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() { return false; }
    a.ct_eq(b).into()
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // hash(data, algo?) → "sha256$salt$hexhash"
        "hash" => {
            if args.is_empty() { return Err("crypto.hash requires (data, algo?)".into()); }
            let data = to_str(&args[0]);
            let algo = if args.len() > 1 { to_str(&args[1]) } else { "sha256".into() };
            let salt = random_hex(8);
            let result = do_hash(&data, &salt, &algo);
            Ok(EvalValue::Str(format!("{}${}${}", algo, salt, result)))
        }
        // verify_hash(data, hashed) → bool
        "verify_hash" => {
            if args.len() < 2 { return Err("crypto.verify_hash requires (data, hashed)".into()); }
            let data   = to_str(&args[0]);
            let hashed = to_str(&args[1]);
            let parts: Vec<&str> = hashed.splitn(3, '$').collect();
            if parts.len() != 3 { return Ok(EvalValue::Bool(false)); }
            let algo = parts[0];
            let salt = parts[1];
            let hash = parts[2];
            let expected = do_hash(&data, salt, algo);
            Ok(EvalValue::Bool(ct_eq(&expected, hash)))
        }
        // sha256(data) → hex string
        "sha256" => {
            let data = one_str("sha256", args)?;
            let mut hasher = Sha256::new();
            hasher.update(data.as_bytes());
            Ok(EvalValue::Str(hex_encode(&hasher.finalize())))
        }
        // md5(data) → hex string (MD5 real, para checksums — NO para seguridad)
        "md5" => {
            let data = one_str("md5", args)?;
            use md5::Digest as _;
            let mut h = md5::Md5::new();
            h.update(data.as_bytes());
            Ok(EvalValue::Str(hex_encode(&h.finalize())))
        }
        // sign(data, key) → hex signature (HMAC-SHA256)
        "sign" => {
            if args.len() < 2 { return Err("crypto.sign requires (data, key)".into()); }
            let data = to_str(&args[0]);
            let key  = to_str(&args[1]);
            let sig  = hmac_sign(data.as_bytes(), key.as_bytes())?;
            Ok(EvalValue::Str(sig))
        }
        // verify(data, signature, key) → bool
        "verify" => {
            if args.len() < 3 { return Err("crypto.verify requires (data, signature, key)".into()); }
            let data = to_str(&args[0]);
            let sig  = to_str(&args[1]);
            let key  = to_str(&args[2]);
            let expected = hmac_sign(data.as_bytes(), key.as_bytes())?;
            Ok(EvalValue::Bool(ct_eq(&expected, &sig)))
        }
        // encrypt(data, key?) → {cipher, key, mode}  — AES-256-GCM (autenticado)
        "encrypt" => {
            if args.is_empty() { return Err("crypto.encrypt requires (data, key?)".into()); }
            let data = to_str(&args[0]);
            let key  = if args.len() > 1 { to_str(&args[1]) } else { random_hex(16) };
            let cipher_b64 = aes_encrypt(data.as_bytes(), &key)?;
            let mut m = HashMap::new();
            m.insert("cipher".into(), EvalValue::Str(cipher_b64));
            m.insert("key".into(),    EvalValue::Str(key));
            m.insert("mode".into(),   EvalValue::Str("aes-256-gcm".into()));
            Ok(EvalValue::Dict(m))
        }
        // decrypt(cipher, key, mode?) → string
        // mode "xor" descifra datos del formato antiguo (inseguro, solo migración).
        "decrypt" => {
            if args.len() < 2 { return Err("crypto.decrypt requires (cipher, key)".into()); }
            let cipher_b64 = to_str(&args[0]);
            let key        = to_str(&args[1]);
            let mode = args.get(2).map(to_str).unwrap_or_else(|| "aes-256-gcm".into());

            if mode == "xor" {
                let cipher = b64_decode(&cipher_b64).map_err(|e| format!("crypto.decrypt: {}", e))?;
                let (plain, _) = xor_encrypt(&cipher, &key);
                return String::from_utf8(plain)
                    .map(EvalValue::Str)
                    .map_err(|_| "crypto.decrypt: the result is not valid UTF-8".into());
            }
            aes_decrypt(&cipher_b64, &key).map(EvalValue::Str)
        }
        // token(length?) → hex token aleatorio
        "token" => {
            let length = if args.is_empty() { 16 } else { to_i64(&args[0])? as usize };
            Ok(EvalValue::Str(random_hex(length)))
        }
        // uuid() → UUID v4
        "uuid" | "uuid_str" => {
            Ok(EvalValue::Str(gen_uuid()))
        }
        // entropy(n?) → hash aleatorio de alta entropía
        "entropy" => {
            let n = if args.is_empty() { 32usize } else { to_i64(&args[0])? as usize };
            let mut rng = rand::thread_rng();
            let bytes: Vec<u8> = (0..n).map(|_| rng.gen()).collect();
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            Ok(EvalValue::Str(hex_encode(&hasher.finalize())))
        }
        // context_token(context, ttl?) → token ligado a tiempo
        "context_token" => {
            if args.is_empty() { return Err("crypto.context_token requires (context, ttl?)".into()); }
            let context = to_str(&args[0]);
            let ttl = if args.len() > 1 { to_i64(&args[1])? as u64 } else { 60 };
            use std::time::{SystemTime, UNIX_EPOCH};
            let ts = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs()).unwrap_or(0);
            let window = ts / ttl;
            let base = format!("{}:{}", context, window);
            let mut hasher = Sha256::new();
            hasher.update(base.as_bytes());
            Ok(EvalValue::Str(hex_encode(&hasher.finalize())))
        }
        // random_key(length?) → clave aleatoria
        "random_key" | "random" => {
            let length = if args.is_empty() { 32usize } else { to_i64(&args[0])? as usize };
            Ok(EvalValue::Str(random_hex(length)))
        }

        f => Err(format!("crypto.{}() does not exist", f)),
    }
}

//     SHA256 hash con salt                                                      

fn do_hash(data: &str, salt: &str, _algo: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    hasher.update(salt.as_bytes());
    let pepper = std::env::var("ORION_PEPPER").unwrap_or_else(|_| "default_orion_pepper".into());
    hasher.update(pepper.as_bytes());
    hex_encode(&hasher.finalize())
}

//     HMAC-SHA256                                                               

fn hmac_sign(data: &[u8], key: &[u8]) -> Result<String, String> {
    // Desambiguar: aes_gcm::KeyInit también aporta new_from_slice al scope.
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key)
        .map_err(|e| format!("crypto.sign: invalid key: {}", e))?;
    mac.update(data);
    Ok(hex_encode(&mac.finalize().into_bytes()))
}

//     AES-256-GCM — cifrado autenticado real
//
// Clave AES derivada del password con Argon2id + salt aleatorio (memory-hard).
// La clave puede venir del usuario (posible password débil), así que un SHA-256
// plano —el esquema viejo— la dejaba rompible por fuerza bruta en GPU y sin salt
// exponía a rainbow tables. Formato versionado:
//   v1 (actual):  base64( 0x01 ‖ salt[16] ‖ nonce[12] ‖ ct )
//   legacy:       base64(                    nonce[12] ‖ ct )   (SHA-256)
// GCM autentica el tag, lo que desambigua v1 vs legacy al descifrar.

const AES_V1: u8 = 0x01;

fn kdf_argon2(key: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let mut out = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(key.as_bytes(), salt, &mut out)
        .map_err(|e| format!("crypto: key derivation: {}", e))?;
    Ok(out)
}

fn aes_encrypt(data: &[u8], key: &str) -> Result<String, String> {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill(&mut salt);
    let k = kdf_argon2(key, &salt)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&k));

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ct = cipher.encrypt(nonce, data)
        .map_err(|e| format!("crypto.encrypt: {}", e))?;

    let mut out = Vec::with_capacity(1 + 16 + 12 + ct.len());
    out.push(AES_V1);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(b64_encode(&out))
}

fn aes_decrypt(cipher_b64: &str, key: &str) -> Result<String, String> {
    let raw = b64_decode(cipher_b64).map_err(|e| format!("crypto.decrypt: {}", e))?;

    // Formato v1: Argon2id + salt.
    if raw.first() == Some(&AES_V1) && raw.len() >= 1 + 16 + 12 {
        let k = kdf_argon2(key, &raw[1..17])?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&k));
        if let Ok(plain) = cipher.decrypt(Nonce::from_slice(&raw[17..29]), &raw[29..]) {
            return String::from_utf8(plain)
                .map_err(|_| "crypto.decrypt: the result is not valid UTF-8".to_string());
        }
    }

    // Formato legacy: SHA-256 sin salt, nonce[12] ‖ ct.
    if raw.len() >= 12 {
        let k = Sha256::digest(key.as_bytes());
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&k));
        if let Ok(plain) = cipher.decrypt(Nonce::from_slice(&raw[..12]), &raw[12..]) {
            return String::from_utf8(plain)
                .map_err(|_| "crypto.decrypt: the result is not valid UTF-8".to_string());
        }
    }

    Err("crypto.decrypt: wrong key, or tampered data".into())
}

//     XOR — LEGACY, inseguro. Solo para descifrar datos del formato antiguo
//     (crypto.decrypt(c, k, "xor")). No se usa para cifrar nunca más.

fn xor_encrypt(data: &[u8], key: &str) -> (Vec<u8>, String) {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let key_bytes = hasher.finalize();
    let key_slice = &key_bytes[..16];
    let encrypted: Vec<u8> = data.iter().enumerate()
        .map(|(i, &b)| b ^ key_slice[i % key_slice.len()])
        .collect();
    (encrypted, key.to_string())
}

//     Hex encoding                                                              

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

//     Random hex                                                                

fn random_hex(n: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..n).map(|_| format!("{:02x}", rng.gen::<u8>())).collect::<String>()[..n*2.min(n*2)].to_string()
}

//     UUID v4                                                                   

fn gen_uuid() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]) & 0x0fff,
        (u16::from_be_bytes([bytes[8], bytes[9]]) & 0x3fff) | 0x8000,
        {
            let hi = (bytes[10] as u64) << 32;
            let lo = u32::from_be_bytes([bytes[11], bytes[12], bytes[13], bytes[14]]) as u64;
            hi | lo
        }
    )
}

//     Base64 (sin dep externa)                                                  

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(input: &[u8]) -> String {
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        out.push(B64[(b0 >> 2) as usize] as char);
        out.push(B64[((b0 & 3) << 4 | b1 >> 4) as usize] as char);
        if chunk.len() > 1 { out.push(B64[((b1 & 0xf) << 2 | b2 >> 6) as usize] as char); } else { out.push('='); }
        if chunk.len() > 2 { out.push(B64[(b2 & 0x3f) as usize] as char); } else { out.push('='); }
    }
    out
}

fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut table = [255u8; 256];
    for (i, &c) in B64.iter().enumerate() { table[c as usize] = i as u8; }
    let clean: Vec<u8> = input.chars().filter(|c| !c.is_whitespace() && *c != '=')
        .map(|c| {
            let v = table[c as usize];
            if v == 255 { Err(format!("invalid character: {}", c)) } else { Ok(v) }
        })
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        let b3 = if chunk.len() > 3 { chunk[3] } else { 0 };
        out.push(b0 << 2 | b1 >> 4);
        if chunk.len() > 2 { out.push((b1 & 0xf) << 4 | b2 >> 2); }
        if chunk.len() > 3 { out.push((b2 & 3) << 6 | b3); }
    }
    Ok(out)
}

//     Helpers

fn one_str(fn_name: &str, args: Vec<EvalValue>) -> Result<String, String> {
    if args.is_empty() { return Err(format!("crypto.{}() requires 1 argument", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("crypto: expected a number, got {}", other.type_name())),
    }
}
