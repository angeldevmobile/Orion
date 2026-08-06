//! Módulo `browser` — automatización web sobre CDP.
//!
//! API en inglés y de un solo nivel: funciones del módulo con el handle como
//! primer argumento, igual que `db`, `frame` o `ws`. No hay builders, ni
//! cadenas de acciones, ni contextos anidados.
//!
//! ```orion
//! use "browser" as web
//!
//! with b = web.open() {
//!     p = web.page(b)
//!     web.goto(p, "https://ejemplo.dev")
//!     show(web.title(p))
//! }
//! ```
//!
//! El `with` funciona sin nada extra: desugara a `web.free(b)` incluso si el
//! cuerpo lanza un error, y `free` cierra en cascada las pestañas del navegador.
//!
//! Esta es la primera entrega: transporte, arranque y navegación. La extracción
//! declarativa, la interacción (click/type) y las ventanas emergentes se apoyan
//! sobre esto y llegan después.

pub mod cdp;
pub mod dom;
pub mod extract;
pub mod input;
pub mod launch;

use crate::eval_value::EvalValue;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use cdp::Conn;
use launch::{LaunchOpts, Launched, Tuning};

//    Registro de handles
//
// Navegadores y pestañas comparten numeración y mapa: así `free(h)` funciona
// con cualquiera de los dos, que es lo que necesita `with` para no obligar al
// programa a saber qué clase de cosa está soltando.

struct BrowserState {
    conn:      Arc<Conn>,
    proc:      std::process::Child,
    exe:       String,
    user_data: std::path::PathBuf,
    temporal:  bool,
    timeout:   Duration,
    tuning:    Tuning,
    pages:     Vec<u64>,
}

struct PageState {
    browser:   u64,
    target_id: String,
    session:   String,
}

enum Handle {
    Browser(BrowserState),
    Page(PageState),
}

static HANDLES: OnceLock<Mutex<HashMap<u64, Handle>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn handles() -> &'static Mutex<HashMap<u64, Handle>> {
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn new_id() -> u64 { NEXT_ID.fetch_add(1, Ordering::SeqCst) }

/// Datos de una pestaña más la conexión de su navegador.
///
/// Se devuelven copiados en vez de mantener el candado: una llamada CDP puede
/// tardar segundos y bloquear el registro mientras tanto dejaría clavado a
/// cualquier otra tarea que solo quisiera abrir una pestaña.
fn page_ctx(h: u64) -> Result<(Arc<Conn>, String, Duration, Tuning), String> {
    let reg = handles().lock().unwrap();
    let Some(Handle::Page(p)) = reg.get(&h) else {
        return Err(match reg.get(&h) {
            Some(Handle::Browser(_)) => format!("browser: {h} es un navegador, no una pestaña"),
            _ => format!("browser: la pestaña {h} no existe (¿ya se cerró?)"),
        });
    };
    let session = p.session.clone();
    let Some(Handle::Browser(b)) = reg.get(&p.browser) else {
        return Err(format!("browser: el navegador de la pestaña {h} ya no existe"));
    };
    Ok((Arc::clone(&b.conn), session, b.timeout, b.tuning.clone()))
}

fn browser_ctx(h: u64) -> Result<(Arc<Conn>, Duration, Tuning), String> {
    let reg = handles().lock().unwrap();
    match reg.get(&h) {
        Some(Handle::Browser(b)) => Ok((Arc::clone(&b.conn), b.timeout, b.tuning.clone())),
        Some(Handle::Page(_))    => Err(format!("browser: {h} es una pestaña, no un navegador")),
        None                     => Err(format!("browser: el navegador {h} no existe")),
    }
}

//    Dispatch

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        "open"    => open(&args),
        "page"    => page(&args),
        "goto"    => goto(&args),
        "title"   => read_str(&args, "document.title", "browser.title"),
        "url"     => read_str(&args, "location.href", "browser.url"),
        "eval"    => eval(&args),
        "content" => read_str(&args, "document.documentElement.outerHTML", "browser.content"),
        "pages"   => pages(&args),

        // Interacción
        "click"      => do_click(&args, "left", 1),
        "dblclick"   => do_click(&args, "left", 2),
        "rightclick" => do_click(&args, "right", 1),
        "hover"      => do_hover(&args),
        "drag"       => do_drag(&args),
        "scroll"     => do_scroll(&args),
        "type"       => do_type(&args),
        "press"      => do_press(&args),
        "select"     => do_select(&args),

        // Modales y ventanas
        "dialogs"     => do_dialogs(&args),
        "click_opens" => do_click_opens(&args),

        // Lectura del DOM.
        //
        // Las que devuelven contenido esperan a que lo haya; las que informan
        // del estado responden sobre el instante actual y no esperan nunca.
        "wait"    => do_wait(&args),
        "text"    => query(&args, "browser.text", Espera::Si,
                        "const e = __find(sel); return e ? (e.innerText || e.textContent || '').trim() : null;"),
        "html"    => query(&args, "browser.html", Espera::Si,
                        "const e = __find(sel); return e ? e.innerHTML : null;"),
        "texts"   => query(&args, "browser.texts", Espera::Si,
                        "return __findAll(sel).map(e => (e.innerText || e.textContent || '').trim());"),
        "exists"  => query(&args, "browser.exists", Espera::No, "return !!__find(sel);"),
        "count"   => query(&args, "browser.count", Espera::No, "return __findAll(sel).length;"),
        "visible" => query(&args, "browser.visible", Espera::No, r#"
                        const e = __find(sel);
                        if (!e) return false;
                        const r = e.getBoundingClientRect();
                        const s = getComputedStyle(e);
                        return r.width > 0 && r.height > 0
                            && s.display !== 'none' && s.visibility !== 'hidden' && s.opacity !== '0';
                     "#),
        "attr"    => do_attr(&args),
        "extract"    => do_extract(&args),
        "extract_to" => do_extract_to(&args),

        // Captura
        "screenshot" => do_screenshot(&args),
        // `close` y `free` son lo mismo: `free` existe porque es el nombre que
        // invoca el desugar de `with`, y `close` porque es el que la gente
        // escribe cuando cierra a mano.
        "free" | "close" => free(&args),
        "info"    => info(),
        f => Err(format!("browser.{f}() no existe")),
    }
}

//    open(opts?) → handle del navegador

