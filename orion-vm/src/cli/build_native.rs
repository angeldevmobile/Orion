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
    banner::section("Native AOT build");

    //   1. Lex → Parse → Codegen                      
    let src = read_src(src_path);

    let tokens = match crate::lexer::lex(&src) {
        Ok(t) => t,
        Err(e) => {
            banner::fail(&format!("Lexical error in {src_path}:{}: {}", e.line, e.message));
            std::process::exit(1);
        }
    };

    let ast = match crate::parser::parse(tokens) {
        Ok(a) => a,
        Err(e) => {
            banner::fail(&format!("Syntax error in {src_path}:{}: {}", e.line, e.message));
            std::process::exit(1);
        }
    };

    let bc = match crate::codegen::compile_entry(ast) {
        Ok(b) => b,
        Err(e) => {
            banner::fail(&format!("Codegen error: {}", e.message));
            std::process::exit(1);
        }
    };

    //   2. cranelift-object → .orx

    let obj_bytes = match crate::jit::aot_backend::compile_to_native_object(&bc) {
        Ok(Some(b)) => {
            banner::ok("Code:     native (Cranelift)");
            b
        }
        Ok(None) => {
            banner::info("The program uses constructs with no native support; embedding the bytecode instead.");
            build_bundle_object(&bc)
        }
        Err(e) => {
            banner::info(&format!("Native build unavailable ({e}); embedding the bytecode instead."));
            build_bundle_object(&bc)
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
        banner::fail(&format!("Error writing object file: {e}"));
        std::process::exit(1);
    }

    banner::ok(&format!("Objeto:   {}", obj_path.display()));

    //   3. Staticlib de orion_vm (con caché)                
    let vm_dir   = locate_vm_crate();
    let lib_path = build_staticlib(&vm_dir, &tmp_dir);

    banner::ok(&format!("Runtime:  {}", lib_path.display()));

    //   4. Enlazar → ejecutable                      
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

//   Helpers

fn build_bundle_object(bc: &crate::bytecode::OrionBytecode) -> Vec<u8> {
    let bc_bytes = match serde_json::to_vec(bc) {
        Ok(b) => b,
        Err(e) => {
            banner::fail(&format!("Error serializing bytecode: {e}"));
            std::process::exit(1);
        }
    };
    banner::ok(&format!("Bytecode: {} bytes", bc_bytes.len()));

    match crate::aot::compile_to_object(&bc_bytes) {
        Ok(b) => b,
        Err(e) => {
            banner::fail(&format!("AOT error (cranelift-object): {e}"));
            std::process::exit(1);
        }
    }
}

fn read_src(path: &str) -> String {
    match fs::read_to_string(path) {
        Ok(s) => s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string(),
        Err(e) => {
            banner::fail(&format!("Cannot read '{path}': {e}"));
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
    let profile_dir = vm_dir.join("target").join("release");
    let lib_name    = if cfg!(windows) { "orion_vm.lib" } else { "liborion_vm.a" };
    let cached      = profile_dir.join(lib_name);

    if !cached.exists() {
        banner::info("Building the Orion runtime (first time, may take ~30s)...");
    }

    let status = Command::new("cargo")
        .args(["build", "--lib", "--release"])
        .current_dir(vm_dir)
        .status();

    match status {
        Ok(s) if s.success() => {}
        _ if cached.exists() => {
            banner::info("Could not rebuild the runtime; using the existing staticlib.");
            return cached;
        }
        Ok(s) => {
            banner::fail(&format!("cargo build --lib failed with code {:?}", s.code()));
            std::process::exit(1);
        }
        Err(e) => {
            banner::fail(&format!("Could not run cargo: {e}"));
            std::process::exit(1);
        }
    }

    if cached.exists() {
        return cached;
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
        banner::fail("Could not find the orion_vm staticlib after building.");
        std::process::exit(1);
    }
}

/// Enlaza el objeto con la staticlib y produce el ejecutable final.
fn link_native(obj: &Path, lib: &Path, out: &Path) {
    banner::info("Enlazando...");

    // Determinar linker disponible
    let (linker, args, env) = detect_linker(obj, lib, out);

    let mut cmd = Command::new(&linker);
    cmd.args(&args);
    for (k, v) in &env {
        cmd.env(k, v);
    }
    let status = cmd.status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            banner::fail(&format!("Linker '{linker}' failed with code {:?}", s.code()));
            suggest_linker_fix();
            std::process::exit(1);
        }
        Err(e) => {
            banner::fail(&format!("Could not run the linker '{linker}': {e}"));
            suggest_linker_fix();
            std::process::exit(1);
        }
    }
}

/// Librerías de importación de Windows que necesita el runtime bundleado.
const WINDOWS_SYSTEM_LIBS: &[&str] = &[
    "kernel32", "user32", "gdi32", "advapi32", "shell32",
    "ole32", "oleaut32", "uuid", "shcore", "propsys", "comdlg32",
    "ws2_32", "bcrypt", "crypt32", "secur32", "ncrypt",
    "opengl32", "dwmapi", "setupapi", "cfgmgr32", "imm32",
    "winmm", "version", "msimg32", "dbghelp", "userenv", "ntdll", "winspool",
    // Accesibilidad (accesskit), WinRT (windows_result), rutas y shell
    // (arboard/webbrowser): uiautomationcore/oleacc, runtimeobject, pathcch, shlwapi.
    "uiautomationcore", "oleacc", "runtimeobject", "pathcch", "shlwapi",
    "uxtheme", // winit dark_mode → SetWindowTheme
];

/// Devuelve (linker, argumentos, variables de entorno) para el sistema actual.
fn detect_linker(obj: &Path, lib: &Path, out: &Path) -> (String, Vec<String>, Vec<(String, String)>) {
    let obj_s = obj.to_string_lossy().to_string();
    let lib_s = lib.to_string_lossy().to_string();
    let out_s = out.to_string_lossy().to_string();

    if cfg!(windows) {
        let msvc_args = |mut args: Vec<String>| -> Vec<String> {
            args.extend([
                format!("/OUT:{out_s}"),
                "/SUBSYSTEM:CONSOLE".to_string(),
                "/NOLOGO".to_string(),
                "/DEFAULTLIB:msvcrt.lib".to_string(),
            ]);
            for l in WINDOWS_SYSTEM_LIBS {
                args.push(format!("/DEFAULTLIB:{l}.lib"));
            }
            args
        };
        let base = vec![obj_s.clone(), lib_s.clone()];

        if std::env::var_os("LIB").is_some() && which("link").is_some() {
            return ("link".to_string(), msvc_args(base), Vec::new());
        }

        // 2) Shell normal: resolver la toolchain nosotros.
        if let Some(msvc) = find_msvc_toolchain() {
            let mut env = vec![("LIB".to_string(), msvc.lib_paths.join(";"))];
            if let Some(bin) = msvc.link_exe.parent() {
                let path = std::env::var("PATH").unwrap_or_default();
                env.push(("PATH".to_string(), format!("{};{}", bin.display(), path)));
            }
            return (
                msvc.link_exe.to_string_lossy().to_string(),
                msvc_args(base),
                env,
            );
        }

        // 3) Fallback: gcc (MinGW) — mismas libs de sistema en forma -l<lib>
        let mut args = vec![obj_s, lib_s, "-o".to_string(), out_s];
        for l in WINDOWS_SYSTEM_LIBS {
            args.push(format!("-l{l}"));
        }
        ("gcc".to_string(), args, Vec::new())
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
            Vec::new(),
        )
    }
}

