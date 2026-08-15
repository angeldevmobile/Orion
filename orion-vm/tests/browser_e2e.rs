//! Módulo `browser` de punta a punta: arrancar Chromium, hablar CDP y limpiar.
//!
//! Las páginas se sirven desde un servidor local propio, no desde internet: un
//! test que depende de la red falla por motivos que no son el código, y un test
//! que falla por motivos ajenos deja de mirarse.
//!
//! Si la máquina no tiene ningún navegador basado en Chromium, los tests se
//! saltan en vez de fallar — no tenerlo instalado no es un defecto de Orion.
//!
//! Ejecutar EN SERIE: `cargo test --test browser_e2e -- --test-threads=1`.
//!
//! Estos tests son INESTABLES bajo carga, y conviene saberlo antes de perder una
//! tarde persiguiendo un fallo que no existe:
//!
//! - En paralelo caen ~2 al azar por pasada, en ~195 s.
//! - En serie caen menos y va más rápido (~145 s), pero **también caen**: se ha
//!   visto una pasada limpia de 74/74 y otra con 2 fallos.
//! - Los que caen NUNCA son los mismos, y cada uno aislado pasa en 1-3 s.
//!
//! Que cambien de una pasada a otra es la prueba de que es contención de
//! recursos —cada test levanta un navegador de verdad y un servidor local— y no
//! un defecto del módulo. La regla práctica: ante un rojo aquí, repetir el test
//! solo; si pasa, era esto. No des por bueno un verde en serie como garantía.

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
/// ¿Hay un navegador Chromium? La respuesta es la misma para toda la suite, así
/// que se calcula una vez en vez de levantar 74 procesos `orion` para lo mismo.
fn hay_navegador(dir: &PathBuf) -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        let (salida, _) = run_orion(dir, "use \"browser\" as web\nshow(web.info())\n");
        salida.contains("found: yes")
    })
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

/// Página que se va a otra al pulsar: el caso que rompe una lectura posterior.
///
/// La navegación se retrasa a propósito. Sin retraso el documento nuevo llega
/// antes de la siguiente instrucción y el test pasaría siempre, incluso con el
/// fallo puesto — que es exactamente por lo que este fallo sobrevive tanto
/// tiempo en un scraper: en local no se reproduce.
const PAGINA_QUE_NAVEGA: &str = r#"<!doctype html>
<html><head><title>Origen</title></head><body>
<button id="enviar">Enviar</button>
<script>
  document.getElementById('enviar').onclick = () => {
    setTimeout(() => { location.href = '/destino'; }, 250);
  };
</script>
</body></html>"#;

#[test]
fn una_lectura_justo_despues_de_navegar_no_se_pierde() {
    let dir = tmp_dir("nav_en_curso");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_rutas(PAGINA_QUE_NAVEGA, b"x", "f.txt");

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.click(p, "#enviar")
    -- Sin reintento esto revienta con "Inspected target navigated or closed":
    -- el documento viejo ya no está y el nuevo aún no ha llegado.
    show("T=" + web.title(p))
}}
"##));
    assert!(ok, "una lectura tras navegar no debería fallar:\n{salida}");
    assert!(salida.contains("T="), "no se leyó nada:\n{salida}");
    assert!(!salida.contains("navigated or closed"),
            "asomó el error crudo de CDP:\n{salida}");
}

//    Formularios y tablas

/// Formulario con los cuatro tipos de control y un rastreador estilo React.
///
/// El rastreador no es decorado: es lo que separa "el campo se ve relleno" de
/// "la aplicación se ha enterado". React instala un descriptor sobre `value` y
/// descarta el evento si el valor coincide con el que él anotó, así que una
/// asignación directa deja el formulario visualmente correcto y funcionalmente
/// vacío. Sin esto en la página, un `fill` roto pasaría el test.
const PAGINA_FORM: &str = r#"<!doctype html>
<html><head><title>Formulario</title></head><body>
<form id="f">
  <input id="nombre">
  <textarea id="notas"></textarea>
  <select id="pais">
    <option value="es">España</option>
    <option value="pt">Portugal</option>
  </select>
  <input type="checkbox" id="acepto">
  <input type="radio" name="plan" id="plan_a" value="a">
  <input type="radio" name="plan" id="plan_b" value="b">
  <div id="bio" contenteditable="true"></div>
</form>
<div id="app">-</div>
<div id="eventos">0</div>
<script>
  var desc = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value');
  var el = document.getElementById('nombre');
  var ultimo = el.value;
  Object.defineProperty(el, 'value', {
    configurable: true,
    get: function () { return desc.get.call(this); },
    set: function (v) { ultimo = String(v); desc.set.call(this, v); }
  });
  el.addEventListener('input', function () {
    var actual = desc.get.call(el);
    if (actual === ultimo) return;          // React: "ya lo sabía", no propaga
    ultimo = actual;
    document.getElementById('app').textContent = 'app:' + actual;
  });
  document.getElementById('f').addEventListener('change', function () {
    var n = document.getElementById('eventos');
    n.textContent = String(parseInt(n.textContent, 10) + 1);
  });
</script>
</body></html>"#;

