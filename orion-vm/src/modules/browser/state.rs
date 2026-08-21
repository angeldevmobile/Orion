//! Sesión reutilizable: cookies y almacenamiento del navegador, a un archivo.
//!
//! El problema que resuelve es el más caro de una automatización que corre a
//! diario: **volver a iniciar sesión en cada ejecución**. Es lento, y sobre todo
//! es frágil — cada login es un formulario que puede cambiar, un captcha que
//! puede aparecer y un doble factor que puede saltar. Un proceso que se loguea
//! cien veces al día también es un proceso que parece un ataque.
//!
//! ```orion
//! -- una vez, a mano
//! web.save_state(p, "sesion.json")
//!
//! -- todos los días
//! web.goto(p, "https://portal.empresa.com")
//! web.load_state(p, "sesion.json")
//! web.reload(p)                       -- ya dentro
//! ```
//!
//! `user_data` en `open()` resuelve algo parecido guardando el perfil entero,
//! pero es una carpeta de cientos de megas atada a una máquina. Esto es un JSON
//! que se puede mover, versionar aparte o guardar en un gestor de secretos.
//!
//! ## Este archivo es una credencial
//!
//! Dentro van las cookies de sesión. Quien lo tenga entra como tú, sin
//! contraseña y sin segundo factor. No va al repositorio y no se comparte: vale
//! exactamente lo mismo que la contraseña, con el agravante de que no caduca
//! cuando la cambias.

use std::time::Duration;

use super::cdp::Conn;

fn origen_js() -> &'static str {
    r#"(() => {
      const vuelca = (s) => {
        const o = {};
        try { for (let i = 0; i < s.length; i++) { const k = s.key(i); o[k] = s.getItem(k); } }
        catch (e) { /* un origen opaco (sandbox, file://) lanza al tocarlo */ }
        return o;
      };
      return {
        origin:  location.origin,
        local:   vuelca(window.localStorage),
        session: vuelca(window.sessionStorage)
      };
    })()"#
}

pub struct Guardado {
    pub cookies: usize,
    pub local:   usize,
    pub session: usize,
    pub origin:  String,
}

/// Vuelca cookies y almacenamiento a un JSON.
pub fn save(
    conn: &Conn, sesion_cdp: &str, ruta: &str, timeout: Duration,
) -> Result<Guardado, String> {
    let c = conn.call("Storage.getCookies", serde_json::json!({}), None, timeout)
        .or_else(|_| conn.call("Network.getCookies", serde_json::json!({}), Some(sesion_cdp), timeout))
        .map_err(|e| format!("browser.save_state: no se pudieron leer las cookies: {e}"))?;
    let cookies = c.get("cookies").cloned().unwrap_or(serde_json::Value::Array(vec![]));

    let s = conn.call(
        "Runtime.evaluate",
        serde_json::json!({ "expression": origen_js(), "returnByValue": true }),
        Some(sesion_cdp), timeout,
    )?;
    let almacen = s.get("result").and_then(|x| x.get("value")).cloned()
        .unwrap_or(serde_json::Value::Null);

    let origin = almacen.get("origin").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let n_local = almacen.get("local").and_then(|x| x.as_object()).map(|m| m.len()).unwrap_or(0);
    let n_ses   = almacen.get("session").and_then(|x| x.as_object()).map(|m| m.len()).unwrap_or(0);

    let doc = serde_json::json!({
        "version": 1,
        "cookies": cookies,
        "origins": if origin.is_empty() || origin == "null" {
            serde_json::Value::Array(vec![])
        } else {
            serde_json::json!([almacen])
        },
    });

    let texto = serde_json::to_string_pretty(&doc)
        .map_err(|e| format!("browser.save_state: no se pudo serializar: {e}"))?;
    std::fs::write(ruta, texto)
        .map_err(|e| format!("browser.save_state: no se pudo escribir '{ruta}': {e}"))?;

    Ok(Guardado {
        cookies: cookies.as_array().map(|a| a.len()).unwrap_or(0),
        local:   n_local,
        session: n_ses,
        origin,
    })
}