fn open(args: &[EvalValue]) -> Result<EvalValue, String> {
    let opts = parse_opts(args.first())?;
    let tuning = parse_tuning(args.first());
    let timeout = opts.timeout;

    let Launched { child, ws_url, exe, user_data, temporal } = launch::launch(&opts, &tuning)?;
    let limits = cdp::Limits {
        max_events: tuning.max_events,
        idle_poll:  Duration::from_millis(tuning.idle_poll_ms),
        send:       Duration::from_millis(tuning.send_ms),
    };
    let conn = match Conn::connect(&ws_url, limits) {
        Ok(c) => c,
        Err(e) => {
            // Si no se puede hablar con él, no dejarlo suelto comiendo memoria.
            let mut c = child;
            let _ = c.kill();
            let _ = c.wait();
            if temporal { let _ = std::fs::remove_dir_all(&user_data); }
            return Err(e);
        }
    };

    let id = new_id();
    handles().lock().unwrap().insert(id, Handle::Browser(BrowserState {
        conn, proc: child, exe, user_data, temporal, timeout, tuning, pages: Vec::new(),
    }));
    Ok(EvalValue::Int(id as i64))
}

//    page(browser) → handle de pestaña

fn page(args: &[EvalValue]) -> Result<EvalValue, String> {
    let b = arg_handle(args, 0, "browser.page(navegador)")?;
    let (conn, timeout, _t) = browser_ctx(b)?;

    let creado = conn.call(
        "Target.createTarget",
        serde_json::json!({ "url": "about:blank" }),
        None, timeout,
    )?;
    let target_id = creado["targetId"].as_str()
        .ok_or("browser.page: el navegador no devolvió targetId")?
        .to_string();

    // `flatten` multiplexa la sesión sobre el mismo socket; sin él haría falta
    // una conexión por pestaña.
    let adjunto = conn.call(
        "Target.attachToTarget",
        serde_json::json!({ "targetId": target_id, "flatten": true }),
        None, timeout,
    )?;
    let session = adjunto["sessionId"].as_str()
        .ok_or("browser.page: el navegador no devolvió sessionId")?
        .to_string();

    // Los eventos de página hacen falta para saber cuándo terminó de cargar.
    conn.call("Page.enable", serde_json::json!({}), Some(&session), timeout)?;

    let id = new_id();
    let mut reg = handles().lock().unwrap();
    reg.insert(id, Handle::Page(PageState { browser: b, target_id, session }));
    if let Some(Handle::Browser(bs)) = reg.get_mut(&b) {
        bs.pages.push(id);
    }
    Ok(EvalValue::Int(id as i64))
}

//    goto(page, url) → url final

fn goto(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.goto(pestaña, url)")?;
    let url = args.get(1).map(to_str)
        .ok_or("browser.goto(pestaña, url): falta la url")?;
    let (conn, session, timeout, t) = page_ctx(p)?;

    // La marca se toma ANTES de navegar: si no, una carga anterior podría
    // hacerse pasar por esta y `goto` volvería antes de tiempo.
    let marca = conn.event_mark();

    let r = conn.call(
        "Page.navigate",
        serde_json::json!({ "url": url }),
        Some(&session), timeout,
    )?;
    if let Some(err) = r.get("errorText").and_then(|e| e.as_str()) {
        return Err(format!("browser.goto '{url}': {err}"));
    }

    // Que la carga no termine no es un error por sí solo: hay páginas que dejan
    // peticiones abiertas para siempre y su contenido ya está ahí.
    let cargo = conn.wait_event("Page.loadEventFired", Some(&session), marca, timeout)?;

    // Pero hay una causa concreta que sí conviene delatar: un alert/confirm
    // durante la carga deja la página congelada y sin nadie que lo atienda. Sin
    // este aviso, el síntoma es un timeout genérico imposible de diagnosticar.
    if cargo.is_none() {
        if let Some(ev) = conn.wait_event(
            "Page.javascriptDialogOpening", Some(&session), marca, Duration::from_millis(0)
        )? {
            let texto = ev.params.get("message").and_then(|m| m.as_str()).unwrap_or("");
            let tipo  = ev.params.get("type").and_then(|t| t.as_str()).unwrap_or("dialog");
            // Se descarta para no dejar el navegador clavado esperando a nadie.
            let _ = conn.call(
                "Page.handleJavaScriptDialog",
                serde_json::json!({ "accept": false }),
                Some(&session), Duration::from_millis(t.close_ms),
            );
            return Err(format!(
                "browser.goto '{url}': la página abrió un {tipo} ({texto:?}) y quedó bloqueada."
            ));
        }
    }

    match evaluate(&conn, &session, "location.href", timeout) {
        Ok(EvalValue::Str(s)) => Ok(EvalValue::Str(s)),
        _ => Ok(EvalValue::Str(url)),
    }
}

//    title / url / content

fn read_str(args: &[EvalValue], expr: &str, quien: &str) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, &format!("{quien}(pestaña)"))?;
    let (conn, session, timeout, _t) = page_ctx(p)?;
    evaluate(&conn, &session, expr, timeout)
}

//    eval(page, js)

fn eval(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.eval(pestaña, js)")?;
    let js = args.get(1).map(to_str)
        .ok_or("browser.eval(pestaña, js): falta el código")?;
    let (conn, session, timeout, _t) = page_ctx(p)?;
    evaluate(&conn, &session, &js, timeout)
}

/// Evalúa JavaScript en la página y trae el resultado ya convertido.
///
/// `returnByValue` es lo que evita traerse referencias a objetos del DOM: solo
/// cruza el socket el valor final, serializado. Es la decisión que mantiene la
/// memoria de Orion proporcional a los datos pedidos y no al peso de la página.
fn evaluate(conn: &Conn, session: &str, expr: &str, timeout: Duration) -> Result<EvalValue, String> {
    evaluate_awaiting(conn, session, expr, timeout, true)
}

fn evaluate_awaiting(
    conn: &Conn, session: &str, expr: &str, timeout: Duration, await_promise: bool,
) -> Result<EvalValue, String> {
    let r = conn.call(
        "Runtime.evaluate",
        serde_json::json!({
            "expression": expr,
            "returnByValue": true,
            "awaitPromise": await_promise,
        }),
        Some(session), timeout,
    )?;

    // Una excepción del JS llega como resultado correcto de CDP con un
    // `exceptionDetails` dentro; si no se mira, los errores del usuario se
    // convierten en `null` silenciosos.
    if let Some(ex) = r.get("exceptionDetails") {
        let msg = ex.get("exception").and_then(|e| e.get("description")).and_then(|d| d.as_str())
            .or_else(|| ex.get("text").and_then(|t| t.as_str()))
            .unwrap_or("error de JavaScript");
        return Err(format!("browser.eval: {msg}"));
    }

    Ok(json_to_eval(r.get("result").and_then(|x| x.get("value")).cloned()
        .unwrap_or(serde_json::Value::Null)))
}

