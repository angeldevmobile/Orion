/// Orion Vision — procesamiento de imágenes usando la crate `image`.
/// Las imágenes se pasan como rutas de archivo.
/// Operaciones en memoria retornan base64 o escriben a nuevos archivos.
use crate::eval_value::EvalValue;
use std::collections::HashMap;
use image::{DynamicImage, ImageReader, imageops};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // info(path) → {width, height, format}
        "info" => {
            let path = one_str("info", args)?;
            let img = open_img(&path)?;
            let mut m = HashMap::new();
            m.insert("width".into(),  EvalValue::Int(img.width() as i64));
            m.insert("height".into(), EvalValue::Int(img.height() as i64));
            m.insert("path".into(),   EvalValue::Str(path));
            Ok(EvalValue::Dict(m))
        }
        // resize(path, width, height, out_path?) → out_path
        "resize" => {
            if args.len() < 3 { return Err("vision.resize requiere (path, width, height, out_path?)".into()); }
            let path = to_str(&args[0]);
            let w    = to_u32(&args[1])?;
            let h    = to_u32(&args[2])?;
            let out  = if args.len() > 3 { to_str(&args[3]) } else { format!("{}_resized.png", strip_ext(&path)) };
            let img  = open_img(&path)?;
            let resized = img.resize(w, h, imageops::FilterType::Lanczos3);
            resized.save(&out).map_err(|e| format!("vision.resize: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // resize_exact(path, w, h, out?) → out_path (sin mantener ratio)
        "resize_exact" => {
            if args.len() < 3 { return Err("vision.resize_exact requiere (path, w, h, out?)".into()); }
            let path = to_str(&args[0]);
            let w    = to_u32(&args[1])?;
            let h    = to_u32(&args[2])?;
            let out  = if args.len() > 3 { to_str(&args[3]) } else { format!("{}_exact.png", strip_ext(&path)) };
            let img  = open_img(&path)?;
            let resized = img.resize_exact(w, h, imageops::FilterType::Lanczos3);
            resized.save(&out).map_err(|e| format!("vision.resize_exact: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // crop(path, x, y, w, h, out?) → out_path
        "crop" => {
            if args.len() < 5 { return Err("vision.crop requiere (path, x, y, w, h, out?)".into()); }
            let path = to_str(&args[0]);
            let x    = to_u32(&args[1])?;
            let y    = to_u32(&args[2])?;
            let w    = to_u32(&args[3])?;
            let h    = to_u32(&args[4])?;
            let out  = if args.len() > 5 { to_str(&args[5]) } else { format!("{}_crop.png", strip_ext(&path)) };
            let mut img = open_img(&path)?;
            let cropped = img.crop(x, y, w, h);
            cropped.save(&out).map_err(|e| format!("vision.crop: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // grayscale(path, out?) → out_path
        "grayscale" | "gray" => {
            let path = one_str(function, args.clone())?;
            let out  = if args.len() > 1 { to_str(&args[1]) } else { format!("{}_gray.png", strip_ext(&path)) };
            let img  = open_img(&path)?;
            img.grayscale().save(&out).map_err(|e| format!("vision.grayscale: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // blur(path, sigma, out?) → out_path
        "blur" => {
            if args.is_empty() { return Err("vision.blur requiere (path, sigma?, out?)".into()); }
            let path  = to_str(&args[0]);
            let sigma = if args.len() > 1 { to_f32(&args[1])? } else { 2.0 };
            let out   = if args.len() > 2 { to_str(&args[2]) } else { format!("{}_blur.png", strip_ext(&path)) };
            let img   = open_img(&path)?;
            img.blur(sigma).save(&out).map_err(|e| format!("vision.blur: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // brighten(path, value, out?) → out_path (value: positive=brighter, negative=darker)
        "brighten" => {
            if args.is_empty() { return Err("vision.brighten requiere (path, value, out?)".into()); }
            let path  = to_str(&args[0]);
            let value = if args.len() > 1 { to_i32(&args[1])? } else { 20 };
            let out   = if args.len() > 2 { to_str(&args[2]) } else { format!("{}_bright.png", strip_ext(&path)) };
            let img   = open_img(&path)?;
            img.brighten(value).save(&out).map_err(|e| format!("vision.brighten: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // flip_h(path, out?) → out_path
        "flip_h" | "flip_horizontal" => {
            let path = one_str(function, args.clone())?;
            let out  = if args.len() > 1 { to_str(&args[1]) } else { format!("{}_fliph.png", strip_ext(&path)) };
            let img  = open_img(&path)?;
            img.fliph().save(&out).map_err(|e| format!("vision.flip_h: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // flip_v(path, out?) → out_path
        "flip_v" | "flip_vertical" => {
            let path = one_str(function, args.clone())?;
            let out  = if args.len() > 1 { to_str(&args[1]) } else { format!("{}_flipv.png", strip_ext(&path)) };
            let img  = open_img(&path)?;
            img.flipv().save(&out).map_err(|e| format!("vision.flip_v: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // rotate90(path, out?) → out_path
        "rotate90" => {
            let path = one_str(function, args.clone())?;
            let out  = if args.len() > 1 { to_str(&args[1]) } else { format!("{}_rot90.png", strip_ext(&path)) };
            let img  = open_img(&path)?;
            img.rotate90().save(&out).map_err(|e| format!("vision.rotate90: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        "rotate180" => {
            let path = one_str(function, args.clone())?;
            let out  = if args.len() > 1 { to_str(&args[1]) } else { format!("{}_rot180.png", strip_ext(&path)) };
            let img  = open_img(&path)?;
            img.rotate180().save(&out).map_err(|e| format!("vision.rotate180: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        "rotate270" => {
            let path = one_str(function, args.clone())?;
            let out  = if args.len() > 1 { to_str(&args[1]) } else { format!("{}_rot270.png", strip_ext(&path)) };
            let img  = open_img(&path)?;
            img.rotate270().save(&out).map_err(|e| format!("vision.rotate270: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // to_base64(path) → string base64 PNG
        "to_base64" | "encode" => {
            let path = one_str(function, args)?;
            let img  = open_img(&path)?;
            let mut buf = std::io::Cursor::new(Vec::new());
            img.write_to(&mut buf, image::ImageFormat::Png)
                .map_err(|e| format!("vision.to_base64: {}", e))?;
            Ok(EvalValue::Str(b64_encode(buf.get_ref())))
        }
        // from_base64(b64_string, out_path) → guarda imagen
        "from_base64" | "decode" => {
            if args.len() < 2 { return Err("vision.from_base64 requiere (b64_string, out_path)".into()); }
            let b64  = to_str(&args[0]);
            let out  = to_str(&args[1]);
            let bytes = b64_decode(&b64).map_err(|e| format!("vision.from_base64: {}", e))?;
            let img   = image::load_from_memory(&bytes).map_err(|e| format!("vision.from_base64: {}", e))?;
            img.save(&out).map_err(|e| format!("vision.from_base64: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // convert(path, out_path) → convierte formato por extensión
        "convert" => {
            if args.len() < 2 { return Err("vision.convert requiere (path, out_path)".into()); }
            let path = to_str(&args[0]);
            let out  = to_str(&args[1]);
            open_img(&path)?.save(&out).map_err(|e| format!("vision.convert: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // thumbnail(path, max_dim, out?) → miniatura cuadrada
        "thumbnail" => {
            if args.is_empty() { return Err("vision.thumbnail requiere (path, max_dim?, out?)".into()); }
            let path    = to_str(&args[0]);
            let max_dim = if args.len() > 1 { to_u32(&args[1])? } else { 128 };
            let out     = if args.len() > 2 { to_str(&args[2]) } else { format!("{}_thumb.png", strip_ext(&path)) };
            let img     = open_img(&path)?;
            img.thumbnail(max_dim, max_dim).save(&out).map_err(|e| format!("vision.thumbnail: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // pixels_sample(path, n?) → lista de n píxeles [r, g, b, a] muestreados
        "pixels_sample" | "sample_pixels" => {
            let path = one_str(function, args.clone())?;
            let n    = if args.len() > 1 { to_i64(&args[1])? as usize } else { 10 };
            let img  = open_img(&path)?.to_rgba8();
            let (w, h) = img.dimensions();
            let total = (w * h) as usize;
            let step  = (total / n).max(1);
            let pixels: Vec<EvalValue> = img.pixels()
                .step_by(step)
                .take(n)
                .map(|p| EvalValue::List(vec![
                    EvalValue::Int(p[0] as i64),
                    EvalValue::Int(p[1] as i64),
                    EvalValue::Int(p[2] as i64),
                    EvalValue::Int(p[3] as i64),
                ]))
                .collect();
            Ok(EvalValue::List(pixels))
        }

        f => Err(format!("vision.{}() no existe", f)),
    }
}

fn open_img(path: &str) -> Result<DynamicImage, String> {
    ImageReader::open(path)
        .map_err(|e| format!("vision: no se pudo abrir '{}': {}", path, e))?
        .decode()
        .map_err(|e| format!("vision: no se pudo decodificar '{}': {}", path, e))
}

fn strip_ext(path: &str) -> String {
    match path.rfind('.') {
        Some(i) => path[..i].to_string(),
        None    => path.to_string(),
    }
}

fn one_str(fn_name: &str, args: Vec<EvalValue>) -> Result<String, String> {
    if args.is_empty() { return Err(format!("vision.{}() requiere 1 argumento", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_u32(v: &EvalValue) -> Result<u32, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n as u32),
        EvalValue::Float(f) => Ok(*f as u32),
        other => Err(format!("vision: esperaba entero, recibió {}", other.type_name())),
    }
}

fn to_i32(v: &EvalValue) -> Result<i32, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n as i32),
        EvalValue::Float(f) => Ok(*f as i32),
        other => Err(format!("vision: esperaba entero, recibió {}", other.type_name())),
    }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("vision: esperaba entero, recibió {}", other.type_name())),
    }
}

fn to_f32(v: &EvalValue) -> Result<f32, String> {
    match v {
        EvalValue::Float(f) => Ok(*f as f32),
        EvalValue::Int(n)   => Ok(*n as f32),
        other => Err(format!("vision: esperaba float, recibió {}", other.type_name())),
    }
}

// Base64 sin dep externa
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

fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut table = [255u8; 256];
    for (i, &c) in B64.iter().enumerate() { table[c as usize] = i as u8; }
    let clean: Vec<u8> = input.chars().filter(|c| !c.is_whitespace() && *c != '=')
        .map(|c| {
            let v = table[c as usize];
            if v == 255 { Err(format!("carácter inválido: {}", c)) } else { Ok(v) }
        })
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
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
