//! Ratón y teclado.
//!
//! Los eventos se despachan por el dominio `Input` de CDP, que los inyecta en la
//! misma capa por la que entran los del usuario. No son `element.click()` ni
//! eventos sintetizados desde JavaScript, que muchos sitios ignoran o detectan.
//!
//! La posición se vuelve a medir en `dom::box_for_click` inmediatamente antes de
//! cada despacho, no al empezar una cadena de acciones. Ahí está la diferencia
//! práctica con `ActionChains`: entre localizar y clicar, una página moderna ha
//! movido el elemento o le ha puesto un banner encima.

use std::time::Duration;

use super::cdp::Conn;
use super::dom;
pub use super::dom::Force;

/// Teclas con nombre. La lista es corta a propósito: cubre lo que se usa de
/// verdad en formularios y navegación, y cualquier otra cosa se escribe como
/// texto normal.
fn tecla(nombre: &str) -> Option<(&'static str, i64)> {
    Some(match nombre.to_lowercase().as_str() {
        "enter" | "return" => ("Enter", 13),
        "tab"              => ("Tab", 9),
        "escape" | "esc"   => ("Escape", 27),
        "backspace"        => ("Backspace", 8),
        "delete" | "del"   => ("Delete", 46),
        "space"            => (" ", 32),
        "up"               => ("ArrowUp", 38),
        "down"             => ("ArrowDown", 40),
        "left"             => ("ArrowLeft", 37),
        "right"            => ("ArrowRight", 39),
        "home"             => ("Home", 36),
        "end"              => ("End", 35),
        "pageup"           => ("PageUp", 33),
        "pagedown"         => ("PageDown", 34),
        _ => return None,
    })
}

/// Nombres de tecla admitidos, para poder decirlo en el mensaje de error en vez
/// de dejar al usuario adivinando.
pub const TECLAS: &str =
    "enter, tab, escape, backspace, delete, space, up, down, left, right, home, end, pageup, pagedown";

fn mouse(
    conn: &Conn, session: &str, tipo: &str, x: f64, y: f64,
    boton: &str, clicks: i64, timeout: Duration,
) -> Result<(), String> {
    conn.call(
        "Input.dispatchMouseEvent",
        serde_json::json!({
            "type": tipo, "x": x, "y": y,
            "button": boton, "buttons": if boton == "none" { 0 } else { 1 },
            "clickCount": clicks,
        }),
        Some(session), timeout,
    )?;
    Ok(())
}

/// Clic sobre un selector. `veces` es 1 o 2 (doble clic).
pub fn click(
    conn: &Conn, session: &str, sel: &str,
    boton: &str, veces: i64, espera_ms: u64, force: Force, timeout: Duration,
) -> Result<(), String> {
    // La espera está dentro de `box_for_click`: espera a que el elemento sea
    // accionable, no solo a que exista. Un scraper que exige acordarse de poner
    // un `wait` es un scraper que falla de forma intermitente.
    let b = dom::box_for_click(conn, session, sel, espera_ms, force)
        .map_err(|e| format!("browser.click {e}"))?;

    // Un movimiento previo dispara los `hover` de los que dependen muchos menús
    // desplegables; sin él, el clic cae sobre un elemento que aún no existe.
    let r = (|| {
        mouse(conn, session, "mouseMoved", b.x, b.y, "none", 0, timeout)?;
        for n in 1..=veces {
            mouse(conn, session, "mousePressed",  b.x, b.y, boton, n, timeout)?;
            mouse(conn, session, "mouseReleased", b.x, b.y, boton, n, timeout)?;
        }
        Ok(())
    })();

    // Se restaura pase lo que pase: dejar media página sorda al ratón rompería
    // todo lo que viniera después, incluido el propio diagnóstico del fallo.
    if b.forced { dom::restore_pointer_events(conn, session, timeout); }
    r
}

/// Deja el puntero sobre un elemento.
pub fn hover(
    conn: &Conn, session: &str, sel: &str, espera_ms: u64, timeout: Duration,
) -> Result<(), String> {
    let b = dom::box_for_click(conn, session, sel, espera_ms, Force::No)
        .map_err(|e| format!("browser.hover {e}"))?;
    mouse(conn, session, "mouseMoved", b.x, b.y, "none", 0, timeout)
}

