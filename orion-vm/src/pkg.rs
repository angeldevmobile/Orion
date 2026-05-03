//! Package manager para Orion — `orion --add`, `--remove`, `--list`, `--search`, `--update`, `--publish`
//!
//! Esquema registry.json:
//!   { "_meta": { "registry": "<base_url>", ... },
//!     "packages": { "<name>": { "version", "description", "file", "type", "author", "tags" } } }
//!
//! Esquema installed.json:
//!   { "<name>": { "version", "description", "file", "source" } }
//!
//! Esquema orion.json (manifiesto de publicación):
//!   { "name", "version", "description", "author", "tags", "file", "license" }

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

const REGISTRY_REMOTE: &str =
    "https://raw.githubusercontent.com/angeldevmobile/Orion/master/packages/registry.json";
const PKG_REMOTE_BASE: &str =
    "https://raw.githubusercontent.com/angeldevmobile/Orion/master/packages";

//    Rutas                                                                      

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

//    Structuras internas                                                        

#[derive(Debug, Clone)]
struct PkgEntry {
    version:     String,
    description: String,
    file:        String,
    pkg_type:    String,
    tags:        Vec<String>,
}

//    Registry                                                                   

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

//    installed.json                                                             

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

//    Descarga de .orx                                                          

fn download_orx(pkg_name: &str, file_name: &str) -> Result<String, String> {
    let url = format!("{}/{}", PKG_REMOTE_BASE, file_name);
    ureq::get(&url)
        .call()
        .map_err(|e| format!("Sin conexión o URL inválida para '{}': {}", pkg_name, e))?
        .into_string()
        .map_err(|e| format!("Error leyendo respuesta para '{}': {}", pkg_name, e))
}

//    Lógica interna de instalación                                              

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

//    orion --add <pkg> [--force]                                                

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

//    orion --remove <pkg>                                                       

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

//    orion --list                                                               

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
    println!("  {}", " ".repeat(72));

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

//    orion --search <query>                                                     

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
    println!("  {}", " ".repeat(60));
    for (_, name, entry) in &results {
        let mark = if installed.contains_key(*name) { "✓" } else { " " };
        println!("  {} {:<13} {:<10} {}", mark, name, entry.version, entry.description);
    }
    println!();
}

//    orion --update [pkg]                                                       

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

//    orion --publish ────────────────────────────────────────────────────────────

const GITHUB_API:    &str = "https://api.github.com";
const REPO_OWNER:    &str = "angeldevmobile";
const REPO_NAME:     &str = "Orion";
const REGISTRY_PATH: &str = "packages/registry.json";

struct PackageManifest {
    name:        String,
    version:     String,
    description: String,
    author:      String,
    tags:        Vec<String>,
    file:        String,
    license:     String,
}

fn read_manifest() -> Result<PackageManifest, String> {
    let raw = fs::read_to_string("orion.json")
        .map_err(|_| concat!(
            "No se encontró orion.json en el directorio actual.\n",
            "  Crea uno con los campos: name, version, description, author, tags, file, license"
        ).to_string())?;

    let json: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("orion.json malformado: {e}"))?;

    let req = |field: &str| -> Result<String, String> {
        json[field].as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("orion.json: campo requerido '{field}' faltante"))
    };

    let name    = req("name")?;
    let version = req("version")?;
    let desc    = req("description")?;

    Ok(PackageManifest {
        file: json["file"].as_str()
            .map(str::to_string)
            .unwrap_or_else(|| format!("{name}.orx")),
        author:  json["author"].as_str().unwrap_or("").to_string(),
        license: json["license"].as_str().unwrap_or("MIT").to_string(),
        tags:    json["tags"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        name, version, description: desc,
    })
}

// ── Helpers de GitHub API ────────────────────────────────────────────────────

fn gh_get(url: &str, token: &str) -> Result<serde_json::Value, String> {
    ureq::get(url)
        .set("Authorization", &format!("token {token}"))
        .set("User-Agent", "orion-lang/publish")
        .set("Accept", "application/vnd.github.v3+json")
        .call()
        .map_err(|e| format!("GitHub GET {url}: {e}"))?
        .into_json::<serde_json::Value>()
        .map_err(|e| format!("Respuesta inválida de GitHub: {e}"))
}

