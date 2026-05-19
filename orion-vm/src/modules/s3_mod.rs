use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::sync::Mutex;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Digest};
use chrono::Utc;

type HmacSha256 = Hmac<Sha256>;

struct S3Config {
    endpoint: String,
    access_key: String,
    secret_key: String,
    region: String,
}

static CONFIG: Mutex<Option<S3Config>> = Mutex::new(None);

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex_encode(&h.finalize())
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).unwrap();
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn parse_host(endpoint: &str) -> String {
    let s = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    s.split('/').next().unwrap_or(s).to_string()
}

fn build_auth_headers(
    method: &str,
    endpoint: &str,
    bucket: &str,
    key: &str,
    query: &str,
    body: &[u8],
    cfg: &S3Config,
) -> HashMap<String, String> {
    let now = Utc::now();
    let date = now.format("%Y%m%d").to_string();
    let datetime = now.format("%Y%m%dT%H%M%SZ").to_string();
    let payload_hash = sha256_hex(body);
    let host = parse_host(&cfg.endpoint);
    let path = if key.is_empty() {
        format!("/{}", bucket)
    } else {
        format!("/{}/{}", bucket, key)
    };
    let canonical_uri = path.clone();
    let canonical_querystring = query.to_string();
    let canonical_headers = format!(
        "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
        host, payload_hash, datetime
    );
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, canonical_uri, canonical_querystring,
        canonical_headers, signed_headers, payload_hash
    );
    let scope = format!("{}/{}/s3/aws4_request", date, cfg.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        datetime, scope, sha256_hex(canonical_request.as_bytes())
    );
    let k_date    = hmac_sha256(format!("AWS4{}", cfg.secret_key).as_bytes(), date.as_bytes());
    let k_region  = hmac_sha256(&k_date,    cfg.region.as_bytes());
    let k_service = hmac_sha256(&k_region,  b"s3");
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex_encode(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));
    let auth = format!(
        "AWS4-HMAC-SHA256 Credential={}/{},SignedHeaders={},Signature={}",
        cfg.access_key, scope, signed_headers, signature
    );
    let mut h = HashMap::new();
    h.insert("Authorization".into(), auth);
    h.insert("x-amz-date".into(), datetime);
    h.insert("x-amz-content-sha256".into(), payload_hash);
    h.insert("host".into(), host);
    let _ = (endpoint, key);
    h
}

fn get_config() -> Result<S3Config, String> {
    let lock = CONFIG.lock().unwrap();
    match &*lock {
        Some(c) => Ok(S3Config {
            endpoint:   c.endpoint.clone(),
            access_key: c.access_key.clone(),
            secret_key: c.secret_key.clone(),
            region:     c.region.clone(),
        }),
        None => Err("s3: llama s3.config() primero".into()),
    }
}

