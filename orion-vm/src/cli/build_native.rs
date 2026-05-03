//! `orion --build <archivo.orx> [-o salida]`
//!
//! Pipeline completo:
//!   1. Lex → Parse → Codegen → bytecode (JSON)
//!   2. cranelift-object → object file (.o/.obj) con main() + bytecode embebido
//!   3. cargo build --lib → staticlib de orion_vm (en caché)
//!   4. Linker del sistema → ejecutable nativo standalone

use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use super::banner;

pub fn run_build(src_path: &str, output: Option<&str>) {
    banner::section("Compilación nativa AOT");

    // ── 1. Lex → Parse → Codegen ─────────────────────────────────────────
    let src = read_src(src_path);

    let tokens = match crate::lexer::lex(&src) {
        Ok(t) => t,
        Err(e) => {
            banner::fail(&format!("Error léxico en {src_path}:{}: {}", e.line, e.message));
            std::process::exit(1);
        }
    };

    let ast = match crate::parser::parse(tokens) {
        Ok(a) => a,
        Err(e) => {
            banner::fail(&format!("Error de sintaxis en {src_path}:{}: {}", e.line, e.message));
            std::process::exit(1);
        }
    };

    let bc = match crate::codegen::compile(ast) {
        Ok(b) => b,
        Err(e) => {
            banner::fail(&format!("Error de codegen: {}", e.message));
            std::process::exit(1);
        }
    };

    let bc_bytes = match serde_json::to_vec(&bc) {
        Ok(b) => b,
        Err(e) => {
            banner::fail(&format!("Error serializando bytecode: {e}"));
            std::process::exit(1);
        }
    };

    banner::ok(&format!("Bytecode: {} bytes", bc_bytes.len()));

    // ── 2. cranelift-object → .o ─────────────────────────────────────────
    let obj_bytes = match crate::aot::compile_to_object(&bc_bytes) {
        Ok(b) => b,
        Err(e) => {
            banner::fail(&format!("Error AOT (cranelift-object): {e}"));
            std::process::exit(1);
        }
    };

    let tmp_dir = std::env::temp_dir().join("orion_build");
    fs::create_dir_all(&tmp_dir).ok();

    let stem = Path::new(src_path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let obj_ext  = if cfg!(windows) { "obj" } else { "o" };
    let obj_path = tmp_dir.join(format!("{stem}.{obj_ext}"));

    if let Err(e) = fs::write(&obj_path, &obj_bytes) {
        banner::fail(&format!("Error escribiendo objeto: {e}"));
        std::process::exit(1);
    }

    banner::ok(&format!("Objeto:   {}", obj_path.display()));

    // ── 3. Staticlib de orion_vm (con caché) ─────────────────────────────
    let vm_dir   = locate_vm_crate();
    let lib_path = build_staticlib(&vm_dir, &tmp_dir);

    banner::ok(&format!("Runtime:  {}", lib_path.display()));

    // ── 4. Enlazar → ejecutable ──────────────────────────────────────────
    let exe_ext = if cfg!(windows) { ".exe" } else { "" };
    let out_path = output
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{stem}{exe_ext}")));

    link_native(&obj_path, &lib_path, &out_path);

    println!();
    banner::ok(&format!("Ejecutable: {}", out_path.display()));
    println!();
    println!("  Uso: {}", out_path.display());
    println!();
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn read_src(path: &str) -> String {
    match fs::read_to_string(path) {
        Ok(s) => s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string(),
        Err(e) => {
            banner::fail(&format!("No se puede leer '{path}': {e}"));
            std::process::exit(1);
        }
    }
}

/// Localiza el directorio de orion-vm relativo al ejecutable actual.
fn locate_vm_crate() -> PathBuf {
    // Buscar hacia arriba desde el ejecutable actual
    if let Ok(exe) = std::env::current_exe() {
        let mut p = exe.as_path();
        for _ in 0..6 {
            p = match p.parent() { Some(pp) => pp, None => break };
            let candidate = p.join("orion-vm");
            if candidate.join("Cargo.toml").exists() {
                return candidate;
            }
            // También probar el directorio actual
            let cwd_candidate = p.join("Cargo.toml");
            if cwd_candidate.exists() {
                // Estamos dentro del crate
                return p.to_path_buf();
            }
        }
    }
    // Fallback: directorio actual
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Construye la staticlib de orion_vm. Devuelve la ruta al .lib/.a.
/// En el primer build es lento (~30s), luego usa la caché de cargo.
fn build_staticlib(vm_dir: &Path, tmp_dir: &Path) -> PathBuf {
    // Verificar si ya existe en caché (el artefacto de cargo)
    let profile_dir = vm_dir.join("target").join("release");
    let lib_name    = if cfg!(windows) { "orion_vm.lib" } else { "liborion_vm.a" };
    let cached      = profile_dir.join(lib_name);

    if cached.exists() {
        return cached;
    }

    // Construir la staticlib con cargo
    banner::info("Compilando runtime de Orion (primera vez, puede tardar ~30s)...");

    let status = Command::new("cargo")
        .args(["build", "--lib", "--release"])
        .current_dir(vm_dir)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            banner::fail(&format!("cargo build --lib falló con código {:?}", s.code()));
            std::process::exit(1);
        }
        Err(e) => {
            banner::fail(&format!("No se pudo ejecutar cargo: {e}"));
            std::process::exit(1);
        }
    }

    // Intentar encontrar el artefacto en la caché de cargo (puede tener hash)
    let deps_dir = profile_dir.join("deps");
    if let Ok(entries) = fs::read_dir(&deps_dir) {
        let prefix = if cfg!(windows) { "orion_vm" } else { "liborion_vm" };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let ext  = if cfg!(windows) { ".lib" } else { ".a" };
            if name.starts_with(prefix) && name.ends_with(ext) {
                let src = entry.path();
                let dst = tmp_dir.join(lib_name);
                let _ = fs::copy(&src, &dst);
                return dst;
            }
        }
    }

    if cached.exists() { cached } else {
        banner::fail("No se encontró la staticlib de orion_vm tras compilar.");
        std::process::exit(1);
    }
}

