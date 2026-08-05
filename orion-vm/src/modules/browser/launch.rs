//! Localización y arranque del navegador.
//!
//! Se busca en cascada y sin nada fijado en el código: primero lo que diga el
//! programa (`opts.chrome`), luego el entorno (`ORION_CHROME`), y solo después
//! la detección automática. Cualquier navegador basado en Chromium sirve —
//! Chrome, Chromium, Brave o Edge— porque todos hablan CDP; en Windows eso
//! importa porque Edge viene instalado de fábrica.
//!
//! El puerto se pide como 0 (que lo elija el sistema) y se lee del propio
//! navegador por su stderr. Fijar un puerto sería un choque garantizado en
//! cuanto se abran dos navegadores a la vez.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Opciones de arranque. Todo tiene un valor por defecto sensato y todo se
/// puede cambiar desde Orion: aquí no hay constantes escondidas.
#[derive(Debug, Clone)]
pub struct LaunchOpts {
    pub chrome:    Option<String>,
    pub headless:  bool,
    /// Las imágenes se desactivan por defecto: son el grueso del consumo de
    /// memoria y de red de una página, y casi ningún scraper las necesita.
    /// Se reactivan con `images: yes` (obligatorio para capturas fieles).
    pub images:    bool,
    pub gpu:       bool,
    pub width:     u32,
    pub height:    u32,
    pub user_data: Option<String>,
    pub timeout:   Duration,
    pub extra:     Vec<String>,
}

impl Default for LaunchOpts {
    fn default() -> Self {
        LaunchOpts {
            chrome:    None,
            headless:  true,
            images:    false,
            gpu:       false,
            width:     1280,
            height:    800,
            user_data: None,
            timeout:   Duration::from_secs(30),
            extra:     Vec::new(),
        }
    }
}

pub struct Launched {
    pub child:     Child,
    pub ws_url:    String,
    pub exe:       String,
    pub user_data: PathBuf,
    /// El perfil lo creamos nosotros y se borra al cerrar; uno indicado por el
    /// usuario no se toca, porque contiene sus sesiones.
    pub temporal:  bool,
}

/// Candidatos de instalación por plataforma, en orden de preferencia.
fn candidatos() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        let envs = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"];
        let relativos = [
            r"Google\Chrome\Application\chrome.exe",
            r"Chromium\Application\chrome.exe",
            r"BraveSoftware\Brave-Browser\Application\brave.exe",
            r"Microsoft\Edge\Application\msedge.exe",
        ];
        for base in envs {
            if let Ok(dir) = std::env::var(base) {
                for rel in relativos {
                    v.push(PathBuf::from(&dir).join(rel));
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        for p in [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ] { v.push(PathBuf::from(p)); }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for p in [
            "/usr/bin/google-chrome", "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium", "/usr/bin/chromium-browser",
            "/usr/bin/brave-browser", "/usr/bin/microsoft-edge",
            "/snap/bin/chromium",
        ] { v.push(PathBuf::from(p)); }
    }

    v
}

/// Resuelve qué ejecutable usar, en el orden documentado arriba.
pub fn resolve_browser(preferido: Option<&str>) -> Result<PathBuf, String> {
    if let Some(p) = preferido {
        let ruta = PathBuf::from(p);
        if ruta.is_file() { return Ok(ruta); }
        return Err(format!(
            "el navegador indicado no existe: {p}\n  Revisa `chrome` en las opciones de browser.open()."
        ));
    }
    if let Ok(p) = std::env::var("ORION_CHROME") {
        if !p.trim().is_empty() {
            let ruta = PathBuf::from(p.trim());
            if ruta.is_file() { return Ok(ruta); }
            return Err(format!("ORION_CHROME apunta a un archivo que no existe: {p}"));
        }
    }
    if let Some(hit) = candidatos().into_iter().find(|p| p.is_file()) {
        return Ok(hit);
    }
    Err(concat!(
        "no se encontró ningún navegador basado en Chromium.\n",
        "  Instala Chrome, Chromium, Brave o Edge, o indica la ruta:\n",
        "    browser.open({ chrome: \"C:/ruta/chrome.exe\" })\n",
        "  o define la variable de entorno ORION_CHROME."
    ).to_string())
}

/// Argumentos de arranque.
///
/// La lista está podada a propósito: cada bandera apaga algo que un navegador
/// automatizado no necesita y que sí consume memoria o red. Lo que el usuario
/// añada en `extra` va al final y por tanto manda.
fn args(opts: &LaunchOpts, user_data: &PathBuf) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "--remote-debugging-port=0".into(),
        format!("--user-data-dir={}", user_data.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-background-networking".into(),
        "--disable-background-timer-throttling".into(),
        "--disable-backgrounding-occluded-windows".into(),
        "--disable-renderer-backgrounding".into(),
        "--disable-extensions".into(),
        "--disable-sync".into(),
        "--disable-default-apps".into(),
        "--disable-client-side-phishing-detection".into(),
        "--disable-component-update".into(),
        "--no-service-autorun".into(),
        "--mute-audio".into(),
        "--metrics-recording-only".into(),
        // En contenedores /dev/shm suele ser diminuto y Chrome revienta.
        "--disable-dev-shm-usage".into(),
        format!("--window-size={},{}", opts.width, opts.height),
    ];
    if opts.headless {
        a.push("--headless=new".into());
    }
    if !opts.gpu {
        a.push("--disable-gpu".into());
    }
    if !opts.images {
        a.push("--blink-settings=imagesEnabled=false".into());
    }
    a.push("about:blank".into());
    a.extend(opts.extra.iter().cloned());
    a
}

