//! Captura de red: leer el JSON que la página pide a su API, ya tipado, en vez
//! de deshacer el HTML. Se arma antes (`watch`) y se lee después (`capture`),
//! porque la escucha tiene que estar puesta antes de provocar la petición.

/// ¿Casa la URL con el patrón? Sin `*` es "contiene"; con `*`, comodín. Sin
/// regex: `?`, `.` y `+` son normales en una URL.
pub fn casa(url: &str, patron: &str) -> bool {
    let p = patron.trim();
    if p.is_empty() { return true; }
    if !p.contains('*') { return url.contains(p); }

    let partes: Vec<&str> = p.split('*').collect();
    let mut resto = url;
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
