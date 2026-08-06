//! Extracción declarativa.
//!
//! El esquema es un diccionario de `campo -> especificación`, y todo él se
//! compila a **una única** `Runtime.evaluate` que corre dentro de la página y
//! vuelve con los registros ya convertidos.
//!
//! ```orion
//! esquema = { nombre: ".title", precio: ".price|num", stock: "[data-qty]@data-qty|num" }
//! items   = web.extract(p, ".card", esquema)
//! ```
//!
//! Ahí está la diferencia de fondo con Selenium: allí cada lectura de un
//! atributo es una petición HTTP al driver, así que 500 productos por 3 campos
//! son unas 1.500 idas y vueltas más las 500 de localizar las filas. Esto es
//! **una**. Y como se usa `returnByValue`, lo que cruza el socket son los datos
//! pedidos, no el HTML: la memoria es proporcional a lo que querías, no al peso
//! de la página.
//!
//! Gramática de una especificación, con las tres partes opcionales:
//!
//! ```text
//!   <selector> @<atributo> |<conversión>
//!   ".price"                 texto del elemento
//!   "a@href"                 atributo de un descendiente
//!   "@data-id"               atributo de la propia fila
//!   ".price|num"             texto convertido a número
//!   "//td[2]|num"            XPath relativo a la fila
//!   "|num"                   el texto de la fila, como número
//! ```

use std::time::Duration;

use super::cdp::Conn;
use super::dom;
use super::launch::Tuning;

/// Una especificación de campo ya analizada.
#[derive(Debug, Clone, PartialEq)]
pub struct Campo {
    pub nombre: String,
    /// Selector relativo a la fila. Vacío significa la fila misma.
    pub sel:    String,
    /// Atributo a leer en vez del texto.
    pub attr:   Option<String>,
    /// Conversión a aplicar al valor bruto.
    pub conv:   Option<String>,
}

/// Busca el separador `sep` más a la derecha que esté fuera de corchetes.
///
/// Hace falta porque `@` y `|` aparecen dentro de los selectores: `//a[@href]`
/// lleva una arroba que no es el sufijo de atributo, y partir por la primera
/// —o por la última sin mirar— rompería todos los XPath con predicado.
fn corte(spec: &str, sep: char) -> Option<usize> {
    let mut nivel = 0i32;
    let mut hallado = None;
    for (i, c) in spec.char_indices() {
        match c {
            '[' | '(' => nivel += 1,
            ']' | ')' => nivel -= 1,
            _ if c == sep && nivel <= 0 => hallado = Some(i),
            _ => {}
        }
    }
    hallado
}

/// Analiza una especificación de campo.
pub fn parse_campo(nombre: &str, spec: &str) -> Result<Campo, String> {
    let spec = spec.trim();

    let (resto, conv) = match corte(spec, '|') {
        Some(i) => {
            let c = spec[i + 1..].trim().to_lowercase();
            if c.is_empty() {
                return Err(format!("campo '{nombre}': falta la conversión después de '|'"));
            }
            const VALIDAS: [&str; 6] = ["num", "int", "bool", "html", "text", "trim"];
            // `list` recoge TODAS las coincidencias en vez de la primera, y
            // admite una conversión detrás para el contenido: `list:num`.
            let sub = match c.strip_prefix("list") {
                Some(r) => Some(r.trim_start_matches(':')),
                None => None,
            };
            let valida = match sub {
                Some("")  => true,
                Some(s)   => VALIDAS.contains(&s),
                None      => VALIDAS.contains(&c.as_str()),
            };
            if !valida {
                return Err(format!(
                    "campo '{nombre}': conversión '{c}' desconocida.\n  Admitidas: {}, list, list:<conversión>",
                    VALIDAS.join(", ")
                ));
            }
            (spec[..i].trim(), Some(c))
        }
        None => (spec, None),
    };

    let (sel, attr) = match corte(resto, '@') {
        Some(i) => {
            let a = resto[i + 1..].trim();
            if a.is_empty() {
                return Err(format!("campo '{nombre}': falta el nombre del atributo después de '@'"));
            }
            (resto[..i].trim(), Some(a.to_string()))
        }
        None => (resto, None),
    };

    Ok(Campo {
        nombre: nombre.to_string(),
        sel:    sel.to_string(),
        attr,
        conv,
    })
}

