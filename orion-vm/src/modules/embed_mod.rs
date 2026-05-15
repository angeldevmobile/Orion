use crate::eval_value::EvalValue;
use std::collections::HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // embed.text(text, model?) → List<float>
        "text" => {
            if args.is_empty() { return Err("embed.text requiere (text, model?)".into()); }
            let text  = to_str(&args[0]);
            let model = if args.len() > 1 { to_str(&args[1]) } else { "auto".into() };
            let emb   = get_embedding(&text, &model)?;
            Ok(EvalValue::List(emb.into_iter().map(EvalValue::Float).collect()))
        }
        // embed.batch(texts, model?) → List<List<float>>
        "batch" => {
            if args.is_empty() { return Err("embed.batch requiere (texts, model?)".into()); }
            let texts = match &args[0] {
                EvalValue::List(v) => v.clone(),
                _ => return Err("embed.batch: texts debe ser una lista de strings".into()),
            };
            let model = if args.len() > 1 { to_str(&args[1]) } else { "auto".into() };
            let mut result = Vec::new();
            for t in &texts {
                let emb = get_embedding(&to_str(t), &model)?;
                result.push(EvalValue::List(emb.into_iter().map(EvalValue::Float).collect()));
            }
            Ok(EvalValue::List(result))
        }
        // embed.similarity(v1, v2) → float  (-1..1)
        "similarity" => {
            if args.len() < 2 { return Err("embed.similarity requiere (v1, v2)".into()); }
            let v1 = to_float_vec(&args[0])?;
            let v2 = to_float_vec(&args[1])?;
            Ok(EvalValue::Float(cosine_similarity(&v1, &v2)))
        }
        // embed.distance(v1, v2) → float  (0..2, donde 0 = idénticos)
        "distance" => {
            if args.len() < 2 { return Err("embed.distance requiere (v1, v2)".into()); }
            let v1 = to_float_vec(&args[0])?;
            let v2 = to_float_vec(&args[1])?;
            Ok(EvalValue::Float(1.0 - cosine_similarity(&v1, &v2)))
        }
        // embed.normalize(v) → List<float>  (L2)
        "normalize" => {
            if args.is_empty() { return Err("embed.normalize requiere (vector)".into()); }
            let v = to_float_vec(&args[0])?;
            let n = l2_norm(&v);
            if n == 0.0 { return Ok(EvalValue::List(v.into_iter().map(EvalValue::Float).collect())); }
            Ok(EvalValue::List(v.into_iter().map(|x| EvalValue::Float(x / n)).collect()))
        }
        // embed.dot(v1, v2) → float  (producto punto)
        "dot" => {
            if args.len() < 2 { return Err("embed.dot requiere (v1, v2)".into()); }
            let v1 = to_float_vec(&args[0])?;
            let v2 = to_float_vec(&args[1])?;
            let dot: f64 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
            Ok(EvalValue::Float(dot))
        }
        // embed.search(query_text, texts, top?, model?) → List<{text, score, index}>
        // Nota: hace N+1 llamadas a la API. Para corpus grandes usa el módulo `vector`.
        "search" => {
            if args.len() < 2 { return Err("embed.search requiere (query, texts, top?, model?)".into()); }
            let query = to_str(&args[0]);
            let texts = match &args[1] {
                EvalValue::List(v) => v.clone(),
                _ => return Err("embed.search: texts debe ser una lista de strings".into()),
            };
            let top   = if args.len() > 2 { to_usize(&args[2]).unwrap_or(5) } else { 5 };
            let model = if args.len() > 3 { to_str(&args[3]) } else { "auto".into() };

            let query_emb = get_embedding(&query, &model)?;
            let mut scored: Vec<(usize, f64, String)> = Vec::new();
            for (i, t) in texts.iter().enumerate() {
                let text = to_str(t);
                let emb  = get_embedding(&text, &model)?;
                scored.push((i, cosine_similarity(&query_emb, &emb), text));
            }
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(top);

            let result: Vec<EvalValue> = scored.into_iter().map(|(idx, score, text)| {
                let mut m = HashMap::new();
                m.insert("text".into(),  EvalValue::Str(text));
                m.insert("score".into(), EvalValue::Float(round6(score)));
                m.insert("index".into(), EvalValue::Int(idx as i64));
                EvalValue::Dict(m)
            }).collect();
            Ok(EvalValue::List(result))
        }
        f => Err(format!("embed.{}() no existe", f)),
    }
}

