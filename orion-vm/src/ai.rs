/// Módulo AI del VM Rust — implementa think / learn / sense.
/// Usa ureq (ya en Cargo.toml) para llamar a Anthropic o OpenAI.
/// Lee API keys desde variables de entorno y/o archivo .env.

use indexmap::IndexMap as HashMap;
use std::sync::Mutex;

//     Memoria de sesión (persistente durante la ejecución del programa)        

static SESSION_MEMORY: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Modelo fijado con `ai.set_model(...)`; tiene prioridad sobre .env.
static MODEL_OVERRIDE: Mutex<Option<String>> = Mutex::new(None);

pub fn set_model_override(name: Option<String>) {
    *MODEL_OVERRIDE.lock().unwrap() = name;
}

//     Carga de .env                                                             

fn load_env_vars() -> HashMap<String, String> {
    // Partir de las variables de entorno del proceso
    let mut vars: HashMap<String, String> = std::env::vars().collect();

    // Buscar .env desde el directorio actual hasta 3 niveles arriba
    let mut path = std::env::current_dir().unwrap_or_default();
    for _ in 0..4 {
        let env_file = path.join(".env");
        if let Ok(content) = std::fs::read_to_string(&env_file) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some(eq) = line.find('=') {
                    let key = line[..eq].trim().to_string();
                    let val = line[eq + 1..]
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                    if !key.is_empty() && !vars.contains_key(&key) {
                        vars.insert(key, val);
                    }
                }
            }
            break;
        }
        if !path.pop() {
            break;
        }
    }
    vars
}

//     HTTP helper                                                               

fn http_post(
    url: &str,
    headers: &[(&str, &str)],
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut req = ureq::post(url);
    for (k, v) in headers {
        req = req.set(k, v);
    }
    let resp = req.send_json(body).map_err(|e| match e {
        ureq::Error::Status(code, r) => {
            let raw = r.into_string().unwrap_or_default();
            let detail = serde_json::from_str::<serde_json::Value>(&raw)
                .ok()
                .and_then(|j| j["error"]["message"].as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| raw.chars().take(300).collect());
            format!("API error ({}): {}", code, detail)
        }
        other => format!("Error de red: {}", other),
    })?;

    resp.into_json::<serde_json::Value>()
        .map_err(|e| format!("Error al parsear respuesta JSON: {}", e))
}

//     Llamadas por proveedor                                                    

/// Modelo efectivo para un proveedor: override de set_model > .env > default.
/// Los defaults son ALIAS sin fecha (el proveedor los apunta al snapshot
/// vigente); quien necesite reproducibilidad pinnea la versión datada en .env.
pub fn model_for(env: &HashMap<String, String>, provider: &str) -> String {
    if let Some(m) = MODEL_OVERRIDE.lock().unwrap().clone() {
        return m;
    }
    match provider {
        "anthropic" => env.get("ANTHROPIC_MODEL").cloned()
            .unwrap_or_else(|| "claude-haiku-4-5".into()),
        "openai" => env.get("OPENAI_MODEL").cloned()
            .unwrap_or_else(|| "gpt-4o-mini".into()),
        _ => "unknown".into(),
    }
}

fn call_anthropic(
    env: &HashMap<String, String>,
    messages: &[serde_json::Value],
    system: Option<&str>,
    max_tokens: u32,
) -> Result<String, String> {
    let key = env
        .get("ANTHROPIC_API_KEY")
        .ok_or("ANTHROPIC_API_KEY no configurada — agrégala en tu .env")?;
    let model = model_for(env, "anthropic");

    let mut body = serde_json::json!({
        "model":      model,
        "max_tokens": max_tokens,
        "messages":   messages
    });
    if let Some(sys) = system {
        body["system"] = serde_json::Value::String(sys.to_string());
    }

    let result = http_post(
        "https://api.anthropic.com/v1/messages",
        &[
            ("Content-Type", "application/json"),
            ("x-api-key", key),
            ("anthropic-version", "2023-06-01"),
        ],
        body,
    )?;

    result["content"][0]["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Respuesta inesperada de Anthropic: {}", result))
}

