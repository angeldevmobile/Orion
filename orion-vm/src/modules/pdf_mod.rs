use crate::eval_value::EvalValue;
use lopdf::{Document, Object, Dictionary, Stream, content::{Content, Operation}};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // create(path, texto, opts?) → Bool  — texto corrido, partido al ancho
        // y en tantas páginas como haga falta. opts: las de página y font_size.
        "create" | "crear" => {
            if args.len() < 2 { return Err("pdf.create requires (path, text, opts?)".into()); }
            super::pdf_layout::texto_corrido(&to_str(&args[0]), &to_str(&args[1]), args.get(2))
        }
        // build(path, bloques, opts?) → Bool  — documento libre por bloques:
        // title, heading, text, table, fields, image, line, space, page_break.
        // opts: size, orientation, margin, header, footer, page_numbers,
        // page_format, title, author, subject.
        "build" | "construir" => {
            if args.len() < 2 { return Err("pdf.build requires (path, blocks, opts?)".into()); }
            super::pdf_layout::construir(&to_str(&args[0]), &args[1], args.get(2))
        }
        // pages(path) → Int
        "pages" | "paginas" => {
            let path = one_str("pdf.paginas", &args)?;
            let doc = Document::load(&path)
                .map_err(|e| format!("pdf.paginas '{}': {}", path, e))?;
            Ok(EvalValue::Int(doc.get_pages().len() as i64))
        }
        // template(path, titulo, campos, opts?) → Bool   campos: Dict
        "template" | "plantilla" => {
            if args.len() < 3 { return Err("pdf.template requires (path, title, fields, opts?)".into()); }
            super::pdf_layout::ficha(&to_str(&args[0]), &to_str(&args[1]), &args[2], args.get(3))
        }
        // report(path, titulo, filas, opts?) → Bool
        // filas: List<List> (la primera es la cabecera) o List<Dict>.
        // opts: las de página, subtitle, columns ({ nombre: { width, align,
        // format, bold, color, title, hidden } }), font_size, header_bg,
        // header_color, zebra, borders, total, decimal, thousands.
        "report" | "reporte" => {
            if args.len() < 3 { return Err("pdf.report requires (path, titulo, filas, opts?)".into()); }
            super::pdf_layout::reporte(&to_str(&args[0]), &to_str(&args[1]), &args[2], args.get(3))
        }
        // watermark(path, salida, texto, opts?) → Bool  — texto grande,
        // girado y semitransparente en todas las páginas. opts: las de stamp.
        "watermark" | "marca" => {
            if args.len() < 3 { return Err("pdf.watermark requires (path, output, text, opts?)".into()); }
            super::pdf_edit::marca_agua(&to_str(&args[0]), &to_str(&args[1]), &to_str(&args[2]), args.get(3))
        }
        // stamp(path, salida, texto, opts?) → Bool  — texto encima de páginas
        // existentes. opts: pages, position, x, y, margin, size, bold, color,
        // opacity, rotation. {page} y {pages} se sustituyen.
        "stamp" | "estampar" => {
            if args.len() < 3 { return Err("pdf.stamp requires (path, output, text, opts?)".into()); }
            super::pdf_edit::estampar(&to_str(&args[0]), &to_str(&args[1]), &to_str(&args[2]), args.get(3))
        }
        // stamp_image(path, salida, imagen, opts?) → Bool  — logo o firma
        // encima de páginas existentes. opts: pages, position, x, y, margin,
        // width, height, opacity.
        "stamp_image" | "estampar_imagen" => {
            if args.len() < 3 { return Err("pdf.stamp_image requires (path, output, image, opts?)".into()); }
            super::pdf_edit::estampar_imagen(&to_str(&args[0]), &to_str(&args[1]), &to_str(&args[2]), args.get(3))
        }
        // merge([paths], salida) → Bool  — une varios PDF en uno.
        "merge" | "unir" => {
            if args.len() < 2 { return Err("pdf.merge requires ([paths], output)".into()); }
            super::pdf_edit::unir(&args[0], &to_str(&args[1]))
        }
        // delete_pages(path, salida, paginas) → Bool  — paginas: 3, [1, 4],
        // "2-5,8" o -1 (la última).
        "delete_pages" | "quitar_paginas" => {
            if args.len() < 3 { return Err("pdf.delete_pages requires (path, output, pages)".into()); }
            super::pdf_edit::quitar(&to_str(&args[0]), &to_str(&args[1]), &args[2])
        }
        // reorder(path, salida, orden) → Bool  — p. ej. [3, 1, 2].
        "reorder" | "reordenar" => {
            if args.len() < 3 { return Err("pdf.reorder requires (path, output, order)".into()); }
            super::pdf_edit::reordenar(&to_str(&args[0]), &to_str(&args[1]), &args[2])
        }
        // rotate(path, salida, grados, paginas?) → Bool  — múltiplos de 90.
        "rotate" | "rotar" => {
            if args.len() < 3 { return Err("pdf.rotate requires (path, output, degrees, pages?)".into()); }
            super::pdf_edit::rotar(&to_str(&args[0]), &to_str(&args[1]), &args[2], args.get(3))
        }
        // set_info(path, salida, { title, author, subject, keywords, … }) → Bool
        "set_info" | "fijar_info" => {
            if args.len() < 3 { return Err("pdf.set_info requires (path, output, info)".into()); }
            super::pdf_edit::fijar_info(&to_str(&args[0]), &to_str(&args[1]), &args[2])
        }
        // paginate(path, salida, inicio, fin) → Bool   páginas 1-indexadas
        "paginate" | "paginar" => {
            if args.len() < 4 { return Err("pdf.paginar requires (path, salida, inicio, fin)".into()); }
            let path   = to_str(&args[0]);
            let salida = to_str(&args[1]);
            let inicio = to_int(&args[2]) as u32;
            let fin    = to_int(&args[3]) as u32;
            extract_pages(&path, &salida, inicio, fin)
        }
        // info(path) → Dict { pages, version, title, author, subject, … }
        "info" => {
            let path = one_str("pdf.info", &args)?;
            super::pdf_edit::info(&path)
        }
        // read(path) → String  — extrae el texto embebido del PDF (PDFs de texto).
        // Para PDFs escaneados (solo imágenes) usar pdf.ocr.
        "read" | "leer" | "extraer_texto" | "extract_text" => {
            let path = one_str("pdf.leer", &args)?;
            let text = pdf_extract::extract_text(&path)
                .map_err(|e| format!("pdf.leer '{}': {}", path, e))?;
            Ok(EvalValue::Str(text))
        }
        // ocr(path, opts?) → String  — OCR de un PDF escaneado: extrae las
        // imágenes embebidas de cada página y las pasa por el motor de vision.
        "ocr" => {
            if args.is_empty() { return Err("pdf.ocr requires (path, opts?)".into()); }
            ocr_pdf(&to_str(&args[0]), args.get(1))
        }
        // text(path) → String  — inteligente: intenta el texto embebido; si el
        // PDF no tiene texto (escaneado), cae automáticamente a OCR.
        "text" | "texto" => {
            let path = one_str("pdf.texto", &args)?;
            let embedded = pdf_extract::extract_text(&path).unwrap_or_default();
            if embedded.trim().len() >= 8 {
                Ok(EvalValue::Str(embedded))
            } else {
                ocr_pdf(&path, None)
            }
        }
        // from_image(imagen, salida_pdf) → salida  — convierte una imagen a PDF.
        "from_image" | "desde_imagen" => {
            if args.len() < 2 { return Err("pdf.desde_imagen requires (imagen, salida_pdf)".into()); }
            image_to_pdf(&to_str(&args[0]), &to_str(&args[1]))
        }
        f => Err(format!("pdf.{}() does not exist", f)),
    }
}

