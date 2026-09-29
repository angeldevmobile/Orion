//! Operaciones sobre PDF existentes: páginas, estampados y metadatos.
//! Todas leen un archivo y escriben otro, que puede ser el mismo.

use crate::eval_value::EvalValue;
use indexmap::IndexMap;
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use printpdf::Color;

use super::pdf_layout::{ancho_mm, color_de, opcion, rgb_de};

fn cargar(funcion: &str, ruta: &str) -> Result<Document, String> {
    Document::load(ruta).map_err(|e| format!("pdf.{funcion} '{ruta}': {e}"))
}

fn guardar(funcion: &str, mut doc: Document, salida: &str) -> Result<EvalValue, String> {
    doc.prune_objects();
    doc.compress();
    doc.save(salida).map_err(|e| format!("pdf.{funcion} '{salida}': {e}"))?;
    Ok(EvalValue::Bool(true))
}

//     Utilidades del árbol de páginas

/// Un objeto, siguiendo la referencia si lo es.
fn resolver<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Object> {
    match o {
        Object::Reference(id) => doc.get_object(*id).ok(),
        otro => Some(otro),
    }
}

/// Un atributo de página que puede heredarse del árbol (Resources, MediaBox,
/// CropBox, Rotate): el de la página o el del primer padre que lo tenga.
fn heredado(doc: &Document, pagina: ObjectId, clave: &[u8]) -> Option<Object> {
    let mut id = Some(pagina);
    while let Some(actual) = id {
        let Ok(Object::Dictionary(d)) = doc.get_object(actual) else { break };
        if let Ok(v) = d.get(clave) {
            return resolver(doc, v).cloned();
        }
        id = match d.get(b"Parent") {
            Ok(Object::Reference(p)) => Some(*p),
            _ => None,
        };
    }
    None
}

/// Copia en la propia página lo que heredaba de sus padres. Hace falta antes
/// de mover una página a otro árbol (unir, reordenar): allí sus padres de
/// antes ya no están, y con ellos se irían sus fuentes y su tamaño.
fn aplanar(doc: &mut Document, pagina: ObjectId) {
    let claves: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
    let valores: Vec<(&[u8], Object)> = claves.iter()
        .filter_map(|k| heredado(doc, pagina, k).map(|v| (*k, v)))
        .collect();
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(pagina) {
        for (k, v) in valores {
            d.set(k, v);
        }
    }
}

/// Ancho y alto en puntos de una página. A4 si no dice nada.
fn tamano(doc: &Document, pagina: ObjectId) -> (f32, f32) {
    if let Some(Object::Array(caja)) = heredado(doc, pagina, b"MediaBox") {
        let n: Vec<f32> = caja.iter().filter_map(|o| match o {
            Object::Integer(i) => Some(*i as f32),
            Object::Real(r) => Some(*r),
            _ => None,
        }).collect();
        if n.len() == 4 {
            return (n[2] - n[0], n[3] - n[1]);
        }
    }
    (595.0, 842.0)
}

/// Deja una única raíz /Pages con estas páginas, en este orden.
fn fijar_paginas(doc: &mut Document, paginas: &[ObjectId]) -> Result<(), String> {
    let raiz = doc.catalog().map_err(|e| e.to_string())?
        .get(b"Pages").and_then(|p| p.as_reference()).map_err(|e| e.to_string())?;
    for &p in paginas {
        aplanar(doc, p);
    }
    for &p in paginas {
        if let Ok(Object::Dictionary(d)) = doc.get_object_mut(p) {
            d.set("Parent", Object::Reference(raiz));
        }
    }
    let mut nodo = Dictionary::new();
    nodo.set("Type", Object::Name(b"Pages".to_vec()));
    nodo.set("Kids", Object::Array(paginas.iter().map(|p| Object::Reference(*p)).collect()));
    nodo.set("Count", Object::Integer(paginas.len() as i64));
    doc.objects.insert(raiz, Object::Dictionary(nodo));
    Ok(())
}

