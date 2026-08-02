/// Orion Insight — análisis de documentos/imágenes con visión computacional + AI.
use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;
use image::{ImageReader, DynamicImage, GenericImageView};

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // analyze(path, question?, opts?) → análisis con AI Vision
        // opts = { model, max_tokens, provider }
        "analyze" => {
            if args.is_empty() { return Err("insight.analyze requiere (path, question?, opts?)".into()); }
            let path     = to_str(&args[0]);
            let question = if args.len() > 1 { to_str(&args[1]) } else { "Describe el contenido de esta imagen en detalle.".into() };
            let opts     = Opts::from(args.get(2))?;
            analyze_with_ai(&path, &question, &opts)
        }
        // metadata(path, opts?) → {width, height, density, contrast, orientation, threshold}
        "metadata" | "extract_metadata" => {
            let (path, opts) = path_and_opts(function, &args)?;
            extract_metadata(&path, &opts)
        }
        // summarize(path, opts?) → resumen estructurado del documento
        "summarize" => {
            let (path, opts) = path_and_opts(function, &args)?;
            summarize_doc(&path, &opts)
        }
        // detect_tables(path, opts?) → {detected, confidence, h_lines, v_lines, threshold}
        // opts = { threshold, line_ratio, gap_tolerance, line_merge, min_lines }
        "detect_tables" | "extract_tables" => {
            let (path, opts) = path_and_opts(function, &args)?;
            detect_tables(&path, &opts)
        }
        // detect_signatures(path, opts?) → {detected, confidence, density, threshold, box?}
        // opts = { threshold, region_from, region_to, min_stroke_ratio, min_width_ratio, max_fill, min_aspect, max_aspect, min_height_ratio, max_straightness }
        "detect_signatures" | "extract_signatures" => {
            let (path, opts) = path_and_opts(function, &args)?;
            detect_signatures(&path, &opts)
        }
        // pixel_density(path, opts?) → float (proporción de píxeles con tinta)
        "pixel_density" | "density" => {
            let (path, opts) = path_and_opts(function, &args)?;
            let img = open_img(&path)?;
            Ok(EvalValue::Float(round6(dark_density(&img, &opts))))
        }
        // threshold(path) → int  — el umbral que Otsu calcula para esta imagen
        "threshold" | "umbral" => {
            let (path, _) = path_and_opts(function, &args)?;
            let img = open_img(&path)?;
            Ok(EvalValue::Int(otsu(&img.to_luma8()) as i64))
        }
        // to_base64(path) → string base64 JPEG (para enviar a AI)
        "to_base64" => {
            let (path, _) = path_and_opts(function, &args)?;
            let img = open_img(&path)?;
            Ok(EvalValue::Str(img_to_base64(&img)?))
        }

        f => Err(format!("insight.{}() no existe", f)),
    }
}

fn path_and_opts(fn_name: &str, args: &[EvalValue]) -> Result<(String, Opts), String> {
    let Some(first) = args.first() else {
        return Err(format!("insight.{}() requiere (path, opts?)", fn_name));
    };
    Ok((to_str(first), Opts::from(args.get(1))?))
}

//     Análisis con AI Vision                                                    

