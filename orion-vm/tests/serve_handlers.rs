//! Regresiones de `serve` encontradas construyendo la demo `comercio`:
//!
//!   1. Un `attempt` escrito en el script principal no capturaba nada dentro
//!      de un handler: el error salía como 500. En un módulo sí funcionaba.
//!   2. Un módulo importado solo por otro módulo no llegaba a los handlers.
//!   3. Un middleware que fallaba respondía 200 con "error interno" de cuerpo.
//!
//! Levanta un servidor real con el binario de Orion y le hace peticiones HTTP.

use std::fs;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const PUERTO: u16 = 47623;

struct Servidor(Child);
impl Drop for Servidor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

const PROGRAMA: &str = r##"
use "router" as router
use "json" as json
use "lib/a" as a

-- 1. attempt en el script principal, dentro de un handler.
fn leer(req) {
    attempt {
        return { "status": 200, "body": "json " + str(json.parse(req["body"])) }
    } handle err {
        return { "status": 400, "body": "atrapado" }
    }
}

-- 2. lib/a importa lib/b; este script no importa lib/b.
fn anidado(req) {
    return { "status": 200, "body": a.llama() }
}

-- 3. El middleware revienta en /boom y deja pasar el resto.
fn mw(req) {
    if req["path"] == "/boom" {
        x = 1 / 0
    }
    return null
}

-- `and` con la derecha que revienta si se evalúa: dentro de serve también
-- tiene que cortocircuitar.
fn corto(req) {
    if has_key(req["query"], "k") and req["query"]["k"] == "1" {
        return { "status": 200, "body": "con k" }
    }
    return { "status": 200, "body": "sin k" }
}

fn nf(req) { return { "status": 404, "body": "nf" } }

r = router.new()
router.post(r, "/leer", "leer")
router.get(r, "/anidado", "anidado")
router.get(r, "/boom", "anidado")
router.get(r, "/corto", "corto")
router.use_middleware(r, "mw")
router.attach(r)
serve PUERTO nf
"##;

fn arrancar() -> Servidor {
    let dir = std::env::temp_dir().join("orion_tests_serve_handlers");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("lib")).unwrap();
    fs::write(dir.join("lib/b.orx"), "LIMITE = 7\nfn hola() {\n    return \"hola desde b\"\n}\n").unwrap();
    fs::write(dir.join("lib/a.orx"),
        "use \"lib/b\" as b\nfn llama() {\n    return b.hola() + \" \" + str(b.LIMITE)\n}\n").unwrap();
    fs::write(dir.join("servidor.orx"), PROGRAMA.replace("PUERTO", &PUERTO.to_string())).unwrap();

    let hijo = Command::new(PathBuf::from(env!("CARGO_BIN_EXE_orion")))
        .args(["--run", "servidor.orx"])
        .current_dir(&dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("no se pudo lanzar orion");
    let guarda = Servidor(hijo);

    let dir_puerto = format!("127.0.0.1:{PUERTO}").parse().unwrap();
    for _ in 0..80 {
        if TcpStream::connect_timeout(&dir_puerto, Duration::from_millis(250)).is_ok() {
            return guarda;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    panic!("el servidor de prueba nunca abrió el puerto {PUERTO}");
}

fn respuesta(r: Result<ureq::Response, ureq::Error>) -> (u16, String) {
    match r {
        Ok(r) => (r.status(), r.into_string().unwrap_or_default()),
        Err(ureq::Error::Status(c, r)) => (c, r.into_string().unwrap_or_default()),
        Err(e) => panic!("la petición falló: {e}"),
    }
}

fn url(ruta: &str) -> String {
    format!("http://127.0.0.1:{PUERTO}{ruta}")
}

#[test]
fn serve_handlers_e2e() {
    let _s = arrancar();

    // 1. attempt del script principal dentro de un handler.
    let (c, b) = respuesta(ureq::post(&url("/leer")).send_string("no es json"));
    assert_eq!((c, b.as_str()), (400, "atrapado"), "el attempt del handler no capturó el error");
    let (c, b) = respuesta(ureq::post(&url("/leer")).send_string("[1, 2]"));
    assert_eq!(c, 200, "{b}");
    assert!(b.starts_with("json"), "{b}");

    // 2. Módulo importado solo por otro módulo.
    let (c, b) = respuesta(ureq::get(&url("/anidado")).call());
    assert_eq!((c, b.as_str()), (200, "hola desde b 7"), "el módulo anidado no llegó al handler");

    // 3. Middleware que falla: 500, no 200.
    let (c, b) = respuesta(ureq::get(&url("/boom")).call());
    assert_eq!(c, 500, "un middleware que falla tenía que responder 500, respondió {c}: {b}");

    // Cortocircuito dentro de serve.
    let (c, b) = respuesta(ureq::get(&url("/corto")).call());
    assert_eq!((c, b.as_str()), (200, "sin k"));
    let (c, b) = respuesta(ureq::get(&url("/corto?k=1")).call());
    assert_eq!((c, b.as_str()), (200, "con k"));
}
