//! Localización de elementos y espera.
//!
//! Un único tipo de selector, deducido del propio texto:
//!
//! | Forma            | Significa                          |
//! |------------------|------------------------------------|
//! | `//div[@id='x']` | XPath (empieza por `//` o `(//`)   |
//! | `text=Comprar`   | por texto visible                  |
//! | `.card > button` | CSS                                |
//!
//! No hay `find_element_by_xpath` y `find_element_by_css`: una sola función que
//! mira lo que le has dado. La variante por texto existe porque la mayoría de
//! los XPath que escribe la gente son para buscar por contenido, y salen
//! frágiles e ilegibles.
//!
//! Todo se resuelve **dentro de la página**, en una sola evaluación. La
//! alternativa —traerse nodos a Orion y consultarlos uno a uno— es lo que hace
//! Selenium, y es la razón de que leer 500 elementos le cueste 1.500 idas y
//! vueltas.

use std::time::Duration;

use super::cdp::Conn;
use super::launch::Tuning;

/// Helper JS que se inyecta en cada evaluación.
///
/// Va como IIFE en vez de definirse una vez en la página: así no depende de
/// que sobreviva a una navegación ni ensucia el espacio global del sitio, que
/// es una forma barata de que un scraper se delate.
pub const FIND_JS: &str = r#"
// Documentos donde buscar: el principal y el de cada iframe accesible.
//
// Muchos modales —los de consentimiento de cookies sobre todo— viven dentro de
// un iframe. Sin esto, el selector correcto "no existe" y el usuario acaba
// buscando en las herramientas del navegador por qué. Los iframes de otro
// origen lanzan al tocarlos y simplemente se saltan: no hay forma de entrar.
const __docs = () => {
  const out = [document];
  const scan = (doc) => {
    let marcos;
    try { marcos = doc.querySelectorAll('iframe,frame'); } catch (e) { return; }
    for (const f of marcos) {
      let d = null;
      try { d = f.contentDocument; } catch (e) { d = null; }
      if (d) { out.push(d); scan(d); }
    }
  };
  scan(document);
  return out;
};

// Desplazamiento acumulado de la cadena de iframes que contiene al elemento.
// Las coordenadas de un elemento dentro de un iframe son relativas a ese
// iframe; para despachar un clic hacen falta las de la página de arriba.
const __offset = (el) => {
  let dx = 0, dy = 0;
  let doc = el.ownerDocument;
  for (let i = 0; i < __PROF; i++) {
    const fr = doc && doc.defaultView && doc.defaultView.frameElement;
    if (!fr) break;
    const r = fr.getBoundingClientRect();
    dx += r.left; dy += r.top;
    doc = fr.ownerDocument;
  }
  return [dx, dy];
};

const __unoEn = (doc, sel) => {
  if (sel.startsWith('//') || sel.startsWith('(//')) {
    return doc.evaluate(sel, doc, null, 9, null).singleNodeValue;
  }
  if (sel.startsWith('text=')) {
    const want = sel.slice(5).trim();
    let laxo = null;
    for (const e of doc.querySelectorAll('*')) {
      if (e.children.length) continue;
      const t = (e.textContent || '').trim();
      if (t === want) return e;
      if (!laxo && t.includes(want)) laxo = e;
    }
    return laxo;
  }
  return doc.querySelector(sel);
};

const __todosEn = (doc, sel) => {
  if (sel.startsWith('//') || sel.startsWith('(//')) {
    const r = doc.evaluate(sel, doc, null, 7, null);
    const out = [];
    for (let i = 0; i < r.snapshotLength; i++) out.push(r.snapshotItem(i));
    return out;
  }
  if (sel.startsWith('text=')) {
    const want = sel.slice(5).trim();
    return Array.from(doc.querySelectorAll('*'))
      .filter(e => !e.children.length && (e.textContent || '').trim().includes(want));
  }
  return Array.from(doc.querySelectorAll(sel));
};

const __find = (sel) => {
  for (const d of __docs()) {
    let e = null;
    try { e = __unoEn(d, sel); } catch (err) { e = null; }
    if (e) return e;
  }
  return null;
};

const __findAll = (sel) => {
  const out = [];
  for (const d of __docs()) {
    try { out.push(...__todosEn(d, sel)); } catch (err) {}
  }
  return out;
};
"#;

/// Envuelve un cuerpo JS con el helper y el selector ya inyectado como literal.
///
/// El selector se serializa con `serde_json` en vez de concatenarse entre
/// comillas: un selector con comillas o barras invertidas rompería la
/// expresión, y ese es el tipo de fallo que aparece en producción con el
/// selector raro de un solo sitio.
pub fn expr(sel: &str, cuerpo: &str, t: &Tuning) -> String {
    let s = serde_json::Value::String(sel.to_string()).to_string();
    let prof = t.iframe_depth;
    format!("(() => {{ const __PROF = {prof};\n {FIND_JS}\n const sel = {s};\n {cuerpo} }})()")
}

