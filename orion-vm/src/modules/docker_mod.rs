use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::sync::Mutex;

static DOCKER_HOST: Mutex<String> = Mutex::new(String::new());

fn host() -> String {
    let h = DOCKER_HOST.lock().unwrap();
    if h.is_empty() {
        "http://localhost:2375".to_string()
    } else {
        h.clone()
    }
}

fn api(path: &str) -> String {
    format!("{}/v1.43{}", host(), path)
}

fn get_json(path: &str) -> Result<serde_json::Value, String> {
    ureq::get(&api(path))
        .set("Content-Type", "application/json")
        .call()
        .map_err(|e| format!("docker: GET {} error: {}", path, e))?
        .into_json::<serde_json::Value>()
        .map_err(|e| format!("docker: error parseando respuesta: {}", e))
}

fn post_empty(path: &str) -> Result<(), String> {
    let url = api(path);
    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .send_string("{}")
        .map_err(|e| format!("docker: POST {} error: {}", path, e))?;
    let status = resp.status();
    if status >= 200 && status < 300 {
        Ok(())
    } else {
        Err(format!("docker: POST {} devolvió status {}", path, status))
    }
}

fn post_json(path: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let body_str = serde_json::to_string(body).unwrap_or_default();
    let resp = ureq::post(&api(path))
        .set("Content-Type", "application/json")
        .send_string(&body_str)
        .map_err(|e| format!("docker: POST {} error: {}", path, e))?;
    resp.into_json::<serde_json::Value>()
        .unwrap_or(serde_json::Value::Null)
        .pipe_ok()
}

trait PipeOk {
    fn pipe_ok(self) -> Result<serde_json::Value, String>;
}
impl PipeOk for serde_json::Value {
    fn pipe_ok(self) -> Result<serde_json::Value, String> { Ok(self) }
}

fn delete_req(path: &str) -> Result<(), String> {
    let url = api(path);
    ureq::delete(&url)
        .call()
        .map_err(|e| format!("docker: DELETE {} error: {}", path, e))?;
    Ok(())
}

fn json_to_eval(v: &serde_json::Value) -> EvalValue {
    match v {
        serde_json::Value::Null    => EvalValue::Null,
        serde_json::Value::Bool(b) => EvalValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { EvalValue::Int(i) }
            else if let Some(f) = n.as_f64() { EvalValue::Float(f) }
            else { EvalValue::Str(n.to_string()) }
        }
        serde_json::Value::String(s) => EvalValue::Str(s.clone()),
        serde_json::Value::Array(a)  => EvalValue::List(a.iter().map(json_to_eval).collect()),
        serde_json::Value::Object(o) => {
            let mut m = HashMap::new();
            for (k, v) in o { m.insert(k.clone(), json_to_eval(v)); }
            EvalValue::Dict(m)
        }
    }
}

fn container_summary(v: &serde_json::Value) -> EvalValue {
    let mut m = HashMap::new();
    m.insert("id".into(),     EvalValue::Str(
        v["Id"].as_str().unwrap_or("").chars().take(12).collect()
    ));
    m.insert("image".into(),  EvalValue::Str(
        v["Image"].as_str().unwrap_or("").to_string()
    ));
    m.insert("status".into(), EvalValue::Str(
        v["Status"].as_str().unwrap_or("").to_string()
    ));
    m.insert("state".into(),  EvalValue::Str(
        v["State"].as_str().unwrap_or("").to_string()
    ));
    // Nombre (quitar el "/" inicial)
    let name = v["Names"].as_array()
        .and_then(|a| a.first())
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .trim_start_matches('/')
        .to_string();
    m.insert("name".into(), EvalValue::Str(name));
    EvalValue::Dict(m)
}

