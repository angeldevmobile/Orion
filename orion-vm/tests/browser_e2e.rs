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

/// Sirve varias rutas: la página, y un archivo que se descarga de verdad.
///
/// El servidor de arriba contesta lo mismo a cualquier ruta, y una descarga
/// necesita justo lo contrario: una ruta que devuelva `Content-Disposition:
/// attachment`, que es lo que hace que el navegador descargue en vez de mostrar.
/// Sin esa cabecera no se prueba nada de lo que se quiere probar.
fn serve_rutas(pagina: &'static str, archivo: &'static [u8], nombre: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir puerto");
    let port = listener.local_addr().unwrap().port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let peticion = String::from_utf8_lossy(&buf[..n]).to_string();
            let ruta = peticion.split_whitespace().nth(1).unwrap_or("/").to_string();

            if ruta.starts_with("/descarga") {
                let cab = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\n\
                     Content-Disposition: attachment; filename=\"{nombre}\"\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                    archivo.len()
                );
                let _ = s.write_all(cab.as_bytes());
                let _ = s.write_all(archivo);
            } else {
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    pagina.len(), pagina
                );
                let _ = s.write_all(resp.as_bytes());
            }
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

/// Turno para arrancar un navegador.
///
/// Cargo ejecuta los tests en paralelo, y un Chrome por test significa una
/// docena de navegadores compitiendo a la vez: en una máquina cargada, una
/// navegación llega a superar los 30 s y el test falla por contención, no por
/// un defecto. Se serializan para que lo que se mida sea el módulo.
static TURNO: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn turno() -> std::sync::MutexGuard<'static, ()> {
    // Un test que falle envenena el mutex; recuperarlo evita que un fallo
    // legítimo se convierta en una cascada de fallos sin relación.
    TURNO.lock().unwrap_or_else(|e| e.into_inner())
}

const PAGINA: &str = r#"<!doctype html>
<html><head><title>Pagina de prueba</title></head>
<body><h1>Hola Orion</h1><div id="n" data-valor="42">contenido</div></body></html>"#;

const PAGINA_CON_ALERT: &str = r#"<!doctype html>
<html><head><title>Bloqueada</title></head>
<body><script>alert("confirma algo");</script></body></html>"#;

/// Página con las tres trampas que tumban a un scraper de Selenium:
/// un banner que tapa los controles, un campo que solo reacciona a eventos de
/// teclado reales, y resultados que llegan después de la acción que los pidió.
const PAGINA_VIVA: &str = r#"<!doctype html>
<html><head><title>Viva</title>
<style>#tapa{position:fixed;top:0;left:0;width:100%;height:100%;background:#0008;z-index:9}</style>
</head><body>
<div id="tapa"></div>
<input id="q" placeholder="buscar">
<button id="ir">Buscar</button>
<div id="salida">nada</div>
<ul id="lista"></ul>
<script>
  setTimeout(() => document.getElementById('tapa').remove(), 700);
  let teclas = 0;
  document.getElementById('q').addEventListener('keydown', () => teclas++);
  const buscar = () => {
    const v = document.getElementById('q').value;
    document.getElementById('salida').textContent = 'v=' + v + ' teclas=' + teclas;
    setTimeout(() => {
      document.getElementById('lista').innerHTML =
        ['uno','dos','tres'].map((t,i) => '<li class="it" data-n="'+i+'">'+t+'</li>').join('');
    }, 400);
  };
  document.getElementById('ir').addEventListener('click', buscar);
  document.getElementById('q').addEventListener('keydown', e => { if (e.key === 'Enter') buscar(); });
</script></body></html>"#;

/// Página cuyo botón nunca se destapa: sirve para comprobar que, cuando de
/// verdad no se puede clicar, el error dice por qué.
const PAGINA_TAPADA: &str = r#"<!doctype html>
<html><head><title>Tapada</title>
<style>#velo{position:fixed;top:0;left:0;width:100%;height:100%;background:#0008;z-index:9}</style>
</head><body>
<div id="velo" class="cookie-banner"></div>
<button id="ir">Buscar</button>
</body></html>"#;

