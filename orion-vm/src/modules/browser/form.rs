//! Formularios y tablas.
//!
//! Dos operaciones que en la práctica son la mitad de una automatización: meter
//! datos y sacarlos de una rejilla. Las dos se resuelven con **una sola
//! evaluación dentro de la página**, por el mismo motivo que la extracción: un
//! formulario de seis campos por el camino largo son doce idas y vueltas, y una
//! tabla de 300 filas leída celda a celda son miles.
//!
//! ## Por qué `fill` no sustituye a `type`
//!
//! Medido contra un sitio real, 51 caracteres tecla a tecla cuestan **221 ms** y
//! la misma asignación en una llamada cuesta **1 ms**: `type` manda dos eventos
//! CDP por carácter. Pero las teclas de verdad hacen falta cuando el sitio
//! reacciona a ellas —autocompletados, máscaras de teléfono, buscadores que
//! filtran mientras escribes—, así que las dos formas conviven y `fill` admite
//! `{ keys: yes }` para pasarse a la lenta cuando el sitio lo exija.
//!
//! ## La trampa del `value`
//!
//! Asignar `el.value = x` y lanzar un evento **no llega a la aplicación** si el
//! sitio usa React. React instala un rastreador sobre el descriptor `value` del
//! elemento y, cuando llega el evento, compara con lo último que él anotó: si
//! coincide, da el cambio por visto y no avisa a nadie. El campo se ve relleno
//! en pantalla y el formulario se envía vacío.
//!
//! La salida es escribir por el **setter nativo del prototipo**, que el
//! rastreador no intercepta. Comprobado sobre el mismo mecanismo que usa React:
//!
//! | Cómo se rellena                      | ¿Se entera la aplicación? |
//! |--------------------------------------|---------------------------|
//! | `el.value = x` + evento              | **No**                    |
//! | setter nativo del prototipo + evento | Sí                        |
//! | teclas reales                        | Sí                        |

use std::time::Duration;

use super::cdp::Conn;
use super::dom;
use super::launch::Tuning;

//    Rellenar un formulario

/// JavaScript que asigna un valor a un control, sea del tipo que sea.
///
/// El tipo de control lo decide la página, no quien escribe el programa: pedir
/// `fill` para los textos, `select` para los desplegables y `check` para las
/// casillas obligaría a saber de qué está hecho cada campo antes de escribir
/// una línea.
const RELLENAR_JS: &str = r#"
const __esVerdad = (v) =>
  !(v === false || v === 0 || v === null || v === undefined ||
    /^(no|false|0|off|)$/i.test(String(v)));

const __setNativo = (el, v) => {
  // El prototipo correcto importa: el setter de HTMLInputElement no sirve para
  // un <textarea> y la asignación se pierde sin decir nada.
  const proto = (el.tagName === 'TEXTAREA')
    ? window.HTMLTextAreaElement.prototype
    : window.HTMLInputElement.prototype;
  const d = Object.getOwnPropertyDescriptor(proto, 'value');
  if (d && d.set) d.set.call(el, String(v));
  else el.value = String(v);
};

