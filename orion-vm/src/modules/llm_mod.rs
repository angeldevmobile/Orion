use crate::eval_value::EvalValue;
use std::collections::HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // llm.query(model, prompt) → string
        "query" => {
            if args.len() < 2 { return Err("llm.query requiere (model, prompt)".into()); }
            let model  = to_str(&args[0]);
            let prompt = to_str(&args[1]);
            Ok(EvalValue::Str(llm_ask(&model, &prompt, "")?))
        }
        // llm.query_with(model, prompt, system) → string
        "query_with" => {
            if args.len() < 3 { return Err("llm.query_with requiere (model, prompt, system)".into()); }
            let model  = to_str(&args[0]);
            let prompt = to_str(&args[1]);
            let system = to_str(&args[2]);
            Ok(EvalValue::Str(llm_ask(&model, &prompt, &system)?))
        }
        // llm.chat(model, messages) → string  (messages: [{role, content}, ...])
        "chat" => {
            if args.len() < 2 { return Err("llm.chat requiere (model, messages)".into()); }
            let model = to_str(&args[0]);
            let msgs  = match &args[1] {
                EvalValue::List(v) => v.clone(),
                _ => return Err("llm.chat: messages debe ser una lista de dicts {role, content}".into()),
            };
            Ok(EvalValue::Str(llm_chat(&model, msgs)?))
        }
        // llm.embed(model, text) → List<float>
        "embed" => {
            if args.len() < 2 { return Err("llm.embed requiere (model, text)".into()); }
            let model = to_str(&args[0]);
            let text  = to_str(&args[1]);
            let emb   = get_embedding_for_model(&model, &text)?;
            Ok(EvalValue::List(emb.into_iter().map(EvalValue::Float).collect()))
        }
        // llm.models() → List<string>  (consulta las APIs reales; solo Anthropic usa fallback estático)
        "models" => {
            let env = load_env();
            let mut list = Vec::new();

            // Anthropic: no tiene endpoint público de modelos → fallback mínimo
            if env.contains_key("ANTHROPIC_API_KEY") {
                for m in &["claude-opus-4-7", "claude-sonnet-4-6", "claude-haiku-4-5-20251001"] {
                    list.push(EvalValue::Str(m.to_string()));
                }
            }

            // OpenAI: GET /v1/models → lista real y siempre actualizada
            if let Some(key) = env.get("OPENAI_API_KEY") {
                if let Ok(resp) = ureq::get("https://api.openai.com/v1/models")
                    .set("Authorization", &format!("Bearer {}", key))
                    .call()
                {
                    if let Ok(json) = resp.into_json::<serde_json::Value>() {
                        if let Some(data) = json["data"].as_array() {
                            let mut ids: Vec<String> = data.iter()
                                .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                                .filter(|id| id.starts_with("gpt") || id.starts_with("o1") || id.starts_with("o3") || id.starts_with("o4") || id.starts_with("text-embedding"))
                                .collect();
                            ids.sort();
                            for id in ids { list.push(EvalValue::Str(id)); }
                        }
                    }
                }
            }

            // Gemini: GET /v1beta/models → lista real
            if let Some(key) = env.get("GEMINI_API_KEY").or_else(|| env.get("GOOGLE_API_KEY")) {
                let url = format!("https://generativelanguage.googleapis.com/v1beta/models?key={}", key);
                if let Ok(resp) = ureq::get(&url).call() {
                    if let Ok(json) = resp.into_json::<serde_json::Value>() {
                        if let Some(models) = json["models"].as_array() {
                            for m in models {
                                if let Some(name) = m["name"].as_str() {
                                    // "models/gemini-2.0-flash" → "gemini-2.0-flash"
                                    let short = name.strip_prefix("models/").unwrap_or(name);
                                    list.push(EvalValue::Str(short.to_string()));
                                }
                            }
                        }
                    }
                }
            }

            // Ollama: GET /api/tags → modelos instalados localmente
            let ollama_url = ollama_base(&env);
            if let Ok(resp) = ureq::get(&format!("{}/api/tags", ollama_url)).call() {
                if let Ok(json) = resp.into_json::<serde_json::Value>() {
                    if let Some(models) = json["models"].as_array() {
                        for m in models {
                            if let Some(name) = m["name"].as_str() {
                                list.push(EvalValue::Str(format!("ollama:{}", name)));
                            }
                        }
                    }
                }
            }

            Ok(EvalValue::List(list))
        }
        // llm.providers() → List<string>
        "providers" => {
            let env = load_env();
            let mut list = Vec::new();
            if env.contains_key("ANTHROPIC_API_KEY") { list.push(EvalValue::Str("anthropic".into())); }
            if env.contains_key("OPENAI_API_KEY")    { list.push(EvalValue::Str("openai".into())); }
            if env.contains_key("GEMINI_API_KEY") || env.contains_key("GOOGLE_API_KEY") {
                list.push(EvalValue::Str("gemini".into()));
            }
            let ollama_url = ollama_base(&env);
            if ureq::get(&format!("{}/api/tags", ollama_url)).call().is_ok() {
                list.push(EvalValue::Str("ollama".into()));
            }
            if list.is_empty() { list.push(EvalValue::Str("none".into())); }
            Ok(EvalValue::List(list))
        }
        f => Err(format!("llm.{}() no existe", f)),
    }
}