fn analyze_with_ai(path: &str, question: &str, opts: &Opts) -> Result<EvalValue, String> {
    let img = open_img(path)?;
    let b64 = img_to_base64(&img)?;

    // Contexto estructural
    let context = format!(
        "Análisis estructural previo:\n\
         - Dimensiones: {}×{}\n\
         - Orientación: {}\n\
         - Densidad de contenido: {:.1}%\n\n{}",
        img.width(), img.height(),
        if img.height() > img.width() { "portrait" } else { "landscape" },
        dark_density(&img, opts) * 100.0,
        question
    );

    let env = load_env();
    // El developer manda: si fija 'provider', se usa ese aunque haya otra clave.
    let quiere = |p: &str| opts.provider.as_deref().map_or(true, |x| x == p);

    if quiere("anthropic") {
      if let Some(key) = env.get("ANTHROPIC_API_KEY") {
        let model = opts.model.clone()
            .or_else(|| env.get("ANTHROPIC_MODEL").cloned())
            .unwrap_or_else(|| "claude-haiku-4-5".into());
        let body = serde_json::json!({
            "model": model,
            "max_tokens": opts.max_tokens,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": b64}},
                    {"type": "text", "text": context}
                ]
            }]
        });
        let resp = ureq::post("https://api.anthropic.com/v1/messages")
            .set("Content-Type", "application/json")
            .set("x-api-key", key)
            .set("anthropic-version", "2023-06-01")
            .send_json(body)
            .map_err(|e| format!("insight.analyze: {}", e))?;
        let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        let text = json["content"][0]["text"].as_str().unwrap_or("").to_string();
        return Ok(EvalValue::Str(text));
      }
    }

    if quiere("openai") {
      if let Some(key) = env.get("OPENAI_API_KEY") {
        let model = opts.model.clone()
            .or_else(|| env.get("OPENAI_MODEL").cloned())
            .unwrap_or_else(|| "gpt-4o-mini".into());
        let body = serde_json::json!({
            "model": model,
            "max_tokens": opts.max_tokens,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{}", b64)}},
                    {"type": "text", "text": context}
                ]
            }]
        });
        let resp = ureq::post("https://api.openai.com/v1/chat/completions")
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {}", key))
            .send_json(body)
            .map_err(|e| format!("insight.analyze: {}", e))?;
        let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        let text = json["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
        return Ok(EvalValue::Str(text));
      }
    }

    if let Some(p) = &opts.provider {
        return Err(format!(
            "insight.analyze: se pidió el proveedor '{p}' pero no hay clave para él \
             (define {}_API_KEY)",
            p.to_uppercase()
        ));
    }

    // Sin API key: retorna solo el análisis estructural
    let mut m = HashMap::new();
    m.insert("width".into(),       EvalValue::Int(img.width() as i64));
    m.insert("height".into(),      EvalValue::Int(img.height() as i64));
    m.insert("density".into(),     EvalValue::Float(round6(dark_density(&img, opts))));
    m.insert("orientation".into(), EvalValue::Str(if img.height() > img.width() { "portrait" } else { "landscape" }.into()));
    m.insert("note".into(), EvalValue::Str("Agrega ANTHROPIC_API_KEY o OPENAI_API_KEY para análisis con AI".into()));
    Ok(EvalValue::Dict(m))
}

//     Umbral y binarización

/// Umbral de intensidad bajo el cual un píxel cuenta como "tinta".
///
/// Sin indicación del developer se calcula por Otsu sobre el histograma de la
/// propia imagen. Un valor fijo solo acierta en escaneos de blanco y negro
/// puros: en un documento gris o con fondo tintado deja fuera todo el
/// contenido, que es de donde salían los falsos negativos.
enum Threshold {
    Otsu,
    Fixed(u8),
}

impl Threshold {
    fn resolve(&self, gray: &image::GrayImage) -> u8 {
        match self {
            Threshold::Fixed(v) => *v,
            Threshold::Otsu     => otsu(gray),
        }
    }
}

/// Umbral de Otsu: el corte que maximiza la varianza entre las dos clases.
fn otsu(gray: &image::GrayImage) -> u8 {
    let mut hist = [0u64; 256];
    for p in gray.pixels() { hist[p[0] as usize] += 1; }
    let total: u64 = hist.iter().sum();
    if total == 0 { return 128; }

    let sum_all: f64 = (0..256).map(|i| i as f64 * hist[i] as f64).sum();
    let (mut sum_b, mut w_b, mut best_var, mut best_t) = (0.0f64, 0u64, -1.0f64, 128u8);

    for t in 0..256 {
        w_b += hist[t];
        if w_b == 0 { continue; }
        let w_f = total - w_b;
        if w_f == 0 { break; }
        sum_b += t as f64 * hist[t] as f64;
        let m_b = sum_b / w_b as f64;
        let m_f = (sum_all - sum_b) / w_f as f64;
        let var = w_b as f64 * w_f as f64 * (m_b - m_f) * (m_b - m_f);
        if var > best_var {
            best_var = var;
            best_t = t as u8;
        }
    }
    best_t
}

/// Máscara de tinta: `true` donde el píxel llega al umbral de oscuridad.
///
/// El umbral es INCLUSIVO (`<=`), que es como lo define Otsu: la clase oscura
/// es `[0..=t]`. Con `<` una imagen de blanco y negro puros da umbral 0 y no
/// se marcaría ni un píxel.
struct Ink {
    w: usize,
    h: usize,
    on: Vec<bool>,
}

