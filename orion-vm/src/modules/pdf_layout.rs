//! Maquetación de los PDF que Orion genera: `pdf.build` y sus atajos
//! `pdf.report`, `pdf.create` y `pdf.template`.
//!
//! Antes cada función colocaba el texto a coordenadas fijas: `report` pintaba
//! cuatro columnas de 38 mm y, al llegar al pie de la primera página, dejaba
//! de pintar sin avisar; `create` escribía una sola línea en tamaño Carta; y
//! un texto más ancho que su columna se montaba encima de la siguiente. Aquí
//! el texto se MIDE (con las métricas de Helvetica) antes de colocarlo, y lo
//! que no cabe pasa a la página siguiente en vez de perderse.

use crate::eval_value::EvalValue;
use printpdf::{
    BuiltinFont, Color, ColorBits, ColorSpace, Greyscale, Image, ImageTransform, ImageXObject,
    IndirectFontRef, Line, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference, Point,
    Polygon, PolygonMode, Px, Rgb,
};
use std::fs::File;
use std::io::BufWriter;

//     Métricas

/// Anchos de Helvetica y Helvetica-Bold para ASCII 32..=126, en milésimas de
/// em (las tablas AFM estándar de las 14 fuentes base de PDF).
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];
const HELV_BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611,
    975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556,
    333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
    611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

/// Letra base de las latinas con tilde: "é" ocupa lo mismo que "e".
fn base_latina(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ñ' => 'n', 'ç' => 'c', 'ý' | 'ÿ' => 'y',
        'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'A',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
        'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => 'O',
        'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
        'Ñ' => 'N', 'Ç' => 'C', 'Ý' => 'Y',
        otro => otro,
    }
}

fn ancho_caracter(c: char, negrita: bool) -> u16 {
    let tabla = if negrita { &HELV_BOLD } else { &HELV };
    let b = base_latina(c);
    if (' '..='~').contains(&b) {
        return tabla[b as usize - 32];
    }
    match c {
        '€' => 556,
        '…' => 1000,
        '·' => 278,
        '–' => 556,
        '—' => 1000,
        '¿' | '¡' => if negrita { 611 } else { 556 },
        'º' | 'ª' => 365,
        _ => 556,
    }
}

const PT_A_MM: f32 = 25.4 / 72.0;

/// Ancho del texto en mm a ese tamaño de letra.
pub fn ancho_mm(texto: &str, tam: f32, negrita: bool) -> f32 {
    let milesimas: u32 = texto.chars().map(|c| ancho_caracter(c, negrita) as u32).sum();
    milesimas as f32 / 1000.0 * tam * PT_A_MM
}

/// El texto recortado con "…" para que quepa en `max_mm`.
pub fn recortar(texto: &str, max_mm: f32, tam: f32, negrita: bool) -> String {
    if ancho_mm(texto, tam, negrita) <= max_mm {
        return texto.to_string();
    }
    let puntos = ancho_mm("…", tam, negrita);
    let mut fuera = String::new();
    let mut usado = 0.0;
    for c in texto.chars() {
        let w = ancho_caracter(c, negrita) as f32 / 1000.0 * tam * PT_A_MM;
        if usado + w + puntos > max_mm { break; }
        fuera.push(c);
        usado += w;
    }
    fuera.trim_end().to_string() + "…"
}

/// Parte un texto en líneas que caben en `max_mm`. Respeta los saltos de
/// línea del original; una palabra más ancha que la línea se recorta.
pub fn partir(texto: &str, max_mm: f32, tam: f32, negrita: bool) -> Vec<String> {
    let mut lineas = Vec::new();
    for parrafo in texto.split('\n') {
        let mut actual = String::new();
        for palabra in parrafo.split_whitespace() {
            let candidata = if actual.is_empty() {
                palabra.to_string()
            } else {
                format!("{actual} {palabra}")
            };
            if ancho_mm(&candidata, tam, negrita) <= max_mm {
                actual = candidata;
            } else {
                if !actual.is_empty() {
                    lineas.push(std::mem::take(&mut actual));
                }
                actual = recortar(palabra, max_mm, tam, negrita);
            }
        }
        lineas.push(actual);
    }
    lineas
}

//     Opciones

/// Un valor de un dict de opciones, buscado por varios nombres: el inglés
/// canónico y sus alias en español (`size` / `tamano`).
pub fn opcion<'a>(opts: Option<&'a EvalValue>, claves: &[&str]) -> Option<&'a EvalValue> {
    match opts {
        Some(EvalValue::Dict(m)) => claves.iter().find_map(|k| m.get(*k)),
        _ => None,
    }
}

fn opcion_texto(opts: Option<&EvalValue>, claves: &[&str]) -> Option<String> {
    opcion(opts, claves).map(celda)
}