#[test]
fn navega_lee_el_titulo_y_evalua_javascript() {
    let dir = tmp_dir("navegacion");
    if !hay_navegador(&dir) {
        eprintln!("[skip] no hay navegador Chromium en esta máquina");
        return;
    }
    let _turno = turno();
    let url = serve_html(PAGINA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    show("T=" + web.title(p))
    show("H=" + web.eval(p, "document.querySelector('h1').textContent"))
    show("A=" + web.eval(p, "document.getElementById('n').dataset.valor"))
}}
show("FIN")
"##));

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
    let _turno = turno();
    let url = serve_html(PAGINA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
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
"##));

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
    let _turno = turno();
    let url = serve_html(PAGINA_CON_ALERT);

    let (salida, _) = run_orion(&dir, &format!(r##"
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
"##));

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
    let _turno = turno();

    let (salida, _) = run_orion(&dir, r##"
use "browser" as web
with b = web.open() {
    p = web.page(b)
    attempt { web.goto(b, "https://x.dev") } handle e { show("E1=" + e) }
    attempt { web.page(p) }               handle e { show("E2=" + e) }
}
"##);

    assert!(salida.contains("E1=") && salida.contains("es un navegador, no una pestaña"),
            "usar un navegador como pestaña debería explicarse:\n{salida}");
    assert!(salida.contains("E2=") && salida.contains("es una pestaña, no un navegador"),
            "usar una pestaña como navegador debería explicarse:\n{salida}");
}

#[test]
fn una_ruta_de_navegador_inventada_falla_con_instrucciones() {
    // No necesita navegador instalado: es justo el caso de que no lo haya.
    let dir = tmp_dir("ruta_mala");
    let (salida, _) = run_orion(&dir, r##"
use "browser" as web
attempt {
    b = web.open({ chrome: "/no/existe/chrome.exe" })
} handle e {
    show("E=" + e)
}
"##);
    assert!(salida.contains("no existe"), "no se explicó el problema:\n{salida}");
    assert!(salida.contains("browser.open"), "no se dijo cómo arreglarlo:\n{salida}");
}

#[test]
fn el_click_espera_a_que_se_quite_lo_que_tapa() {
    // Selenium clicaría sobre el banner y seguiría como si nada. Aquí el clic
    // espera a que el elemento sea accionable de verdad.
    let dir = tmp_dir("click_tapado");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_VIVA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.click(p, "#ir")
    show("S=" + web.text(p, "#salida"))
}}
"##));
    assert!(ok, "el clic falló:\n{salida}");
    assert!(salida.contains("S=v= teclas=0"),
            "el clic no llegó al botón real:\n{salida}");
}

#[test]
fn escribir_produce_eventos_de_teclado_reales() {
    // Si `type` asignara `value` desde JS, el contador de teclas del sitio se
    // quedaría en cero y los formularios de React no se enterarían del cambio.
    let dir = tmp_dir("teclado_real");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_VIVA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.type(p, "#q", "abc")
    web.click(p, "#ir")
    show("S=" + web.text(p, "#salida"))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("v=abc"), "el texto no llegó al campo:\n{salida}");
    assert!(!salida.contains("teclas=0"),
            "no se dispararon eventos de teclado reales:\n{salida}");
}

#[test]
fn press_enter_dispara_el_formulario() {
    let dir = tmp_dir("press_enter");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_VIVA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.type(p, "#q", "hola")
    web.press(p, "enter")
    show("S=" + web.text(p, "#salida"))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("v=hola"), "el Enter no envió el formulario:\n{salida}");
}

#[test]
fn las_lecturas_de_contenido_esperan_y_las_de_estado_no() {
    // La regla que hace el módulo usable: `text`/`texts`/`attr` esperan a que
    // haya contenido; `exists`/`count` responden sobre el instante actual.
    let dir = tmp_dir("espera_lectura");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_VIVA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.click(p, "#ir")
    show("C=" + str(web.count(p, ".it")))
    show("T=" + str(web.texts(p, ".it")))
    show("A=" + str(web.attr(p, ".it", "data-n")))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("C=0"),
            "count no debería esperar: la lista aún no existía:\n{salida}");
    assert!(salida.contains("uno") && salida.contains("tres"),
            "texts debería haber esperado a la lista:\n{salida}");
    assert!(salida.contains("A=0"), "attr debería haber esperado:\n{salida}");
}

#[test]
fn css_xpath_y_texto_son_el_mismo_selector() {
    let dir = tmp_dir("selectores");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_VIVA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.click(p, "#ir")
    web.wait(p, ".it")
    show("CSS=" + web.text(p, ".it"))
    show("XP=" + web.text(p, "//li[@data-n='2']"))
    show("TX=" + str(web.exists(p, "text=dos")))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("CSS=uno"), "CSS no funcionó:\n{salida}");
    assert!(salida.contains("XP=tres"), "XPath no funcionó:\n{salida}");
    assert!(salida.contains("TX=yes"), "el selector por texto no funcionó:\n{salida}");
}

#[test]
fn cuando_de_verdad_no_se_puede_clicar_el_error_dice_por_que() {
    // "element not interactable" no ayuda a nadie. El error debe nombrar lo que
    // está estorbando.
    let dir = tmp_dir("tapado_siempre");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_TAPADA);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{ web.click(p, "#ir", 1500) }} handle e {{ show("E=" + e) }}
}}
"##));
    assert!(salida.contains("E="), "debería haber fallado:\n{salida}");
    assert!(salida.contains("lo tapa"), "el error no dice qué estorba:\n{salida}");
    assert!(salida.contains("cookie-banner"),
            "el error no identifica al elemento que tapa:\n{salida}");
}

#[test]
fn la_captura_se_escribe_en_disco() {
    let dir = tmp_dir("captura");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.screenshot(p, "shot.png")
}}
"##));
    assert!(ok, "falló:\n{salida}");
    let png = dir.join("shot.png");
    assert!(png.is_file(), "no se escribió la captura:\n{salida}");
    let bytes = fs::read(&png).unwrap();
    assert!(bytes.len() > 100, "la captura está vacía");
    assert_eq!(&bytes[..4], b"\x89PNG", "no es un PNG válido");
}

