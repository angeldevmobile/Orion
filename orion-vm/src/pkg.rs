//! Package manager para Orion — `orion --add`, `--remove`, `--list`, `--search`,
//! `--update`, `--install`, `--publish`.
//!
//! Dónde vive cada cosa lo decide `crate::paths`, no este archivo: el gestor, el
//! `use` del runtime y el doctor comparten una única noción de "directorio de
//! paquetes". Aquí solo se decide *qué* se instala y *de dónde* viene.
//!
//! Esquema registry.json:
//!   { "_meta": { "registry": "<base_url>", ... },
//!     "packages": { "<name>": {
//!         "version", "description", "file", "type", "author", "tags",
//!         "sha256"?,                       // integridad del .orx
//!         "dependencies"? { "<pkg>": "<spec>" },
//!         "assets"? { "<plataforma>": { "url", "sha256", "signature"? } }
//!     } } }
//!
//! Esquema installed.json:
//!   { "<name>": { "version", "description", "file", "source", "sha256"?, "native"? } }
//!
//! Esquema orion.json (manifiesto de proyecto y de publicación):
//!   { "name", "version", "description", "author", "tags", "file", "license",
//!     "dependencies"? { "<pkg>": "<spec>" }, "assets"? { ... } }
//!
//! Esquema orion.lock:
//!   { "packages": { "<name>": { "version", "resolved", "sha256", "source" } } }

use indexmap::IndexMap as HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use sha2::{Digest, Sha256};

use crate::paths;

const DEFAULT_REGISTRY_BASE: &str =
    "https://raw.githubusercontent.com/angeldevmobile/Orion/master/packages";

/// Tope de descarga. Un `.orx` son kilobytes y una librería nativa decenas de
/// megabytes; más allá de esto casi seguro es un error de configuración o una
/// respuesta HTML de error, y conviene fallar en vez de llenar el disco.
const MAX_DOWNLOAD: u64 = 512 * 1024 * 1024;

//    Configuración del registry

/// Base del registry. `ORION_REGISTRY` la sobrescribe, que es lo que permite
/// apuntar a un registro propio o a un espejo interno sin recompilar.
fn registry_base() -> String {
    match std::env::var("ORION_REGISTRY") {
        Ok(v) if !v.trim().is_empty() => v.trim().trim_end_matches('/').to_string(),
        _ => DEFAULT_REGISTRY_BASE.to_string(),
    }
}

fn registry_url() -> String { format!("{}/registry.json", registry_base()) }

//    Rutas (delegadas en `paths`)

/// Directorio donde se escribe al instalar.
fn install_dir() -> PathBuf { paths::install_dir() }

/// Directorios donde se busca lo ya instalado, del más específico al más general.
fn search_dirs() -> Vec<PathBuf> { paths::packages_dirs() }

/// Caché local del registry. Si el proyecto trae uno propio manda ese (permite
/// fijar un registro vendorizado); si no, se usa el de la caché global.
fn registry_path() -> PathBuf {
    let project = paths::project_packages_dir().join("registry.json");
    if project.is_file() { return project; }
    paths::global_packages_dir().join("registry.json")
}

/// Dónde se escribe la caché del registry al refrescarla.
fn registry_cache_path() -> PathBuf {
    let project = paths::project_packages_dir().join("registry.json");
    if project.is_file() { return project; }
    paths::global_packages_dir().join("registry.json")
}

fn installed_path_in(dir: &Path) -> PathBuf { dir.join("installed.json") }

//    Estructuras internas

#[derive(Debug, Clone, Default)]
struct Asset {
    url:       String,
    sha256:    String,
    signature: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct PkgEntry {
    version:     String,
    description: String,
    file:        String,
    pkg_type:    String,
    tags:        Vec<String>,
    sha256:      Option<String>,
    deps:        HashMap<String, String>,
    assets:      HashMap<String, Asset>,
}

//    Utilidades de integridad

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Compara un digest esperado con el real. La comparación es sobre hex en
/// minúsculas para que dé igual cómo lo haya escrito quien publicó.
fn verify_sha256(what: &str, bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = sha256_hex(bytes);
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err(format!(
            "checksum de '{what}' no coincide.\n  esperado: {}\n  obtenido: {}",
            expected.trim(), actual
        ))
    }
}

//    Firma de assets nativos

/// Directorios donde se buscan claves públicas de confianza (`*.pem`).
/// `ORION_TRUSTED_KEYS` gana; si no, `<global>/trusted_keys`.
fn trusted_key_dirs() -> Vec<PathBuf> {
    if let Ok(d) = std::env::var("ORION_TRUSTED_KEYS") {
        if !d.trim().is_empty() { return vec![PathBuf::from(d)]; }
    }
    vec![paths::global_packages_dir().join("trusted_keys")]
}

fn load_trusted_keys() -> Vec<(String, String)> {
    let mut keys = Vec::new();
    for dir in trusted_key_dirs() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("pem") { continue; }
            if let Ok(pem) = fs::read_to_string(&p) {
                keys.push((p.file_name().unwrap_or_default().to_string_lossy().to_string(), pem));
            }
        }
    }
    keys
}