//    Interacción
//
// Todas esperan al selector antes de actuar. La espera implícita no es una
// comodidad: un scraper que exige acordarse de poner `wait` es un scraper que
// falla de forma intermitente, y esos son los que nadie consigue depurar.

/// Milisegundos de espera de un selector.
///
/// Tres niveles, del más concreto al más general: lo que diga esta llamada, lo
/// que se fijó al abrir el navegador, y el default. Antes solo existía el
/// primero y el último, así que quien quisiera otro plazo tenía que repetirlo en
/// **cada** llamada — que es la forma más cansada de tener una constante fijada.
///
/// El argumento admite un número (los milisegundos) o un dict de opciones, para
/// que el caso simple siga siendo `click(p, sel)` y el complicado no necesite
/// una función aparte.
fn espera_de(args: &[EvalValue], i: usize, t: &Tuning) -> u64 {
    match args.get(i) {
        Some(EvalValue::Dict(m)) => m.get("wait").and_then(|v| to_u64(v).ok()).unwrap_or(t.wait_ms),
        Some(v) => to_u64(v).unwrap_or(t.wait_ms),
        None => t.wait_ms,
    }
}

/// Afinado leído de las opciones de `open`.
///
/// Los parámetros de política viven en la raíz (`wait`, `drag_steps`…) porque se
/// tocan de verdad; los de mecanismo van bajo `tuning` para no ensuciar la API
/// de uso diario. Lo que no se indique conserva su default.
fn parse_tuning(v: Option<&EvalValue>) -> Tuning {
    let mut t = Tuning::default();
    let Some(EvalValue::Dict(m)) = v else { return t };

    let u64_de = |d: &IndexMap<String, EvalValue>, k: &str| -> Option<u64> {
        d.get(k).and_then(|x| to_u64(x).ok())
    };

    // Política — en la raíz de las opciones.
    if let Some(x) = u64_de(m, "wait")          { t.wait_ms       = x; }
    if let Some(x) = u64_de(m, "retry")         { t.retry_ms      = x.max(1); }
    if let Some(x) = u64_de(m, "cdp_margin")    { t.cdp_margin_ms = x; }
    if let Some(x) = u64_de(m, "drag_steps")    { t.drag_steps    = x.max(1) as u32; }
    if let Some(x) = u64_de(m, "force_layers")  { t.force_layers  = x as u32; }
    if let Some(x) = u64_de(m, "iframe_depth")  { t.iframe_depth  = x as u32; }
    if let Some(x) = m.get("hit_inset").and_then(to_f64) { t.hit_inset = x.max(0.0); }

    // Mecanismo — bajo `tuning`.
    if let Some(EvalValue::Dict(g)) = m.get("tuning") {
        if let Some(x) = u64_de(g, "max_events")    { t.max_events    = x.max(1) as usize; }
        if let Some(x) = u64_de(g, "idle_poll")     { t.idle_poll_ms  = x.max(1); }
        if let Some(x) = u64_de(g, "close_timeout") { t.close_ms      = x; }
        if let Some(x) = u64_de(g, "send_timeout")  { t.send_ms       = x; }
        if let Some(x) = u64_de(g, "cleanup_tries") { t.cleanup_tries = x as u32; }
    }
    t
}

/// ¿Se pidió atravesar lo que tape el elemento?
fn force_de(args: &[EvalValue], i: usize) -> input::Force {
    match args.get(i) {
        Some(EvalValue::Dict(m)) => match m.get("force").map(truthy) {
            Some(true) => input::Force::Si,
            _ => input::Force::No,
        },
        _ => input::Force::No,
    }
}

fn do_click(args: &[EvalValue], boton: &str, veces: i64) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.click(pestaña, selector)")?;
    let sel = args.get(1).map(to_str)
        .ok_or("browser.click(pestaña, selector): falta el selector")?;
    let (conn, session, timeout, t) = page_ctx(p)?;
    input::click(&conn, &session, &sel, boton, veces,
                 espera_de(args, 2, &t), force_de(args, 2), &t, timeout)?;
    Ok(EvalValue::Bool(true))
}

fn do_hover(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.hover(pestaña, selector)")?;
    let sel = args.get(1).map(to_str)
        .ok_or("browser.hover(pestaña, selector): falta el selector")?;
    let (conn, session, timeout, t) = page_ctx(p)?;
    input::hover(&conn, &session, &sel, espera_de(args, 2, &t), &t, timeout)?;
    Ok(EvalValue::Bool(true))
}

fn do_drag(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.drag(pestaña, origen, destino)")?;
    let a = args.get(1).map(to_str).ok_or("browser.drag: falta el origen")?;
    let z = args.get(2).map(to_str).ok_or("browser.drag: falta el destino")?;
    let (conn, session, timeout, t) = page_ctx(p)?;
    input::drag(&conn, &session, &a, &z, espera_de(args, 3, &t), &t, timeout)?;
    Ok(EvalValue::Bool(true))
}

fn do_scroll(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.scroll(pestaña, dx, dy)")?;
    let dx = args.get(1).and_then(|v| to_f64(v)).unwrap_or(0.0);
    let dy = args.get(2).and_then(|v| to_f64(v)).unwrap_or(0.0);
    let (conn, session, timeout, _t) = page_ctx(p)?;
    input::scroll(&conn, &session, dx, dy, timeout)?;
    Ok(EvalValue::Bool(true))
}

fn do_type(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.type(pestaña, selector, texto)")?;
    let sel = args.get(1).map(to_str).ok_or("browser.type: falta el selector")?;
    let txt = args.get(2).map(to_str).ok_or("browser.type: falta el texto")?;
    // Limpiar antes de escribir es lo que casi siempre se quiere; lo contrario
    // se pide con `{ clear: no }`.
    let limpiar = match args.get(3) {
        Some(EvalValue::Dict(m)) => m.get("clear").map(truthy).unwrap_or(true),
        Some(v) => truthy(v),
        None => true,
    };
    let (conn, session, timeout, t) = page_ctx(p)?;
    input::type_text(&conn, &session, &sel, &txt, limpiar,
                     espera_de(args, 3, &t), force_de(args, 3), &t, timeout)?;
    Ok(EvalValue::Bool(true))
}

