use crate::eval_value::EvalValue;
use regex::Regex;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // --- Básico ---
        "length" => {
            let s = one_str("length", &args)?;
            Ok(EvalValue::Int(s.chars().count() as i64))
        }
        "upper" => Ok(EvalValue::Str(one_str("upper", &args)?.to_uppercase())),
        "lower" => Ok(EvalValue::Str(one_str("lower", &args)?.to_lowercase())),
        "strip" => Ok(EvalValue::Str(one_str("strip", &args)?.trim().to_string())),
        "reverse" => {
            let s = one_str("reverse", &args)?;
            Ok(EvalValue::Str(s.chars().rev().collect()))
        }
        "title" => {
            let s = one_str("title", &args)?;
            let titled = s.split_whitespace()
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        None    => String::new(),
                        Some(f) => f.to_uppercase().to_string() + c.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            Ok(EvalValue::Str(titled))
        }

        // --- División y unión ---
        "split" => {
            if args.is_empty() { return Err("strings.split requiere (s, sep?)".into()); }
            let s = to_str(&args[0]);
            let parts: Vec<EvalValue> = if args.len() > 1 {
                let sep = to_str(&args[1]);
                s.split(sep.as_str()).map(|p| EvalValue::Str(p.to_string())).collect()
            } else {
                s.split_whitespace().map(|p| EvalValue::Str(p.to_string())).collect()
            };
            Ok(EvalValue::List(parts))
        }
        "join" => {
            if args.len() < 2 { return Err("strings.join requiere (list, sep)".into()); }
            let list = match &args[0] {
                EvalValue::List(v) => v.iter().map(|x| format!("{}", x)).collect::<Vec<_>>(),
                _ => return Err("strings.join: el primer argumento debe ser una lista".into()),
            };
            let sep = to_str(&args[1]);
            Ok(EvalValue::Str(list.join(&sep)))
        }

        // --- Reemplazos y búsquedas ---
        "replace" => {
            if args.len() < 3 { return Err("strings.replace requiere (s, old, new)".into()); }
            let s   = to_str(&args[0]);
            let old = to_str(&args[1]);
            let new = to_str(&args[2]);
            Ok(EvalValue::Str(s.replace(old.as_str(), new.as_str())))
        }
        "contains" => {
            if args.len() < 2 { return Err("strings.contains requiere (s, sub)".into()); }
            let s   = to_str(&args[0]);
            let sub = to_str(&args[1]);
            Ok(EvalValue::Bool(s.contains(sub.as_str())))
        }
        "starts_with" => {
            if args.len() < 2 { return Err("strings.starts_with requiere (s, sub)".into()); }
            let s   = to_str(&args[0]);
            let sub = to_str(&args[1]);
            Ok(EvalValue::Bool(s.starts_with(sub.as_str())))
        }
        "ends_with" => {
            if args.len() < 2 { return Err("strings.ends_with requiere (s, sub)".into()); }
            let s   = to_str(&args[0]);
            let sub = to_str(&args[1]);
            Ok(EvalValue::Bool(s.ends_with(sub.as_str())))
        }
        "index_of" | "find_index" => {
            if args.len() < 2 { return Err("strings.index_of requiere (s, sub)".into()); }
            let s   = to_str(&args[0]);
            let sub = to_str(&args[1]);
            match s.find(sub.as_str()) {
                Some(i) => Ok(EvalValue::Int(i as i64)),
                None    => Ok(EvalValue::Int(-1)),
            }
        }

        // --- Regex ---
        "match" => {
            if args.len() < 2 { return Err("strings.match requiere (pattern, s)".into()); }
            let pattern = to_str(&args[0]);
            let s       = to_str(&args[1]);
            let re = Regex::new(&pattern).map_err(|e| format!("strings.match regex: {}", e))?;
            Ok(EvalValue::Bool(re.is_match(&s)))
        }
        "find" => {
            if args.len() < 2 { return Err("strings.find requiere (pattern, s)".into()); }
            let pattern = to_str(&args[0]);
            let s       = to_str(&args[1]);
            let re = Regex::new(&pattern).map_err(|e| format!("strings.find regex: {}", e))?;
            let matches: Vec<EvalValue> = re.find_iter(&s)
                .map(|m| EvalValue::Str(m.as_str().to_string()))
                .collect();
            Ok(EvalValue::List(matches))
        }
        "replace_regex" => {
            if args.len() < 3 { return Err("strings.replace_regex requiere (pattern, repl, s)".into()); }
            let pattern = to_str(&args[0]);
            let repl    = to_str(&args[1]);
            let s       = to_str(&args[2]);
            let re = Regex::new(&pattern).map_err(|e| format!("strings.replace_regex: {}", e))?;
            Ok(EvalValue::Str(re.replace_all(&s, repl.as_str()).into_owned()))
        }

        // --- Padding y formato ---
        "pad" => {
            if args.is_empty() { return Err("strings.pad requiere (s, width, char?)".into()); }
            let s     = to_str(&args[0]);
            let width = if args.len() > 1 { to_i64(&args[1])? as usize } else { 0 };
            let ch    = if args.len() > 2 { to_str(&args[2]).chars().next().unwrap_or(' ') } else { ' ' };
            let padded = format!("{}{}", s, ch.to_string().repeat(width.saturating_sub(s.chars().count())));
            Ok(EvalValue::Str(padded))
        }
        "center" => {
            if args.len() < 2 { return Err("strings.center requiere (s, width)".into()); }
            let s     = to_str(&args[0]);
            let width = to_i64(&args[1])? as usize;
            let ch    = if args.len() > 2 { to_str(&args[2]).chars().next().unwrap_or(' ') } else { ' ' };
            let len   = s.chars().count();
            if len >= width { return Ok(EvalValue::Str(s)); }
            let total_pad = width - len;
            let left  = total_pad / 2;
            let right = total_pad - left;
            Ok(EvalValue::Str(format!(
                "{}{}{}",
                ch.to_string().repeat(left),
                s,
                ch.to_string().repeat(right)
            )))
        }

        // --- Futuristas Orion ---
        "orbit" => {
            if args.is_empty() { return Err("strings.orbit requiere (s, times?)".into()); }
            let s     = to_str(&args[0]);
            let times = if args.len() > 1 { to_i64(&args[1])? as usize } else { 2 };
            let chars: Vec<char> = s.chars().collect();
            if chars.is_empty() { return Ok(EvalValue::Str(s)); }
            let t = times % chars.len();
            let rotated: String = chars[t..].iter().chain(chars[..t].iter()).collect();
            Ok(EvalValue::Str(rotated))
        }
        "mirror" => {
            let s = one_str("mirror", &args)?;
            let rev: String = s.chars().rev().collect();
            Ok(EvalValue::Str(format!("{}{}", s, rev)))
        }
        "glow" => {
            let s = one_str("glow", &args)?;
            Ok(EvalValue::Str(format!("✨{}✨", s.to_uppercase())))
        }

        // --- Codificación ---
        "encode_base64" => {
            let s = one_str("encode_base64", &args)?;
            Ok(EvalValue::Str(base64_encode(s.as_bytes())))
        }
        "decode_base64" => {
            let s = one_str("decode_base64", &args)?;
            let bytes = base64_decode(&s).map_err(|e| format!("strings.decode_base64: {}", e))?;
            String::from_utf8(bytes)
                .map(EvalValue::Str)
                .map_err(|e| format!("strings.decode_base64: {}", e))
        }

        // --- Utilidades extra ---
        "repeat" => {
            if args.len() < 2 { return Err("strings.repeat requiere (s, n)".into()); }
            let s = to_str(&args[0]);
            let n = to_i64(&args[1])? as usize;
            Ok(EvalValue::Str(s.repeat(n)))
        }
        "count" => {
            if args.len() < 2 { return Err("strings.count requiere (s, sub)".into()); }
            let s   = to_str(&args[0]);
            let sub = to_str(&args[1]);
            Ok(EvalValue::Int(s.matches(sub.as_str()).count() as i64))
        }
        "is_empty" => {
            let s = one_str("is_empty", &args)?;
            Ok(EvalValue::Bool(s.is_empty()))
        }
        "is_numeric" => {
            let s = one_str("is_numeric", &args)?;
            Ok(EvalValue::Bool(s.parse::<f64>().is_ok()))
        }

        f => Err(format!("strings.{}() no existe", f)),
    }
}