//    paginar                                                                    

fn extract_pages(path: &str, salida: &str, inicio: u32, fin: u32) -> Result<EvalValue, String> {
    let mut doc = Document::load(path)
        .map_err(|e| format!("pdf.paginar '{}': {}", path, e))?;

    let total = doc.get_pages().len() as u32;
    let inicio = inicio.max(1);
    let fin    = fin.min(total);

    if inicio > fin {
        return Err(format!("pdf.paginar: invalid range {}-{} (total: {})", inicio, fin, total));
    }

    // Páginas a eliminar: antes de inicio y después de fin
    let to_delete: Vec<u32> = (1..inicio).chain((fin + 1)..=total).collect();
    if !to_delete.is_empty() {
        doc.delete_pages(&to_delete);
    }

    doc.save(salida).map_err(|e| format!("pdf.paginar save: {}", e))?;
    Ok(EvalValue::Bool(true))
}

//    utilidades                                                                 

fn one_str(fn_name: &str, args: &[EvalValue]) -> Result<String, String> {
    if args.is_empty() { return Err(format!("{} requires (path)", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

//    Conversión imagen → PDF (embebe la imagen como JPEG/DCTDecode)

fn image_to_pdf(img_path: &str, out: &str) -> Result<EvalValue, String> {
    use std::io::Cursor;
    let img = image::open(img_path)
        .map_err(|e| format!("pdf.desde_imagen: could not open '{}': {}", img_path, e))?;
    let (w, h) = (img.width(), img.height());
    // Codificar a JPEG → va directo como stream DCTDecode (sin recomprimir en PDF).
    let mut jpeg = Vec::new();
    image::DynamicImage::ImageRgb8(img.to_rgb8())
        .write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
        .map_err(|e| format!("pdf.desde_imagen: {}", e))?;

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    // XObject imagen
    let mut xdict = Dictionary::new();
    xdict.set("Type",             Object::Name(b"XObject".to_vec()));
    xdict.set("Subtype",          Object::Name(b"Image".to_vec()));
    xdict.set("Width",            Object::Integer(w as i64));
    xdict.set("Height",           Object::Integer(h as i64));
    xdict.set("ColorSpace",       Object::Name(b"DeviceRGB".to_vec()));
    xdict.set("BitsPerComponent", Object::Integer(8));
    xdict.set("Filter",           Object::Name(b"DCTDecode".to_vec()));
    let img_id = doc.add_object(Stream::new(xdict, jpeg));

    // Contenido: dibuja la imagen ocupando toda la página (cm = escala).
    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new("cm", vec![
                Object::Integer(w as i64), Object::Integer(0),
                Object::Integer(0),        Object::Integer(h as i64),
                Object::Integer(0),        Object::Integer(0),
            ]),
            Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
            Operation::new("Q", vec![]),
        ],
    };
    let content_bytes = content.encode().map_err(|e| format!("pdf.desde_imagen: {}", e))?;
    let content_id = doc.add_object(Stream::new(Dictionary::new(), content_bytes));

    let mut xobjects = Dictionary::new();
    xobjects.set("Im0", Object::Reference(img_id));
    let mut resources = Dictionary::new();
    resources.set("XObject", Object::Dictionary(xobjects));

    let mut page = Dictionary::new();
    page.set("Type",      Object::Name(b"Page".to_vec()));
    page.set("Parent",    Object::Reference(pages_id));
    page.set("MediaBox",  Object::Array(vec![
        Object::Integer(0), Object::Integer(0),
        Object::Integer(w as i64), Object::Integer(h as i64),
    ]));
    page.set("Contents",  Object::Reference(content_id));
    page.set("Resources", Object::Dictionary(resources));
    let page_id = doc.add_object(Object::Dictionary(page));

    let mut pages = Dictionary::new();
    pages.set("Type",  Object::Name(b"Pages".to_vec()));
    pages.set("Kids",  Object::Array(vec![Object::Reference(page_id)]));
    pages.set("Count", Object::Integer(1));
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let mut catalog = Dictionary::new();
    catalog.set("Type",  Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    doc.save(out).map_err(|e| format!("pdf.desde_imagen: {}", e))?;
    Ok(EvalValue::Str(out.to_string()))
}

//    OCR de PDF escaneado: extrae las imágenes DCTDecode/JPEG y las pasa por vision

fn ocr_pdf(path: &str, _opts: Option<&EvalValue>) -> Result<EvalValue, String> {
    let doc = Document::load(path)
        .map_err(|e| format!("pdf.ocr '{}': {}", path, e))?;

    // Recorrer objetos por id (orden estable) buscando XObjects de imagen JPEG.
    let mut ids: Vec<_> = doc.objects.keys().cloned().collect();
    ids.sort();

    let mut partes: Vec<String> = Vec::new();
    for id in ids {
        if let Some(Object::Stream(stream)) = doc.objects.get(&id) {
            let dict = &stream.dict;
            let is_image = dict.get(b"Subtype").ok()
                .and_then(|o| o.as_name().ok())
                .map(|n| n == b"Image").unwrap_or(false);
            if !is_image { continue; }
            if !has_filter(dict, b"DCTDecode") { continue; }
            // El contenido crudo de un stream DCTDecode ES un JPEG → OCR directo.
            // preprocess=true: binariza antes de leer (menos ruido en escaneos).
            if let Ok(text) = crate::modules::vision_mod::ocr_image_bytes(&stream.content, true) {
                let t = text.trim();
                if !t.is_empty() { partes.push(t.to_string()); }
            }
        }
    }

    // Sin imágenes JPEG embebidas → PDF vectorial/de texto: rasterizamos cada
    // página con pdfium y hacemos OCR del render. Cubre CUALQUIER PDF.
    if partes.is_empty() {
        for img in rasterize_pdf(path)? {
            if let Ok(text) = crate::modules::vision_mod::ocr_dynamic_image(&img, true) {
                let t = text.trim();
                if !t.is_empty() { partes.push(t.to_string()); }
            }
        }
    }

    if partes.is_empty() {
        return Err("pdf.ocr: could not extract text from the PDF with OCR.".into());
    }
    Ok(EvalValue::Str(partes.join("\n")))
}

//    Rasterización de PDF con pdfium (binario incrustado, self-contained)
//
// El binario de pdfium correspondiente a la plataforma va INCRUSTADO en Orion
// (include_bytes) y se extrae a un temporal en el 1er uso. Lo ÚNICO específico
// de cada SO es qué binario se incrusta (pdfium_blob); la lógica de rasterizado
// es compartida. Soporta Windows/Linux x64 y macOS arm64/x64.

// Selección del binario por plataforma (lo único que cambia entre SOs).
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn pdfium_blob() -> (&'static [u8], &'static str) {
    (include_bytes!("../../models/pdfium.dll"), "pdfium.dll")
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn pdfium_blob() -> (&'static [u8], &'static str) {
    (include_bytes!("../../models/libpdfium.so"), "libpdfium.so")
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn pdfium_blob() -> (&'static [u8], &'static str) {
    (include_bytes!("../../models/libpdfium-arm64.dylib"), "libpdfium.dylib")
}
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn pdfium_blob() -> (&'static [u8], &'static str) {
    (include_bytes!("../../models/libpdfium-x64.dylib"), "libpdfium.dylib")
}

// Lógica COMPARTIDA (validada en Windows): escribe el binario a un temporal y
// rasteriza. Gateada a las plataformas con binario disponible.
#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux",   target_arch = "x86_64"),
    all(target_os = "macos",   target_arch = "aarch64"),
    all(target_os = "macos",   target_arch = "x86_64"),
))]
fn ensure_pdfium() -> Result<std::path::PathBuf, String> {
    let (bytes, name) = pdfium_blob();
    let dir = std::env::temp_dir().join("orion_pdfium");
    let lib = dir.join(name);
    if !lib.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("pdf.ocr: {}", e))?;
        std::fs::write(&lib, bytes).map_err(|e| format!("pdf.ocr: {}", e))?;
    }
    Ok(lib)
}