#[test]
fn una_tecla_inventada_lista_las_validas() {
    let dir = tmp_dir("tecla_mala");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{ web.press(p, "teletransporte") }} handle e {{ show("E=" + e) }}
}}
"##));
    assert!(salida.contains("desconocida"), "no se explicó el error:\n{salida}");
    assert!(salida.contains("enter"), "no se listaron las teclas válidas:\n{salida}");
}

/// Cabecera fija que cubre la mitad superior del botón y nunca se quita.
const PAGINA_PARCIAL: &str = r##"<!doctype html>
<html><head><title>Parcial</title><style>
 body{margin:0}
 #cab{position:fixed;top:0;left:0;width:100%;height:118px;background:#c00;z-index:9}
 button{position:absolute;top:90px;left:40px;padding:22px 30px;font-size:16px}
</style></head><body>
<div id="cab" class="cabecera-fija"></div>
<button id="b">Medio tapado</button>
<div id="log">-</div>
<script>
  const log = t => document.getElementById('log').textContent = t;
  document.getElementById('b').onclick   = () => log('BOTON-OK');
  document.getElementById('cab').onclick = () => log('CLICASTE-LA-CABECERA');
</script></body></html>"##;

/// Velo a pantalla completa que no se quita jamás.
const PAGINA_VELO: &str = r##"<!doctype html>
<html><head><title>Velo</title><style>
 body{margin:0}
 #velo{position:fixed;top:0;left:0;width:100%;height:100%;background:#0008;z-index:99}
 button{position:absolute;top:200px;left:40px;padding:10px 20px}
</style></head><body>
<div id="velo" class="cookie-banner"></div>
<button id="total">Bajo el velo</button>
<div id="log">-</div>
<script>
  const log = t => document.getElementById('log').textContent = t;
  document.getElementById('total').onclick = () => log('LLEGO-AL-BOTON');
  document.getElementById('velo').onclick  = () => log('CLICASTE-EL-VELO');
</script></body></html>"##;

#[test]
fn un_tapado_parcial_se_clica_por_la_zona_libre() {
    // Probar solo el centro es lo que hacen las demás herramientas, y por eso
    // fallan con una cabecera fija sobre media mitad de un botón. Una persona
    // pincharía en la parte visible; esto hace lo mismo, sin pedir `force`.
    let dir = tmp_dir("tapado_parcial");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_PARCIAL);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.click(p, "#b", 1500)
    show("R=" + web.text(p, "#log"))
}}
"##));
    assert!(ok, "no consiguió clicar la zona libre:\n{salida}");
    assert!(salida.contains("R=BOTON-OK"), "el clic no llegó al botón:\n{salida}");
    assert!(!salida.contains("CABECERA"),
            "clicó la cabecera en vez del botón — el fallo silencioso de Selenium:\n{salida}");
}

#[test]
fn force_atraviesa_un_velo_permanente_sin_clicar_el_velo() {
    // La diferencia con clicar coordenadas a ciegas: el evento sigue siendo
    // real y aterriza en el botón, no en lo que estaba encima.
    let dir = tmp_dir("force_velo");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_VELO);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.click(p, "#total", {{ force: yes, wait: 1200 }})
    show("R=" + web.text(p, "#log"))
    show("PE=" + str(web.eval(p, "document.querySelectorAll('[data-orion-pe]').length")))
    show("VE=" + web.eval(p, "getComputedStyle(document.getElementById('velo')).pointerEvents"))
}}
"##));
    assert!(ok, "force no consiguió clicar:\n{salida}");
    assert!(salida.contains("R=LLEGO-AL-BOTON"), "el clic no llegó al botón:\n{salida}");
    assert!(!salida.contains("CLICASTE-EL-VELO"),
            "el clic aterrizó en el velo, que es justo lo que hay que evitar:\n{salida}");
    assert!(salida.contains("PE=0"), "quedaron elementos sin restaurar:\n{salida}");
    assert!(salida.contains("VE=auto"),
            "el velo se quedó sordo al ratón tras el clic forzado:\n{salida}");
}

#[test]
fn sin_force_el_error_dice_como_resolverlo() {
    let dir = tmp_dir("sugerencia_force");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_VELO);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{ web.click(p, "#total", 900) }} handle e {{ show("E=" + e) }}
}}
"##));
    assert!(salida.contains("cookie-banner"), "no nombra al culpable:\n{salida}");
    assert!(salida.contains("force: yes"), "no sugiere la salida:\n{salida}");
}

/// Página con todo lo que un scraper se encuentra en la vida real: modal con
/// fondo bloqueante, `<select>` nativo, menú desplegable, diálogos del
/// navegador y un iframe accesible.
const PAGINA_MODAL: &str = r##"<!doctype html><html><head><meta charset="utf-8"><title>Modales</title><style>
 #modal{display:none;position:fixed;top:20%;left:20%;width:60%;background:#fff;border:3px solid #333;padding:20px;z-index:50}
 #fondo{display:none;position:fixed;top:0;left:0;width:100%;height:100%;background:#0006;z-index:40}
 .abierto{display:block !important}
</style></head><body>
<button id="abrir">Abrir modal</button>
<div id="fondo"></div>
<div id="modal">
  <select id="pais"><option value="pe">Peru</option><option value="mx">Mexico</option></select>
  <input id="nom" placeholder="nombre">
  <button id="ok">Confirmar</button>
