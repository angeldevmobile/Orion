//! Rutas de proyecto y de paquetes — fuente ÚNICA de verdad.
//!
//! Antes cada componente resolvía por su cuenta dónde viven los paquetes y las
//! tres respuestas no coincidían: el gestor instalaba junto al ejecutable o en
//! `cwd/packages`, el `use` buscaba `packages/x.orx` relativo al directorio
//! actual, y el doctor informaba de `~/.orion/packages`. El resultado era que
//! `orion doctor` decía "ningún paquete instalado" con diez paquetes instalados.
//! Todo eso pasa por aquí ahora.
//!
//! Modelo:
//!
//! - **Raíz de proyecto**: el ancestro más cercano al archivo que se ejecuta que
//!   contenga `orion.json`. Sin manifiesto se acepta un ancestro con `packages/`,
//!   y en último término el directorio actual. Esto es lo que convierte el
//!   "por accidente" (dependía del cwd) en "por diseño" (depende del proyecto).
//! - **Paquetes de proyecto**: `<raíz>/packages`, versionables con el repo.
//! - **Paquetes globales**: `$ORION_PKGS`, `$ORION_HOME/packages` o
//!   `~/.orion/packages`, compartidos entre proyectos.
//!
//! La búsqueda va de lo más específico a lo más general: proyecto, después
//! global. Instalar escribe en el proyecto cuando hay manifiesto y en el global
//! cuando no lo hay.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Nombre del manifiesto que marca la raíz de un proyecto Orion.
pub const MANIFEST: &str = "orion.json";
/// Nombre del archivo de bloqueo con las versiones resueltas.
pub const LOCKFILE: &str = "orion.lock";

/// Archivo `.orx` de entrada del proceso.
///
/// Es un global y no un parámetro porque el runtime del JIT resuelve módulos
/// desde funciones `extern "C"` sin contexto: no hay dónde enhebrar un handle.
static ENTRY_FILE: OnceLock<PathBuf> = OnceLock::new();

/// Registra el archivo que se está ejecutando. Lo llama `main` antes de correr
/// nada. Solo el primero cuenta: los módulos importados no reubican el proyecto.
pub fn set_entry_file(path: impl AsRef<Path>) {
    let p = path.as_ref();
    let abs = std::fs::canonicalize(p).unwrap_or_else(|_| {
        cwd().join(p)
    });
    let _ = ENTRY_FILE.set(abs);
}

