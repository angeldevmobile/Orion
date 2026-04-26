use std::fs;
use std::path::PathBuf;
use crate::{lexer, parser, codegen};
use super::banner;

pub fn run_doctor() {
    banner::animate_startup();
    banner::print_banner();
    banner::section("Diagnóstico del entorno Orion");

    let mut all_ok = true;

    // 1. Binary version
    banner::row("Versión VM", "v0.4.0", true);

    // 2. Packages directory
    let pkg_dir = packages_dir();
    let pkg_exists = pkg_dir.exists();
    banner::row(
        "Directorio paquetes",
        &pkg_dir.to_string_lossy(),
        pkg_exists,
    );
    if !pkg_exists { all_ok = false; }

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
    println!();
    banner::section("Paquetes instalados");
    match list_installed(&pkg_dir) {
        pkgs if pkgs.is_empty() => banner::info("Ningún paquete instalado"),
        pkgs => {
            for p in pkgs {
                banner::row(&p, "", true);
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

fn packages_dir() -> PathBuf {
    if let Ok(home) = std::env::var("ORION_PKGS") {
        return PathBuf::from(home);
    }
    if let Ok(home) = std::env::var("ORION_HOME") {
        return PathBuf::from(home).join("packages");
    }
    // Fallback: ~/.orion/packages
    dirs_home().join(".orion").join("packages")
}

fn dirs_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
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

fn list_installed(dir: &PathBuf) -> Vec<String> {
    if !dir.exists() { return vec![]; }
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}