</div>
<div id="menu"><button id="mbtn">Menu</button><ul id="ops" style="display:none"><li id="op2">Opcion B</li></ul></div>
<button id="alerta">confirm</button>
<button id="preg">prompt</button>
<iframe id="marco" style="width:400px;height:120px"
        srcdoc="&lt;button id='dentro'&gt;En iframe&lt;/button&gt;&lt;div id='eco'&gt;-&lt;/div&gt;&lt;script&gt;document.getElementById('dentro').onclick=()=&gt;document.getElementById('eco').textContent='IFRAME-OK'&lt;/script&gt;"></iframe>
<div id="log">-</div>
<script>
 const log = t => document.getElementById('log').textContent = t;
 abrir.onclick = () => { modal.classList.add('abierto'); fondo.classList.add('abierto'); };
 ok.onclick = () => { log('MODAL:' + pais.value + ':' + nom.value);
                      modal.classList.remove('abierto'); fondo.classList.remove('abierto'); };
 mbtn.onclick = () => ops.style.display = 'block';
 op2.onclick = () => log('MENU-B');
 alerta.onclick = () => log(confirm('Seguro?') ? 'CONFIRM-SI' : 'CONFIRM-NO');
 preg.onclick = () => log('PROMPT:' + prompt('Nombre?'));
</script></body></html>"##;

#[test]
fn modal_con_select_nativo_y_desplegable() {
    // Un `<select>` abre un desplegable del sistema operativo, fuera del DOM:
    // ningún clic puede navegarlo. Por eso se elige la opción y se emiten
    // `input`/`change`, que es lo que el sitio escucha.
    let dir = tmp_dir("modales");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_MODAL);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.click(p, "#abrir")
    web.select(p, "#pais", "Mexico")
    web.type(p, "#nom", "Angel")
    web.click(p, "#ok")
    show("M=" + web.text(p, "#log"))
    web.click(p, "#mbtn")
    web.click(p, "#op2")
    show("D=" + web.text(p, "#log"))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("M=MODAL:mx:Angel"),
            "el select por texto visible o el campo del modal fallaron:\n{salida}");
    assert!(salida.contains("D=MENU-B"), "el desplegable no funcionó:\n{salida}");
}

#[test]
fn select_con_opcion_inexistente_lista_las_que_hay() {
    let dir = tmp_dir("select_malo");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_MODAL);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.click(p, "#abrir")
    attempt {{ web.select(p, "#pais", "Narnia") }} handle e {{ show("E=" + e) }}
}}
"##));
    assert!(salida.contains("no hay opción"), "no se explicó el fallo:\n{salida}");
    assert!(salida.contains("Peru") && salida.contains("Mexico"),
            "no se listaron las opciones disponibles:\n{salida}");
}

#[test]
fn los_dialogos_nativos_se_atienden_por_politica() {
    // Un diálogo sin atender congela la página sin dar ningún error: es el peor
    // fallo posible. La política se declara una vez y vale para la sesión.
    let dir = tmp_dir("dialogos");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_MODAL);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.dialogs(p, "accept")
    web.click(p, "#alerta")
    show("A=" + web.text(p, "#log"))
    web.dialogs(p, "dismiss")
    web.click(p, "#alerta")
    show("R=" + web.text(p, "#log"))
    web.dialogs(p, "answer:Orion")
    web.click(p, "#preg")
    show("P=" + web.text(p, "#log"))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("A=CONFIRM-SI"), "accept no funcionó:\n{salida}");
    assert!(salida.contains("R=CONFIRM-NO"), "dismiss no funcionó:\n{salida}");
    assert!(salida.contains("P=PROMPT:Orion"), "answer no funcionó:\n{salida}");
}

#[test]
fn una_politica_de_dialogo_invalida_lista_las_validas() {
    let dir = tmp_dir("dialogo_malo");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{ web.dialogs(p, "quizas") }} handle e {{ show("E=" + e) }}
}}
"##));
    assert!(salida.contains("desconocida"), "no se explicó:\n{salida}");
    assert!(salida.contains("accept") && salida.contains("answer:"),
            "no se listaron las políticas válidas:\n{salida}");
}

#[test]
fn los_selectores_atraviesan_iframes_accesibles() {
    // Los modales de consentimiento suelen vivir en un iframe. Sin esto, el
    // selector correcto "no existe" y nadie entiende por qué.
    let dir = tmp_dir("iframe");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_MODAL);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.click(p, "#dentro")
    show("F=" + web.text(p, "#eco"))
}}
"##));
    assert!(ok, "no se pudo clicar dentro del iframe:\n{salida}");
    assert!(salida.contains("F=IFRAME-OK"),
            "el clic no llegó al botón del iframe:\n{salida}");
}

/// El contenido llega tarde a propósito: sirve para comprobar que el plazo de
/// espera es el que se pidió y no uno fijado en el código.
const PAGINA_LENTA: &str = r##"<!doctype html><html><head><meta charset="utf-8"><title>Lenta</title></head>
<body><div id="zona"></div>
<script>setTimeout(()=>{document.getElementById('zona').innerHTML='<b id="tarde">llegue tarde</b>'},2500)</script>
</body></html>"##;