fn para_poner(c: &serde_json::Value) -> serde_json::Value {
    let mut o = serde_json::Map::new();
    for campo in ["name", "value", "domain", "path", "sameSite"] {
        if let Some(v) = c.get(campo) {
            if !v.is_null() { o.insert(campo.into(), v.clone()); }
        }
    }
    for campo in ["secure", "httpOnly"] {
        if let Some(v) = c.get(campo).and_then(|x| x.as_bool()) {
            o.insert(campo.into(), serde_json::Value::Bool(v));
        }
    }
    // Solo se copia la caducidad si es una de verdad.
    if let Some(e) = c.get("expires").and_then(|x| x.as_f64()) {
        if e > 0.0 {
            o.insert("expires".into(), serde_json::Value::from(e));
        }
    }
    serde_json::Value::Object(o)
}

pub struct Cargado {
    pub cookies:  usize,
    pub local:    usize,
    pub session:  usize,
    /// Orígenes del archivo que NO se aplicaron por no ser el de la página.
    pub omitidos: Vec<String>,
}

pub fn load(
    conn: &Conn, sesion_cdp: &str, ruta: &str, timeout: Duration,
) -> Result<Cargado, String> {
    let texto = std::fs::read_to_string(ruta).map_err(|e| format!(
        "browser.load_state: no se pudo leer '{ruta}': {e}\n  \
         ¿Se guardó antes con save_state?"
    ))?;
    let doc: serde_json::Value = serde_json::from_str(&texto)
        .map_err(|e| format!("browser.load_state: '{ruta}' no es un estado válido: {e}"))?;

    let cookies = doc.get("cookies").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let listas: Vec<serde_json::Value> = cookies.iter().map(para_poner).collect();
    if !listas.is_empty() {
        conn.call("Network.setCookies", serde_json::json!({ "cookies": listas }),
                  Some(sesion_cdp), timeout)
            .map_err(|e| format!("browser.load_state: no se pudieron poner las cookies: {e}"))?;
    }

    let actual = conn.call(
        "Runtime.evaluate",
        serde_json::json!({ "expression": "location.origin", "returnByValue": true }),
        Some(sesion_cdp), timeout,
    )?;
    let origen_actual = actual.get("result").and_then(|x| x.get("value"))
        .and_then(|x| x.as_str()).unwrap_or("").to_string();

    let mut local = 0usize;
    let mut session = 0usize;
    let mut omitidos = Vec::new();

    for o in doc.get("origins").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let origen = o.get("origin").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if origen != origen_actual {
            omitidos.push(origen);
            continue;
        }
        let l = o.get("local").cloned().unwrap_or(serde_json::json!({}));
        let s = o.get("session").cloned().unwrap_or(serde_json::json!({}));
        local   += l.as_object().map(|m| m.len()).unwrap_or(0);
        session += s.as_object().map(|m| m.len()).unwrap_or(0);

        let js = format!(r#"(() => {{
          const pon = (almacen, datos) => {{
            try {{ for (const k of Object.keys(datos)) almacen.setItem(k, datos[k]); }}
            catch (e) {{}}
          }};
          pon(window.localStorage, {l});
          pon(window.sessionStorage, {s});
          return true;
        }})()"#);
        conn.call(
            "Runtime.evaluate",
            serde_json::json!({ "expression": js, "returnByValue": true }),
            Some(sesion_cdp), timeout,
        )?;
    }

    Ok(Cargado { cookies: cookies.len(), local, session, omitidos })
}

//    Lista blanca de dominios

pub fn permitida(url: &str, lista: &[String]) -> bool {
    if lista.is_empty() { return true; }
    let u = url.trim();
    if u.is_empty()
        || u.starts_with("about:")
        || u.starts_with("data:")
        || u.starts_with("blob:")
        || u.starts_with("chrome-extension:")
        || u.starts_with("devtools:")
    {
        return true;
    }

    let host = host_de(u);
    lista.iter().any(|p| coincide(&host, p))
}

