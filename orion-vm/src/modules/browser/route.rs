//! Intercepción de peticiones: decidir qué hace el navegador con cada una.
//!
//! Hasta aquí el módulo solo sabía **mirar** la red (`watch`/`capture`) y
//! cortarla por dominio (`allow`). Eso deja fuera media automatización seria:
//!
//! - **Probar el camino de error.** Un carrito que falla cuando la API de
//!   stock devuelve 500 no se puede probar sin provocar ese 500. Con un mock
//!   se provoca en una línea; sin él hay que tocar el servidor de verdad.
//! - **Trabajar sin backend.** El front está listo y la API todavía no.
//! - **Ir rápido.** Un listado de 500 filas con imágenes, tipografías y tres
//!   trazadores tarda más en pintarse que en leerse. Bloquear lo que no se
//!   mira cambia el tiempo del trabajo entero, no el de una llamada.
//! - **Autenticarse donde no hay formulario.** Añadir una cabecera a las
//!   peticiones de la propia página evita tener que simular un login.
//!
//! ```orion
//! web.route(p, "*/api/stock*", { mock: { status: 500, json: { error: "caido" } } })
//! web.route(p, "*.png",        { block: yes })
//! web.route(p, "*/api/*",      { headers: { Authorization: "Bearer " + token } })
//! web.route(p, "*/lento*",     { fail: "timedout" })
//! ```
//!
//! Las reglas se prueban **en orden** y manda la primera que casa, como en un
//! cortafuegos: así una regla concreta puede ir delante de otra general sin
//! que el orden de evaluación sea un misterio.
//!
//! La lista blanca de `open({ allow })` se comprueba ANTES que las rutas y no
//! se puede levantar con una regla: es una medida de seguridad, y una regla de
//! conveniencia no debe poder abrir un dominio que se cerró a propósito.

use serde_json::Value;

use super::capture::casa;

/// Qué hacer con una petición que casa.
#[derive(Debug, Clone, PartialEq)]
pub enum Accion {
    /// Cortarla como si un bloqueador de anuncios la hubiera parado.
    Bloquear,
    /// Cortarla con un motivo de red concreto, para probar el camino de error.
    Fallar(String),
    /// Responder desde Orion sin que la petición llegue a salir.
    Simular(Simulada),
    /// Dejarla pasar con estas cabeceras añadidas o reescritas.
    Cabeceras(Vec<(String, String)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Simulada {
    pub status:  u32,
    pub headers: Vec<(String, String)>,
    /// Cuerpo ya en texto. Un `json:` se serializa al crear la regla, no en
    /// cada petición: el trabajo se hace una vez y no dentro del camino
    /// caliente de la red.
    pub body:    String,
}

#[derive(Debug, Clone)]
pub struct Ruta {
    pub patron: String,
    pub accion: Accion,
    /// Cuántas veces puede disparar. `None` = siempre. Sirve para simular algo
    /// que falla la primera vez y a la segunda funciona, que es justo el caso
    /// que hay que probar cuando se escribe un reintento.
    pub limite: Option<u64>,
    pub veces:  u64,
}

impl Ruta {
    pub fn agotada(&self) -> bool {
        matches!(self.limite, Some(n) if self.veces >= n)
    }
}

/// Motivos de fallo que entiende el navegador, con el nombre que se escribe en
/// Orion. Se aceptan en minúsculas porque nadie recuerda el camelCase de CDP.
pub const MOTIVOS: &[(&str, &str)] = &[
    ("failed",          "Failed"),
    ("aborted",         "Aborted"),
    ("timedout",        "TimedOut"),
    ("accessdenied",    "AccessDenied"),
    ("connectionclosed","ConnectionClosed"),
    ("connectionreset", "ConnectionReset"),
    ("connectionrefused","ConnectionRefused"),
    ("connectionaborted","ConnectionAborted"),
    ("connectionfailed","ConnectionFailed"),
    ("namenotresolved", "NameNotResolved"),
    ("internetdisconnected", "InternetDisconnected"),
    ("addressunreachable",   "AddressUnreachable"),
    ("blockedbyclient", "BlockedByClient"),
    ("blockedbyresponse",    "BlockedByResponse"),
];

/// Traduce el motivo que escribió el usuario al que espera CDP.
pub fn motivo(nombre: &str) -> Result<&'static str, String> {
    let limpio: String = nombre.chars().filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase()).collect();
    MOTIVOS.iter().find(|(k, _)| *k == limpio).map(|(_, v)| *v).ok_or_else(|| {
        let validos: Vec<&str> = MOTIVOS.iter().map(|(k, _)| *k).collect();
        format!(
            "browser.route: unknown fail reason '{nombre}'.\n  Accepted: {}",
            validos.join(", ")
        )
    })
}

