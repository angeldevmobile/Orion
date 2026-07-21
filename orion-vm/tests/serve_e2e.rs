//! E2E del stack web: levanta tests/server_e2e.orx con el binario real de
//! Orion y lo golpea con HTTP de verdad (router, middlewares, headers, CORS,
//! JWT, rate limit, SSE, state). Un solo #[test] secuencial: el rate limiter
//! y el contador dependen del orden de los requests.
//! OJO: el fixture NO puede llamarse test_*.orx ni *_test.orx — el runner de
//! `orion test` lo ejecutaría y se quedaría bloqueado en serve.

use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const BASE: &str = "http://127.0.0.1:47611";

struct ServerGuard(Child);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Crea la carpeta de archivos estáticos que sirve el servidor de prueba,
/// incluido un "PNG" con bytes no-UTF8 para verificar integridad binaria.
fn write_static_fixtures() -> Vec<u8> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/static_e2e");
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(format!("{}/index.html", dir), "<h1>portada</h1>").unwrap();
    std::fs::write(format!("{}/style.css", dir), "body{color:red}").unwrap();
    let png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0xFF, 0x10, 0x80];
    std::fs::write(format!("{}/logo.png", dir), &png).unwrap();
    png
}

fn start_server() -> ServerGuard {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/server_e2e.orx");
    let child = Command::new(env!("CARGO_BIN_EXE_orion"))
        .arg(script)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("no se pudo lanzar el binario de orion");
    let guard = ServerGuard(child);

    // Esperar a que el puerto acepte conexiones (máx ~20 s)
    for _ in 0..80 {
        if TcpStream::connect_timeout(
            &"127.0.0.1:47611".parse().unwrap(),
            Duration::from_millis(250),
        ).is_ok() {
            return guard;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    panic!("el servidor de prueba nunca abrió el puerto 47611");
}

fn unpack(resp: ureq::Response) -> (u16, Vec<(String, String)>, String) {
    let status = resp.status();
    let headers: Vec<(String, String)> = resp.headers_names().iter()
        .filter_map(|n| resp.header(n).map(|v| (n.to_lowercase(), v.to_string())))
        .collect();
    let body = resp.into_string().unwrap_or_default();
    (status, headers, body)
}

fn get(url: &str) -> (u16, Vec<(String, String)>, String) {
    match ureq::get(url).call() {
        Ok(r) => unpack(r),
        Err(ureq::Error::Status(_, r)) => unpack(r),
        Err(e) => panic!("GET {} falló: {}", url, e),
    }
}

fn get_with(url: &str, header: (&str, &str)) -> (u16, Vec<(String, String)>, String) {
    match ureq::get(url).set(header.0, header.1).call() {
        Ok(r) => unpack(r),
        Err(ureq::Error::Status(_, r)) => unpack(r),
        Err(e) => panic!("GET {} falló: {}", url, e),
    }
}

fn post(url: &str, body: &str) -> (u16, Vec<(String, String)>, String) {
    match ureq::post(url).set("Content-Type", "application/json").send_string(body) {
        Ok(r) => unpack(r),
        Err(ureq::Error::Status(_, r)) => unpack(r),
        Err(e) => panic!("POST {} falló: {}", url, e),
    }
}

fn header_of<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn get_bytes(url: &str) -> (u16, Vec<(String, String)>, Vec<u8>) {
    use std::io::Read;
    let unpack_b = |r: ureq::Response| {
        let status = r.status();
        let headers: Vec<(String, String)> = r.headers_names().iter()
            .filter_map(|n| r.header(n).map(|v| (n.to_lowercase(), v.to_string())))
            .collect();
        let mut buf = Vec::new();
        r.into_reader().read_to_end(&mut buf).unwrap();
        (status, headers, buf)
    };
    match ureq::get(url).call() {
        Ok(r) => unpack_b(r),
        Err(ureq::Error::Status(_, r)) => unpack_b(r),
        Err(e) => panic!("GET {} falló: {}", url, e),
    }
}

#[test]
fn stack_web_e2e() {
    let png = write_static_fixtures();
    let _server = start_server();

    // 1. Ruta raíz por el router + nosniff por defecto
    let (st, hs, body) = get(&format!("{}/", BASE));
    assert_eq!(st, 200);
    assert_eq!(body, "hola orion");
    assert_eq!(header_of(&hs, "x-content-type-options"), Some("nosniff"));

    // 2. req expone query decodificada, headers e ip
    let (st, _, body) = get_with(
        &format!("{}/eco?q=hola%20mundo", BASE),
        ("User-Agent", "orion-e2e"),
    );
    assert_eq!(st, 200);
    let eco: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(eco["metodo"], "GET");
    assert_eq!(eco["q"], "hola mundo");
    assert_eq!(eco["agente"], "orion-e2e");
    assert_eq!(eco["ip"], "127.0.0.1");

    // 3. '+' en la query también es espacio
    let (_, _, body) = get(&format!("{}/eco?q=a+b", BASE));
    let eco: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(eco["q"], "a b");

    // 4. El body del POST llega al handler
    let (st, _, body) = post(&format!("{}/eco", BASE), "{\"x\":1}");
    assert_eq!(st, 200);
    let eco: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(eco["metodo"], "POST");
    assert_eq!(eco["cuerpo"], "{\"x\":1}");

    // 5. Parámetros de ruta :id/:seccion
    let (st, _, body) = get(&format!("{}/usuarios/42/perfil", BASE));
    assert_eq!(st, 200);
    let u: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(u["id"], "42");
    assert_eq!(u["seccion"], "perfil");

    // 6. Wildcard *resto captura el path restante
    let (st, _, body) = get(&format!("{}/archivos/a/b/c.txt", BASE));
    assert_eq!(st, 200);
    assert_eq!(body, "ruta:a/b/c.txt");

    // 7. Respuesta con status, headers custom y Location
    let (st, hs, body) = post(&format!("{}/usuarios", BASE), "{\"nombre\":\"eva\"}");
    assert_eq!(st, 201);
    let c: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(c["creado"], "eva");
    assert_eq!(header_of(&hs, "x-creado-por"), Some("orion"));
    assert_eq!(header_of(&hs, "location"), Some("/usuarios/99"));

    // 8. CORS end-to-end: los headers de middleware.cors llegan al cliente
    let (st, hs, _) = get(&format!("{}/cors", BASE));
    assert_eq!(st, 200);
    assert_eq!(header_of(&hs, "access-control-allow-origin"), Some("https://miapp.com"));
    assert!(header_of(&hs, "access-control-allow-methods").is_some());

    // 9. JWT end-to-end: sin token 401, con "Bearer <token>" 200
    let (st, _, _) = get(&format!("{}/privado", BASE));
    assert_eq!(st, 401);
    let (_, _, token) = get(&format!("{}/token", BASE));
    assert!(token.split('.').count() == 3, "token JWT con 3 partes");
    let (st, _, body) = get_with(
        &format!("{}/privado", BASE),
        ("Authorization", &format!("Bearer {}", token)),
    );
    assert_eq!(st, 200);
    assert_eq!(body, "bienvenido ana");

    // 10. Rate limit por IP: 3 pasan, la 4ª es 429
    for i in 1..=3 {
        let (st, _, _) = get(&format!("{}/limitado", BASE));
        assert_eq!(st, 200, "request {} debía pasar", i);
    }
    let (st, _, body) = get(&format!("{}/limitado", BASE));
    assert_eq!(st, 429);
    assert_eq!(body, "demasiadas peticiones");

    // 11. state.incr es atómico y compartido entre workers
    let (_, _, v1) = get(&format!("{}/contador", BASE));
    let (_, _, v2) = get(&format!("{}/contador", BASE));
    assert_eq!(v1, "visita 1");
    assert_eq!(v2, "visita 2");

    // 12. SSE: content-type y eventos bien formados
    let (st, hs, body) = get(&format!("{}/eventos", BASE));
    assert_eq!(st, 200);
    assert!(header_of(&hs, "content-type").unwrap().starts_with("text/event-stream"));
    assert!(body.contains("event: progreso\ndata: 50"));
    assert!(body.contains("event: fin\ndata: {\"ok\":true}"));
    assert_eq!(header_of(&hs, "cache-control"), Some("no-cache"));

    // 13. Sin ruta → fallback handler de serve
    let (st, _, body) = get(&format!("{}/no_existe", BASE));
    assert_eq!(st, 404);
    assert_eq!(body, "sin ruta: /no_existe");

    // 14. Middleware global corta la petición
    let (st, _, body) = get_with(&format!("{}/", BASE), ("X-Bloqueado", "1"));
    assert_eq!(st, 403);
    assert_eq!(body, "bloqueado por middleware");

    // 15. Estáticos: CSS con MIME correcto
    let (st, hs, body) = get(&format!("{}/static/style.css", BASE));
    assert_eq!(st, 200);
    assert!(header_of(&hs, "content-type").unwrap().starts_with("text/css"));
    assert_eq!(body, "body{color:red}");

    // 16. Binario servido byte a byte (no-UTF8 intacto)
    let (st, hs, bytes) = get_bytes(&format!("{}/static/logo.png", BASE));
    assert_eq!(st, 200);
    assert_eq!(header_of(&hs, "content-type"), Some("image/png"));
    assert_eq!(bytes, png, "el PNG debe llegar byte a byte");

    // 17. Directorio → index.html
    let (st, hs, body) = get(&format!("{}/static/", BASE));
    assert_eq!(st, 200);
    assert!(header_of(&hs, "content-type").unwrap().starts_with("text/html"));
    assert_eq!(body, "<h1>portada</h1>");

    // 18. Path traversal bloqueado (../ codificado no escapa la carpeta)
    let (st, _, _) = get(&format!("{}/static/%2e%2e/server_e2e.orx", BASE));
    assert_eq!(st, 404, "un ../ codificado no debe salir de la carpeta estática");

    // 19. Estático inexistente → 404 directo
    let (st, _, body) = get(&format!("{}/static/nope.css", BASE));
    assert_eq!(st, 404);
    assert_eq!(body, "archivo no encontrado");

    // 20. Respuesta { "file": ruta } → binario con MIME automático
    let (st, hs, bytes) = get_bytes(&format!("{}/descarga", BASE));
    assert_eq!(st, 200);
    assert_eq!(header_of(&hs, "content-type"), Some("image/png"));
    assert_eq!(bytes, png, "file: debe enviar el archivo byte a byte");
}
