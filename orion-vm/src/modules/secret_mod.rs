//! Secrets for Orion programs. Values come from the environment, `NAME_FILE`
//! (Docker/Kubernetes secrets) or a `.env`, and are redacted from Orion's output.

use crate::eval_value::EvalValue;
use indexmap::IndexMap;
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};

/// Variables read from a `.env` with `secret.load`.
static DOTENV: Mutex<Option<IndexMap<String, String>>> = Mutex::new(None);
/// Secret values handed to the program; `redact` replaces them with "***".
static KNOWN: RwLock<Vec<String>> = RwLock::new(Vec::new());
/// Names of the secrets read so far, for `secret.all`.
static NAMES: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Fast path: while no secret has been read, `redact` costs one atomic load.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Shorter values are not redacted: "abc" would also hide ordinary text.
const MIN_REDACT: usize = 6;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // load(path?) → int: reads a .env (default ".env"); refused in production
        "load" => {
            if is_production() {
                return Err("secret.load: .env files are not loaded with ORION_ENV=production; \
                            set real environment variables instead".into());
            }
            let explicit = args.first().map(to_str);
            let path = explicit.clone().unwrap_or_else(|| ".env".into());
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                // Without an explicit path, a missing .env is fine (e.g. in CI).
                Err(_) if explicit.is_none() => return Ok(EvalValue::Int(0)),
                Err(e) => return Err(format!("secret.load: cannot read '{}': {}", path, e)),
            };
            let map = parse_dotenv(&content).map_err(|e| format!("secret.load '{}': {}", path, e))?;
            let n = map.len() as i64;
            *DOTENV.lock().unwrap() = Some(map);
            Ok(EvalValue::Int(n))
        }
        // get(name, default?) → string or null; the default is refused in production
        "get" => {
            let name = name_arg("get", &args)?;
            if let Some(v) = lookup(&name)? {
                remember(&name, &v);
                return Ok(EvalValue::Str(v));
            }
            match args.get(1) {
                None => Ok(EvalValue::Null),
                Some(_) if is_production() => Err(format!(
                    "secret '{}' is not set, and defaults are not allowed with ORION_ENV=production", name)),
                Some(d) => Ok(d.clone()),
            }
        }
        // require(name | [names], { min_length }?) → string, or a dict for a list
        // Checks everything first and reports every missing or weak secret at once.
        "require" => {
            let (names, single) = match args.first() {
                Some(EvalValue::List(l)) => (l.iter().map(to_str).collect::<Vec<_>>(), false),
                Some(v) => (vec![to_str(v)], true),
                None => return Err("secret.require requires (name | [names], options?)".into()),
            };
            let min = match args.get(1) {
                Some(EvalValue::Dict(o)) => match o.get("min_length") {
                    Some(EvalValue::Int(n)) => *n as usize,
                    Some(EvalValue::Float(f)) => *f as usize,
                    _ => 0,
                },
                _ => 0,
            };
            let mut found = IndexMap::new();
            let mut problems = Vec::new();
            for name in &names {
                match lookup(name) {
                    Ok(Some(v)) if v.chars().count() < min =>
                        problems.push(format!("{}: shorter than {} characters", name, min)),
                    Ok(Some(v)) => { found.insert(name.clone(), v); }
                    Ok(None) => problems.push(format!("{}: not set", name)),
                    Err(e) => problems.push(e),
                }
            }
            if !problems.is_empty() {
                return Err(format!("missing or invalid secrets:\n  - {}", problems.join("\n  - ")));
            }
            for (k, v) in &found {
                remember(k, v);
            }
            if single {
                Ok(EvalValue::Str(found.into_values().next().unwrap_or_default()))
            } else {
                Ok(EvalValue::Dict(found.into_iter().map(|(k, v)| (k, EvalValue::Str(v))).collect()))
            }
        }
        // has(name) → bool
        "has" => {
            let name = name_arg("has", &args)?;
            Ok(EvalValue::Bool(lookup(&name)?.is_some()))
        }
        // mask(value) → "ab***yz" (short values become "***")
        "mask" => {
            let v = args.first().map(to_str).ok_or("secret.mask requires (value)")?;
            Ok(EvalValue::Str(mask(&v)))
        }
        // redact(text) → text with every known secret replaced by "***"
        "redact" => {
            let t = args.first().map(to_str).ok_or("secret.redact requires (text)")?;
            Ok(EvalValue::Str(redact(&t).into_owned()))
        }
        // all() → dict of the secrets read so far, masked
        "all" => {
            let names = NAMES.lock().unwrap().clone();
            let mut out = IndexMap::new();
            for n in names {
                if let Ok(Some(v)) = lookup(&n) {
                    out.insert(n, EvalValue::Str(mask(&v)));
                }
            }
            Ok(EvalValue::Dict(out))
        }
        // production() → bool: ORION_ENV is "production" or "prod"
        "production" => Ok(EvalValue::Bool(is_production())),
        f => Err(format!("secret.{}() does not exist", f)),
    }
}