/// Verifica una firma RSA PKCS#1 v1.5 sobre el SHA-256 del asset.
///
/// Sin claves de confianza instaladas no se puede afirmar nada: se avisa y se
/// continúa, porque el sha256 ya garantiza que el binario es exactamente el que
/// el registry declara. La firma añade *quién* lo declara, y eso solo tiene
/// sentido si el usuario ha decidido en quién confía.
fn verify_signature(pkg: &str, bytes: &[u8], sig_b64: &str) -> Result<bool, String> {
    use rsa::RsaPublicKey;
    use rsa::pkcs1v15::Pkcs1v15Sign;
    use rsa::pkcs8::DecodePublicKey;

    let keys = load_trusted_keys();
    if keys.is_empty() {
        return Ok(false); // nada que verificar contra
    }
    let sig = B64.decode(sig_b64.trim())
        .map_err(|e| format!("firma de '{pkg}' no es base64 válido: {e}"))?;
    let digest = Sha256::digest(bytes);

    for (name, pem) in &keys {
        let Ok(key) = RsaPublicKey::from_public_key_pem(pem) else { continue };
        if key.verify(Pkcs1v15Sign::new::<Sha256>(), &digest, &sig).is_ok() {
            println!("  firma verificada con {name}");
            return Ok(true);
        }
    }
    Err(format!(
        "la firma de '{pkg}' no valida contra ninguna clave de confianza ({} probada(s))",
        keys.len()
    ))
}

//    HTTP