/// Enlaza el objeto con la staticlib y produce el ejecutable final.
fn link_native(obj: &Path, lib: &Path, out: &Path) {
    banner::info("Enlazando...");

    // Determinar linker disponible
    let (linker, args) = detect_linker(obj, lib, out);

    let status = Command::new(&linker)
        .args(&args)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            banner::fail(&format!("Linker '{linker}' falló con código {:?}", s.code()));
            suggest_linker_fix();
            std::process::exit(1);
        }
        Err(e) => {
            banner::fail(&format!("No se pudo ejecutar el linker '{linker}': {e}"));
            suggest_linker_fix();
            std::process::exit(1);
        }
    }
}

/// Devuelve (linker, argumentos) para el sistema actual.
fn detect_linker(obj: &Path, lib: &Path, out: &Path) -> (String, Vec<String>) {
    let obj_s = obj.to_string_lossy().to_string();
    let lib_s = lib.to_string_lossy().to_string();
    let out_s = out.to_string_lossy().to_string();

    if cfg!(windows) {
        // Intentar link.exe (MSVC) primero
        if which("link").is_some() {
            return (
                "link".to_string(),
                vec![
                    obj_s,
                    lib_s,
                    format!("/OUT:{out_s}"),
                    "/SUBSYSTEM:CONSOLE".to_string(),
                    "/DEFAULTLIB:msvcrt.lib".to_string(),
                    "/NOLOGO".to_string(),
                ],
            );
        }
        // Fallback: gcc (MinGW)
        (
            "gcc".to_string(),
            vec![obj_s, lib_s, "-o".to_string(), out_s, "-lws2_32".to_string()],
        )
    } else {
        // Linux / macOS: usar cc (wrapper del compilador del sistema)
        let linker = if which("cc").is_some() { "cc" } else { "gcc" };
        (
            linker.to_string(),
            vec![
                obj_s,
                lib_s,
                "-o".to_string(),
                out_s,
                "-lpthread".to_string(),
                "-ldl".to_string(),
                "-lm".to_string(),
            ],
        )
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    std::env::var_os("PATH")
        .iter()
        .flat_map(|path| std::env::split_paths(path))
        .map(|p| p.join(format!("{name}{ext}")))
        .find(|p| p.exists())
}

fn suggest_linker_fix() {
    if cfg!(windows) {
        eprintln!("  Instala Visual Studio Build Tools o MinGW y asegúrate de que link.exe o gcc estén en PATH.");
    } else {
        eprintln!("  Instala gcc o clang: sudo apt install gcc  (Ubuntu/Debian)");
    }
}