/// Replaces every known secret in `text` with "***". Orion calls it on `show`,
/// runtime errors and `serve` logs, so a secret never reaches them in clear.
pub fn redact(text: &str) -> Cow<'_, str> {
    if !ACTIVE.load(Ordering::Relaxed) {
        return Cow::Borrowed(text);
    }
    let known = KNOWN.read().unwrap();
    if !known.iter().any(|v| text.contains(v.as_str())) {
        return Cow::Borrowed(text);
    }
    let mut s = text.to_string();
    for v in known.iter() {
        s = s.replace(v.as_str(), "***");
    }
    Cow::Owned(s)
}

pub fn is_production() -> bool {
    matches!(std::env::var("ORION_ENV").unwrap_or_default().trim().to_lowercase().as_str(),
             "production" | "prod")
}

/// Environment first, then `NAME_FILE`, then the loaded .env.
fn lookup(name: &str) -> Result<Option<String>, String> {
    if let Ok(v) = std::env::var(name) {
        if !v.is_empty() {
            return Ok(Some(v));
        }
    }
    if let Ok(path) = std::env::var(format!("{}_FILE", name)) {
        let v = std::fs::read_to_string(&path)
            .map_err(|e| format!("{}: cannot read {}_FILE ('{}'): {}", name, name, path, e))?;
        return Ok(Some(v.trim_end_matches(['\n', '\r']).to_string()));
    }
    // An empty value counts as not set, as it does for environment variables.
    Ok(DOTENV.lock().unwrap().as_ref().and_then(|m| m.get(name).cloned()).filter(|v| !v.is_empty()))
}

fn remember(name: &str, value: &str) {
    {
        let mut names = NAMES.lock().unwrap();
        if !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    if value.chars().count() < MIN_REDACT {
        return;
    }
    let mut known = KNOWN.write().unwrap();
    if !known.iter().any(|v| v == value) {
        known.push(value.to_string());
        // Longest first, so a secret that contains another is hidden whole.
        known.sort_by(|a, b| b.len().cmp(&a.len()));
    }
    ACTIVE.store(true, Ordering::Relaxed);
}

/// Counts characters, not bytes: slicing bytes panicked on "ñ" or "é".
fn mask(v: &str) -> String {
    let chars: Vec<char> = v.chars().collect();
    if chars.len() < 12 {
        return "***".into();
    }
    let start: String = chars[..2].iter().collect();
    let end: String = chars[chars.len() - 2..].iter().collect();
    format!("{}***{}", start, end)
}

/// KEY=value lines with optional `export`, "double" quotes (\n \" \\ escapes, may span
/// lines), 'single' quotes (literal) and `#` comments after unquoted values.
fn parse_dotenv(content: &str) -> Result<IndexMap<String, String>, String> {
    let mut map = IndexMap::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        i += 1;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (key, rest) = line.split_once('=')
            .ok_or_else(|| format!("line {}: expected KEY=value", i))?;
        let key = key.trim();
        let valid = key.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return Err(format!("line {}: invalid name '{}'", i, key));
        }
        let rest = rest.trim();
        let value = if let Some(first) = rest.strip_prefix('"') {
            // A double-quoted value may go on over several lines (PEM keys do).
            let start = i;
            let mut body = first.to_string();
            let mut out = String::new();
            loop {
                let mut chars = body.chars();
                let mut closed = false;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => { closed = true; break; }
                        '\\' => match chars.next() {
                            Some('n') => out.push('\n'),
                            Some('t') => out.push('\t'),
                            Some(o) => out.push(o),
                            None => break,
                        },
                        o => out.push(o),
                    }
                }
                if closed {
                    break;
                }
                if i >= lines.len() {
                    return Err(format!("line {}: missing closing \" for {}", start, key));
                }
                out.push('\n');
                body = lines[i].to_string();
                i += 1;
            }
            out
        } else if let Some(body) = rest.strip_prefix('\'') {
            body.split_once('\'').map(|(v, _)| v.to_string())
                .ok_or_else(|| format!("line {}: missing closing ' for {}", i, key))?
        } else {
            rest.split(" #").next().unwrap_or("").trim().to_string()
        };
        map.insert(key.to_string(), value);
    }
    Ok(map)
}

fn name_arg(f: &str, args: &[EvalValue]) -> Result<String, String> {
    args.first().map(to_str).filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("secret.{} requires (name)", f))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