fn opcion_num(opts: Option<&EvalValue>, claves: &[&str]) -> Option<f32> {
    match opcion(opts, claves)? {
        EvalValue::Int(n) => Some(*n as f32),
        EvalValue::Float(f) => Some(*f as f32),
        EvalValue::Str(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn opcion_si(opts: Option<&EvalValue>, claves: &[&str], por_defecto: bool) -> bool {
    match opcion(opts, claves) {
        Some(EvalValue::Bool(b)) => *b,
        Some(EvalValue::Null) | None => por_defecto,
        Some(_) => true,
    }
}

/// Un color: "#1f6f4a", "#fff", un nombre ("red", "gris"...) o un número
/// de 0 (negro) a 1 (blanco) para un gris.
pub fn color_de(v: &EvalValue) -> Option<Color> {
    let rgb = |r: f32, g: f32, b: f32| Some(Color::Rgb(Rgb::new(r, g, b, None)));
    match v {
        EvalValue::Int(n) => Some(Color::Greyscale(Greyscale::new(*n as f32, None))),
        EvalValue::Float(f) => Some(Color::Greyscale(Greyscale::new(*f as f32, None))),
        EvalValue::Str(s) => {
            let t = s.trim().to_lowercase();
            if let Some(hex) = t.strip_prefix('#') {
                let hex: String = if hex.len() == 3 {
                    hex.chars().flat_map(|c| [c, c]).collect()
                } else { hex.to_string() };
                if hex.len() != 6 { return None; }
                let c = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok().map(|v| v as f32 / 255.0);
                return rgb(c(0)?, c(2)?, c(4)?);
            }
            match t.as_str() {
                "black" | "negro" => rgb(0.0, 0.0, 0.0),
                "white" | "blanco" => rgb(1.0, 1.0, 1.0),
                "gray" | "grey" | "gris" => rgb(0.5, 0.5, 0.5),
                "lightgray" | "gris claro" => rgb(0.9, 0.9, 0.9),
                "red" | "rojo" => rgb(0.75, 0.16, 0.14),
                "green" | "verde" => rgb(0.12, 0.44, 0.29),
                "blue" | "azul" => rgb(0.16, 0.35, 0.65),
                "orange" | "naranja" => rgb(0.85, 0.45, 0.1),
                _ => None,
            }
        }
        _ => None,
    }
}

fn opcion_color(opts: Option<&EvalValue>, claves: &[&str], por_defecto: Color) -> Color {
    opcion(opts, claves).and_then(color_de).unwrap_or(por_defecto)
}

fn gris(nivel: f32) -> Color {
    Color::Greyscale(Greyscale::new(nivel, None))
}

#[derive(Clone, Copy, PartialEq)]
pub enum Alinear { Izquierda, Centro, Derecha }

fn alinear_de(v: Option<&EvalValue>) -> Option<Alinear> {
    match v.map(celda)?.to_lowercase().as_str() {
        "left" | "izquierda" => Some(Alinear::Izquierda),
        "center" | "centre" | "centro" => Some(Alinear::Centro),
        "right" | "derecha" => Some(Alinear::Derecha),
        _ => None,
    }
}

//     Documento paginado

/// Tamaño de página en mm: "A4" (por defecto), "A3", "A5", "letter",
/// "legal", o una lista [ancho, alto].
fn tamano_pagina(opts: Option<&EvalValue>) -> (f32, f32) {
    let (w, h) = match opcion(opts, &["size", "tamano", "tamaño"]) {
        Some(EvalValue::List(l)) if l.len() == 2 => {
            let n = |v: &EvalValue| match v {
                EvalValue::Int(i) => *i as f32,
                EvalValue::Float(f) => *f as f32,
                _ => 0.0,
            };
            (n(&l[0]).max(50.0), n(&l[1]).max(50.0))
        }
        otro => match otro.map(celda).unwrap_or_default().to_lowercase().as_str() {
            "a3" => (297.0, 420.0),
            "a5" => (148.0, 210.0),
            "letter" | "carta" => (215.9, 279.4),
            "legal" | "oficio" => (215.9, 355.6),
            _ => (210.0, 297.0),
        },
    };
    let horizontal = matches!(
        opcion_texto(opts, &["orientation", "orientacion", "orientación"])
            .unwrap_or_default().to_lowercase().as_str(),
        "landscape" | "horizontal" | "apaisado"
    );
    if horizontal == (h > w) { (h, w) } else { (w, h) }
}

/// Un documento que sabe en qué página y a qué altura va, y abre otra cuando
/// lo siguiente no cabe. Encabezado, pie y números de página se pintan al
/// final, cuando ya se sabe cuántas páginas hay.
pub struct Hojas {
    doc: PdfDocumentReference,
    paginas: Vec<PdfLayerReference>,
    pub ancho: f32,
    pub alto: f32,
    pub margen: f32,
    pub y: f32,
    pub normal: IndirectFontRef,
    pub negrita: IndirectFontRef,
    encabezado: String,
    pie: String,
    numerar: bool,
    formato_pagina: String,
}

impl Hojas {
    /// Opciones de página: size, orientation, margin, header, footer,
    /// page_numbers (yes), page_format ("Página {page} de {pages}"),
    /// title / author / subject (metadatos).
    pub fn new(titulo: &str, opts: Option<&EvalValue>) -> Result<Self, String> {
        let (ancho, alto) = tamano_pagina(opts);
        let meta = opcion_texto(opts, &["title", "titulo_documento"]).unwrap_or_else(|| titulo.to_string());
        let (doc, p, l) = PdfDocument::new(&meta, Mm(ancho), Mm(alto), "Capa 1");
        let mut doc = doc.with_creator("Orion");
        if let Some(a) = opcion_texto(opts, &["author", "autor"]) { doc = doc.with_author(a); }
        if let Some(s) = opcion_texto(opts, &["subject", "asunto"]) { doc = doc.with_subject(s); }
        let normal = doc.add_builtin_font(BuiltinFont::Helvetica).map_err(|e| e.to_string())?;
        let negrita = doc.add_builtin_font(BuiltinFont::HelveticaBold).map_err(|e| e.to_string())?;
        let capa = doc.get_page(p).get_layer(l);
        let margen = opcion_num(opts, &["margin", "margen"]).unwrap_or(15.0).clamp(3.0, ancho / 3.0);
        let encabezado = opcion_texto(opts, &["header", "encabezado"]).unwrap_or_default();
        let mut h = Hojas {
            doc, paginas: vec![capa], ancho, alto, margen, y: 0.0, normal, negrita,
            pie: opcion_texto(opts, &["footer", "pie"]).unwrap_or_default(),
            numerar: opcion_si(opts, &["page_numbers", "numerar"], true),
            formato_pagina: opcion_texto(opts, &["page_format", "formato_pagina"])
                .unwrap_or_else(|| "Página {page} de {pages}".into()),
            encabezado,
        };
        h.y = h.techo();
        Ok(h)
    }

    pub fn capa(&self) -> &PdfLayerReference {
        self.paginas.last().unwrap()
    }

    /// Ancho útil, entre márgenes.
    pub fn util(&self) -> f32 {
        self.ancho - 2.0 * self.margen
    }

    pub fn izquierda(&self) -> f32 {
        self.margen
    }

    /// Lo más arriba que empieza el contenido (debajo del encabezado).
    fn techo(&self) -> f32 {
        self.alto - self.margen - if self.encabezado.is_empty() { 0.0 } else { 7.0 }
    }

    /// Lo más abajo que se puede escribir antes del pie.
    pub fn suelo(&self) -> f32 {
        let hay_pie = self.numerar || !self.pie.is_empty();
        self.margen + if hay_pie { 4.0 } else { 0.0 }
    }

    pub fn cabe(&self, alto: f32) -> bool {
        self.y - alto >= self.suelo()
    }

    /// Página nueva si lo que viene no cabe (y si la actual no está vacía:
    /// algo más alto que una página entera va en una página propia).
    pub fn asegurar(&mut self, alto: f32) {
        if !self.cabe(alto) && self.y < self.techo() {
            self.nueva_pagina();
        }
    }

    pub fn nueva_pagina(&mut self) {
        let (p, l) = self.doc.add_page(Mm(self.ancho), Mm(self.alto), "Capa 1");
        self.paginas.push(self.doc.get_page(p).get_layer(l));
        self.y = self.techo();
    }

    pub fn texto(&self, t: &str, tam: f32, x: f32, y: f32, negrita: bool, color: &Color) {
        let capa = self.capa();
        capa.set_fill_color(color.clone());
        let fuente = if negrita { &self.negrita } else { &self.normal };
        capa.use_text(t, tam, Mm(x), Mm(y), fuente);
    }

    /// Texto alineado dentro de [x, x + ancho].
    #[allow(clippy::too_many_arguments)]
    pub fn texto_en(&self, t: &str, tam: f32, x: f32, ancho: f32, y: f32, negrita: bool,
                    color: &Color, alinear: Alinear) {
        let w = ancho_mm(t, tam, negrita);
        let tx = match alinear {
            Alinear::Izquierda => x,
            Alinear::Centro => x + (ancho - w) / 2.0,
            Alinear::Derecha => x + ancho - w,
        };
        self.texto(t, tam, tx, y, negrita, color);
    }

    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32, color: &Color) {
        let capa = self.capa();
        capa.set_fill_color(color.clone());
        let pt = |a: f32, b: f32| (Point::new(Mm(a), Mm(b)), false);
        let mut poly: Polygon = vec![pt(x, y), pt(x + w, y), pt(x + w, y + h), pt(x, y + h)]
            .into_iter().collect();
        poly.mode = PolygonMode::Fill;
        capa.add_polygon(poly);
    }

    pub fn linea(&self, x1: f32, y1: f32, x2: f32, y2: f32, grosor: f32, color: &Color) {
        let capa = self.capa();
        capa.set_outline_color(color.clone());
        capa.set_outline_thickness(grosor);
        capa.add_line(Line {
            points: vec![
                (Point::new(Mm(x1), Mm(y1)), false),
                (Point::new(Mm(x2), Mm(y2)), false),
            ],
            is_closed: false,
        });
    }

    /// Encabezado, pie y número de página en todas, y a disco.
    pub fn guardar(self, ruta: &str) -> Result<(), String> {
        let total = self.paginas.len();
        let tenue = gris(0.45);
        let pie_y = (self.margen / 2.0).max(4.0);
        for (i, capa) in self.paginas.iter().enumerate() {
            capa.set_fill_color(tenue.clone());
            if !self.encabezado.is_empty() {
                let t = recortar(&self.encabezado, self.util(), 8.0, false);
                capa.use_text(t, 8.0, Mm(self.margen), Mm(self.alto - self.margen - 2.5), &self.normal);
            }
            let mut libre = self.util();
            if self.numerar {
                let n = self.formato_pagina
                    .replace("{page}", &(i + 1).to_string())
                    .replace("{pages}", &total.to_string());
                let w = ancho_mm(&n, 7.5, false);
                capa.use_text(n, 7.5, Mm(self.ancho - self.margen - w), Mm(pie_y), &self.normal);
                libre -= w + 5.0;
            }
            if !self.pie.is_empty() {
                let pie = recortar(&self.pie, libre, 7.5, false);
                capa.use_text(pie, 7.5, Mm(self.margen), Mm(pie_y), &self.normal);
            }
        }
        let f = File::create(ruta).map_err(|e| format!("'{ruta}': {e}"))?;
        self.doc.save(&mut BufWriter::new(f)).map_err(|e| e.to_string())
    }
}

//     Celdas y formatos

fn celda(v: &EvalValue) -> String {
    match v {
        EvalValue::Null => String::new(),
        EvalValue::Str(s) => s.clone(),
        otro => otro.to_string(),
    }
}

fn numero_de(v: &EvalValue) -> Option<f64> {
    match v {
        EvalValue::Int(n) => Some(*n as f64),
        EvalValue::Float(f) => Some(*f),
        EvalValue::Str(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// ¿Parece una cifra? "12", "-3,5", "1.234,50 €", "15 %", "$ 9.99".
fn parece_numero(s: &str) -> bool {
    let t: String = s.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '€' | '$' | '£' | '%'))
        .collect();
    !t.is_empty()
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-' | '+'))
}

/// Separadores de los formatos numéricos. Por defecto, los del español:
/// "1.234,50". Se cambian con `decimal` y `thousands` en las opciones.
#[derive(Clone)]
struct Separadores { decimal: String, miles: String }

impl Separadores {
    fn de(opts: Option<&EvalValue>) -> Self {
        Separadores {
            decimal: opcion_texto(opts, &["decimal", "separador_decimal"]).unwrap_or_else(|| ",".into()),
            miles: opcion_texto(opts, &["thousands", "miles", "separador_miles"]).unwrap_or_else(|| ".".into()),
        }
    }
}

fn con_separadores(n: f64, decimales: usize, sep: &Separadores) -> String {
    let texto = format!("{:.*}", decimales, n.abs());
    let (entera, frac) = match texto.split_once('.') {
        Some((e, f)) => (e.to_string(), Some(f.to_string())),
        None => (texto, None),
    };
    let mut agrupada = String::new();
    for (i, c) in entera.chars().enumerate() {
        if i > 0 && (entera.len() - i) % 3 == 0 { agrupada.push_str(&sep.miles); }
        agrupada.push(c);
    }
    // Sin signo si al redondear queda en cero: "-0,00" no es una cifra.
    let es_cero = texto_es_cero(&agrupada, frac.as_deref());
    let signo = if n < 0.0 && !es_cero { "-" } else { "" };
    match frac {
        Some(f) => format!("{signo}{agrupada}{}{f}", sep.decimal),
        None => format!("{signo}{agrupada}"),
    }
}

fn texto_es_cero(entera: &str, frac: Option<&str>) -> bool {
    entera.chars().all(|c| !c.is_ascii_digit() || c == '0')
        && frac.map_or(true, |f| f.chars().all(|c| c == '0'))
}

/// Formato de una columna: "integer", "decimal", "decimal:3", "money"
/// (con €), "money:$" (otro símbolo), "percent" (0.15 → 15 %), "percent:1".
/// Lo que no sea número se deja como viene.
fn formatear(v: &EvalValue, formato: &str, sep: &Separadores) -> String {
    let Some(n) = numero_de(v) else { return celda(v) };
    let (tipo, arg) = match formato.split_once(':') {
        Some((t, a)) => (t.trim().to_lowercase(), Some(a.trim().to_string())),
        None => (formato.trim().to_lowercase(), None),
    };
    let decimales = |por_defecto: usize| arg.as_deref().and_then(|a| a.parse().ok()).unwrap_or(por_defecto);
    match tipo.as_str() {
        "integer" | "entero" => con_separadores(n, 0, sep),
        "decimal" | "number" | "numero" => con_separadores(n, decimales(2), sep),
        "money" | "moneda" | "currency" => {
            let simbolo = arg.clone().filter(|a| a.parse::<usize>().is_err()).unwrap_or_else(|| "€".into());
            format!("{} {simbolo}", con_separadores(n, 2, sep))
        }
        "percent" | "porcentaje" => format!("{} %", con_separadores(n * 100.0, decimales(0), sep)),
        _ => celda(v),
    }
}

//     Tabla

/// Filas de una tabla: cabecera y datos. Acepta lista de listas (la primera
/// es la cabecera, salvo que `headers` la dé aparte) o lista de dicts (la
/// cabecera son las claves, en el orden en que se escribieron).
type Filas = (Vec<String>, Vec<Vec<EvalValue>>);

fn filas_de(datos: &EvalValue, opts: Option<&EvalValue>) -> Result<Filas, String> {
    let filas = match datos {
        EvalValue::List(l) => l,
        _ => return Err("the table rows must be a list".into()),
    };
    let dada = match opcion(opts, &["headers", "cabeceras"]) {
        Some(EvalValue::List(h)) => Some(h.iter().map(celda).collect::<Vec<_>>()),
        _ => None,
    };
    if matches!(filas.first(), Some(EvalValue::Dict(_))) {
        let claves = crate::modules::excel_mod::collect_headers(filas);
        let datos = filas.iter().filter_map(|f| match f {
            EvalValue::Dict(m) => Some(claves.iter().map(|k| m.get(k).cloned().unwrap_or(EvalValue::Null)).collect()),
            _ => None,
        }).collect();
        return Ok((dada.unwrap_or(claves), datos));
    }
    let mut todas: Vec<Vec<EvalValue>> = filas.iter().map(|f| match f {
        EvalValue::List(c) => c.clone(),
        otro => vec![otro.clone()],
    }).collect();
    let cabecera = match dada {
        Some(h) => h,
        None if todas.is_empty() => vec![],
        None => todas.remove(0).iter().map(celda).collect(),
    };
    Ok((cabecera, todas))
}

/// Configuración de una columna, desde `columns`: un dict por nombre de
/// columna o una lista por posición. Claves: width (mm), align, format,
/// bold, color, title (otro texto para la cabecera), hidden.
#[derive(Clone, Default)]
struct Columna {
    ancho: Option<f32>,
    alinear: Option<Alinear>,
    formato: Option<String>,
    negrita: bool,
    color: Option<Color>,
    titulo: Option<String>,
    oculta: bool,
}

fn columna(o: Option<&EvalValue>) -> Columna {
    Columna {
        ancho: opcion_num(o, &["width", "ancho"]),
        alinear: alinear_de(opcion(o, &["align", "alinear"])),
        formato: opcion_texto(o, &["format", "formato"]),
        negrita: opcion_si(o, &["bold", "negrita"], false),
        color: opcion(o, &["color"]).and_then(color_de),
        titulo: opcion_texto(o, &["title", "titulo"]),
        oculta: opcion_si(o, &["hidden", "oculta"], false),
    }
}

fn columnas_de(opts: Option<&EvalValue>, cabecera: &[String]) -> Vec<Columna> {
    match opcion(opts, &["columns", "columnas"]) {
        Some(EvalValue::Dict(m)) => cabecera.iter().map(|c| columna(m.get(c))).collect(),
        Some(EvalValue::List(l)) => (0..cabecera.len()).map(|j| columna(l.get(j))).collect(),
        _ => vec![Columna::default(); cabecera.len()],
    }
}

/// Anchos: las columnas con `width` lo tienen fijo; el resto, lo que pide su
/// texto más largo. Si no caben, ceden solo las de texto que piden más de lo
/// que les tocaría a partes iguales; las cifras y las estrechas se quedan
/// como están, porque recortar "1.335.052,50 €" a "1.335.0…" es peor que no
/// poner nada. Si sobra sitio, se reparte para ocupar la página.
fn anchos(textos: &[Vec<String>], cfg: &[Columna], tam: f32, disponible: f32, numerica: &[bool]) -> Vec<f32> {
    const HUECO: f32 = 3.0;
    let n = cfg.len();
    let mut pide = vec![8.0_f32; n];
    for (i, fila) in textos.iter().enumerate() {
        for (j, t) in fila.iter().enumerate().take(n) {
            let w = ancho_mm(t, tam, i == 0 || cfg[j].negrita) + HUECO;
            if w > pide[j] { pide[j] = w; }
        }
    }
    for j in 0..n {
        if let Some(w) = cfg[j].ancho { pide[j] = w; }
    }
    let fijado = |j: usize| cfg[j].ancho.is_some();
    let total: f32 = pide.iter().sum();
    if total <= disponible {
        let libres = (0..n).filter(|&j| !fijado(j)).count();
        if libres == 0 { return pide; }
        let extra = (disponible - total) / libres as f32;
        return (0..n).map(|j| if fijado(j) { pide[j] } else { pide[j] + extra }).collect();
    }
    let justo = disponible / n as f32;
    let fija = |j: usize| fijado(j) || numerica[j] || pide[j] <= justo;
    let fijas: f32 = (0..n).filter(|&j| fija(j)).map(|j| pide[j]).sum();
    let flexibles: f32 = (0..n).filter(|&j| !fija(j)).map(|j| pide[j]).sum();
    let queda = disponible - fijas;
    if flexibles <= 0.0 || queda <= 0.0 {
        // Ni recortando todo el texto caben: se reparte a escala.
        return pide.iter().map(|w| w / total * disponible).collect();
    }
    (0..n).map(|j| if fija(j) { pide[j] } else { pide[j] / flexibles * queda }).collect()
}

/// Pinta una tabla desde la altura actual, partiéndola entre páginas y
/// repitiendo la cabecera en cada una.
///
/// Opciones: columns, headers, font_size, header_bg, header_color, zebra
/// (yes / no / color), borders, total, decimal, thousands.
pub fn tabla(h: &mut Hojas, datos: &EvalValue, opts: Option<&EvalValue>) -> Result<(), String> {
    let (mut cabecera, filas) = filas_de(datos, opts)?;
    let n_total = cabecera.len().max(filas.iter().map(|f| f.len()).max().unwrap_or(0));
    if n_total == 0 { return Ok(()); }
    cabecera.resize(n_total, String::new());
    let cfg_todas = columnas_de(opts, &cabecera);
    let visibles: Vec<usize> = (0..n_total).filter(|&j| !cfg_todas[j].oculta).collect();
    let n = visibles.len();
    if n == 0 { return Ok(()); }
    let cfg: Vec<Columna> = visibles.iter().map(|&j| cfg_todas[j].clone()).collect();
    let sep = Separadores::de(opts);

    // Todo a texto, ya formateado: de aquí salen los anchos.
    let titulos: Vec<String> = visibles.iter().enumerate()
        .map(|(k, &j)| cfg[k].titulo.clone().unwrap_or_else(|| cabecera[j].clone()))
        .collect();
    let cuerpo: Vec<Vec<String>> = filas.iter().map(|f| {
        visibles.iter().enumerate().map(|(k, &j)| {
            let v = f.get(j).cloned().unwrap_or(EvalValue::Null);
            match &cfg[k].formato {
                Some(fm) => formatear(&v, fm, &sep),
                None => celda(&v),
            }
        }).collect()
    }).collect();

    // Numérica si lo son la MAYORÍA de sus celdas con algo: la fila de total
    // suele llevar texto ("70 productos") en una columna de cifras.
    let numerica: Vec<bool> = (0..n).map(|k| {
        let llenas: Vec<&String> = cuerpo.iter().filter_map(|f| f.get(k)).filter(|t| !t.trim().is_empty()).collect();
        let cifras = llenas.iter().filter(|t| parece_numero(t)).count();
        !llenas.is_empty() && cifras * 2 > llenas.len()
    }).collect();
    let alinear: Vec<Alinear> = (0..n).map(|k| cfg[k].alinear.unwrap_or(
        if numerica[k] { Alinear::Derecha } else { Alinear::Izquierda })).collect();

    let tam = opcion_num(opts, &["font_size", "tamano_letra"])
        .unwrap_or(if n > 8 { 7.5 } else if n > 5 { 8.5 } else { 9.0 });
    let alto_fila = tam * PT_A_MM * 2.0;
    let mut textos = vec![titulos.clone()];
    textos.extend(cuerpo.iter().cloned());
    let w = anchos(&textos, &cfg, tam, h.util(), &numerica);

    let fondo_cab = opcion_color(opts, &["header_bg", "fondo_cabecera"], gris(0.9));
    let texto_cab = opcion_color(opts, &["header_color", "color_cabecera"], gris(0.0));
    let cebra: Option<Color> = match opcion(opts, &["zebra", "cebra"]) {
        Some(EvalValue::Bool(false)) => None,
        Some(v @ EvalValue::Str(_)) => color_de(v).or(Some(gris(0.965))),
        _ => Some(gris(0.965)),
    };
    let bordes = opcion_si(opts, &["borders", "bordes"], false);
    let con_total = opcion_si(opts, &["total", "totales"], false);
    let raya = gris(0.35);
    let negro = gris(0.0);
    let ultima = cuerpo.len().saturating_sub(1);
    let x0 = h.izquierda();
    let ancho_tabla: f32 = w.iter().sum();

    let pintar = |h: &Hojas, fila: &[String], es_cabecera: bool, forzar_negrita: bool| {
        let mut x = x0;
        let base = h.y - alto_fila + (alto_fila - tam * PT_A_MM) / 2.0 + 0.4;
        for k in 0..n {
            let negrita = es_cabecera || forzar_negrita || cfg[k].negrita;
            let t = fila.get(k).map(|s| s.as_str()).unwrap_or("");
            let t = recortar(t, w[k] - 2.4, tam, negrita);
            let color = if es_cabecera { &texto_cab } else { cfg[k].color.as_ref().unwrap_or(&negro) };
            h.texto_en(&t, tam, x + 1.2, w[k] - 2.4, base, negrita, color, alinear[k]);
            x += w[k];
        }
    };
    let verticales = |h: &Hojas, arriba: f32, abajo: f32| {
        let mut x = x0;
        h.linea(x, arriba, x, abajo, 0.3, &raya);
        for wk in &w {
            x += wk;
            h.linea(x, arriba, x, abajo, 0.3, &raya);
        }
    };
    let cabecera_pagina = |h: &mut Hojas| {
        h.rect(x0, h.y - alto_fila, ancho_tabla, alto_fila, &fondo_cab);
        pintar(h, &titulos, true, false);
        if bordes {
            h.linea(x0, h.y, x0 + ancho_tabla, h.y, 0.3, &raya);
            verticales(h, h.y, h.y - alto_fila);
        }
        h.y -= alto_fila;
        h.linea(x0, h.y, x0 + ancho_tabla, h.y, 0.6, &raya);
    };

    h.asegurar(alto_fila * 2.0);
    cabecera_pagina(h);
    let mut par = false;
    for (i, fila) in cuerpo.iter().enumerate() {
        let vacia = fila.iter().all(|t| t.trim().is_empty());
        let alto = if vacia { alto_fila / 2.0 } else { alto_fila };
        if !h.cabe(alto) {
            h.nueva_pagina();
            cabecera_pagina(h);
            par = false;
        }
        if vacia {
            h.y -= alto;
            continue;
        }
        let es_total = con_total && i == ultima;
        if es_total {
            h.linea(x0, h.y, x0 + ancho_tabla, h.y, 0.6, &raya);
        } else if let (true, Some(c)) = (par, &cebra) {
            h.rect(x0, h.y - alto, ancho_tabla, alto, c);
        }
        pintar(h, fila, false, es_total);
        if bordes {
            verticales(h, h.y, h.y - alto);
            h.linea(x0, h.y - alto, x0 + ancho_tabla, h.y - alto, 0.3, &raya);
        }
        h.y -= alto;
        par = !par;
    }
    h.y -= 3.0;
    Ok(())
}

//     Bloques

/// Párrafo partido al ancho útil, con saltos de página si hace falta.
fn parrafo(h: &mut Hojas, t: &str, tam: f32, negrita: bool, color: &Color, alinear: Alinear, interlineado: f32) {
    let paso = tam * PT_A_MM * interlineado;
    for l in partir(t, h.util(), tam, negrita) {
        h.asegurar(paso);
        let (x, w) = (h.izquierda(), h.util());
        h.texto_en(&l, tam, x, w, h.y - tam * PT_A_MM, negrita, color, alinear);
        h.y -= paso;
    }
}

/// Pares etiqueta / valor: la etiqueta en negrita y el valor al lado, partido
/// en varias líneas si es largo.
fn campos(h: &mut Hojas, datos: &EvalValue, opts: Option<&EvalValue>) {
    let pares: Vec<(String, String)> = match datos {
        EvalValue::Dict(m) => m.iter().map(|(k, v)| (k.clone(), celda(v))).collect(),
        EvalValue::List(l) => l.iter().filter_map(|p| match p {
            EvalValue::List(kv) if kv.len() >= 2 => Some((celda(&kv[0]), celda(&kv[1]))),
            _ => None,
        }).collect(),
        _ => vec![],
    };
    let tam = opcion_num(opts, &["font_size", "tamano_letra"]).unwrap_or(11.0);
    let paso = tam * PT_A_MM * 1.5;
    let etiqueta = opcion_num(opts, &["label_width", "ancho_etiqueta"]).unwrap_or_else(|| {
        pares.iter().map(|(k, _)| ancho_mm(k, tam, true)).fold(0.0_f32, f32::max).min(h.util() * 0.4) + 6.0
    });
    let color = opcion_color(opts, &["color"], gris(0.0));
    for (k, v) in &pares {
        let lineas = partir(v, h.util() - etiqueta, tam, false);
        let alto = paso * lineas.len().max(1) as f32 + 1.5;
        h.asegurar(alto);
        h.texto(&recortar(k, etiqueta - 4.0, tam, true), tam, h.izquierda(), h.y - tam * PT_A_MM, true, &color);
        let mut y = h.y;
        for l in &lineas {
            h.texto(l, tam, h.izquierda() + etiqueta, y - tam * PT_A_MM, false, &color);
            y -= paso;
        }
        h.y -= alto;
    }
    h.y -= 2.0;
}

/// Píxeles RGB de una imagen, con la transparencia fundida sobre blanco.
pub fn rgb_de(ruta: &str) -> Result<(usize, usize, Vec<u8>), String> {
    let img = image::open(ruta).map_err(|e| format!("image '{ruta}': {e}"))?;
    let (w, h) = (img.width() as usize, img.height() as usize);
    let rgba = img.to_rgba8();
    let mut rgb = Vec::with_capacity(w * h * 3);
    for p in rgba.pixels() {
        let a = p[3] as f32 / 255.0;
        for c in 0..3 {
            rgb.push((p[c] as f32 * a + 255.0 * (1.0 - a)).round() as u8);
        }
    }
    Ok((w, h, rgb))
}

/// Una imagen (PNG, JPEG, GIF, BMP). `width` en mm (por defecto, su tamaño a
/// 96 ppp sin pasarse del ancho útil); el alto sale de la proporción salvo
/// que se dé `height`. `align`: left, center, right.
fn imagen(h: &mut Hojas, ruta: &str, opts: Option<&EvalValue>) -> Result<(), String> {
    let (px_w, px_h, rgb) = rgb_de(ruta)?;
    let natural = px_w as f32 / 96.0 * 25.4;
    let mut ancho = opcion_num(opts, &["width", "ancho"]).unwrap_or(natural).min(h.util());
    let mut alto = opcion_num(opts, &["height", "alto"]).unwrap_or(ancho * px_h as f32 / px_w as f32);
    let max_alto = h.techo() - h.suelo();
    if alto > max_alto {
        ancho *= max_alto / alto;
        alto = max_alto;
    }
    h.asegurar(alto);
    let x = match alinear_de(opcion(opts, &["align", "alinear"])).unwrap_or(Alinear::Izquierda) {
        Alinear::Izquierda => h.izquierda(),
        Alinear::Centro => h.izquierda() + (h.util() - ancho) / 2.0,
        Alinear::Derecha => h.izquierda() + h.util() - ancho,
    };
    let xobj = ImageXObject {
        width: Px(px_w),
        height: Px(px_h),
        color_space: ColorSpace::Rgb,
        bits_per_component: ColorBits::Bit8,
        interpolate: true,
        image_data: rgb,
        image_filter: None,
        clipping_bbox: None,
    };
    // A `dpi` ppp, 1 px mide 25.4/dpi mm: con esta cuenta el ancho es `ancho`,
    // y la escala vertical corrige el alto si se dio uno que no respeta la
    // proporción.
    let dpi = px_w as f32 * 25.4 / ancho;
    let alto_natural = px_h as f32 * 25.4 / dpi;
    Image::from(xobj).add_to_layer(h.capa().clone(), ImageTransform {
        translate_x: Some(Mm(x)),
        translate_y: Some(Mm(h.y - alto)),
        dpi: Some(dpi),
        scale_y: Some(alto / alto_natural),
        ..Default::default()
    });
    h.y -= alto + 3.0;
    Ok(())
}

/// Un bloque de `pdf.build`. El tipo lo da la clave principal del dict:
///
///   { "title": "…" }                 título grande
///   { "heading": "…" }               encabezado de sección
///   { "text": "…" }                  párrafo (size, bold, color, align)
///   { "table": filas, … }            tabla, con las opciones de tabla
///   { "fields": { clave: valor } }   ficha etiqueta / valor
///   { "image": "logo.png" }          imagen (width, height, align)
///   { "line": yes }                  raya (color, thickness)
///   { "space": 10 }                  hueco en mm
///   { "page_break": yes }            página nueva
///
/// Cada uno admite también su nombre en español (titulo, seccion, texto,
/// tabla, campos, imagen, linea, espacio, salto). Un texto suelto, en vez de
/// un dict, es un párrafo.
fn bloque(h: &mut Hojas, b: &EvalValue) -> Result<(), String> {
    let o = Some(b);
    let EvalValue::Dict(m) = b else {
        parrafo(h, &celda(b), 11.0, false, &gris(0.0), Alinear::Izquierda, 1.45);
        return Ok(());
    };
    let tam = opcion_num(o, &["size", "tamano"]);
    let negrita = opcion_si(o, &["bold", "negrita"], false);
    let color = opcion_color(o, &["color"], gris(0.0));
    let alinear = alinear_de(opcion(o, &["align", "alinear"])).unwrap_or(Alinear::Izquierda);

    if let Some(t) = opcion(o, &["title", "titulo"]) {
        let tam = tam.unwrap_or(18.0);
        h.asegurar(tam * PT_A_MM * 2.0);
        h.y -= 2.0;
        parrafo(h, &celda(t), tam, true, &color, alinear, 1.25);
        h.y -= 3.0;
    } else if let Some(t) = opcion(o, &["heading", "seccion", "sección", "encabezado"]) {
        let tam = tam.unwrap_or(13.0);
        // Un encabezado no se queda solo al pie: pide sitio para algo debajo.
        h.asegurar(tam * PT_A_MM * 1.4 + 15.0);
        h.y -= 3.0;
        parrafo(h, &celda(t), tam, true, &color, alinear, 1.3);
        h.y -= 1.5;
    } else if let Some(t) = opcion(o, &["text", "texto"]) {
        let tam = tam.unwrap_or(11.0);
        let inter = opcion_num(o, &["line_height", "interlineado"]).unwrap_or(1.45);
        parrafo(h, &celda(t), tam, negrita, &color, alinear, inter);
        h.y -= opcion_num(o, &["space_after", "espacio_despues"]).unwrap_or(2.5);
    } else if let Some(filas) = opcion(o, &["table", "tabla"]) {
        tabla(h, filas, o)?;
    } else if let Some(c) = opcion(o, &["fields", "campos"]) {
        campos(h, c, o);
    } else if let Some(r) = opcion(o, &["image", "imagen"]) {
        imagen(h, &celda(r), o)?;
    } else if opcion(o, &["line", "linea", "línea"]).is_some() {
        let grosor = opcion_num(o, &["thickness", "grosor"]).unwrap_or(0.5);
        let color = opcion_color(o, &["color"], gris(0.6));
        h.asegurar(4.0);
        h.y -= 2.0;
        let (x, w) = (h.izquierda(), h.util());
        h.linea(x, h.y, x + w, h.y, grosor, &color);
        h.y -= 2.0;
    } else if let Some(e) = opcion(o, &["space", "espacio"]) {
        let mm = numero_de(e).unwrap_or(5.0) as f32;
        if h.cabe(mm) { h.y -= mm; } else { h.nueva_pagina(); }
    } else if opcion(o, &["page_break", "salto"]).is_some() {
        h.nueva_pagina();
    } else {
        let claves: Vec<&str> = m.keys().map(|k| k.as_str()).collect();
        return Err(format!(
            "unknown block {{{}}}: use title, heading, text, table, fields, image, line, space or page_break",
            claves.join(", ")
        ));
    }
    Ok(())
}

//     API

/// pdf.build(ruta, bloques, opts?): un documento libre a base de bloques.
/// opts son las de página (size, orientation, margin, header, footer,
/// page_numbers, page_format, title, author, subject).
pub fn construir(ruta: &str, bloques: &EvalValue, opts: Option<&EvalValue>) -> Result<EvalValue, String> {
    let lista = match bloques {
        EvalValue::List(l) => l.clone(),
        otro => vec![otro.clone()],
    };
    let titulo = lista.iter().find_map(|b| opcion_texto(Some(b), &["title", "titulo"])).unwrap_or_default();
    let mut h = Hojas::new(&titulo, opts).map_err(|e| format!("pdf.build: {e}"))?;
    for (i, b) in lista.iter().enumerate() {
        bloque(&mut h, b).map_err(|e| format!("pdf.build, block {}: {e}", i + 1))?;
    }
    h.guardar(ruta).map_err(|e| format!("pdf.build: {e}"))?;
    Ok(EvalValue::Bool(true))
}

/// pdf.report(ruta, titulo, filas, opts?): título, subtítulo y una tabla.
/// opts: las de página, las de tabla, `subtitle` y `title_size`.
pub fn reporte(ruta: &str, titulo: &str, datos: &EvalValue, opts: Option<&EvalValue>) -> Result<EvalValue, String> {
    let mut h = Hojas::new(titulo, opts).map_err(|e| format!("pdf.report: {e}"))?;
    let tam = opcion_num(opts, &["title_size", "tamano_titulo"]).unwrap_or(16.0);
    parrafo(&mut h, titulo, tam, true, &gris(0.0), Alinear::Izquierda, 1.25);
    if let Some(sub) = opcion_texto(opts, &["subtitle", "subtitulo"]) {
        parrafo(&mut h, &sub, 9.5, false, &gris(0.4), Alinear::Izquierda, 1.4);
    }
    h.y -= 4.0;
    tabla(&mut h, datos, opts).map_err(|e| format!("pdf.report: {e}"))?;
    h.guardar(ruta).map_err(|e| format!("pdf.report: {e}"))?;
    Ok(EvalValue::Bool(true))
}

/// pdf.create(ruta, texto, opts?): texto corrido, partido al ancho y con
/// tantas páginas como haga falta.
pub fn texto_corrido(ruta: &str, texto: &str, opts: Option<&EvalValue>) -> Result<EvalValue, String> {
    let mut h = Hojas::new("", opts).map_err(|e| format!("pdf.create: {e}"))?;
    let tam = opcion_num(opts, &["font_size", "tamano_letra"]).unwrap_or(11.0);
    parrafo(&mut h, texto, tam, false, &gris(0.0), Alinear::Izquierda, 1.45);
    h.guardar(ruta).map_err(|e| format!("pdf.create: {e}"))?;
    Ok(EvalValue::Bool(true))
}

/// pdf.template(ruta, titulo, campos, opts?): título y ficha etiqueta / valor.
pub fn ficha(ruta: &str, titulo: &str, datos: &EvalValue, opts: Option<&EvalValue>) -> Result<EvalValue, String> {
    let mut h = Hojas::new(titulo, opts).map_err(|e| format!("pdf.template: {e}"))?;
    parrafo(&mut h, titulo, 18.0, true, &gris(0.0), Alinear::Izquierda, 1.25);
    h.y -= 5.0;
    campos(&mut h, datos, opts);
    h.guardar(ruta).map_err(|e| format!("pdf.template: {e}"))?;
    Ok(EvalValue::Bool(true))
}