/// JavaScript que resuelve un campo dentro de una fila y convierte el valor.
///
/// La conversión numérica merece cuidado: los precios reales vienen como
/// `"1.234,56 €"` o `"$1,234.56"`, y quedarse con los dígitos sin más daría
/// 123456. Se decide por cuál de los dos separadores aparece el último.
const EXTRAER_JS: &str = r#"
const __enFila = (fila, sel) => {
  if (!sel) return fila;
  if (sel.startsWith('/') || sel.startsWith('(/') || sel.startsWith('./')) {
    // Un XPath absoluto dentro de un campo busca desde la raíz del documento y
    // devuelve el MISMO nodo para todas las filas: el listado entero sale
    // repetido y con datos que parecen buenos. Como una especificación de campo
    // describe por definición algo que está dentro de la fila, se relativiza.
    let x = sel;
    if (x.startsWith('/'))  x = '.' + x;
    if (x.startsWith('(/')) x = '(.' + x.slice(1);
    const d = fila.ownerDocument;
    return d.evaluate(x, fila, null, 9, null).singleNodeValue;
  }
  if (sel.startsWith('text=')) {
    const want = sel.slice(5).trim();
    for (const e of fila.querySelectorAll('*')) {
      if (!e.children.length && (e.textContent || '').trim().includes(want)) return e;
    }
    return null;
  }
  return fila.querySelector(sel);
};

const __aNumero = (s) => {
  let t = String(s).replace(/[^0-9,.\-]/g, '');
  if (!t) return null;
  const coma = t.lastIndexOf(','), punto = t.lastIndexOf('.');
  if (coma > -1 && punto > -1) {
    // Manda el que aparece más a la derecha: es el separador decimal.
    t = coma > punto ? t.replace(/\./g, '').replace(',', '.')
                     : t.replace(/,/g, '');
  } else if (coma > -1) {
    // Una coma sola: decimal si separa 1 o 2 dígitos finales, si no, miles.
    t = /,\d{1,2}$/.test(t) ? t.replace(',', '.') : t.replace(/,/g, '');
  }
  const n = parseFloat(t);
  return Number.isFinite(n) ? n : null;
};

// Como `__enFila` pero en plural, para los campos `|list`.
//
// Un campo que recoge varios valores —las etiquetas de un producto, las
// imágenes de una galería— es habitual, y con la versión singular devolvía la
// primera coincidencia y las demás se perdían en silencio.
const __todosEnFila = (fila, sel) => {
  if (!sel) return [fila];
  if (sel.startsWith('/') || sel.startsWith('(/') || sel.startsWith('./')) {
    let x = sel;
    if (x.startsWith('/'))  x = '.' + x;
    if (x.startsWith('(/')) x = '(.' + x.slice(1);
    const d = fila.ownerDocument;
    const r = d.evaluate(x, fila, null, 7, null);
    const out = [];
    for (let i = 0; i < r.snapshotLength; i++) out.push(r.snapshotItem(i));
    return out;
  }
  if (sel.startsWith('text=')) {
    const want = sel.slice(5).trim();
    return Array.from(fila.querySelectorAll('*'))
      .filter(e => !e.children.length && (e.textContent || '').trim().includes(want));
  }
  return Array.from(fila.querySelectorAll(sel));
};

/// Texto o atributo de un elemento, sin convertir todavía.
const __bruto = (el, attr, conv) => {
  let b;
  if (attr) b = el.getAttribute(attr);
  else if (conv === 'html') b = el.innerHTML;
  else b = (el.innerText || el.textContent || '');
  if (b === null || b === undefined) return null;
  return String(b).trim();
};

const __convertir = (bruto, conv) => {
  switch (conv) {
    case 'num':  return __aNumero(bruto);
    case 'int':  { const n = __aNumero(bruto); return n === null ? null : Math.trunc(n); }
    case 'bool': return !(bruto === '' || /^(no|false|0|off)$/i.test(bruto));
    default:     return bruto;
  }
};

