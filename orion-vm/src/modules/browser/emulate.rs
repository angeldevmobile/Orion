//! Emulación: decirle al navegador qué dispositivo, idioma y zona horaria es.
//!
//! Sin esto hay sitios que sencillamente no se pueden automatizar:
//!
//! - **Los que sirven otro HTML al móvil.** El menú que hay que pulsar no
//!   existe en la versión de escritorio, así que el selector correcto "no
//!   aparece" y no hay forma de que aparezca.
//! - **Los que dependen de la zona horaria.** Un panel que muestra "hoy"
//!   cambia de datos según dónde crea el navegador que está. Reproducir el
//!   error de un compañero en Madrid desde una máquina en UTC es imposible sin
//!   fijarla.
//! - **Los que cambian con el idioma.** `text=Comprar` contra un sitio que
//!   decidió servir inglés por el `Accept-Language` del contenedor de CI.
//! - **Los que piden ubicación.** El diálogo del navegador bloquea el flujo, y
//!   una posición fija lo resuelve sin que llegue a aparecer.
//!
//! ```orion
//! web.emulate(p, { device: "iphone" })
//! web.emulate(p, { width: 1920, height: 1080, locale: "es-ES", timezone: "Europe/Madrid" })
//! web.emulate(p, { dark: yes, geo: { lat: 40.4168, lon: -3.7038 } })
//! web.emulate(p, no)                 -- quita todo y vuelve a lo de open()
//! ```
//!
//! Los presets son un atajo, no una lista cerrada: cualquiera de sus campos se
//! puede sobrescribir en la misma llamada, y sin preset se configura a mano.
//! Las medidas salen de los dispositivos reales, y están aquí y no incrustadas
//! en el código de la llamada para poder mirarlas y corregirlas.

/// Un dispositivo de referencia: lo justo para que el sitio sirva su versión.
#[derive(Debug, Clone, Copy)]
pub struct Device {
    pub nombre: &'static str,
    pub width:  u32,
    pub height: u32,
    pub scale:  f64,
    pub movil:  bool,
    pub tactil: bool,
    pub ua:     &'static str,
}

const UA_IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) \
AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
const UA_IPAD: &str = "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) \
AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
const UA_ANDROID: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 7) \
AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Mobile Safari/537.36";

pub const DEVICES: &[Device] = &[
    Device { nombre: "iphone",  width: 390,  height: 844,  scale: 3.0, movil: true,  tactil: true,  ua: UA_IPHONE },
    Device { nombre: "iphone-se", width: 375, height: 667, scale: 2.0, movil: true,  tactil: true,  ua: UA_IPHONE },
    Device { nombre: "ipad",    width: 820,  height: 1180, scale: 2.0, movil: true,  tactil: true,  ua: UA_IPAD },
    Device { nombre: "android", width: 412,  height: 915,  scale: 2.625, movil: true, tactil: true, ua: UA_ANDROID },
    Device { nombre: "laptop",  width: 1366, height: 768,  scale: 1.0, movil: false, tactil: false, ua: "" },
    Device { nombre: "desktop", width: 1920, height: 1080, scale: 1.0, movil: false, tactil: false, ua: "" },
];

pub fn device(nombre: &str) -> Result<&'static Device, String> {
    let limpio = nombre.trim().to_ascii_lowercase().replace([' ', '_'], "-");
    DEVICES.iter().find(|d| d.nombre == limpio).ok_or_else(|| {
        let ns: Vec<&str> = DEVICES.iter().map(|d| d.nombre).collect();
        format!(
            "browser.emulate: unknown device '{nombre}'.\n  Accepted: {}\n  \
             Or set width/height/mobile/ua by hand.",
            ns.join(", ")
        )
    })
}

/// Lo que se va a emular, ya resuelto: preset + lo que el usuario sobrescriba.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub width:    Option<u32>,
    pub height:   Option<u32>,
    pub scale:    Option<f64>,
    pub movil:    Option<bool>,
    pub tactil:   Option<bool>,
    pub ua:       Option<String>,
    pub locale:   Option<String>,
    pub timezone: Option<String>,
    pub geo:      Option<(f64, f64, f64)>,
    /// `Some(true)` fuerza modo oscuro, `Some(false)` claro, `None` no toca.
    pub dark:     Option<bool>,
}

impl Plan {
    pub fn desde_device(d: &Device) -> Plan {
        Plan {
            width:  Some(d.width),
            height: Some(d.height),
            scale:  Some(d.scale),
            movil:  Some(d.movil),
            tactil: Some(d.tactil),
            ua:     if d.ua.is_empty() { None } else { Some(d.ua.to_string()) },
            ..Plan::default()
        }
    }

    pub fn vacio(&self) -> bool {
        *self == Plan::default()
    }