#[test]
fn el_wait_se_puede_fijar_al_abrir_y_manda_la_llamada() {
    // Antes el plazo estaba fijado en 10 s y solo se podía cambiar repitiéndolo
    // en cada llamada. Ahora hay tres niveles: llamada > open() > default.
    let dir = tmp_dir("wait_global");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_LENTA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web

with b = web.open({{ wait: 800 }}) {{
    p = web.page(b)
    web.goto(p, "{url}")
    show("CORTO=" + str(web.text(p, "#tarde")))
}}
with b2 = web.open({{ wait: 6000 }}) {{
    p2 = web.page(b2)
    web.goto(p2, "{url}")
    show("LARGO=" + str(web.text(p2, "#tarde")))
}}
with b3 = web.open({{ wait: 300 }}) {{
    p3 = web.page(b3)
    web.goto(p3, "{url}")
    show("LLAMADA=" + str(web.text(p3, "#tarde", 6000)))
}}
"##));

    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("CORTO=null"),
            "con wait corto no debería haber esperado al contenido:\n{salida}");
    assert!(salida.contains("LARGO=llegue tarde"),
            "el wait global de open() no se respetó:\n{salida}");
    assert!(salida.contains("LLAMADA=llegue tarde"),
            "el plazo de la llamada debería mandar sobre el global:\n{salida}");
}

#[test]
fn el_afinado_de_mecanismo_se_acepta_y_no_estorba() {
    // Los parámetros de recursos van bajo `tuning` para no ensuciar la API de
    // uso diario, pero tienen que existir y no romper nada.
    let dir = tmp_dir("tuning_mecanismo");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open({{
    wait: 4000, retry: 20, drag_steps: 3, iframe_depth: 2, hit_inset: 8,
    tuning: {{ max_events: 64, idle_poll: 20, close_timeout: 3000, cleanup_tries: 4 }}
}}) {{
    p = web.page(b)
    web.goto(p, "{url}")
    show("T=" + web.title(p))
}}
"##));
    assert!(ok, "un afinado completo no debería romper nada:\n{salida}");
    assert!(salida.contains("T=Pagina de prueba"), "salida inesperada:\n{salida}");
}

#[test]
fn se_pueden_quitar_banderas_de_arranque() {
    // `extra` solo añadía. Sin poder quitar, un sitio que necesite extensiones
    // no tenía forma de deshacer `--disable-extensions`.
    let dir = tmp_dir("sin_banderas");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open({{ sin: ["--disable-extensions", "--mute-audio"] }}) {{
    p = web.page(b)
    web.goto(p, "{url}")
    show("T=" + web.title(p))
}}
"##));
    assert!(ok, "quitar banderas dejó el navegador inservible:\n{salida}");
    assert!(salida.contains("T=Pagina de prueba"), "salida inesperada:\n{salida}");
}

/// Listado con los dos formatos de precio del mundo real y una tabla, para
/// ejercitar la extracción de punta a punta.
const PAGINA_TIENDA: &str = r##"<!doctype html><html><head><meta charset="utf-8"><title>Tienda</title></head><body>
<div id="lista"></div>
<table id="t"><tbody>
 <tr><td>Fila A</td><td>1.234,56 EUR</td></tr>
 <tr><td>Fila B</td><td>$1,234.56</td></tr>
 <tr><td>Fila C</td><td>Agotado</td></tr>
</tbody></table>
<script>
setTimeout(() => {
 document.getElementById('lista').innerHTML = [
  {n:'Laptop Pro', p:'1.299,00 EUR', q:7,  u:'/p/1', d:'si'},
  {n:'Mouse',      p:'$24.99',       q:0,  u:'/p/2', d:'no'},
  {n:'Teclado',    p:'89,50 EUR',    q:12, u:'/p/3', d:'si'}
 ].map(x => '<div class="card" data-id="' + x.u.slice(3) + '">'
    + '<h3 class="title">' + x.n + '</h3>'
    + '<span class="price">' + x.p + '</span>'
    + '<b data-qty="' + x.q + '">stock</b>'
    + '<a href="' + x.u + '">ver</a>'
    + '<em class="disp">' + x.d + '</em></div>').join('');
}, 500);
</script></body></html>"##;

#[test]
fn extract_saca_un_listado_completo_en_una_llamada() {
    let dir = tmp_dir("extract_listado");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_TIENDA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    items = web.extract(p, ".card", {{
        id: "@data-id", nombre: ".title", precio: ".price|num",
        stock: "[data-qty]@data-qty|int", url: "a@href", hay: ".disp|bool"
    }})
    show("N=" + str(len(items)))
    for it in items {{ show("R=" + str(it)) }}
}}
"##));

    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("N=3"), "no salieron las 3 filas:\n{salida}");
    // Formato europeo: 1.299,00 debe ser 1299, no 129900.
    assert!(salida.contains("precio: 1299"), "el precio europeo se malinterpretó:\n{salida}");
    // Formato estadounidense en la misma página.
    assert!(salida.contains("precio: 24.99"), "el precio con coma de miles falló:\n{salida}");
    assert!(salida.contains("precio: 89.5"), "el decimal con coma falló:\n{salida}");
    // Atributos, enlaces y conversiones.
    assert!(salida.contains("stock: 7") && salida.contains("stock: 0"),
            "no se leyeron los atributos numéricos:\n{salida}");
    assert!(salida.contains("url: /p/1"), "no se leyó el href:\n{salida}");
    assert!(salida.contains("hay: yes") && salida.contains("hay: no"),
            "la conversión a booleano falló:\n{salida}");
}