/// Qué páginas: `null`/"all" = todas; un número; una lista de números; o un
/// texto como "1-3,7". Números de 1 en adelante; negativos cuentan desde el
/// final (-1 es la última).
fn seleccion(funcion: &str, v: Option<&EvalValue>, total: u32) -> Result<Vec<u32>, String> {
    let una = |n: i64| -> Result<u32, String> {
        let p = if n < 0 { total as i64 + 1 + n } else { n };
        if p < 1 || p > total as i64 {
            return Err(format!("pdf.{funcion}: page {n} does not exist (the document has {total})"));
        }
        Ok(p as u32)
    };
    let mut fuera = Vec::new();
    match v {
        None | Some(EvalValue::Null) => fuera.extend(1..=total),
        Some(EvalValue::Int(n)) => fuera.push(una(*n)?),
        Some(EvalValue::Float(f)) => fuera.push(una(*f as i64)?),
        Some(EvalValue::List(l)) => {
            for x in l {
                match x {
                    EvalValue::Int(n) => fuera.push(una(*n)?),
                    EvalValue::Float(f) => fuera.push(una(*f as i64)?),
                    otro => return Err(format!("pdf.{funcion}: page numbers must be integers, got {otro}")),
                }
            }
        }
        Some(EvalValue::Str(s)) if matches!(s.trim().to_lowercase().as_str(), "all" | "todas") => {
            fuera.extend(1..=total)
        }
        Some(EvalValue::Str(s)) => {
            for trozo in s.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                let num = |t: &str| t.trim().parse::<i64>()
                    .map_err(|_| format!("pdf.{funcion}: '{s}' is not a page selection like \"1-3,7\""));
                match trozo.split_once('-') {
                    Some((a, b)) if !a.trim().is_empty() => {
                        let (a, b) = (una(num(a)?)?, una(num(b)?)?);
                        if a > b {
                            return Err(format!("pdf.{funcion}: range {a}-{b} is backwards"));
                        }
                        fuera.extend(a..=b);
                    }
                    _ => fuera.push(una(num(trozo)?)?),
                }
            }
        }
        Some(otro) => return Err(format!("pdf.{funcion}: invalid page selection {otro}")),
    }
    Ok(fuera)
}

fn rutas(funcion: &str, v: &EvalValue) -> Result<Vec<String>, String> {
    match v {
        EvalValue::List(l) if !l.is_empty() => Ok(l.iter().map(|x| x.to_string()).collect()),
        _ => Err(format!("pdf.{funcion}: expected a non-empty list of PDF paths")),
    }
}

//     Páginas

/// merge([a.pdf, b.pdf, …], salida): una tras otra, en ese orden.
pub fn unir(entradas: &EvalValue, salida: &str) -> Result<EvalValue, String> {
    let rutas = rutas("merge", entradas)?;
    let mut base = cargar("merge", &rutas[0])?;
    let mut paginas: Vec<ObjectId> = base.get_pages().values().copied().collect();

    for ruta in &rutas[1..] {
        let mut otro = cargar("merge", ruta)?;
        // Ids nuevos para que no choquen con los del documento base.
        otro.renumber_objects_with(base.max_id + 1);
        let suyas: Vec<ObjectId> = otro.get_pages().values().copied().collect();
        for &p in &suyas {
            aplanar(&mut otro, p);
        }
        base.max_id = otro.max_id;
        // Solo se traen sus objetos; su catálogo y su árbol de páginas se
        // quedan sin nadie que los use y `prune_objects` los quita al guardar.
        base.objects.extend(otro.objects);
        paginas.extend(suyas);
    }
    fijar_paginas(&mut base, &paginas).map_err(|e| format!("pdf.merge: {e}"))?;
    guardar("merge", base, salida)
}

/// delete_pages(ruta, salida, paginas): quita esas páginas.
pub fn quitar(ruta: &str, salida: &str, cuales: &EvalValue) -> Result<EvalValue, String> {
    let mut doc = cargar("delete_pages", ruta)?;
    let total = doc.get_pages().len() as u32;
    let mut fuera = seleccion("delete_pages", Some(cuales), total)?;
    fuera.sort_unstable();
    fuera.dedup();
    if fuera.len() as u32 == total {
        return Err("pdf.delete_pages: a PDF needs at least one page".into());
    }
    doc.delete_pages(&fuera);
    guardar("delete_pages", doc, salida)
}