const __rellenar = (el, v) => {
  const tag  = el.tagName;
  const tipo = (el.type || '').toLowerCase();

  if (tag === 'SELECT') {
    const q = String(v);
    const ops = Array.from(el.options);
    let i = ops.findIndex(o => o.value === q);
    if (i < 0) i = ops.findIndex(o => (o.textContent || '').trim() === q);
    if (i < 0 && /^[0-9]+$/.test(q)) i = parseInt(q, 10);
    if (i < 0 || i >= ops.length) {
      return { ok: false, why: 'no hay opción ' + JSON.stringify(q),
               opciones: ops.map(o => (o.textContent || '').trim()) };
    }
    el.selectedIndex = i;
    el.dispatchEvent(new Event('input',  { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return { ok: true, tipo: 'select' };
  }

  if (tipo === 'checkbox' || tipo === 'radio') {
    const querer = __esVerdad(v);
    if (tipo === 'radio' && !querer) {
      return { ok: false, why: 'un radio no se puede desmarcar; marca otro del grupo' };
    }
    // Un clic y no `checked = x`: muchos formularios escuchan el clic, y
    // además así se respetan los `disabled` y los `<label>` asociados.
    if (el.checked !== querer) el.click();
    return { ok: true, tipo: tipo };
  }

  if (el.isContentEditable) {
    el.textContent = String(v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    return { ok: true, tipo: 'editable' };
  }

  if (tag !== 'INPUT' && tag !== 'TEXTAREA') {
    return { ok: false, why: 'no es un campo de formulario (es <' + tag.toLowerCase() + '>)' };
  }

  el.focus();
  __setNativo(el, v);
  el.dispatchEvent(new Event('input',  { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
  // Muchos formularios validan al perder el foco; sin esto el campo queda
  // relleno pero marcado en rojo y el botón de enviar sigue deshabilitado.
  el.blur();
  return { ok: true, tipo: 'value' };
};
"#;

pub struct Relleno {
    pub puestos:  usize,
    /// Selectores que no encontraron ningún elemento.
    pub ausentes: Vec<String>,
    /// Selectores que sí existen pero no admitieron el valor, con el motivo.
    pub fallidos: Vec<(String, String)>,
}

/// Rellena varios campos de una vez. `campos` es una lista `(selector, valor)`.
///
/// El orden se respeta porque importa: un desplegable de provincia que solo se
/// llena al elegir el país tiene que ir después del país.
pub fn fill(
    conn: &Conn, session: &str, campos: &[(String, serde_json::Value)],
    ms: u64, t: &Tuning,
) -> Result<Relleno, String> {
    let defs = serde_json::Value::Array(
        campos.iter().map(|(s, v)| serde_json::json!({ "sel": s, "val": v })).collect()
    ).to_string();

    let cuerpo = format!(r#"
    {RELLENAR_JS}
    const campos = {defs};
    return new Promise((resolve) => {{
      const limite = Date.now() + {ms};
      const intenta = () => {{
        // Se espera a que estén TODOS antes de tocar ninguno. A medio rellenar
        // es el peor sitio donde parar: el formulario queda en un estado que
        // nadie escribió, y el reintento del programa lo encuentra a medias.
        const faltan = campos.filter(c => !__find(c.sel));
        if (faltan.length && Date.now() < limite) return setTimeout(intenta, {retry});

        const ausentes = faltan.map(c => c.sel);
        const fallidos = [];
        let puestos = 0;

        for (const c of campos) {{
          const el = __find(c.sel);
          if (!el) continue;
          let r;
          try {{ r = __rellenar(el, c.val); }}
          catch (e) {{ r = {{ ok: false, why: String(e && e.message || e) }}; }}
          if (r.ok) puestos++;
          else fallidos.push([c.sel, r.why + (r.opciones ? '\n  Opciones: ' + r.opciones.join(', ') : '')]);
        }}
        resolve({{ puestos: puestos, ausentes: ausentes, fallidos: fallidos }});
      }};
      intenta();
    }});
    "#, retry = t.retry_ms);

    let r = conn.call(
        "Runtime.evaluate",
        serde_json::json!({
            "expression": dom::expr_multi(&cuerpo, t),
            "returnByValue": true,
            "awaitPromise": true,
        }),
        Some(session), Duration::from_millis(ms + t.cdp_margin_ms),
    )?;

    if let Some(ex) = r.get("exceptionDetails") {
        let msg = ex.get("exception").and_then(|e| e.get("description")).and_then(|d| d.as_str())
            .or_else(|| ex.get("text").and_then(|x| x.as_str()))
            .unwrap_or("error de JavaScript");
        return Err(format!("browser.fill: {msg}"));
    }

    let v = r.get("result").and_then(|x| x.get("value")).cloned().unwrap_or(serde_json::Value::Null);
    Ok(Relleno {
        puestos: v.get("puestos").and_then(|x| x.as_u64()).unwrap_or(0) as usize,
        ausentes: v.get("ausentes").and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        fallidos: v.get("fallidos").and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|p| {
                let par = p.as_array()?;
                Some((par.first()?.as_str()?.to_string(), par.get(1)?.as_str()?.to_string()))
            }).collect())
            .unwrap_or_default(),
    })
}

/// Mensaje de lo que no se pudo rellenar.
///
/// Un campo que no se rellena casi nunca es un dato que faltaba: es el selector
/// equivocado, o el formulario que cambió. Callarlo deja el envío incompleto y
/// el fallo aparece en el servidor de otro, sin nada que lo relacione con esto.
pub fn queja(r: &Relleno) -> Option<String> {
    if r.ausentes.is_empty() && r.fallidos.is_empty() {
        return None;
    }
    let mut m = String::from("browser.fill: ");
    if !r.ausentes.is_empty() {
        m.push_str(&format!("{} campo(s) no existen en la página:\n", r.ausentes.len()));
        for s in &r.ausentes { m.push_str(&format!("    {s}\n")); }
    }
    for (s, why) in &r.fallidos {
        m.push_str(&format!("    {s}  ->  {why}\n"));
    }
    m.push_str("  Revisa esos selectores, o usa { strict: no } si de verdad pueden faltar.");
    Some(m)
}

//    Casillas

/// Estado actual de una casilla, y si el selector es siquiera una.
pub fn estado_casilla(
    conn: &Conn, session: &str, sel: &str, ms: u64, t: &Tuning,
) -> Result<(bool, String), String> {
    let cuerpo = r#"
    const e = __find(sel);
    if (!e) return null;
    const tipo = (e.type || '').toLowerCase();
    return { tipo: tipo, marcado: !!e.checked, tag: e.tagName.toLowerCase() };
    "#;
    let r = conn.call(
        "Runtime.evaluate",
        serde_json::json!({
            "expression": dom::expr_waiting(sel, cuerpo, ms, t),
            "returnByValue": true,
            "awaitPromise": true,
        }),
        Some(session), Duration::from_millis(ms + t.cdp_margin_ms),
    )?;
    let v = r.get("result").and_then(|x| x.get("value")).cloned().unwrap_or(serde_json::Value::Null);
    if v.is_null() {
        return Err(format!("no apareció '{sel}' en {ms} ms"));
    }
    let tipo = v.get("tipo").and_then(|x| x.as_str()).unwrap_or("").to_string();
    if tipo != "checkbox" && tipo != "radio" {
        let tag = v.get("tag").and_then(|x| x.as_str()).unwrap_or("?");
        return Err(format!(
            "'{sel}' no es una casilla ni un radio (es <{tag}{}>)",
            if tipo.is_empty() { String::new() } else { format!(" type={tipo}") }
        ));
    }
    Ok((v.get("marcado").and_then(|x| x.as_bool()).unwrap_or(false), tipo))
}

//    Tablas

/// Lee una `<table>` entera y devuelve sus filas como registros.
///
/// Las decisiones de aquí no salen de un ejemplo de manual sino de mirar tablas
/// reales. En tres páginas de Wikipedia, de 13 tablas: **ninguna tenía
/// `<thead>`**, 10 llevaban `<th>` dentro del cuerpo (encabezados de fila, no de
/// tabla), 4 usaban `colspan`/`rowspan` y una tenía otra tabla dentro.
///
/// Un lector que dé por hecho el `<thead>` —que es como sale la primera
/// versión— funciona perfecto en el sitio de demostración y falla en el 100% de
/// las tablas de verdad. De ahí las cuatro reglas:
///
/// 1. La cabecera se busca en cascada: `<thead>`, o la primera fila si **todas**
///    sus celdas son `<th>`, o nombres generados.
/// 2. Exigir que sean *todas* `<th>` es lo que evita confundir una fila de datos
///    que empieza con un encabezado de fila con la cabecera de la tabla.
/// 3. `colspan` y `rowspan` se expanden a una rejilla, o las columnas se
///    desalinean a partir de la primera celda combinada.
/// 4. Las filas de una tabla anidada pertenecen a la de dentro, no a esta.
pub fn table(
    conn: &Conn, session: &str, sel: &str, con_cabecera: bool, ms: u64, t: &Tuning,
) -> Result<serde_json::Value, String> {
    let cuerpo = format!(r#"
    const raiz = __find(sel);
    if (!raiz) return null;
    const tabla = raiz.tagName === 'TABLE' ? raiz : raiz.querySelector('table');
    if (!tabla) return {{ error: 'no es una <table> ni contiene ninguna (es <'
                          + raiz.tagName.toLowerCase() + '>)' }};

    // `closest` devuelve la tabla más cercana hacia arriba: si no es esta, la
    // fila es de una tabla anidada y no le pertenece.
    const filas = Array.from(tabla.querySelectorAll('tr'))
      .filter(tr => tr.closest('table') === tabla);
    if (!filas.length) return {{ error: 'la tabla no tiene filas' }};

    // Rejilla con las celdas combinadas ya expandidas.
    const rejilla = [];
    const esTh    = [];
    let r = 0;
    for (const tr of filas) {{
      if (!rejilla[r]) {{ rejilla[r] = []; esTh[r] = []; }}
      let col = 0;
      for (const cel of Array.from(tr.children)) {{
        if (cel.tagName !== 'TD' && cel.tagName !== 'TH') continue;
        while (rejilla[r][col] !== undefined) col++;
        const cs = Math.max(1, parseInt(cel.getAttribute('colspan') || '1', 10) || 1);
        const rs = Math.max(1, parseInt(cel.getAttribute('rowspan') || '1', 10) || 1);
        const txt = (cel.innerText || cel.textContent || '').trim();
        for (let i = 0; i < rs; i++) {{
          for (let j = 0; j < cs; j++) {{
            if (!rejilla[r + i]) {{ rejilla[r + i] = []; esTh[r + i] = []; }}
            rejilla[r + i][col + j] = txt;
            esTh[r + i][col + j] = (cel.tagName === 'TH');
          }}
        }}
        col += cs;
      }}
      r++;
    }}

    const ancho = rejilla.reduce((m, f) => Math.max(m, f.length), 0);
    const dentroDelThead = filas.map(tr => !!tr.closest('thead'));

    // Qué fila es la cabecera.
    let iCab = -1;
    if ({con_cabecera}) {{
      let ultimaThead = -1;
      for (let i = 0; i < filas.length; i++) if (dentroDelThead[i]) ultimaThead = i;
      if (ultimaThead >= 0) {{
        // Con cabeceras a varios pisos, la de abajo es la que nombra columnas.
        iCab = ultimaThead;
      }} else {{
        const f = esTh[0] || [];
        const celdas = (rejilla[0] || []).length;
        const todas = celdas > 0 && f.slice(0, celdas).every(x => x === true);
        if (todas) iCab = 0;
      }}
    }}

    // Nombres de columna, sin repetidos ni vacíos: un registro con dos claves
    // iguales pierde una, y con la clave vacía no hay forma de pedirla.
    const nombres = [];
    const vistos = {{}};
    for (let j = 0; j < ancho; j++) {{
      // Se colapsan los espacios: una cabecera con un <br> dentro —que en
      // Wikipedia son casi todas— daría una clave con un salto de línea, y
      // una clave así no hay quien la escriba para pedir la columna. Los
      // valores NO se tocan: ahí el salto puede ser parte del dato.
      let n = iCab >= 0
        ? String((rejilla[iCab] || [])[j] || '').replace(/\s+/g, ' ').trim()
        : '';
      if (!n) n = 'col_' + (j + 1);
      if (vistos[n]) {{ vistos[n]++; n = n + '_' + vistos[n]; }} else {{ vistos[n] = 1; }}
      nombres.push(n);
    }}

    const datos = [];
    for (let i = 0; i < rejilla.length; i++) {{
      if (i === iCab) continue;
      // Una fila de cabecera que quedara suelta arriba del thead no es un dato.
      if (iCab >= 0 && dentroDelThead[i]) continue;
      const fila = rejilla[i] || [];
      if (!fila.some(v => v !== undefined && String(v).trim() !== '')) continue;
      const reg = {{}};
      for (let j = 0; j < ancho; j++) {{
        const v = fila[j];
        reg[nombres[j]] = (v === undefined) ? null : v;
      }}
      datos.push(reg);
    }}
    return {{ filas: datos, columnas: nombres }};
    "#, con_cabecera = if con_cabecera { "true" } else { "false" });

    let r = conn.call(
        "Runtime.evaluate",
        serde_json::json!({
            "expression": dom::expr_waiting(sel, &cuerpo, ms, t),
            "returnByValue": true,
            "awaitPromise": true,
        }),
        Some(session), Duration::from_millis(ms + t.cdp_margin_ms),
    )?;

    if let Some(ex) = r.get("exceptionDetails") {
        let msg = ex.get("exception").and_then(|e| e.get("description")).and_then(|d| d.as_str())
            .or_else(|| ex.get("text").and_then(|x| x.as_str()))
            .unwrap_or("error de JavaScript");
        return Err(format!("browser.table: {msg}"));
    }

    let v = r.get("result").and_then(|x| x.get("value")).cloned().unwrap_or(serde_json::Value::Null);
    if v.is_null() {
        return Err(format!("browser.table: no apareció '{sel}' en {ms} ms"));
    }
    if let Some(e) = v.get("error").and_then(|x| x.as_str()) {
        return Err(format!("browser.table '{sel}': {e}"));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relleno(ausentes: &[&str], fallidos: &[(&str, &str)]) -> Relleno {
        Relleno {
            puestos: 0,
            ausentes: ausentes.iter().map(|s| s.to_string()).collect(),
            fallidos: fallidos.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        }
    }

    #[test]
    fn sin_problemas_no_hay_queja() {
        assert!(queja(&relleno(&[], &[])).is_none());
    }

    #[test]
    fn la_queja_nombra_cada_selector() {
        let m = queja(&relleno(&["#falta"], &[("#pais", "no hay opción \"Marte\"")])).unwrap();
        assert!(m.contains("#falta"), "{m}");
        assert!(m.contains("#pais"), "{m}");
        assert!(m.contains("Marte"), "{m}");
        // Y dice cómo desactivarlo, o el usuario solo puede rendirse.
        assert!(m.contains("strict: no"), "{m}");
    }
}
