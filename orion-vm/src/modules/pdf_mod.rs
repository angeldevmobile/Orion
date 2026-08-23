use crate::eval_value::EvalValue;
use lopdf::{Document, Object, Dictionary, Stream, content::{Content, Operation}};
use printpdf::{PdfDocument, Mm, BuiltinFont};
use indexmap::IndexMap as HashMap;
use std::fs::File;
use std::io::BufWriter;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // create(path, texto) → Bool
        "create" | "crear" => {
            if args.len() < 2 { return Err("pdf.crear requires (path, texto)".into()); }
            create_pdf(&to_str(&args[0]), &to_str(&args[1]))
        }
        // pages(path) → Int
        "pages" | "paginas" => {
            let path = one_str("pdf.paginas", &args)?;
            let doc = Document::load(&path)
                .map_err(|e| format!("pdf.paginas '{}': {}", path, e))?;
            Ok(EvalValue::Int(doc.get_pages().len() as i64))
        }
        // template(path, titulo, campos) → Bool   campos: Dict<Str,Str>
        "template" | "plantilla" => {
            if args.len() < 3 { return Err("pdf.plantilla requires (path, titulo, campos)".into()); }
            let path   = to_str(&args[0]);
            let titulo = to_str(&args[1]);
            let campos = to_dict(&args[2]);
            create_template(&path, &titulo, &campos)
        }
        // report(path, titulo, filas) → Bool   filas: List<List<Str>>
        "report" | "reporte" => {
            if args.len() < 3 { return Err("pdf.reporte requires (path, titulo, filas)".into()); }
            let path   = to_str(&args[0]);
            let titulo = to_str(&args[1]);
            let filas  = to_list_of_list(&args[2]);
            create_report(&path, &titulo, &filas)
        }
        // watermark(path, salida, texto) → Bool
        "watermark" | "marca" => {
            if args.len() < 3 { return Err("pdf.marca requires (path, salida, texto)".into()); }
            add_watermark(&to_str(&args[0]), &to_str(&args[1]), &to_str(&args[2]))
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
        // info(path) → Dict
        "info" => {
            let path = one_str("pdf.info", &args)?;
            get_pdf_info(&path)
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

//    crear                                                                      

fn create_pdf(path: &str, text: &str) -> Result<EvalValue, String> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let mut font = Dictionary::new();
    font.set("Type",     Object::Name(b"Font".to_vec()));
    font.set("Subtype",  Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(font));

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Integer(12)]),
            Operation::new("Td", vec![Object::Integer(50), Object::Integer(750)]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_bytes = content.encode()
        .map_err(|e| format!("pdf.crear: {}", e))?;
    let content_id = doc.add_object(Stream::new(Dictionary::new(), content_bytes));

    let mut font_res = Dictionary::new();
    font_res.set("F1", Object::Reference(font_id));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(font_res));

    let mut page = Dictionary::new();
    page.set("Type",      Object::Name(b"Page".to_vec()));
    page.set("Parent",    Object::Reference(pages_id));
    page.set("MediaBox",  Object::Array(vec![
        Object::Integer(0), Object::Integer(0),
        Object::Integer(612), Object::Integer(792),
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

    doc.save(path).map_err(|e| format!("pdf.crear: {}", e))?;
    Ok(EvalValue::Bool(true))
}

//    plantilla                                                                  

fn create_template(path: &str, titulo: &str, campos: &[(String, String)]) -> Result<EvalValue, String> {
    let (doc, page1, layer1) = PdfDocument::new(titulo, Mm(210.0), Mm(297.0), "Capa 1");
    let layer = doc.get_page(page1).get_layer(layer1);

    let font = doc.add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("pdf.plantilla: {}", e))?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| format!("pdf.plantilla: {}", e))?;

    layer.use_text(titulo, 20.0, Mm(20.0), Mm(270.0), &font_bold);
    layer.use_text(
        "                                                    ",
        7.5, Mm(20.0), Mm(264.0), &font,
    );

    let mut y = 254.0_f32;
    for (clave, valor) in campos {
        layer.use_text(clave.as_str(), 11.0, Mm(20.0), Mm(y), &font_bold);
        layer.use_text(valor.as_str(), 11.0, Mm(80.0), Mm(y), &font);
        y -= 12.0;
        if y < 20.0 { break; }
    }

    let file = File::create(path).map_err(|e| format!("pdf.plantilla: {}", e))?;
    doc.save(&mut BufWriter::new(file))
        .map_err(|e| format!("pdf.plantilla save: {}", e))?;
    Ok(EvalValue::Bool(true))
}