fn do_press(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.press(pestaña, tecla)")?;
    let k = args.get(1).map(to_str)
        .ok_or_else(|| format!("browser.press(pestaña, tecla): falta la tecla.\n  Admitidas: {}", input::TECLAS))?;
    let (conn, session, timeout, _t) = page_ctx(p)?;
    input::press(&conn, &session, &k, timeout)?;
    Ok(EvalValue::Bool(true))
}

/// Elige una opción de un `<select>` nativo.
///
/// Un `<select>` abre un desplegable del **sistema operativo**, fuera del DOM:
/// ningún clic sintético ni real puede navegarlo, y por eso Selenium tiene una
/// clase `Select` aparte. Aquí se asigna el valor y se emiten `input` y `change`
/// como haría el navegador, que es lo que el sitio está escuchando.
///
/// Acepta el `value`, el texto visible o el índice: quien escribe el scraper ve
/// el texto en pantalla, no el `value` del HTML.
fn do_select(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.select(pestaña, selector, opción)")?;
    let sel = args.get(1).map(to_str).ok_or("browser.select: falta el selector")?;
    let opt = args.get(2).map(to_str).ok_or("browser.select: falta la opción")?;
    let (conn, session, timeout, t) = page_ctx(p)?;

    let quiere = serde_json::Value::String(opt.clone()).to_string();
    let cuerpo = format!(r#"
    const el = __find(sel);
    if (!el) return {{ ok: false, why: 'no existe' }};
    if (el.tagName !== 'SELECT') return {{ ok: false, why: 'no es un <select> (es <' + el.tagName.toLowerCase() + '>)' }};
    const q = {quiere};
    const ops = Array.from(el.options);
    let i = ops.findIndex(o => o.value === q);
    if (i < 0) i = ops.findIndex(o => (o.textContent || '').trim() === q);
    if (i < 0 && /^[0-9]+$/.test(q)) i = parseInt(q, 10);
    if (i < 0 || i >= ops.length) {{
      return {{ ok: false, why: 'no hay opción ' + JSON.stringify(q),
                opciones: ops.map(o => (o.textContent || '').trim()) }};
    }}
    el.selectedIndex = i;
    el.dispatchEvent(new Event('input',  {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
    return {{ ok: true, value: ops[i].value, text: (ops[i].textContent || '').trim() }};
    "#);

    let ms = espera_de(args, 3, &t);
    let v = evaluate_awaiting(
        &conn, &session,
        &dom::expr_waiting(&sel, &cuerpo, ms, &t),
        Duration::from_millis(ms + t.cdp_margin_ms), true,
    )?;
    let _ = timeout;

    if let EvalValue::Dict(m) = &v {
        if !matches!(m.get("ok"), Some(EvalValue::Bool(true))) {
            let why = m.get("why").map(to_str).unwrap_or_default();
            let ops = match m.get("opciones") {
                Some(EvalValue::List(l)) if !l.is_empty() => format!(
                    "\n  Opciones: {}",
                    l.iter().map(to_str).collect::<Vec<_>>().join(", ")
                ),
                _ => String::new(),
            };
            return Err(format!("browser.select '{sel}': {why}{ops}"));
        }
        return Ok(m.get("value").cloned().unwrap_or(EvalValue::Bool(true)));
    }
    Ok(v)
}

/// Espera a que una pestaña termine de cargar.
///
/// Se pregunta por `readyState` en vez de esperar el evento de carga porque el
/// evento puede haber pasado ya antes de que nos adjuntáramos, y entonces la
/// espera no terminaría nunca.
fn wait_ready(conn: &Conn, session: &str, ms: u64, t: &Tuning) -> Result<(), String> {
    let reintento = t.retry_ms;
    let js = format!(r#"(() => {{
      return new Promise((resolve) => {{
        if (document.readyState === 'complete') return resolve(true);
        const limite = Date.now() + {ms};
        const iv = setInterval(() => {{
          if (document.readyState === 'complete' || Date.now() >= limite) {{
            clearInterval(iv); resolve(true);
          }}
        }}, {reintento});
      }});
    }})()"#);
    conn.call(
        "Runtime.evaluate",
        serde_json::json!({ "expression": js, "returnByValue": true, "awaitPromise": true }),
        Some(session), Duration::from_millis(ms + t.cdp_margin_ms),
    )?;
    Ok(())
}

/// Política para los diálogos nativos de una pestaña.
///
/// `accept`, `dismiss`, o `answer:<texto>` para un `prompt`. Se declara una vez
/// y vale para toda la sesión, en vez del registro previo a cada acción que usan
/// Playwright y Selenium: un diálogo abierto por un temporizador de la página no
/// tiene ninguna llamada tuya a la que engancharse.
fn do_dialogs(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.dialogs(pestaña, política)")?;
    let (conn, session, _to, _t) = page_ctx(p)?;

    let pol = args.get(1).map(to_str);
    match pol.as_deref() {
        None | Some("") | Some("off") | Some("none") => {
            conn.set_dialog_policy(&session, None);
        }
        Some(v) => {
            let valido = matches!(v, "accept" | "aceptar" | "dismiss" | "rechazar" | "yes" | "ok")
                || v.starts_with("answer:") || v.starts_with("responder:");
            if !valido {
                return Err(format!(
                    "browser.dialogs: política '{v}' desconocida.\n  Admitidas: accept, dismiss, answer:<texto>, off"
                ));
            }
            conn.set_dialog_policy(&session, Some(v.to_string()));
        }
    }
    Ok(EvalValue::Bool(true))
}

/// Clic que abre una pestaña nueva; devuelve el handle de la que se abrió.
///
/// Playwright necesita envolver el clic en `expect_popup` **antes** de hacerlo, y
/// Selenium te hace listar los handles de ventana y adivinar cuál es la nueva.
/// Aquí es una llamada, y la carrera entre el clic y la aparición de la pestaña
/// la resuelve el módulo: la marca de eventos se toma antes de clicar.
fn do_click_opens(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.click_opens(pestaña, selector)")?;
    let sel = args.get(1).map(to_str).ok_or("browser.click_opens: falta el selector")?;
    let (conn, session, timeout, t) = page_ctx(p)?;
    let browser = match handles().lock().unwrap().get(&p) {
        Some(Handle::Page(ps)) => ps.browser,
        _ => return Err(format!("browser.click_opens: la pestaña {p} no existe")),
    };

    // Descubrir targets antes de clicar: si se activa después, el evento de la
    // pestaña recién creada ya habrá pasado.
    conn.call("Target.setDiscoverTargets", serde_json::json!({ "discover": true }),
              None, timeout)?;
    let marca = conn.event_mark();

    input::click(&conn, &session, &sel, "left", 1,
                 espera_de(args, 2, &t), force_de(args, 2), &t, timeout)?;

    let ms = espera_de(args, 2, &t);
    let ev = conn.wait_event("Target.targetCreated", None, marca, Duration::from_millis(ms))?
        .filter(|e| e.params.get("targetInfo")
            .and_then(|t| t.get("type")).and_then(|t| t.as_str()) == Some("page"))
        .ok_or_else(|| format!(
            "browser.click_opens '{sel}': el clic no abrió ninguna pestaña en {ms} ms"
        ))?;

    let target_id = ev.params["targetInfo"]["targetId"].as_str()
        .ok_or("browser.click_opens: la pestaña nueva no trae targetId")?
        .to_string();

    let adjunto = conn.call(
        "Target.attachToTarget",
        serde_json::json!({ "targetId": target_id, "flatten": true }),
        None, timeout,
    )?;
    let nueva_ses = adjunto["sessionId"].as_str()
        .ok_or("browser.click_opens: no se pudo adjuntar a la pestaña nueva")?
        .to_string();
    conn.call("Page.enable", serde_json::json!({}), Some(&nueva_ses), timeout)?;

    // Nos enganchamos en `targetCreated`, que ocurre antes de que la pestaña
    // tenga contenido. Sin esperar a que termine de cargar, el primer `title`
    // o `text` sobre ella devolvería vacío y parecería que la página está mal.
    wait_ready(&conn, &nueva_ses, ms, &t)?;

    let id = new_id();
    let mut reg = handles().lock().unwrap();
    reg.insert(id, Handle::Page(PageState {
        browser, target_id, session: nueva_ses,
    }));
    if let Some(Handle::Browser(bs)) = reg.get_mut(&browser) {
        bs.pages.push(id);
    }
    Ok(EvalValue::Int(id as i64))
}

//    Lectura del DOM

fn do_wait(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.wait(pestaña, selector, ms?)")?;
    let sel = args.get(1).map(to_str).ok_or("browser.wait: falta el selector")?;
    let (conn, session, _to, t) = page_ctx(p)?;
    let ms = espera_de(args, 2, &t);
    if !dom::wait_for(&conn, &session, &sel, ms, &t)? {
        return Err(format!("browser.wait: '{sel}' no apareció en {ms} ms"));
    }
    Ok(EvalValue::Bool(true))
}

/// ¿Esta lectura debe esperar a que haya contenido?
#[derive(Clone, Copy, PartialEq)]
enum Espera { Si, No }

/// Ejecuta una consulta sobre el DOM con el selector ya inyectado.
///
/// Toda la lectura pasa por aquí y por tanto por **una** evaluación: es lo que
/// evita el patrón de Selenium de una petición HTTP por cada atributo leído.
fn query(args: &[EvalValue], quien: &str, espera: Espera, cuerpo: &str) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, &format!("{quien}(pestaña, selector)"))?;
    let sel = args.get(1).map(to_str)
        .ok_or_else(|| format!("{quien}(pestaña, selector): falta el selector"))?;
    let (conn, session, timeout, t) = page_ctx(p)?;

    let (js, limite) = match espera {
        Espera::No => (dom::expr(&sel, cuerpo, &t), timeout),
        Espera::Si => {
            let ms = espera_de(args, 2, &t);
            (dom::expr_waiting(&sel, cuerpo, ms, &t), Duration::from_millis(ms + t.cdp_margin_ms))
        }
    };
    evaluate_awaiting(&conn, &session, &js, limite, espera == Espera::Si)
}

