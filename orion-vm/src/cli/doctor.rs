use std::fs;
use crate::{lexer, parser, codegen, paths};
use super::banner;

pub fn run_doctor() {
    banner::animate_startup();
    banner::print_banner();
    banner::section("Diagnóstico del entorno Orion");

    let mut all_ok = true;

    // 1. Binary version
    banner::row("Versión VM", &format!("v{}", env!("CARGO_PKG_VERSION")), true);

    // 2. Project + package directories
    //
    // Se informa de las mismas rutas que usa el runtime (crate::paths), no de
    // una copia local: este check existía y mentía porque miraba otro sitio.
    let root = paths::project_root();
    banner::row("Raíz de proyecto", &root.to_string_lossy(), true);
    banner::row(
        "Manifiesto",
        &if paths::has_manifest() {
            paths::manifest_path().to_string_lossy().to_string()
        } else {
            format!("(sin {})", paths::MANIFEST)
        },
        paths::has_manifest(),
    );

    let proj_pkgs = paths::project_packages_dir();
    banner::row("Paquetes del proyecto", &proj_pkgs.to_string_lossy(), proj_pkgs.is_dir());

    let global_pkgs = paths::global_packages_dir();
    banner::row("Paquetes globales", &global_pkgs.to_string_lossy(), global_pkgs.is_dir());

    // Ninguno de los dos es obligatorio: un proyecto sin dependencias no tiene
    // por qué tener directorio de paquetes, así que esto no tumba el
    // diagnóstico. Lo que sí importa es poder escribir donde toca.
    let pkg_dir = paths::install_dir();
    banner::row("Instalaría en", &pkg_dir.to_string_lossy(), true);

    // 3. Temp write access
    let tmp = std::env::temp_dir().join("orion_doctor_check.tmp");
    let can_write = fs::write(&tmp, b"ok").is_ok();
    let _ = fs::remove_file(&tmp);
    banner::row("Escritura en /tmp", if can_write { "OK" } else { "Sin permisos" }, can_write);
    if !can_write { all_ok = false; }

    // 4. Quick compile + run sanity check
    let hello = r#"print("__doctor_ok__")"#;
    let compile_ok = check_compile(hello);
    banner::row("Pipeline lex+parse+codegen", if compile_ok { "OK" } else { "ERROR" }, compile_ok);
    if !compile_ok { all_ok = false; }

    // 5. Environment variables
    println!();
    banner::section("Variables de entorno");
    for var in &["ORION_HOME", "ORION_PKGS", "ORION_DEBUG"] {
        match std::env::var(var) {
            Ok(v) => banner::row(var, &v, true),
            Err(_) => banner::row(var, "(no definida)", false),
        }
    }

    // 6. Installed packages
    //
    // Se leen de installed.json, no listando subdirectorios: los paquetes son
    // archivos .orx sueltos, así que el listado anterior siempre salía vacío.
    println!();
    banner::section("Paquetes instalados");
    let inventario = crate::pkg::installed_everywhere();
    if inventario.is_empty() {
        banner::info("Ningún paquete instalado");
    } else {
        for (dir, pkgs) in inventario {
            banner::info(&format!("{} ({} paquete(s))", dir.display(), pkgs.len()));
            for (name, rec) in &pkgs {
                let ver = rec["version"].as_str().unwrap_or("?");
                let nat = if rec["native"].is_string() { "  [nativo]" } else { "" };
                banner::row(name, &format!("v{ver}{nat}"), true);
            }
        }
    }

    // Final verdict
    println!();
    if all_ok {
        banner::ok("Todo en orden — Orion listo para usar");
    } else {
        banner::fail("Algunos checks fallaron — revisa los elementos marcados con ✗");
        std::process::exit(1);
    }
}

fn check_compile(src: &str) -> bool {
    let tokens = match lexer::lex(src) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let stmts = match parser::parse(tokens) {
        Ok(s) => s,
        Err(_) => return false,
    };
    codegen::compile(stmts).is_ok()
}

