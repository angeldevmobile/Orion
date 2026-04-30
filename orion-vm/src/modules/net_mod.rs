use crate::eval_value::EvalValue;
use std::collections::HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // reach(url, headers?) → {status, body, ok}
        "reach" | "get" => {
            if args.is_empty() { return Err("net.reach requiere (url)".into()); }
            let url     = to_str(&args[0]);
            let headers = extract_headers(args.get(1));
            http_get(&url, headers)
        }
        // transmit(url, body, headers?) → {status, body, ok}
        "transmit" | "post" => {
            if args.is_empty() { return Err("net.transmit requiere (url, body?)".into()); }
            let url     = to_str(&args[0]);
            let body    = args.get(1).cloned();
            let headers = extract_headers(args.get(2));
            http_post(&url, body, headers)
        }
        // put(url, body, headers?) → {status, body, ok}
        "put" => {
            if args.is_empty() { return Err("net.put requiere (url, body?)".into()); }
            let url     = to_str(&args[0]);
            let body    = args.get(1).cloned();
            let headers = extract_headers(args.get(2));
            http_method("PUT", &url, body, headers)
        }
        // delete(url, headers?) → {status, body, ok}
        "delete" => {
            if args.is_empty() { return Err("net.delete requiere (url)".into()); }
            let url     = to_str(&args[0]);
            let headers = extract_headers(args.get(1));
            http_method("DELETE", &url, None, headers)
        }
        // status(url) → int código HTTP
        "status" => {
            if args.is_empty() { return Err("net.status requiere (url)".into()); }
            let url = to_str(&args[0]);
            let resp = ureq::get(&url).call();
            match resp {
                Ok(r) => Ok(EvalValue::Int(r.status() as i64)),
                Err(ureq::Error::Status(code, _)) => Ok(EvalValue::Int(code as i64)),
                Err(e) => Err(format!("net.status: {}", e)),
            }
        }
        // download(url, path) → guarda archivo
        "download" => {
            if args.len() < 2 { return Err("net.download requiere (url, path)".into()); }
            let url  = to_str(&args[0]);
            let path = to_str(&args[1]);
            let resp = ureq::get(&url).call()
                .map_err(|e| format!("net.download: {}", e))?;
            let mut reader = resp.into_reader();
            let mut buf = Vec::new();
            use std::io::Read;
            reader.read_to_end(&mut buf).map_err(|e| format!("net.download: {}", e))?;
            std::fs::write(&path, &buf).map_err(|e| format!("net.download: {}", e))?;
            Ok(EvalValue::Str(path))
        }
        // resolve(host) → IP string
        "resolve" => {
            if args.is_empty() { return Err("net.resolve requiere (host)".into()); }
            let host = to_str(&args[0]);
            use std::net::ToSocketAddrs;
            let addr = format!("{}:80", host).to_socket_addrs()
                .map_err(|e| format!("net.resolve: {}", e))?
                .next()
                .ok_or_else(|| format!("net.resolve: sin resultado para {}", host))?;
            Ok(EvalValue::Str(addr.ip().to_string()))
        }
        // pulse(host, port?) → {alive, latency_ms}
        "pulse" => {
            if args.is_empty() { return Err("net.pulse requiere (host, port?)".into()); }
            let host = to_str(&args[0]);
            let port = if args.len() > 1 { to_i64(&args[1])? as u16 } else { 80 };
            let start = std::time::Instant::now();
            let alive = std::net::TcpStream::connect(format!("{}:{}", host, port)).is_ok();
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            let mut m = HashMap::new();
            m.insert("alive".into(),      EvalValue::Bool(alive));
            m.insert("latency_ms".into(), EvalValue::Float((latency * 100.0).round() / 100.0));
            Ok(EvalValue::Dict(m))
        }

        f => Err(format!("net.{}() no existe", f)),
    }
}

fn http_get(url: &str, headers: Vec<(String, String)>) -> Result<EvalValue, String> {
    let mut req = ureq::get(url);
    for (k, v) in &headers { req = req.set(k, v); }
    match req.call() {
        Ok(resp)  => pack_response(resp),
        Err(ureq::Error::Status(code, resp)) => pack_error_response(code, resp),
        Err(e)    => Err(format!("net.reach: {}", e)),
    }
}

fn http_post(url: &str, body: Option<EvalValue>, headers: Vec<(String, String)>) -> Result<EvalValue, String> {
    http_method("POST", url, body, headers)
}

fn http_method(method: &str, url: &str, body: Option<EvalValue>, headers: Vec<(String, String)>) -> Result<EvalValue, String> {
    let mut req = match method {
        "POST"   => ureq::post(url),
        "PUT"    => ureq::put(url),
        "DELETE" => ureq::delete(url),
        _        => ureq::post(url),
    };
    for (k, v) in &headers { req = req.set(k, v); }

    let result = match body {
        None => req.call().map_err(|e| match e {
            ureq::Error::Status(code, r) => format!("HTTP {}: {}", code, r.into_string().unwrap_or_default()),
            other => format!("net.{}: {}", method.to_lowercase(), other),
        }),
        Some(EvalValue::Str(s)) => req.send_string(&s).map_err(|e| format!("net.{}: {}", method.to_lowercase(), e)),
        Some(EvalValue::Dict(_)) | Some(EvalValue::List(_)) => {
            // serializa como JSON automáticamente
            let json_body = crate::modules::json_mod::eval_to_json(body.unwrap());
            req.set("Content-Type", "application/json")
               .send_string(&json_body.to_string())
               .map_err(|e| format!("net.{}: {}", method.to_lowercase(), e))
        }
        Some(other) => req.send_string(&format!("{}", other))
            .map_err(|e| format!("net.{}: {}", method.to_lowercase(), e)),
    };

    match result {
        Ok(resp)  => pack_response(resp),
        Err(e)    => Err(e),
    }
}

fn pack_response(resp: ureq::Response) -> Result<EvalValue, String> {
    let status = resp.status();
    let body   = resp.into_string().unwrap_or_default();
    let mut m  = HashMap::new();
    m.insert("status".into(), EvalValue::Int(status as i64));
    m.insert("ok".into(),     EvalValue::Bool(status >= 200 && status < 300));

    // Intenta parsear como JSON automáticamente
    if let Ok(j) = serde_json::from_str::<serde_json::Value>(&body) {
        m.insert("body".into(), crate::modules::json_mod::json_to_eval(j));
    } else {
        m.insert("body".into(), EvalValue::Str(body));
    }
    Ok(EvalValue::Dict(m))
}

fn pack_error_response(code: u16, resp: ureq::Response) -> Result<EvalValue, String> {
    let body = resp.into_string().unwrap_or_default();
    let mut m = HashMap::new();
    m.insert("status".into(), EvalValue::Int(code as i64));
    m.insert("ok".into(),     EvalValue::Bool(false));
    m.insert("body".into(),   EvalValue::Str(body));
    Ok(EvalValue::Dict(m))
}

fn extract_headers(v: Option<&EvalValue>) -> Vec<(String, String)> {
    match v {
        Some(EvalValue::Dict(m)) => m.iter()
            .map(|(k, v)| (k.clone(), format!("{}", v)))
            .collect(),
        _ => vec![],
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("net: esperaba número, recibió {}", other.type_name())),
    }
}
