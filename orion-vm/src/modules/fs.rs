use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::fs as std_fs;
use std::path::Path;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // --- Paths y utilidades ---
        "cwd" => {
            let path = std::env::current_dir().map_err(|e| e.to_string())?;
            Ok(EvalValue::Str(path.to_string_lossy().into_owned()))
        }
        "home" => {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".into());
            Ok(EvalValue::Str(home))
        }
        "join" => {
            let parts: Vec<String> = args.into_iter().map(|a| format!("{}", a)).collect();
            let mut path = std::path::PathBuf::new();
            for p in parts { path.push(p); }
            Ok(EvalValue::Str(path.to_string_lossy().into_owned()))
        }
        "exists" => {
            let path = one_str("exists", args)?;
            Ok(EvalValue::Bool(Path::new(&path).exists()))
        }
        "is_file" => {
            let path = one_str("is_file", args)?;
            Ok(EvalValue::Bool(Path::new(&path).is_file()))
        }
        "is_dir" => {
            let path = one_str("is_dir", args)?;
            Ok(EvalValue::Bool(Path::new(&path).is_dir()))
        }
        "ls" => {
            let path = if args.is_empty() { ".".into() } else { one_str("ls", args)? };
            let entries = std_fs::read_dir(&path).map_err(|e| e.to_string())?;
            let list: Vec<EvalValue> = entries
                .filter_map(|e| e.ok())
                .map(|e| EvalValue::Str(e.path().to_string_lossy().into_owned()))
                .collect();
            Ok(EvalValue::List(list))
        }
        "walk" => {
            let path = if args.is_empty() { ".".into() } else { one_str("walk", args)? };
            let mut result = Vec::new();
            walk_dir(Path::new(&path), &mut result);
            Ok(EvalValue::List(result.into_iter().map(EvalValue::Str).collect()))
        }

        // --- Archivos ---
        "read" => {
            let path = one_str("read", args)?;
            let content = std_fs::read_to_string(&path)
                .map_err(|e| format!("fs.read: {}", e))?;
            Ok(EvalValue::Str(content))
        }
        "write" => {
            if args.len() < 2 { return Err("fs.write requiere (path, content)".into()); }
            let path = eval_to_str(&args[0]);
            let content = eval_to_str(&args[1]);
            std_fs::write(&path, content).map_err(|e| format!("fs.write: {}", e))?;
            Ok(EvalValue::Null)
        }
        "append" => {
            if args.len() < 2 { return Err("fs.append requiere (path, content)".into()); }
            let path = eval_to_str(&args[0]);
            let content = eval_to_str(&args[1]);
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true).create(true).open(&path)
                .map_err(|e| format!("fs.append: {}", e))?;
            f.write_all(content.as_bytes()).map_err(|e| format!("fs.append: {}", e))?;
            Ok(EvalValue::Null)
        }
        "copy" => {
            if args.len() < 2 { return Err("fs.copy requiere (src, dst)".into()); }
            let src = eval_to_str(&args[0]);
            let dst = eval_to_str(&args[1]);
            std_fs::copy(&src, &dst).map_err(|e| format!("fs.copy: {}", e))?;
            Ok(EvalValue::Bool(true))
        }
        "move" => {
            if args.len() < 2 { return Err("fs.move requiere (src, dst)".into()); }
            let src = eval_to_str(&args[0]);
            let dst = eval_to_str(&args[1]);
            std_fs::rename(&src, &dst).map_err(|e| format!("fs.move: {}", e))?;
            Ok(EvalValue::Null)
        }
        "delete" => {
            let path = one_str("delete", args)?;
            if Path::new(&path).is_dir() {
                std_fs::remove_dir_all(&path).ok();
            } else {
                std_fs::remove_file(&path).ok();
            }
            Ok(EvalValue::Null)
        }
        "safe_write" => {
            if args.len() < 2 { return Err("fs.safe_write requiere (path, content)".into()); }
            let path = eval_to_str(&args[0]);
            let content = eval_to_str(&args[1]);
            let tmp = format!("{}.tmp", path);
            std_fs::write(&tmp, &content).map_err(|e| format!("fs.safe_write: {}", e))?;
            std_fs::rename(&tmp, &path).map_err(|e| format!("fs.safe_write rename: {}", e))?;
            Ok(EvalValue::Null)
        }
        "ensure" => {
            let path = one_str("ensure", args)?;
            if !Path::new(&path).exists() {
                std_fs::write(&path, "").map_err(|e| format!("fs.ensure: {}", e))?;
            }
            Ok(EvalValue::Str(path))
        }
        "backup" => {
            if args.is_empty() { return Err("fs.backup requiere (path)".into()); }
            let path = eval_to_str(&args[0]);
            let suffix = if args.len() > 1 { eval_to_str(&args[1]) } else { ".bak".into() };
            let dst = format!("{}{}", path, suffix);
            std_fs::copy(&path, &dst).map_err(|e| format!("fs.backup: {}", e))?;
            Ok(EvalValue::Str(dst))
        }

        // --- Directorios ---
        "mkdir" => {
            let path = one_str("mkdir", args)?;
            std_fs::create_dir_all(&path).map_err(|e| format!("fs.mkdir: {}", e))?;
            Ok(EvalValue::Null)
        }
        "rmdir" => {
            let path = one_str("rmdir", args)?;
            std_fs::remove_dir_all(&path).map_err(|e| format!("fs.rmdir: {}", e))?;
            Ok(EvalValue::Null)
        }
        "clear_dir" => {
            let path = one_str("clear_dir", args)?;
            for entry in std_fs::read_dir(&path).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let p = entry.path();
                if p.is_dir() { std_fs::remove_dir_all(&p).ok(); }
                else { std_fs::remove_file(&p).ok(); }
            }
            Ok(EvalValue::Null)
        }

        // --- Metadatos ---
        "info" => {
            let path = one_str("info", args)?;
            let p = Path::new(&path);
            let meta = std_fs::metadata(&path).map_err(|e| format!("fs.info: {}", e))?;
            let mut m = HashMap::new();
            m.insert("name".into(), EvalValue::Str(p.file_name().unwrap_or_default().to_string_lossy().into_owned()));
            m.insert("path".into(), EvalValue::Str(p.to_string_lossy().into_owned()));
            m.insert("size".into(), EvalValue::Int(meta.len() as i64));
            m.insert("is_file".into(), EvalValue::Bool(meta.is_file()));
            m.insert("is_dir".into(), EvalValue::Bool(meta.is_dir()));
            Ok(EvalValue::Dict(m))
        }
        "size" => {
            let path = one_str("size", args)?;
            let meta = std_fs::metadata(&path).map_err(|e| format!("fs.size: {}", e))?;
            Ok(EvalValue::Int(meta.len() as i64))
        }

        // guardar_b64(path, base64_str) → guarda binario decodificado de base64
        "guardar_b64" | "write_b64" => {
            if args.len() < 2 { return Err("fs.guardar_b64 requiere (path, base64_str)".into()); }
            let path   = eval_to_str(&args[0]);
            let b64    = eval_to_str(&args[1]);
            let bytes  = base64_decode(&b64)
                .map_err(|e| format!("fs.guardar_b64: base64 inválido: {}", e))?;
            // Crear directorios padre si no existen
            if let Some(parent) = Path::new(&path).parent() {
                if !parent.as_os_str().is_empty() {
                    std_fs::create_dir_all(parent)
                        .map_err(|e| format!("fs.guardar_b64: no se pudo crear directorio: {}", e))?;
                }
            }
            std_fs::write(&path, &bytes)
                .map_err(|e| format!("fs.guardar_b64: {}", e))?;
            Ok(EvalValue::Int(bytes.len() as i64))
        }

        // leer_b64(path) → lee archivo binario y retorna base64 string
        "leer_b64" | "read_b64" => {
            let path  = one_str("leer_b64", args)?;
            let bytes = std_fs::read(&path)
                .map_err(|e| format!("fs.leer_b64: {}", e))?;
            Ok(EvalValue::Str(base64_encode(&bytes)))
        }

        // mkdir(path) → crea directorio (y padres)
        "mkdir" => {
            let path = one_str("mkdir", args)?;
            std_fs::create_dir_all(&path)
                .map_err(|e| format!("fs.mkdir: {}", e))?;
            Ok(EvalValue::Bool(true))
        }

        f => Err(format!("fs.{}() no existe", f)),
    }
}