/// reorder(ruta, salida, orden): las páginas en el orden dado, p. ej.
/// [3, 1, 2]. Las que no se nombren se quitan; una página no puede ir dos
/// veces.
pub fn reordenar(ruta: &str, salida: &str, orden: &EvalValue) -> Result<EvalValue, String> {
    let mut doc = cargar("reorder", ruta)?;
    let mapa = doc.get_pages();
    let numeros = seleccion("reorder", Some(orden), mapa.len() as u32)?;
    let mut vistas = std::collections::HashSet::new();
    for n in &numeros {
        if !vistas.insert(*n) {
            return Err(format!("pdf.reorder: page {n} appears twice"));
        }
    }
    let paginas: Vec<ObjectId> = numeros.iter().map(|n| mapa[n]).collect();
    fijar_paginas(&mut doc, &paginas).map_err(|e| format!("pdf.reorder: {e}"))?;
    guardar("reorder", doc, salida)
}

/// rotate(ruta, salida, grados, paginas?): gira en el sentido de las agujas
/// del reloj, en múltiplos de 90.
pub fn rotar(ruta: &str, salida: &str, grados: &EvalValue, cuales: Option<&EvalValue>) -> Result<EvalValue, String> {
    let grados = match grados {
        EvalValue::Int(n) => *n,
        EvalValue::Float(f) => *f as i64,
        otro => return Err(format!("pdf.rotate: degrees must be a number, got {otro}")),
    };
    if grados % 90 != 0 {
        return Err(format!("pdf.rotate: degrees must be a multiple of 90, got {grados}"));
    }
    let mut doc = cargar("rotate", ruta)?;
    let mapa = doc.get_pages();
    for n in seleccion("rotate", cuales, mapa.len() as u32)? {
        let id = mapa[&n];
        let actual = match heredado(&doc, id, b"Rotate") {
            Some(Object::Integer(r)) => r,
            _ => 0,
        };
        if let Ok(Object::Dictionary(d)) = doc.get_object_mut(id) {
            d.set("Rotate", Object::Integer((actual + grados).rem_euclid(360)));
        }
    }
    guardar("rotate", doc, salida)
}

//     Estampar

/// Texto a WinAnsi (cp1252), la codificación de las fuentes base de PDF.
/// Sin ella, una "ñ" o un "€" salían como dos caracteres raros.
fn win_ansi(s: &str) -> Vec<u8> {
    s.chars().map(|c| match c {
        '€' => 0x80, '…' => 0x85, '‘' => 0x91, '’' => 0x92, '“' => 0x93, '”' => 0x94,
        '–' => 0x96, '—' => 0x97,
        c if (c as u32) < 0x80 || (0xA0..=0xFF).contains(&(c as u32)) => c as u32 as u8,
        _ => b'?',
    }).collect()
}

fn rgb(color: &Color) -> (f32, f32, f32) {
    match color {
        Color::Rgb(c) => (c.r, c.g, c.b),
        Color::Greyscale(g) => (g.percent, g.percent, g.percent),
        _ => (0.0, 0.0, 0.0),
    }
}

fn num(opts: Option<&EvalValue>, claves: &[&str]) -> Option<f32> {
    match opcion(opts, claves)? {
        EvalValue::Int(n) => Some(*n as f32),
        EvalValue::Float(f) => Some(*f as f32),
        _ => None,
    }
}

const MM: f32 = 72.0 / 25.4;

/// Dónde colocar una caja de `w`×`h` pt: por `position` ("top-right"…) o por
/// `x`, `y` en mm desde la esquina superior izquierda.
fn colocar(opts: Option<&EvalValue>, por_defecto: &str, pw: f32, ph: f32, w: f32, h: f32) -> (f32, f32) {
    if let (Some(x), Some(y)) = (num(opts, &["x"]), num(opts, &["y"])) {
        return (x * MM, ph - y * MM - h);
    }
    let margen = num(opts, &["margin", "margen"]).unwrap_or(10.0) * MM;
    let pos = opcion(opts, &["position", "posicion", "posición"])
        .map(|v| v.to_string().to_lowercase())
        .unwrap_or_else(|| por_defecto.to_string());
    let (v, hz) = match pos.as_str() {
        "center" | "centro" => ("center", "center"),
        otro => otro.split_once('-').unwrap_or((otro, "center")),
    };
    let x = match hz {
        "left" | "izquierda" => margen,
        "right" | "derecha" => pw - margen - w,
        _ => (pw - w) / 2.0,
    };
    let y = match v {
        "top" | "arriba" => ph - margen - h,
        "bottom" | "abajo" => margen,
        _ => (ph - h) / 2.0,
    };
    (x, y)
}