/// Renderiza cada página del PDF a una imagen (RAM constante por página).
#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux",   target_arch = "x86_64"),
    all(target_os = "macos",   target_arch = "aarch64"),
    all(target_os = "macos",   target_arch = "x86_64"),
))]
fn rasterize_pdf(path: &str) -> Result<Vec<image::DynamicImage>, String> {
    use pdfium_render::prelude::*;
    let lib = ensure_pdfium()?;
    let bindings = Pdfium::bind_to_library(&lib)
        .map_err(|e| format!("pdf.ocr: could not load pdfium: {}", e))?;
    let pdfium = Pdfium::new(bindings);
    let doc = pdfium.load_pdf_from_file(path, None)
        .map_err(|e| format!("pdf.ocr: {}", e))?;
    // 2000px de ancho → buena resolución para OCR sin inflar memoria.
    let cfg = PdfRenderConfig::new().set_target_width(2000);
    let mut out = Vec::new();
    for page in doc.pages().iter() {
        let bmp = page.render_with_config(&cfg)
            .map_err(|e| format!("pdf.ocr render: {}", e))?;
        let img = bmp.as_image()
            .map_err(|e| format!("pdf.ocr as_image: {}", e))?;
        out.push(img);
    }
    Ok(out)
}

// Plataformas sin binario pdfium incrustado (p. ej. ARM Linux, Windows ARM):
// pdf.ocr sigue funcionando con imágenes JPEG embebidas, solo no rasteriza.
#[cfg(not(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux",   target_arch = "x86_64"),
    all(target_os = "macos",   target_arch = "aarch64"),
    all(target_os = "macos",   target_arch = "x86_64"),
)))]
fn rasterize_pdf(_path: &str) -> Result<Vec<image::DynamicImage>, String> {
    Err("pdf.ocr: la rasterización de PDF (pdfium) no está disponible en esta \
         plataforma/arquitectura. El OCR de imágenes embebidas sí funciona.".into())
}

/// ¿El diccionario del stream declara `name` en su Filter (nombre o array)?
fn has_filter(dict: &Dictionary, name: &[u8]) -> bool {
    match dict.get(b"Filter") {
        Ok(Object::Name(n)) => n.as_slice() == name,
        Ok(Object::Array(arr)) => arr.iter().any(|o| o.as_name().map(|n| n == name).unwrap_or(false)),
        _ => false,
    }
}

fn to_int(v: &EvalValue) -> i64 {
    match v {
        EvalValue::Int(n)   => *n,
        EvalValue::Float(f) => *f as i64,
        EvalValue::Str(s)   => s.parse().unwrap_or(0),
        _                   => 0,
    }
}


