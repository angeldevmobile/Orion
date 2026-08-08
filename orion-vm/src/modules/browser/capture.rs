//! Captura de red: leer lo que la página le pide a su propia API.
//!
//! Casi todo sitio moderno pinta sus listados con JavaScript a partir de un
//! JSON que él mismo se descarga. Un scraper clásico espera a que ese JSON se
//! convierta en HTML y luego deshace el trabajo: busca `div`s, quita etiquetas,
//! reconstruye números que ya venían siendo números. Y se rompe el día que el
//! sitio cambia una clase de CSS.
//!
//! Aquí se lee la fuente. Los datos llegan **ya tipados**, sin nombres de clase
//! de por medio, y suelen traer campos que la página no llega a mostrar.
//!
//! ```orion
//! web.watch(p, "/api/productos")     -- arma la escucha
//! web.click(p, "#cargar")            -- lo que provoque la petición
//! r = web.capture(p)                 -- devuelve lo que pidió, ya parseado
//! ```
//!
//! Dos llamadas y no una porque hay que armar **antes** de provocar: si se
//! encendiera la escucha después del clic, la petición ya habría pasado. Es la
//! misma razón por la que `click_opens` toma la marca de eventos antes de
//! pulsar.
//!
//! Playwright tiene `page.on("response")`, que es un callback donde hay que
//! filtrar a mano, pedir el cuerpo con otro `await` y acordarse de que el
//! cuerpo puede no estar ya. Selenium no tiene nada equivalente sin un proxy
//! delante.

/// ¿Casa la URL con el patrón?
///
/// Sin `*` es "contiene", que es lo que casi siempre se quiere y lo que
/// cualquiera escribe primero: `web.watch(p, "/api/")`. Con `*` es un comodín
/// que cubre cualquier trozo, para cuando hace falta afinar
/// (`"*/v2/pedidos?*"`).
///
/// No se usa una expresión regular a propósito: una URL lleva `?`, `.` y `+`,
/// que en regex significan otra cosa, y el patrón obvio daría resultados
/// sorprendentes.
pub fn casa(url: &str, patron: &str) -> bool {
    let p = patron.trim();
    if p.is_empty() { return true; }
    if !p.contains('*') { return url.contains(p); }

    let partes: Vec<&str> = p.split('*').collect();
    let mut resto = url;

    // El primer trozo tiene que estar pegado al principio si el patrón no
    // empieza por comodín; el último, al final por el mismo motivo.
    for (i, trozo) in partes.iter().enumerate() {
        if trozo.is_empty() { continue; }
        if i == 0 {
            if !resto.starts_with(trozo) { return false; }
            resto = &resto[trozo.len()..];
            continue;
        }
        if i == partes.len() - 1 && !p.ends_with('*') {
            return resto.ends_with(trozo);
        }
        match resto.find(trozo) {
            Some(j) => resto = &resto[j + trozo.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_comodin_es_contiene() {
        assert!(casa("https://x.com/api/productos?p=2", "/api/"));
        assert!(casa("https://x.com/api/productos", "productos"));
        assert!(!casa("https://x.com/pagina.html", "/api/"));
    }

    #[test]
    fn el_comodin_cubre_cualquier_trozo() {
        assert!(casa("https://x.com/v2/pedidos?page=1", "*/v2/pedidos?*"));
        assert!(casa("https://x.com/a/b/c.json", "*.json"));
        assert!(!casa("https://x.com/a/b/c.html", "*.json"));
    }

    #[test]
    fn el_principio_y_el_final_se_anclan() {
        // Sin comodín delante, el patrón empieza donde empieza la URL.
        assert!(casa("https://x.com/api", "https://x.com/*"));
        assert!(!casa("http://otro.com/x", "https://x.com/*"));
        // Sin comodín detrás, tiene que terminar ahí.
        assert!(!casa("https://x.com/datos.json?v=2", "*.json"));
    }

    #[test]
    fn un_patron_vacio_lo_coge_todo() {
        assert!(casa("https://loquesea", ""));
    }

    #[test]
    fn los_signos_de_una_url_no_son_comodines() {
        // En una regex, `?` y `.` significan otra cosa; aquí son literales.
        assert!(!casa("https://x.com/apiZproductos", "/api?productos"));
        assert!(casa("https://x.com/api?productos", "/api?productos"));
    }
}