impl Ink {
    fn new(gray: &image::GrayImage, thr: u8) -> Self {
        let (w, h) = gray.dimensions();
        let on = gray.pixels().map(|p| p[0] <= thr).collect();
        Ink { w: w as usize, h: h as usize, on }
    }
    #[inline]
    fn at(&self, x: usize, y: usize) -> bool { self.on[y * self.w + x] }
    fn count(&self) -> usize { self.on.iter().filter(|&&b| b).count() }
    fn density(&self) -> f64 {
        let total = self.w * self.h;
        if total == 0 { 0.0 } else { self.count() as f64 / total as f64 }
    }

    /// Caja que encierra toda la tinta, o `None` si no hay ninguna.
    ///
    /// Las líneas se miden contra esto y no contra la imagen completa: una
    /// tabla centrada en una hoja con márgenes no llega al 70% del ancho del
    /// papel aunque sus reglas la crucen entera.
    fn content_box(&self) -> Option<(usize, usize, usize, usize)> {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.at(x, y) {
                    x0 = x0.min(x); x1 = x1.max(x);
                    y0 = y0.min(y); y1 = y1.max(y);
                }
            }
        }
        if x0 == usize::MAX { None } else { Some((x0, y0, x1, y1)) }
    }
}

//     Análisis estructural (sin AI)

fn extract_metadata(path: &str, opts: &Opts) -> Result<EvalValue, String> {
    let img = open_img(path)?;
    Ok(extract_metadata_from_img(&img, opts))
}

fn extract_metadata_from_img(img: &DynamicImage, opts: &Opts) -> EvalValue {
    let (w, h) = img.dimensions();
    let gray   = img.to_luma8();
    let thr    = opts.threshold.resolve(&gray);
    let ink    = Ink::new(&gray, thr);
    let total  = (w * h) as f64;
    let dark   = ink.count() as f64;
    let light  = total - dark;
    let density    = if total > 0.0 { dark / total } else { 0.0 };
    let contrast   = if total > 0.0 { (dark - light).abs() / total } else { 0.0 };
    let orientation = if h > w { "portrait" } else { "landscape" };

    let mut m = HashMap::new();
    m.insert("width".into(),       EvalValue::Int(w as i64));
    m.insert("height".into(),      EvalValue::Int(h as i64));
    m.insert("density".into(),     EvalValue::Float(round6(density)));
    m.insert("contrast".into(),    EvalValue::Float(round6(contrast)));
    m.insert("orientation".into(), EvalValue::Str(orientation.into()));
    m.insert("threshold".into(),   EvalValue::Int(thr as i64));
    EvalValue::Dict(m)
}

/// Longitud del tramo continuo de tinta más largo de una fila o columna.
///
/// Es lo que distingue una regla de tabla de una línea de texto: el texto tiene
/// muchos píxeles oscuros pero interrumpidos, mientras que un borde de tabla es
/// un tramo seguido. Contar el total de oscuros, como se hacía antes, confundía
/// ambos casos en los dos sentidos.
fn longest_run(ink: &Ink, along_x: bool, idx: usize, tolerance: usize) -> usize {
    let n = if along_x { ink.w } else { ink.h };
    let (mut best, mut cur, mut gap) = (0usize, 0usize, 0usize);
    for i in 0..n {
        let on = if along_x { ink.at(i, idx) } else { ink.at(idx, i) };
        if on {
            cur += gap + 1;
            gap = 0;
        } else if cur > 0 && gap < tolerance {
            // Huecos cortos (antialiasing, líneas punteadas) no cortan el tramo.
            gap += 1;
        } else {
            best = best.max(cur);
            cur = 0;
            gap = 0;
        }
    }
    best.max(cur)
}

/// Agrupa índices contiguos: una regla de 3 px de grosor es UNA línea, no tres.
fn group_adjacent(idxs: &[usize], max_gap: usize) -> usize {
    if idxs.is_empty() { return 0; }
    let mut groups = 1;
    for pair in idxs.windows(2) {
        if pair[1] - pair[0] > max_gap { groups += 1; }
    }
    groups
}