fn do_attr(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.attr(pestaña, selector, atributo)")?;
    let sel = args.get(1).map(to_str).ok_or("browser.attr: falta el selector")?;
    let at  = args.get(2).map(to_str).ok_or("browser.attr: falta el nombre del atributo")?;
    let (conn, session, timeout, t) = page_ctx(p)?;

    // El nombre del atributo también se serializa: puede traer comillas.
    let a = serde_json::Value::String(at).to_string();
    let cuerpo = format!("const e = __find(sel); return e ? e.getAttribute({a}) : null;");

    let ms = espera_de(args, 3, &t);
    let _ = timeout;
    evaluate_awaiting(
        &conn, &session,
        &dom::expr_waiting(&sel, &cuerpo, ms, &t),
        Duration::from_millis(ms + t.cdp_margin_ms),
        true,
    )
}

//    Extracción declarativa

/// `extract(pestaña, selector_de_fila, esquema, opts?)` → lista de dicts.
///
/// El esquema entero se compila a una sola evaluación dentro de la página. Ver
/// `extract.rs` para la gramática de las especificaciones.
fn do_extract(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.extract(pestaña, selector, esquema)")?;
    let fila_sel = args.get(1).map(to_str)
        .ok_or("browser.extract(pestaña, selector, esquema): falta el selector de fila")?;

    let Some(EvalValue::Dict(esquema)) = args.get(2) else {
        return Err(concat!(
            "browser.extract: el esquema debe ser un diccionario campo → especificación.
",
            "  Ejemplo: { nombre: \".title\", precio: \".price|num\" }"
        ).to_string());
    };
    if esquema.is_empty() {
        return Err("browser.extract: el esquema está vacío".into());
    }

    let campos: Vec<extract::Campo> = esquema.iter()
        .map(|(k, v)| extract::parse_campo(k, &to_str(v)))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("browser.extract: {e}"))?;

    let (conn, session, _to, t) = page_ctx(p)?;
    let ms = espera_de(args, 3, &t);
    let r = extract::extract(&conn, &session, &fila_sel, &campos, ms, &t)?;

    // Un campo vacío en TODAS las filas es casi siempre un selector equivocado,
    // no un dato ausente. Callarlo devolvería una lista de nulls que parece
    // buena y revienta mucho más adelante, que es el fallo clásico de
    // BeautifulSoup. Con `strict: no` se acepta y se sigue.
    let estricto = match args.get(3) {
        Some(EvalValue::Dict(m)) => m.get("strict").map(truthy).unwrap_or(true),
        _ => true,
    };
    if estricto && !r.muertos.is_empty() {
        let detalle: Vec<String> = r.muertos.iter()
            .map(|(campo, spec)| format!("    {campo}  ←  {spec}"))
            .collect();
        return Err(format!(
            "browser.extract: {} campo(s) no encontraron nada en ninguna de las {} filas:
{}
  Revisa esos selectores, o usa {{ strict: no }} si de verdad pueden faltar.",
            r.muertos.len(), r.filas, detalle.join("
")
        ));
    }

    Ok(json_to_eval(r.json))
}