    pub fn mensajes(&self) -> Vec<(&'static str, serde_json::Value)> {
        let mut out = Vec::new();

        if self.width.is_some() || self.height.is_some() || self.movil.is_some() {
            out.push(("Emulation.setDeviceMetricsOverride", serde_json::json!({
                "width":  self.width.unwrap_or(0),
                "height": self.height.unwrap_or(0),
                "deviceScaleFactor": self.scale.unwrap_or(0.0),
                "mobile": self.movil.unwrap_or(false),
            })));
        }
        if let Some(t) = self.tactil {
            out.push(("Emulation.setTouchEmulationEnabled", serde_json::json!({
                "enabled": t,
                "maxTouchPoints": if t { 5 } else { 0 },
            })));
        }
        if self.ua.is_some() || self.locale.is_some() {
            let mut p = serde_json::json!({ "userAgent": self.ua.clone().unwrap_or_default() });
            if let Some(l) = &self.locale {
                // `acceptLanguage` es lo que viaja en la cabecera; sin él el
                // sitio sigue sirviendo el idioma del contenedor.
                p["acceptLanguage"] = serde_json::Value::String(l.clone());
            }
            out.push(("Network.setUserAgentOverride", p));
        }
        if let Some(l) = &self.locale {
            out.push(("Emulation.setLocaleOverride", serde_json::json!({ "locale": l })));
        }
        if let Some(z) = &self.timezone {
            out.push(("Emulation.setTimezoneOverride", serde_json::json!({ "timezoneId": z })));
        }
        if let Some((lat, lon, acc)) = self.geo {
            out.push(("Emulation.setGeolocationOverride", serde_json::json!({
                "latitude": lat, "longitude": lon, "accuracy": acc,
            })));
        }
        if let Some(oscuro) = self.dark {
            out.push(("Emulation.setEmulatedMedia", serde_json::json!({
                "features": [{
                    "name": "prefers-color-scheme",
                    "value": if oscuro { "dark" } else { "light" },
                }],
            })));
        }
        out
    }
}

/// Permisos que el navegador pregunta con un diálogo, con el nombre corto que
/// se escribe en Orion.
///
/// El diálogo de permisos es un bloqueo de los de verdad: aparece encima de la
/// página, no se puede clicar desde JavaScript y deja la automatización parada
/// sin decir por qué. Concederlo por adelantado hace que no llegue a existir.
pub const PERMISOS: &[(&str, &str)] = &[
    ("geolocation",  "geolocation"),
    ("notifications","notifications"),
    ("camera",       "videoCapture"),
    ("microphone",   "audioCapture"),
    ("clipboard",    "clipboardReadWrite"),
    ("clipboardread","clipboardReadWrite"),
    ("midi",         "midi"),
    ("sensors",      "sensors"),
    ("background",   "backgroundSync"),
];

pub fn permiso(nombre: &str) -> Result<&'static str, String> {
    let limpio: String = nombre.chars().filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase()).collect();
    PERMISOS.iter().find(|(k, _)| *k == limpio).map(|(_, v)| *v).ok_or_else(|| {
        let ns: Vec<&str> = PERMISOS.iter().map(|(k, _)| *k).collect();
        format!(
            "browser.emulate: unknown permission '{nombre}'.\n  Accepted: {}",
            ns.join(", ")
        )
    })
}

/// Los mensajes que deshacen cualquier emulación.
pub fn limpiar() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        ("Emulation.clearDeviceMetricsOverride", serde_json::json!({})),
        ("Emulation.setTouchEmulationEnabled",   serde_json::json!({ "enabled": false })),
        ("Network.setUserAgentOverride",         serde_json::json!({ "userAgent": "" })),
        ("Emulation.clearGeolocationOverride",   serde_json::json!({})),
        ("Emulation.setEmulatedMedia",           serde_json::json!({ "features": [] })),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_preset_trae_medidas_de_movil_y_su_ua() {
        let d = device("iphone").unwrap();
        assert!(d.movil && d.tactil, "un iPhone es móvil y táctil");
        assert!(d.ua.contains("iPhone"), "el UA no dice que sea un iPhone");
        assert!(d.width < d.height, "un móvil está de pie");
    }

    #[test]
    fn el_nombre_del_preset_se_escribe_como_uno_quiera() {
        assert_eq!(device("iPhone SE").unwrap().nombre, "iphone-se");
        assert_eq!(device("iphone_se").unwrap().nombre, "iphone-se");
    }

    #[test]
    fn un_dispositivo_inventado_lista_los_que_hay() {
        let e = device("nokia3310").unwrap_err();
        assert!(e.contains("iphone") && e.contains("desktop"), "no listó los válidos: {e}");
        assert!(e.contains("width"), "no dice que se puede a mano: {e}");
    }

    #[test]
    fn lo_que_no_se_pide_no_se_toca() {
        let p = Plan { timezone: Some("Europe/Madrid".into()), ..Plan::default() };
        let ms: Vec<&str> = p.mensajes().iter().map(|(m, _)| *m).collect();
        assert_eq!(ms, vec!["Emulation.setTimezoneOverride"],
                   "cambiar la zona horaria no debe redimensionar la ventana");
    }

    #[test]
    fn el_idioma_viaja_tambien_en_la_cabecera() {
        // Solo con setLocaleOverride, el sitio sigue mirando Accept-Language y
        // sirve el idioma del contenedor: el síntoma es "en mi máquina sale en
        // español y en CI en inglés".
        let p = Plan { locale: Some("es-ES".into()), ..Plan::default() };
        let ms = p.mensajes();
        let ua = ms.iter().find(|(m, _)| *m == "Network.setUserAgentOverride")
                  .expect("falta la cabecera de idioma");
        assert_eq!(ua.1["acceptLanguage"], "es-ES");
        assert!(ms.iter().any(|(m, _)| *m == "Emulation.setLocaleOverride"));
    }

    #[test]
    fn el_preset_se_puede_sobrescribir() {
        let mut p = Plan::desde_device(device("iphone").unwrap());
        p.width = Some(1000);
        let ms = p.mensajes();
        let m = ms.iter().find(|(m, _)| *m == "Emulation.setDeviceMetricsOverride").unwrap();
        assert_eq!(m.1["width"], 1000);
        assert_eq!(m.1["mobile"], true, "seguía siendo un móvil, solo cambió el ancho");
    }

    #[test]
    fn limpiar_deshace_lo_que_se_puso() {
        let ms: Vec<&str> = limpiar().iter().map(|(m, _)| *m).collect();
        assert!(ms.contains(&"Emulation.clearDeviceMetricsOverride"));
        assert!(ms.contains(&"Emulation.clearGeolocationOverride"));
    }
}