fn detect_tables(path: &str, opts: &Opts) -> Result<EvalValue, String> {
    let img  = open_img(path)?;
    let gray = img.to_luma8();
    let thr  = opts.threshold.resolve(&gray);
    let ink  = Ink::new(&gray, thr);

    let tol = opts.gap_tolerance;
    // Extensión del contenido: la referencia contra la que se mide "cruza toda
    // la tabla". Sin tinta no hay nada que medir.
    let (cx0, cy0, cx1, cy1) = ink.content_box().unwrap_or((0, 0, 0, 0));
    let cont_w = cx1.saturating_sub(cx0) + 1;
    let cont_h = cy1.saturating_sub(cy0) + 1;

    let h_idx: Vec<usize> = (cy0..=cy1)
        .filter(|&y| y < ink.h)
        .filter(|&y| longest_run(&ink, true, y, tol) as f64 >= opts.line_ratio * cont_w as f64)
        .collect();
    let v_idx: Vec<usize> = (cx0..=cx1)
        .filter(|&x| x < ink.w)
        .filter(|&x| longest_run(&ink, false, x, tol) as f64 >= opts.line_ratio * cont_h as f64)
        .collect();

    let h_lines = group_adjacent(&h_idx, opts.line_merge);
    let v_lines = group_adjacent(&v_idx, opts.line_merge);

    // Una tabla necesita rejilla en ambos ejes; con líneas en uno solo se
    // estaría reportando un subrayado o un separador.
    let detected = h_lines >= opts.min_lines && v_lines >= opts.min_lines;
    let confidence = if detected {
        let peor = h_lines.min(v_lines) as f64;
        (peor / (opts.min_lines as f64 * 2.0)).min(1.0)
    } else {
        0.0
    };

    let mut m = HashMap::new();
    m.insert("detected".into(),   EvalValue::Bool(detected));
    m.insert("confidence".into(), EvalValue::Float(round2(confidence)));
    m.insert("h_lines".into(),    EvalValue::Int(h_lines as i64));
    m.insert("v_lines".into(),    EvalValue::Int(v_lines as i64));
    m.insert("threshold".into(),  EvalValue::Int(thr as i64));
    Ok(EvalValue::Dict(m))
}

/// Caja envolvente de una componente conexa de tinta.
struct Blob {
    x0: usize, y0: usize, x1: usize, y1: usize,
    px: usize,
}

impl Blob {
    fn w(&self) -> usize { self.x1 - self.x0 + 1 }
    fn h(&self) -> usize { self.y1 - self.y0 + 1 }
    /// Proporción de la caja que ocupa realmente la tinta. Un trazo manuscrito
    /// barre una caja grande con pocos píxeles; un bloque de texto la llena.
    fn fill(&self) -> f64 {
        let area = self.w() * self.h();
        if area == 0 { 0.0 } else { self.px as f64 / area as f64 }
    }
    fn aspect(&self) -> f64 {
        if self.h() == 0 { 0.0 } else { self.w() as f64 / self.h() as f64 }
    }
}

/// Tramo recto más largo dentro de la caja del blob, en fracción de su ancho
/// o su alto. Sirve para separar tinta manuscrita de estructura impresa: una
/// rejilla de tabla es una sola componente conexa que pasa todos los filtros
/// de forma, y lo que la delata es que contiene rectas que la cruzan entera.
fn straightness(ink: &Ink, b: &Blob, tol: usize) -> f64 {
    let mut best = 0.0f64;
    for y in b.y0..=b.y1.min(ink.h.saturating_sub(1)) {
        let run = longest_run_in(ink, true, y, b.x0, b.x1, tol);
        best = best.max(run as f64 / b.w() as f64);
    }
    for x in b.x0..=b.x1.min(ink.w.saturating_sub(1)) {
        let run = longest_run_in(ink, false, x, b.y0, b.y1, tol);
        best = best.max(run as f64 / b.h() as f64);
    }
    best
}

/// Como `longest_run` pero acotado a un rango de la fila/columna.
fn longest_run_in(ink: &Ink, along_x: bool, idx: usize, from: usize, to: usize, tolerance: usize) -> usize {
    let limit = if along_x { ink.w } else { ink.h };
    let to = to.min(limit.saturating_sub(1));
    let (mut best, mut cur, mut gap) = (0usize, 0usize, 0usize);
    for i in from..=to {
        let on = if along_x { ink.at(i, idx) } else { ink.at(idx, i) };
        if on {
            cur += gap + 1;
            gap = 0;
        } else if cur > 0 && gap < tolerance {
            gap += 1;
        } else {
            best = best.max(cur);
            cur = 0;
            gap = 0;
        }
    }
    best.max(cur)
}