fn obj_url(cfg: &S3Config, bucket: &str, key: &str) -> String {
    format!("{}/{}/{}", cfg.endpoint.trim_end_matches('/'), bucket, key)
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // s3.config(endpoint, access_key, secret_key, region)
        "config" => {
            if args.len() < 4 {
                return Err("s3.config requiere (endpoint, access_key, secret_key, region)".into());
            }
            let endpoint   = to_str(&args[0]);
            let access_key = to_str(&args[1]);
            let secret_key = to_str(&args[2]);
            let region     = to_str(&args[3]);
            *CONFIG.lock().unwrap() = Some(S3Config { endpoint, access_key, secret_key, region });
            Ok(EvalValue::Null)
        }

        // s3.upload(bucket, key, local_path) → {ok, url}
        "upload" => {
            if args.len() < 3 {
                return Err("s3.upload requiere (bucket, key, local_path)".into());
            }
            let bucket = to_str(&args[0]);
            let key    = to_str(&args[1]);
            let path   = to_str(&args[2]);
            let cfg    = get_config()?;
            let body = std::fs::read(&path)
                .map_err(|e| format!("s3.upload: no se pudo leer '{}': {}", path, e))?;
            let headers = build_auth_headers("PUT", &cfg.endpoint, &bucket, &key, "", &body, &cfg);
            let url = obj_url(&cfg, &bucket, &key);
            let mut req = ureq::put(&url);
            for (k, v) in &headers { req = req.set(k, v); }
            req.send_bytes(&body)
                .map_err(|e| format!("s3.upload error: {}", e))?;
            let mut m = HashMap::new();
            m.insert("ok".into(),  EvalValue::Bool(true));
            m.insert("url".into(), EvalValue::Str(url));
            Ok(EvalValue::Dict(m))
        }

        // s3.download(bucket, key, local_path) → {ok, bytes}
        "download" => {
            if args.len() < 3 {
                return Err("s3.download requiere (bucket, key, local_path)".into());
            }
            let bucket = to_str(&args[0]);
            let key    = to_str(&args[1]);
            let dest   = to_str(&args[2]);
            let cfg    = get_config()?;
            let headers = build_auth_headers("GET", &cfg.endpoint, &bucket, &key, "", b"", &cfg);
            let url = obj_url(&cfg, &bucket, &key);
            let mut req = ureq::get(&url);
            for (k, v) in &headers { req = req.set(k, v); }
            let resp = req.call().map_err(|e| format!("s3.download error: {}", e))?;
            let mut buf = Vec::new();
            resp.into_reader()
                .read_to_end(&mut buf)
                .map_err(|e| format!("s3.download lectura: {}", e))?;
            std::fs::write(&dest, &buf)
                .map_err(|e| format!("s3.download escritura: {}", e))?;
            let mut m = HashMap::new();
            m.insert("ok".into(),    EvalValue::Bool(true));
            m.insert("bytes".into(), EvalValue::Int(buf.len() as i64));
            Ok(EvalValue::Dict(m))
        }

        // s3.list(bucket, prefix?) → [{key, size, modified}]
        "list" => {
            if args.is_empty() {
                return Err("s3.list requiere (bucket, prefix?)".into());
            }
            let bucket = to_str(&args[0]);
            let prefix = if args.len() > 1 { to_str(&args[1]) } else { String::new() };
            let cfg    = get_config()?;
            let query  = if prefix.is_empty() {
                "list-type=2".to_string()
            } else {
                format!("list-type=2&prefix={}", urlenc(&prefix))
            };
            let headers = build_auth_headers("GET", &cfg.endpoint, &bucket, "", &query, b"", &cfg);
            let url = format!("{}/{}?{}", cfg.endpoint.trim_end_matches('/'), bucket, query);
            let mut req = ureq::get(&url);
            for (k, v) in &headers { req = req.set(k, v); }
            let xml = req.call()
                .map_err(|e| format!("s3.list error: {}", e))?
                .into_string()
                .map_err(|e| format!("s3.list lectura: {}", e))?;
            // Parse XML básico: extraer <Key>, <Size>, <LastModified>
            let keys  = xml_extract_all(&xml, "Key");
            let sizes = xml_extract_all(&xml, "Size");
            let dates = xml_extract_all(&xml, "LastModified");
            let mut out = Vec::new();
            for (i, k) in keys.iter().enumerate() {
                let mut m = HashMap::new();
                m.insert("key".into(),      EvalValue::Str(k.clone()));
                m.insert("size".into(),     EvalValue::Int(
                    sizes.get(i).and_then(|s| s.parse().ok()).unwrap_or(0)
                ));
                m.insert("modified".into(), EvalValue::Str(
                    dates.get(i).cloned().unwrap_or_default()
                ));
                out.push(EvalValue::Dict(m));
            }
            Ok(EvalValue::List(out))
        }

        // s3.delete(bucket, key) → {ok}
        "delete" => {
            if args.len() < 2 {
                return Err("s3.delete requiere (bucket, key)".into());
            }
            let bucket = to_str(&args[0]);
            let key    = to_str(&args[1]);
            let cfg    = get_config()?;
            let headers = build_auth_headers("DELETE", &cfg.endpoint, &bucket, &key, "", b"", &cfg);
            let url = obj_url(&cfg, &bucket, &key);
            let mut req = ureq::delete(&url);
            for (k, v) in &headers { req = req.set(k, v); }
            req.call().map_err(|e| format!("s3.delete error: {}", e))?;
            let mut m = HashMap::new();
            m.insert("ok".into(), EvalValue::Bool(true));
            Ok(EvalValue::Dict(m))
        }

        // s3.exists(bucket, key) → bool
        "exists" => {
            if args.len() < 2 {
                return Err("s3.exists requiere (bucket, key)".into());
            }
            let bucket = to_str(&args[0]);
            let key    = to_str(&args[1]);
            let cfg    = get_config()?;
            let headers = build_auth_headers("HEAD", &cfg.endpoint, &bucket, &key, "", b"", &cfg);
            let url = obj_url(&cfg, &bucket, &key);
            let mut req = ureq::head(&url);
            for (k, v) in &headers { req = req.set(k, v); }
            match req.call() {
                Ok(_)  => Ok(EvalValue::Bool(true)),
                Err(_) => Ok(EvalValue::Bool(false)),
            }
        }

        // s3.url(bucket, key) → string  — URL pública del objeto
        "url" => {
            if args.len() < 2 {
                return Err("s3.url requiere (bucket, key)".into());
            }
            let bucket = to_str(&args[0]);
            let key    = to_str(&args[1]);
            let cfg    = get_config()?;
            Ok(EvalValue::Str(obj_url(&cfg, &bucket, &key)))
        }

        // s3.copy(src_bucket, src_key, dst_bucket, dst_key) → {ok}
        // Copia server-side: no descarga el archivo, S3 lo copia en su infraestructura.
        "copy" => {
            if args.len() < 4 {
                return Err("s3.copy requiere (src_bucket, src_key, dst_bucket, dst_key)".into());
            }
            let src_bucket = to_str(&args[0]);
            let src_key    = to_str(&args[1]);
            let dst_bucket = to_str(&args[2]);
            let dst_key    = to_str(&args[3]);
            let cfg = get_config()?;
            // Header x-amz-copy-source apunta al origen
            let copy_source = format!("/{}/{}", src_bucket, src_key);
            let url = obj_url(&cfg, &dst_bucket, &dst_key);
            let mut headers = build_auth_headers("PUT", &cfg.endpoint, &dst_bucket, &dst_key, "", b"", &cfg);
            headers.insert("x-amz-copy-source".into(), copy_source);
            let mut req = ureq::put(&url);
            for (k, v) in &headers { req = req.set(k, v); }
            req.send_string("")
                .map_err(|e| format!("s3.copy error: {}", e))?;
            let mut m = HashMap::new();
            m.insert("ok".into(), EvalValue::Bool(true));
            Ok(EvalValue::Dict(m))
        }

        // s3.move(src_bucket, src_key, dst_bucket, dst_key) → {ok}
        // Copia server-side y luego elimina el origen (equivale a renombrar/mover).
        "move" => {
            if args.len() < 4 {
                return Err("s3.move requiere (src_bucket, src_key, dst_bucket, dst_key)".into());
            }
            let src_bucket = to_str(&args[0]);
            let src_key    = to_str(&args[1]);
            let dst_bucket = to_str(&args[2]);
            let dst_key    = to_str(&args[3]);
            let cfg = get_config()?;
            // Copiar primero
            let copy_source = format!("/{}/{}", src_bucket, src_key);
            let dst_url = obj_url(&cfg, &dst_bucket, &dst_key);
            let mut headers = build_auth_headers("PUT", &cfg.endpoint, &dst_bucket, &dst_key, "", b"", &cfg);
            headers.insert("x-amz-copy-source".into(), copy_source);
            let mut req = ureq::put(&dst_url);
            for (k, v) in &headers { req = req.set(k, v); }
            req.send_string("")
                .map_err(|e| format!("s3.move error al copiar: {}", e))?;
            // Eliminar origen
            let src_headers = build_auth_headers("DELETE", &cfg.endpoint, &src_bucket, &src_key, "", b"", &cfg);
            let src_url = obj_url(&cfg, &src_bucket, &src_key);
            let mut del = ureq::delete(&src_url);
            for (k, v) in &src_headers { del = del.set(k, v); }
            del.call().map_err(|e| format!("s3.move error al eliminar origen: {}", e))?;
            let mut m = HashMap::new();
            m.insert("ok".into(), EvalValue::Bool(true));
            Ok(EvalValue::Dict(m))
        }

        // s3.size(bucket, key) → int (bytes)  — tamaño sin descargar
        "size" => {
            if args.len() < 2 {
                return Err("s3.size requiere (bucket, key)".into());
            }
            let bucket = to_str(&args[0]);
            let key    = to_str(&args[1]);
            let cfg    = get_config()?;
            let headers = build_auth_headers("HEAD", &cfg.endpoint, &bucket, &key, "", b"", &cfg);
            let url = obj_url(&cfg, &bucket, &key);
            let mut req = ureq::head(&url);
            for (k, v) in &headers { req = req.set(k, v); }
            let resp = req.call().map_err(|e| format!("s3.size error: {}", e))?;
            let size = resp.header("content-length")
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            Ok(EvalValue::Int(size))
        }

        f => Err(format!(
            "s3.{}() no existe. Funciones: config, upload, download, list, delete, exists, \
             url, copy, move, size",
            f
        )),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v {
        EvalValue::Str(s) => s.clone(),
        other => other.to_string(),
    }
}

fn urlenc(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "%20".to_string(),
            c => format!("%{:02X}", c as u32),
        })
        .collect()
}

fn xml_extract_all(xml: &str, tag: &str) -> Vec<String> {
    let open  = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(start) = xml[pos..].find(&open) {
        let abs_start = pos + start + open.len();
        if let Some(end) = xml[abs_start..].find(&close) {
            out.push(xml[abs_start..abs_start + end].to_string());
            pos = abs_start + end + close.len();
        } else { break; }
    }
    out
}

// Necesario para read_to_end
use std::io::Read;