#[test]
fn un_xpath_absoluto_en_un_campo_se_relativiza() {
    // Regresión: `//td[1]` busca desde la raíz del documento y devuelve el
    // MISMO nodo para todas las filas — el listado sale repetido con datos que
    // parecen buenos. Es el fallo silencioso que hay que evitar.
    let dir = tmp_dir("extract_xpath");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_TIENDA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    filas = web.extract(p, "#t tbody tr", {{ que: "//td[1]", cuanto: "//td[2]|num" }}, {{ strict: no }})
    for f in filas {{ show("F=" + str(f)) }}
}}
"##));

    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("Fila A") && salida.contains("Fila B") && salida.contains("Fila C"),
            "el XPath absoluto devolvió la misma fila repetida:\n{salida}");
    assert!(salida.contains("cuanto: null"),
            "'Agotado' debería dar null, no un número inventado:\n{salida}");
}

#[test]
fn un_selector_muerto_se_delata_en_vez_de_devolver_nulls() {
    // La diferencia con BeautifulSoup: un campo vacío en TODAS las filas es un
    // selector equivocado, no un dato ausente. Callarlo devuelve una lista que
    // parece buena y revienta cien líneas más adelante.
    let dir = tmp_dir("extract_muerto");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_TIENDA);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{
        web.extract(p, ".card", {{ nombre: ".title", precio: ".precio-viejo|num" }})
    }} handle e {{ show("E=" + e) }}
    r = web.extract(p, ".card", {{ nombre: ".title", precio: ".precio-viejo|num" }}, {{ strict: no }})
    show("LAXO=" + str(r[0]))
}}
"##));

    assert!(salida.contains("E="), "debería haber fallado:\n{salida}");
    assert!(salida.contains("precio"), "el error no nombra el campo roto:\n{salida}");
    assert!(salida.contains(".precio-viejo"), "el error no muestra el selector:\n{salida}");
    assert!(salida.contains("strict: no"), "el error no dice cómo seguir:\n{salida}");
    // Y el campo que sí funciona no se ve afectado.
    assert!(salida.contains("LAXO=") && salida.contains("Laptop Pro"),
            "con strict:no debería devolver lo que sí encontró:\n{salida}");
}

#[test]
fn extract_espera_a_que_haya_filas() {
    // El listado llega 500 ms después de cargar. Devolver una lista vacía
    // convertiría un problema de tiempo en un resultado vacío silencioso.
    let dir = tmp_dir("extract_espera");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_TIENDA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    items = web.extract(p, ".card", {{ nombre: ".title" }})
    show("N=" + str(len(items)))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("N=3"), "no esperó a que apareciera el listado:\n{salida}");
}

/// Sirve varias páginas distintas en el mismo puerto, para probar recorridos.
fn serve_paginas(paginas: Vec<(String, String)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir puerto");
    let port = listener.local_addr().unwrap().port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let req = String::from_utf8_lossy(&buf).to_string();
            let ruta = req.split_whitespace().nth(1).unwrap_or("/").to_string();

            let cuerpo = paginas.iter().find(|(p, _)| *p == ruta).map(|(_, h)| h.clone());
            let resp = match cuerpo {
                Some(h) => format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    h.len(), h),
                // Un 404 con plantilla HTML: carga bien y simplemente no tiene
                // filas, que es justo el caso que no debe pasar desapercibido.
                None => {
                    let h = "<!doctype html><html><head><title>404</title></head><body><h1>No existe</h1></body></html>";
                    format!("HTTP/1.1 404 Not Found\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", h.len(), h)
                }
            };
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn pagina_de(n: u32, filas: u32) -> String {
    let cards: String = (1..=filas).map(|i| format!(
        r#"<div class="card" data-id="{n}-{i}"><h3 class="title">Producto {n}-{i}</h3><span class="price">{i}.{i:03}0,50</span><b data-qty="{i}">s</b></div>"#
    )).collect();
    format!(r#"<!doctype html><html><head><meta charset="utf-8"><title>P{n}</title></head><body>{cards}</body></html>"#)
}

#[test]
fn extract_to_recorre_varias_paginas_y_escribe_csv() {
    let dir = tmp_dir("extract_to_csv");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let base = serve_paginas((1..=3).map(|n| (format!("/p{n}"), pagina_de(n, 5))).collect());

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
esquema = {{ id: "@data-id", nombre: ".title", precio: ".price|num" }}
urls = ["{base}/p1", "{base}/p2", "{base}/p3"]
with b = web.open() {{
    p = web.page(b)
    r = web.extract_to(p, urls, ".card", esquema, "salida.csv")
    show("RES=" + str(r))
}}
"##));

    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("rows: 15"), "no salieron las 15 filas:\n{salida}");
    assert!(salida.contains("ok: 3"), "no recorrió las 3 páginas:\n{salida}");

    let csv = fs::read_to_string(dir.join("salida.csv")).expect("no se escribió el csv");
    assert_eq!(csv.lines().count(), 16, "faltan filas o la cabecera:\n{csv}");
    assert!(csv.starts_with("id,nombre,precio"), "cabecera incorrecta:\n{csv}");
    assert!(csv.contains("Producto 3-5"), "falta la última fila:\n{csv}");
}

