use crate::eval_value::EvalValue;
use lopdf::{Document, Object, Dictionary, Stream, content::{Content, Operation}};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // crear(path, texto) → Bool  — genera un PDF de una página con texto
        "crear" | "create" => {
            if args.len() < 2 { return Err("pdf.crear requiere (path, texto)".into()); }
            let path  = to_str(&args[0]);
            let texto = to_str(&args[1]);
            create_pdf(&path, &texto)
        }
        // paginas(path) → Int  — número de páginas de un PDF existente
        "paginas" | "pages" => {
            let path = one_str("pdf.paginas", &args)?;
            let doc  = Document::load(&path)
                .map_err(|e| format!("pdf.paginas '{}': {}", path, e))?;
            Ok(EvalValue::Int(doc.get_pages().len() as i64))
        }
        f => Err(format!("pdf.{}() no existe", f)),
    }
}

fn create_pdf(path: &str, text: &str) -> Result<EvalValue, String> {
    let mut doc = Document::with_version("1.5");

    // Reservar ID para el nodo Pages antes de crearlo (referencia circular)
    let pages_id = doc.new_object_id();

    // Fuente Helvetica
    let mut font = Dictionary::new();
    font.set("Type",     Object::Name(b"Font".to_vec()));
    font.set("Subtype",  Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    let font_id = doc.add_object(Object::Dictionary(font));

    // Stream de contenido: escribe el texto en (50, 750)
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![
                Object::Name(b"F1".to_vec()),
                Object::Integer(12),
            ]),
            Operation::new("Td", vec![Object::Integer(50), Object::Integer(750)]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_bytes = content.encode()
        .map_err(|e| format!("pdf: error codificando contenido: {}", e))?;
    let content_id = doc.add_object(Stream::new(Dictionary::new(), content_bytes));

    // Resources
    let mut font_res = Dictionary::new();
    font_res.set("F1", Object::Reference(font_id));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(font_res));

    // Página
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

    // Árbol de páginas
    let mut pages = Dictionary::new();
    pages.set("Type",  Object::Name(b"Pages".to_vec()));
    pages.set("Kids",  Object::Array(vec![Object::Reference(page_id)]));
    pages.set("Count", Object::Integer(1));
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    // Catálogo raíz
    let mut catalog = Dictionary::new();
    catalog.set("Type",  Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    doc.save(path).map_err(|e| format!("pdf.crear: {}", e))?;
    Ok(EvalValue::Bool(true))
}

fn one_str(fn_name: &str, args: &[EvalValue]) -> Result<String, String> {
    if args.is_empty() { return Err(format!("{} requiere (path)", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
