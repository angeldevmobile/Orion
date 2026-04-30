/// Módulo AI del VM Rust — implementa think / learn / sense.
/// Usa ureq (ya en Cargo.toml) para llamar a Anthropic o OpenAI.
/// Lee API keys desde variables de entorno y/o archivo .env.

use std::collections::HashMap;
use std::sync::Mutex;

// ─── Memoria de sesión (persistente durante la ejecución del programa) ───────

static SESSION_MEMORY: Mutex<Vec<String>> = Mutex::new(Vec::new());

// ─── Carga de .env ────────────────────────────────────────────────────────────

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

// ─── HTTP helper ──────────────────────────────────────────────────────────────

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

// ─── Llamadas por proveedor ───────────────────────────────────────────────────

fn call_anthropic(
    env: &HashMap<String, String>,
    prompt: &str,
    system: Option<&str>,
    max_tokens: u32,
) -> Result<String, String> {
    let key = env
        .get("ANTHROPIC_API_KEY")
        .ok_or("ANTHROPIC_API_KEY no configurada — agrégala en tu .env")?;
    let model = env
        .get("ANTHROPIC_MODEL")
        .map(|s| s.as_str())
        .unwrap_or("claude-haiku-4-5-20251001");

    let mut body = serde_json::json!({
        "model":      model,
        "max_tokens": max_tokens,
        "messages":   [{"role": "user", "content": prompt}]
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
    prompt: &str,
    system: Option<&str>,
    max_tokens: u32,
) -> Result<String, String> {
    let key = env
        .get("OPENAI_API_KEY")
        .ok_or("OPENAI_API_KEY no configurada — agrégala en tu .env")?;
    let model = env
        .get("OPENAI_MODEL")
        .map(|s| s.as_str())
        .unwrap_or("gpt-4o-mini");

    let mut messages: Vec<serde_json::Value> = Vec::new();
    if let Some(sys) = system {
        messages.push(serde_json::json!({"role": "system", "content": sys}));
    }
    messages.push(serde_json::json!({"role": "user", "content": prompt}));

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

/// Selecciona proveedor y hace la llamada al modelo.
fn ai_call(prompt: &str, system: Option<&str>, max_tokens: u32) -> Result<String, String> {
    let env = load_env_vars();

    let has_anthropic = env.contains_key("ANTHROPIC_API_KEY");
    let has_openai    = env.contains_key("OPENAI_API_KEY");
    let pref          = env.get("AI_MODEL").map(|s| s.to_lowercase());

    let use_anthropic = match pref.as_deref() {
        Some("openai") if has_openai    => false,
        Some("claude") if has_anthropic => true,
        _                               => has_anthropic,
    };

    if use_anthropic {
        call_anthropic(&env, prompt, system, max_tokens)
    } else if has_openai {
        call_openai(&env, prompt, system, max_tokens)
    } else {
        Err(
            "No hay API key de AI configurada.\n\
             Agrega en tu .env:\n\
               ANTHROPIC_API_KEY=sk-ant-...\n\
             o\n\
               OPENAI_API_KEY=sk-..."
            .into(),
        )
    }
}

// ─── API pública (usada desde eval.rs) ───────────────────────────────────────

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
