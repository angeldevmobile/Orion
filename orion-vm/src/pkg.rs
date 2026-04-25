//! Package manager para Orion — `orion --add`, `--remove`, `--list`, `--search`, `--update`
//!
//! Porta la lógica de orion/pkg.py a Rust.
//!
//! Esquema registry.json:
//!   { "_meta": { "registry": "<base_url>", ... },
//!     "packages": { "<name>": { "version", "description", "file", "type", "tags" } } }
//!
//! Esquema installed.json:
//!   { "<name>": { "version", "description", "file", "source" } }

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const REGISTRY_REMOTE: &str =
    "https://raw.githubusercontent.com/angeldevmobile/Orion/master/packages/registry.json";
const PKG_REMOTE_BASE: &str =
    "https://raw.githubusercontent.com/angeldevmobile/Orion/master/packages";

// ── Rutas ─────────────────────────────────────────────────────────────────────

fn packages_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe
            .parent().unwrap_or(Path::new("."))
            .parent().unwrap_or(Path::new("."))
            .parent().unwrap_or(Path::new("."))
            .join("packages");
        if candidate.exists() {
            return candidate;
        }
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("packages")
}

fn registry_path() -> PathBuf  { packages_dir().join("registry.json") }
fn installed_path() -> PathBuf { packages_dir().join("installed.json") }

// ── Structuras internas ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct PkgEntry {
    version:     String,
    description: String,
    file:        String,
    pkg_type:    String,
    tags:        Vec<String>,
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// Carga el registry local. Si `refresh=true` intenta actualizar desde GitHub primero.
fn load_registry(refresh: bool) -> Result<(String, HashMap<String, PkgEntry>), String> {
    let local = registry_path();

    // Intentar refrescar desde remoto si se pide o no existe localmente
    if refresh || !local.exists() {
        match ureq::get(REGISTRY_REMOTE).call() {
            Ok(resp) => {
                if let Ok(body) = resp.into_string() {
                    let _ = fs::create_dir_all(local.parent().unwrap_or(Path::new(".")));
                    let _ = fs::write(&local, &body);
                }
            }
            Err(_) => {} // Sin conexión — seguir con local
        }
    }

    let raw = fs::read_to_string(&local)
        .map_err(|e| format!("No se pudo leer registry.json en {}: {}", local.display(), e))?;

    parse_registry(&raw)
}

fn parse_registry(raw: &str) -> Result<(String, HashMap<String, PkgEntry>), String> {
    let json: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| format!("registry.json malformado: {}", e))?;

    let base_url = json["_meta"]["registry"]
        .as_str()
        .unwrap_or(PKG_REMOTE_BASE)
        .to_string();

    let pkgs_obj = json["packages"].as_object()
        .ok_or("registry.json: campo 'packages' no es un objeto")?;

    let mut map = HashMap::new();
    for (name, val) in pkgs_obj {
        let tags = val["tags"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        map.insert(name.clone(), PkgEntry {
            version:     val["version"].as_str().unwrap_or("0.0.0").to_string(),
            description: val["description"].as_str().unwrap_or("").to_string(),
            file:        val["file"].as_str().unwrap_or(&format!("{}.orx", name)).to_string(),
            pkg_type:    val["type"].as_str().unwrap_or("community").to_string(),
            tags,
        });
    }
    Ok((base_url, map))
}

// ── installed.json ────────────────────────────────────────────────────────────

