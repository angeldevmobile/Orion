//! Descubrimiento de estructura: deducir el esquema de extracción solo.
//!
//! El problema real de un scraper no es leer datos, es **averiguar qué
//! selector usar**. Uno abre las herramientas del navegador, va bajando por el
//! árbol, prueba una clase, ve que también casa con el menú, prueba otra… y
//! veinte minutos después tiene un esquema que se rompe en la página siguiente.
//!
//! `discover` mira la página y propone el esquema: el selector de la fila que se
//! repite y un selector por cada campo con valor. No adivina la intención —no
//! sabe que eso es un "precio"—, pero te deja a un paso de `extract` en vez de a
//! veinte minutos.
//!
//! Nadie lo tiene de serie. En Python te pones a leer el HTML a mano; aquí es
//! una llamada, y devuelve además una muestra ya extraída para que compruebes de
//! un vistazo que la propuesta acierta.
//!
//! ```orion
//! e = web.discover(p)
//! show(e["row"])       -- ".quote"
//! show(e["fields"])    -- {campo_1: ".text", author: ".author", url: "a@href"}
//! show(e["sample"])    -- [{...}, {...}]  las primeras filas ya extraídas
//! ```
//!
//! Cómo lo deduce, para que no sea magia:
//!
//! 1. **La fila** es el grupo de hermanos que más se repite con la misma
//!    estructura interna. Se puntúa por cantidad y por riqueza —texto y número
//!    de campos— para no confundir un listado de productos con el menú de
//!    navegación, que también se repite pero está vacío.
//! 2. **El selector de fila** es la clase común a todas las filas que además
//!    selecciona exactamente esas y no más. Si ninguna clase sirve (sitios con
//!    clases generadas tipo `x1i10hfl`), se cae a un selector estructural y se
//!    avisa de que es frágil.
//! 3. **Los campos** son los descendientes con valor —texto, enlaces,
//!    imágenes—, cada uno con un selector relativo a la fila, y solo se
//!    conservan los que aparecen en la mayoría de las filas (uno que solo esté
//!    en una fila no es un campo, es una casualidad).

