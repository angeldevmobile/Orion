use crate::eval_value::EvalValue;
use lopdf::{Document, Object, Dictionary, Stream, content::{Content, Operation}};
use printpdf::{PdfDocument, Mm, BuiltinFont};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // crear(path, texto) → Bool
        "crear" | "create" => {
            if args.len() < 2 { return Err("pdf.crear requiere (path, texto)".into()); }
            create_pdf(&to_str(&args[0]), &to_str(&args[1]))
        }
        // paginas(path) → Int
        "paginas" | "pages" => {
            let path = one_str("pdf.paginas", &args)?;
            let doc = Document::load(&path)
                .map_err(|e| format!("pdf.paginas '{}': {}", path, e))?;
            Ok(EvalValue::Int(doc.get_pages().len() as i64))
        }
        // plantilla(path, titulo, campos) → Bool   campos: Dict<Str,Str>
        "plantilla" | "template" => {
            if args.len() < 3 { return Err("pdf.plantilla requiere (path, titulo, campos)".into()); }
            let path   = to_str(&args[0]);
            let titulo = to_str(&args[1]);
            let campos = to_dict(&args[2]);
            create_template(&path, &titulo, &campos)
        }
        // reporte(path, titulo, filas) → Bool   filas: List<List<Str>>
        "reporte" | "report" => {
            if args.len() < 3 { return Err("pdf.reporte requiere (path, titulo, filas)".into()); }
            let path   = to_str(&args[0]);
            let titulo = to_str(&args[1]);
            let filas  = to_list_of_list(&args[2]);
            create_report(&path, &titulo, &filas)
        }
        // marca(path, salida, texto) → Bool
        "marca" | "watermark" => {
            if args.len() < 3 { return Err("pdf.marca requiere (path, salida, texto)".into()); }
            add_watermark(&to_str(&args[0]), &to_str(&args[1]), &to_str(&args[2]))
        }
        // paginar(path, salida, inicio, fin) → Bool   páginas 1-indexadas
        "paginar" | "paginate" => {
            if args.len() < 4 { return Err("pdf.paginar requiere (path, salida, inicio, fin)".into()); }
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
        f => Err(format!("pdf.{}() no existe", f)),
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
        .map_err(|e| format!("pdf.plantilla guardar: {}", e))?;
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
        .map_err(|e| format!("pdf.reporte guardar: {}", e))?;
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

    doc.save(salida).map_err(|e| format!("pdf.marca guardar: {}", e))?;
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
        return Err(format!("pdf.paginar: rango {}-{} inválido (total: {})", inicio, fin, total));
    }

    // Páginas a eliminar: antes de inicio y después de fin
    let to_delete: Vec<u32> = (1..inicio).chain((fin + 1)..=total).collect();
    if !to_delete.is_empty() {
        doc.delete_pages(&to_delete);
    }

    doc.save(salida).map_err(|e| format!("pdf.paginar guardar: {}", e))?;
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
    if args.is_empty() { return Err(format!("{} requiere (path)", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
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