fn http_get_bytes(what: &str, url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .set("User-Agent", "orion-lang/pkg")
        .call()
        .map_err(|e| format!("no se pudo descargar '{what}' desde {url}: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take(MAX_DOWNLOAD)
        .read_to_end(&mut buf)
        .map_err(|e| format!("error leyendo '{what}': {e}"))?;
    Ok(buf)
}

fn http_get_string(what: &str, url: &str) -> Result<String, String> {
    let bytes = http_get_bytes(what, url)?;
    String::from_utf8(bytes).map_err(|_| format!("'{what}' no es texto UTF-8 válido"))
}

//    Registry

/// Carga el registry. Si `refresh` es cierto intenta refrescar desde remoto
/// primero; sin conexión se sigue con la copia local.
fn load_registry(refresh: bool) -> Result<(String, HashMap<String, PkgEntry>), String> {
    let local = registry_path();

    if refresh || !local.exists() {
        if let Ok(body) = http_get_string("registry.json", &registry_url()) {
            let dest = registry_cache_path();
            let _ = fs::create_dir_all(dest.parent().unwrap_or(Path::new(".")));
            let _ = fs::write(&dest, &body);
        }
    }

    let path = registry_path();
    let raw = fs::read_to_string(&path).map_err(|e| {
        format!(
            "No se pudo leer registry.json en {}: {}\n  Ejecuta `orion --update` con conexión para descargarlo.",
            path.display(), e
        )
    })?;
    parse_registry(&raw)
}

fn parse_registry(raw: &str) -> Result<(String, HashMap<String, PkgEntry>), String> {
    let json: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| format!("registry.json malformado: {e}"))?;

    let base_url = json["_meta"]["registry"].as_str()
        .map(str::to_string)
        .unwrap_or_else(registry_base);

    let pkgs_obj = json["packages"].as_object()
        .ok_or("registry.json: campo 'packages' no es un objeto")?;

    let mut map = HashMap::new();
    for (name, val) in pkgs_obj {
        map.insert(name.clone(), parse_entry(name, val));
    }
    Ok((base_url, map))
}

fn parse_entry(name: &str, val: &serde_json::Value) -> PkgEntry {
    let tags = val["tags"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();

    let mut deps = HashMap::new();
    if let Some(obj) = val["dependencies"].as_object() {
        for (k, v) in obj {
            deps.insert(k.clone(), v.as_str().unwrap_or("*").to_string());
        }
    }

    let mut assets = HashMap::new();
    if let Some(obj) = val["assets"].as_object() {
        for (plat, a) in obj {
            assets.insert(plat.clone(), Asset {
                url:       a["url"].as_str().unwrap_or("").to_string(),
                sha256:    a["sha256"].as_str().unwrap_or("").to_string(),
                signature: a["signature"].as_str().map(str::to_string),
            });
        }
    }

    PkgEntry {
        version:     val["version"].as_str().unwrap_or("0.0.0").to_string(),
        description: val["description"].as_str().unwrap_or("").to_string(),
        file:        val["file"].as_str().map(str::to_string)
                        .unwrap_or_else(|| format!("{name}.orx")),
        pkg_type:    val["type"].as_str().unwrap_or("community").to_string(),
        sha256:      val["sha256"].as_str().map(str::to_string),
        tags, deps, assets,
    }
}

//    installed.json

fn load_installed_at(dir: &Path) -> HashMap<String, serde_json::Value> {
    match fs::read_to_string(installed_path_in(dir)) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

/// Lo instalado en el directorio donde escribiríamos ahora.
fn load_installed() -> HashMap<String, serde_json::Value> {
    load_installed_at(&install_dir())
}

/// Todo lo instalado visible desde aquí, con el directorio de cada cosa. El
/// proyecto tiene prioridad: si un paquete está en los dos, gana el de dentro.
pub fn installed_everywhere() -> Vec<(PathBuf, HashMap<String, serde_json::Value>)> {
    search_dirs().into_iter()
        .map(|d| { let m = load_installed_at(&d); (d, m) })
        .filter(|(_, m)| !m.is_empty())
        .collect()
}

fn save_installed_at(dir: &Path, installed: &HashMap<String, serde_json::Value>) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let path = installed_path_in(dir);
    let json = serde_json::to_string_pretty(installed)
        .map_err(|e| format!("Error serializando installed.json: {e}"))?;
    fs::write(&path, json)
        .map_err(|e| format!("Error escribiendo {}: {e}", path.display()))
}

//    Especificadores de versión

/// ¿`version` satisface `spec`?
///
/// Se admite `*`/`latest` (cualquiera), exacta (`1.2.3`), caret (`^1.2.3`:
/// mismo major), tilde (`~1.2.3`: mismo major.minor) y `>=`/`>`/`<=`/`<`.
/// Es un subconjunto deliberado de semver: cubre lo que la gente escribe de
/// verdad sin arrastrar un resolvedor completo.
fn satisfies(version: &str, spec: &str) -> bool {
    let spec = spec.trim();
    if spec.is_empty() || spec == "*" || spec == "latest" { return true; }

    let v = parse_semver(version);
    if let Some(rest) = spec.strip_prefix('^') {
        let r = parse_semver(rest);
        return v.0 == r.0 && cmp_semver(v, r) >= std::cmp::Ordering::Equal;
    }
    if let Some(rest) = spec.strip_prefix('~') {
        let r = parse_semver(rest);
        return v.0 == r.0 && v.1 == r.1 && cmp_semver(v, r) >= std::cmp::Ordering::Equal;
    }
    for (pfx, want) in [
        (">=", vec![std::cmp::Ordering::Greater, std::cmp::Ordering::Equal]),
        ("<=", vec![std::cmp::Ordering::Less,    std::cmp::Ordering::Equal]),
        (">",  vec![std::cmp::Ordering::Greater]),
        ("<",  vec![std::cmp::Ordering::Less]),
        ("=",  vec![std::cmp::Ordering::Equal]),
    ] {
        if let Some(rest) = spec.strip_prefix(pfx) {
            return want.contains(&cmp_semver(v, parse_semver(rest)));
        }
    }
    version.trim() == spec
}

fn parse_semver(s: &str) -> (u64, u64, u64) {
    let mut it = s.trim().trim_start_matches('v')
        .split(['.', '-', '+'])
        .map(|p| p.parse::<u64>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

fn cmp_semver(a: (u64, u64, u64), b: (u64, u64, u64)) -> std::cmp::Ordering {
    a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
}

//    Fuentes de paquete

/// De dónde sale un paquete. Rompe el cuello de botella del registro único:
/// `--add` acepta una URL, un repo de GitHub o una ruta local, no solo un
/// nombre del registry oficial.
#[derive(Debug, Clone)]
enum Source {
    /// Nombre en el registry configurado.
    Registry(String),
    /// URL directa a un `.orx`.
    Url(String),
    /// `github:owner/repo[@ref][:ruta/al/archivo.orx]`
    GitHub { owner: String, repo: String, git_ref: String, path: Option<String> },
    /// Ruta en disco a un `.orx`.
    Local(PathBuf),
}

fn parse_source(spec: &str) -> Source {
    let s = spec.trim();

    if s.starts_with("http://") || s.starts_with("https://") {
        return Source::Url(s.to_string());
    }

    if let Some(rest) = s.strip_prefix("github:").or_else(|| s.strip_prefix("gh:")) {
        // owner/repo[@ref][:path]
        let (loc, path) = match rest.split_once(':') {
            Some((l, p)) => (l, Some(p.to_string())),
            None => (rest, None),
        };
        let (loc, git_ref) = match loc.split_once('@') {
            Some((l, r)) => (l, r.to_string()),
            None => (loc, "HEAD".to_string()),
        };
        let (owner, repo) = loc.split_once('/').unwrap_or((loc, ""));
        return Source::GitHub {
            owner: owner.to_string(), repo: repo.to_string(), git_ref, path,
        };
    }

    let looks_local = s.starts_with("./") || s.starts_with("../")
        || s.starts_with(".\\") || s.starts_with("..\\")
        || Path::new(s).is_absolute()
        || s.ends_with(".orx");
    if looks_local {
        return Source::Local(PathBuf::from(s));
    }

    Source::Registry(s.to_string())
}

/// Nombre de paquete deducido de una fuente no-registry.
fn source_default_name(src: &Source) -> String {
    let stem = |s: &str| Path::new(s).file_stem()
        .and_then(|x| x.to_str()).unwrap_or("paquete").to_string();
    match src {
        Source::Registry(n) => n.clone(),
        Source::Url(u)      => stem(u.split(['?', '#']).next().unwrap_or(u)),
        Source::Local(p)    => stem(&p.to_string_lossy()),
        Source::GitHub { repo, path, .. } => match path {
            Some(p) => stem(p),
            None    => repo.clone(),
        },
    }
}

/// Descarga (o lee) el `.orx` de una fuente y devuelve `(bytes, url_resuelta)`.
fn fetch_source(name: &str, src: &Source, entry: Option<&PkgEntry>) -> Result<(Vec<u8>, String), String> {
    match src {
        Source::Registry(_) => {
            let file = entry.map(|e| e.file.clone()).unwrap_or_else(|| format!("{name}.orx"));
            let url = format!("{}/{}", registry_base(), file);
            Ok((http_get_bytes(name, &url)?, url))
        }
        Source::Url(u) => Ok((http_get_bytes(name, u)?, u.clone())),
        Source::Local(p) => {
            let bytes = fs::read(p)
                .map_err(|e| format!("no se pudo leer '{}': {e}", p.display()))?;
            Ok((bytes, p.to_string_lossy().to_string()))
        }
        Source::GitHub { owner, repo, git_ref, path } => {
            // Sin ruta explícita se prueban las convenciones habituales antes
            // de rendirse, para que `gh:owner/repo` funcione en el caso normal.
            let candidates: Vec<String> = match path {
                Some(p) => vec![p.clone()],
                None => vec![
                    format!("{repo}.orx"),
                    format!("src/{repo}.orx"),
                    format!("packages/{repo}.orx"),
                    "main.orx".to_string(),
                ],
            };
            let mut tried = Vec::new();
            for c in &candidates {
                let url = format!("https://raw.githubusercontent.com/{owner}/{repo}/{git_ref}/{c}");
                if let Ok(b) = http_get_bytes(name, &url) {
                    return Ok((b, url));
                }
                tried.push(c.clone());
            }
            Err(format!(
                "no se encontró ningún .orx en {owner}/{repo}@{git_ref}.\n  Probado: {}\n  Indica la ruta con  gh:{owner}/{repo}:ruta/al/archivo.orx",
                tried.join(", ")
            ))
        }
    }
}

//    Assets nativos

/// Descarga el asset nativo de la plataforma actual, si el paquete tiene alguno.
/// Devuelve la ruta relativa dentro del directorio de paquetes.
fn install_native_asset(name: &str, entry: &PkgEntry, dir: &Path) -> Result<Option<String>, String> {
    if entry.assets.is_empty() { return Ok(None); }

    let plat = paths::current_platform();
    let Some(asset) = entry.assets.get(&plat) else {
        let disponibles: Vec<&str> = entry.assets.keys().map(String::as_str).collect();
        return Err(format!(
            "'{name}' no publica binario para {plat}.\n  Plataformas disponibles: {}",
            disponibles.join(", ")
        ));
    };
    if asset.url.is_empty() {
        return Err(format!("'{name}': el asset de {plat} no tiene 'url'"));
    }
    // Un binario remoto sin checksum declarado no se instala: es el único
    // punto del flujo donde ejecutamos código que no compiló el usuario.
    if asset.sha256.trim().is_empty() {
        return Err(format!(
            "'{name}': el asset de {plat} no declara 'sha256'.\n  Un binario nativo sin checksum no se instala."
        ));
    }

    println!("  descargando binario nativo para {plat} ...");
    let bytes = http_get_bytes(&format!("{name} ({plat})"), &asset.url)?;
    verify_sha256(&format!("{name} ({plat})"), &bytes, &asset.sha256)?;

    match &asset.signature {
        Some(sig) => {
            if !verify_signature(name, &bytes, sig)? {
                println!("  aviso: '{name}' viene firmado pero no hay claves de confianza instaladas");
                println!("         (el checksum sí se verificó; añade claves en {})",
                         trusted_key_dirs()[0].display());
            }
        }
        None => {}
    }

    let file_name = paths::dylib_file_name(
        Path::new(&asset.url).file_stem().and_then(|s| s.to_str()).unwrap_or(name)
    );
    let target_dir = paths::native_dir(dir, name);
    fs::create_dir_all(&target_dir)
        .map_err(|e| format!("no se pudo crear {}: {e}", target_dir.display()))?;
    let target = target_dir.join(&file_name);
    fs::write(&target, &bytes)
        .map_err(|e| format!("no se pudo escribir {}: {e}", target.display()))?;

    // En Unix la librería necesita permiso de ejecución.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&target, fs::Permissions::from_mode(0o755));
    }

    Ok(Some(format!("native/{name}/{file_name}")))
}

//    Instalación

struct Installed {
    version: String,
    sha256:  String,
    origin:  String,
}

/// Instala un paquete concreto. `expect_sha` viene de `--sha256` o del lockfile.
fn install_one(
    name: &str,
    src: &Source,
    entry: Option<&PkgEntry>,
    expect_sha: Option<&str>,
    force: bool,
) -> Result<Installed, String> {
    let dir = install_dir();
    let file = entry.map(|e| e.file.clone()).unwrap_or_else(|| format!("{name}.orx"));
    let dest = dir.join(&file);

    let (bytes, origin) = match fetch_source(name, src, entry) {
        Ok(v) => v,
        Err(e) if dest.exists() && !force => {
            // Sin red pero con copia en disco: se registra lo que ya hay.
            let local = fs::read(&dest).map_err(|_| e.clone())?;
            (local, "local".to_string())
        }
        Err(e) => return Err(e),
    };

    // El checksum explícito manda sobre el del registry: es lo que el usuario
    // fijó a mano o lo que quedó grabado en el lockfile.
    let expected = expect_sha
        .map(str::to_string)
        .or_else(|| entry.and_then(|e| e.sha256.clone()));
    if let Some(exp) = &expected {
        verify_sha256(name, &bytes, exp)?;
    }
    let digest = sha256_hex(&bytes);

    fs::create_dir_all(&dir)
        .map_err(|e| format!("no se pudo crear {}: {e}", dir.display()))?;
    fs::write(&dest, &bytes)
        .map_err(|e| format!("no se pudo escribir {}: {e}", dest.display()))?;

    let native = match entry {
        Some(e) => install_native_asset(name, e, &dir)?,
        None    => None,
    };

    let version = entry.map(|e| e.version.clone()).unwrap_or_else(|| "0.0.0".to_string());
    let mut record = serde_json::json!({
        "version":     version,
        "description": entry.map(|e| e.description.clone()).unwrap_or_default(),
        "file":        file,
        "source":      origin_kind(src, entry),
        "sha256":      digest,
        "origin":      origin,
    });
    if let Some(n) = &native {
        record["native"] = serde_json::Value::String(n.clone());
    }

    let mut installed = load_installed_at(&dir);
    installed.insert(name.to_string(), record);
    save_installed_at(&dir, &installed)?;

    Ok(Installed { version, sha256: digest, origin })
}

fn origin_kind(src: &Source, entry: Option<&PkgEntry>) -> String {
    match src {
        Source::Registry(_) => entry.map(|e| e.pkg_type.clone()).unwrap_or_else(|| "remote".into()),
        Source::Url(_)      => "url".into(),
        Source::GitHub {..} => "github".into(),
        Source::Local(_)    => "local".into(),
    }
}

//    orion --add <spec> [--force] [--sha256 <hex>]

/// `sha` viene de `--sha256` y fija qué contenido se acepta: es lo que permite
/// instalar desde una URL cualquiera sin confiar en el servidor.
pub fn add_package(spec: &str, force: bool, sha: Option<&str>) {
    let src = parse_source(spec);
    let name = source_default_name(&src);

    let dir = install_dir();
    let installed = load_installed_at(&dir);
    if installed.contains_key(&name) && !force {
        let v = installed[&name]["version"].as_str().unwrap_or("?");
        println!("[ya instalado] {name} v{v}  — usa --force para reinstalar");
        return;
    }

    // Solo el registry aporta metadatos (versión, deps, assets). Una URL o un
    // repo se instalan tal cual: no hay nadie que responda por ellos.
    let entry = match &src {
        Source::Registry(n) => match lookup_registry(n) {
            Ok(e) => Some(e),
            Err(e) => { eprintln!("[orion pkg] {e}"); std::process::exit(1); }
        },
        _ => None,
    };

    if entry.is_none() && sha.is_none() {
        println!("[orion pkg] aviso: '{spec}' no viene del registry y no fijaste --sha256;");
        println!("            se instalará lo que responda el servidor y se anotará su hash.");
    }

    match install_one(&name, &src, entry.as_ref(), sha, force) {
        Ok(i) => {
            println!("[orion pkg] {name} v{} instalado → {}", i.version, dir.display());
            println!("            sha256 {}", &i.sha256[..16.min(i.sha256.len())]);
            if i.origin != "local" && entry.is_some() {
                install_deps_of(entry.as_ref().unwrap(), force);
            }
            println!("            úsalo con:  use \"{name}\"");
        }
        Err(e) => { eprintln!("[orion pkg] {e}"); std::process::exit(1); }
    }
}

/// Instala las dependencias declaradas por un paquete del registry.
fn install_deps_of(entry: &PkgEntry, force: bool) {
    if entry.deps.is_empty() { return; }
    let dir = install_dir();
    for (dep, spec) in &entry.deps {
        let installed = load_installed_at(&dir);
        if let Some(cur) = installed.get(dep) {
            let v = cur["version"].as_str().unwrap_or("0.0.0");
            if satisfies(v, spec) { continue; }
        }
        println!("[orion pkg] dependencia: {dep} {spec}");
        match lookup_registry(dep) {
            Ok(e) => {
                if !satisfies(&e.version, spec) {
                    eprintln!("[orion pkg] aviso: '{dep}' v{} no satisface '{spec}' (se instala igual)", e.version);
                }
                if let Err(err) = install_one(dep, &Source::Registry(dep.clone()), Some(&e), None, force) {
                    eprintln!("[orion pkg] {err}");
                }
            }
            Err(err) => eprintln!("[orion pkg] {err}"),
        }
    }
}

fn lookup_registry(name: &str) -> Result<PkgEntry, String> {
    let (_b, reg) = load_registry(false)?;
    if let Some(e) = reg.get(name) { return Ok(e.clone()); }

    let (_b2, reg2) = load_registry(true)?;
    if let Some(e) = reg2.get(name) { return Ok(e.clone()); }

    let mut available: Vec<&String> = reg2.keys().collect();
    available.sort();
    Err(format!(
        "Paquete '{name}' no encontrado en el registry.\n  Disponibles: {}\n  También puedes instalar desde una URL, un repo (gh:owner/repo) o una ruta local.",
        available.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
    ))
}

//    orion --install  (dependencias del manifiesto del proyecto)

pub fn install_project() {
    let manifest = paths::manifest_path();
    if !manifest.is_file() {
        eprintln!("[orion pkg] No hay {} en este proyecto.", paths::MANIFEST);
        eprintln!("  Raíz detectada: {}", paths::project_root().display());
        eprintln!("  Crea el manifiesto con los campos name, version y dependencies.");
        std::process::exit(1);
    }

    let raw = match fs::read_to_string(&manifest) {
        Ok(s) => s,
        Err(e) => { eprintln!("[orion pkg] No se pudo leer {}: {e}", manifest.display()); std::process::exit(1); }
    };
    let json: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(j) => j,
        Err(e) => { eprintln!("[orion pkg] {} malformado: {e}", paths::MANIFEST); std::process::exit(1); }
    };

    let deps = json["dependencies"].as_object().cloned().unwrap_or_default();
    if deps.is_empty() {
        println!("[orion pkg] {} no declara dependencies — nada que instalar.", paths::MANIFEST);
        return;
    }

    let lock = load_lock();
    let mut new_lock = HashMap::new();
    let mut fallos = 0;

    println!("[orion pkg] Instalando {} dependencia(s) en {}", deps.len(), install_dir().display());

    for (name, spec_val) in &deps {
        let spec = spec_val.as_str().unwrap_or("*");
        // Un spec que no es una versión (URL, repo, ruta) se toma como fuente.
        let src = if spec.contains('/') || spec.contains(':') || spec.ends_with(".orx") {
            parse_source(spec)
        } else {
            Source::Registry(name.clone())
        };

        // El lockfile manda: reinstalar un proyecto debe dar exactamente lo
        // mismo, y el sha grabado es lo que lo garantiza.
        let pinned = lock.get(name);
        let expect_sha = pinned.and_then(|p| p["sha256"].as_str());

        let entry = match &src {
            Source::Registry(n) => match lookup_registry(n) {
                Ok(e) => {
                    if !satisfies(&e.version, spec) {
                        eprintln!("[orion pkg] aviso: '{n}' v{} no satisface '{spec}'", e.version);
                    }
                    Some(e)
                }
                Err(e) => { eprintln!("[orion pkg] {e}"); fallos += 1; continue; }
            },
            _ => None,
        };

        match install_one(name, &src, entry.as_ref(), expect_sha, true) {
            Ok(i) => {
                println!("  ok  {name} v{}", i.version);
                new_lock.insert(name.clone(), serde_json::json!({
                    "version":  i.version,
                    "resolved": i.origin,
                    "sha256":   i.sha256,
                    "source":   origin_kind(&src, entry.as_ref()),
                }));
            }
            Err(e) => { eprintln!("  err {name}: {e}"); fallos += 1; }
        }
    }

    if let Err(e) = save_lock(&new_lock) {
        eprintln!("[orion pkg] {e}");
    } else {
        println!("[orion pkg] {} actualizado.", paths::LOCKFILE);
    }

    if fallos > 0 {
        eprintln!("[orion pkg] {fallos} dependencia(s) no se pudieron instalar.");
        std::process::exit(1);
    }
}

fn load_lock() -> HashMap<String, serde_json::Value> {
    let path = paths::lockfile_path();
    let Ok(raw) = fs::read_to_string(path) else { return HashMap::new() };
    let json: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(j) => j,
        Err(_) => return HashMap::new(),
    };
    let mut map = HashMap::new();
    if let Some(obj) = json["packages"].as_object() {
        for (k, v) in obj { map.insert(k.clone(), v.clone()); }
    }
    map
}

fn save_lock(pkgs: &HashMap<String, serde_json::Value>) -> Result<(), String> {
    let path = paths::lockfile_path();
    let doc = serde_json::json!({
        "lockfileVersion": 1,
        "packages": pkgs,
    });
    let text = serde_json::to_string_pretty(&doc)
        .map_err(|e| format!("Error serializando {}: {e}", paths::LOCKFILE))?;
    fs::write(&path, text)
        .map_err(|e| format!("Error escribiendo {}: {e}", path.display()))
}

//    orion --remove <pkg>

pub fn remove_package(name: &str) {
    // Se borra de donde esté, no de donde instalaríamos ahora.
    let Some(dir) = search_dirs().into_iter()
        .find(|d| load_installed_at(d).contains_key(name))
    else {
        eprintln!("[orion pkg] '{name}' no está instalado.");
        std::process::exit(1);
    };

    let mut installed = load_installed_at(&dir);
    let source = installed[name]["source"].as_str().unwrap_or("").to_string();
    let file   = installed[name]["file"].as_str().unwrap_or("").to_string();
    let native = installed[name]["native"].as_str().map(str::to_string);

    installed.shift_remove(name);
    if let Err(e) = save_installed_at(&dir, &installed) {
        eprintln!("[orion pkg] {e}");
        std::process::exit(1);
    }

    // El binario nativo siempre se va: lo trajimos nosotros.
    if let Some(rel) = native {
        let p = dir.join(rel);
        let _ = fs::remove_file(&p);
        let _ = fs::remove_dir(p.parent().unwrap_or(Path::new(".")));
    }

    // Los .orx builtin vienen con Orion y se conservan.
    if source == "builtin" {
        println!("[orion pkg] '{name}' desregistrado (archivo builtin conservado).");
        return;
    }
    if !file.is_empty() {
        let path = dir.join(&file);
        if path.exists() {
            match fs::remove_file(&path) {
                Ok(_)  => { println!("[orion pkg] '{name}' desinstalado de {}.", dir.display()); return; }
                Err(e) => eprintln!("[orion pkg] Advertencia: no se pudo eliminar {}: {e}", path.display()),
            }
        }
    }
    println!("[orion pkg] '{name}' desregistrado.");
}

//    orion --list

pub fn list_packages() {
    let (_base_url, registry) = match load_registry(false) {
        Ok(r) => r,
        Err(e) => { eprintln!("[orion pkg] {e}"); std::process::exit(1); }
    };

    let mut all_installed: HashMap<String, (String, PathBuf)> = HashMap::new();
    for (dir, map) in installed_everywhere() {
        for (name, rec) in map {
            all_installed.entry(name).or_insert_with(|| (
                rec["version"].as_str().unwrap_or("?").to_string(),
                dir.clone(),
            ));
        }
    }

    let mut names: Vec<&String> = registry.keys().collect();
    names.sort();

    println!();
    println!("  Paquetes Orion disponibles:");
    println!("  {:<14} {:<10} {:<12} {}", "NOMBRE", "VERSIÓN", "TIPO", "DESCRIPCIÓN");
    println!("  {}", " ".repeat(72));
    for name in names {
        let entry = &registry[name];
        let mark = if all_installed.contains_key(name) { "✓" } else { " " };
        println!("  {} {:<13} {:<10} {:<12} {}",
                 mark, name, entry.version, entry.pkg_type, entry.description);
    }

    // Lo instalado fuera del registry (URL, repo, ruta local) también existe y
    // omitirlo daría una lista que miente sobre lo que hay en disco.
    let extras: Vec<(&String, &(String, PathBuf))> = all_installed.iter()
        .filter(|(n, _)| !registry.contains_key(n.as_str()))
        .collect();
    if !extras.is_empty() {
        println!();
        println!("  Instalados fuera del registry:");
        for (name, (ver, dir)) in extras {
            println!("  ✓ {:<13} {:<10} {}", name, ver, dir.display());
        }
    }

    println!();
    println!("  ✓ = instalado   |   Instalar: orion --add <paquete|url|gh:owner/repo|ruta>");
    println!();
}

//    orion --search <query>

pub fn search_packages(query: &str) {
    let (_base, registry) = match load_registry(false) {
        Ok(r) => r,
        Err(e) => { eprintln!("[orion pkg] {e}"); std::process::exit(1); }
    };

    let q = query.to_lowercase();
    let mut results: Vec<(i32, &String, &PkgEntry)> = registry.iter()
        .filter_map(|(name, entry)| {
            let mut score: i32 = 0;
            if name.to_lowercase().contains(&q)              { score += 10; }
            if entry.description.to_lowercase().contains(&q) { score += 5; }
            for tag in &entry.tags {
                if tag.to_lowercase().contains(&q) { score += 3; }
            }
            if score > 0 { Some((score, name, entry)) } else { None }
        })
        .collect();
    results.sort_by(|a, b| b.0.cmp(&a.0));

    if results.is_empty() {
        println!("[orion pkg] Sin resultados para '{query}'.");
        return;
    }

    let installed = load_installed();
    println!();
    println!("  Resultados para '{query}':");
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
    let dir = install_dir();
    let installed = load_installed_at(&dir);
    if installed.is_empty() {
        println!("[orion pkg] No hay paquetes instalados en {}.", dir.display());
        return;
    }

    let (_base, registry) = match load_registry(true) {
        Ok(r) => r,
        Err(e) => { eprintln!("[orion pkg] {e}"); std::process::exit(1); }
    };

    let targets: Vec<String> = match pkg_name {
        Some(n) => vec![n.to_string()],
        None    => { let mut v: Vec<_> = installed.keys().cloned().collect(); v.sort(); v }
    };

    for name in &targets {
        if !installed.contains_key(name.as_str()) {
            eprintln!("[orion pkg] '{name}' no está instalado.");
            continue;
        }
        match registry.get(name.as_str()) {
            None => eprintln!("[orion pkg] '{name}' no está en el registry (instalado desde otra fuente)."),
            Some(entry) => {
                match install_one(name, &Source::Registry(name.clone()), Some(entry), None, true) {
                    Ok(i)  => println!("[orion pkg] {name} → v{}", i.version),
                    Err(e) => eprintln!("[orion pkg] {name}: {e}"),
                }
            }
        }
    }
}

//    orion --publish

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
    deps:        serde_json::Value,
    assets:      serde_json::Value,
}