/// Componentes conexas (vecindad 8) por flood fill iterativo.
/// Iterativo a propósito: una imagen grande desbordaría la pila con recursión.
fn connected_blobs(ink: &Ink, min_px: usize) -> Vec<Blob> {
    let mut seen = vec![false; ink.w * ink.h];
    let mut out  = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();

    for sy in 0..ink.h {
        for sx in 0..ink.w {
            if seen[sy * ink.w + sx] || !ink.at(sx, sy) { continue; }
            let (mut x0, mut y0, mut x1, mut y1, mut px) = (sx, sy, sx, sy, 0usize);
            stack.push((sx, sy));
            seen[sy * ink.w + sx] = true;

            while let Some((x, y)) = stack.pop() {
                px += 1;
                x0 = x0.min(x); x1 = x1.max(x);
                y0 = y0.min(y); y1 = y1.max(y);
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        if dx == 0 && dy == 0 { continue; }
                        let nx = x as i64 + dx;
                        let ny = y as i64 + dy;
                        if nx < 0 || ny < 0 || nx >= ink.w as i64 || ny >= ink.h as i64 { continue; }
                        let (nx, ny) = (nx as usize, ny as usize);
                        let i = ny * ink.w + nx;
                        if !seen[i] && ink.at(nx, ny) {
                            seen[i] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            if px >= min_px {
                out.push(Blob { x0, y0, x1, y1, px });
            }
        }
    }
    out
}

fn detect_signatures(path: &str, opts: &Opts) -> Result<EvalValue, String> {
    let img  = open_img(path)?;
    let gray = img.to_luma8();
    let thr  = opts.threshold.resolve(&gray);
    let full = Ink::new(&gray, thr);

    // Se mira solo la franja indicada (por defecto el tercio inferior, donde se
    // firma). La densidad global del documento no dice nada sobre si hay firma:
    // cualquier página con texto caía dentro del rango y daba positivo.
    let y_from = ((opts.region_from.clamp(0.0, 1.0)) * full.h as f64) as usize;
    let y_to   = (((opts.region_to.clamp(0.0, 1.0)) * full.h as f64) as usize).min(full.h);
    let band_h = y_to.saturating_sub(y_from);

    let mut best: Option<(f64, &Blob)> = None;
    let blobs;
    let mut density = 0.0;

    if band_h > 0 && full.w > 0 {
        let band = Ink {
            w: full.w,
            h: band_h,
            on: full.on[y_from * full.w..y_to * full.w].to_vec(),
        };
        density = band.density();
        let min_px = ((opts.min_stroke_ratio * (full.w * band_h) as f64) as usize).max(1);
        blobs = connected_blobs(&band, min_px);

        // Una firma es un trazo: caja ancha respecto a la banda, relleno bajo
        // (poca tinta para el espacio que abarca) y proporción apaisada pero
        // NO plana — una regla de tabla cumple todo lo demás y se distingue
        // justo por ahí: es un rectángulo de 2 px de alto y cientos de ancho.
        for b in &blobs {
            let ancho_rel = b.w() as f64 / full.w as f64;
            if ancho_rel < opts.min_width_ratio { continue; }
            if b.fill() > opts.max_fill { continue; }
            if b.aspect() < opts.min_aspect { continue; }
            if b.aspect() > opts.max_aspect { continue; }
            if (b.h() as f64) < opts.min_height_ratio * band_h as f64 { continue; }
            if straightness(&band, b, opts.gap_tolerance) > opts.max_straightness { continue; }
            // Cuanto más barre con menos tinta, más se parece a un trazo suelto.
            let score = (ancho_rel / opts.min_width_ratio).min(2.0) / 2.0
                      * (1.0 - b.fill() / opts.max_fill).clamp(0.0, 1.0);
            if best.map_or(true, |(s, _)| score > s) {
                best = Some((score, b));
            }
        }
    }

    let (detected, confidence) = match best {
        Some((s, _)) => (true, s.clamp(0.0, 1.0)),
        None         => (false, 0.0),
    };

    let mut m = HashMap::new();
    m.insert("detected".into(),   EvalValue::Bool(detected));
    m.insert("confidence".into(), EvalValue::Float(round2(confidence)));
    m.insert("density".into(),    EvalValue::Float(round6(density)));
    m.insert("threshold".into(),  EvalValue::Int(thr as i64));
    if let Some((_, b)) = best {
        let mut caja = HashMap::new();
        caja.insert("x".into(),      EvalValue::Int(b.x0 as i64));
        caja.insert("y".into(),      EvalValue::Int((b.y0 + y_from) as i64));
        caja.insert("width".into(),  EvalValue::Int(b.w() as i64));
        caja.insert("height".into(), EvalValue::Int(b.h() as i64));
        m.insert("box".into(), EvalValue::Dict(caja));
    }
    Ok(EvalValue::Dict(m))
}

fn summarize_doc(path: &str, opts: &Opts) -> Result<EvalValue, String> {
    let img      = open_img(path)?;
    let meta     = extract_metadata_from_img(&img, opts);
    let tables   = detect_tables(path, opts)?;
    let sigs     = detect_signatures(path, opts)?;
    let mut m = HashMap::new();
    m.insert("metadata".into(),   meta);
    m.insert("tables".into(),     tables);
    m.insert("signatures".into(), sigs);
    Ok(EvalValue::Dict(m))
}

fn dark_density(img: &DynamicImage, opts: &Opts) -> f64 {
    let gray = img.to_luma8();
    let thr  = opts.threshold.resolve(&gray);
    Ink::new(&gray, thr).density()
}

fn round6(v: f64) -> f64 { (v * 1e6).round() / 1e6 }
fn round2(v: f64) -> f64 { (v * 100.0).round() / 100.0 }

fn img_to_base64(img: &DynamicImage) -> Result<String, String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| format!("insight: error al codificar imagen: {}", e))?;
    Ok(b64_encode(buf.get_ref()))
}