#[test]
fn rellena_un_formulario_entero_de_una_vez() {
    let dir = tmp_dir("fill");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_FORM);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    n = web.fill(p, {{
        "#nombre":  "Ana Torres",
        "#notas":   "dos lineas",
        "#pais":    "Portugal",
        "#acepto":  yes,
        "#plan_b":  yes,
        "#bio":     "editable"
    }})
    show("N=" + str(n))
    show("NOMBRE=" + web.value(p, "#nombre"))
    show("NOTAS=" + web.eval(p, "document.querySelector('#notas').value"))
    show("PAIS=" + web.eval(p, "document.querySelector('#pais').value"))
    show("ACEPTO=" + str(web.eval(p, "document.querySelector('#acepto').checked")))
    show("PLAN=" + str(web.eval(p, "document.querySelector('#plan_b').checked")))
    show("BIO=" + web.text(p, "#bio"))
    -- attr lee el ATRIBUTO del HTML, que no cambia al escribir: es la
    -- confusión que hace creer que el fill no funcionó.
    show("ATTR=" + str(web.attr(p, "#nombre", "value")))
    show("SEL=" + str(web.value(p, "#pais")))
    show("CHK=" + str(web.value(p, "#acepto")))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("N=6"), "no rellenó los seis campos:\n{salida}");
    assert!(salida.contains("NOMBRE=Ana Torres"), "{salida}");
    assert!(salida.contains("ATTR=null"),
            "attr('value') debe seguir leyendo el atributo del HTML:
{salida}");
    assert!(salida.contains("SEL=pt"), "{salida}");
    assert!(salida.contains("CHK=yes"), "{salida}");
    assert!(salida.contains("NOTAS=dos lineas"), "el textarea usa otro prototipo:\n{salida}");
    // El desplegable se pidió por texto visible y el value es el código.
    assert!(salida.contains("PAIS=pt"), "no eligió por texto visible:\n{salida}");
    assert!(salida.contains("ACEPTO=yes"), "{salida}");
    assert!(salida.contains("PLAN=yes"), "{salida}");
    assert!(salida.contains("BIO=editable"), "{salida}");
}

#[test]
fn el_valor_llega_a_la_aplicacion_no_solo_a_la_pantalla() {
    let dir = tmp_dir("fill_react");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_FORM);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.fill(p, {{ "#nombre": "Ana Torres" }})
    show("APP=" + web.text(p, "#app"))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    // Con `el.value = x` esto seguiría diciendo "-": el campo se vería relleno
    // y el formulario se enviaría vacío.
    assert!(salida.contains("APP=app:Ana Torres"),
            "el valor no llegó a la aplicación, solo a la pantalla:\n{salida}");
}

#[test]
fn un_campo_que_no_existe_no_se_traga_en_silencio() {
    let dir = tmp_dir("fill_estricto");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_FORM);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{
        web.fill(p, {{ "#nombre": "Ana", "#telefono_viejo": "600" }}, {{ wait: 800 }})
    }} handle e {{ show("E=" + e) }}
    -- Y con strict: no se acepta lo que sí existe.
    n = web.fill(p, {{ "#nombre": "Ana", "#telefono_viejo": "600" }},
                 {{ strict: no, wait: 800 }})
    show("N=" + str(n))
}}
"##));
    assert!(salida.contains("E=") && salida.contains("#telefono_viejo"),
            "debería delatar el selector que no existe:\n{salida}");
    assert!(salida.contains("N=1"), "con strict: no debería rellenar el que sí está:\n{salida}");
}

#[test]
fn una_opcion_inexistente_de_un_select_lista_las_que_hay() {
    let dir = tmp_dir("fill_select");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_FORM);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{ web.fill(p, {{ "#pais": "Marte" }}) }}
    handle e {{ show("E=" + e) }}
}}
"##));
    assert!(salida.contains("E="), "{salida}");
    assert!(salida.contains("España") && salida.contains("Portugal"),
            "el error no dice qué opciones hay:\n{salida}");
}

#[test]
fn fill_con_teclas_reales_dispara_los_eventos_de_teclado() {
    let dir = tmp_dir("fill_teclas");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_VIVA);

    // La página VIVA cuenta pulsaciones: es la prueba de que `{ keys: yes }` no
    // se limita a asignar el valor.
    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.fill(p, {{ "#q": "hola" }}, {{ keys: yes }})
    web.click(p, "#ir")
    show("S=" + web.text(p, "#salida"))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("v=hola"), "no escribió el texto:\n{salida}");
    assert!(!salida.contains("teclas=0"),
            "con keys: yes tienen que llegar pulsaciones reales:\n{salida}");
}

#[test]
fn marcar_una_casilla_es_idempotente() {
    let dir = tmp_dir("check");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_FORM);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.check(p, "#acepto")
    web.check(p, "#acepto")
    web.check(p, "#acepto")
    show("A=" + str(web.eval(p, "document.querySelector('#acepto').checked")))
    web.uncheck(p, "#acepto")
    show("B=" + str(web.eval(p, "document.querySelector('#acepto').checked")))
    attempt {{ web.uncheck(p, "#plan_a") }} handle e {{ show("E=" + e) }}
    attempt {{ web.check(p, "#nombre") }} handle e2 {{ show("F=" + e2) }}
}}
"##));
    assert!(ok, "falló:\n{salida}");
    // Tres check seguidos la dejan marcada: si se limitara a pulsar, el tercero
    // la habría desmarcado.
    assert!(salida.contains("A=yes"), "check no es idempotente:\n{salida}");
    assert!(salida.contains("B=no"), "uncheck no la desmarcó:\n{salida}");
    assert!(salida.contains("E=") && salida.contains("radio"),
            "desmarcar un radio debería explicarse:\n{salida}");
    assert!(salida.contains("F=") && salida.contains("no es una casilla"),
            "check sobre un campo de texto debería explicarse:\n{salida}");
}

