use crate::eval_value::EvalValue;
use crate::ai;
use indexmap::IndexMap as HashMap;
use std::sync::Mutex;

// Historial de chat de sesión (para chat sessions)
static CHAT_HISTORY: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
// System prompt de la sesión de chat (lo fija chat_start)
static CHAT_SYSTEM: Mutex<Option<String>> = Mutex::new(None);

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
            if args.is_empty() { return Err("ai.summarize requires (text)".into()); }
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
            if args.len() < 2 { return Err("ai.classify requires (text, categories)".into()); }
            let text = to_str(&args[0]);
            let cats = match &args[1] {
                EvalValue::List(v) => v.iter().map(|x| format!("{}", x)).collect::<Vec<_>>().join(", "),
                other => format!("{}", other),
            };
            let result = ai_call_with_system(
                &text,
                &format!("Classify the text into ONE of these categories: {}. Answer with the category name ONLY.", cats),
                32,
            )?;
            Ok(EvalValue::Str(result.trim().to_string()))
        }

        // extract(text, [fields]) → dict
        "extract" => {
            if args.len() < 2 { return Err("ai.extract requires (text, fields)".into()); }
            let text = to_str(&args[0]);
            let fields = match &args[1] {
                EvalValue::List(v) => v.iter().map(|x| format!("{}", x)).collect::<Vec<_>>(),
                other => vec![format!("{}", other)],
            };
            let fields_json = serde_json::to_string(&fields).unwrap_or_default();
            let result = ai_call_with_system(
                &text,
                &format!("Extract the fields {} from the text. Answer with valid JSON ONLY. Use null for a field that is not there.", fields_json),
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
            if args.is_empty() { return Err("ai.code requires (description, lang?)".into()); }
            let desc = to_str(&args[0]);
            let lang = if args.len() > 1 { to_str(&args[1]) } else { "orion".into() };
            let result = ai_call_with_system(
                &desc,
                &format!("Generate code in {}. Answer with the code ONLY, no explanations and no markdown blocks.", lang),
                1024,
            )?;
            Ok(EvalValue::Str(result))
        }

        // fix(code, error?) → código corregido
        "fix" => {
            if args.is_empty() { return Err("ai.fix requires (code, error?)".into()); }
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
            if args.is_empty() { return Err("ai.translate requires (text, to?)".into()); }
            let text = to_str(&args[0]);
            let to   = if args.len() > 1 { to_str(&args[1]) } else { "english".into() };
            let result = ai_call_with_system(
                &text,
                &format!("Translate into {}. Answer with the translation ONLY.", to),
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
            if args.is_empty() { return Err("ai.complete requires (text)".into()); }
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
            if args.is_empty() { return Err("ai.explain requires (code, lang?)".into()); }
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
            if args.len() < 2 { return Err("ai.qa requires (context, question)".into()); }
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
            if args.len() < 2 { return Err("ai.search_in requires (text, query)".into()); }
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

        // chat_start(system) → confirmación; define el system prompt de la sesión
        "chat_start" | "chat_say" => {
            let system = one_str(function, args)?;
            CHAT_HISTORY.lock().unwrap().clear();
            *CHAT_SYSTEM.lock().unwrap() = Some(system.clone());
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
            let system = CHAT_SYSTEM.lock().unwrap().clone();
            let response = ai::ai_call_chat(&messages, system.as_deref(), 1024)?;
            let mut hist2 = CHAT_HISTORY.lock().unwrap();
            hist2.push(("assistant".into(), response.clone()));
            Ok(EvalValue::Str(response))
        }

        // chat_reset() → limpia historial y system prompt
        "chat_reset" => {
            CHAT_HISTORY.lock().unwrap().clear();
            *CHAT_SYSTEM.lock().unwrap() = None;
            Ok(EvalValue::Str("[chat reseteado]".into()))
        }

        // --- Utilidades de modelo ---

        // set_model(name) → nombre del modelo; afecta a TODAS las funciones ai.*
        "set_model" => {
            let name = one_str("set_model", args)?;
            ai::set_model_override(Some(name.clone()));
            Ok(EvalValue::Str(name))
        }

        // provider() → "anthropic" | "openai" | "none"
        "provider" => {
            let env = ai::env_vars();
            let p = ai::detect_provider(&env).unwrap_or("none");
            Ok(EvalValue::Str(p.into()))
        }

        // status() → {configured, provider, model, memory, chat}
        "status" => {
            let env = ai::env_vars();
            let p = ai::detect_provider(&env);
            let mut m = HashMap::new();
            m.insert("configured".into(), EvalValue::Bool(p.is_some()));
            m.insert("provider".into(),   EvalValue::Str(p.unwrap_or("none").into()));
            m.insert("model".into(),      EvalValue::Str(match p {
                Some(prov) => ai::model_for(&env, prov),
                None => "none".into(),
            }));
            m.insert("memory".into(), EvalValue::Int(ai::memory_size() as i64));
            m.insert("chat".into(),   EvalValue::Int(CHAT_HISTORY.lock().unwrap().len() as i64));
            Ok(EvalValue::Dict(m))
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

        f => Err(format!("ai.{}() does not exist", f)),
    }
}

//     Helpers internos                                                          

// Toda la selección de proveedor/modelo y el HTTP viven en crate::ai; aquí
// solo se arma el prompt. Así set_model y AI_MODEL aplican parejo en todo ai.*.
fn ai_call_with_system(prompt: &str, system: &str, max_tokens: u32) -> Result<String, String> {
    ai::ai_call(prompt, Some(system), max_tokens)
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
    if args.is_empty() { return Err(format!("ai.{}() requires 1 argument", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("ai: expected a number, got {}", other.type_name())),
    }
}