/// Añade recursos a una página sin perder los que ya tenía (antes la marca de
/// agua borraba las fuentes y el texto de la página dejaba de verse).
fn anadir_recursos(doc: &mut Document, pagina: ObjectId, nuevos: &[(&str, &str, ObjectId)]) {
    let mut recursos = match heredado(doc, pagina, b"Resources") {
        Some(Object::Dictionary(d)) => d,
        _ => Dictionary::new(),
    };
    for (categoria, nombre, id) in nuevos {
        let mut grupo = match recursos.get(categoria.as_bytes()).ok().and_then(|g| resolver(doc, g)) {
            Some(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        grupo.set(*nombre, Object::Reference(*id));
        recursos.set(*categoria, Object::Dictionary(grupo));
    }
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(pagina) {
        d.set("Resources", Object::Dictionary(recursos));
    }
}

/// Pone `ops` encima del contenido de la página. El contenido original va
/// entre q/Q: si deja el estado gráfico cambiado (una escala, un color), no
/// le afecta a lo que se pinta encima.
fn encima(doc: &mut Document, pagina: ObjectId, ops: Vec<Operation>) -> Result<(), String> {
    let codificar = |ops: Vec<Operation>| Content { operations: ops }.encode().map_err(|e| e.to_string());
    let abre = doc.add_object(Stream::new(Dictionary::new(), codificar(vec![Operation::new("q", vec![])])?));
    let cierra = doc.add_object(Stream::new(Dictionary::new(), codificar(vec![Operation::new("Q", vec![])])?));
    let nuevo = doc.add_object(Stream::new(Dictionary::new(), codificar(ops)?));
    let Ok(Object::Dictionary(d)) = doc.get_object_mut(pagina) else {
        return Err("page is not a dictionary".into());
    };
    let mut lista = vec![Object::Reference(abre)];
    match d.get(b"Contents").ok().cloned() {
        Some(Object::Array(a)) => lista.extend(a),
        Some(otro) => lista.push(otro),
        None => {}
    }
    lista.push(Object::Reference(cierra));
    lista.push(Object::Reference(nuevo));
    d.set("Contents", Object::Array(lista));
    Ok(())
}

fn estado_opacidad(doc: &mut Document, opacidad: f32) -> ObjectId {
    let mut gs = Dictionary::new();
    gs.set("Type", Object::Name(b"ExtGState".to_vec()));
    gs.set("ca", Object::Real(opacidad));
    gs.set("CA", Object::Real(opacidad));
    doc.add_object(Object::Dictionary(gs))
}

fn fuente(doc: &mut Document, negrita: bool) -> ObjectId {
    let mut f = Dictionary::new();
    f.set("Type", Object::Name(b"Font".to_vec()));
    f.set("Subtype", Object::Name(b"Type1".to_vec()));
    f.set("BaseFont", Object::Name(if negrita { b"Helvetica-Bold".to_vec() } else { b"Helvetica".to_vec() }));
    f.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    doc.add_object(Object::Dictionary(f))
}

/// stamp(ruta, salida, texto, opts?): texto encima de páginas existentes.
/// {page} y {pages} se sustituyen, así sirve también para numerar.
pub fn estampar(ruta: &str, salida: &str, texto: &str, opts: Option<&EvalValue>) -> Result<EvalValue, String> {
    let mut doc = cargar("stamp", ruta)?;
    estampar_en(&mut doc, "stamp", texto, opts, "top-right", 12.0, (0.0, 0.0, 0.0), 1.0, 0.0)?;
    guardar("stamp", doc, salida)
}

/// watermark(ruta, salida, texto, opts?): un estampado con otros valores por
/// defecto (centrado, 48 pt, gris, 45°, semitransparente) y las mismas
/// opciones para cambiarlos.
pub fn marca_agua(ruta: &str, salida: &str, texto: &str, opts: Option<&EvalValue>) -> Result<EvalValue, String> {
    let mut doc = cargar("watermark", ruta)?;
    estampar_en(&mut doc, "watermark", texto, opts, "center", 48.0, (0.35, 0.35, 0.35), 0.25, 45.0)?;
    guardar("watermark", doc, salida)
}

#[allow(clippy::too_many_arguments)]
fn estampar_en(doc: &mut Document, funcion: &str, texto: &str, opts: Option<&EvalValue>,
               posicion: &str, tam: f32, color: (f32, f32, f32), opacidad: f32, giro: f32)
               -> Result<(), String> {
    let tam = num(opts, &["size", "tamano"]).unwrap_or(tam);
    let negrita = matches!(opcion(opts, &["bold", "negrita"]), Some(EvalValue::Bool(true)));
    let (r, g, b) = opcion(opts, &["color"]).and_then(color_de).map(|c| rgb(&c)).unwrap_or(color);
    let opacidad = num(opts, &["opacity", "opacidad"]).unwrap_or(opacidad).clamp(0.0, 1.0);
    let giro = num(opts, &["rotation", "rotacion", "rotación"]).unwrap_or(giro).to_radians();

    let mapa = doc.get_pages();
    let total = mapa.len() as u32;
    let paginas = seleccion(funcion, opcion(opts, &["pages", "paginas", "páginas"]), total)?;
    let id_fuente = fuente(doc, negrita);
    let id_estado = estado_opacidad(doc, opacidad);

    for n in paginas {
        let pagina = mapa[&n];
        let t = texto.replace("{page}", &n.to_string()).replace("{pages}", &total.to_string());
        let (pw, ph) = tamano(doc, pagina);
        let w = ancho_mm(&t, tam, negrita) * MM;
        let h = tam * 0.72;
        // Se coloca la caja SIN girar y se gira alrededor de su centro: así
        // "center" deja el centro del texto en el centro de la página, gire
        // lo que gire.
        let (x, y) = colocar(opts, posicion, pw, ph, w, h);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (s, c) = giro.sin_cos();
        let ox = cx - (c * w / 2.0 - s * h / 2.0);
        let oy = cy - (s * w / 2.0 + c * h / 2.0);
        let ops = vec![
            Operation::new("gs", vec![Object::Name(b"OrionGS".to_vec())]),
            Operation::new("rg", vec![Object::Real(r), Object::Real(g), Object::Real(b)]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"OrionF".to_vec()), Object::Real(tam)]),
            Operation::new("Tm", vec![
                Object::Real(c), Object::Real(s), Object::Real(-s), Object::Real(c),
                Object::Real(ox), Object::Real(oy),
            ]),
            Operation::new("Tj", vec![Object::String(win_ansi(&t), StringFormat::Literal)]),
            Operation::new("ET", vec![]),
        ];
        anadir_recursos(doc, pagina, &[("Font", "OrionF", id_fuente), ("ExtGState", "OrionGS", id_estado)]);
        encima(doc, pagina, ops).map_err(|e| format!("pdf.{funcion}: {e}"))?;
    }
    Ok(())
}