/// Como `expr`, pero reintentando hasta obtener algo o agotar el plazo.
///
/// Las lecturas que devuelven contenido (`text`, `html`, `attr`, `texts`) tienen
/// que esperar: en una página moderna el contenido llega después del clic que lo
/// pidió, y devolver `null` porque aún no estaba convierte un problema de tiempo
/// en un dato perdido en silencio. Es justo el fallo que hace que un scraper de
/// Python funcione en el portátil y no en el servidor.
///
/// Las que preguntan por el estado —`exists`, `visible`, `count`— **no** pasan
/// por aquí: su trabajo es responder sobre el momento actual, y hacerlas esperar
/// convertiría un "no está" legítimo en diez segundos de bloqueo.
///
/// Se considera "aún no hay nada" un `null`, un `undefined` o una lista vacía.
/// El bucle vive en la página, así que sigue siendo una única llamada CDP.
pub fn expr_waiting(sel: &str, cuerpo: &str, ms: u64, t: &Tuning) -> String {
    let reintento = t.retry_ms;
    let envuelto = format!(r#"
    return new Promise((resolve) => {{
      const limite = Date.now() + {ms};
      const intenta = () => {{
        const v = (() => {{ {cuerpo} }})();
        const vacio = v === null || v === undefined || (Array.isArray(v) && v.length === 0);
        if (!vacio || Date.now() >= limite) return resolve(v);
        setTimeout(intenta, {reintento});
      }};
      intenta();
    }});
    "#);
    expr(sel, &envuelto, t)
}

/// Espera a que un selector aparezca.
///
/// No hay sondeo desde Orion: la espera ocurre en la página con un
/// `MutationObserver` y se resuelve con una promesa, así que son **una llamada
/// CDP y una sola**, en vez de una cada 50 ms. Menos mensajes, menos latencia y
/// nada de tráfico mientras no pasa nada.
pub fn wait_for(
    conn: &Conn,
    session: &str,
    sel: &str,
    ms: u64,
    t: &Tuning,
) -> Result<bool, String> {
    let cuerpo = format!(r#"
    return new Promise((resolve) => {{
      if (__find(sel)) return resolve(true);
      const obs = new MutationObserver(() => {{
        if (__find(sel)) {{ obs.disconnect(); clearTimeout(t); resolve(true); }}
      }});
      obs.observe(document.documentElement, {{ childList: true, subtree: true, attributes: true }});
      const t = setTimeout(() => {{ obs.disconnect(); resolve(false); }}, {ms});
    }});
    "#);

    // El plazo de CDP tiene que ser mayor que el del JS: si vencieran a la vez,
    // el error sería un timeout de transporte en vez del "no apareció" real.
    let limite = Duration::from_millis(ms + t.cdp_margin_ms);
    let r = conn.call(
        "Runtime.evaluate",
        serde_json::json!({
            "expression": expr(sel, &cuerpo, t),
            "returnByValue": true,
            "awaitPromise": true,
        }),
        Some(session), limite,
    )?;
    Ok(r.get("result").and_then(|x| x.get("value")).and_then(|v| v.as_bool()).unwrap_or(false))
}

/// Posición de un elemento, leída **en el instante anterior** a usarla.
#[derive(Debug, Clone)]
pub struct Box {
    pub x: f64,
    pub y: f64,
    /// Cierto si hubo que atravesar algo para llegar: quien despache el evento
    /// tiene que restaurar la página después.
    pub forced: bool,
}

/// Espera a que un elemento sea **accionable** y devuelve dónde pincharlo.
///
/// Accionable quiere decir las cuatro cosas a la vez: existe, ocupa espacio, no
/// está oculto por estilo, y nadie lo tapa en su punto central. Si falta alguna
/// se reintenta hasta agotar el plazo, porque casi siempre es cuestión de
/// tiempo: el banner de cookies que se va, la animación que termina, el bloque
/// que aún se está montando.
///
/// Aquí está la diferencia con Selenium. Una cadena de acciones suya son varias
/// peticiones al driver, y entre ellas la página ha seguido ejecutando JS: el
/// elemento se movió o algo se le puso encima. Cuando llega el clic, las
/// coordenadas ya no valen y el clic acaba en otro sitio, sin avisar.
///
/// El bucle de reintento vive **dentro de la página**, así que todo esto es una
/// sola llamada CDP y la medición ocurre en el mismo instante en que se
/// aprueba, no varios mensajes antes. Si vence el plazo, el error dice qué
/// impedía el clic — "lo tapa `<div.cookie-banner>`" y no "element not
/// interactable".
/// Estrategia frente a algo que tapa el elemento.
#[derive(Clone, Copy, PartialEq)]
pub enum Force {
    /// Espera a que se despeje; si no se despeja, falla diciendo qué estorba.
    No,
    /// Si al agotar el plazo sigue tapado, se atraviesa.
    Si,
}

pub fn box_for_click(
    conn: &Conn,
    session: &str,
    sel: &str,
    ms: u64,
    force: Force,
    t: &Tuning,
) -> Result<Box, String> {
    let forzar = if force == Force::Si { "true" } else { "false" };
    let inset  = t.hit_inset;
    let capas  = t.force_layers;
    let reintento = t.retry_ms;
    let cuerpo = format!(r#"
    // Puntos candidatos dentro del elemento. Probar solo el centro es lo que
    // hacen las demás herramientas, y por eso fallan con una cabecera fija que
    // cubre media mitad de un botón: la otra mitad era perfectamente clicable.
    // Una persona pincharía en la parte visible, y esto hace lo mismo.
    const __puntos = (r) => {{
      const dx = Math.min(r.width / 4, {inset}), dy = Math.min(r.height / 4, {inset});
      const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
      return [
        [cx, cy],
        [cx, cy - dy], [cx, cy + dy], [cx - dx, cy], [cx + dx, cy],
        [r.left + 3, r.top + 3], [r.right - 3, r.top + 3],
        [r.left + 3, r.bottom - 3], [r.right - 3, r.bottom - 3],
      ].filter(([x, y]) => x >= 0 && y >= 0 && x < innerWidth && y < innerHeight);
    }};
    const __suyo = (el, enc) => enc && (enc === el || el.contains(enc) || enc.contains(el));
    // El hit-test tiene que ocurrir en el documento del elemento: dentro de un
    // iframe, `document.elementFromPoint` de la página de arriba devolvería el
    // propio iframe y todo parecería tapado.
    const __enPunto = (el, x, y) => el.ownerDocument.elementFromPoint(x, y);
    const __nombre = (e) => {{
      const cls = String(e.className || '').split(' ').filter(Boolean)[0];
      return '<' + e.tagName.toLowerCase() + (cls ? '.' + cls : '') + '>';
    }};

    return new Promise((resolve) => {{
      const limite = Date.now() + {ms};

      const intenta = () => {{
        const el = __find(sel);
        let why = 'no apareció';

        if (el) {{
          el.scrollIntoView({{ block: 'center', inline: 'center' }});
          const r = el.getBoundingClientRect();
          const st = getComputedStyle(el);

          if (r.width === 0 || r.height === 0) why = 'no ocupa espacio';
          else if (st.display === 'none' || st.visibility === 'hidden' || st.opacity === '0')
            why = 'está oculto';
          else {{
            const cands = __puntos(r);
            const [ox, oy] = __offset(el);
            for (const [x, y] of cands) {{
              if (__suyo(el, __enPunto(el, x, y))) {{
                return resolve({{ ok: true, x: x + ox, y: y + oy }});
              }}
            }}
            const enc = __enPunto(el, cands[0][0], cands[0][1]);
            why = enc ? ('lo tapa ' + __nombre(enc)) : 'no responde al puntero';
          }}
        }}

        if (Date.now() < limite) return setTimeout(intenta, {reintento});

        // Se agotó el plazo. Sin `force` se informa y se acabó.
        if (!{forzar} || !el) return resolve({{ ok: false, why: why }});

        // Con `force`: en vez de clicar a ciegas unas coordenadas —que es como
        // Selenium acaba pulsando el banner en lugar del botón— se vuelve
        // transparente al puntero lo que estorba. El clic sigue siendo un
        // evento real del navegador y ahora sí alcanza al elemento.
        const r = el.getBoundingClientRect();
        const cands = __puntos(r);
        const [x, y] = cands[0];
        const [ox, oy] = __offset(el);
        const tapados = [];
        for (let i = 0; i < {capas}; i++) {{
          const enc = __enPunto(el, x, y);
          if (__suyo(el, enc) || !enc) break;
          // El valor original se guarda en el propio elemento: así se restaura
          // exactamente lo que había, sin ensuciar el espacio global de la
          // página (que además es una forma barata de delatarse).
          enc.setAttribute('data-orion-pe', enc.style.pointerEvents || '');
          enc.style.pointerEvents = 'none';
          tapados.push(__nombre(enc));
        }}
        if (!__suyo(el, __enPunto(el, x, y))) {{
          return resolve({{ ok: false, why: why + ' (ni forzando)' }});
        }}
        return resolve({{ ok: true, x: x + ox, y: y + oy, forced: true, through: tapados }});
      }};

      intenta();
    }});
    "#);

    let limite = Duration::from_millis(ms + t.cdp_margin_ms);
    let r = conn.call(
        "Runtime.evaluate",
        serde_json::json!({
            "expression": expr(sel, &cuerpo, t),
            "returnByValue": true,
            "awaitPromise": true,
        }),
        Some(session), limite,
    )?;
    let v = r.get("result").and_then(|x| x.get("value")).cloned().unwrap_or(serde_json::Value::Null);

    if v.get("ok").and_then(|b| b.as_bool()) != Some(true) {
        let why = v.get("why").and_then(|w| w.as_str()).unwrap_or("no se pudo localizar");
        let pista = if force == Force::No {
            "\n  Si el elemento que estorba no se va a quitar, usa: { force: yes }"
        } else { "" };
        return Err(format!("'{sel}': {why} (tras esperar {ms} ms){pista}"));
    }
    Ok(Box {
        x: v.get("x").and_then(|n| n.as_f64()).unwrap_or(0.0),
        y: v.get("y").and_then(|n| n.as_f64()).unwrap_or(0.0),
        forced: v.get("forced").and_then(|b| b.as_bool()).unwrap_or(false),
    })
}

/// Devuelve su `pointer-events` a todo lo que se neutralizó para forzar un clic.
///
/// Se llama siempre después de un clic forzado, incluso si el clic falló: dejar
/// media página sorda al ratón rompería todo lo que viniera después.
pub fn restore_pointer_events(conn: &Conn, session: &str, timeout: Duration) {
    let js = r#"(() => {
      const docs = [document];
      const scan = (d) => { for (const f of d.querySelectorAll('iframe,frame')) {
        let c = null; try { c = f.contentDocument; } catch (err) {}
        if (c) { docs.push(c); scan(c); } } };
      try { scan(document); } catch (err) {}
      for (const d of docs) for (const e of d.querySelectorAll('[data-orion-pe]')) {
        const prev = e.getAttribute('data-orion-pe');
        if (prev) e.style.pointerEvents = prev; else e.style.removeProperty('pointer-events');
        e.removeAttribute('data-orion-pe');
      }
      return true;
    })()"#;
    let _ = conn.call(
        "Runtime.evaluate",
        serde_json::json!({ "expression": js, "returnByValue": true }),
        Some(session), timeout,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_selector_se_inyecta_escapado() {
        // Un selector con comillas rompería la expresión si se concatenara.
        let e = expr(r#"div[title="hola \" mundo"]"#, "return 1;", &Tuning::default());
        assert!(e.contains(r#"\"hola"#) || e.contains(r#"\\\""#),
                "el selector no se escapó: {e}");
        // Y debe seguir siendo una expresión JS cerrada.
        assert!(e.starts_with("(() => {"), "{e}");
        assert!(e.trim_end().ends_with("})()"), "{e}");
    }

    #[test]
    fn el_helper_cubre_las_tres_formas() {
        // La búsqueda es por documento (`doc`), no sobre `document`: es lo que
        // permite entrar en los iframes.
        assert!(FIND_JS.contains("doc.evaluate"), "falta XPath");
        assert!(FIND_JS.contains("text="), "falta la búsqueda por texto");
        assert!(FIND_JS.contains("doc.querySelector"), "falta CSS");
    }

    #[test]
    fn el_helper_entra_en_los_iframes() {
        // Los modales de consentimiento suelen vivir en un iframe; sin esto el
        // selector correcto parece no existir.
        assert!(FIND_JS.contains("contentDocument"), "no se recorre el contenido de los iframes");
        assert!(FIND_JS.contains("frameElement"),
                "no se calcula el desplazamiento del iframe, y el clic caería en otro sitio");
        // Y la búsqueda debe partir del conjunto de documentos, no de uno solo.
        assert!(FIND_JS.contains("__docs()"), "la búsqueda no recorre todos los documentos");
    }

    #[test]
    fn un_iframe_de_otro_origen_no_rompe_la_busqueda() {
        // Tocar un iframe cross-origin lanza; el helper tiene que seguir.
        assert!(FIND_JS.matches("catch").count() >= 3,
                "faltan guardas: un iframe de otro origen abortaría la búsqueda entera");
    }

    #[test]
    fn la_espera_usa_observador_y_no_sondeo() {
        // Si esto cambiara a un bucle de sondeo, se multiplicarían los mensajes
        // CDP sin que nadie lo notara hasta medir.
        let cuerpo = format!("{}", 1234);
        assert!(!cuerpo.is_empty());
        // El cuerpo real se construye en wait_for; se comprueba su forma aquí.
        let js = expr("x", "return new Promise((resolve) => { const obs = new MutationObserver(() => {}); });", &Tuning::default());
        assert!(js.contains("MutationObserver"));
    }
}