/// Arranca el navegador y devuelve su endpoint CDP.
pub fn launch(opts: &LaunchOpts) -> Result<Launched, String> {
    let exe = resolve_browser(opts.chrome.as_deref())?;

    let (user_data, temporal) = match &opts.user_data {
        Some(d) => (PathBuf::from(d), false),
        None => {
            let d = std::env::temp_dir().join(format!(
                "orion-browser-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|t| t.subsec_nanos()).unwrap_or(0)
            ));
            (d, true)
        }
    };
    std::fs::create_dir_all(&user_data)
        .map_err(|e| format!("no se pudo crear el perfil en {}: {e}", user_data.display()))?;

    let mut child = Command::new(&exe)
        .args(args(opts, &user_data))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo arrancar {}: {e}", exe.display()))?;

    let stderr = child.stderr.take()
        .ok_or("no se pudo leer la salida del navegador")?;

    // El navegador anuncia su endpoint en stderr. Se lee desde otro hilo porque
    // no hay forma portable de leer con plazo: si Chrome nunca lo imprime, el
    // hilo se queda ahí y el plazo lo pone el canal, no la lectura.
    let (tx, rx) = mpsc::channel::<String>();
    let diag = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let diag_hilo = std::sync::Arc::clone(&diag);
    std::thread::Builder::new()
        .name("orion-chrome-stderr".into())
        .spawn(move || {
            for linea in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Some(url) = linea.split("DevTools listening on ").nth(1) {
                    let _ = tx.send(url.trim().to_string());
                    // Se sigue drenando: si nadie lee el stderr, Chrome se
                    // bloquea al llenarse la tubería.
                    continue;
                }
                let mut d = diag_hilo.lock().unwrap();
                if d.len() < 20 { d.push(linea); }
            }
        })
        .map_err(|e| format!("no se pudo leer la salida del navegador: {e}"))?;

    match rx.recv_timeout(opts.timeout) {
        Ok(ws_url) => Ok(Launched {
            child, ws_url,
            exe: exe.display().to_string(),
            user_data, temporal,
        }),
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            if temporal { let _ = std::fs::remove_dir_all(&user_data); }
            let pistas = diag.lock().unwrap().join("\n    ");
            Err(format!(
                "{} arrancó pero no anunció su puerto CDP en {} s.{}",
                exe.display(),
                opts.timeout.as_secs(),
                if pistas.is_empty() { String::new() } else { format!("\n  Dijo:\n    {pistas}") }
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_ruta_inexistente_se_explica() {
        let e = resolve_browser(Some("/no/existe/chrome.exe")).unwrap_err();
        assert!(e.contains("no existe"), "{e}");
        assert!(e.contains("browser.open"), "el error no dice cómo arreglarlo: {e}");
    }

    #[test]
    fn hay_candidatos_para_esta_plataforma() {
        // No se exige que estén instalados, solo que la lista no esté vacía:
        // una plataforma sin candidatos sería un olvido, no una configuración.
        assert!(!candidatos().is_empty(), "no hay rutas candidatas en esta plataforma");
    }

    #[test]
    fn las_banderas_reflejan_las_opciones() {
        let dir = PathBuf::from("/tmp/perfil");

        let por_defecto = args(&LaunchOpts::default(), &dir);
        assert!(por_defecto.iter().any(|a| a == "--headless=new"));
        assert!(por_defecto.iter().any(|a| a.contains("imagesEnabled=false")),
                "las imágenes deberían venir desactivadas por defecto");
        assert!(por_defecto.iter().any(|a| a == "--disable-gpu"));
        assert!(por_defecto.iter().any(|a| a == "--remote-debugging-port=0"),
                "el puerto debe elegirlo el sistema, no estar fijado");

        let visible = LaunchOpts { headless: false, images: true, gpu: true, ..Default::default() };
        let con = args(&visible, &dir);
        assert!(!con.iter().any(|a| a == "--headless=new"));
        assert!(!con.iter().any(|a| a.contains("imagesEnabled=false")));
        assert!(!con.iter().any(|a| a == "--disable-gpu"));
    }

    #[test]
    fn los_extras_del_usuario_van_al_final() {
        let opts = LaunchOpts { extra: vec!["--proxy-server=x:1".into()], ..Default::default() };
        let a = args(&opts, &PathBuf::from("/tmp/p"));
        assert_eq!(a.last().map(String::as_str), Some("--proxy-server=x:1"));
    }
}