//   Routing principal                              

fn llm_ask(model: &str, prompt: &str, system: &str) -> Result<String, String> {
    let env = load_env();
    match detect_provider(model) {
        "anthropic" => call_anthropic(&env, model, prompt, system, 2048),
        "openai"    => call_openai(&env, model, prompt, system, 2048),
        "gemini"    => call_gemini(&env, model, prompt, system, 2048),
        "ollama"    => call_ollama(&env, strip_ollama(model), prompt, system),
        _ => {
            if env.contains_key("ANTHROPIC_API_KEY") {
                return call_anthropic(&env, &default_model(&env, "anthropic", "chat"), prompt, system, 2048);
            }
            if env.contains_key("OPENAI_API_KEY") {
                return call_openai(&env, &default_model(&env, "openai", "chat"), prompt, system, 2048);
            }
            if env.contains_key("GEMINI_API_KEY") || env.contains_key("GOOGLE_API_KEY") {
                return call_gemini(&env, &default_model(&env, "gemini", "chat"), prompt, system, 2048);
            }
            let fallback = if model == "auto" { default_model(&env, "ollama", "chat") } else { model.to_string() };
            call_ollama(&env, &fallback, prompt, system)
                .map_err(|e| format!("llm: no hay API key configurada ni Ollama disponible. Configura ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY o inicia Ollama. ({})", e))
        }
    }
}

fn llm_chat(model: &str, msgs: Vec<EvalValue>) -> Result<String, String> {
    let env = load_env();
    let messages: Vec<serde_json::Value> = msgs.iter().map(|msg| {
        match msg {
            EvalValue::Dict(d) => {
                let role    = d.get("role").map(|v| to_str(v)).unwrap_or_else(|| "user".into());
                let content = d.get("content").map(|v| to_str(v)).unwrap_or_default();
                serde_json::json!({"role": role, "content": content})
            }
            other => serde_json::json!({"role": "user", "content": format!("{}", other)}),
        }
    }).collect();

    match detect_provider(model) {
        "anthropic" => chat_anthropic(&env, model, messages, 2048),
        "openai"    => chat_openai(&env, model, messages, 2048),
        "gemini"    => chat_gemini(&env, model, messages, 2048),
        "ollama"    => chat_ollama(&env, strip_ollama(model), messages),
        _ => {
            if env.contains_key("ANTHROPIC_API_KEY") {
                return chat_anthropic(&env, &default_model(&env, "anthropic", "chat"), messages, 2048);
            }
            if env.contains_key("OPENAI_API_KEY") {
                return chat_openai(&env, &default_model(&env, "openai", "chat"), messages, 2048);
            }
            if env.contains_key("GEMINI_API_KEY") || env.contains_key("GOOGLE_API_KEY") {
                return chat_gemini(&env, &default_model(&env, "gemini", "chat"), messages, 2048);
            }
            Err("llm.chat: no hay API key configurada. Agrega ANTHROPIC_API_KEY, OPENAI_API_KEY o GEMINI_API_KEY".into())
        }
    }
}

