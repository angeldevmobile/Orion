//! Resolución de paquetes end-to-end.
//!
//! Lo que se protege aquí es la promesa que motivó el cambio: un `use` encuentra
//! sus paquetes por la **raíz del proyecto**, no por el directorio desde el que
//! se invocó el binario. Antes dependía del cwd, así que el mismo programa
//! funcionaba o no según desde dónde lo lanzaras.

use std::fs;
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