/// Toolchain de MSVC resuelta: el linker y las rutas de librerías que necesita.
struct MsvcToolchain {
    link_exe:  PathBuf,
    lib_paths: Vec<String>,
}

/// Arquitectura del host en la nomenclatura de directorios de MSVC.
fn msvc_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" }
}

/// Localiza link.exe de MSVC y las rutas de LIB (VC Tools + Windows SDK).
fn find_msvc_toolchain() -> Option<MsvcToolchain> {
    let arch = msvc_arch();

    let vs_root = find_vs_install()?;
    let ver = fs::read_to_string(
        vs_root.join("VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt"),
    ).ok()?;
    let ver = ver.trim();

    let tools    = vs_root.join("VC/Tools/MSVC").join(ver);
    let link_exe = tools.join(format!("bin/Host{arch}/{arch}/link.exe"));
    if !link_exe.exists() { return None; }

    let mut lib_paths = Vec::new();
    let vc_lib = tools.join("lib").join(arch);
    if vc_lib.exists() {
        lib_paths.push(vc_lib.to_string_lossy().to_string());
    }
    if let Some((sdk_root, sdk_ver)) = find_windows_sdk() {
        for part in ["um", "ucrt"] {
            let p = sdk_root.join("Lib").join(&sdk_ver).join(part).join(arch);
            if p.exists() {
                lib_paths.push(p.to_string_lossy().to_string());
            }
        }
    }
    if lib_paths.is_empty() { return None; }

    Some(MsvcToolchain { link_exe, lib_paths })
}

/// Directorio de instalación de Visual Studio, vía vswhere o rutas conocidas.
fn find_vs_install() -> Option<PathBuf> {
    let pf86 = std::env::var("ProgramFiles(x86)")
        .unwrap_or_else(|_| "C:/Program Files (x86)".to_string());

    let vswhere = PathBuf::from(&pf86).join("Microsoft Visual Studio/Installer/vswhere.exe");
    if vswhere.exists() {
        let out = Command::new(&vswhere)
            .args([
                "-latest", "-products", "*",
                "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                "-property", "installationPath",
                "-utf8",
            ])
            .output();
        if let Ok(o) = out {
            let path = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !path.is_empty() && Path::new(&path).exists() {
                return Some(PathBuf::from(path));
            }
        }
    }

    // vswhere ausente o sin resultados: rutas de instalación por defecto.
    let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:/Program Files".to_string());
    for root in [pf.as_str(), pf86.as_str()] {
        for year in ["2022", "2019"] {
            for ed in ["Enterprise", "Professional", "Community", "BuildTools"] {
                let p = PathBuf::from(root).join("Microsoft Visual Studio").join(year).join(ed);
                if p.join("VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt").exists() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Raíz y versión del Windows SDK más reciente que tenga las libs del host.
fn find_windows_sdk() -> Option<(PathBuf, String)> {
    let pf86 = std::env::var("ProgramFiles(x86)")
        .unwrap_or_else(|_| "C:/Program Files (x86)".to_string());
    let root = PathBuf::from(pf86).join("Windows Kits/10");

    let mut versions: Vec<String> = fs::read_dir(root.join("Lib")).ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|v| root.join("Lib").join(v).join("um").join(msvc_arch()).exists())
        .collect();

    versions.sort_by_key(|v| {
        v.split('.').map(|n| n.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>()
    });
    versions.pop().map(|v| (root, v))
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
        eprintln!("  Install Visual Studio Build Tools or MinGW and make sure link.exe or gcc is on PATH.");
    } else {
        eprintln!("  Instala gcc o clang: sudo apt install gcc  (Ubuntu/Debian)");
    }
}