/// `extract_to(pestaña, urls, selector, esquema, salida, opts?)` → resumen.
///
/// Recorre las URLs **reutilizando una sola pestaña** y vuelca lo extraído a
/// disco según se obtiene. Las dos cosas son deliberadas: abrir una pestaña por
/// URL multiplica la memoria del navegador, y acumular el listado entero antes
/// de guardar es lo que hace que un scraper de Python se coma la RAM en cuanto
/// el volumen crece.
fn do_extract_to(args: &[EvalValue]) -> Result<EvalValue, String> {
    const USO: &str = "browser.extract_to(pestaña, urls, selector, esquema, salida)";
    let p = arg_handle(args, 0, USO)?;

    // Una sola URL también vale: obligar a envolverla en una lista sería
    // ceremonia por nada.
    let urls: Vec<String> = match args.get(1) {
        Some(EvalValue::List(l)) => l.iter().map(to_str).collect(),
        Some(v) => vec![to_str(v)],
        None => return Err(format!("{USO}: faltan las urls")),
    };
    if urls.is_empty() { return Err("browser.extract_to: la lista de urls está vacía".into()); }

    let fila_sel = args.get(2).map(to_str).ok_or(format!("{USO}: falta el selector de fila"))?;
    let Some(EvalValue::Dict(esquema)) = args.get(3) else {
        return Err(format!("{USO}: el esquema debe ser un diccionario campo → especificación"));
    };
    let salida = args.get(4).map(to_str).ok_or(format!("{USO}: falta la ruta de salida"))?;

    let campos: Vec<extract::Campo> = esquema.iter()
        .map(|(k, v)| extract::parse_campo(k, &to_str(v)))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("browser.extract_to: {e}"))?;

    let (conn, session, timeout, t) = page_ctx(p)?;
    let ms = espera_de(args, 5, &t);
    let chunk = match args.get(5) {
        Some(EvalValue::Dict(m)) => m.get("chunk").and_then(|v| to_u64(v).ok()).unwrap_or(50_000),
        _ => 50_000,
    } as usize;

    let headers: Vec<String> = campos.iter().map(|c| c.nombre.clone()).collect();
    let mut volcador = extract::Volcador::nuevo(&salida, headers.clone(), chunk)
        .map_err(|e| format!("browser.extract_to: {e}"))?;

    let mut errores: Vec<EvalValue> = Vec::new();
    let mut vacias: Vec<EvalValue> = Vec::new();
    let mut ok = 0usize;
    let mut muertos_globales: Vec<(String, String)> = Vec::new();

    for url in &urls {
        // Un fallo en una URL no aborta el recorrido: en una tanda de veinte,
        // morir por un 404 tira el trabajo de las diecinueve buenas. Se anota y
        // se sigue, y el resumen dice exactamente qué pasó.
        if let Err(e) = navegar(&conn, &session, url, timeout) {
            errores.push(EvalValue::Str(format!("{url}: {e}")));
            continue;
        }
        match extract::extract(&conn, &session, &fila_sel, &campos, ms, &t) {
            Ok(r) => {
                if r.filas > 0 && !r.muertos.is_empty() && muertos_globales.is_empty() {
                    muertos_globales = r.muertos.clone();
                }
                // Una página que carga pero no da filas no es un error de red y
                // por eso pasa desapercibida: un 404 con plantilla, un redirect
                // al login, o el selector que dejó de valer en esa sección. Se
                // anota aparte para que un recorrido no pierda páginas en
                // silencio, que es el fallo que nadie detecta hasta que faltan
                // datos en el informe.
                if r.filas == 0 { vacias.push(EvalValue::Str(url.clone())); }
                if let Some(arr) = r.json.as_array() {
                    for reg in arr {
                        let fila: Vec<String> = headers.iter()
                            .map(|h| extract::a_texto(reg.get(h).unwrap_or(&serde_json::Value::Null)))
                            .collect();
                        volcador.escribir(fila).map_err(|e| format!("browser.extract_to: {e}"))?;
                    }
                }
                ok += 1;
            }
            Err(e) => errores.push(EvalValue::Str(format!("{url}: {e}"))),
        }
    }

    let (filas, archivos) = volcador.cerrar().map_err(|e| format!("browser.extract_to: {e}"))?;

    let estricto = match args.get(5) {
        Some(EvalValue::Dict(m)) => m.get("strict").map(truthy).unwrap_or(true),
        _ => true,
    };
    if estricto && !muertos_globales.is_empty() {
        let detalle: Vec<String> = muertos_globales.iter()
            .map(|(campo, spec)| format!("    {campo}  ←  {spec}"))
            .collect();
        return Err(format!(
            "browser.extract_to: {} campo(s) no encontraron nada en ninguna fila:
{}
  Los datos ya se escribieron en {salida}. Revisa esos selectores, o usa {{ strict: no }}.",
            muertos_globales.len(), detalle.join("
")
        ));
    }

    let mut m: IndexMap<String, EvalValue> = IndexMap::new();
    m.insert("rows".into(),   EvalValue::Int(filas as i64));
    m.insert("urls".into(),   EvalValue::Int(urls.len() as i64));
    m.insert("ok".into(),     EvalValue::Int(ok as i64));
    m.insert("failed".into(), EvalValue::Int(errores.len() as i64));
    m.insert("empty".into(),  EvalValue::List(vacias));
    m.insert("files".into(),  EvalValue::List(
        archivos.into_iter().map(EvalValue::Str).collect()));
    m.insert("errors".into(), EvalValue::List(errores));
    Ok(EvalValue::Dict(m))
}

/// Navega y espera la carga. Extraído de `goto` para poder reutilizarlo en el
/// recorrido sin pasar por la conversión de argumentos de Orion.
fn navegar(conn: &Conn, session: &str, url: &str, timeout: Duration) -> Result<(), String> {
    let marca = conn.event_mark();
    let r = conn.call("Page.navigate", serde_json::json!({ "url": url }), Some(session), timeout)?;
    if let Some(err) = r.get("errorText").and_then(|e| e.as_str()) {
        return Err(err.to_string());
    }
    let _ = conn.wait_event("Page.loadEventFired", Some(session), marca, timeout)?;
    Ok(())
}