fn image_summary(v: &serde_json::Value) -> EvalValue {
    let mut m = HashMap::new();
    m.insert("id".into(),      EvalValue::Str(
        v["Id"].as_str().unwrap_or("").replace("sha256:", "").chars().take(12).collect()
    ));
    m.insert("created".into(), EvalValue::Int(v["Created"].as_i64().unwrap_or(0)));
    m.insert("size".into(),    EvalValue::Int(v["Size"].as_i64().unwrap_or(0)));
    let tags: Vec<EvalValue> = v["RepoTags"].as_array()
        .map(|a| a.iter()
            .filter_map(|t| t.as_str())
            .map(|t| EvalValue::Str(t.to_string()))
            .collect()
        )
        .unwrap_or_default();
    m.insert("tags".into(), EvalValue::List(tags));
    EvalValue::Dict(m)
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // docker.config(host?) — default: "http://localhost:2375"
        "config" => {
            let h = if args.is_empty() {
                "http://localhost:2375".to_string()
            } else {
                to_str(&args[0])
            };
            *DOCKER_HOST.lock().unwrap() = h;
            Ok(EvalValue::Null)
        }

        // docker.containers(all?) → [{id, name, image, status, state}]
        "containers" => {
            let all = args.first().map(|v| v.is_truthy()).unwrap_or(false);
            let path = if all { "/containers/json?all=1" } else { "/containers/json" };
            let json = get_json(path)?;
            let list = json.as_array()
                .ok_or("docker.containers: respuesta inesperada")?
                .iter()
                .map(container_summary)
                .collect();
            Ok(EvalValue::List(list))
        }

        // docker.start(id) → {ok}
        "start" => {
            if args.is_empty() { return Err("docker.start requiere (id)".into()); }
            let id = to_str(&args[0]);
            post_empty(&format!("/containers/{}/start", id))?;
            let mut m = HashMap::new();
            m.insert("ok".into(), EvalValue::Bool(true));
            Ok(EvalValue::Dict(m))
        }

        // docker.stop(id, timeout?) → {ok}
        "stop" => {
            if args.is_empty() { return Err("docker.stop requiere (id)".into()); }
            let id      = to_str(&args[0]);
            let timeout = args.get(1).and_then(|v| v.to_i64().ok()).unwrap_or(10);
            post_empty(&format!("/containers/{}/stop?t={}", id, timeout))?;
            let mut m = HashMap::new();
            m.insert("ok".into(), EvalValue::Bool(true));
            Ok(EvalValue::Dict(m))
        }

        // docker.restart(id) → {ok}
        "restart" => {
            if args.is_empty() { return Err("docker.restart requiere (id)".into()); }
            let id = to_str(&args[0]);
            post_empty(&format!("/containers/{}/restart", id))?;
            let mut m = HashMap::new();
            m.insert("ok".into(), EvalValue::Bool(true));
            Ok(EvalValue::Dict(m))
        }

        // docker.kill(id) → {ok}
        "kill" => {
            if args.is_empty() { return Err("docker.kill requiere (id)".into()); }
            let id = to_str(&args[0]);
            post_empty(&format!("/containers/{}/kill", id))?;
            let mut m = HashMap::new();
            m.insert("ok".into(), EvalValue::Bool(true));
            Ok(EvalValue::Dict(m))
        }

        // docker.remove(id, force?) → {ok}
        "remove" => {
            if args.is_empty() { return Err("docker.remove requiere (id)".into()); }
            let id    = to_str(&args[0]);
            let force = args.get(1).map(|v| v.is_truthy()).unwrap_or(false);
            let path  = if force {
                format!("/containers/{}?force=1", id)
            } else {
                format!("/containers/{}", id)
            };
            delete_req(&path)?;
            let mut m = HashMap::new();
            m.insert("ok".into(), EvalValue::Bool(true));
            Ok(EvalValue::Dict(m))
        }

        // docker.logs(id, tail?) → string
        "logs" => {
            if args.is_empty() { return Err("docker.logs requiere (id)".into()); }
            let id   = to_str(&args[0]);
            let tail = args.get(1).and_then(|v| v.to_i64().ok()).unwrap_or(100);
            let url  = api(&format!(
                "/containers/{}/logs?stdout=1&stderr=1&tail={}",
                id, tail
            ));
            let text = ureq::get(&url)
                .call()
                .map_err(|e| format!("docker.logs error: {}", e))?
                .into_string()
                .map_err(|e| format!("docker.logs lectura: {}", e))?;
            // Docker multiplexed stream tiene 8 bytes de header por frame; limpiar
            let clean = strip_docker_logs(&text);
            Ok(EvalValue::Str(clean))
        }

        // docker.inspect(id) → dict completo
        "inspect" => {
            if args.is_empty() { return Err("docker.inspect requiere (id)".into()); }
            let id   = to_str(&args[0]);
            let json = get_json(&format!("/containers/{}/json", id))?;
            Ok(json_to_eval(&json))
        }

        // docker.run(image, opts?) → {id, name}
        // opts: {name, env, ports, cmd}
        "run" => {
            if args.is_empty() { return Err("docker.run requiere (image, opts?)".into()); }
            let image = to_str(&args[0]);
            let mut body = serde_json::json!({ "Image": image });
            if let Some(EvalValue::Dict(opts)) = args.get(1) {
                if let Some(EvalValue::Str(name)) = opts.get("name") {
                    body["Name"] = serde_json::json!(name);
                }
                if let Some(EvalValue::List(env)) = opts.get("env") {
                    let ev: Vec<serde_json::Value> = env.iter()
                        .map(|e| serde_json::json!(e.to_string()))
                        .collect();
                    body["Env"] = serde_json::json!(ev);
                }
                if let Some(EvalValue::List(cmd)) = opts.get("cmd") {
                    let cv: Vec<serde_json::Value> = cmd.iter()
                        .map(|c| serde_json::json!(c.to_string()))
                        .collect();
                    body["Cmd"] = serde_json::json!(cv);
                }
            }
            // Crear
            let name_query = if let Some(serde_json::Value::String(n)) = body.get("Name") {
                format!("?name={}", n)
            } else { String::new() };
            let created = post_json(&format!("/containers/create{}", name_query), &body)?;
            let cid = created["Id"].as_str().unwrap_or("").to_string();
            // Arrancar
            post_empty(&format!("/containers/{}/start", cid))?;
            let mut m = HashMap::new();
            m.insert("id".into(),   EvalValue::Str(cid.chars().take(12).collect()));
            m.insert("name".into(), EvalValue::Str(
                created["Name"].as_str().unwrap_or("").trim_start_matches('/').to_string()
            ));
            Ok(EvalValue::Dict(m))
        }

        // docker.images() → [{id, tags, size, created}]
        "images" => {
            let json = get_json("/images/json")?;
            let list = json.as_array()
                .ok_or("docker.images: respuesta inesperada")?
                .iter()
                .map(image_summary)
                .collect();
            Ok(EvalValue::List(list))
        }

        // docker.pull(image) → {ok}
        "pull" => {
            if args.is_empty() { return Err("docker.pull requiere (image)".into()); }
            let image = to_str(&args[0]);
            let url   = api(&format!("/images/create?fromImage={}", urlenc(&image)));
            ureq::post(&url)
                .call()
                .map_err(|e| format!("docker.pull error: {}", e))?;
            let mut m = HashMap::new();
            m.insert("ok".into(), EvalValue::Bool(true));
            Ok(EvalValue::Dict(m))
        }

        // docker.stats(id) → {cpu_pct, mem_usage, mem_limit}
        "stats" => {
            if args.is_empty() { return Err("docker.stats requiere (id)".into()); }
            let id   = to_str(&args[0]);
            let json = get_json(&format!("/containers/{}/stats?stream=false", id))?;
            let cpu_delta = json["cpu_stats"]["cpu_usage"]["total_usage"].as_f64().unwrap_or(0.0)
                - json["precpu_stats"]["cpu_usage"]["total_usage"].as_f64().unwrap_or(0.0);
            let sys_delta = json["cpu_stats"]["system_cpu_usage"].as_f64().unwrap_or(1.0)
                - json["precpu_stats"]["system_cpu_usage"].as_f64().unwrap_or(0.0);
            let ncpu = json["cpu_stats"]["online_cpus"].as_f64().unwrap_or(1.0);
            let cpu_pct = if sys_delta > 0.0 {
                (cpu_delta / sys_delta) * ncpu * 100.0
            } else { 0.0 };
            let mem_usage = json["memory_stats"]["usage"].as_i64().unwrap_or(0);
            let mem_limit = json["memory_stats"]["limit"].as_i64().unwrap_or(0);
            let mut m = HashMap::new();
            m.insert("cpu_pct".into(),   EvalValue::Float(
                (cpu_pct * 100.0).round() / 100.0
            ));
            m.insert("mem_usage".into(), EvalValue::Int(mem_usage));
            m.insert("mem_limit".into(), EvalValue::Int(mem_limit));
            Ok(EvalValue::Dict(m))
        }

        // docker.exec(id, cmd, opts?) → {out, exit_code}
        // Ejecuta un comando dentro de un contenedor en ejecución.
        // opts: {user, workdir, env}
        "exec" => {
            if args.len() < 2 {
                return Err("docker.exec requiere (id, cmd, opts?)".into());
            }
            let id  = to_str(&args[0]);
            // cmd puede ser string "ls -la" o lista ["ls", "-la"]
            let cmd_val: Vec<serde_json::Value> = match &args[1] {
                EvalValue::List(parts) => parts.iter()
                    .map(|p| serde_json::json!(to_str(p)))
                    .collect(),
                other => {
                    // Dividir el string por espacios como shell simple
                    to_str(other).split_whitespace()
                        .map(|s| serde_json::json!(s))
                        .collect()
                }
            };
            let mut exec_body = serde_json::json!({
                "Cmd": cmd_val,
                "AttachStdout": true,
                "AttachStderr": true,
            });
            if let Some(EvalValue::Dict(opts)) = args.get(2) {
                if let Some(EvalValue::Str(user)) = opts.get("user") {
                    exec_body["User"] = serde_json::json!(user);
                }
                if let Some(EvalValue::Str(workdir)) = opts.get("workdir") {
                    exec_body["WorkingDir"] = serde_json::json!(workdir);
                }
                if let Some(EvalValue::List(env)) = opts.get("env") {
                    let ev: Vec<serde_json::Value> = env.iter()
                        .map(|e| serde_json::json!(to_str(e)))
                        .collect();
                    exec_body["Env"] = serde_json::json!(ev);
                }
            }
            // Crear el exec
            let created = post_json(&format!("/containers/{}/exec", id), &exec_body)?;
            let exec_id = created["Id"].as_str()
                .ok_or_else(|| "docker.exec: respuesta sin Id".to_string())?
                .to_string();
            // Arrancar el exec y capturar output
            let start_body = serde_json::json!({ "Detach": false });
            let output = {
                let body_str = serde_json::to_string(&start_body).unwrap_or_default();
                let resp = ureq::post(&api(&format!("/exec/{}/start", exec_id)))
                    .set("Content-Type", "application/json")
                    .send_string(&body_str)
                    .map_err(|e| format!("docker.exec start error: {}", e))?;
                resp.into_string()
                    .map_err(|e| format!("docker.exec lectura output: {}", e))?
            };
            // Obtener el código de salida
            let inspect = get_json(&format!("/exec/{}/json", exec_id))?;
            let exit_code = inspect["ExitCode"].as_i64().unwrap_or(-1);
            let mut m = HashMap::new();
            m.insert("out".into(),       EvalValue::Str(strip_docker_logs(&output)));
            m.insert("exit_code".into(), EvalValue::Int(exit_code));
            Ok(EvalValue::Dict(m))
        }

        // docker.network_list() → [{id, name, driver, scope}]
        "network_list" => {
            let json = get_json("/networks")?;
            let list = json.as_array()
                .ok_or("docker.network_list: respuesta inesperada")?
                .iter()
                .map(|v| {
                    let mut m = HashMap::new();
                    m.insert("id".into(),     EvalValue::Str(
                        v["Id"].as_str().unwrap_or("").chars().take(12).collect()
                    ));
                    m.insert("name".into(),   EvalValue::Str(
                        v["Name"].as_str().unwrap_or("").to_string()
                    ));
                    m.insert("driver".into(), EvalValue::Str(
                        v["Driver"].as_str().unwrap_or("").to_string()
                    ));
                    m.insert("scope".into(),  EvalValue::Str(
                        v["Scope"].as_str().unwrap_or("").to_string()
                    ));
                    EvalValue::Dict(m)
                })
                .collect();
            Ok(EvalValue::List(list))
        }

        // docker.ping() → bool
        "ping" => {
            match ureq::get(&api("/_ping")).call() {
                Ok(_)  => Ok(EvalValue::Bool(true)),
                Err(_) => Ok(EvalValue::Bool(false)),
            }
        }

        // docker.version() → {version, api_version, os, arch}
        "version" => {
            let json = get_json("/version")?;
            let mut m = HashMap::new();
            m.insert("version".into(),     EvalValue::Str(
                json["Version"].as_str().unwrap_or("").to_string()
            ));
            m.insert("api_version".into(), EvalValue::Str(
                json["ApiVersion"].as_str().unwrap_or("").to_string()
            ));
            m.insert("os".into(),   EvalValue::Str(json["Os"].as_str().unwrap_or("").to_string()));
            m.insert("arch".into(), EvalValue::Str(json["Arch"].as_str().unwrap_or("").to_string()));
            Ok(EvalValue::Dict(m))
        }

        f => Err(format!(
            "docker.{}() no existe. Funciones disponibles: config, containers, start, stop, \
             restart, kill, remove, logs, inspect, run, exec, images, pull, stats, \
             ping, version, network_list",
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
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | ':' => c.to_string(),
            c => format!("%{:02X}", c as u32),
        })
        .collect()
}

// Docker logs vienen en formato multiplexado con header de 8 bytes por frame
fn strip_docker_logs(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i + 8 <= bytes.len() {
        // stream_type (1 byte) + 3 padding + 4 bytes de longitud big-endian
        let len = u32::from_be_bytes([bytes[i+4], bytes[i+5], bytes[i+6], bytes[i+7]]) as usize;
        i += 8;
        if i + len <= bytes.len() {
            if let Ok(s) = std::str::from_utf8(&bytes[i..i+len]) {
                out.push_str(s);
            }
            i += len;
        } else {
            // Fallback: el resto como texto plano
            if let Ok(s) = std::str::from_utf8(&bytes[i..]) {
                out.push_str(s);
            }
            break;
        }
    }
    if out.is_empty() { raw.to_string() } else { out }
}
