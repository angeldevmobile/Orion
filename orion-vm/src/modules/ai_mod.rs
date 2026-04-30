use crate::eval_value::EvalValue;
use crate::ai;
use std::collections::HashMap;
use std::sync::Mutex;

// Historial de chat de sesión (para chat sessions)
static CHAT_HISTORY: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
static ACTIVE_MODEL: Mutex<Option<String>> = Mutex::new(None);

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // --- Core: ya implementados en ai.rs ---

        // think(prompt) → respuesta
        "think" | "ask" => {
            let prompt = one_str(function, args)?;
            let result = ai::think(&prompt)?;
            Ok(EvalValue::Str(result))
        }
        // learn(text) → confirmación
        "learn" => {
            let text = one_str("learn", args)?;
            Ok(EvalValue::Str(ai::learn(&text)))
        }
        // sense(query) → respuesta con memoria
        "sense" => {
            let query = one_str("sense", args)?;
            let result = ai::sense(&query)?;
            Ok(EvalValue::Str(result))
        }

        // --- Funciones de alto nivel (nuevas) ---

        // summarize(text, lang?, length?) → resumen
        "summarize" => {
            if args.is_empty() { return Err("ai.summarize requiere (text)".into()); }
            let text = to_str(&args[0]);
            let lang   = if args.len() > 1 { to_str(&args[1]) } else { "español".into() };
            let length = if args.len() > 2 { to_str(&args[2]) } else { "corto".into() };
            let max_tokens = match length.as_str() { "largo" => 1024, "medio" => 512, _ => 256 };
            let result = ai_call_with_system(
                &format!("Resume este texto de forma {}:\n\n{}", length, text),
                &format!("Eres un asistente que resume textos en {}. Sé conciso y claro.", lang),
                max_tokens,
            )?;
            Ok(EvalValue::Str(result))
        }

        // classify(text, [categories]) → categoría
        "classify" => {
            if args.len() < 2 { return Err("ai.classify requiere (text, categories)".into()); }
            let text = to_str(&args[0]);
            let cats = match &args[1] {
                EvalValue::List(v) => v.iter().map(|x| format!("{}", x)).collect::<Vec<_>>().join(", "),
                other => format!("{}", other),
            };
            let result = ai_call_with_system(
                &text,
                &format!("Clasifica el texto en UNA de estas categorías: {}. Responde SOLO con el nombre de la categoría.", cats),
                32,
            )?;
            Ok(EvalValue::Str(result.trim().to_string()))
        }

        // extract(text, [fields]) → dict
        "extract" => {
            if args.len() < 2 { return Err("ai.extract requiere (text, fields)".into()); }
            let text = to_str(&args[0]);
            let fields = match &args[1] {
                EvalValue::List(v) => v.iter().map(|x| format!("{}", x)).collect::<Vec<_>>(),
                other => vec![format!("{}", other)],
            };
            let fields_json = serde_json::to_string(&fields).unwrap_or_default();
            let result = ai_call_with_system(
                &text,
                &format!("Extrae los campos {} del texto. Responde SOLO con JSON válido. Si un campo no existe usa null.", fields_json),
                512,
            )?;
            // Intenta parsear como JSON
            let clean = clean_json(&result);
            match serde_json::from_str::<serde_json::Value>(&clean) {
                Ok(v) => Ok(crate::modules::json_mod::json_to_eval(v)),
                Err(_) => {
                    let mut m = HashMap::new();
                    m.insert("raw".into(), EvalValue::Str(result));
                    Ok(EvalValue::Dict(m))
                }
            }
        }

        // code(description, lang?) → código generado
        "code" => {
            if args.is_empty() { return Err("ai.code requiere (description, lang?)".into()); }
            let desc = to_str(&args[0]);
            let lang = if args.len() > 1 { to_str(&args[1]) } else { "orion".into() };
            let result = ai_call_with_system(
                &desc,
                &format!("Genera código en {}. Responde SOLO con el código, sin explicaciones ni bloques markdown.", lang),
                1024,
            )?;
            Ok(EvalValue::Str(result))
        }

        // fix(code, error?) → código corregido
        "fix" => {
            if args.is_empty() { return Err("ai.fix requiere (code, error?)".into()); }
            let code_text = to_str(&args[0]);
            let error = if args.len() > 1 { to_str(&args[1]) } else { String::new() };
            let content = if error.is_empty() {
                format!("Código:\n{}", code_text)
            } else {
                format!("Código:\n{}\n\nError:\n{}", code_text, error)
            };
            let result = ai_call_with_system(
                &content,
                "Corrige el código. Responde SOLO con el código corregido, sin explicaciones.",
                1024,
            )?;
            Ok(EvalValue::Str(result))
        }

        // translate(text, to?) → traducción
        "translate" => {
            if args.is_empty() { return Err("ai.translate requiere (text, to?)".into()); }
            let text = to_str(&args[0]);
            let to   = if args.len() > 1 { to_str(&args[1]) } else { "english".into() };
            let result = ai_call_with_system(
                &text,
                &format!("Traduce al {}. Responde SOLO con la traducción.", to),
                1024,
            )?;
            Ok(EvalValue::Str(result))
        }

        // sentiment(text) → "positivo" | "negativo" | "neutro"
        "sentiment" => {
            let text = one_str("sentiment", args)?;
            let result = ai_call_with_system(
                &text,
                "Analiza el sentimiento. Responde SOLO con una palabra: positivo, negativo, o neutro.",
                8,
            )?;
            Ok(EvalValue::Str(result.trim().to_lowercase()))
        }

        // complete(text, max_tokens?) → continuación
        "complete" => {
            if args.is_empty() { return Err("ai.complete requiere (text)".into()); }
            let text       = to_str(&args[0]);
            let max_tokens = if args.len() > 1 { to_i64(&args[1])? as u32 } else { 256 };
            let result = ai_call_with_system(
                &text,
                "Continúa el texto o código de forma natural y coherente. Responde SOLO con la continuación.",
                max_tokens,
            )?;
            Ok(EvalValue::Str(result))
        }

        // improve(text) → texto mejorado
        "improve" => {
            let text = one_str("improve", args)?;
            let result = ai_call_with_system(
                &text,
                "Mejora la redacción, claridad y calidad del texto. Responde SOLO con el texto mejorado.",
                1024,
            )?;
            Ok(EvalValue::Str(result))
        }

        // explain(code, lang?) → explicación
        "explain" => {
            if args.is_empty() { return Err("ai.explain requiere (code, lang?)".into()); }
            let code_text = to_str(&args[0]);
            let lang = if args.len() > 1 { to_str(&args[1]) } else { "español".into() };
            let result = ai_call_with_system(
                &format!("Explica este código:\n\n{}", code_text),
                &format!("Eres un experto programador. Explica el código en {} de forma clara y concisa.", lang),
                512,
            )?;
            Ok(EvalValue::Str(result))
        }

        // qa(context, question) → respuesta
        "qa" => {
            if args.len() < 2 { return Err("ai.qa requiere (context, question)".into()); }
            let context  = to_str(&args[0]);
            let question = to_str(&args[1]);
            let result = ai_call_with_system(
                &format!("Contexto:\n{}\n\nPregunta: {}", context, question),
                "Responde SOLO con base en el contexto dado. Si la respuesta no está en el contexto, dilo.",
                512,
            )?;
            Ok(EvalValue::Str(result))
        }

        // search_in(text, query) → extracto relevante
        "search_in" => {
            if args.len() < 2 { return Err("ai.search_in requiere (text, query)".into()); }
            let text  = to_str(&args[0]);
            let query = to_str(&args[1]);
            let result = ai_call_with_system(
                &format!("Texto:\n{}\n\nBusca: {}", text, query),
                "Encuentra y extrae la información solicitada del texto. Sé directo y preciso.",
                256,
            )?;
            Ok(EvalValue::Str(result))
        }

        // --- Chat session (memoria de conversación) ---

        // chat_say(message) → inicia sesión con system prompt
        "chat_start" | "chat_say" => {
            let system = one_str(function, args)?;
            let mut hist = CHAT_HISTORY.lock().unwrap();
            hist.clear();
            drop(hist);
            Ok(EvalValue::Str(format!("[chat iniciado: {}]", system)))
        }

        // chat_ask(prompt) → respuesta manteniendo historial
        "chat_ask" => {
            let prompt = one_str("chat_ask", args)?;
            let mut hist = CHAT_HISTORY.lock().unwrap();
            hist.push(("user".into(), prompt.clone()));
            let messages: Vec<serde_json::Value> = hist.iter()
                .map(|(role, content)| serde_json::json!({"role": role, "content": content}))
                .collect();
            drop(hist);
            let response = ai_call_messages(messages, 1024)?;
            let mut hist2 = CHAT_HISTORY.lock().unwrap();
            hist2.push(("assistant".into(), response.clone()));
            Ok(EvalValue::Str(response))
        }

        // chat_reset() → limpia historial
        "chat_reset" => {
            CHAT_HISTORY.lock().unwrap().clear();
            Ok(EvalValue::Str("[chat reseteado]".into()))
        }

        // --- Utilidades de modelo ---

        // set_model(name) → nombre del modelo
        "set_model" => {
            let name = one_str("set_model", args)?;
            *ACTIVE_MODEL.lock().unwrap() = Some(name.clone());
            Ok(EvalValue::Str(name))
        }

        // provider() → "anthropic" | "openai" | "none"
        "provider" => {
            let env = load_env();
            let p = detect_provider(&env);
            Ok(EvalValue::Str(p.unwrap_or_else(|| "none".into())))
        }

        // status() → info del estado AI
        "status" => {
            let env = load_env();
            let p = detect_provider(&env);
            let msg = match p {
                Some(ref provider) => format!("AI activo — proveedor: {}", provider),
                None => "AI no configurado. Agrega ANTHROPIC_API_KEY o OPENAI_API_KEY en tu .env".into(),
            };
            Ok(EvalValue::Str(msg))
        }

        // memory_size() → int
        "memory_size" => {
            let size = ai::memory_size();
            Ok(EvalValue::Int(size as i64))
        }

        // memory_clear() → confirmación
        "memory_clear" => {
            ai::memory_clear();
            Ok(EvalValue::Str("[memoria borrada]".into()))
        }

        f => Err(format!("ai.{}() no existe", f)),
    }
}