/// Tabla como las de verdad: SIN `<thead>`, con `<th>` de fila en el cuerpo,
/// con celdas combinadas y con otra tabla dentro.
///
/// Las cuatro cosas salieron de mirar tablas reales, no de imaginarlas: de 13
/// tablas en tres páginas de Wikipedia, ninguna tenía `<thead>`, diez llevaban
/// `<th>` en el cuerpo, cuatro usaban colspan/rowspan y una estaba anidada.
const PAGINA_TABLA: &str = r#"<!doctype html>
<html><head><title>Tablas</title></head><body>
<table id="real">
  <tr><th>Pais</th><th>Capital</th><th>PIB</th></tr>
  <tr><th>España</th><td>Madrid</td><td>1.400</td></tr>
  <tr><th>Portugal</th><td>Lisboa</td><td>250</td></tr>
</table>

<table id="combinada">
  <tr><th>Zona</th><th>Q1</th><th>Q2</th></tr>
  <tr><td rowspan="2">Norte</td><td>10</td><td>20</td></tr>
  <tr><td>30</td><td>40</td></tr>
  <tr><td>Sur</td><td colspan="2">sin datos</td></tr>
</table>

<table id="padre">
  <tr><th>A</th><th>B</th></tr>
  <tr><td>1</td><td>
    <table id="hija"><tr><td>oculto</td></tr></table>
  </td></tr>
</table>

<table id="conthead">
  <thead><tr><th>X</th><th>Y</th></tr></thead>
  <tbody><tr><td>7</td><td>8</td></tr></tbody>
</table>

<table id="repes">
  <tr><th>n</th><th>n</th><th></th><th>PIB<br>(2026)</th></tr>
  <tr><td>a</td><td>b</td><td>c</td><td>x</td></tr>
</table>

<div id="nodable">no soy una tabla</div>
</body></html>"#;

#[test]
fn lee_una_tabla_sin_thead_con_th_en_el_cuerpo() {
    let dir = tmp_dir("tabla_real");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_TABLA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    f = web.table(p, "#real")
    show("N=" + str(len(f)))
    show("R0=" + str(f[0]))
    show("R1=" + str(f[1]))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    // Dos filas de datos: la primera es la cabecera y las que empiezan por <th>
    // NO son cabeceras, son encabezados de fila.
    assert!(salida.contains("N=2"), "confundió la cabecera con los datos:\n{salida}");
    assert!(salida.contains("Pais: España") && salida.contains("Capital: Madrid"),
            "no nombró las columnas con la primera fila:\n{salida}");
    assert!(salida.contains("Portugal"), "{salida}");
}

#[test]
fn expande_las_celdas_combinadas() {
    let dir = tmp_dir("tabla_span");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_TABLA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    f = web.table(p, "#combinada")
    show("N=" + str(len(f)))
    show("R0=" + str(f[0]))
    show("R1=" + str(f[1]))
    show("R2=" + str(f[2]))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("N=3"), "{salida}");
    // Sin expandir el rowspan, la segunda fila tendría 30 en la columna Zona y
    // todo lo demás correría un puesto a la izquierda.
    assert!(salida.contains("R1={Zona: Norte, Q1: 30, Q2: 40}"),
            "el rowspan no se propagó a la fila siguiente:\n{salida}");
    assert!(salida.contains("R2={Zona: Sur, Q1: sin datos, Q2: sin datos}"),
            "el colspan no se repartió:\n{salida}");
}

#[test]
fn una_tabla_anidada_no_cuela_sus_filas() {
    let dir = tmp_dir("tabla_anidada");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_TABLA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    f = web.table(p, "#padre")
    show("N=" + str(len(f)))
    show("R0=" + str(f[0]))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("N=1"), "se colaron las filas de la tabla de dentro:\n{salida}");
}

#[test]
fn thead_columnas_repetidas_y_lo_que_no_es_tabla() {
    let dir = tmp_dir("tabla_varios");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_TABLA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    show("T=" + str(web.table(p, "#conthead")))
    -- Dos columnas con el mismo nombre y una sin nombre: si se dejaran igual,
    -- una clave pisaría a la otra y la vacía sería impedible de pedir.
    show("R=" + str(web.table(p, "#repes")[0]))
    -- Sin cabecera, todo son datos.
    show("S=" + str(len(web.table(p, "#real", {{ header: no }}))))
    attempt {{ web.table(p, "#nodable") }} handle e {{ show("E=" + e) }}
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("T=[{X: 7, Y: 8}]"), "el thead no se usó como cabecera:\n{salida}");
    assert!(salida.contains("n: a") && salida.contains("n_2: b") && salida.contains("col_3: c"),
            "no desambiguó las columnas repetidas o vacías:\n{salida}");
    // Una cabecera con un <br> dentro —en Wikipedia son casi todas— daría una
    // clave con un salto de línea, y esa clave no hay quien la escriba para
    // pedir la columna.
    assert!(salida.contains("PIB (2026): x"),
            "no colapsó el salto de línea del nombre de columna:\n{salida}");
    assert!(salida.contains("S=3"), "con header: no deberían salir las tres filas:\n{salida}");
    assert!(salida.contains("E=") && salida.contains("no es una <table>"),
            "debería decir que eso no es una tabla:\n{salida}");
}