fn load_installed() -> HashMap<String, serde_json::Value> {
    let path = installed_path();
    match fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

fn save_installed(installed: &HashMap<String, serde_json::Value>) -> Result<(), String> {
    let path = installed_path();
    let json = serde_json::to_string_pretty(installed)
        .map_err(|e| format!("Error serializando installed.json: {}", e))?;
    fs::write(&path, json)
        .map_err(|e| format!("Error escribiendo {}: {}", path.display(), e))
}

// ── Descarga de .orx ─────────────────────────────────────────────────────────

fn download_orx(pkg_name: &str, file_name: &str) -> Result<String, String> {
    let url = format!("{}/{}", PKG_REMOTE_BASE, file_name);
    ureq::get(&url)
        .call()
        .map_err(|e| format!("Sin conexión o URL inválida para '{}': {}", pkg_name, e))?
        .into_string()
        .map_err(|e| format!("Error leyendo respuesta para '{}': {}", pkg_name, e))
}

// ── Lógica interna de instalación ─────────────────────────────────────────────

fn install_entry(name: &str, entry: &PkgEntry, force: bool) -> String {
    let dest = packages_dir().join(&entry.file);

    if dest.exists() && !force {
        // Archivo local ya presente (builtin o previo) — solo registrar
        let mut installed = load_installed();
        installed.insert(name.to_string(), serde_json::json!({
            "version":     entry.version,
            "description": entry.description,
            "file":        entry.file,
            "source":      entry.pkg_type,
        }));
        if let Err(e) = save_installed(&installed) {
            return format!("[error] {}", e);
        }
        return format!("[ok] {} v{} instalado  → packages/{}", name, entry.version, entry.file);
    }

    // Intentar descarga remota
    match download_orx(name, &entry.file) {
        Ok(body) => {
            if let Err(e) = fs::write(&dest, &body) {
                return format!("[error] No se pudo guardar {}: {}", dest.display(), e);
            }
        }
        Err(_) if dest.exists() => {
            // Sin red pero hay copia local
            let mut installed = load_installed();
            installed.insert(name.to_string(), serde_json::json!({
                "version":     entry.version,
                "description": entry.description,
                "file":        entry.file,
                "source":      "local",
            }));
            let _ = save_installed(&installed);
            return format!(
                "[ok] {} v{} instalado (sin conexión, versión local)",
                name, entry.version
            );
        }
        Err(e) => return format!("[error] '{}' no disponible localmente y sin conexión: {}", name, e),
    }

    let mut installed = load_installed();
    installed.insert(name.to_string(), serde_json::json!({
        "version":     entry.version,
        "description": entry.description,
        "file":        entry.file,
        "source":      "remote",
    }));
    if let Err(e) = save_installed(&installed) {
        return format!("[error] {}", e);
    }
    format!("[ok] {} v{} instalado  → packages/{}", name, entry.version, entry.file)
}

// ── orion --add <pkg> [--force] ───────────────────────────────────────────────

pub fn add_package(name: &str, force: bool) {
    let installed = load_installed();
    if installed.contains_key(name) && !force {
        let v = installed[name]["version"].as_str().unwrap_or("?");
        println!("[ya instalado] {} v{}  — usa --force para reinstalar", name, v);
        return;
    }

    // Intentar con registry local primero, luego remoto si no se encuentra
    let (_base, registry) = match load_registry(false) {
        Ok(r) => r,
        Err(e) => { eprintln!("[orion pkg] {}", e); std::process::exit(1); }
    };

    let entry = if let Some(e) = registry.get(name) {
        e.clone()
    } else {
        // Refrescar desde remoto
        let (_b, reg2) = match load_registry(true) {
            Ok(r) => r,
            Err(e) => { eprintln!("[orion pkg] {}", e); std::process::exit(1); }
        };
        match reg2.get(name) {
            Some(e) => e.clone(),
            None => {
                let available: Vec<_> = {
                    let mut v: Vec<_> = reg2.keys().collect();
                    v.sort();
                    v.iter().map(|s| s.as_str()).collect()
                };
                eprintln!("[orion pkg] Paquete '{}' no encontrado en el registry.", name);
                eprintln!("[orion pkg] Disponibles: {}", available.join(", "));
                std::process::exit(1);
            }
        }
    };

    let msg = install_entry(name, &entry, force);
    if msg.starts_with("[error]") {
        eprintln!("[orion pkg] {}", msg);
        std::process::exit(1);
    }
    println!("[orion pkg] {}", msg);
}

// ── orion --remove <pkg> ──────────────────────────────────────────────────────

pub fn remove_package(name: &str) {
    let mut installed = load_installed();

    if !installed.contains_key(name) {
        eprintln!("[orion pkg] '{}' no está instalado.", name);
        std::process::exit(1);
    }

    let source = installed[name]["source"].as_str().unwrap_or("").to_string();
    let file   = installed[name]["file"].as_str().unwrap_or("").to_string();

    installed.remove(name);
    if let Err(e) = save_installed(&installed) {
        eprintln!("[orion pkg] {}", e);
        std::process::exit(1);
    }

    // No borrar archivos builtin (vienen con Orion)
    if source == "builtin" {
        println!("[orion pkg] '{}' desregistrado (archivo builtin conservado).", name);
        return;
    }

    if !file.is_empty() {
        let path = packages_dir().join(&file);
        if path.exists() {
            if let Err(e) = fs::remove_file(&path) {
                eprintln!("[orion pkg] Advertencia: no se pudo eliminar {}: {}", path.display(), e);
            } else {
                println!("[orion pkg] '{}' desinstalado y archivo eliminado.", name);
                return;
            }
        }
    }
    println!("[orion pkg] '{}' desregistrado.", name);
}

// ── orion --list ──────────────────────────────────────────────────────────────

pub fn list_packages() {
    let (_base_url, registry) = match load_registry(false) {
        Ok(r) => r,
        Err(e) => { eprintln!("[orion pkg] {}", e); std::process::exit(1); }
    };
    let installed = load_installed();

    let mut names: Vec<&String> = registry.keys().collect();
    names.sort();

    println!();
    println!("  Paquetes Orion disponibles:");
    println!("  {:<14} {:<10} {:<12} {}", "NOMBRE", "VERSIÓN", "TIPO", "DESCRIPCIÓN");
    println!("  {}", "─".repeat(72));

    for name in names {
        let entry = &registry[name];
        let mark = if installed.contains_key(name) { "✓" } else { " " };
        println!(
            "  {} {:<13} {:<10} {:<12} {}",
            mark, name, entry.version, entry.pkg_type, entry.description
        );
    }
    println!();
    println!("  ✓ = instalado   |   Instalar: orion --add <paquete>");
    println!();
}

// ── orion --search <query> ────────────────────────────────────────────────────

pub fn search_packages(query: &str) {
    let (_base, registry) = match load_registry(false) {
        Ok(r) => r,
        Err(e) => { eprintln!("[orion pkg] {}", e); std::process::exit(1); }
    };

    let q = query.to_lowercase();
    let mut results: Vec<(i32, &String, &PkgEntry)> = registry.iter()
        .filter_map(|(name, entry)| {
            let mut score: i32 = 0;
            if name.to_lowercase().contains(&q)               { score += 10; }
            if entry.description.to_lowercase().contains(&q)  { score += 5; }
            for tag in &entry.tags {
                if tag.to_lowercase().contains(&q) { score += 3; }
            }
            if score > 0 { Some((score, name, entry)) } else { None }
        })
        .collect();

    results.sort_by(|a, b| b.0.cmp(&a.0));

    if results.is_empty() {
        println!("[orion pkg] Sin resultados para '{}'.", query);
        return;
    }

    let installed = load_installed();
    println!();
    println!("  Resultados para '{}':", query);
    println!("  {:<14} {:<10} {}", "NOMBRE", "VERSIÓN", "DESCRIPCIÓN");
    println!("  {}", "─".repeat(60));
    for (_, name, entry) in &results {
        let mark = if installed.contains_key(*name) { "✓" } else { " " };
        println!("  {} {:<13} {:<10} {}", mark, name, entry.version, entry.description);
    }
    println!();
}

// ── orion --update [pkg] ──────────────────────────────────────────────────────

pub fn update_packages(pkg_name: Option<&str>) {
    let installed = load_installed();
    if installed.is_empty() {
        println!("[orion pkg] No hay paquetes instalados.");
        return;
    }

    let (_base, registry) = match load_registry(true) {
        Ok(r) => r,
        Err(e) => { eprintln!("[orion pkg] {}", e); std::process::exit(1); }
    };

    let targets: Vec<String> = match pkg_name {
        Some(n) => vec![n.to_string()],
        None    => { let mut v: Vec<_> = installed.keys().cloned().collect(); v.sort(); v }
    };

    for name in &targets {
        if !installed.contains_key(name.as_str()) {
            eprintln!("[orion pkg] '{}' no está instalado.", name);
            continue;
        }
        match registry.get(name.as_str()) {
            None => eprintln!("[orion pkg] '{}' no está en el registry.", name),
            Some(entry) => {
                let msg = install_entry(name, entry, true);
                println!("[orion pkg] {}", msg);
            }
        }
    }
}