//     Opciones

/// Todos los parámetros del análisis. Cada uno tiene un valor por defecto
/// razonable y ninguno está fijado en el código: `opts` los reemplaza.
struct Opts {
    /// Corte de intensidad para considerar un píxel como tinta.
    threshold: Threshold,
    //  detect_tables
    /// Fracción del ancho (o alto) que debe cubrir un tramo para ser línea.
    line_ratio: f64,
    /// Huecos de hasta N píxeles que no rompen un tramo (líneas punteadas).
    gap_tolerance: usize,
    /// Filas/columnas separadas por <= N píxeles cuentan como una sola línea.
    line_merge: usize,
    /// Líneas necesarias en cada eje para dar una tabla por detectada.
    min_lines: usize,
    //  detect_signatures
    /// Franja vertical analizada, en fracción de la altura.
    region_from: f64,
    region_to: f64,
    /// Tamaño mínimo de una componente, en fracción del área de la franja.
    min_stroke_ratio: f64,
    /// Ancho mínimo de la componente respecto al ancho de la imagen.
    min_width_ratio: f64,
    /// Relleno máximo de su caja: por encima se parece a un bloque de texto.
    max_fill: f64,
    /// Relación ancho/alto mínima.
    min_aspect: f64,
    /// Relación ancho/alto máxima: descarta reglas y subrayados, que son
    /// mucho más planos que cualquier trazo manuscrito.
    max_aspect: f64,
    /// Altura mínima de la componente respecto a la franja analizada.
    min_height_ratio: f64,
    /// Rectitud máxima: si un tramo recto cruza esta fracción de la caja, la
    /// componente es estructura impresa (rejilla, regla) y no un trazo.
    max_straightness: f64,
    //  analyze
    model: Option<String>,
    max_tokens: i64,
    provider: Option<String>,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            threshold: Threshold::Otsu,
            line_ratio: 0.7,
            gap_tolerance: 2,
            line_merge: 3,
            min_lines: 2,
            region_from: 0.6,
            region_to: 1.0,
            min_stroke_ratio: 0.0005,
            min_width_ratio: 0.08,
            max_fill: 0.45,
            min_aspect: 1.0,
            max_aspect: 25.0,
            min_height_ratio: 0.06,
            max_straightness: 0.75,
            model: None,
            max_tokens: 1024,
            provider: None,
        }
    }
}