//    reporte                                                                    

fn create_report(path: &str, titulo: &str, filas: &[Vec<String>]) -> Result<EvalValue, String> {
    let (doc, page1, layer1) = PdfDocument::new(titulo, Mm(210.0), Mm(297.0), "Capa 1");
    let layer = doc.get_page(page1).get_layer(layer1);

    let font = doc.add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("pdf.reporte: {}", e))?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| format!("pdf.reporte: {}", e))?;

    layer.use_text(titulo, 18.0, Mm(20.0), Mm(272.0), &font_bold);
    layer.use_text(
        "                                                    ",
        7.5, Mm(20.0), Mm(266.0), &font,
    );

    let col_w   = 38.0_f32;
    let row_h   =  8.0_f32;
    let max_col =  4usize;   // máximo 4 columnas en página A4 con margen 20mm
    let mut y   = 258.0_f32;

    for (i, fila) in filas.iter().enumerate() {
        let f = if i == 0 { &font_bold } else { &font };
        for (j, celda) in fila.iter().take(max_col).enumerate() {
            let x = 20.0_f32 + j as f32 * col_w;
            layer.use_text(celda.as_str(), 9.0, Mm(x), Mm(y), f);
        }
        y -= row_h;
        if y < 20.0 { break; }
    }

    let file = File::create(path).map_err(|e| format!("pdf.reporte: {}", e))?;
    doc.save(&mut BufWriter::new(file))
        .map_err(|e| format!("pdf.reporte save: {}", e))?;
    Ok(EvalValue::Bool(true))
}

//    marca (watermark)                                                          

fn add_watermark(path: &str, salida: &str, texto: &str) -> Result<EvalValue, String> {
    let mut doc = Document::load(path)
        .map_err(|e| format!("pdf.marca '{}': {}", path, e))?;

    // Un único objeto fuente Helvetica compartido entre todas las páginas
    let mut font_dict = Dictionary::new();
    font_dict.set("Type",     Object::Name(b"Font".to_vec()));
    font_dict.set("Subtype",  Object::Name(b"Type1".to_vec()));
    font_dict.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(font_dict));

    let page_ids: Vec<_> = doc.get_pages().values().copied().collect();

    for page_id in page_ids {
        // Stream de contenido con texto diagonal
        let wm_ops = Content {
            operations: vec![
                Operation::new("q", vec![]),
                // Rotar 45° y centrar en página carta (306, 396)
                Operation::new("cm", vec![
                    Object::Real(0.707), Object::Real(0.707),
                    Object::Real(-0.707), Object::Real(0.707),
                    Object::Integer(306), Object::Integer(396),
                ]),
                Operation::new("g",  vec![Object::Real(0.75)]),
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec![
                    Object::Name(b"WMFONT".to_vec()),
                    Object::Integer(48),
                ]),
                Operation::new("Tj", vec![Object::string_literal(texto)]),
                Operation::new("ET", vec![]),
                Operation::new("Q",  vec![]),
            ],
        };
        let wm_bytes = wm_ops.encode().map_err(|e| format!("pdf.marca encode: {}", e))?;
        let wm_id = doc.add_object(Stream::new(Dictionary::new(), wm_bytes));

        // Clonar página para evitar conflictos de borrow
        let page_clone = doc.objects.get(&page_id).cloned();
        if let Some(Object::Dictionary(mut page_dict)) = page_clone {
            // Actualizar Contents
            let old_contents = page_dict.get(b"Contents").ok().cloned();
            let new_contents = match old_contents {
                Some(Object::Reference(r))   =>
                    Object::Array(vec![Object::Reference(r), Object::Reference(wm_id)]),
                Some(Object::Array(mut arr)) => {
                    arr.push(Object::Reference(wm_id));
                    Object::Array(arr)
                }
                _ => Object::Reference(wm_id),
            };
            page_dict.set("Contents", new_contents);

            // Inyectar fuente en Resources
            let res_val = page_dict.get(b"Resources").ok().cloned();
            match res_val {
                Some(Object::Dictionary(mut rd)) => {
                    inject_font(&mut rd, font_id);
                    page_dict.set("Resources", Object::Dictionary(rd));
                }
                Some(Object::Reference(res_id)) => {
                    let res_clone = doc.objects.get(&res_id).cloned();
                    if let Some(Object::Dictionary(mut rd)) = res_clone {
                        inject_font(&mut rd, font_id);
                        if let Some(o) = doc.objects.get_mut(&res_id) {
                            *o = Object::Dictionary(rd);
                        }
                    }
                }
                _ => {
                    let mut fd = Dictionary::new();
                    fd.set("WMFONT", Object::Reference(font_id));
                    let mut rd = Dictionary::new();
                    rd.set("Font", Object::Dictionary(fd));
                    page_dict.set("Resources", Object::Dictionary(rd));
                }
            }

            // Escribir página modificada
            if let Some(o) = doc.objects.get_mut(&page_id) {
                *o = Object::Dictionary(page_dict);
            }
        }
    }

    doc.save(salida).map_err(|e| format!("pdf.marca save: {}", e))?;
    Ok(EvalValue::Bool(true))
}