/// La primera regla que casa con la url, si queda alguna sin agotar.
pub fn elegir<'a>(rutas: &'a mut [Ruta], url: &str) -> Option<&'a mut Ruta> {
    rutas.iter_mut().find(|r| !r.agotada() && casa(url, &r.patron))
}

/// El mensaje CDP que responde a una petición pausada.
///
/// Devolver el mensaje en vez de mandarlo aquí deja la decisión probada sin
/// necesidad de un navegador: la parte difícil es esta, no el envío.
pub fn respuesta_cdp(accion: &Accion, request_id: &str) -> (&'static str, Value) {
    match accion {
        Accion::Bloquear => (
            "Fetch.failRequest",
            serde_json::json!({ "requestId": request_id, "errorReason": "BlockedByClient" }),
        ),
        Accion::Fallar(razon) => (
            "Fetch.failRequest",
            serde_json::json!({ "requestId": request_id, "errorReason": razon }),
        ),
        Accion::Cabeceras(hs) => (
            "Fetch.continueRequest",
            serde_json::json!({
                "requestId": request_id,
                "headers": hs.iter()
                    .map(|(n, v)| serde_json::json!({ "name": n, "value": v }))
                    .collect::<Vec<_>>(),
            }),
        ),
        Accion::Simular(s) => {
            use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
            (
                "Fetch.fulfillRequest",
                serde_json::json!({
                    "requestId": request_id,
                    "responseCode": s.status,
                    "responseHeaders": s.headers.iter()
                        .map(|(n, v)| serde_json::json!({ "name": n, "value": v }))
                        .collect::<Vec<_>>(),
                    "body": B64.encode(s.body.as_bytes()),
                }),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ruta(patron: &str, accion: Accion) -> Ruta {
        Ruta { patron: patron.into(), accion, limite: None, veces: 0 }
    }

    #[test]
    fn manda_la_primera_regla_que_casa() {
        let mut rs = vec![
            ruta("*/api/stock*", Accion::Bloquear),
            ruta("/api/", Accion::Fallar("TimedOut".into())),
        ];
        let elegida = elegir(&mut rs, "https://x.com/api/stock?id=1").unwrap();
        assert_eq!(elegida.accion, Accion::Bloquear, "ganó la general en vez de la concreta");
    }

    #[test]
    fn una_regla_agotada_deja_pasar_a_la_siguiente() {
        let mut rs = vec![
            Ruta { patron: "/api/".into(), accion: Accion::Bloquear, limite: Some(1), veces: 1 },
            ruta("/api/", Accion::Fallar("TimedOut".into())),
        ];
        let e = elegir(&mut rs, "https://x.com/api/x").unwrap();
        assert_eq!(e.accion, Accion::Fallar("TimedOut".into()));
    }

    #[test]
    fn sin_regla_que_case_no_se_toca_la_peticion() {
        let mut rs = vec![ruta("*.png", Accion::Bloquear)];
        assert!(elegir(&mut rs, "https://x.com/datos.json").is_none());
    }

    #[test]
    fn el_limite_deja_fallar_solo_las_primeras() {
        let mut r = Ruta { patron: "/a".into(), accion: Accion::Bloquear, limite: Some(2), veces: 0 };
        assert!(!r.agotada());
        r.veces = 2;
        assert!(r.agotada(), "debería dejar de disparar tras 2 veces");
    }

    #[test]
    fn el_motivo_se_escribe_como_uno_quiera() {
        assert_eq!(motivo("timedout").unwrap(), "TimedOut");
        assert_eq!(motivo("TimedOut").unwrap(), "TimedOut");
        assert_eq!(motivo("connection_refused").unwrap(), "ConnectionRefused");
    }

    #[test]
    fn un_motivo_inventado_lista_los_validos() {
        let e = motivo("explota").unwrap_err();
        assert!(e.contains("timedout"), "no listó los válidos: {e}");
    }

    #[test]
    fn el_mock_viaja_en_base64_con_su_estado() {
        let s = Simulada {
            status: 503,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: "{\"error\":\"caido\"}".into(),
        };
        let (metodo, p) = respuesta_cdp(&Accion::Simular(s), "req-1");
        assert_eq!(metodo, "Fetch.fulfillRequest");
        assert_eq!(p["responseCode"], 503);
        // El cuerpo va codificado: CDP no acepta texto crudo.
        assert_eq!(p["body"], "eyJlcnJvciI6ImNhaWRvIn0=");
    }

    #[test]
    fn las_cabeceras_van_en_la_forma_que_pide_cdp() {
        let a = Accion::Cabeceras(vec![("Authorization".into(), "Bearer x".into())]);
        let (metodo, p) = respuesta_cdp(&a, "req-2");
        assert_eq!(metodo, "Fetch.continueRequest");
        assert_eq!(p["headers"][0]["name"], "Authorization");
        assert_eq!(p["headers"][0]["value"], "Bearer x");
    }
}