const __valor = (fila, c) => {
  const conv = c.conv || '';

  if (conv === 'list' || conv.startsWith('list:')) {
    const sub = conv.slice(4).replace(/^:/, '');
    const out = [];
    for (const el of __todosEnFila(fila, c.sel)) {
      const b = __bruto(el, c.attr, sub);
      // Un elemento sin nada dentro es ruido, no un valor: se salta, igual que
      // en el caso singular. Lo que sí se conserva es un null que venga de la
      // conversión ("Agotado" con |num), porque ahí sí hubo algo y hace falta
      // verlo para entender por qué no salió el número.
      if (b === null || (b === '' && !c.attr)) continue;
      out.push(__convertir(b, sub));
    }
    return out;
  }

  const el = __enFila(fila, c.sel);
  if (!el) return null;
  const bruto = __bruto(el, c.attr, conv);
  if (bruto === null) return null;
  if (bruto === '' && !c.attr) return null;
  return __convertir(bruto, conv);
};
"#;

pub struct Resultado {
    pub json:   serde_json::Value,
    /// Campos que no encontraron valor en NINGUNA fila.
    pub muertos: Vec<(String, String)>,
    pub filas:  usize,
}

/// Ejecuta la extracción. Una sola llamada CDP.
pub fn extract(
    conn: &Conn,
    session: &str,
    fila_sel: &str,
    campos: &[Campo],
    ms: u64,
    t: &Tuning,
) -> Result<Resultado, String> {
    let defs = serde_json::Value::Array(campos.iter().map(|c| serde_json::json!({
        "nombre": c.nombre,
        "sel":    c.sel,
        "attr":   c.attr,
        "conv":   c.conv,
    })).collect()).to_string();

    let cuerpo = format!(r#"
    {EXTRAER_JS}
    const campos = {defs};
    return new Promise((resolve) => {{
      const limite = Date.now() + {ms};
      const intenta = () => {{
        const filas = __findAll(sel);
        // Esperar a que haya filas: en una página moderna el listado llega
        // después de la acción que lo pidió, y devolver una lista vacía
        // convertiría un problema de tiempo en un resultado vacío silencioso.
        if (filas.length === 0 && Date.now() < limite) return setTimeout(intenta, {retry});

        const datos = [];
        const vacios = {{}};
        for (const c of campos) vacios[c.nombre] = 0;

        for (const fila of filas) {{
          const reg = {{}};
          for (const c of campos) {{
            const v = __valor(fila, c);
            // Una lista vacía es tan "no encontré nada" como un null: sin esto,
            // un `|list` con el selector equivocado se saltaría el aviso de
            // selector muerto, que es justo donde más falta hace.
            if (v === null || (Array.isArray(v) && v.length === 0)) vacios[c.nombre]++;
            reg[c.nombre] = v;
          }}
          datos.push(reg);
        }}
        resolve({{ datos: datos, vacios: vacios, total: filas.length }});
      }};
      intenta();
    }});
    "#, retry = t.retry_ms);

    let r = conn.call(
        "Runtime.evaluate",
        serde_json::json!({
            "expression": dom::expr_waiting_raw(fila_sel, &cuerpo, t),
            "returnByValue": true,
            "awaitPromise": true,
        }),
        Some(session), Duration::from_millis(ms + t.cdp_margin_ms),
    )?;

    if let Some(ex) = r.get("exceptionDetails") {
        let msg = ex.get("exception").and_then(|e| e.get("description")).and_then(|d| d.as_str())
            .or_else(|| ex.get("text").and_then(|t| t.as_str()))
            .unwrap_or("error de JavaScript");
        return Err(format!("browser.extract: {msg}"));
    }

    let v = r.get("result").and_then(|x| x.get("value")).cloned()
        .unwrap_or(serde_json::Value::Null);
    let total = v.get("total").and_then(|x| x.as_u64()).unwrap_or(0) as usize;

    // Un campo que falla en TODAS las filas casi nunca es un dato ausente: es un
    // selector equivocado, o el sitio que cambió de estructura. Distinguirlo de
    // "esta fila no tiene ese dato opcional" es justo lo que evita el fallo
    // silencioso de BeautifulSoup, donde el null se propaga cien líneas.
    let mut muertos = Vec::new();
    if total > 0 {
        if let Some(vac) = v.get("vacios").and_then(|x| x.as_object()) {
            for c in campos {
                if vac.get(&c.nombre).and_then(|n| n.as_u64()) == Some(total as u64) {
                    let spec = match (&c.attr, &c.conv) {
                        (Some(a), _) => format!("{}@{a}", c.sel),
                        (None, _)    => c.sel.clone(),
                    };
                    muertos.push((c.nombre.clone(), spec));
                }
            }
        }
    }

    Ok(Resultado {
        json: v.get("datos").cloned().unwrap_or(serde_json::Value::Array(vec![])),
        muertos,
        filas: total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(nombre: &str, spec: &str) -> Campo {
        parse_campo(nombre, spec).expect("spec válida")
    }

    #[test]
    fn una_especificacion_simple_es_solo_el_selector() {
        let x = c("nombre", ".title");
        assert_eq!(x.sel, ".title");
        assert!(x.attr.is_none());
        assert!(x.conv.is_none());
    }

    #[test]
    fn atributo_y_conversion_se_separan() {
        let x = c("stock", "[data-qty]@data-qty|num");
        assert_eq!(x.sel, "[data-qty]");
        assert_eq!(x.attr.as_deref(), Some("data-qty"));
        assert_eq!(x.conv.as_deref(), Some("num"));
    }

    #[test]
    fn la_arroba_de_un_xpath_no_es_el_sufijo_de_atributo() {
        // `//a[@href]` lleva una arroba dentro del predicado. Partir por la
        // primera —o por la última sin mirar corchetes— rompería todo XPath
        // con predicado, que son casi todos.
        let x = c("enlace", "//a[@href]");
        assert_eq!(x.sel, "//a[@href]");
        assert!(x.attr.is_none(), "se confundió la arroba del predicado");

        // Y con sufijo de verdad, se separan las dos.
        let y = c("url", "//a[@href]@href");
        assert_eq!(y.sel, "//a[@href]");
        assert_eq!(y.attr.as_deref(), Some("href"));
    }

    #[test]
    fn se_puede_leer_de_la_propia_fila() {
        let x = c("id", "@data-id");
        assert_eq!(x.sel, "");
        assert_eq!(x.attr.as_deref(), Some("data-id"));

        let y = c("valor", "|num");
        assert_eq!(y.sel, "");
        assert_eq!(y.conv.as_deref(), Some("num"));
    }

    #[test]
    fn list_se_admite_sola_y_con_conversion_detras() {
        assert_eq!(c("tags", ".tag|list").conv.as_deref(), Some("list"));
        assert_eq!(c("precios", ".p|list:num").conv.as_deref(), Some("list:num"));
        assert_eq!(c("urls", "a@href|list").attr.as_deref(), Some("href"));

        // Una conversión inventada detrás de `list` no se cuela.
        assert!(parse_campo("x", ".p|list:dinero").is_err());
        // Y `list` no se anida consigo misma.
        assert!(parse_campo("x", ".p|list:list").is_err());
    }

    #[test]
    fn una_conversion_inventada_lista_las_validas() {
        let e = parse_campo("precio", ".p|dinero").unwrap_err();
        assert!(e.contains("desconocida"), "{e}");
        assert!(e.contains("num"), "no listó las válidas: {e}");
    }

    #[test]
    fn los_separadores_sueltos_se_explican() {
        assert!(parse_campo("x", ".a|").unwrap_err().contains("conversión"));
        assert!(parse_campo("x", ".a@").unwrap_err().contains("atributo"));
    }

    #[test]
    fn se_admiten_espacios_alrededor() {
        let x = c("precio", "  .price @ data-v | num  ");
        assert_eq!(x.sel, ".price");
        assert_eq!(x.attr.as_deref(), Some("data-v"));
        assert_eq!(x.conv.as_deref(), Some("num"));
    }

    #[test]
    fn el_js_de_numeros_cubre_los_dos_formatos() {
        // No se ejecuta JS aquí; se comprueba que la lógica de ambos
        // separadores está presente, porque perderla convertiría "1.234,56" en
        // 123456 sin que ningún test lo notara.
        assert!(EXTRAER_JS.contains("lastIndexOf(',')"));
        assert!(EXTRAER_JS.contains("lastIndexOf('.')"));
        assert!(EXTRAER_JS.contains("/,\\d{1,2}$/"));
    }
}

//    Volcado a disco en streaming

/// Formato de salida, deducido de la extensión del archivo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Formato { Csv, Odf }

impl Formato {
    pub fn de_ruta(ruta: &str) -> Result<Formato, String> {
        match std::path::Path::new(ruta).extension().and_then(|e| e.to_str())
                  .map(|s| s.to_lowercase()).as_deref() {
            Some("csv") => Ok(Formato::Csv),
            Some("odf") => Ok(Formato::Odf),
            Some(otra)  => Err(format!(
                "extensión '.{otra}' no soportada.\n  Usa .csv (una fila cada vez) o .odf (binario por bloques)."
            )),
            None => Err("la salida necesita extensión: .csv o .odf".into()),
        }
    }
}

/// Escritor que mantiene la memoria acotada.
///
/// El CSV se escribe fila a fila, así que la memoria es constante de verdad. El
/// `.odf` lleva el número de filas en la cabecera y no admite añadir al final,
/// así que se acumula un bloque y se vuelca — el mismo patrón que usa
/// `frame.txt_to_odf`. En los dos casos lo que nunca ocurre es tener el listado
/// entero en RAM, que es lo que hace un scraper de Python antes de guardar.
pub struct Volcador {
    formato:  Formato,
    ruta:     String,
    headers:  Vec<String>,
    /// Bloque pendiente (solo `.odf`).
    buffer:   Vec<Vec<String>>,
    chunk:    usize,
    csv:      Option<csv::Writer<std::fs::File>>,
    pub filas:    usize,
    pub archivos: Vec<String>,
}

impl Volcador {
    pub fn nuevo(ruta: &str, headers: Vec<String>, chunk: usize) -> Result<Volcador, String> {
        let formato = Formato::de_ruta(ruta)?;
        let csv = match formato {
            Formato::Csv => {
                let f = std::fs::File::create(ruta)
                    .map_err(|e| format!("no se pudo crear '{ruta}': {e}"))?;
                let mut w = csv::Writer::from_writer(f);
                w.write_record(&headers).map_err(|e| format!("csv: {e}"))?;
                Some(w)
            }
            Formato::Odf => None,
        };
        Ok(Volcador {
            formato, ruta: ruta.to_string(), headers,
            buffer: Vec::new(), chunk: chunk.max(1), csv,
            filas: 0, archivos: Vec::new(),
        })
    }

    pub fn escribir(&mut self, fila: Vec<String>) -> Result<(), String> {
        self.filas += 1;
        match self.formato {
            Formato::Csv => {
                self.csv.as_mut().unwrap()
                    .write_record(&fila).map_err(|e| format!("csv: {e}"))?;
            }
            Formato::Odf => {
                self.buffer.push(fila);
                if self.buffer.len() >= self.chunk { self.volcar_bloque()?; }
            }
        }
        Ok(())
    }

    /// Vuelca el bloque pendiente y **libera** su memoria.
    fn volcar_bloque(&mut self) -> Result<(), String> {
        if self.buffer.is_empty() { return Ok(()); }
        // El primer bloque conserva el nombre pedido; a partir del segundo se
        // numeran. Así el caso normal produce el archivo que el usuario escribió
        // y el caso grande no pisa nada.
        let ruta = if self.archivos.is_empty() {
            self.ruta.clone()
        } else {
            let base = self.ruta.trim_end_matches(".odf");
            format!("{base}_{}.odf", self.archivos.len() + 1)
        };
        crate::modules::frame_mod::escribir_odf_filas(&ruta, &self.headers, &self.buffer)?;
        self.archivos.push(ruta);
        self.buffer.clear();
        self.buffer.shrink_to_fit();
        Ok(())
    }

    pub fn cerrar(mut self) -> Result<(usize, Vec<String>), String> {
        match self.formato {
            Formato::Csv => {
                if let Some(mut w) = self.csv.take() {
                    w.flush().map_err(|e| format!("csv: {e}"))?;
                }
                self.archivos.push(self.ruta.clone());
            }
            Formato::Odf => self.volcar_bloque()?,
        }
        Ok((self.filas, self.archivos))
    }
}

/// Convierte un valor JSON de un campo a su forma textual para el volcado.
///
/// Los nulos van como cadena vacía, que es lo que tanto CSV como la inferencia
/// de columnas del `.odf` entienden como ausencia.
pub fn a_texto(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => (if *b { "yes" } else { "no" }).to_string(),
        serde_json::Value::String(s) => s.clone(),
        otro => otro.to_string(),
    }
}