/// Host de una URL, sin puerto ni credenciales.
fn host_de(url: &str) -> String {
    let sin_esquema = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => url,
    };
    let hasta = sin_esquema.find(['/', '?', '#']).unwrap_or(sin_esquema.len());
    let autoridad = &sin_esquema[..hasta];
    let sin_cred = match autoridad.rfind('@') {
        Some(i) => &autoridad[i + 1..],
        None => autoridad,
    };
    let sin_puerto = match sin_cred.rfind(':') {
        // Un `:` dentro de corchetes es una dirección IPv6, no un puerto.
        Some(i) if !sin_cred[i..].contains(']') => &sin_cred[..i],
        _ => sin_cred,
    };
    sin_puerto.to_lowercase()
}

/// `*.empresa.com` cubre los subdominios; `empresa.com` solo ese host.
fn coincide(host: &str, patron: &str) -> bool {
    let p = patron.trim().to_lowercase();
    if let Some(sufijo) = p.strip_prefix("*.") {
        return host == sufijo || host.ends_with(&format!(".{sufijo}"));
    }
    host == p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lista(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn una_cookie_de_sesion_no_se_restaura_ya_caducada() {
        let c = serde_json::json!({
            "name": "sid", "value": "abc", "domain": ".x.com", "path": "/",
            "expires": -1.0, "httpOnly": true, "secure": true,
            "session": true, "size": 32, "priority": "Medium"
        });
        let p = para_poner(&c);
        assert!(p.get("expires").is_none(), "no debe llevar caducidad: {p}");
        assert_eq!(p["name"], "sid");
        assert_eq!(p["httpOnly"], true);
        // Y no se cuelan campos que describen cómo está guardada, no cómo se crea.
        assert!(p.get("size").is_none());
        assert!(p.get("priority").is_none());
        assert!(p.get("session").is_none());
    }

    #[test]
    fn una_caducidad_de_verdad_se_conserva() {
        let c = serde_json::json!({ "name": "a", "value": "b", "expires": 1893456000.0 });
        let p = para_poner(&c);
        assert_eq!(p["expires"], 1893456000.0);
    }

    #[test]
    fn sin_lista_pasa_todo() {
        assert!(permitida("https://loquesea.net/x", &[]));
    }

    #[test]
    fn el_comodin_cubre_subdominios_y_el_dominio_pelado() {
        let l = lista(&["*.empresa.com"]);
        assert!(permitida("https://portal.empresa.com/login", &l));
        assert!(permitida("https://a.b.empresa.com/", &l));
        assert!(permitida("https://empresa.com/", &l));
        assert!(!permitida("https://otra.net/", &l));
        // Y no cuela un dominio que solo TERMINA parecido.
        assert!(!permitida("https://noesempresa.com/", &l));
        assert!(!permitida("https://empresa.com.malo.net/", &l));
    }

    #[test]
    fn sin_comodin_es_solo_ese_host() {
        let l = lista(&["empresa.com"]);
        assert!(permitida("https://empresa.com/x", &l));
        assert!(!permitida("https://portal.empresa.com/x", &l));
    }

    #[test]
    fn el_puerto_y_las_credenciales_no_confunden_al_host() {
        let l = lista(&["empresa.com"]);
        assert!(permitida("http://empresa.com:8080/x", &l));
        // El truco clásico: lo de antes de la arroba es usuario, no host.
        assert!(!permitida("http://empresa.com@malo.net/x", &l));
        assert!(!permitida("http://malo.net/?x=empresa.com", &l));
    }

    #[test]
    fn los_esquemas_internos_del_navegador_no_se_bloquean() {
        let l = lista(&["empresa.com"]);
        // Bloquear about:blank dejaría al navegador sin poder abrir una pestaña.
        assert!(permitida("about:blank", &l));
        assert!(permitida("data:text/html,x", &l));
    }
}