fn one_str(fn_name: &str, args: Vec<EvalValue>) -> Result<String, String> {
    if args.is_empty() {
        return Err(format!("fs.{}() requiere al menos 1 argumento", fn_name));
    }
    Ok(eval_to_str(&args[0]))
}

fn eval_to_str(v: &EvalValue) -> String {
    match v {
        EvalValue::Str(s) => s.clone(),
        other => format!("{}", other),
    }
}

const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            if chunk.len() > 1 { chunk[1] } else { 0 },
            if chunk.len() > 2 { chunk[2] } else { 0 },
        ];
        out.push(B64_CHARS[(b[0] >> 2) as usize] as char);
        out.push(B64_CHARS[((b[0] & 3) << 4 | b[1] >> 4) as usize] as char);
        out.push(if chunk.len() > 1 { B64_CHARS[((b[1] & 0xf) << 2 | b[2] >> 6) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64_CHARS[(b[2] & 0x3f) as usize] as char } else { '=' });
    }
    out
}

fn base64_decode(data: &str) -> Result<Vec<u8>, String> {
    let data = data.trim().replace('\n', "").replace('\r', "");
    if data.len() % 4 != 0 { return Err("longitud inválida".into()); }
    let mut table = [0u8; 256];
    for (i, &c) in B64_CHARS.iter().enumerate() { table[c as usize] = i as u8; }
    let mut out = Vec::with_capacity(data.len() / 4 * 3);
    for chunk in data.as_bytes().chunks(4) {
        let [a, b, c, d] = [
            table[chunk[0] as usize],
            table[chunk[1] as usize],
            if chunk[2] == b'=' { 0 } else { table[chunk[2] as usize] },
            if chunk[3] == b'=' { 0 } else { table[chunk[3] as usize] },
        ];
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' { out.push((b << 4) | (c >> 2)); }
        if chunk[3] != b'=' { out.push((c << 6) | d); }
    }
    Ok(out)
}

fn walk_dir(dir: &Path, results: &mut Vec<String>) {
    if let Ok(entries) = std_fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            results.push(path.to_string_lossy().into_owned());
            if path.is_dir() {
                walk_dir(&path, results);
            }
        }
    }
}