/// stamp_image(ruta, salida, imagen, opts?): logo, firma o sello encima de
/// páginas existentes.
pub fn estampar_imagen(ruta: &str, salida: &str, imagen: &str, opts: Option<&EvalValue>) -> Result<EvalValue, String> {
    let mut doc = cargar("stamp_image", ruta)?;
    let (px_w, px_h, pixeles) = rgb_de(imagen).map_err(|e| format!("pdf.stamp_image: {e}"))?;

    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Image".to_vec()));
    dict.set("Width", Object::Integer(px_w as i64));
    dict.set("Height", Object::Integer(px_h as i64));
    dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
    dict.set("BitsPerComponent", Object::Integer(8));
    let mut flujo = Stream::new(dict, pixeles);
    let _ = flujo.compress();
    let id_imagen = doc.add_object(flujo);

    let ancho = num(opts, &["width", "ancho"]).unwrap_or(30.0) * MM;
    let alto = num(opts, &["height", "alto"]).map(|h| h * MM)
        .unwrap_or(ancho * px_h as f32 / px_w as f32);
    let opacidad = num(opts, &["opacity", "opacidad"]).unwrap_or(1.0).clamp(0.0, 1.0);
    let id_estado = estado_opacidad(&mut doc, opacidad);

    let mapa = doc.get_pages();
    for n in seleccion("stamp_image", opcion(opts, &["pages", "paginas", "páginas"]), mapa.len() as u32)? {
        let pagina = mapa[&n];
        let (pw, ph) = tamano(&doc, pagina);
        let (x, y) = colocar(opts, "top-right", pw, ph, ancho, alto);
        let ops = vec![
            Operation::new("gs", vec![Object::Name(b"OrionGS".to_vec())]),
            Operation::new("cm", vec![
                Object::Real(ancho), Object::Integer(0), Object::Integer(0), Object::Real(alto),
                Object::Real(x), Object::Real(y),
            ]),
            Operation::new("Do", vec![Object::Name(b"OrionImg".to_vec())]),
        ];
        anadir_recursos(&mut doc, pagina, &[("XObject", "OrionImg", id_imagen), ("ExtGState", "OrionGS", id_estado)]);
        encima(&mut doc, pagina, ops).map_err(|e| format!("pdf.stamp_image: {e}"))?;
    }
    guardar("stamp_image", doc, salida)
}