fn call_openai(
    env: &HashMap<String, String>,
    history: &[serde_json::Value],
    system: Option<&str>,
    max_tokens: u32,
) -> Result<String, String> {
    let key = env
        .get("OPENAI_API_KEY")
        .ok_or("OPENAI_API_KEY no configurada — agrégala en tu .env")?;
    let model = model_for(env, "openai");

    let mut messages: Vec<serde_json::Value> = Vec::new();
    if let Some(sys) = system {
        messages.push(serde_json::json!({"role": "system", "content": sys}));
    }
    messages.extend_from_slice(history);

    let result = http_post(
        "https://api.openai.com/v1/chat/completions",
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {}", key)),
        ],
        serde_json::json!({
            "model":      model,
            "max_tokens": max_tokens,
            "messages":   messages
        }),
    )?;

    result["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Respuesta inesperada de OpenAI: {}", result))
}

/// Proveedor efectivo según keys presentes y preferencia AI_MODEL:
/// Some("anthropic") | Some("openai") | None.
pub fn detect_provider(env: &HashMap<String, String>) -> Option<&'static str> {
    let has_anthropic = env.contains_key("ANTHROPIC_API_KEY");
    let has_openai    = env.contains_key("OPENAI_API_KEY");
    let pref          = env.get("AI_MODEL").map(|s| s.to_lowercase());

    let use_anthropic = match pref.as_deref() {
        Some("openai") if has_openai    => false,
        Some("claude") if has_anthropic => true,
        _                               => has_anthropic,
    };

    if use_anthropic { Some("anthropic") }
    else if has_openai { Some("openai") }
    else { None }
}

/// Entorno efectivo (proceso + .env). Expuesto para ai_mod (status/provider).
pub fn env_vars() -> HashMap<String, String> {
    load_env_vars()
}

/// Selecciona proveedor y hace la llamada al modelo (un solo turno de usuario).
pub fn ai_call(prompt: &str, system: Option<&str>, max_tokens: u32) -> Result<String, String> {
    let messages = vec![serde_json::json!({"role": "user", "content": prompt})];
    ai_call_chat(&messages, system, max_tokens)
}

/// Igual que `ai_call` pero con historial de mensajes (chat multi-turno).
pub fn ai_call_chat(
    messages: &[serde_json::Value],
    system: Option<&str>,
    max_tokens: u32,
) -> Result<String, String> {
    let env = load_env_vars();
    match detect_provider(&env) {
        Some("anthropic") => call_anthropic(&env, messages, system, max_tokens),
        Some("openai")    => call_openai(&env, messages, system, max_tokens),
        _ => Err(
            "No hay API key de AI configurada.\n\
             Agrega en tu .env:\n\
               ANTHROPIC_API_KEY=sk-ant-...\n\
             o\n\
               OPENAI_API_KEY=sk-..."
            .into(),
        ),
    }
}

//     API pública (usada desde eval.rs)                                        

/// `think <expr>` — pregunta al modelo y muestra la respuesta.
pub fn think(prompt: &str) -> Result<String, String> {
    ai_call(prompt, None, 1024)
}

/// `learn <expr>` — guarda texto en la memoria de sesión.
pub fn learn(text: &str) -> String {
    let mut mem = SESSION_MEMORY.lock().unwrap();
    mem.push(text.to_string());
    format!("[aprendido: {} {} en memoria]", mem.len(),
            if mem.len() == 1 { "entrada" } else { "entradas" })
}

/// Retorna el número de entradas en memoria de sesión.
pub fn memory_size() -> usize {
    SESSION_MEMORY.lock().unwrap().len()
}

pub fn memory_clear() {
    SESSION_MEMORY.lock().unwrap().clear();
}

/// `sense <expr>` — consulta la memoria de sesión con ayuda del modelo.
pub fn sense(query: &str) -> Result<String, String> {
    let context = {
        let mem = SESSION_MEMORY.lock().unwrap();
        if mem.is_empty() {
            return Ok("[sense: memoria vacía — usa 'learn' primero]".into());
        }
        mem.join("\n---\n")
    };

    ai_call(
        query,
        Some(&format!(
            "Responde usando ÚNICAMENTE la siguiente información almacenada:\n\n\
             {}\n\n\
             Si la respuesta no está en la información, dilo claramente.",
            context
        )),
        512,
    )
}