#[test]
fn un_campo_de_varios_valores_se_recoge_entero() {
    let dir = tmp_dir("extract_list");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_LISTA);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    r = web.extract(p, ".card", {{
        titulo:  ".t",
        tags:    ".tag|list",
        precios: ".p|list:num",
        urls:    "a@href|list"
    }})
    show("R0=" + str(r[0]))
    show("R1=" + str(r[1]))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    // Antes devolvía solo "rojo" y las demás se perdían sin decir nada.
    assert!(salida.contains("tags: [rojo, azul, verde]"),
            "no recogió todas las etiquetas:\n{salida}");
    assert!(salida.contains("precios: [10.5, 3]"),
            "la conversión detrás de list no se aplicó:\n{salida}");
    assert!(salida.contains("urls: [/a, /b]"), "no recogió los atributos:\n{salida}");
    assert!(salida.contains("tags: []"), "la segunda tarjeta no tiene etiquetas:\n{salida}");
}

#[test]
fn un_list_con_el_selector_equivocado_se_delata() {
    let dir = tmp_dir("extract_list_muerto");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_LISTA);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{ web.extract(p, ".card", {{ t: ".t", tags: ".etiqueta-vieja|list" }}) }}
    handle e {{ show("E=" + e) }}
}}
"##));
    // Una lista vacía en TODAS las filas es un selector muerto, no un dato que
    // falta: sin esto el aviso se perdía justo donde más sirve.
    assert!(salida.contains("E=") && salida.contains("etiqueta-vieja"),
            "un list muerto debería delatarse igual que un campo simple:\n{salida}");
}

const PAGINA_LISTA: &str = r#"<!doctype html>
<html><head><title>Lista</title></head><body>
<div class="card">
  <div class="t">Primero</div>
  <span class="tag">rojo</span><span class="tag">azul</span><span class="tag">verde</span>
  <span class="p">10,50 €</span><span class="p">3 €</span>
  <a href="/a">uno</a><a href="/b">dos</a>
</div>
<div class="card">
  <div class="t">Segundo</div>
  <a href="/c">tres</a>
</div>
</body></html>"#;

//    Estabilidad y sesión

/// Página que guarda algo en localStorage y muestra una cookie.
const PAGINA_SESION: &str = r#"<!doctype html>
<html><head><title>Sesion</title></head><body>
<div id="quien">anonimo</div>
<div id="ruta">-</div>
<script>
  document.getElementById('ruta').textContent = location.pathname;
  const t = localStorage.getItem('token');
  if (t) document.getElementById('quien').textContent = 'sesion:' + t;
</script>
</body></html>"#;

#[test]
fn la_sesion_se_guarda_y_se_restaura() {
    let dir = tmp_dir("estado");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_SESION);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    -- Se "inicia sesión": una cookie y una marca en el almacenamiento local.
    web.eval(p, "document.cookie = 'sid=abc123; path=/'; localStorage.setItem('token', 'XYZ'); true")
    g = web.save_state(p, "sesion.json")
    show("G=" + str(g["local"]) + "/" + str(g["cookies"] > 0))
}}

-- Navegador NUEVO: sin nada compartido con el anterior.
with b2 = web.open() {{
    p2 = web.page(b2)
    web.goto(p2, "{url}")
    show("ANTES=" + web.text(p2, "#quien"))
    c = web.load_state(p2, "sesion.json")
    show("C=" + str(c["cookies"] > 0) + "/" + str(c["local"]))
    web.reload(p2)
    show("DESPUES=" + web.text(p2, "#quien"))
    show("COOKIE=" + web.eval(p2, "document.cookie"))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("G=1/yes"), "no se guardó el estado:\n{salida}");
    assert!(salida.contains("ANTES=anonimo"), "el navegador nuevo no estaba limpio:\n{salida}");
    assert!(salida.contains("C=yes/1"), "no se restauró:\n{salida}");
    // Esto es lo que ahorra el login diario: la página ve la sesión de la
    // ejecución anterior sin que nadie haya vuelto a escribir la contraseña.
    assert!(salida.contains("DESPUES=sesion:XYZ"),
            "el almacenamiento no sobrevivió al viaje:\n{salida}");
    assert!(salida.contains("sid=abc123"), "la cookie no se restauró:\n{salida}");
}

#[test]
fn cargar_un_estado_en_otro_origen_lo_dice() {
    let dir = tmp_dir("estado_origen");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let a = serve_html(PAGINA_SESION);
    let b = serve_html(PAGINA_SESION);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with nav = web.open() {{
    p = web.page(nav)
    web.goto(p, "{a}")
    web.eval(p, "localStorage.setItem('token', 'XYZ'); true")
    web.save_state(p, "s.json")

    -- Otro puerto es otro origen: el navegador no deja escribir su
    -- almacenamiento desde aquí, y callarlo dejaría una sesión a medias.
    web.goto(p, "{b}")
    c = web.load_state(p, "s.json")
    show("SKIP=" + str(len(c["skipped"])))
    show("LOCAL=" + str(c["local"]))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("SKIP=1"), "no avisó del origen que no aplicó:\n{salida}");
    assert!(salida.contains("LOCAL=0"), "no debería haber aplicado nada:\n{salida}");
}

#[test]
fn un_estado_que_no_existe_lo_explica() {
    let dir = tmp_dir("estado_ausente");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_SESION);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{ web.load_state(p, "no_esta.json") }} handle e {{ show("E=" + e) }}
}}
"##));
    assert!(salida.contains("E=") && salida.contains("save_state"),
            "el error debería apuntar a cómo se crea el archivo:\n{salida}");
}