//    Captura

fn do_screenshot(args: &[EvalValue]) -> Result<EvalValue, String> {
    let p = arg_handle(args, 0, "browser.screenshot(pestaña, ruta)")?;
    let ruta = args.get(1).map(to_str)
        .ok_or("browser.screenshot(pestaña, ruta): falta la ruta")?;
    let (conn, session, timeout, _t) = page_ctx(p)?;

    let r = conn.call(
        "Page.captureScreenshot",
        serde_json::json!({ "format": "png", "captureBeyondViewport": false }),
        Some(&session), timeout,
    )?;
    let b64 = r.get("data").and_then(|d| d.as_str())
        .ok_or("browser.screenshot: el navegador no devolvió imagen")?;

    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let bytes = B64.decode(b64)
        .map_err(|e| format!("browser.screenshot: imagen ilegible ({e})"))?;
    std::fs::write(&ruta, &bytes)
        .map_err(|e| format!("browser.screenshot: no se pudo escribir '{ruta}': {e}"))?;

    Ok(EvalValue::Str(ruta))
}

//    pages(browser) → lista de handles

fn pages(args: &[EvalValue]) -> Result<EvalValue, String> {
    let b = arg_handle(args, 0, "browser.pages(navegador)")?;
    let reg = handles().lock().unwrap();
    match reg.get(&b) {
        Some(Handle::Browser(bs)) => Ok(EvalValue::List(
            bs.pages.iter().map(|p| EvalValue::Int(*p as i64)).collect()
        )),
        _ => Err(format!("browser.pages: el navegador {b} no existe")),
    }
}

//    free(handle) — cierra pestaña o navegador

fn free(args: &[EvalValue]) -> Result<EvalValue, String> {
    let h = match args.first() {
        Some(v) => match to_u64(v) { Ok(h) => h, Err(_) => return Ok(EvalValue::Bool(false)) },
        None => return Ok(EvalValue::Bool(false)),
    };

    let sacado = handles().lock().unwrap().remove(&h);
    match sacado {
        None => Ok(EvalValue::Bool(false)),

        Some(Handle::Page(p)) => {
            // Cerrar la pestaña es lo que libera la memoria del renderer, que
            // es la parte cara: dejarla abierta y olvidada es RAM que nadie
            // reclama hasta que muere el navegador.
            let mut reg = handles().lock().unwrap();
            if let Some(Handle::Browser(b)) = reg.get_mut(&p.browser) {
                b.pages.retain(|x| *x != h);
                let conn = Arc::clone(&b.conn);
                let timeout = b.timeout;
                drop(reg);
                let _ = conn.call(
                    "Target.closeTarget",
                    serde_json::json!({ "targetId": p.target_id }),
                    None, timeout,
                );
            }
            Ok(EvalValue::Bool(true))
        }

        Some(Handle::Browser(mut b)) => {
            // Las pestañas se van con él: sus handles quedarían apuntando a un
            // navegador muerto.
            {
                let mut reg = handles().lock().unwrap();
                for p in &b.pages { reg.remove(p); }
            }
            let _ = b.conn.call("Browser.close", serde_json::json!({}), None,
                                Duration::from_millis(b.tuning.close_ms));
            b.conn.close();
            // `Browser.close` es una petición amable; si no la atendió, se
            // termina el proceso a mano para no dejarlo huérfano.
            let _ = b.proc.kill();
            let _ = b.proc.wait();
            if b.temporal {
                remove_profile(&b.user_data, b.tuning.cleanup_tries);
            }
            Ok(EvalValue::Bool(true))
        }
    }
}

/// Borra el perfil temporal, reintentando.
///
/// `wait()` solo espera al proceso principal, pero Chrome deja hijos (renderer,
/// GPU, red) que tardan un instante en soltar los archivos del perfil. En
/// Windows eso hace fallar el borrado inmediato, y un intento único dejaba
/// varios MB por sesión tirados en el temporal.
fn remove_profile(dir: &std::path::Path, intentos: u32) {
    for intento in 0..intentos.max(1) as u64 {
        if !dir.exists() || std::fs::remove_dir_all(dir).is_ok() { return; }
        std::thread::sleep(Duration::from_millis(50 + intento * 25));
    }
    // Si tras un segundo largo sigue bloqueado, no se insiste: perder el
    // directorio temporal es mucho menos grave que colgar el programa del
    // usuario al cerrar un navegador.
}

//    info() — diagnóstico

/// Qué navegador se usaría y de dónde sale. Sin esto, "no me funciona" es
/// indepurable: no se sabe si falta el navegador, si se eligió otro, o si el
/// problema está en la página.
fn info() -> Result<EvalValue, String> {
    let mut m: IndexMap<String, EvalValue> = IndexMap::new();
    match launch::resolve_browser(None) {
        Ok(p) => {
            m.insert("found".into(), EvalValue::Bool(true));
            m.insert("path".into(), EvalValue::Str(p.display().to_string()));
        }
        Err(e) => {
            m.insert("found".into(), EvalValue::Bool(false));
            m.insert("error".into(), EvalValue::Str(e));
        }
    }
    m.insert("env".into(), EvalValue::Str(
        std::env::var("ORION_CHROME").unwrap_or_else(|_| String::new())
    ));
    let abiertos = handles().lock().unwrap();
    let en_uso: Vec<EvalValue> = abiertos.values()
        .filter_map(|h| match h {
            // Se informa del ejecutable REAL de cada navegador abierto, no del
            // que se resolvería ahora: pueden no coincidir si el programa pasó
            // una ruta distinta en `open`, y ahí es donde está el malentendido.
            Handle::Browser(b) => Some(EvalValue::Str(b.exe.clone())),
            Handle::Page(_) => None,
        })
        .collect();
    m.insert("open_browsers".into(), EvalValue::Int(en_uso.len() as i64));
    m.insert("in_use".into(), EvalValue::List(en_uso));
    m.insert("open_pages".into(), EvalValue::Int(
        abiertos.values().filter(|h| matches!(h, Handle::Page(_))).count() as i64
    ));
    Ok(EvalValue::Dict(m))
}

//    Conversión de argumentos