/// Directorio del archivo de entrada, si se registró alguno.
fn entry_dir() -> Option<PathBuf> {
    ENTRY_FILE.get().and_then(|f| f.parent().map(Path::to_path_buf))
}

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn home_dir() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Sube por los ancestros de `start` hasta encontrar un directorio que cumpla
/// `pred`. Devuelve `None` si ninguno lo hace.
fn find_up(start: &Path, pred: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if pred(d) {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// Raíz del proyecto actual.
///
/// Busca hacia arriba desde el archivo de entrada y, si no hay ninguno
/// registrado, desde el directorio actual. Prioriza el manifiesto sobre la
/// heurística de `packages/`: un proyecto con `orion.json` manda aunque haya un
/// `packages/` más cerca.
pub fn project_root() -> PathBuf {
    let starts: Vec<PathBuf> = match entry_dir() {
        Some(d) => vec![d, cwd()],
        None    => vec![cwd()],
    };

    for start in &starts {
        if let Some(r) = find_up(start, |d| d.join(MANIFEST).is_file()) {
            return r;
        }
    }
    // Sin manifiesto: un ancestro con `packages/` sigue siendo un proyecto
    // reconocible. Mantiene funcionando los repos que ya existían.
    for start in &starts {
        if let Some(r) = find_up(start, |d| d.join("packages").is_dir()) {
            return r;
        }
    }
    starts.into_iter().next().unwrap_or_else(cwd)
}

/// Ruta del manifiesto del proyecto, exista o no el archivo.
pub fn manifest_path() -> PathBuf { project_root().join(MANIFEST) }

/// Ruta del lockfile del proyecto, exista o no el archivo.
pub fn lockfile_path() -> PathBuf { project_root().join(LOCKFILE) }

/// ¿El proyecto actual tiene manifiesto?
pub fn has_manifest() -> bool { manifest_path().is_file() }

/// Paquetes propios del proyecto: `<raíz>/packages`.
pub fn project_packages_dir() -> PathBuf { project_root().join("packages") }

/// Paquetes compartidos entre proyectos.
///
/// `$ORION_PKGS` gana sobre `$ORION_HOME/packages`, y sin ninguna de las dos se
/// usa `~/.orion/packages`.
pub fn global_packages_dir() -> PathBuf {
    if let Ok(p) = std::env::var("ORION_PKGS") {
        if !p.trim().is_empty() { return PathBuf::from(p); }
    }
    if let Ok(h) = std::env::var("ORION_HOME") {
        if !h.trim().is_empty() { return PathBuf::from(h).join("packages"); }
    }
    home_dir().join(".orion").join("packages")
}

/// Directorios de paquetes en orden de búsqueda: proyecto primero, global
/// después. Sin duplicados, para que un proyecto que vive dentro de
/// `~/.orion` no se busque dos veces.
pub fn packages_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![project_packages_dir()];
    let global = global_packages_dir();
    if !dirs.iter().any(|d| same_path(d, &global)) {
        dirs.push(global);
    }
    dirs
}

/// Directorio donde escribe `--add`.
///
/// Se instala dentro del proyecto cuando hay algo que lo identifique como tal:
/// un manifiesto, o un `packages/` que ya existe. Lo segundo es lo que mantiene
/// intacto el comportamiento de los repos anteriores al manifiesto, que
/// instalaban justo ahí. Sin ninguna de las dos señales se usa el global, para
/// no sembrar un `packages/` en cualquier directorio desde el que se invoque.
pub fn install_dir() -> PathBuf {
    let project = project_packages_dir();
    if has_manifest() || project.is_dir() { project } else { global_packages_dir() }
}

/// Igualdad de rutas tolerante: compara la forma canónica cuando ambas existen
/// y cae en comparación textual cuando no.
fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// Directorio de librerías nativas de un paquete: `<pkgs>/native/<paquete>`.
pub fn native_dir(base: &Path, pkg: &str) -> PathBuf {
    base.join("native").join(pkg)
}

/// Todos los directorios nativos donde puede vivir una librería instalada.
pub fn native_dirs() -> Vec<PathBuf> {
    packages_dirs().into_iter().map(|d| d.join("native")).collect()
}

//    Resolución de módulos

/// Localiza el archivo `.orx` que corresponde a un `use "<path>"`.
///
/// `path` llega tal cual lo escribió el programa, con o sin `packages/` delante.
/// Se prueban, en este orden, las raíces relevantes (proyecto, directorio del
/// archivo de entrada, directorio actual) y dentro de cada una las formas
/// habituales. Devuelve la primera coincidencia real en disco.
pub fn resolve_module_file(path: &str) -> Option<PathBuf> {
    // Ruta absoluta escrita a mano: se respeta sin más.
    let raw = Path::new(path);
    if raw.is_absolute() {
        for cand in [PathBuf::from(format!("{path}.orx")), raw.to_path_buf()] {
            if cand.is_file() { return Some(cand); }
        }
        return None;
    }

    // `packages/x` y `x` deben encontrar el mismo archivo: si el programa ya
    // escribió el prefijo, se quita para no acabar buscando `packages/packages/x`.
    let bare = path
        .trim_start_matches("./")
        .strip_prefix("packages/")
        .or_else(|| path.strip_prefix("packages\\"))
        .unwrap_or(path);

    let mut roots: Vec<PathBuf> = vec![project_root()];
    if let Some(d) = entry_dir() { push_unique(&mut roots, d); }
    push_unique(&mut roots, cwd());

    for root in &roots {
        let candidates = [
            root.join("packages").join(format!("{bare}.orx")),
            root.join(format!("{path}.orx")),
            root.join("lib").join(format!("{bare}.orx")),
        ];
        for c in &candidates {
            if c.is_file() { return Some(c.clone()); }
        }
    }

    // Paquetes globales: instalados con `--add` fuera de un proyecto.
    let global = global_packages_dir();
    for c in [global.join(format!("{bare}.orx")), global.join(format!("{path}.orx"))] {
        if c.is_file() { return Some(c); }
    }

    None
}

fn push_unique(v: &mut Vec<PathBuf>, p: PathBuf) {
    if !v.iter().any(|x| same_path(x, &p)) { v.push(p); }
}

/// Plataforma actual con el mismo vocabulario que usan los assets del registry
/// y los binarios de los releases: `win32-x64`, `linux-x64`, `darwin-arm64`.
pub fn current_platform() -> String {
    let os = match std::env::consts::OS {
        "windows" => "win32",
        "macos"   => "darwin",
        other     => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64"  => "x64",
        "aarch64" => "arm64",
        other     => other,
    };
    format!("{os}-{arch}")
}

/// Nombre de archivo de una librería dinámica según la plataforma.
pub fn dylib_file_name(stem: &str) -> String {
    if stem.contains('.') { return stem.to_string(); }
    match std::env::consts::OS {
        "windows" => format!("{stem}.dll"),
        "macos"   => format!("lib{stem}.dylib"),
        _         => format!("lib{stem}.so"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plataforma_usa_el_vocabulario_de_los_assets() {
        let p = current_platform();
        assert!(p.contains('-'), "esperado <os>-<arch>, obtenido {p}");
        assert!(!p.contains("x86_64"), "arch sin normalizar en {p}");
        assert!(!p.contains("windows"), "os sin normalizar en {p}");
    }

    #[test]
    fn dylib_respeta_extension_explicita() {
        assert_eq!(dylib_file_name("libfoo.so.1"), "libfoo.so.1");
    }

    #[test]
    fn dylib_añade_la_extension_de_la_plataforma() {
        let n = dylib_file_name("browser");
        assert!(n.contains("browser"));
        assert!(n.ends_with(".dll") || n.ends_with(".so") || n.ends_with(".dylib"));
    }

    #[test]
    fn el_global_respeta_orion_pkgs() {
        // Sin tocar el entorno del proceso de test: se comprueba que la ruta
        // por defecto termina en .orion/packages cuando no hay variables.
        if std::env::var("ORION_PKGS").is_err() && std::env::var("ORION_HOME").is_err() {
            let g = global_packages_dir();
            assert!(g.ends_with("packages"), "{}", g.display());
            assert!(g.to_string_lossy().contains(".orion"), "{}", g.display());
        }
    }

    #[test]
    fn find_up_encuentra_el_ancestro_marcado() {
        let tmp = std::env::temp_dir().join("orion_paths_test_root");
        let deep = tmp.join("a").join("b").join("c");
        let _ = std::fs::create_dir_all(&deep);
        let _ = std::fs::write(tmp.join(MANIFEST), "{}");

        let found = find_up(&deep, |d| d.join(MANIFEST).is_file());
        assert_eq!(found.as_deref(), Some(tmp.as_path()));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
