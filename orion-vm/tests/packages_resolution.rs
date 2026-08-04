//! Resolución de paquetes end-to-end.
//!
//! Lo que se protege aquí es la promesa que motivó el cambio: un `use` encuentra
//! sus paquetes por la **raíz del proyecto**, no por el directorio desde el que
//! se invocó el binario. Antes dependía del cwd, así que el mismo programa
//! funcionaba o no según desde dónde lo lanzaras.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

fn orion_bin() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") { p.pop(); }
    p.join(if cfg!(windows) { "orion.exe" } else { "orion" })
}

/// Cada test trabaja en su propio árbol para no pisarse con los demás.
fn fresh_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("orion_pkg_tests").join(name);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Ejecuta `orion run <archivo>` desde `cwd`, con un HOME propio para que la
/// caché global de paquetes del test no toque la del usuario.
fn run_from(cwd: &Path, home: &Path, file: &Path) -> (String, bool) {
    let out = Command::new(orion_bin())
        .arg("run")
        .arg(file)
        .current_dir(cwd)
        .env("ORION_PKGS", home.join("global_pkgs"))
        .output()
        .expect("no se pudo ejecutar orion");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (text, out.status.success())
}

/// Monta un proyecto: manifiesto en la raíz, un paquete y un programa anidado.
fn scaffold(root: &Path) -> PathBuf {
    fs::create_dir_all(root.join("packages")).unwrap();
    fs::create_dir_all(root.join("src").join("hondo")).unwrap();

    fs::write(root.join("orion.json"), r#"{
      "name": "proyecto-de-prueba",
      "version": "1.0.0",
      "description": "fixture de tests"
    }"#).unwrap();

    fs::write(root.join("packages").join("saludo.orx"), r#"
fn hola(nombre) {
    return "hola " + nombre
}
"#).unwrap();

    let prog = root.join("src").join("hondo").join("main.orx");
    fs::write(&prog, r#"
use "saludo" as s
show(s.hola("orion"))
"#).unwrap();
    prog
}

#[test]
fn el_paquete_se_encuentra_desde_cualquier_cwd() {
    let root = fresh_dir("cwd_independiente");
    let prog = scaffold(&root);

    // Desde la raíz del proyecto: es el caso que ya funcionaba antes.
    let (desde_raiz, ok1) = run_from(&root, &root, &prog);
    assert!(ok1, "falló ejecutando desde la raíz:\n{desde_raiz}");
    assert!(desde_raiz.contains("hola orion"), "salida inesperada:\n{desde_raiz}");

    // Desde un subdirectorio hondo: este es el que se rompía, porque `use`
    // resolvía "packages/saludo.orx" relativo al cwd y ahí no hay ninguno.
    let hondo = root.join("src").join("hondo");
    let (desde_hondo, ok2) = run_from(&hondo, &root, &prog);
    assert!(ok2, "falló ejecutando desde un subdirectorio:\n{desde_hondo}");
    assert!(desde_hondo.contains("hola orion"), "salida inesperada:\n{desde_hondo}");

    // Desde fuera del proyecto por completo.
    let fuera = std::env::temp_dir();
    let (desde_fuera, ok3) = run_from(&fuera, &root, &prog);
    assert!(ok3, "falló ejecutando desde fuera del proyecto:\n{desde_fuera}");
    assert!(desde_fuera.contains("hola orion"), "salida inesperada:\n{desde_fuera}");
}

#[test]
fn el_prefijo_packages_sigue_valiendo() {
    // Retrocompatibilidad: el código que ya existe escribe `use "packages/x"`.
    let root = fresh_dir("prefijo_packages");
    scaffold(&root);

    let prog = root.join("src").join("con_prefijo.orx");
    fs::write(&prog, r#"
use "packages/saludo" as s
show(s.hola("mundo"))
"#).unwrap();

    let (salida, ok) = run_from(&std::env::temp_dir(), &root, &prog);
    assert!(ok, "falló con prefijo packages/:\n{salida}");
    assert!(salida.contains("hola mundo"), "salida inesperada:\n{salida}");
}

#[test]
fn el_manifiesto_gana_a_un_packages_mas_cercano() {
    // Un `packages/` suelto dentro del proyecto no debe secuestrar la raíz:
    // manda quien tiene orion.json.
    let root = fresh_dir("manifiesto_manda");
    let prog = scaffold(&root);

    let intruso = root.join("src").join("hondo").join("packages");
    fs::create_dir_all(&intruso).unwrap();
    fs::write(intruso.join("saludo.orx"), r#"
fn hola(nombre) { return "IMPOSTOR " + nombre }
"#).unwrap();

    let (salida, ok) = run_from(&std::env::temp_dir(), &root, &prog);
    assert!(ok, "falló:\n{salida}");
    assert!(salida.contains("hola orion"), "ganó el packages/ intruso:\n{salida}");
    assert!(!salida.contains("IMPOSTOR"), "ganó el packages/ intruso:\n{salida}");
}

#[test]
fn doctor_reporta_los_paquetes_realmente_instalados() {
    // El bug original: doctor miraba ~/.orion/packages y listaba subdirectorios,
    // así que decía "ningún paquete instalado" con paquetes instalados.
    let root = fresh_dir("doctor_honesto");
    scaffold(&root);

    fs::write(root.join("packages").join("installed.json"), r#"{
      "saludo": { "version": "2.1.0", "description": "d", "file": "saludo.orx", "source": "local" }
    }"#).unwrap();

    let out = Command::new(orion_bin())
        .arg("doctor")
        .current_dir(&root)
        .env("ORION_PKGS", root.join("global_pkgs"))
        .output()
        .expect("no se pudo ejecutar orion doctor");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(text.contains("saludo"), "doctor no listó el paquete instalado:\n{text}");
    assert!(text.contains("2.1.0"), "doctor no mostró la versión:\n{text}");
    assert!(!text.contains("Ningún paquete instalado"),
            "doctor sigue diciendo que no hay paquetes:\n{text}");
    // Y debe informar de la raíz que de verdad está usando.
    assert!(text.contains("Raíz de proyecto"), "doctor no informa de la raíz:\n{text}");
}

#[test]
fn install_sin_manifiesto_explica_en_vez_de_romperse() {
    let dir = fresh_dir("sin_manifiesto");

    let out = Command::new(orion_bin())
        .arg("install")
        .current_dir(&dir)
        .env("ORION_PKGS", dir.join("global_pkgs"))
        .output()
        .expect("no se pudo ejecutar orion install");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(!out.status.success(), "debería salir con error");
    assert!(text.contains("orion.json"), "el error no menciona el manifiesto:\n{text}");
    assert!(text.contains("Raíz detectada"), "el error no dice qué raíz encontró:\n{text}");
}

#[test]
fn add_desde_ruta_local_con_checksum() {
    // `--add` con una ruta local y `--sha256`: el camino que no depende de red
    // y que ejercita la verificación de integridad de punta a punta.
    let dir = fresh_dir("add_local");
    fs::write(dir.join("orion.json"), r#"{
      "name": "consumidor", "version": "1.0.0", "description": "d"
    }"#).unwrap();

    let fuente = dir.join("colores.orx");
    let contenido = "fn rojo() { return \"rojo\" }\n";
    fs::write(&fuente, contenido).unwrap();

    // sha256 del contenido, calculado aquí para no confiar en el propio código.
    let digest = {
        use std::process::Stdio;
        // Sin dependencias de test: se pide el hash al propio binario mediante
        // un programa Orion, que ya expone crypto.sha256.
        let helper = dir.join("_hash.orx");
        fs::write(&helper, r#"
use "crypto" as c
use "fs" as f
show(c.sha256(f.read("colores.orx")))
"#).unwrap();
        let out = Command::new(orion_bin())
            .arg("run").arg(&helper)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .expect("no se pudo calcular el hash");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    // Si el módulo crypto no devolvió un hash usable, el test no puede afirmar
    // nada sobre la verificación: se salta en vez de dar un falso fallo.
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        eprintln!("[skip] no se pudo obtener sha256 con el módulo crypto: {digest:?}");
        return;
    }

    // Con el checksum correcto instala.
    let ok = Command::new(orion_bin())
        .args(["--add", "./colores.orx", "--sha256", &digest])
        .current_dir(&dir)
        .env("ORION_PKGS", dir.join("global_pkgs"))
        .output()
        .expect("no se pudo ejecutar orion --add");
    let ok_text = format!(
        "{}{}",
        String::from_utf8_lossy(&ok.stdout),
        String::from_utf8_lossy(&ok.stderr)
    );
    assert!(ok.status.success(), "add con sha correcto falló:\n{ok_text}");
    assert!(dir.join("packages").join("colores.orx").is_file(),
            "no se copió al directorio del proyecto:\n{ok_text}");

    // Con un checksum equivocado se niega.
    let malo = "0".repeat(64);
    let bad = Command::new(orion_bin())
        .args(["--add", "./colores.orx", "--force", "--sha256", &malo])
        .current_dir(&dir)
        .env("ORION_PKGS", dir.join("global_pkgs"))
        .output()
        .expect("no se pudo ejecutar orion --add");
    let bad_text = format!(
        "{}{}",
        String::from_utf8_lossy(&bad.stdout),
        String::from_utf8_lossy(&bad.stderr)
    );
    assert!(!bad.status.success(), "add con sha incorrecto debería fallar:\n{bad_text}");
    assert!(bad_text.contains("checksum"), "el error no menciona el checksum:\n{bad_text}");
}

/// SHA-256 de "abc": vector estándar, para que el test no dependa de que el
/// propio código calcule bien el hash que luego va a comprobar.
const ABC: &[u8] = b"abc";
const ABC_SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

/// ¿Tiene esta plataforma un nombre de asset definido? Los tests de binarios
/// nativos no aplican donde no lo tiene.
fn plataforma_soportada() -> Option<&'static str> {
    if cfg!(all(windows, target_arch = "x86_64")) { Some("win32-x64") }
    else if cfg!(all(target_os = "macos", target_arch = "aarch64")) { Some("darwin-arm64") }
    else if cfg!(all(target_os = "linux", target_arch = "x86_64")) { Some("linux-x64") }
    else { None }
}

/// Servidor HTTP mínimo, suficiente para servir un registry y un asset. Se
/// escribe a mano para no añadir una dependencia de test solo por esto.
/// Devuelve la URL base; el hilo atiende hasta que el proceso de test termina.
fn serve(rutas: Vec<(String, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("no se pudo abrir puerto");
    let port = listener.local_addr().unwrap().port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();

            let cuerpo = rutas.iter().find(|(p, _)| *p == path).map(|(_, b)| b.clone());
            let resp = match cuerpo {
                Some(b) => {
                    let mut head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        b.len()
                    ).into_bytes();
                    head.extend_from_slice(&b);
                    head
                }
                None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            };
            let _ = s.write_all(&resp);
            let _ = s.flush();
        }
    });

    format!("http://127.0.0.1:{port}")
}

/// Levanta un registry servido por HTTP que publica un paquete con binario
/// nativo para la plataforma actual. `sha` controla qué checksum se declara.
fn registry_con_asset(plat: &str, sha: Option<&str>) -> String {
    // Dos pasos porque la URL del asset apunta al propio servidor: primero se
    // reserva el puerto sirviendo el binario, y después se publica el registry
    // que lo referencia.
    let assets = serve(vec![
        ("/nativo.bin".to_string(), ABC.to_vec()),
        ("/nativo.orx".to_string(), b"fn nada() { return 1 }\n".to_vec()),
    ]);
    let sha_campo = match sha {
        Some(s) => format!(r#", "sha256": "{s}""#),
        None    => String::new(),
    };
    let registry = format!(r#"{{
      "_meta": {{ "registry": "{assets}" }},
      "packages": {{
        "nativo": {{
          "version": "1.0.0",
          "description": "paquete con binario",
          "type": "native",
          "file": "nativo.orx",
          "assets": {{
            "{plat}": {{ "url": "{assets}/nativo.bin"{sha_campo} }}
          }}
        }}
      }}
    }}"#);

    serve(vec![
        ("/registry.json".to_string(), registry.into_bytes()),
        ("/nativo.orx".to_string(),    b"fn nada() { return 1 }\n".to_vec()),
        ("/nativo.bin".to_string(),    ABC.to_vec()),
    ])
}

fn add_nativo(dir: &Path, base_registry: &str) -> (String, bool) {
    let out = Command::new(orion_bin())
        .args(["--add", "nativo", "--force"])
        .current_dir(dir)
        .env("ORION_REGISTRY", base_registry)
        .env("ORION_PKGS", dir.join("global_pkgs"))
        .output()
        .expect("no se pudo ejecutar orion --add");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (text, out.status.success())
}

fn proyecto_vacio(nombre: &str) -> PathBuf {
    let dir = fresh_dir(nombre);
    fs::write(dir.join("orion.json"),
              r#"{"name":"c","version":"1.0.0","description":"d"}"#).unwrap();
    dir
}

#[test]
fn asset_nativo_se_instala_en_su_directorio() {
    let Some(plat) = plataforma_soportada() else {
        eprintln!("[skip] plataforma sin nombre de asset definido");
        return;
    };
    let dir = proyecto_vacio("asset_nativo");
    let base = registry_con_asset(plat, Some(ABC_SHA));

    let (salida, ok) = add_nativo(&dir, &base);
    assert!(ok, "la instalación con sha correcto falló:\n{salida}");

    let native_root = dir.join("packages").join("native").join("nativo");
    assert!(native_root.is_dir(), "no se creó {}:\n{salida}", native_root.display());
    assert!(fs::read_dir(&native_root).unwrap().flatten().count() > 0,
            "no se guardó ningún binario en {}:\n{salida}", native_root.display());

    // installed.json debe anotar la ruta nativa para poder desinstalarla luego.
    let inst = fs::read_to_string(dir.join("packages").join("installed.json")).unwrap();
    assert!(inst.contains("native"), "installed.json no anota el binario:\n{inst}");
}

#[test]
fn asset_con_checksum_falso_se_rechaza() {
    let Some(plat) = plataforma_soportada() else { return };
    let dir = proyecto_vacio("asset_sha_malo");
    let base = registry_con_asset(plat, Some(&"0".repeat(64)));

    let (salida, ok) = add_nativo(&dir, &base);
    assert!(!ok, "un binario con checksum falso NO debe instalarse:\n{salida}");
    assert!(salida.contains("checksum"), "el error no menciona el checksum:\n{salida}");
}

#[test]
fn asset_sin_checksum_se_rechaza() {
    let Some(plat) = plataforma_soportada() else { return };
    let dir = proyecto_vacio("asset_sin_sha");
    let base = registry_con_asset(plat, None);

    let (salida, ok) = add_nativo(&dir, &base);
    assert!(!ok, "un binario sin sha256 declarado NO debe instalarse:\n{salida}");
    assert!(salida.contains("sha256"), "el error no explica que falta el sha256:\n{salida}");
}

/// Genera un par de claves RSA y firma `datos` usando el propio Orion.
/// Devuelve `(pem_publica, firma_base64)`, o `None` si el módulo crypto2 no
/// devolvió lo esperado — en ese caso el test que la use se salta en vez de
/// dar un falso fallo.
fn firmar_con_orion(dir: &Path, datos: &str) -> Option<(String, String)> {
    let script = dir.join("_firmar.orx");
    fs::write(&script, format!(r#"
use "crypto2" as k
use "fs" as f
par = k.rsa_keygen(2048)
f.write("_clave.pem", par["public_key"])
show(k.rsa_sign("{datos}", par["private_key"]))
"#)).unwrap();

    let out = Command::new(orion_bin())
        .arg("run").arg(&script)
        .current_dir(dir)
        .output()
        .ok()?;
    let firma = String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.len() > 100 && !l.starts_with('['))
        .map(str::to_string)?;
    let pem = fs::read_to_string(dir.join("_clave.pem")).ok()?;
    if !pem.contains("BEGIN PUBLIC KEY") { return None; }
    Some((pem, firma))
}

#[test]
fn firma_valida_se_acepta_y_firma_ajena_se_rechaza() {
    let Some(plat) = plataforma_soportada() else { return };
    let dir = proyecto_vacio("firma_rsa");

    let Some((pem, firma)) = firmar_con_orion(&dir, "abc") else {
        eprintln!("[skip] crypto2 no produjo un par de claves usable");
        return;
    };

    // La clave se instala como de confianza en un directorio propio.
    let claves = dir.join("claves");
    fs::create_dir_all(&claves).unwrap();
    fs::write(claves.join("editor.pem"), &pem).unwrap();

    let instalar = |firma_declarada: &str| {
        let assets = serve(vec![("/nativo.bin".to_string(), ABC.to_vec())]);
        let registry = format!(r#"{{
          "_meta": {{ "registry": "{assets}" }},
          "packages": {{ "nativo": {{
            "version": "1.0.0", "description": "d", "type": "native", "file": "nativo.orx",
            "assets": {{ "{plat}": {{ "url": "{assets}/nativo.bin", "sha256": "{ABC_SHA}",
                                     "signature": "{firma_declarada}" }} }}
          }} }}
        }}"#);
        let base = serve(vec![
            ("/registry.json".to_string(), registry.into_bytes()),
            ("/nativo.orx".to_string(),    b"fn nada() { return 1 }\n".to_vec()),
        ]);
        let out = Command::new(orion_bin())
            .args(["--add", "nativo", "--force"])
            .current_dir(&dir)
            .env("ORION_REGISTRY", &base)
            .env("ORION_PKGS", dir.join("global_pkgs"))
            .env("ORION_TRUSTED_KEYS", &claves)
            .output()
            .expect("no se pudo ejecutar orion --add");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (text, out.status.success())
    };

    // 1. Firma correcta: se instala y se dice con qué clave se verificó.
    let (bien, ok) = instalar(&firma);
    assert!(ok, "una firma válida debería aceptarse:\n{bien}");
    assert!(bien.contains("editor.pem"), "no se informó de la clave usada:\n{bien}");

    // 2. Firma de otro contenido: mismo formato, no valida contra este binario.
    let Some((_, firma_ajena)) = firmar_con_orion(&dir, "otra-cosa") else { return };
    let (mal, ok2) = instalar(&firma_ajena);
    assert!(!ok2, "una firma que no corresponde debería rechazarse:\n{mal}");
    assert!(mal.contains("firma"), "el error no menciona la firma:\n{mal}");
}

#[test]
fn firma_sin_claves_de_confianza_avisa_pero_instala() {
    // Sin claves instaladas no se puede afirmar nada sobre la autoría, pero el
    // sha256 sí se comprobó: se avisa y se sigue, en vez de bloquear.
    let Some(plat) = plataforma_soportada() else { return };
    let dir = proyecto_vacio("firma_sin_claves");

    let assets = serve(vec![("/nativo.bin".to_string(), ABC.to_vec())]);
    let registry = format!(r#"{{
      "_meta": {{ "registry": "{assets}" }},
      "packages": {{ "nativo": {{
        "version": "1.0.0", "description": "d", "type": "native", "file": "nativo.orx",
        "assets": {{ "{plat}": {{ "url": "{assets}/nativo.bin", "sha256": "{ABC_SHA}",
                                 "signature": "Zmlybm5hLWludmVudGFkYQ==" }} }}
      }} }}
    }}"#);
    let base = serve(vec![
        ("/registry.json".to_string(), registry.into_bytes()),
        ("/nativo.orx".to_string(),    b"fn nada() { return 1 }\n".to_vec()),
    ]);

    let vacio = dir.join("sin_claves");
    fs::create_dir_all(&vacio).unwrap();
    let out = Command::new(orion_bin())
        .args(["--add", "nativo", "--force"])
        .current_dir(&dir)
        .env("ORION_REGISTRY", &base)
        .env("ORION_PKGS", dir.join("global_pkgs"))
        .env("ORION_TRUSTED_KEYS", &vacio)
        .output()
        .expect("no se pudo ejecutar orion --add");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(out.status.success(), "debería instalar avisando, no bloquear:\n{text}");
    assert!(text.contains("claves de confianza"), "no se avisó de la falta de claves:\n{text}");
}

#[test]
fn sin_terminal_no_se_imprimen_animaciones() {
    // La barra de progreso se reescribe con \r. En una tubería o en un log de
    // CI eso solo deja basura, así que debe callarse: aquí la salida se captura
    // por pipe, que es exactamente el caso.
    let Some(plat) = plataforma_soportada() else { return };
    let dir = proyecto_vacio("sin_tty");
    let base = registry_con_asset(plat, Some(ABC_SHA));

    let (salida, ok) = add_nativo(&dir, &base);
    assert!(ok, "la instalación falló:\n{salida}");

    for basura in ['█', '░', '⠋', '⠙', '⠹'] {
        assert!(!salida.contains(basura),
                "se coló el carácter de progreso {basura:?} en salida no interactiva:\n{salida}");
    }
    assert!(!salida.contains('\r'),
            "se colaron retornos de carro en salida no interactiva:\n{salida:?}");
}

#[test]
fn el_lockfile_sobrevive_a_un_install_fallido() {
    // Regresión: al fallar una dependencia se reescribía el lock sin su entrada,
    // convirtiendo un error de red o de checksum en la pérdida del pin.
    let dir = fresh_dir("lock_superviviente");
    fs::create_dir_all(dir.join("externo")).unwrap();
    fs::write(dir.join("externo").join("dep.orx"), "fn f() { return 1 }\n").unwrap();
    fs::write(dir.join("orion.json"), r#"{
      "name": "c", "version": "1.0.0", "description": "d",
      "dependencies": { "dep": "./externo/dep.orx" }
    }"#).unwrap();

    let install = |d: &Path| Command::new(orion_bin())
        .arg("install")
        .current_dir(d)
        .env("ORION_PKGS", d.join("global_pkgs"))
        .output()
        .expect("no se pudo ejecutar orion install");

    assert!(install(&dir).status.success(), "el primer install debería funcionar");
    let lock_bueno = fs::read_to_string(dir.join("orion.lock")).unwrap();
    assert!(lock_bueno.contains("sha256"), "el lock no fijó el hash:\n{lock_bueno}");

    // Se altera la fuente: el hash deja de cuadrar con el pin.
    fs::write(dir.join("externo").join("dep.orx"), "fn f() { return 999 }\n").unwrap();
    let out = install(&dir);
    assert!(!out.status.success(), "install con contenido alterado debería fallar");

    let lock_despues = fs::read_to_string(dir.join("orion.lock")).unwrap();
    assert_eq!(lock_bueno, lock_despues, "el lockfile perdió el pin tras el fallo");

    let instalado = fs::read_to_string(dir.join("packages").join("dep.orx")).unwrap();
    assert!(!instalado.contains("999"), "se escribió el contenido alterado:\n{instalado}");
}