/// Arrastra de un elemento a otro con eventos reales de ratón.
pub fn drag(
    conn: &Conn, session: &str, desde: &str, hasta: &str, espera_ms: u64, timeout: Duration,
) -> Result<(), String> {
    let a = dom::box_for_click(conn, session, desde, espera_ms, Force::No)
        .map_err(|e| format!("browser.drag origen {e}"))?;
    let z = dom::box_for_click(conn, session, hasta, espera_ms, Force::No)
        .map_err(|e| format!("browser.drag destino {e}"))?;

    mouse(conn, session, "mouseMoved",   a.x, a.y, "none", 0, timeout)?;
    mouse(conn, session, "mousePressed", a.x, a.y, "left", 1, timeout)?;
    // Pasos intermedios: un salto directo no dispara los `dragover` en los que
    // se apoyan las librerías de arrastrar y soltar.
    for i in 1..=10 {
        let t = i as f64 / 10.0;
        mouse(conn, session, "mouseMoved",
              a.x + (z.x - a.x) * t, a.y + (z.y - a.y) * t, "left", 1, timeout)?;
    }
    mouse(conn, session, "mouseReleased", z.x, z.y, "left", 1, timeout)
}

/// Rueda del ratón sobre la página.
pub fn scroll(
    conn: &Conn, session: &str, dx: f64, dy: f64, timeout: Duration,
) -> Result<(), String> {
    conn.call(
        "Input.dispatchMouseEvent",
        serde_json::json!({
            "type": "mouseWheel", "x": 10, "y": 10,
            "deltaX": dx, "deltaY": dy,
        }),
        Some(session), timeout,
    )?;
    Ok(())
}

/// Escribe texto en un campo.
///
/// Se manda tecla a tecla en vez de asignar `value` desde JS: React, Vue y
/// compañía solo se enteran del cambio si llegan los eventos de teclado, y un
/// `value` puesto a mano queda ignorado al enviar el formulario.
pub fn type_text(
    conn: &Conn, session: &str, sel: &str, texto: &str,
    limpiar: bool, espera_ms: u64, force: Force, timeout: Duration,
) -> Result<(), String> {
    // Enfocar con un clic real: hay campos que solo se activan al recibirlo.
    // El clic ya espera a que el campo sea accionable.
    click(conn, session, sel, "left", 1, espera_ms, force, timeout)
        .map_err(|e| e.replace("browser.click", "browser.type"))?;

    if limpiar {
        let cuerpo = r#"
        const el = __find(sel);
        if (el) { el.focus(); if ('value' in el) el.select(); }
        return true;
        "#;
        conn.call(
            "Runtime.evaluate",
            serde_json::json!({ "expression": dom::expr(sel, cuerpo), "returnByValue": true }),
            Some(session), timeout,
        )?;
        press(conn, session, "delete", timeout)?;
    }

    for ch in texto.chars() {
        let s = ch.to_string();
        conn.call("Input.dispatchKeyEvent", serde_json::json!({
            "type": "keyDown", "text": s, "key": s, "unmodifiedText": s,
        }), Some(session), timeout)?;
        conn.call("Input.dispatchKeyEvent", serde_json::json!({
            "type": "keyUp", "key": s,
        }), Some(session), timeout)?;
    }
    Ok(())
}

/// Pulsa una tecla con nombre sobre lo que tenga el foco.
pub fn press(conn: &Conn, session: &str, nombre: &str, timeout: Duration) -> Result<(), String> {
    let (key, code) = tecla(nombre)
        .ok_or_else(|| format!("browser.press: tecla '{nombre}' desconocida.\n  Admitidas: {TECLAS}"))?;

    let mut abajo = serde_json::json!({
        "type": "rawKeyDown", "key": key,
        "windowsVirtualKeyCode": code, "nativeVirtualKeyCode": code,
    });
    // Enter y Tab además producen texto; sin él, muchos formularios no
    // reaccionan al Enter.
    if key == "Enter" {
        abajo["type"] = serde_json::Value::String("keyDown".into());
        abajo["text"] = serde_json::Value::String("\r".into());
    }
    conn.call("Input.dispatchKeyEvent", abajo, Some(session), timeout)?;
    conn.call("Input.dispatchKeyEvent", serde_json::json!({
        "type": "keyUp", "key": key,
        "windowsVirtualKeyCode": code, "nativeVirtualKeyCode": code,
    }), Some(session), timeout)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_teclas_comunes_estan() {
        for t in ["enter", "Enter", "ESC", "tab", "up", "pagedown"] {
            assert!(tecla(t).is_some(), "falta la tecla {t}");
        }
        assert_eq!(tecla("enter").unwrap(), ("Enter", 13));
        assert_eq!(tecla("esc").unwrap(), ("Escape", 27));
    }

    #[test]
    fn una_tecla_inventada_no_existe() {
        assert!(tecla("teletransporte").is_none());
    }

    #[test]
    fn el_listado_de_teclas_coincide_con_las_implementadas() {
        // Si se añade una tecla y se olvida el listado, el mensaje de error
        // mentiría; esto lo impide.
        for nombre in TECLAS.split(',').map(str::trim) {
            assert!(tecla(nombre).is_some(),
                    "'{nombre}' aparece en el listado de ayuda pero no está implementada");
        }
    }
}