fn parse_opts(v: Option<&EvalValue>) -> Result<LaunchOpts, String> {
    let mut o = LaunchOpts::default();
    let Some(EvalValue::Dict(m)) = v else { return Ok(o) };

    if let Some(x) = m.get("chrome").or_else(|| m.get("browser")) {
        o.chrome = Some(to_str(x));
    }
    if let Some(x) = m.get("user_data") { o.user_data = Some(to_str(x)); }
    if let Some(x) = m.get("headless")  { o.headless = truthy(x); }
    if let Some(x) = m.get("images")    { o.images   = truthy(x); }
    if let Some(x) = m.get("gpu")       { o.gpu      = truthy(x); }
    if let Some(x) = m.get("width")     { o.width    = to_u64(x).unwrap_or(1280) as u32; }
    if let Some(x) = m.get("height")    { o.height   = to_u64(x).unwrap_or(800) as u32; }
    if let Some(x) = m.get("timeout") {
        let ms = to_u64(x).unwrap_or(30_000);
        o.timeout = Duration::from_millis(ms.max(1_000));
    }
    if let Some(EvalValue::List(xs)) = m.get("args") {
        o.extra = xs.iter().map(to_str).collect();
    }
    Ok(o)
}

fn arg_handle(args: &[EvalValue], i: usize, uso: &str) -> Result<u64, String> {
    let v = args.get(i).ok_or_else(|| format!("{uso}: falta el handle"))?;
    to_u64(v).map_err(|_| format!("{uso}: esperaba un handle, recibió {}", v.type_name()))
}

fn to_u64(v: &EvalValue) -> Result<u64, String> {
    match v {
        EvalValue::Int(n) if *n > 0 => Ok(*n as u64),
        other => Err(format!("esperaba un handle positivo, recibió {}", other.type_name())),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{other}") }
}

fn to_f64(v: &EvalValue) -> Option<f64> {
    match v {
        EvalValue::Int(n) => Some(*n as f64),
        EvalValue::Float(f) => Some(*f),
        _ => None,
    }
}

fn truthy(v: &EvalValue) -> bool {
    match v {
        EvalValue::Bool(b) => *b,
        EvalValue::Int(n)  => *n != 0,
        EvalValue::Str(s)  => matches!(s.to_lowercase().as_str(), "yes" | "true" | "si" | "sí" | "1"),
        _ => false,
    }
}

fn json_to_eval(v: serde_json::Value) -> EvalValue {
    match v {
        serde_json::Value::Null => EvalValue::Null,
        serde_json::Value::Bool(b) => EvalValue::Bool(b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => EvalValue::Int(i),
            None => EvalValue::Float(n.as_f64().unwrap_or(0.0)),
        },
        serde_json::Value::String(s) => EvalValue::Str(s),
        serde_json::Value::Array(a) => EvalValue::List(a.into_iter().map(json_to_eval).collect()),
        serde_json::Value::Object(o) => {
            let mut m: IndexMap<String, EvalValue> = IndexMap::new();
            for (k, val) in o { m.insert(k, json_to_eval(val)); }
            EvalValue::Dict(m)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_opciones_por_defecto_ahorran_memoria() {
        let o = parse_opts(None).unwrap();
        assert!(o.headless, "por defecto debería ser headless");
        assert!(!o.images, "las imágenes deberían venir desactivadas");
        assert!(!o.gpu);
    }

    #[test]
    fn las_opciones_se_leen_del_dict() {
        let mut m: IndexMap<String, EvalValue> = IndexMap::new();
        m.insert("headless".into(), EvalValue::Bool(false));
        m.insert("images".into(),   EvalValue::Bool(true));
        m.insert("width".into(),    EvalValue::Int(1920));
        m.insert("timeout".into(),  EvalValue::Int(5_000));
        let o = parse_opts(Some(&EvalValue::Dict(m))).unwrap();
        assert!(!o.headless);
        assert!(o.images);
        assert_eq!(o.width, 1920);
        assert_eq!(o.timeout, Duration::from_millis(5_000));
    }

    #[test]
    fn yes_del_lenguaje_se_entiende_como_cierto() {
        // En Orion los booleanos se escriben yes/no; si además llegan como
        // texto desde un config, deben significar lo mismo.
        assert!(truthy(&EvalValue::Str("yes".into())));
        assert!(truthy(&EvalValue::Bool(true)));
        assert!(!truthy(&EvalValue::Str("no".into())));
    }

    #[test]
    fn un_timeout_ridiculo_se_eleva_a_un_minimo_util() {
        let mut m: IndexMap<String, EvalValue> = IndexMap::new();
        m.insert("timeout".into(), EvalValue::Int(5));
        let o = parse_opts(Some(&EvalValue::Dict(m))).unwrap();
        assert!(o.timeout >= Duration::from_secs(1), "un timeout de 5 ms no deja arrancar nada");
    }

    #[test]
    fn free_de_un_handle_inexistente_no_revienta() {
        // `with` llama a free en el camino de error: si ahí explotara, taparía
        // el error original del usuario.
        for entrada in [vec![EvalValue::Int(999_999)], vec![EvalValue::Null], vec![]] {
            assert!(matches!(free(&entrada), Ok(EvalValue::Bool(false))),
                    "free debería devolver no, sin fallar");
        }
    }

    #[test]
    fn un_handle_desconocido_da_un_error_legible() {
        let e = page_ctx(424_242).err().expect("debería fallar");
        assert!(e.contains("no existe"), "{e}");
    }

    #[test]
    fn info_responde_aunque_no_haya_navegador() {
        let EvalValue::Dict(m) = info().unwrap() else { panic!("info debería devolver un dict") };
        assert!(m.contains_key("found"));
        assert!(m.contains_key("open_browsers"));
    }

    #[test]
    fn json_se_convierte_a_valores_de_orion() {
        let v = serde_json::json!({ "a": 1, "b": [true, null, "x"], "c": 1.5 });
        let EvalValue::Dict(m) = json_to_eval(v) else { panic!("esperaba dict") };
        assert!(matches!(m["a"], EvalValue::Int(1)));
        assert!(matches!(m["c"], EvalValue::Float(f) if (f - 1.5).abs() < 1e-9));
        let EvalValue::List(l) = &m["b"] else { panic!("esperaba lista") };
        assert!(matches!(l[0], EvalValue::Bool(true)));
        assert!(matches!(l[1], EvalValue::Null));
        assert!(matches!(&l[2], EvalValue::Str(s) if s == "x"));
    }
}