#[test]
fn una_pagina_sin_filas_se_reporta_en_vez_de_perderse() {
    // Un 404 con plantilla, un redirect al login o un selector que dejó de
    // valer cargan bien y no dan filas. Sin reportarlo, el recorrido pierde
    // páginas en silencio y nadie lo nota hasta que faltan datos.
    let dir = tmp_dir("extract_to_vacia");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let base = serve_paginas(vec![("/p1".to_string(), pagina_de(1, 4))]);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open({{ wait: 1500 }}) {{
    p = web.page(b)
    r = web.extract_to(p, ["{base}/p1", "{base}/no-existe"], ".card",
                       {{ nombre: ".title" }}, "s.csv")
    show("RES=" + str(r))
}}
"##));

    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("rows: 4"), "no extrajo la página buena:\n{salida}");
    assert!(salida.contains("no-existe"),
            "la página sin filas no aparece en el resumen:\n{salida}");
}

#[test]
fn extract_to_en_odf_escribe_por_bloques_y_lo_lee_el_motor_de_datos() {
    // El .odf lleva el número de filas en la cabecera, así que se vuelca por
    // bloques liberando cada uno: es lo que mantiene la memoria acotada.
    let dir = tmp_dir("extract_to_odf");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let base = serve_paginas((1..=3).map(|n| (format!("/p{n}"), pagina_de(n, 5))).collect());

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
use "frame" as fr
esquema = {{ id: "@data-id", nombre: ".title", precio: ".price|num" }}
urls = ["{base}/p1", "{base}/p2", "{base}/p3"]
with b = web.open() {{
    p = web.page(b)
    r = web.extract_to(p, urls, ".card", esquema, "d.odf", {{ chunk: 6 }})
    show("FILES=" + str(r["files"]))
}}
h = fr.open("d.odf")
show("SIZE=" + str(fr.size(h)))
show("SCHEMA=" + str(fr.schema(h)))
"##));

    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("d.odf") && salida.contains("d_2.odf"),
            "no se partió en bloques:\n{salida}");
    // Lo escrito por el scraper tiene que poder leerlo el motor de datos, con
    // los tipos ya inferidos: es lo que encadena scraping con análisis.
    assert!(salida.contains("rows: 6"), "el primer bloque no tiene 6 filas:\n{salida}");
    assert!(salida.contains("precio: float"), "no infirió el tipo numérico:\n{salida}");
}

#[test]
fn una_extension_no_soportada_dice_cuales_valen() {
    let dir = tmp_dir("extract_to_ext");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let base = serve_paginas(vec![("/p1".to_string(), pagina_de(1, 2))]);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    attempt {{
        web.extract_to(p, ["{base}/p1"], ".card", {{ n: ".title" }}, "datos.xlsx")
    }} handle e {{ show("E=" + e) }}
}}
"##));
    assert!(salida.contains("E="), "debería fallar con una extensión no soportada:\n{salida}");
    assert!(salida.contains(".csv") && salida.contains(".odf"),
            "el error no dice qué formatos valen:\n{salida}");
}

//    Archivos: las tres ventanas del sistema operativo
//
// Lo que se comprueba aquí no es que se escriba un archivo, es que **no aparece
// ninguna ventana nativa**. Si apareciera, el test se colgaría hasta el plazo:
// no hay nadie que pueda cerrarla, y esa es exactamente la situación en la que
// se queda un scraper de Selenium en un servidor sin escritorio.

/// Página con los dos casos de subida que existen en la vida real.
const PAGINA_ARCHIVOS: &str = r#"<!doctype html>
<html><head><title>Archivos</title></head><body>
  <!-- Caso 1: el campo es alcanzable -->
  <input type="file" id="visible">
  <div id="v1">nada</div>

  <!-- Caso 2: el campo está oculto y se abre desde un botón. Es lo que hacen
       todos los sitios con un diseño propio, y lo que no cubre send_keys. -->
  <input type="file" id="oculto" style="display:none" multiple>
  <button id="examinar">Adjuntar documento</button>
  <div id="v2">nada</div>

  <a id="baja" href="/descarga">Descargar informe</a>
<script>
  const pinta = (inp, salida) => inp.addEventListener('change', () => {
    document.getElementById(salida).textContent =
      Array.from(inp.files).map(f => f.name + ':' + f.size).join(',');
  });
  pinta(document.getElementById('visible'), 'v1');
  pinta(document.getElementById('oculto'), 'v2');
  document.getElementById('examinar').onclick = () =>
    document.getElementById('oculto').click();
</script>
</body></html>"#;

#[test]
fn sube_un_archivo_al_campo_directamente() {
    let dir = tmp_dir("upload_directo");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_rutas(PAGINA_ARCHIVOS, b"contenido", "informe.txt");
    fs::write(dir.join("carta.txt"), b"hola mundo").unwrap();

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.upload(p, "#visible", "carta.txt")
    show("V1=" + web.text(p, "#v1"))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    // El evento `change` del sitio confirma que el archivo llegó a la página
    // con su tamaño real, no que la llamada devolviera sin quejarse.
    assert!(salida.contains("V1=carta.txt:10"),
            "la página no recibió el archivo:\n{salida}");
}