/// JavaScript que analiza la página y devuelve el esquema propuesto.
///
/// Todo ocurre dentro de la página en una sola evaluación. Las clases modernas
/// son basura (`x1i10hfl`), así que la repetición se detecta por **estructura**
/// —el tag y los tags de los hijos—, no por nombres de clase.
pub const DISCOVER_JS: &str = r#"
(() => {
  const MIN = __MIN__;

  const sig = (el) => el.tagName + '>' +
      Array.from(el.children).map(c => c.tagName).join(',');
  const txtLen = (el) => (el.innerText || el.textContent || '').trim().length;
  const esc = (c) => c.replace(/[^a-zA-Z0-9_-]/g, '\\$&');

  //  1. El grupo de hermanos repetido que más "pesa".
  let best = null;
  for (const parent of document.querySelectorAll('*')) {
    const kids = Array.from(parent.children);
    if (kids.length < MIN) continue;
    const grupos = {};
    for (const k of kids) (grupos[sig(k)] = grupos[sig(k)] || []).push(k);
    for (const s in grupos) {
      const g = grupos[s];
      if (g.length < MIN) continue;
      const txt = g.reduce((a, e) => a + txtLen(e), 0);
      const estructura = g.reduce((a, e) => a + e.querySelectorAll('*').length, 0) / g.length;
      // Cantidad × riqueza de texto × riqueza de estructura. El log evita que un
      // bloque con un párrafo enorme gane a un listado de verdad.
      const score = g.length * Math.log(2 + txt) * (1 + Math.min(estructura, 10));
      if (!best || score > best.score) best = { rows: g, score, parent };
    }
  }
  if (!best) return { error: 'no se encontro ninguna estructura repetida de al menos ' + MIN + ' elementos' };

  const rows = best.rows;

  //  2. Selector de la fila.
  const claseComun = (els) => {
    let inter = null;
    for (const e of els) {
      const toks = new Set(Array.from(e.classList));
      inter = inter === null ? toks : new Set([...inter].filter(t => toks.has(t)));
    }
    return inter ? [...inter] : [];
  };
  const comunes = claseComun(rows);
  let rowSel = null, fragil = false;

  // Una sola clase que seleccione EXACTAMENTE las filas.
  for (const c of comunes) {
    try { if (document.querySelectorAll('.' + esc(c)).length === rows.length) { rowSel = '.' + c; break; } }
    catch (e) {}
  }
  // Si no, un par de clases combinadas.
  if (!rowSel) {
    outer:
    for (let i = 0; i < comunes.length; i++)
      for (let j = i + 1; j < comunes.length; j++) {
        const sel = '.' + esc(comunes[i]) + '.' + esc(comunes[j]);
        try { if (document.querySelectorAll(sel).length === rows.length) { rowSel = '.' + comunes[i] + '.' + comunes[j]; break outer; } }
        catch (e) {}
      }
  }
  // Si ninguna clase sirve, selector estructural — y se avisa.
  if (!rowSel) {
    fragil = true;
    const tag = rows[0].tagName.toLowerCase();
    if (best.parent.id) rowSel = '#' + best.parent.id + ' > ' + tag;
    else {
      const pc = Array.from(best.parent.classList).find(x => /^[a-z][\w-]{1,24}$/i.test(x) && !/^x[0-9]/.test(x));
      rowSel = (pc ? '.' + pc + ' > ' : '') + tag;
    }
  }

  //  3. Campos, con una fila representativa (la 2ª: la 1ª a veces es distinta).
  const rep = rows[Math.min(1, rows.length - 1)];

  // Selector de un elemento relativo a su fila.
  //
  // Se prefiere SIEMPRE una clase propia que sea única dentro de la fila: es lo
  // legible y lo estable. Solo si no hay se construye un camino estructural
  // `padre > hijo` con `nth-of-type` en cada nivel.
  //
  // Y aquí está el detalle que hay que respetar: `nth-of-type` cuenta respecto
  // al PADRE, no respecto a la fila. Un índice global dentro de la fila genera
  // un `a:nth-of-type(2)` que en CSS significa otra cosa y no casa en cuanto la
  // pagina tiene el enlace anidado —el fallo justo que se comio el titulo en
  // Hacker News—.
  const relSel = (el, root) => {
    for (const c of Array.from(el.classList)) {
      try { if (root.querySelectorAll('.' + esc(c)).length === 1) return '.' + c; } catch (e) {}
    }
    const partes = [];
    let cur = el;
    while (cur && cur !== root) {
      let parte = cur.tagName.toLowerCase();
      const padre = cur.parentElement;
      if (padre) {
        const mismos = Array.from(padre.children).filter(c => c.tagName === cur.tagName);
        if (mismos.length > 1) parte += ':nth-of-type(' + (mismos.indexOf(cur) + 1) + ')';
      }
      partes.unshift(parte);
      cur = cur.parentElement;
    }
    return partes.join(' > ');
  };

  const relText = (el) => (el.innerText || el.textContent || '').trim();

  const cands = [];

  // Enlaces. Los que tienen una clase propia se conservan todos; los anónimos
  // (que solo se pueden apuntar por posición, `nth-of-type`) suelen ser ruido
  // —tags, iconos, la flecha de votar—, así que de esos se guarda solo el que
  // MÁS texto tiene. En Hacker News eso descarta la flecha de voto (sin texto)
  // y se queda con el titulo; en un listado de citas descarta los tags.
  const conClase = [], anon = [];
  for (const a of rep.querySelectorAll('a[href]')) {
    const rel = relSel(a, rep);
    // Un selector con `>` o `nth-of-type` es un camino estructural: es anónimo.
    (rel.indexOf('>') >= 0 || rel.indexOf(':nth') >= 0 ? anon : conClase).push(a);
  }
  for (const a of conClase) cands.push({ el: a, tipo: 'link' });
  // De los enlaces sin clase, uno solo: el de más texto (el titulo, no la flecha
  // de votar ni un tag). Emitirlos todos llenaria el esquema de ruido.
  if (anon.length) {
    anon.sort((x, y) => relText(y).length - relText(x).length);
    cands.push({ el: anon[0], tipo: 'link' });
    // Si ese enlace lleva un texto largo, es el titulo: se ofrece tambien su
    // texto. Un titulo suele estar SOLO dentro del enlace, no en un span aparte.
    if (relText(anon[0]).length >= 12) cands.push({ el: anon[0], tipo: 'text' });
  }

  for (const im of rep.querySelectorAll('img[src]')) cands.push({ el: im, tipo: 'img' });

  // Texto: hojas con contenido, sin contar los enlaces (ya tratados arriba).
  for (const e of rep.querySelectorAll('*')) {
    if (e.children.length === 0 && e.tagName !== 'A' && relText(e)) cands.push({ el: e, tipo: 'text' });
  }

  const muestraFilas = rows.slice(0, Math.min(rows.length, 8));
  const fields = {};
  const vistos = new Set();
  let nText = 0, nLink = 0, nImg = 0;

  for (const c of cands) {
    const rel = relSel(c.el, rep);
    // Un campo de verdad aparece en la mayoría de las filas, no en una sola.
    let hits = 0;
    for (const r of muestraFilas) { try { if (r.querySelector(rel)) hits++; } catch (e) {} }
    if (hits < Math.ceil(muestraFilas.length / 2)) continue;

    let spec, nombre;
    if (c.tipo === 'link') { spec = rel + '@href'; nombre = 'url' + (nLink++ ? '_' + nLink : ''); }
    else if (c.tipo === 'img') { spec = rel + '@src'; nombre = 'img' + (nImg++ ? '_' + nImg : ''); }
    else {
      spec = rel;
      // Un nombre de clase legible se aprovecha; la basura tipo x1i10hfl no.
      const humano = Array.from(c.el.classList)
          .find(x => /^[a-z][a-z0-9_-]{1,20}$/i.test(x) && !/^x[0-9]/.test(x) && !/^css-/.test(x));
      nombre = humano ? humano.replace(/-/g, '_') : ('campo_' + (++nText));
    }
    if (vistos.has(spec)) continue;
    vistos.add(spec);
    let n = nombre, k = 2;
    while (fields[n] !== undefined) n = nombre + '_' + (k++);
    fields[n] = spec;
  }

  //  4. Muestra: las primeras filas ya extraídas con la propuesta, para que se
  //     vea de un vistazo que acierta.
  const valor = (fila, spec) => {
    let sel = spec, attr = null;
    const at = spec.lastIndexOf('@');
    if (at >= 0) { sel = spec.slice(0, at); attr = spec.slice(at + 1); }
    let el; try { el = sel ? fila.querySelector(sel) : fila; } catch (e) { el = null; }
    if (!el) return null;
    if (attr) return el.getAttribute(attr);
    return (el.innerText || el.textContent || '').trim();
  };
  const sample = [];
  for (const r of rows.slice(0, 3)) {
    const reg = {};
    for (const k in fields) reg[k] = valor(r, fields[k]);
    sample.push(reg);
  }

  return { row: rowSel, count: rows.length, fragil, fields, sample };
})()
"#;