//   Dispatch de embeddings                           

pub fn get_embedding(text: &str, model: &str) -> Result<Vec<f64>, String> {
    let env = load_env();
    match model {
        "auto" | "" => {
            if env.contains_key("OPENAI_API_KEY") {
                return embed_openai(&env, "text-embedding-3-small", text);
            }
            if env.contains_key("GEMINI_API_KEY") || env.contains_key("GOOGLE_API_KEY") {
                return embed_gemini(&env, "text-embedding-004", text);
            }
            embed_ollama(&env, "nomic-embed-text", text)
                .map_err(|e| format!("embed: configura OPENAI_API_KEY, GEMINI_API_KEY o inicia Ollama con nomic-embed-text. ({})", e))
        }
        m if m.starts_with("text-embedding-3") || m.starts_with("text-embedding-ada") => {
            embed_openai(&env, m, text)
        }
        m if m == "text-embedding-004" => embed_gemini(&env, m, text),
        m if m.starts_with("ollama:") => {
            embed_ollama(&env, m.strip_prefix("ollama:").unwrap_or(m), text)
        }
        m if ["nomic", "mxbai", "all-minilm", "llama", "mistral", "phi", "qwen", "gemma"].iter().any(|p| m.starts_with(p)) => {
            embed_ollama(&env, m, text)
        }
        m => {
            if env.contains_key("OPENAI_API_KEY") { embed_openai(&env, m, text) }
            else { embed_ollama(&env, m, text) }
        }
    }
}

fn embed_openai(env: &HashMap<String, String>, model: &str, text: &str) -> Result<Vec<f64>, String> {
    let key = env.get("OPENAI_API_KEY").ok_or("OPENAI_API_KEY no configurada")?;
    let body = serde_json::json!({"model": model, "input": text});
    let resp = ureq::post("https://api.openai.com/v1/embeddings")
        .set("Content-Type", "application/json")
        .set("Authorization", &format!("Bearer {}", key))
        .send_json(body)
        .map_err(|e| format!("embed[openai]: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let arr = json["data"][0]["embedding"].as_array()
        .ok_or_else(|| format!("embed[openai]: respuesta inesperada: {}", json))?;
    Ok(arr.iter().filter_map(|v| v.as_f64()).collect())
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
        .map_err(|e| format!("embed[gemini]: {}", e))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let arr = json["embedding"]["values"].as_array()
        .ok_or_else(|| format!("embed[gemini]: respuesta inesperada: {}", json))?;
    Ok(arr.iter().filter_map(|v| v.as_f64()).collect())
}

fn embed_ollama(env: &HashMap<String, String>, model: &str, text: &str) -> Result<Vec<f64>, String> {
    let base = env.get("OLLAMA_URL").cloned().unwrap_or_else(|| "http://localhost:11434".into());
    let body = serde_json::json!({"model": model, "prompt": text});
    let resp = ureq::post(&format!("{}/api/embeddings", base))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| format!("embed[ollama]: {}. ¿Está Ollama corriendo en {}?", e, base))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let arr = json["embedding"].as_array()
        .ok_or_else(|| format!("embed[ollama]: respuesta inesperada: {}", json))?;
    Ok(arr.iter().filter_map(|v| v.as_f64()).collect())
}

//   Matemática                                 

pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() { return 0.0; }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na = l2_norm(a);
    let nb = l2_norm(b);
    if na == 0.0 || nb == 0.0 { return 0.0; }
    (dot / (na * nb)).clamp(-1.0, 1.0)
}

pub fn l2_norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

//   Helpers                                  ─

pub fn to_float_vec(v: &EvalValue) -> Result<Vec<f64>, String> {
    match v {
        EvalValue::List(items) => items.iter().map(|x| match x {
            EvalValue::Float(f) => Ok(*f),
            EvalValue::Int(i)   => Ok(*i as f64),
            other => Err(format!("embed: vector debe contener números, encontró {}", other.type_name())),
        }).collect(),
        _ => Err("embed: se esperaba un vector (lista de números)".into()),
    }
}

fn to_usize(v: &EvalValue) -> Result<usize, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n as usize),
        EvalValue::Float(f) => Ok(*f as usize),
        _ => Err("embed: se esperaba un número entero".into()),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn round6(x: f64) -> f64 { (x * 1_000_000.0).round() / 1_000_000.0 }

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