fn read_manifest() -> Result<PackageManifest, String> {
    let path = if Path::new(paths::MANIFEST).is_file() {
        PathBuf::from(paths::MANIFEST)
    } else {
        paths::manifest_path()
    };
    let raw = fs::read_to_string(&path).map_err(|_| format!(
        "No se encontró {} (buscado en {}).\n  Crea uno con los campos: name, version, description, author, tags, file, license",
        paths::MANIFEST, path.display()
    ))?;

    let json: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("{} malformado: {e}", paths::MANIFEST))?;

    let req = |field: &str| -> Result<String, String> {
        json[field].as_str().map(str::to_string)
            .ok_or_else(|| format!("{}: campo requerido '{field}' faltante", paths::MANIFEST))
    };

    let name    = req("name")?;
    let version = req("version")?;
    let desc    = req("description")?;

    Ok(PackageManifest {
        file: json["file"].as_str().map(str::to_string)
                .unwrap_or_else(|| format!("{name}.orx")),
        author:  json["author"].as_str().unwrap_or("").to_string(),
        license: json["license"].as_str().unwrap_or("MIT").to_string(),
        tags:    json["tags"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        deps:   json["dependencies"].clone(),
        assets: json["assets"].clone(),
        name, version, description: desc,
    })
}

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

pub fn publish_package() {
    let m = match read_manifest() {
        Ok(m) => m,
        Err(e) => { eprintln!("[orion publish] {e}"); std::process::exit(1); }
    };

    if !m.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        eprintln!("[orion publish] Nombre inválido '{}'. Usa solo letras minúsculas, números y guiones.", m.name);
        std::process::exit(1);
    }

    let orx_src = match fs::read(&m.file) {
        Ok(s) => s,
        Err(e) => { eprintln!("[orion publish] No se pudo leer '{}': {e}", m.file); std::process::exit(1); }
    };
    // El registry publica el checksum: quien instale puede comprobar que recibe
    // exactamente lo que se publicó, aunque el CDN o el espejo mientan.
    let orx_sha = sha256_hex(&orx_src);

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
    println!("[orion publish] sha256 {orx_sha}");

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

    if let Some(existing) = reg_json["packages"][&m.name].as_object() {
        let ev = existing.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");
        if ev == m.version {
            eprintln!("[orion publish] '{}' v{} ya está en el registry.", m.name, m.version);
            eprintln!("  Incrementa la versión en {} antes de publicar.", paths::MANIFEST);
            std::process::exit(1);
        }
        println!("[orion publish] Actualizando {} v{ev} → v{}", m.name, m.version);
    }

    let mut entry = serde_json::json!({
        "version":     m.version,
        "description": m.description,
        "file":        m.file,
        "type":        if m.assets.is_object() { "native" } else { "community" },
        "author":      m.author,
        "tags":        m.tags,
        "sha256":      orx_sha,
    });
    if m.deps.is_object()   { entry["dependencies"] = m.deps.clone(); }
    if m.assets.is_object() { entry["assets"]       = m.assets.clone(); }
    reg_json["packages"][&m.name] = entry;

    println!("[orion publish] Obteniendo referencia de master ...");
    let refs_resp = match gh_get(&format!("{api_base}/git/refs/heads/master"), &token) {
        Ok(r) => r,
        Err(e) => { eprintln!("[orion publish] {e}"); std::process::exit(1); }
    };
    let master_sha = match refs_resp["object"]["sha"].as_str() {
        Some(s) => s.to_string(),
        None => { eprintln!("[orion publish] No se pudo leer el SHA de master."); std::process::exit(1); }
    };

    let branch = format!("publish/{}-{}", m.name, m.version.replace('.', "-"));
    println!("[orion publish] Creando rama {branch} ...");
    if let Err(e) = gh_create_branch(&api_base, &token, &branch, &master_sha) {
        eprintln!("[orion publish] {e}"); std::process::exit(1);
    }

    let orx_path_in_repo = format!("packages/{}", m.file);
    let orx_url = format!("{api_base}/contents/{orx_path_in_repo}");
    let orx_b64 = B64.encode(&orx_src);

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

    println!("[orion publish] Abriendo Pull Request ...");
    let tags_str = if m.tags.is_empty() { "-".to_string() } else { m.tags.join(", ") };
    let pr_body = format!(
        "## Paquete: `{}` v{}\n\n{}\n\n| Campo | Valor |\n|---|---|\n| Autor | {} |\n| Tags | {} |\n| Licencia | {} |\n| sha256 | `{}` |\n\n---\n*Publicado con `orion --publish`*",
        m.name, m.version, m.description, m.author, tags_str, m.license, orx_sha
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_conocido() {
        // Vector estándar: SHA-256 de "abc".
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verify_sha256_ignora_mayusculas() {
        let d = sha256_hex(b"hola").to_uppercase();
        assert!(verify_sha256("x", b"hola", &d).is_ok());
        assert!(verify_sha256("x", b"adios", &d).is_err());
    }

    #[test]
    fn specs_de_version() {
        assert!(satisfies("1.2.3", "*"));
        assert!(satisfies("1.2.3", "latest"));
        assert!(satisfies("1.2.3", "1.2.3"));
        assert!(!satisfies("1.2.4", "1.2.3"));

        assert!(satisfies("1.9.0", "^1.2.3"));
        assert!(!satisfies("2.0.0", "^1.2.3"));
        assert!(!satisfies("1.0.0", "^1.2.3"));

        assert!(satisfies("1.2.9", "~1.2.3"));
        assert!(!satisfies("1.3.0", "~1.2.3"));

        assert!(satisfies("2.0.0", ">=1.0.0"));
        assert!(satisfies("1.0.0", ">=1.0.0"));
        assert!(!satisfies("0.9.0", ">=1.0.0"));
        assert!(satisfies("0.9.0", "<1.0.0"));
    }

    #[test]
    fn fuentes_reconocidas() {
        assert!(matches!(parse_source("http"), Source::Registry(_)));
        assert!(matches!(parse_source("https://x.dev/a.orx"), Source::Url(_)));
        assert!(matches!(parse_source("./local.orx"), Source::Local(_)));
        assert!(matches!(parse_source("libreria.orx"), Source::Local(_)));

        match parse_source("gh:angeldevmobile/Orion@v1:packages/http.orx") {
            Source::GitHub { owner, repo, git_ref, path } => {
                assert_eq!(owner, "angeldevmobile");
                assert_eq!(repo, "Orion");
                assert_eq!(git_ref, "v1");
                assert_eq!(path.as_deref(), Some("packages/http.orx"));
            }
            other => panic!("esperado GitHub, obtenido {other:?}"),
        }

        match parse_source("github:owner/repo") {
            Source::GitHub { git_ref, path, .. } => {
                assert_eq!(git_ref, "HEAD");
                assert!(path.is_none());
            }
            other => panic!("esperado GitHub, obtenido {other:?}"),
        }
    }

    #[test]
    fn nombre_deducido_de_la_fuente() {
        assert_eq!(source_default_name(&parse_source("https://x.dev/colors.orx")), "colors");
        assert_eq!(source_default_name(&parse_source("./libs/util.orx")), "util");
        assert_eq!(source_default_name(&parse_source("gh:o/mi-lib")), "mi-lib");
    }

    #[test]
    fn registry_extendido_se_parsea_entero() {
        let raw = r#"{
          "_meta": { "registry": "https://ejemplo.dev/pkgs" },
          "packages": {
            "browser": {
              "version": "0.1.0", "description": "d", "file": "browser.orx",
              "type": "native", "sha256": "aa",
              "dependencies": { "http": "^1.0.0" },
              "assets": {
                "win32-x64": { "url": "https://ejemplo.dev/b.dll", "sha256": "bb", "signature": "cc" }
              }
            }
          }
        }"#;
        let (base, reg) = parse_registry(raw).expect("registry válido");
        assert_eq!(base, "https://ejemplo.dev/pkgs");
        let e = &reg["browser"];
        assert_eq!(e.sha256.as_deref(), Some("aa"));
        assert_eq!(e.deps["http"], "^1.0.0");
        let a = &e.assets["win32-x64"];
        assert_eq!(a.url, "https://ejemplo.dev/b.dll");
        assert_eq!(a.signature.as_deref(), Some("cc"));
    }

    #[test]
    fn registry_legacy_sigue_funcionando() {
        // Sin sha256, sin deps y sin assets: el esquema viejo debe cargar igual.
        let raw = r#"{"packages":{"math":{"version":"1.0.0","description":"d","file":"math.orx","type":"builtin"}}}"#;
        let (_b, reg) = parse_registry(raw).expect("registry válido");
        let e = &reg["math"];
        assert!(e.sha256.is_none());
        assert!(e.deps.is_empty());
        assert!(e.assets.is_empty());
        assert_eq!(e.file, "math.orx");
    }
}