#[test]
fn atras_y_adelante_recorren_el_historial() {
    let dir = tmp_dir("historial");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_SESION);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}uno")
    web.goto(p, "{url}dos")
    show("A=" + web.text(p, "#ruta"))
    web.back(p)
    show("B=" + web.text(p, "#ruta"))
    web.forward(p)
    show("C=" + web.text(p, "#ruta"))
    web.reload(p)
    show("D=" + web.text(p, "#ruta"))

    -- Una pestaña recién abierta no tiene a dónde volver: tiene que decirlo en
    -- vez de callarse. (En la de arriba SÍ queda historial: toda pestaña
    -- empieza en about:blank y esa cuenta como página anterior.)
    q = web.page(b)
    attempt {{ web.back(q) }} handle e {{ show("E=" + e) }}
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("A=/dos"), "{salida}");
    assert!(salida.contains("B=/uno"), "back no volvió:\n{salida}");
    assert!(salida.contains("C=/dos"), "forward no avanzó:\n{salida}");
    assert!(salida.contains("D=/dos"), "reload cambió de página:\n{salida}");
    assert!(salida.contains("E=") && salida.contains("historial"),
            "un back sin historial debería explicarse:\n{salida}");
}

/// Página que trae datos por fetch con retraso, sin nada que anuncie el final.
const PAGINA_RED: &str = r#"<!doctype html>
<html><head><title>Red</title></head><body>
<button id="pide">Pedir</button>
<div id="n">0</div>
<script>
  let n = 0;
  document.getElementById('pide').onclick = () => {
    for (let i = 0; i < 3; i++) {
      setTimeout(() => {
        fetch('/dato?i=' + i).then(() => {
          n++;
          document.getElementById('n').textContent = String(n);
        });
      }, i * 120);
    }
  };
</script>
</body></html>"#;

#[test]
fn esperar_a_que_la_red_se_calme() {
    let dir = tmp_dir("red_idle");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_RED);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.click(p, "#pide")
    -- No hay ningún selector que esperar: no se sabe QUÉ va a aparecer, solo
    -- que la página sigue trayendo cosas.
    web.wait(p, {{ idle: 300 }})
    show("N=" + web.text(p, "#n"))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("N=3"),
            "volvió antes de que terminaran las tres peticiones:\n{salida}");
}

#[test]
fn la_lista_blanca_corta_lo_que_no_esta_en_ella() {
    let dir = tmp_dir("allow");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_SESION);

    // 127.0.0.1 está permitido; cualquier otro dominio no. Un proceso
    // automático lleva encima la sesión de la empresa: si una página
    // comprometida lo redirige, va con ella puesta.
    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open({{ allow: ["127.0.0.1"] }}) {{
    p = web.page(b)
    web.goto(p, "{url}")
    show("OK=" + web.text(p, "#quien"))
    attempt {{ web.goto(p, "https://example.com/") }} handle e {{ show("E=1") }}
    show("BLOQ=" + str(len(web.blocked(b)) > 0))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("OK=anonimo"), "bloqueó el dominio permitido:\n{salida}");
    assert!(salida.contains("BLOQ=yes"), "no registró ningún bloqueo:\n{salida}");
}

#[test]
fn una_lista_blanca_vacia_no_se_acepta() {
    let dir = tmp_dir("allow_vacia");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();

    let (salida, _) = run_orion(&dir, r##"
use "browser" as web
attempt {
    b = web.open({ allow: [] })
    web.free(b)
} handle e { show("E=" + e) }
"##);
    // Una lista vacía bloquearía absolutamente todo: es un descuido, no una
    // política, y aceptarlo en silencio daría un navegador inútil sin motivo.
    assert!(salida.contains("E=") && salida.contains("allow"),
            "debería rechazar la lista vacía:\n{salida}");
}

#[test]
fn un_campo_secreto_no_aparece_en_el_error() {
    let dir = tmp_dir("secreto");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_FORM);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    -- Un <select> delata el valor que no admitió, y ese error acaba en un log.
    attempt {{
        web.fill(p, {{ "#pais": "clave-secreta-real" }}, {{ secret: ["#pais"] }})
    }} handle e {{ show("E=" + e) }}
}}
"##));
    assert!(salida.contains("E="), "{salida}");
    assert!(!salida.contains("clave-secreta-real"),
            "el valor secreto se filtró al error:\n{salida}");
    assert!(salida.contains("oculto"), "debería decir que lo ocultó:\n{salida}");
}

//    Captura de red

/// Sirve una página que pinta su listado desde su propia API.
///
/// El JSON trae campos que la página **no** llega a pintar (`stock`), que es
/// justo lo que hace útil capturar la fuente en vez de deshacer el HTML.
fn serve_api(pagina: &'static str, json: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir puerto");
    let port = listener.local_addr().unwrap().port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let peticion = String::from_utf8_lossy(&buf[..n]).to_string();
            let ruta = peticion.split_whitespace().nth(1).unwrap_or("/").to_string();

            let (tipo, cuerpo) = if ruta.starts_with("/api/") {
                ("application/json", json)
            } else {
                ("text/html; charset=utf-8", pagina)
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{}",
                tipo, cuerpo.len(), cuerpo
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });

    format!("http://127.0.0.1:{port}/")
}

const PAGINA_API: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Catalogo</title></head><body>
<button id="cargar">Cargar</button>
<div id="lista">nada</div>
<script>
document.getElementById('cargar').onclick = async () => {
  const r = await fetch('/api/productos?pagina=1');
  const d = await r.json();
  // Se pinta SOLO el nombre: el stock viaja en el JSON y no llega al HTML.
  document.getElementById('lista').innerHTML =
    d.items.map(p => '<div class="card"><span class="t">' + p.nombre + '</span></div>').join('');
};
</script>
</body></html>"#;

