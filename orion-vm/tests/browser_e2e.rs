//! Módulo `browser` de punta a punta: arrancar Chromium, hablar CDP y limpiar.
//!
//! Las páginas se sirven desde un servidor local propio, no desde internet: un
//! test que depende de la red falla por motivos que no son el código, y un test
//! que falla por motivos ajenos deja de mirarse.
//!
//! Si la máquina no tiene ningún navegador basado en Chromium, los tests se
//! saltan en vez de fallar — no tenerlo instalado no es un defecto de Orion.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;

fn orion_bin() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") { p.pop(); }
    p.join(if cfg!(windows) { "orion.exe" } else { "orion" })
}

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("orion_browser_tests").join(name);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Sirve un HTML fijo en un puerto libre y devuelve su URL.
fn serve_html(html: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir puerto");
    let port = listener.local_addr().unwrap().port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                html.len(), html
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });

    format!("http://127.0.0.1:{port}/")
}

/// Ejecuta un programa Orion y devuelve su salida.
fn run_orion(dir: &PathBuf, fuente: &str) -> (String, bool) {
    let f = dir.join("prog.orx");
    fs::write(&f, fuente).unwrap();
    let out = Command::new(orion_bin())
        .arg("run").arg(&f)
        .current_dir(dir)
        .output()
        .expect("no se pudo ejecutar orion");
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        out.status.success(),
    )
}

/// ¿Hay navegador en esta máquina? Se le pregunta al propio módulo.
fn hay_navegador(dir: &PathBuf) -> bool {
    let (salida, _) = run_orion(dir, "use \"browser\" as web\nshow(web.info())\n");
    salida.contains("found: yes")
}

const PAGINA: &str = r#"<!doctype html>
<html><head><title>Pagina de prueba</title></head>
<body><h1>Hola Orion</h1><div id="n" data-valor="42">contenido</div></body></html>"#;

const PAGINA_CON_ALERT: &str = r#"<!doctype html>
<html><head><title>Bloqueada</title></head>
<body><script>alert("confirma algo");</script></body></html>"#;

#[test]
fn navega_lee_el_titulo_y_evalua_javascript() {
    let dir = tmp_dir("navegacion");
    if !hay_navegador(&dir) {
        eprintln!("[skip] no hay navegador Chromium en esta máquina");
        return;
    }
    let url = serve_html(PAGINA);

    let (salida, ok) = run_orion(&dir, &format!(r#"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    show("T=" + web.title(p))
    show("H=" + web.eval(p, "document.querySelector('h1').textContent"))
    show("A=" + web.eval(p, "document.getElementById('n').dataset.valor"))
}}
show("FIN")
"#));

    assert!(ok, "el programa falló:\n{salida}");
    assert!(salida.contains("T=Pagina de prueba"), "título incorrecto:\n{salida}");
    assert!(salida.contains("H=Hola Orion"), "no se leyó el h1:\n{salida}");
    assert!(salida.contains("A=42"), "no se leyó el atributo:\n{salida}");
    assert!(salida.contains("FIN"), "el bloque with no terminó:\n{salida}");
}

#[test]
fn el_with_cierra_el_navegador_aunque_falle_el_cuerpo() {
    // Es la garantía que hace usable el módulo: un error a mitad no debe dejar
    // un Chrome huérfano comiendo memoria.
    let dir = tmp_dir("limpieza");
    if !hay_navegador(&dir) { return; }
    let url = serve_html(PAGINA);

    let (salida, ok) = run_orion(&dir, &format!(r#"
use "browser" as web
attempt {{
    with b = web.open() {{
        p = web.page(b)
        web.goto(p, "{url}")
        error "fallo a proposito"
    }}
}} handle e {{
    show("CAPTURADO")
}}
show(web.info())
"#));

    assert!(ok, "el programa falló:\n{salida}");
    assert!(salida.contains("CAPTURADO"), "no se capturó el error:\n{salida}");
    assert!(salida.contains("open_browsers: 0"),
            "quedó un navegador abierto tras el error:\n{salida}");
    assert!(salida.contains("open_pages: 0"),
            "quedó una pestaña abierta tras el error:\n{salida}");
}

#[test]
fn un_dialogo_que_bloquea_la_carga_se_explica() {
    // Sin esto el síntoma sería un timeout genérico: el caso clásico de
    // "se me queda colgado" que nadie sabe diagnosticar.
    let dir = tmp_dir("dialogo");
    if !hay_navegador(&dir) { return; }
    let url = serve_html(PAGINA_CON_ALERT);

    let (salida, _) = run_orion(&dir, &format!(r#"
use "browser" as web
with b = web.open({{ timeout: 8000 }}) {{
    p = web.page(b)
    attempt {{
        web.goto(p, "{url}")
        show("SIN-ERROR")
    }} handle e {{
        show("E=" + e)
    }}
}}
"#));

    // Chrome headless puede autodescartar el diálogo; si lo hace, la carga
    // termina y no hay nada que explicar. Lo que no puede pasar es que se
    // quede colgado sin decir nada.
    assert!(
        salida.contains("SIN-ERROR") || salida.contains("dialog") || salida.contains("alert"),
        "ni cargó ni explicó el bloqueo:\n{salida}"
    );
}

#[test]
fn los_handles_equivocados_dan_errores_claros() {
    let dir = tmp_dir("handles");
    if !hay_navegador(&dir) { return; }

    let (salida, _) = run_orion(&dir, r#"
use "browser" as web
with b = web.open() {
    p = web.page(b)
    attempt { web.goto(b, "https://x.dev") } handle e { show("E1=" + e) }
    attempt { web.page(p) }               handle e { show("E2=" + e) }
}
"#);

    assert!(salida.contains("E1=") && salida.contains("es un navegador, no una pestaña"),
            "usar un navegador como pestaña debería explicarse:\n{salida}");
    assert!(salida.contains("E2=") && salida.contains("es una pestaña, no un navegador"),
            "usar una pestaña como navegador debería explicarse:\n{salida}");
}

#[test]
fn una_ruta_de_navegador_inventada_falla_con_instrucciones() {
    // No necesita navegador instalado: es justo el caso de que no lo haya.
    let dir = tmp_dir("ruta_mala");
    let (salida, _) = run_orion(&dir, r#"
use "browser" as web
attempt {
    b = web.open({ chrome: "/no/existe/chrome.exe" })
} handle e {
    show("E=" + e)
}
"#);
    assert!(salida.contains("no existe"), "no se explicó el problema:\n{salida}");
    assert!(salida.contains("browser.open"), "no se dijo cómo arreglarlo:\n{salida}");
}