impl Opts {
    fn from(arg: Option<&EvalValue>) -> Result<Self, String> {
        let mut o = Opts::default();
        let Some(EvalValue::Dict(m)) = arg else { return Ok(o) };

        // "auto" (o cualquier no-número) mantiene Otsu.
        if let Some(v) = m.get("threshold").or_else(|| m.get("umbral")) {
            match v {
                EvalValue::Int(n)   => o.threshold = Threshold::Fixed((*n).clamp(0, 255) as u8),
                EvalValue::Float(f) => o.threshold = Threshold::Fixed(f.clamp(0.0, 255.0) as u8),
                _ => {}
            }
        }
        if let Some(v) = num(m, &["line_ratio", "ratio_linea"]) { o.line_ratio = v; }
        if let Some(v) = num(m, &["gap_tolerance", "tolerancia_hueco"]) { o.gap_tolerance = v.max(0.0) as usize; }
        if let Some(v) = num(m, &["line_merge", "fusion_lineas"]) { o.line_merge = v.max(0.0) as usize; }
        if let Some(v) = num(m, &["min_lines", "min_lineas"]) { o.min_lines = v.max(0.0) as usize; }
        if let Some(v) = num(m, &["region_from", "region_desde"]) { o.region_from = v; }
        if let Some(v) = num(m, &["region_to", "region_hasta"]) { o.region_to = v; }
        if let Some(v) = num(m, &["min_stroke_ratio", "min_trazo"]) { o.min_stroke_ratio = v; }
        if let Some(v) = num(m, &["min_width_ratio", "min_ancho"]) { o.min_width_ratio = v; }
        if let Some(v) = num(m, &["max_fill", "max_relleno"]) { o.max_fill = v; }
        if let Some(v) = num(m, &["min_aspect", "min_aspecto"]) { o.min_aspect = v; }
        if let Some(v) = num(m, &["max_aspect", "max_aspecto"]) { o.max_aspect = v; }
        if let Some(v) = num(m, &["min_height_ratio", "min_alto"]) { o.min_height_ratio = v; }
        if let Some(v) = num(m, &["max_straightness", "max_rectitud"]) { o.max_straightness = v; }
        if let Some(v) = num(m, &["max_tokens"]) { o.max_tokens = v as i64; }
        if let Some(EvalValue::Str(s)) = m.get("model").or_else(|| m.get("modelo")) {
            o.model = Some(s.clone());
        }
        if let Some(EvalValue::Str(s)) = m.get("provider").or_else(|| m.get("proveedor")) {
            o.provider = Some(s.to_lowercase());
        }

        if o.region_from > o.region_to {
            return Err(format!(
                "insight: región inválida ({} > {}): 'region_from' no puede superar a 'region_to'",
                o.region_from, o.region_to
            ));
        }
        Ok(o)
    }
}

fn num(m: &HashMap<String, EvalValue>, keys: &[&str]) -> Option<f64> {
    for k in keys {
        match m.get(*k) {
            Some(EvalValue::Float(f)) => return Some(*f),
            Some(EvalValue::Int(n))   => return Some(*n as f64),
            _ => {}
        }
    }
    None
}

//     Helpers

fn open_img(path: &str) -> Result<DynamicImage, String> {
    ImageReader::open(path)
        .map_err(|e| format!("insight: no se pudo abrir '{}': {}", path, e))?
        .decode()
        .map_err(|e| format!("insight: no se pudo decodificar '{}': {}", path, e))
}

fn load_env() -> std::collections::HashMap<String, String> {
    let mut vars: std::collections::HashMap<String, String> = std::env::vars().collect();
    let mut path = std::env::current_dir().unwrap_or_default();
    for _ in 0..4 {
        let env_file = path.join(".env");
        if let Ok(content) = std::fs::read_to_string(&env_file) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') { continue; }
                if let Some(eq) = line.find('=') {
                    let key = line[..eq].trim().to_string();
                    let val = line[eq+1..].trim().trim_matches('"').trim_matches('\'').to_string();
                    if !key.is_empty() && !vars.contains_key(&key) { vars.insert(key, val); }
                }
            }
            break;
        }
        if !path.pop() { break; }
    }
    vars
}


fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

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
