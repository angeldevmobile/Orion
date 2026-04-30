/// Orion Insight — análisis de documentos/imágenes con visión computacional + AI.
use crate::eval_value::EvalValue;
use std::collections::HashMap;
use image::{ImageReader, DynamicImage, GenericImageView};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // analyze(path, question?) → análisis con AI Vision (Claude o GPT-4o)
        "analyze" => {
            if args.is_empty() { return Err("insight.analyze requiere (path, question?)".into()); }
            let path     = to_str(&args[0]);
            let question = if args.len() > 1 { to_str(&args[1]) } else { "Describe el contenido de esta imagen en detalle.".into() };
            analyze_with_ai(&path, &question)
        }
        // metadata(path) → {width, height, density, orientation, contrast}
        "metadata" | "extract_metadata" => {
            let path = one_str(function, args)?;
            extract_metadata(&path)
        }
        // summarize(path) → resumen estructurado del documento
        "summarize" => {
            let path = one_str(function, args)?;
            summarize_doc(&path)
        }
        // detect_tables(path) → {detected: bool, confidence: float}
        "detect_tables" | "extract_tables" => {
            let path = one_str(function, args)?;
            detect_tables(&path)
        }
        // detect_signatures(path) → {detected: bool, confidence: float}
        "detect_signatures" | "extract_signatures" => {
            let path = one_str(function, args)?;
            detect_signatures(&path)
        }
        // pixel_density(path) → float (proporción de píxeles oscuros)
        "pixel_density" | "density" => {
            let path = one_str(function, args)?;
            let img  = open_img(&path)?;
            let d    = dark_density(&img);
            Ok(EvalValue::Float((d * 1e6).round() / 1e6))
        }
        // to_base64(path) → string base64 JPEG (para enviar a AI)
        "to_base64" => {
            let path = one_str(function, args)?;
            let img  = open_img(&path)?;
            Ok(EvalValue::Str(img_to_base64(&img)?))
        }

        f => Err(format!("insight.{}() no existe", f)),
    }
}

// ─── Análisis con AI Vision ───────────────────────────────────────────────────

fn analyze_with_ai(path: &str, question: &str) -> Result<EvalValue, String> {
    let img = open_img(path)?;
    let b64 = img_to_base64(&img)?;

    // Contexto estructural
    let meta    = extract_metadata_from_img(&img);
    let context = format!(
        "Análisis estructural previo:\n\
         - Dimensiones: {}×{}\n\
         - Orientación: {}\n\
         - Densidad de contenido: {:.1}%\n\n{}",
        img.width(), img.height(),
        if img.height() > img.width() { "portrait" } else { "landscape" },
        dark_density(&img) * 100.0,
        question
    );

    let env = load_env();

    if let Some(key) = env.get("ANTHROPIC_API_KEY") {
        let model = env.get("ANTHROPIC_MODEL").cloned().unwrap_or_else(|| "claude-haiku-4-5-20251001".into());
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 1024,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": b64}},
                    {"type": "text", "text": context}
                ]
            }]
        });
        let resp = ureq::post("https://api.anthropic.com/v1/messages")
            .set("Content-Type", "application/json")
            .set("x-api-key", key)
            .set("anthropic-version", "2023-06-01")
            .send_json(body)
            .map_err(|e| format!("insight.analyze: {}", e))?;
        let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        let text = json["content"][0]["text"].as_str().unwrap_or("").to_string();
        return Ok(EvalValue::Str(text));
    }

    if let Some(key) = env.get("OPENAI_API_KEY") {
        let model = env.get("OPENAI_MODEL").cloned().unwrap_or_else(|| "gpt-4o-mini".into());
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 1024,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{}", b64)}},
                    {"type": "text", "text": context}
                ]
            }]
        });
        let resp = ureq::post("https://api.openai.com/v1/chat/completions")
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {}", key))
            .send_json(body)
            .map_err(|e| format!("insight.analyze: {}", e))?;
        let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        let text = json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
        return Ok(EvalValue::Str(text));
    }

    // Sin API key: retorna solo el análisis estructural
    let mut m = HashMap::new();
    m.insert("width".into(),       EvalValue::Int(img.width() as i64));
    m.insert("height".into(),      EvalValue::Int(img.height() as i64));
    m.insert("density".into(),     EvalValue::Float(dark_density(&img)));
    m.insert("orientation".into(), EvalValue::Str(if img.height() > img.width() { "portrait" } else { "landscape" }.into()));
    m.insert("note".into(), EvalValue::Str("Agrega ANTHROPIC_API_KEY o OPENAI_API_KEY para análisis con AI".into()));
    Ok(EvalValue::Dict(m))
}

// ─── Análisis estructural (sin AI) ───────────────────────────────────────────