fn one_str(fn_name: &str, args: &[EvalValue]) -> Result<String, String> {
    if args.is_empty() {
        return Err(format!("strings.{}() requiere al menos 1 argumento", fn_name));
    }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v {
        EvalValue::Str(s) => s.clone(),
        other => format!("{}", other),
    }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("strings: esperaba número, recibió {}", other.type_name())),
    }
}

// Base64 sin dependencia externa (RFC 4648)
const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &[u8]) -> String {
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        out.push(B64_CHARS[(b0 >> 2) as usize] as char);
        out.push(B64_CHARS[((b0 & 3) << 4 | b1 >> 4) as usize] as char);
        if chunk.len() > 1 { out.push(B64_CHARS[((b1 & 0xf) << 2 | b2 >> 6) as usize] as char); } else { out.push('='); }
        if chunk.len() > 2 { out.push(B64_CHARS[(b2 & 0x3f) as usize] as char); } else { out.push('='); }
    }
    out
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let table: [u8; 256] = {
        let mut t = [255u8; 256];
        for (i, &c) in B64_CHARS.iter().enumerate() { t[c as usize] = i as u8; }
        t
    };
    let input = input.replace('=', "");
    let mut out = Vec::new();
    let bytes: Vec<u8> = input.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| {
            let v = table[c as usize];
            if v == 255 { Err(format!("carácter inválido en base64: {}", c)) }
            else { Ok(v) }
        })
        .collect::<Result<_, _>>()?;
    for chunk in bytes.chunks(4) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        let b3 = if chunk.len() > 3 { chunk[3] } else { 0 };
        out.push(b0 << 2 | b1 >> 4);
        if chunk.len() > 2 { out.push((b1 & 0xf) << 4 | b2 >> 2); }
        if chunk.len() > 3 { out.push((b2 & 3) << 6 | b3); }
    }
    Ok(out)
}