pub fn get_embedding_for_model(model: &str, text: &str) -> Result<Vec<f64>, String> {
    let env = load_env();
    match detect_provider(model) {
        "openai"    => embed_openai(&env, model, text),
        "gemini"    => embed_gemini(&env, model, text),
        "ollama"    => embed_ollama(&env, strip_ollama(model), text),
        "anthropic" => Err("llm.embed: Anthropic no ofrece API de embeddings. Usa text-embedding-3-small (OpenAI) o nomic-embed-text (Ollama)".into()),
        _ => {
            if env.contains_key("OPENAI_API_KEY") { return embed_openai(&env, &default_model(&env, "openai", "embed"), text); }
            if env.contains_key("GEMINI_API_KEY") || env.contains_key("GOOGLE_API_KEY") { return embed_gemini(&env, &default_model(&env, "gemini", "embed"), text); }
            let fallback = if model == "auto" { default_model(&env, "ollama", "embed") } else { model.to_string() };
            embed_ollama(&env, &fallback, text)
                .map_err(|e| format!("llm.embed: configura OPENAI_API_KEY, GEMINI_API_KEY o inicia Ollama con {}. ({})", default_model(&env, "ollama", "embed"), e))
        }
    }
}

//   Anthropic                                 ─

fn call_anthropic(env: &HashMap<String, String>, model: &str, prompt: &str, system: &str, max_tokens: u32) -> Result<String, String> {
    let key = env.get("ANTHROPIC_API_KEY").ok_or("ANTHROPIC_API_KEY no configurada")?;
    let mut body = serde_json::json!({
        "model": model, "max_tokens": max_tokens,
        "messages": [{"role": "user", "content": prompt}]
    });
    if !system.is_empty() { body["system"] = serde_json::Value::String(system.to_string()); }
    let resp = ureq::post("https://api.anthropic.com/v1/messages")
        .set("Content-Type", "application/json")
        .set("x-api-key", key)
        .set("anthropic-version", "2023-06-01")
        .send_json(body)
        .map_err(|e| format!("llm[anthropic]: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    json["content"][0]["text"].as_str().map(|s| s.to_string())
        .ok_or_else(|| format!("llm[anthropic]: respuesta inesperada: {}", json))
}

fn chat_anthropic(env: &HashMap<String, String>, model: &str, messages: Vec<serde_json::Value>, max_tokens: u32) -> Result<String, String> {
    let key = env.get("ANTHROPIC_API_KEY").ok_or("ANTHROPIC_API_KEY no configurada")?;
    let body = serde_json::json!({"model": model, "max_tokens": max_tokens, "messages": messages});
    let resp = ureq::post("https://api.anthropic.com/v1/messages")
        .set("Content-Type", "application/json")
        .set("x-api-key", key)
        .set("anthropic-version", "2023-06-01")
        .send_json(body)
        .map_err(|e| format!("llm[anthropic] chat: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    json["content"][0]["text"].as_str().map(|s| s.to_string())
        .ok_or_else(|| "llm[anthropic]: respuesta inesperada".into())
}

//   OpenAI                                   

fn call_openai(env: &HashMap<String, String>, model: &str, prompt: &str, system: &str, max_tokens: u32) -> Result<String, String> {
    let key = env.get("OPENAI_API_KEY").ok_or("OPENAI_API_KEY no configurada")?;
    let mut messages = Vec::new();
    if !system.is_empty() { messages.push(serde_json::json!({"role": "system", "content": system})); }
    messages.push(serde_json::json!({"role": "user", "content": prompt}));
    let body = serde_json::json!({"model": model, "max_tokens": max_tokens, "messages": messages});
    let resp = ureq::post("https://api.openai.com/v1/chat/completions")
        .set("Content-Type", "application/json")
        .set("Authorization", &format!("Bearer {}", key))
        .send_json(body)
        .map_err(|e| format!("llm[openai]: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    json["choices"][0]["message"]["content"].as_str().map(|s| s.to_string())
        .ok_or_else(|| format!("llm[openai]: respuesta inesperada: {}", json))
}

fn chat_openai(env: &HashMap<String, String>, model: &str, messages: Vec<serde_json::Value>, max_tokens: u32) -> Result<String, String> {
    let key = env.get("OPENAI_API_KEY").ok_or("OPENAI_API_KEY no configurada")?;
    let body = serde_json::json!({"model": model, "max_tokens": max_tokens, "messages": messages});
    let resp = ureq::post("https://api.openai.com/v1/chat/completions")
        .set("Content-Type", "application/json")
        .set("Authorization", &format!("Bearer {}", key))
        .send_json(body)
        .map_err(|e| format!("llm[openai] chat: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    json["choices"][0]["message"]["content"].as_str().map(|s| s.to_string())
        .ok_or_else(|| "llm[openai]: respuesta inesperada".into())
}

fn embed_openai(env: &HashMap<String, String>, model: &str, text: &str) -> Result<Vec<f64>, String> {
    let key = env.get("OPENAI_API_KEY").ok_or("OPENAI_API_KEY no configurada")?;
    let body = serde_json::json!({"model": model, "input": text});
    let resp = ureq::post("https://api.openai.com/v1/embeddings")
        .set("Content-Type", "application/json")
        .set("Authorization", &format!("Bearer {}", key))
        .send_json(body)
        .map_err(|e| format!("llm.embed[openai]: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let arr = json["data"][0]["embedding"].as_array()
        .ok_or_else(|| format!("llm.embed[openai]: respuesta inesperada: {}", json))?;
    Ok(arr.iter().filter_map(|v| v.as_f64()).collect())
}

//   Gemini                                   

fn call_gemini(env: &HashMap<String, String>, model: &str, prompt: &str, system: &str, max_tokens: u32) -> Result<String, String> {
    let key = env.get("GEMINI_API_KEY").or_else(|| env.get("GOOGLE_API_KEY"))
        .ok_or("GEMINI_API_KEY o GOOGLE_API_KEY no configurada")?;
    let mut contents: Vec<serde_json::Value> = Vec::new();
    if !system.is_empty() {
        contents.push(serde_json::json!({"role": "user",  "parts": [{"text": system}]}));
        contents.push(serde_json::json!({"role": "model", "parts": [{"text": "Entendido."}]}));
    }
    contents.push(serde_json::json!({"role": "user", "parts": [{"text": prompt}]}));
    let body = serde_json::json!({
        "contents": contents,
        "generationConfig": {"maxOutputTokens": max_tokens}
    });
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}", model, key);
    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| format!("llm[gemini]: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    json["candidates"][0]["content"]["parts"][0]["text"].as_str().map(|s| s.to_string())
        .ok_or_else(|| format!("llm[gemini]: respuesta inesperada: {}", json))
}

fn chat_gemini(env: &HashMap<String, String>, model: &str, messages: Vec<serde_json::Value>, max_tokens: u32) -> Result<String, String> {
    let key = env.get("GEMINI_API_KEY").or_else(|| env.get("GOOGLE_API_KEY"))
        .ok_or("GEMINI_API_KEY no configurada")?;
    let contents: Vec<serde_json::Value> = messages.iter().map(|m| {
        let role    = m["role"].as_str().unwrap_or("user");
        let content = m["content"].as_str().unwrap_or("");
        let grole   = if role == "assistant" { "model" } else { "user" };
        serde_json::json!({"role": grole, "parts": [{"text": content}]})
    }).collect();
    let body = serde_json::json!({
        "contents": contents,
        "generationConfig": {"maxOutputTokens": max_tokens}
    });
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}", model, key);
    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| format!("llm[gemini] chat: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    json["candidates"][0]["content"]["parts"][0]["text"].as_str().map(|s| s.to_string())
        .ok_or_else(|| "llm[gemini]: respuesta inesperada".into())
}

fn embed_gemini(env: &HashMap<String, String>, model: &str, text: &str) -> Result<Vec<f64>, String> {
    let key = env.get("GEMINI_API_KEY").or_else(|| env.get("GOOGLE_API_KEY"))
        .ok_or("GEMINI_API_KEY no configurada")?;
    let embed_model = if model.starts_with("models/") { model.to_string() } else { format!("models/{}", model) };
    let body = serde_json::json!({"model": embed_model, "content": {"parts": [{"text": text}]}});
    let url  = format!("https://generativelanguage.googleapis.com/v1beta/{}:embedContent?key={}", embed_model, key);
    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| format!("llm.embed[gemini]: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let arr = json["embedding"]["values"].as_array()
        .ok_or_else(|| format!("llm.embed[gemini]: respuesta inesperada: {}", json))?;
    Ok(arr.iter().filter_map(|v| v.as_f64()).collect())
}

//   Ollama (local)                               

fn call_ollama(env: &HashMap<String, String>, model: &str, prompt: &str, system: &str) -> Result<String, String> {
    let base = ollama_base(env);
    let mut body = serde_json::json!({"model": model, "prompt": prompt, "stream": false});
    if !system.is_empty() { body["system"] = serde_json::Value::String(system.to_string()); }
    let resp = ureq::post(&format!("{}/api/generate", base))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| format!("llm[ollama]: {}. ¿Está Ollama corriendo en {}?", e, base))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    json["response"].as_str().map(|s| s.to_string())
        .ok_or_else(|| format!("llm[ollama]: respuesta inesperada: {}", json))
}

fn chat_ollama(env: &HashMap<String, String>, model: &str, messages: Vec<serde_json::Value>) -> Result<String, String> {
    let base = ollama_base(env);
    let body = serde_json::json!({"model": model, "messages": messages, "stream": false});
    let resp = ureq::post(&format!("{}/api/chat", base))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| format!("llm[ollama] chat: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    json["message"]["content"].as_str().map(|s| s.to_string())
        .ok_or_else(|| "llm[ollama]: respuesta inesperada".into())
}

fn embed_ollama(env: &HashMap<String, String>, model: &str, text: &str) -> Result<Vec<f64>, String> {
    let base = ollama_base(env);
    let body = serde_json::json!({"model": model, "prompt": text});
    let resp = ureq::post(&format!("{}/api/embeddings", base))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| format!("llm.embed[ollama]: {}. ¿Está Ollama corriendo en {}?", e, base))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let arr = json["embedding"].as_array()
        .ok_or_else(|| format!("llm.embed[ollama]: respuesta inesperada: {}", json))?;
    Ok(arr.iter().filter_map(|v| v.as_f64()).collect())
}

//   Helpers                                  ─

// Un único lugar con todos los defaults — sobreescribibles desde .env
fn default_model(env: &HashMap<String, String>, provider: &str, kind: &str) -> String {
    match (provider, kind) {
        ("anthropic", _)    => env.get("ANTHROPIC_MODEL").cloned()
                                  .unwrap_or_else(|| "claude-haiku-4-5-20251001".into()),
        ("openai",  "chat") => env.get("OPENAI_MODEL").cloned()
                                  .unwrap_or_else(|| "gpt-4o-mini".into()),
        ("openai",  "embed")=> env.get("OPENAI_EMBED_MODEL").cloned()
                                  .unwrap_or_else(|| "text-embedding-3-small".into()),
        ("gemini",  "chat") => env.get("GEMINI_MODEL").cloned()
                                  .unwrap_or_else(|| "gemini-2.0-flash".into()),
        ("gemini",  "embed")=> env.get("GEMINI_EMBED_MODEL").cloned()
                                  .unwrap_or_else(|| "text-embedding-004".into()),
        ("ollama",  "chat") => env.get("OLLAMA_MODEL").cloned()
                                  .unwrap_or_else(|| "llama3".into()),
        ("ollama",  "embed")=> env.get("OLLAMA_EMBED_MODEL").cloned()
                                  .unwrap_or_else(|| "nomic-embed-text".into()),
        _                   => "unknown".into(),
    }
}

fn detect_provider(model: &str) -> &'static str {
    let m = model.to_lowercase();
    if m.starts_with("claude") { "anthropic" }
    else if m.starts_with("gpt") || m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") || m.starts_with("text-embedding-3") || m.starts_with("text-embedding-ada") { "openai" }
    else if m.starts_with("gemini") || m == "text-embedding-004" { "gemini" }
    else if m.starts_with("ollama:") { "ollama" }
    else if ["llama", "mistral", "phi", "qwen", "gemma", "deepseek", "nomic", "mxbai", "all-minilm", "codellama"].iter().any(|p| m.starts_with(p)) { "ollama" }
    else { "auto" }
}

fn strip_ollama(model: &str) -> &str {
    model.strip_prefix("ollama:").unwrap_or(model)
}

fn ollama_base(env: &HashMap<String, String>) -> String {
    env.get("OLLAMA_URL").cloned().unwrap_or_else(|| "http://localhost:11434".into())
}

fn load_env() -> HashMap<String, String> {
    let mut vars: HashMap<String, String> = std::env::vars().collect();
    let mut path = std::env::current_dir().unwrap_or_default();
    for _ in 0..4 {
        let env_file = path.join(".env");
        if let Ok(content) = std::fs::read_to_string(&env_file) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') { continue; }
                if let Some(eq) = line.find('=') {
                    let key = line[..eq].trim().to_string();
                    let val = line[eq+1..].trim().trim_matches('"').trim_matches('\'').to_string();
                    if !key.is_empty() && !vars.contains_key(&key) { vars.insert(key, val); }
                }
            }
            break;
        }
        if !path.pop() { break; }
    }
    vars
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