#[test]
fn sube_por_el_boton_cuando_el_campo_esta_oculto() {
    let dir = tmp_dir("upload_boton");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_rutas(PAGINA_ARCHIVOS, b"contenido", "informe.txt");
    fs::write(dir.join("a.txt"), b"aaa").unwrap();
    fs::write(dir.join("b.txt"), b"bbbb").unwrap();

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.upload(p, "#examinar", ["a.txt", "b.txt"])
    show("V2=" + web.text(p, "#v2"))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("V2=a.txt:3,b.txt:4"),
            "no se adjuntaron los dos archivos por el botón:\n{salida}");
}

#[test]
fn un_archivo_que_no_existe_se_dice_antes_de_tocar_la_pagina() {
    let dir = tmp_dir("upload_inexistente");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_rutas(PAGINA_ARCHIVOS, b"x", "informe.txt");

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{ web.upload(p, "#visible", "no_esta.txt") }}
    handle e {{ show("E=" + e) }}
    -- La página tiene que haber quedado intacta: si se hubiera pulsado, el
    -- diálogo del sistema estaría abierto y lo siguiente se colgaría.
    show("V1=" + web.text(p, "#v1"))
}}
"##));
    assert!(salida.contains("E=") && salida.contains("no existe"),
            "debería decir que el archivo no existe:\n{salida}");
    assert!(salida.contains("se buscó en:"),
            "el error no dice dónde buscó, que es lo único que resuelve el caso:\n{salida}");
    assert!(salida.contains("V1=nada"), "la página no quedó intacta:\n{salida}");
}

#[test]
fn descarga_un_archivo_y_espera_a_que_termine() {
    let dir = tmp_dir("descarga");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_rutas(PAGINA_ARCHIVOS, b"REPORTE-2026", "informe.txt");

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    d = web.download(p, "#baja", {{ dir: "bajadas" }})
    show("NAME=" + d["name"])
    show("BYTES=" + str(d["bytes"]))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("NAME=informe.txt"), "nombre incorrecto:\n{salida}");
    // El tamaño exacto prueba lo que de verdad importa: se volvió cuando el
    // archivo estaba entero, no mientras aún era un .crdownload.
    assert!(salida.contains("BYTES=12"), "no esperó al final de la descarga:\n{salida}");
    let f = dir.join("bajadas").join("informe.txt");
    assert!(f.is_file(), "no está el archivo descargado:\n{salida}");
    assert_eq!(fs::read(&f).unwrap(), b"REPORTE-2026", "el contenido no coincide");
}

#[test]
fn dos_descargas_iguales_no_se_pisan() {
    let dir = tmp_dir("descarga_doble");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_rutas(PAGINA_ARCHIVOS, b"REPORTE-2026", "informe.txt");

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    a = web.download(p, "#baja", {{ dir: "bajadas" }})
    c = web.download(p, "#baja", {{ dir: "bajadas" }})
    show("A=" + a["name"])
    show("C=" + c["name"])
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("A=informe.txt"), "{salida}");
    // Sobrescribir en silencio es lo que hace perder una tanda entera de
    // facturas sin que nadie se entere hasta el cierre del mes.
    assert!(salida.contains("C=informe (2).txt"),
            "la segunda descarga pisó a la primera:\n{salida}");
}

#[test]
fn renombra_la_descarga_si_se_le_pide() {
    let dir = tmp_dir("descarga_nombre");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_rutas(PAGINA_ARCHIVOS, b"12345", "informe.txt");

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    d = web.download(p, "#baja", {{ dir: ".", name: "factura-042.txt" }})
    show("P=" + d["path"])
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(dir.join("factura-042.txt").is_file(), "no se renombró:\n{salida}");
    assert!(salida.contains("P=") && salida.contains("factura-042.txt"),
            "la ruta devuelta no es la real:\n{salida}");
}

#[test]
fn un_elemento_que_no_descarga_lo_dice_en_vez_de_colgarse() {
    let dir = tmp_dir("descarga_falsa");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_rutas(PAGINA_ARCHIVOS, b"x", "informe.txt");

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{ web.download(p, "#examinar", {{ wait: 2000 }}) }}
    handle e {{ show("E=" + e) }}
}}
"##));
    assert!(salida.contains("E=") && salida.contains("no inició ninguna descarga"),
            "debería explicar que ese elemento no descarga:\n{salida}");
}

#[test]
fn imprime_la_pagina_a_pdf_sin_dialogo() {
    let dir = tmp_dir("pdf");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.pdf(p, "salida.pdf", {{ landscape: yes, margin: 0.4 }})
}}
"##));
    assert!(ok, "falló:\n{salida}");
    let f = dir.join("salida.pdf");
    assert!(f.is_file(), "no se escribió el PDF:\n{salida}");
    let bytes = fs::read(&f).unwrap();
    assert!(bytes.len() > 500, "el PDF está vacío ({} bytes)", bytes.len());
    assert_eq!(&bytes[..5], b"%PDF-", "no es un PDF válido");
}
