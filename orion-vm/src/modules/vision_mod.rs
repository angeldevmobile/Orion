/// Orion Vision — procesamiento de imágenes usando la crate `image`.
/// Las imágenes se pasan como rutas de archivo.
/// Operaciones en memoria retornan base64 o escriben a nuevos archivos.
use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;
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
        // ocr(path, opts?) → String  — reconoce el texto de una imagen.
        // Por defecto usa el motor `ocrs` (Rust puro, local, sin externos).
        // opts = { "engine": "ocrs"|"tesseract", "lang": "spa" } — con "tesseract"
        // llama al binario del sistema si el developer lo tiene instalado.
        "ocr" | "leer_texto" | "read_text" => {
            if args.is_empty() { return Err("vision.ocr requires (path, opts?)".into()); }
            let path = to_str(&args[0]);
            let (engine, lang, prep) = ocr_opts(args.get(1));
            let text = match engine.as_str() {
                "tesseract" => ocr_tesseract(&path, &lang)?,
                _           => ocr_ocrs(&path, prep)?,
            };
            Ok(EvalValue::Str(text))
        }
        // threshold(path, out?) → out  — binariza (blanco/negro) con Otsu
        // automático. Ideal como pre-paso del OCR: limpia fondo y ruido.
        "threshold" | "umbral" | "binarize" => {
            if args.is_empty() { return Err("vision.threshold requires (path, out?)".into()); }
            let path = to_str(&args[0]);
            let out  = if args.len() > 1 { to_str(&args[1]) } else { format!("{}_bw.png", strip_ext(&path)) };
            let gray = open_img(&path)?.to_luma8();
            let level = imageproc::contrast::otsu_level(&gray);
            let bin = imageproc::contrast::threshold(&gray, level, imageproc::contrast::ThresholdType::Binary);
            bin.save(&out).map_err(|e| format!("vision.threshold: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // edges(path, out?, low?, high?) → out  — detección de bordes Canny
        "edges" | "bordes" | "canny" => {
            if args.is_empty() { return Err("vision.edges requires (path, out?, low?, high?)".into()); }
            let path = to_str(&args[0]);
            let out  = if args.len() > 1 { to_str(&args[1]) } else { format!("{}_edges.png", strip_ext(&path)) };
            let low  = if args.len() > 2 { to_f32(&args[2])? } else { 50.0 };
            let high = if args.len() > 3 { to_f32(&args[3])? } else { 150.0 };
            let gray = open_img(&path)?.to_luma8();
            let edges = imageproc::edges::canny(&gray, low, high);
            edges.save(&out).map_err(|e| format!("vision.edges: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // contrast(path, factor, out?) → out  — ajusta contraste (>0 aumenta)
        "contrast" | "contraste" => {
            if args.len() < 2 { return Err("vision.contrast requires (path, factor, out?)".into()); }
            let path   = to_str(&args[0]);
            let factor = to_f32(&args[1])?;
            let out    = if args.len() > 2 { to_str(&args[2]) } else { format!("{}_contrast.png", strip_ext(&path)) };
            let img = open_img(&path)?.adjust_contrast(factor);
            img.save(&out).map_err(|e| format!("vision.contrast: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // sharpen(path, out?) → out  — realce de nitidez (unsharp mask)
        "sharpen" | "nitidez" => {
            if args.is_empty() { return Err("vision.sharpen requires (path, out?)".into()); }
            let path = to_str(&args[0]);
            let out  = if args.len() > 1 { to_str(&args[1]) } else { format!("{}_sharp.png", strip_ext(&path)) };
            let img = open_img(&path)?.unsharpen(2.0, 5);
            img.save(&out).map_err(|e| format!("vision.sharpen: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // invert(path, out?) → out  — invierte los colores (negativo)
        "invert" | "invertir" => {
            if args.is_empty() { return Err("vision.invert requires (path, out?)".into()); }
            let path = to_str(&args[0]);
            let out  = if args.len() > 1 { to_str(&args[1]) } else { format!("{}_inv.png", strip_ext(&path)) };
            let mut img = open_img(&path)?;
            img.invert();
            img.save(&out).map_err(|e| format!("vision.invert: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // resize(path, width, height, out_path?) → out_path
        "resize" => {
            if args.len() < 3 { return Err("vision.resize requires (path, width, height, out_path?)".into()); }
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
            if args.len() < 3 { return Err("vision.resize_exact requires (path, w, h, out?)".into()); }
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
            if args.len() < 5 { return Err("vision.crop requires (path, x, y, w, h, out?)".into()); }
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
            if args.is_empty() { return Err("vision.blur requires (path, sigma?, out?)".into()); }
            let path  = to_str(&args[0]);
            let sigma = if args.len() > 1 { to_f32(&args[1])? } else { 2.0 };
            let out   = if args.len() > 2 { to_str(&args[2]) } else { format!("{}_blur.png", strip_ext(&path)) };
            let img   = open_img(&path)?;
            img.blur(sigma).save(&out).map_err(|e| format!("vision.blur: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // brighten(path, value, out?) → out_path (value: positive=brighter, negative=darker)
        "brighten" => {
            if args.is_empty() { return Err("vision.brighten requires (path, value, out?)".into()); }
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
            if args.len() < 2 { return Err("vision.from_base64 requires (b64_string, out_path)".into()); }
            let b64  = to_str(&args[0]);
            let out  = to_str(&args[1]);
            let bytes = b64_decode(&b64).map_err(|e| format!("vision.from_base64: {}", e))?;
            let img   = image::load_from_memory(&bytes).map_err(|e| format!("vision.from_base64: {}", e))?;
            img.save(&out).map_err(|e| format!("vision.from_base64: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // convert(path, out_path) → convierte formato por extensión
        "convert" => {
            if args.len() < 2 { return Err("vision.convert requires (path, out_path)".into()); }
            let path = to_str(&args[0]);
            let out  = to_str(&args[1]);
            open_img(&path)?.save(&out).map_err(|e| format!("vision.convert: {}", e))?;
            Ok(EvalValue::Str(out))
        }
        // thumbnail(path, max_dim, out?) → miniatura cuadrada
        "thumbnail" => {
            if args.is_empty() { return Err("vision.thumbnail requires (path, max_dim?, out?)".into()); }
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

        f => Err(format!("vision.{}() does not exist", f)),
    }
}

fn open_img(path: &str) -> Result<DynamicImage, String> {
    ImageReader::open(path)
        .map_err(|e| format!("vision: could not open '{}': {}", path, e))?
        .decode()
        .map_err(|e| format!("vision: could not decode '{}': {}", path, e))
}

fn strip_ext(path: &str) -> String {
    match path.rfind('.') {
        Some(i) => path[..i].to_string(),
        None    => path.to_string(),
    }
}

fn one_str(fn_name: &str, args: Vec<EvalValue>) -> Result<String, String> {
    if args.is_empty() { return Err(format!("vision.{}() requires 1 argument", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_u32(v: &EvalValue) -> Result<u32, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n as u32),
        EvalValue::Float(f) => Ok(*f as u32),
        other => Err(format!("vision: expected an integer, got {}", other.type_name())),
    }
}

fn to_i32(v: &EvalValue) -> Result<i32, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n as i32),
        EvalValue::Float(f) => Ok(*f as i32),
        other => Err(format!("vision: expected an integer, got {}", other.type_name())),
    }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("vision: expected an integer, got {}", other.type_name())),
    }
}

fn to_f32(v: &EvalValue) -> Result<f32, String> {
    match v {
        EvalValue::Float(f) => Ok(*f as f32),
        EvalValue::Int(n)   => Ok(*n as f32),
        other => Err(format!("vision: expected a float, got {}", other.type_name())),
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
            if v == 255 { Err(format!("invalid character: {}", c)) } else { Ok(v) }
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

//    OCR — reconocimiento de texto
//
// Motor por defecto: `ocrs` (Rust puro, redes ONNX corriendo local vía rten, sin
// Tesseract, sin API, sin internet). Los modelos van INCRUSTADOS en el binario
// (include_bytes) → OCR out-of-the-box, sin archivos sueltos que instalar.
// Motor opcional: Tesseract, solo si el developer lo tiene instalado y lo pide.

use std::sync::{Mutex, OnceLock};

// Modelos de OCR incrustados (detección de regiones + reconocimiento de texto).
static DETECTION_MODEL:   &[u8] = include_bytes!("../../models/text-detection.rten");
static RECOGNITION_MODEL: &[u8] = include_bytes!("../../models/text-recognition.rten");

/// Motor OCR inicializado una sola vez (cargar los modelos es caro). Tras Mutex
/// para compartirlo entre los workers de serve.
fn ocr_engine() -> Result<&'static Mutex<ocrs::OcrEngine>, String> {
    static ENGINE: OnceLock<Result<Mutex<ocrs::OcrEngine>, String>> = OnceLock::new();
    ENGINE.get_or_init(build_ocr_engine).as_ref().map_err(|e| e.clone())
}

fn build_ocr_engine() -> Result<Mutex<ocrs::OcrEngine>, String> {
    let det = rten::Model::load_static_slice(DETECTION_MODEL)
        .map_err(|e| format!("vision.ocr: invalid detection model: {}", e))?;
    let rec = rten::Model::load_static_slice(RECOGNITION_MODEL)
        .map_err(|e| format!("vision.ocr: invalid recognition model: {}", e))?;
    let engine = ocrs::OcrEngine::new(ocrs::OcrEngineParams {
        detection_model: Some(det),
        recognition_model: Some(rec),
        ..Default::default()
    }).map_err(|e| format!("vision.ocr: could not create el motor: {}", e))?;
    Ok(Mutex::new(engine))
}

/// OCR con el motor local `ocrs`, desde una ruta de archivo.
fn ocr_ocrs(path: &str, preprocess: bool) -> Result<String, String> {
    let img = image::open(path)
        .map_err(|e| format!("vision.ocr: could not open '{}': {}", path, e))?;
    ocr_dynamic(&img, preprocess)
}

/// OCR con `ocrs` desde bytes de imagen en memoria (usado por pdf.ocr).
pub(crate) fn ocr_image_bytes(data: &[u8], preprocess: bool) -> Result<String, String> {
    let img = image::load_from_memory(data)
        .map_err(|e| format!("vision.ocr: imagen inválida: {}", e))?;
    ocr_dynamic(&img, preprocess)
}

/// OCR sobre una imagen ya decodificada (usado por pdf.ocr al rasterizar).
pub(crate) fn ocr_dynamic_image(img: &image::DynamicImage, preprocess: bool) -> Result<String, String> {
    ocr_dynamic(img, preprocess)
}

/// Núcleo del OCR local sobre una imagen ya decodificada. Con `preprocess`,
/// convierte a gris y binariza con Otsu antes de leer — limpia fondo y ruido,
/// lo que reduce falsos positivos (las "E" fantasma) en documentos escaneados.
fn ocr_dynamic(img: &image::DynamicImage, preprocess: bool) -> Result<String, String> {
    let rgb = if preprocess {
        let gray = img.to_luma8();
        let level = imageproc::contrast::otsu_level(&gray);
        let bin = imageproc::contrast::threshold(&gray, level, imageproc::contrast::ThresholdType::Binary);
        image::DynamicImage::ImageLuma8(bin).to_rgb8()
    } else {
        img.to_rgb8()
    };
    let engine = ocr_engine()?.lock().unwrap();
    let (w, h) = rgb.dimensions();
    let source = ocrs::ImageSource::from_bytes(rgb.as_raw(), (w, h))
        .map_err(|e| format!("vision.ocr: {}", e))?;
    let input = engine.prepare_input(source)
        .map_err(|e| format!("vision.ocr: {}", e))?;
    engine.get_text(&input)
        .map_err(|e| format!("vision.ocr: {}", e))
}

/// OCR con Tesseract (binario del sistema), solo si el developer lo instaló.
fn ocr_tesseract(path: &str, lang: &str) -> Result<String, String> {
    let out = std::process::Command::new("tesseract")
        .arg(path).arg("stdout")
        .arg("-l").arg(lang)
        .output()
        .map_err(|e| format!(
            "vision.ocr(tesseract): no se encontró 'tesseract' en el PATH. \
             Instálalo o usa el motor por defecto (ocrs). ({})", e))?;
    if !out.status.success() {
        return Err(format!("vision.ocr(tesseract): {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Lee (engine, lang, preprocess) del Dict de opciones.
/// Defaults: motor "ocrs", idioma "eng", preprocess false.
fn ocr_opts(arg: Option<&EvalValue>) -> (String, String, bool) {
    let mut engine = "ocrs".to_string();
    let mut lang = "eng".to_string();
    let mut prep = false;
    if let Some(EvalValue::Dict(m)) = arg {
        if let Some(EvalValue::Str(s)) = m.get("engine").or_else(|| m.get("motor")) {
            engine = s.to_lowercase();
        }
        if let Some(EvalValue::Str(s)) = m.get("lang").or_else(|| m.get("idioma")) {
            lang = s.clone();
        }
        if let Some(v) = m.get("preprocess").or_else(|| m.get("preprocesar")) {
            prep = matches!(v, EvalValue::Bool(true)) || matches!(v, EvalValue::Int(n) if *n != 0);
        }
    }
    (engine, lang, prep)
}