// ─── Helpers internos ─────────────────────────────────────────────────────────

fn ai_call_with_system(prompt: &str, system: &str, max_tokens: u32) -> Result<String, String> {
    let env = load_env();
    let has_anthropic = env.contains_key("ANTHROPIC_API_KEY");
    let has_openai    = env.contains_key("OPENAI_API_KEY");
    let pref = env.get("AI_MODEL").map(|s| s.to_lowercase());

    let use_anthropic = match pref.as_deref() {
        Some("openai") if has_openai    => false,
        Some("claude") if has_anthropic => true,
        _                               => has_anthropic,
    };

    if use_anthropic {
        call_anthropic_with_system(&env, prompt, system, max_tokens)
    } else if has_openai {
        call_openai_with_system(&env, prompt, system, max_tokens)
    } else {
        Err("No hay API key configurada. Agrega ANTHROPIC_API_KEY o OPENAI_API_KEY en tu .env".into())
    }
}

fn call_anthropic_with_system(env: &std::collections::HashMap<String, String>, prompt: &str, system: &str, max_tokens: u32) -> Result<String, String> {
    let key   = env.get("ANTHROPIC_API_KEY").ok_or("ANTHROPIC_API_KEY no configurada")?;
    let model = get_model(env, "anthropic");
    let body  = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": system,
        "messages": [{"role": "user", "content": prompt}]
    });
    let resp = ureq::post("https://api.anthropic.com/v1/messages")
        .set("Content-Type", "application/json")
        .set("x-api-key", key)
        .set("anthropic-version", "2023-06-01")
        .send_json(body)
        .map_err(|e| format!("ai error: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    json["content"][0]["text"].as_str().map(|s| s.to_string())
        .ok_or_else(|| format!("Respuesta inesperada: {}", json))
}

fn call_openai_with_system(env: &std::collections::HashMap<String, String>, prompt: &str, system: &str, max_tokens: u32) -> Result<String, String> {
    let key   = env.get("OPENAI_API_KEY").ok_or("OPENAI_API_KEY no configurada")?;
    let model = get_model(env, "openai");
    let body  = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user",   "content": prompt}
        ]
    });
    let resp = ureq::post("https://api.openai.com/v1/chat/completions")
        .set("Content-Type", "application/json")
        .set("Authorization", &format!("Bearer {}", key))
        .send_json(body)
        .map_err(|e| format!("ai error: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    json["choices"][0]["message"]["content"].as_str().map(|s| s.to_string())
        .ok_or_else(|| format!("Respuesta inesperada: {}", json))
}

fn ai_call_messages(messages: Vec<serde_json::Value>, max_tokens: u32) -> Result<String, String> {
    let env = load_env();
    if let Some(key) = env.get("ANTHROPIC_API_KEY") {
        let model = get_model(&env, "anthropic");
        let body = serde_json::json!({
            "model": model, "max_tokens": max_tokens, "messages": messages
        });
        let resp = ureq::post("https://api.anthropic.com/v1/messages")
            .set("Content-Type", "application/json")
            .set("x-api-key", key)
            .set("anthropic-version", "2023-06-01")
            .send_json(body).map_err(|e| format!("chat error: {}", e))?;
        let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        return json["content"][0]["text"].as_str().map(|s| s.to_string())
            .ok_or_else(|| "Respuesta inesperada".into());
    }
    if let Some(key) = env.get("OPENAI_API_KEY") {
        let model = get_model(&env, "openai");
        let body = serde_json::json!({
            "model": model, "max_tokens": max_tokens, "messages": messages
        });
        let resp = ureq::post("https://api.openai.com/v1/chat/completions")
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {}", key))
            .send_json(body).map_err(|e| format!("chat error: {}", e))?;
        let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        return json["choices"][0]["message"]["content"].as_str().map(|s| s.to_string())
            .ok_or_else(|| "Respuesta inesperada".into());
    }
    Err("No hay API key configurada".into())
}

fn get_model(env: &std::collections::HashMap<String, String>, provider: &str) -> String {
    if let Some(m) = ACTIVE_MODEL.lock().unwrap().clone() { return m; }
    match provider {
        "anthropic" => env.get("ANTHROPIC_MODEL").cloned().unwrap_or_else(|| "claude-haiku-4-5-20251001".into()),
        "openai"    => env.get("OPENAI_MODEL").cloned().unwrap_or_else(|| "gpt-4o-mini".into()),
        _           => "unknown".into(),
    }
}

fn load_env() -> std::collections::HashMap<String, String> {
    let mut vars: std::collections::HashMap<String, String> = std::env::vars().collect();
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

fn detect_provider(env: &std::collections::HashMap<String, String>) -> Option<String> {
    let has_a = env.contains_key("ANTHROPIC_API_KEY");
    let has_o = env.contains_key("OPENAI_API_KEY");
    if has_a { Some("anthropic".into()) }
    else if has_o { Some("openai".into()) }
    else { None }
}

fn clean_json(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with("```") {
        let lines: Vec<&str> = raw.lines().collect();
        let start = 1;
        let end   = if lines.last().map(|l| l.trim() == "```").unwrap_or(false) { lines.len()-1 } else { lines.len() };
        return lines[start..end].join("\n").trim().to_string();
    }
    raw.to_string()
}

fn one_str(fn_name: &str, args: Vec<EvalValue>) -> Result<String, String> {
    if args.is_empty() { return Err(format!("ai.{}() requiere 1 argumento", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("ai: esperaba número, recibió {}", other.type_name())),
    }
}
