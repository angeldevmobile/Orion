use crate::eval_value::EvalValue;
use std::collections::HashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // event(data) → "data: ...\n\n"
        "event" => {
            let data  = one_str("sse.event", &args)?;
            let lines = fmt_data(&data);
            Ok(EvalValue::Str(format!("{}\n", lines)))
        }

        // named(name, data) → "event: name\ndata: ...\n\n"
        "named" => {
            if args.len() < 2 { return Err("sse.named requiere (name, data)".into()); }
            let name  = to_str(&args[0]);
            let data  = to_str(&args[1]);
            let lines = fmt_data(&data);
            Ok(EvalValue::Str(format!("event: {}\n{}\n", name, lines)))
        }

        // id(id, name, data) → "id: id\nevent: name\ndata: ...\n\n"
        "id" => {
            if args.len() < 3 { return Err("sse.id requiere (id, name, data)".into()); }
            let id    = to_str(&args[0]);
            let name  = to_str(&args[1]);
            let data  = to_str(&args[2]);
            let lines = fmt_data(&data);
            Ok(EvalValue::Str(format!("id: {}\nevent: {}\n{}\n", id, name, lines)))
        }

        // json_event(name, dict_or_list) → SSE con data JSON
        "json_event" => {
            if args.len() < 2 { return Err("sse.json_event requiere (name, value)".into()); }
            let name = to_str(&args[0]);
            let json = eval_to_json_str(&args[1]);
            let lines = fmt_data(&json);
            Ok(EvalValue::Str(format!("event: {}\n{}\n", name, lines)))
        }

        // comment(text) → ": text\n\n"
        "comment" => {
            let text = one_str("sse.comment", &args)?;
            Ok(EvalValue::Str(format!(": {}\n\n", text)))
        }

        // retry(ms) → "retry: ms\n\n"
        "retry" => {
            if args.is_empty() { return Err("sse.retry requiere (ms)".into()); }
            let ms = args[0].to_i64()?;
            if ms < 0 { return Err("sse.retry: ms debe ser >= 0".into()); }
            Ok(EvalValue::Str(format!("retry: {}\n\n", ms)))
        }

        // keep_alive() → ": keep-alive\n\n"
        "keep_alive" => {
            Ok(EvalValue::Str(": keep-alive\n\n".into()))
        }

        // headers() → Dict con headers HTTP para SSE
        "headers" => {
            let mut d = HashMap::new();
            d.insert("Content-Type".into(),      EvalValue::Str("text/event-stream".into()));
            d.insert("Cache-Control".into(),     EvalValue::Str("no-cache".into()));
            d.insert("Connection".into(),        EvalValue::Str("keep-alive".into()));
            d.insert("X-Accel-Buffering".into(), EvalValue::Str("no".into()));
            d.insert("Transfer-Encoding".into(), EvalValue::Str("chunked".into()));
            Ok(EvalValue::Dict(d))
        }

        // parse(raw_sse_str) → Dict {event?, id?, data, retry?}
        "parse" => {
            let raw = one_str("sse.parse", &args)?;
            Ok(parse_event(&raw))
        }

        f => Err(format!("sse.{}() no existe", f)),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Formatea el campo `data:` respetando líneas múltiples (spec SSE §9.2.6).
fn fmt_data(data: &str) -> String {
    data.lines()
        .map(|l| format!("data: {}\n", l))
        .collect::<Vec<_>>()
        .join("")
}

/// Parsea un bloque SSE crudo en un Dict.
fn parse_event(raw: &str) -> EvalValue {
    let mut d: HashMap<String, EvalValue> = HashMap::new();
    let mut data_lines: Vec<String> = Vec::new();

    for line in raw.lines() {
        if line.is_empty() { continue; }
        if let Some(rest) = line.strip_prefix(':') {
            // comentario — ignorar
            let _ = rest;
        } else if let Some(val) = line.strip_prefix("event: ") {
            d.insert("event".into(), EvalValue::Str(val.to_string()));
        } else if let Some(val) = line.strip_prefix("id: ") {
            d.insert("id".into(), EvalValue::Str(val.to_string()));
        } else if let Some(val) = line.strip_prefix("retry: ") {
            if let Ok(ms) = val.trim().parse::<i64>() {
                d.insert("retry".into(), EvalValue::Int(ms));
            }
        } else if let Some(val) = line.strip_prefix("data: ") {
            data_lines.push(val.to_string());
        } else if line == "data" {
            data_lines.push(String::new());
        }
    }

    d.insert("data".into(), EvalValue::Str(data_lines.join("\n")));
    EvalValue::Dict(d)
}

fn eval_to_json_str(v: &EvalValue) -> String {
    match v {
        EvalValue::Dict(m) => {
            let pairs: Vec<String> = m.iter()
                .map(|(k, v)| format!("\"{}\":{}", k, eval_to_json_str(v)))
                .collect();
            format!("{{{}}}", pairs.join(","))
        }
        EvalValue::List(items) => {
            let elems: Vec<String> = items.iter().map(eval_to_json_str).collect();
            format!("[{}]", elems.join(","))
        }
        EvalValue::Str(s)   => format!("\"{}\"", s.replace('"', "\\\"")),
        EvalValue::Int(n)   => n.to_string(),
        EvalValue::Float(f) => f.to_string(),
        EvalValue::Bool(b)  => if *b { "true".into() } else { "false".into() },
        EvalValue::Null     => "null".into(),
        other               => format!("\"{}\"", other),
    }
}

fn one_str(ctx: &str, args: &[EvalValue]) -> Result<String, String> {
    if args.is_empty() { return Err(format!("{} requiere (data)", ctx)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