fn gh_put(url: &str, token: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    ureq::put(url)
        .set("Authorization", &format!("token {token}"))
        .set("User-Agent", "orion-lang/publish")
        .set("Accept", "application/vnd.github.v3+json")
        .send_json(body.clone())
        .map_err(|e| format!("GitHub PUT {url}: {e}"))?
        .into_json::<serde_json::Value>()
        .map_err(|e| format!("Respuesta inválida de GitHub: {e}"))
}

fn gh_post(url: &str, token: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    ureq::post(url)
        .set("Authorization", &format!("token {token}"))
        .set("User-Agent", "orion-lang/publish")
        .set("Accept", "application/vnd.github.v3+json")
        .send_json(body.clone())
        .map_err(|e| format!("GitHub POST {url}: {e}"))?
        .into_json::<serde_json::Value>()
        .map_err(|e| format!("Respuesta inválida de GitHub: {e}"))
}

/// Crea una rama; si ya existe (422) la reutiliza sin error.
fn gh_create_branch(api_base: &str, token: &str, branch: &str, sha: &str) -> Result<(), String> {
    let url = format!("{api_base}/git/refs");
    match ureq::post(&url)
        .set("Authorization", &format!("token {token}"))
        .set("User-Agent", "orion-lang/publish")
        .set("Accept", "application/vnd.github.v3+json")
        .send_json(serde_json::json!({
            "ref": format!("refs/heads/{branch}"),
            "sha": sha,
        })) {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(422, _)) => Ok(()), // rama ya existía
        Err(e) => Err(format!("GitHub POST {url}: {e}")),
    }
}

// ── Función pública ──────────────────────────────────────────────────────────

