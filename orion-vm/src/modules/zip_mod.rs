use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // compress(src, dest) → crea un .zip (archivo o carpeta)
        "compress" => {
            if args.len() < 2 { return Err("zip.compress requiere (src, dest)".into()); }
            compress_zip(&to_str(&args[0]), &to_str(&args[1]))
        }
        // decompress(src, dest) / extract(src, dest)
        "decompress" | "extract" => {
            if args.len() < 2 { return Err("zip.decompress requiere (src, dest)".into()); }
            decompress_zip(&to_str(&args[0]), &to_str(&args[1]))
        }
        // list(path) → lista de entradas del zip
        "list" => {
            if args.is_empty() { return Err("zip.list requiere (path)".into()); }
            list_zip(&to_str(&args[0]))
        }
        // gzip(src, dest) → comprime un archivo con gzip
        "gzip" => {
            if args.len() < 2 { return Err("zip.gzip requiere (src, dest)".into()); }
            gzip_file(&to_str(&args[0]), &to_str(&args[1]))
        }
        // gunzip(src, dest) → descomprime un archivo gzip
        "gunzip" => {
            if args.len() < 2 { return Err("zip.gunzip requiere (src, dest)".into()); }
            gunzip_file(&to_str(&args[0]), &to_str(&args[1]))
        }
        f => Err(format!("zip.{}() no existe", f)),
    }
}

fn compress_zip(src: &str, dest: &str) -> Result<EvalValue, String> {
    let src_path = Path::new(src);
    if !src_path.exists() {
        return Err(format!("zip.compress: '{}' no existe", src));
    }
    let out_file = fs::File::create(dest)
        .map_err(|e| format!("zip.compress: {}", e))?;
    let mut writer = zip::ZipWriter::new(out_file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut count = 0i64;
    if src_path.is_dir() {
        add_dir(&mut writer, src_path, src_path, opts, &mut count)?;
    } else {
        add_file(&mut writer, src_path, opts, &mut count)?;
    }
    writer.finish().map_err(|e| format!("zip.compress: {}", e))?;
    Ok(EvalValue::Int(count))
}

fn add_dir(
    writer: &mut zip::ZipWriter<fs::File>,
    base:   &Path,
    dir:    &Path,
    opts:   zip::write::SimpleFileOptions,
    count:  &mut i64,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("zip.compress: {}", e))? {
        let entry = entry.map_err(|e| format!("zip.compress: {}", e))?;
        let path  = entry.path();
        if path.is_dir() {
            add_dir(writer, base, &path, opts, count)?;
        } else {
            add_file(writer, &path, opts, count)?;
        }
    }
    Ok(())
}

fn add_file(
    writer: &mut zip::ZipWriter<fs::File>,
    path:   &Path,
    opts:   zip::write::SimpleFileOptions,
    count:  &mut i64,
) -> Result<(), String> {
    let name = path.to_string_lossy().replace('\\', "/");
    writer.start_file(&name, opts)
        .map_err(|e| format!("zip.compress: {}", e))?;
    let mut buf = Vec::new();
    fs::File::open(path)
        .map_err(|e| format!("zip.compress: {}", e))?
        .read_to_end(&mut buf)
        .map_err(|e| format!("zip.compress: {}", e))?;
    writer.write_all(&buf)
        .map_err(|e| format!("zip.compress: {}", e))?;
    *count += 1;
    Ok(())
}

fn decompress_zip(src: &str, dest: &str) -> Result<EvalValue, String> {
    let file = fs::File::open(src)
        .map_err(|e| format!("zip.decompress '{}': {}", src, e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("zip.decompress: {}", e))?;

    let mut extracted = 0i64;
    for i in 0..archive.len() {
        let mut entry    = archive.by_index(i).map_err(|e| format!("zip.decompress: {}", e))?;
        let entry_name   = entry.name().to_string();
        let out_path     = Path::new(dest).join(&entry_name);

        if entry_name.ends_with('/') {
            fs::create_dir_all(&out_path).map_err(|e| format!("zip.decompress: {}", e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("zip.decompress: {}", e))?;
            }
            let mut out_file = fs::File::create(&out_path)
                .map_err(|e| format!("zip.decompress: {}", e))?;
            std::io::copy(&mut entry, &mut out_file)
                .map_err(|e| format!("zip.decompress: {}", e))?;
            extracted += 1;
        }
    }
    Ok(EvalValue::Int(extracted))
}

fn list_zip(path: &str) -> Result<EvalValue, String> {
    let file = fs::File::open(path)
        .map_err(|e| format!("zip.list '{}': {}", path, e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("zip.list: {}", e))?;

    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| format!("zip.list: {}", e))?;
        let mut info = HashMap::new();
        info.insert("name".into(), EvalValue::Str(entry.name().to_string()));
        info.insert("size".into(), EvalValue::Int(entry.size() as i64));
        info.insert("compressed".into(), EvalValue::Int(entry.compressed_size() as i64));
        info.insert("is_dir".into(), EvalValue::Bool(entry.is_dir()));
        entries.push(EvalValue::Dict(info));
    }
    Ok(EvalValue::List(entries))
}

fn gzip_file(src: &str, dest: &str) -> Result<EvalValue, String> {
    let mut input = fs::File::open(src)
        .map_err(|e| format!("zip.gzip '{}': {}", src, e))?;
    let output = fs::File::create(dest)
        .map_err(|e| format!("zip.gzip '{}': {}", dest, e))?;
    let mut encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
    std::io::copy(&mut input, &mut encoder)
        .map_err(|e| format!("zip.gzip: {}", e))?;
    encoder.finish().map_err(|e| format!("zip.gzip: {}", e))?;
    Ok(EvalValue::Bool(true))
}

fn gunzip_file(src: &str, dest: &str) -> Result<EvalValue, String> {
    let input = fs::File::open(src)
        .map_err(|e| format!("zip.gunzip '{}': {}", src, e))?;
    let mut decoder = flate2::read::GzDecoder::new(input);
    let mut output = fs::File::create(dest)
        .map_err(|e| format!("zip.gunzip '{}': {}", dest, e))?;
    std::io::copy(&mut decoder, &mut output)
        .map_err(|e| format!("zip.gunzip: {}", e))?;
    Ok(EvalValue::Bool(true))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