//     Metadatos

const CLAVES_INFO: [(&str, &str); 6] = [
    ("title", "Title"), ("author", "Author"), ("subject", "Subject"),
    ("keywords", "Keywords"), ("creator", "Creator"), ("producer", "Producer"),
];

/// Una cadena de texto PDF: UTF-16BE con BOM si no es ASCII (así los
/// visores muestran bien las tildes).
fn texto_pdf(s: &str) -> Object {
    if s.is_ascii() {
        return Object::String(s.as_bytes().to_vec(), StringFormat::Literal);
    }
    let mut b = vec![0xFE, 0xFF];
    for u in s.encode_utf16() {
        b.extend_from_slice(&u.to_be_bytes());
    }
    Object::String(b, StringFormat::Hexadecimal)
}

/// Lo contrario de `texto_pdf`. Antes `info` leía los bytes como UTF-8, y
/// un título con tildes (guardado en UTF-16) salía como basura.
fn leer_texto_pdf(b: &[u8]) -> String {
    if b.starts_with(&[0xFE, 0xFF]) {
        let u: Vec<u16> = b[2..].chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u);
    }
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        // PDFDocEncoding coincide con Latin-1 en lo que importa.
        Err(_) => b.iter().map(|&c| c as char).collect(),
    }
}

/// info(ruta) → { pages, version, title, author, subject, keywords,
/// creator, producer } (los que tenga). `paginas` se mantiene por
/// compatibilidad.
pub fn info(ruta: &str) -> Result<EvalValue, String> {
    let doc = cargar("info", ruta)?;
    let mut m: IndexMap<String, EvalValue> = IndexMap::new();
    let n = doc.get_pages().len() as i64;
    m.insert("pages".into(), EvalValue::Int(n));
    m.insert("paginas".into(), EvalValue::Int(n));
    m.insert("version".into(), EvalValue::Str(doc.version.clone()));
    if let Some(Object::Dictionary(d)) = doc.trailer.get(b"Info").ok().and_then(|i| resolver(&doc, i)) {
        for (clave, nombre) in CLAVES_INFO {
            if let Ok(Object::String(b, _)) = d.get(nombre.as_bytes()) {
                m.insert(clave.into(), EvalValue::Str(leer_texto_pdf(b)));
            }
        }
    }
    Ok(EvalValue::Dict(m))
}

/// set_info(ruta, salida, { title, author, subject, keywords, creator,
/// producer }): cambia los metadatos dados y deja el resto como estaba. Un
/// valor null borra ese dato.
pub fn fijar_info(ruta: &str, salida: &str, datos: &EvalValue) -> Result<EvalValue, String> {
    let EvalValue::Dict(nuevos) = datos else {
        return Err("pdf.set_info: expected a dict like { \"title\": \"…\" }".into());
    };
    let mut doc = cargar("set_info", ruta)?;
    let mut d = match doc.trailer.get(b"Info").ok().and_then(|i| resolver(&doc, i)) {
        Some(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    for (clave, valor) in nuevos {
        let Some((_, nombre)) = CLAVES_INFO.iter().find(|(c, _)| c == &clave.to_lowercase().as_str()) else {
            let validas: Vec<&str> = CLAVES_INFO.iter().map(|(c, _)| *c).collect();
            return Err(format!("pdf.set_info: unknown key '{clave}' (valid: {})", validas.join(", ")));
        };
        match valor {
            EvalValue::Null => { d.remove(nombre.as_bytes()); }
            v => d.set(*nombre, texto_pdf(&v.to_string())),
        }
    }
    let id = doc.add_object(Object::Dictionary(d));
    doc.trailer.set("Info", Object::Reference(id));
    guardar("set_info", doc, salida)
}