pub fn publish_package() {
    let m = match read_manifest() {
        Ok(m) => m,
        Err(e) => { eprintln!("[orion publish] {e}"); std::process::exit(1); }
    };

    // Validar nombre (solo lowercase, dígitos y guiones)
    if !m.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        eprintln!("[orion publish] Nombre inválido '{}'. Usa solo letras minúsculas, números y guiones.", m.name);
        std::process::exit(1);
    }

    // Leer el archivo .orx
    let orx_src = match fs::read_to_string(&m.file) {
        Ok(s) => s,
        Err(e) => { eprintln!("[orion publish] No se pudo leer '{}': {e}", m.file); std::process::exit(1); }
    };

    // Token de GitHub
    let token = match std::env::var("ORION_GITHUB_TOKEN") {
        Ok(t) if !t.trim().is_empty() => t,
        _ => {
            eprintln!("[orion publish] Falta el token de GitHub.");
            eprintln!("  1. Crea uno en:  https://github.com/settings/tokens");
            eprintln!("     Permisos: repo (Contents read/write, Pull requests write)");
            eprintln!("  2. Configúralo:  $env:ORION_GITHUB_TOKEN = \"<token>\"");
            std::process::exit(1);
        }
    };

    let api_base = format!("{GITHUB_API}/repos/{REPO_OWNER}/{REPO_NAME}");

    println!("[orion publish] Publicando {} v{} ...", m.name, m.version);

    // 1. Obtener registry.json actual (contenido + SHA del blob)
    println!("[orion publish] Leyendo registry remoto ...");
    let reg_url = format!("{api_base}/contents/{REGISTRY_PATH}");
    let reg_resp = match gh_get(&reg_url, &token) {
        Ok(r) => r,
        Err(e) => { eprintln!("[orion publish] {e}"); std::process::exit(1); }
    };

    let reg_blob_sha = reg_resp["sha"].as_str().unwrap_or("").to_string();
    let reg_b64_raw  = reg_resp["content"].as_str().unwrap_or("");
    let reg_b64_clean: String = reg_b64_raw.chars().filter(|c| *c != '\n').collect();

    let reg_bytes = match B64.decode(&reg_b64_clean) {
        Ok(b) => b,
        Err(e) => { eprintln!("[orion publish] Error decodificando registry.json: {e}"); std::process::exit(1); }
    };

    let mut reg_json: serde_json::Value = match serde_json::from_slice(&reg_bytes) {
        Ok(j) => j,
        Err(e) => { eprintln!("[orion publish] registry.json malformado: {e}"); std::process::exit(1); }
    };

    // Verificar conflicto de versión
    if let Some(existing) = reg_json["packages"][&m.name].as_object() {
        let ev = existing.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");
        if ev == m.version {
            eprintln!("[orion publish] '{}' v{} ya está en el registry.", m.name, m.version);
            eprintln!("  Incrementa la versión en orion.json antes de publicar.");
            std::process::exit(1);
        }
        println!("[orion publish] Actualizando {} v{} → v{}", m.name, ev, m.version);
    }

    // Actualizar entrada en el registry
    reg_json["packages"][&m.name] = serde_json::json!({
        "version":     m.version,
        "description": m.description,
        "file":        m.file,
        "type":        "community",
        "author":      m.author,
        "tags":        m.tags,
    });

    // 2. Obtener SHA de master para crear rama
    println!("[orion publish] Obteniendo referencia de master ...");
    let refs_resp = match gh_get(&format!("{api_base}/git/refs/heads/master"), &token) {
        Ok(r) => r,
        Err(e) => { eprintln!("[orion publish] {e}"); std::process::exit(1); }
    };
    let master_sha = match refs_resp["object"]["sha"].as_str() {
        Some(s) => s.to_string(),
        None => { eprintln!("[orion publish] No se pudo leer el SHA de master."); std::process::exit(1); }
    };

    // 3. Crear rama publish/<name>-<version>
    let branch = format!("publish/{}-{}", m.name, m.version.replace('.', "-"));
    println!("[orion publish] Creando rama {branch} ...");
    if let Err(e) = gh_create_branch(&api_base, &token, &branch, &master_sha) {
        eprintln!("[orion publish] {e}"); std::process::exit(1);
    }

    // 4. Subir archivo .orx a la rama
    let orx_path_in_repo = format!("packages/{}", m.file);
    let orx_url = format!("{api_base}/contents/{orx_path_in_repo}");
    let orx_b64 = B64.encode(orx_src.as_bytes());

    // Si el archivo ya existe en el repo hay que incluir su SHA
    let mut orx_body = serde_json::json!({
        "message": format!("feat: publish {} v{}", m.name, m.version),
        "content": orx_b64,
        "branch":  branch,
    });
    if let Ok(existing_file) = gh_get(&format!("{orx_url}?ref={branch}"), &token) {
        if let Some(sha) = existing_file["sha"].as_str() {
            orx_body["sha"] = serde_json::Value::String(sha.to_string());
        }
    }

    println!("[orion publish] Subiendo {} ...", m.file);
    if let Err(e) = gh_put(&orx_url, &token, &orx_body) {
        eprintln!("[orion publish] Error subiendo .orx: {e}"); std::process::exit(1);
    }

    // 5. Actualizar registry.json en la rama
    let updated_reg = serde_json::to_string_pretty(&reg_json).unwrap_or_default();
    let reg_new_b64 = B64.encode(updated_reg.as_bytes());

    println!("[orion publish] Actualizando registry.json ...");
    if let Err(e) = gh_put(&reg_url, &token, &serde_json::json!({
        "message": format!("registry: add {} v{}", m.name, m.version),
        "content": reg_new_b64,
        "sha":     reg_blob_sha,
        "branch":  branch,
    })) {
        eprintln!("[orion publish] Error actualizando registry: {e}"); std::process::exit(1);
    }

    // 6. Crear Pull Request
    println!("[orion publish] Abriendo Pull Request ...");
    let tags_str = if m.tags.is_empty() { "-".to_string() } else { m.tags.join(", ") };
    let pr_body = format!(
        "## Paquete: `{}` v{}\n\n{}\n\n| Campo | Valor |\n|---|---|\n| Autor | {} |\n| Tags | {} |\n| Licencia | {} |\n\n---\n*Publicado con `orion --publish`*",
        m.name, m.version, m.description, m.author, tags_str, m.license
    );

    let pr_resp = match gh_post(&format!("{api_base}/pulls"), &token, &serde_json::json!({
        "title": format!("feat: publish {} v{}", m.name, m.version),
        "body":  pr_body,
        "head":  branch,
        "base":  "master",
    })) {
        Ok(r) => r,
        Err(e) => { eprintln!("[orion publish] Error creando PR: {e}"); std::process::exit(1); }
    };

    let pr_url = pr_resp["html_url"].as_str().unwrap_or("(ver GitHub)");

    println!();
    println!("[orion publish] Publicacion enviada exitosamente.");
    println!("[orion publish] PR:  {pr_url}");
    println!();
    println!("  El paquete estara disponible despues de la revision y merge del PR.");
    println!("  Usa `orion --update {}` cuando el PR sea aceptado.", m.name);
    println!();
}
