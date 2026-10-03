use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // reach(url, headers?, opts?) → {status, body, ok, headers}
        // opts: { timeout: segundos } (30 por defecto). Un 4xx/5xx no es error:
        // vuelve con ok = no y su status, para que el script decida.
        "reach" | "get" => {
            if args.is_empty() { return Err("net.reach requires (url)".into()); }
            let url = to_str(&args[0]);
            http_method("GET", &url, None, extract_headers(args.get(1)), timeout_de(args.get(2)))
        }
        // transmit(url, body, headers?, opts?) → {status, body, ok, headers}
        "transmit" | "post" => {
            if args.is_empty() { return Err("net.transmit requires (url, body?)".into()); }
            let url = to_str(&args[0]);
            http_method("POST", &url, args.get(1).cloned(), extract_headers(args.get(2)), timeout_de(args.get(3)))
        }
        // put(url, body, headers?, opts?) → {status, body, ok, headers}
        "put" => {
            if args.is_empty() { return Err("net.put requires (url, body?)".into()); }
            let url = to_str(&args[0]);
            http_method("PUT", &url, args.get(1).cloned(), extract_headers(args.get(2)), timeout_de(args.get(3)))
        }
        // delete(url, headers?, opts?) → {status, body, ok, headers}
        "delete" => {
            if args.is_empty() { return Err("net.delete requires (url)".into()); }
            let url = to_str(&args[0]);
            http_method("DELETE", &url, None, extract_headers(args.get(1)), timeout_de(args.get(2)))
        }
        // status(url) → int código HTTP
        "status" => {
            if args.is_empty() { return Err("net.status requires (url)".into()); }
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
            if args.len() < 2 { return Err("net.download requires (url, path)".into()); }
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
            if args.is_empty() { return Err("net.resolve requires (host)".into()); }
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
            if args.is_empty() { return Err("net.pulse requires (host, port?)".into()); }
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

        f => Err(format!("net.{}() does not exist", f)),
    }
}

const TIMEOUT_POR_DEFECTO: f64 = 30.0;

/// `{ timeout: segundos }`; sin él, 30 s. Sin tope, un servidor que no
/// responde dejaría colgado al script (o a un hilo de `serve`) para siempre.
fn timeout_de(opts: Option<&EvalValue>) -> std::time::Duration {
    let s = match opts {
        Some(EvalValue::Dict(m)) => match m.get("timeout") {
            Some(EvalValue::Int(n))   => *n as f64,
            Some(EvalValue::Float(f)) => *f,
            _ => TIMEOUT_POR_DEFECTO,
        },
        _ => TIMEOUT_POR_DEFECTO,
    };
    std::time::Duration::from_secs_f64(s.max(0.1))
}

fn http_method(
    method: &str, url: &str, body: Option<EvalValue>,
    headers: Vec<(String, String)>, timeout: std::time::Duration,
) -> Result<EvalValue, String> {
    let agente = ureq::AgentBuilder::new().timeout(timeout).build();
    let mut req = agente.request(method, url);
    for (k, v) in &headers { req = req.set(k, v); }

    let result = match body {
        None => req.call(),
        Some(EvalValue::Str(s)) => req.send_string(&s),
        Some(b @ (EvalValue::Dict(_) | EvalValue::List(_))) => {
            let json_body = crate::modules::json_mod::eval_to_json(b);
            req.set("Content-Type", "application/json").send_string(&json_body.to_string())
        }
        Some(other) => req.send_string(&format!("{}", other)),
    };

    match result {
        Ok(resp) | Err(ureq::Error::Status(_, resp)) => pack_response(resp),
        Err(e) => Err(format!("net.{}: {}", method.to_lowercase(), e)),
    }
}

fn pack_response(resp: ureq::Response) -> Result<EvalValue, String> {
    let status = resp.status();

    // Headers de respuesta (claves en minúscula) — permiten verificar CORS,
    // content-type, cache, etc. desde Orion.
    let mut headers = HashMap::new();
    for name in resp.headers_names() {
        if let Some(v) = resp.header(&name) {
            headers.insert(name.to_lowercase(), EvalValue::Str(v.to_string()));
        }
    }

    let body   = resp.into_string().unwrap_or_default();
    let mut m  = HashMap::new();
    m.insert("status".into(),  EvalValue::Int(status as i64));
    m.insert("ok".into(),      EvalValue::Bool(status >= 200 && status < 300));
    m.insert("headers".into(), EvalValue::Dict(headers));

    // Intenta parsear como JSON automáticamente
    if let Ok(j) = serde_json::from_str::<serde_json::Value>(&body) {
        m.insert("body".into(), crate::modules::json_mod::json_to_eval(j));
    } else {
        m.insert("body".into(), EvalValue::Str(body));
    }
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
        other => Err(format!("net: expected a number, got {}", other.type_name())),
    }
}