const JSON_API: &str =
    r#"{"pagina":1,"total":2,"items":[{"id":1,"nombre":"Teclado","stock":12},{"id":2,"nombre":"Monitor","stock":3}]}"#;

#[test]
fn captura_el_json_que_la_pagina_le_pide_a_su_api() {
    let dir = tmp_dir("capture");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_api(PAGINA_API, JSON_API);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.watch(p, "/api/")
    web.click(p, "#cargar")
    r = web.capture(p)
    show("N=" + str(len(r)))
    show("STATUS=" + str(r[0]["status"]))
    d = r[0]["json"]
    show("TOTAL=" + str(d["total"]))
    -- El stock NO está en el HTML; solo se puede saber leyendo la fuente.
    show("STOCK=" + str(d["items"][0]["stock"]))
    show("HTML=" + str(web.texts(p, ".t")))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("N=1"), "no capturó la respuesta:\n{salida}");
    assert!(salida.contains("STATUS=200"), "{salida}");
    assert!(salida.contains("TOTAL=2"), "el JSON no se parseó:\n{salida}");
    assert!(salida.contains("STOCK=12"),
            "no llegó un campo que la página no pinta:\n{salida}");
    assert!(salida.contains("HTML=[Teclado, Monitor]"),
            "la página debería mostrar solo los nombres:\n{salida}");
}

#[test]
fn capturar_sin_armar_la_escucha_lo_dice() {
    let dir = tmp_dir("capture_sin_watch");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_api(PAGINA_API, JSON_API);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{ web.capture(p) }} handle e {{ show("E=" + e) }}
}}
"##));
    // Armar después de provocar la petición no capturaría nada, así que el
    // error tiene que decir el orden y no solo que falta algo.
    assert!(salida.contains("E=") && salida.contains("watch"),
            "el error debería explicar que hay que armar antes:\n{salida}");
}

#[test]
fn un_patron_que_no_casa_devuelve_vacio_sin_colgarse() {
    let dir = tmp_dir("capture_sin_casar");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_api(PAGINA_API, JSON_API);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.watch(p, "/inventado/")
    web.click(p, "#cargar")
    r = web.capture(p, {{ wait: 1500 }})
    show("N=" + str(len(r)))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("N=0"), "debería devolver vacío:\n{salida}");
}

#[test]
fn el_comodin_afina_el_patron() {
    let dir = tmp_dir("capture_comodin");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_api(PAGINA_API, JSON_API);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    web.watch(p, "*/api/productos?*")
    web.click(p, "#cargar")
    r = web.capture(p)
    show("N=" + str(len(r)))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("N=1"), "el comodín no casó:\n{salida}");
}

//    Descubrimiento de estructura

/// Un listado de tarjetas con la estructura tipica: titulo, precio y enlace,
/// repetido, mas ruido alrededor (cabecera y menu) que NO debe confundirse con
/// el listado.
const PAGINA_LISTADO: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Tienda</title></head><body>
<nav><a href="/a">Inicio</a><a href="/b">Ofertas</a></nav>
<header><h1>Catalogo</h1></header>
<div id="lista">
  <div class="card"><span class="titulo">Teclado</span><span class="precio">49,90</span><a href="/p/1">ver</a></div>
  <div class="card"><span class="titulo">Monitor</span><span class="precio">219,00</span><a href="/p/2">ver</a></div>
  <div class="card"><span class="titulo">Raton</span><span class="precio">24,50</span><a href="/p/3">ver</a></div>
  <div class="card"><span class="titulo">Webcam</span><span class="precio">59,00</span><a href="/p/4">ver</a></div>
</div>
</body></html>"#;

#[test]
fn discover_deduce_la_fila_y_los_campos() {
    let dir = tmp_dir("discover");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_LISTADO);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    e = web.discover(p)
    show("ROW=" + e["row"])
    show("COUNT=" + str(e["count"]))
    show("FRAGIL=" + str(e["fragil"]))
    show("FIELDS=" + str(e["fields"]))
    show("S0=" + str(e["sample"][0]))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    // El menu tiene 2 enlaces y el listado 4 tarjetas ricas: debe elegir las
    // tarjetas, no el menu.
    assert!(salida.contains("ROW=.card"), "no encontró la fila correcta:\n{salida}");
    assert!(salida.contains("COUNT=4"), "no contó las cuatro tarjetas:\n{salida}");
    assert!(salida.contains("FRAGIL=no"), "la clase .card deberia dar selector estable:\n{salida}");
    // Los campos con clase legible se nombran con su clase.
    assert!(salida.contains("titulo:") && salida.contains(".titulo"), "falta el campo titulo:\n{salida}");
    assert!(salida.contains("precio:") && salida.contains(".precio"), "falta el campo precio:\n{salida}");
    // La muestra ya trae datos reales.
    assert!(salida.contains("Teclado"), "la muestra no extrajo el primer titulo:\n{salida}");
}

#[test]
fn discover_lo_que_propone_funciona_con_extract() {
    let dir = tmp_dir("discover_extract");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let url = serve_html(PAGINA_LISTADO);

    // La prueba de fondo: el selector de fila que propone `discover` sirve tal
    // cual en `extract`. Si no, la propuesta seria decorativa.
    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    e = web.discover(p)
    filas = web.extract(p, e["row"], {{ t: ".titulo", pr: ".precio|num" }})
    show("N=" + str(len(filas)))
    show("R0=" + str(filas[0]))
    show("R3=" + str(filas[3]))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("N=4"), "el row de discover no sirvio en extract:\n{salida}");
    assert!(salida.contains("t: Teclado") && salida.contains("pr: 49.9"), "{salida}");
    assert!(salida.contains("t: Webcam"), "{salida}");
}