fn extract_metadata(path: &str) -> Result<EvalValue, String> {
    let img = open_img(path)?;
    Ok(extract_metadata_from_img(&img))
}

fn extract_metadata_from_img(img: &DynamicImage) -> EvalValue {
    let (w, h) = img.dimensions();
    let gray   = img.to_luma8();
    let pixels = gray.pixels();
    let total  = (w * h) as f64;
    let dark   = pixels.filter(|p| p[0] < 50).count() as f64;
    let light  = total - dark;
    let density    = dark / total;
    let contrast   = (dark - light).abs() / total;
    let orientation = if h > w { "portrait" } else { "landscape" };

    let mut m = HashMap::new();
    m.insert("width".into(),       EvalValue::Int(w as i64));
    m.insert("height".into(),      EvalValue::Int(h as i64));
    m.insert("density".into(),     EvalValue::Float((density * 1e6).round() / 1e6));
    m.insert("contrast".into(),    EvalValue::Float((contrast * 1e6).round() / 1e6));
    m.insert("orientation".into(), EvalValue::Str(orientation.into()));
    EvalValue::Dict(m)
}

fn detect_tables(path: &str) -> Result<EvalValue, String> {
    let img  = open_img(path)?;
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();

    let h_lines = (0..h).filter(|&y| {
        let dark_count = (0..w).filter(|&x| gray.get_pixel(x, y)[0] < 50).count();
        dark_count as f32 / w as f32 > 0.7
    }).count();

    let v_lines = (0..w).filter(|&x| {
        let dark_count = (0..h).filter(|&y| gray.get_pixel(x, y)[0] < 50).count();
        dark_count as f32 / h as f32 > 0.7
    }).count();

    let detected   = h_lines > 2 && v_lines > 2;
    let confidence = ((h_lines + v_lines) as f64 / 10.0).min(1.0);
    let mut m = HashMap::new();
    m.insert("detected".into(),    EvalValue::Bool(detected));
    m.insert("confidence".into(),  EvalValue::Float((confidence * 100.0).round() / 100.0));
    m.insert("h_lines".into(),     EvalValue::Int(h_lines as i64));
    m.insert("v_lines".into(),     EvalValue::Int(v_lines as i64));
    Ok(EvalValue::Dict(m))
}

fn detect_signatures(path: &str) -> Result<EvalValue, String> {
    let img  = open_img(path)?;
    let (w, h) = img.dimensions();
    let d    = dark_density(&img);
    let detected   = d > 0.01 && d < 0.2;
    let confidence = if detected { ((d - 0.01) / 0.19).max(0.0).min(1.0) } else { 0.0 };
    let mut m = HashMap::new();
    m.insert("detected".into(),   EvalValue::Bool(detected));
    m.insert("confidence".into(), EvalValue::Float((confidence * 100.0).round() / 100.0));
    m.insert("density".into(),    EvalValue::Float((d * 1e6).round() / 1e6));
    Ok(EvalValue::Dict(m))
}

fn summarize_doc(path: &str) -> Result<EvalValue, String> {
    let img      = open_img(path)?;
    let meta     = extract_metadata_from_img(&img);
    let tables   = detect_tables(path)?;
    let sigs     = detect_signatures(path)?;
    let mut m = HashMap::new();
    m.insert("metadata".into(),   meta);
    m.insert("tables".into(),     tables);
    m.insert("signatures".into(), sigs);
    Ok(EvalValue::Dict(m))
}

fn dark_density(img: &DynamicImage) -> f64 {
    let gray  = img.to_luma8();
    let (w, h) = gray.dimensions();
    let total = (w * h) as f64;
    let dark  = gray.pixels().filter(|p| p[0] < 50).count() as f64;
    dark / total
}

fn img_to_base64(img: &DynamicImage) -> Result<String, String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| format!("insight: error al codificar imagen: {}", e))?;
    Ok(b64_encode(buf.get_ref()))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn open_img(path: &str) -> Result<DynamicImage, String> {
    ImageReader::open(path)
        .map_err(|e| format!("insight: no se pudo abrir '{}': {}", path, e))?
        .decode()
        .map_err(|e| format!("insight: no se pudo decodificar '{}': {}", path, e))
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

fn one_str(fn_name: &str, args: Vec<EvalValue>) -> Result<String, String> {
    if args.is_empty() { return Err(format!("insight.{}() requiere 1 argumento", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(input: &[u8]) -> String {
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        out.push(B64[(b0 >> 2) as usize] as char);
        out.push(B64[((b0 & 3) << 4 | b1 >> 4) as usize] as char);
        if chunk.len() > 1 { out.push(B64[((b1 & 0xf) << 2 | b2 >> 6) as usize] as char); } else { out.push('='); }
        if chunk.len() > 2 { out.push(B64[(b2 & 0x3f) as usize] as char); } else { out.push('='); }
    }
    out
}