fn inject_font(res: &mut Dictionary, font_id: (u32, u16)) {
    let font_val = res.get(b"Font").ok().cloned();
    match font_val {
        Some(Object::Dictionary(mut fd)) => {
            fd.set("WMFONT", Object::Reference(font_id));
            res.set("Font", Object::Dictionary(fd));
        }
        _ => {
            let mut fd = Dictionary::new();
            fd.set("WMFONT", Object::Reference(font_id));
            res.set("Font", Object::Dictionary(fd));
        }
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

//    info                                                                       

fn get_pdf_info(path: &str) -> Result<EvalValue, String> {
    let doc = Document::load(path)
        .map_err(|e| format!("pdf.info '{}': {}", path, e))?;

    let mut map: HashMap<String, EvalValue> = HashMap::new();
    map.insert("paginas".into(), EvalValue::Int(doc.get_pages().len() as i64));
    map.insert("version".into(), EvalValue::Str(doc.version.clone()));

    // Leer Info dictionary del trailer
    let info_ref = doc.trailer.get(b"Info").ok().and_then(|o| {
        if let Object::Reference(r) = o { Some(*r) } else { None }
    });

    if let Some(info_id) = info_ref {
        if let Some(Object::Dictionary(info)) = doc.objects.get(&info_id) {
            for key in &["Title", "Author", "Subject", "Keywords", "Creator", "Producer"] {
                if let Ok(Object::String(bytes, _)) = info.get(key.as_bytes()) {
                    let val = String::from_utf8_lossy(bytes).into_owned();
                    map.insert(key.to_lowercase(), EvalValue::Str(val));
                }
            }
        }
    }

    Ok(EvalValue::Dict(map))
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

fn to_dict(v: &EvalValue) -> Vec<(String, String)> {
    match v {
        EvalValue::Dict(map) => map.iter().map(|(k, v)| (k.clone(), to_str(v))).collect(),
        _ => vec![],
    }
}

fn to_list_of_list(v: &EvalValue) -> Vec<Vec<String>> {
    match v {
        EvalValue::List(rows) => rows.iter().map(|row| match row {
            EvalValue::List(cells) => cells.iter().map(|c| to_str(c)).collect(),
            other                  => vec![to_str(other)],
        }).collect(),
        _ => vec![],
    }
}