#[test]
fn discover_sin_estructura_repetida_lo_dice() {
    let dir = tmp_dir("discover_vacio");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    // Una pagina sin ningun listado: solo texto suelto.
    let url = serve_html(r#"<!doctype html><html><head><title>X</title></head>
<body><h1>Hola</h1><p>Un parrafo suelto y nada mas.</p></body></html>"#);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
with b = web.open() {{
    p = web.page(b)
    web.goto(p, "{url}")
    attempt {{ web.discover(p, {{ wait: 800 }}) }} handle e {{ show("E=" + e) }}
}}
"##));
    assert!(salida.contains("E=") && salida.contains("estructura repetida"),
            "deberia decir que no hay listado:\n{salida}");
}

//    Recorrido paralelo (crawl)

/// Sirve N paginas de catalogo, cada una con un retraso, desde varios hilos.
/// El retraso y la concurrencia del servidor son lo que hace que el paralelismo
/// se note: en serie el recorrido tarda N×retraso, en paralelo mucho menos.
fn serve_catalogo(paginas: usize, retraso_ms: u64) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir puerto");
    let port = listener.local_addr().unwrap().port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            // Un hilo por conexion: el servidor tiene que poder atender varias a
            // la vez o mediria su propia serializacion, no la del crawler.
            std::thread::spawn(move || {
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let ruta = req.split_whitespace().nth(1).unwrap_or("/");
                let pag: usize = ruta.rsplit("page=").next()
                    .and_then(|x| x.parse().ok()).unwrap_or(0);
                std::thread::sleep(std::time::Duration::from_millis(retraso_ms));
                let filas: String = (0..5).map(|i| format!(
                    "<div class=\"card\"><span class=\"t\">Item {pag}-{i}</span>\
                     <span class=\"p\">{}.50</span></div>", pag * 10 + i
                )).collect();
                let html = format!("<!doctype html><html><head><title>P{pag}</title></head>\
                    <body><div id=l>{filas}</div></body></html>");
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{}", html.len(), html);
                let _ = s.write_all(resp.as_bytes());
                let _ = s.flush();
            });
        }
    });
    let _ = paginas;
    format!("http://127.0.0.1:{port}/")
}

