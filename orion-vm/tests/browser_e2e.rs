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