#[test]
fn crawl_recorre_en_paralelo_y_vuelca_a_disco() {
    let dir = tmp_dir("crawl");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let base = serve_catalogo(12, 150);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
fn urls(base, n) {{
    l = []
    for i in range(1, n + 1) {{ l.push(base + "?page=" + str(i)) }}
    return l
}}
with b = web.open() {{
    r = web.crawl(b, {{
        urls:    urls("{base}", 12),
        row:     ".card",
        schema:  {{ item: ".t", precio: ".p|num" }},
        out:     "cat.csv",
        workers: 6
    }})
    show("ROWS=" + str(r["rows"]))
    show("OK=" + str(r["ok"]))
    show("WORKERS=" + str(r["workers"]))
    show("FAILED=" + str(r["failed"]))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    // 12 páginas × 5 filas.
    assert!(salida.contains("ROWS=60"), "no extrajo todas las filas:\n{salida}");
    assert!(salida.contains("OK=12"), "{salida}");
    assert!(salida.contains("WORKERS=6"), "no abrió las seis pestañas:\n{salida}");
    assert!(salida.contains("FAILED=0"), "{salida}");
    // El archivo tiene cabecera + 60 filas.
    let csv = fs::read_to_string(dir.join("cat.csv")).unwrap_or_default();
    assert_eq!(csv.lines().count(), 61, "el csv no tiene las 60 filas + cabecera:\n{csv}");
    assert!(csv.contains("Item 1-0") && csv.contains("Item 12-4"), "faltan filas:\n{csv}");
}

#[test]
fn crawl_es_mas_rapido_en_paralelo_que_en_serie() {
    let dir = tmp_dir("crawl_veloc");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    // 12 páginas a 200 ms: en serie ~2.4 s, con 6 workers deberia bajar claro.
    let base = serve_catalogo(12, 200);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
use "datetime"
fn urls(base, n) {{
    l = []
    for i in range(1, n + 1) {{ l.push(base + "?page=" + str(i)) }}
    return l
}}
with b = web.open() {{
    p = web.page(b)
    us = urls("{base}", 12)
    esq = {{ item: ".t" }}

    t0 = datetime.timestamp_ms()
    web.extract_to(p, us, ".card", esq, "s.csv")
    serie = datetime.timestamp_ms() - t0

    t1 = datetime.timestamp_ms()
    web.crawl(b, {{ urls: us, row: ".card", schema: esq, out: "p.csv", workers: 6 }})
    par = datetime.timestamp_ms() - t1

    show("SERIE=" + str(serie))
    show("PAR=" + str(par))
    show("MAS_RAPIDO=" + str(par < serie))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    // No se fija un factor exacto —depende de la máquina—, solo que el paralelo
    // gana con claridad. En serie son 12×200ms; el paralelo con 6 workers debe
    // quedar holgadamente por debajo.
    assert!(salida.contains("MAS_RAPIDO=yes"),
            "el recorrido paralelo no fue más rápido que el serie:\n{salida}");
}

#[test]
fn crawl_reanuda_sin_repetir_lo_hecho() {
    let dir = tmp_dir("crawl_resume");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let base = serve_catalogo(24, 60);

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web
fn urls(base, desde, hasta) {{
    l = []
    for i in range(desde, hasta + 1) {{ l.push(base + "?page=" + str(i)) }}
    return l
}}
with b = web.open() {{
    esq = {{ item: ".t" }}
    -- Primera tanda: 1-12.
    a = web.crawl(b, {{ urls: urls("{base}", 1, 12), row: ".card", schema: esq,
                        out: "r.csv", workers: 4, resume: yes }})
    show("A_OK=" + str(a["ok"]) + " A_SKIP=" + str(a["skipped"]))
    -- Segunda tanda: 1-24 con resume. Debe saltar las 12 hechas.
    c = web.crawl(b, {{ urls: urls("{base}", 1, 24), row: ".card", schema: esq,
                        out: "r.csv", workers: 4, resume: yes }})
    show("C_OK=" + str(c["ok"]) + " C_SKIP=" + str(c["skipped"]))
}}
"##));
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("A_OK=12 A_SKIP=0"), "la primera tanda no hizo 12:\n{salida}");
    // La clave: la segunda tanda salta las 12 ya hechas y solo procesa 12 nuevas.
    assert!(salida.contains("C_OK=12 C_SKIP=12"),
            "la reanudación no saltó lo ya hecho:\n{salida}");
    // El csv final tiene las 24 páginas (120 filas) sin duplicados.
    let csv = fs::read_to_string(dir.join("r.csv")).unwrap_or_default();
    assert_eq!(csv.lines().count(), 121, "el csv reanudado no tiene 120 filas + cabecera");
}

#[test]
fn crawl_un_campo_muerto_en_todas_las_paginas_se_delata() {
    let dir = tmp_dir("crawl_muerto");
    if !hay_navegador(&dir) { return; }
    let _turno = turno();
    let base = serve_catalogo(4, 30);

    let (salida, _) = run_orion(&dir, &format!(r##"
use "browser" as web
fn urls(base, n) {{
    l = []
    for i in range(1, n + 1) {{ l.push(base + "?page=" + str(i)) }}
    return l
}}
with b = web.open() {{
    attempt {{
        web.crawl(b, {{ urls: urls("{base}", 4), row: ".card",
                        schema: {{ item: ".t", fantasma: ".no-existe" }}, out: "m.csv", workers: 4 }})
    }} handle e {{ show("E=" + e) }}
}}
"##));
    // `.no-existe` no aparece en ninguna página: callarlo dejaría una columna
    // vacía que parece buena, que es el fallo que este aviso evita.
    assert!(salida.contains("E=") && salida.contains("fantasma"),
            "un campo muerto en todo el recorrido debería delatarse:\n{salida}");
}

//    attach() — engancharse a un navegador ajeno

#[test]
fn attach_usa_un_navegador_ya_abierto_y_no_lo_cierra() {
    // El invariante de `attach`: soltar el enganche no puede matar un navegador
    // que no arrancamos. Se comprueba usándolo después, no mirando si el proceso
    // existe — en Windows tarda en desaparecer y daría un verde falso.
    let dir = tmp_dir("attach");
    if !hay_navegador(&dir) {
        eprintln!("[skip] no hay navegador Chromium en esta máquina");
        return;
    }
    let _turno = turno();
    let url = serve_html(PAGINA);
    let puerto = 39_222;

    let (salida, ok) = run_orion(&dir, &format!(r##"
use "browser" as web

with propio = web.open({{ args: ["--remote-debugging-port={puerto}"] }}) {{
    -- Segundo handle sobre el MISMO navegador, por el puerto.
    ajeno = web.attach({puerto})
    p = web.page(ajeno)
    web.goto(p, "{url}")
    show("ENGANCHADO=" + web.title(p))

    -- Soltar el enganche. Si `free` matara el navegador ajeno, lo de abajo
    -- fallaría: es la comprobación de verdad.
    web.free(ajeno)

    q = web.page(propio)
    web.goto(q, "{url}")
    show("SIGUE_VIVO=" + web.title(q))
}}
show("FIN")
"##));

    assert!(ok, "el programa falló:\n{salida}");
    assert!(salida.contains("ENGANCHADO=Pagina de prueba"),
            "attach no pudo usar el navegador ya abierto:\n{salida}");
    assert!(salida.contains("SIGUE_VIVO=Pagina de prueba"),
            "soltar el enganche mató un navegador que no era suyo:\n{salida}");
    assert!(salida.contains("FIN"), "el bloque with no terminó:\n{salida}");
}

#[test]
fn attach_a_un_puerto_sin_navegador_explica_que_hacer() {
    // Es EL error del caso real: en un equipo gestionado el navegador del día a
    // día no expone CDP. Un "conexión rechazada" a secas no dice qué hacer, así
    // que el mensaje tiene que nombrar la bandera que falta.
    let dir = tmp_dir("attach_sin_nadie");
    let (salida, ok) = run_orion(&dir, r##"
use "browser" as web
attempt {
    b = web.attach(39299)
    show("NO_DEBERIA_LLEGAR")
} handle e {
    show("ERR=" + e)
}
"##);

    assert!(ok, "el attempt debería recoger el error:\n{salida}");
    assert!(salida.contains("--remote-debugging-port"),
            "el error debe decir qué bandera falta:\n{salida}");
}
