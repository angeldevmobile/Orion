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

#[derive(Debug, Clone)]
pub struct LaunchOpts {
    pub chrome:    Option<String>,
    pub headless:  bool,
    pub images:    bool,
    pub gpu:       bool,
    pub width:     u32,
    pub height:    u32,
    pub user_data: Option<String>,
    pub timeout:   Duration,
    /// Banderas añadidas por el programa. Van al final, así que mandan.
    pub extra:     Vec<String>,
    pub sin:       Vec<String>,
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
            sin:       Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tuning {
    pub wait_ms:       u64,
    /// Cada cuánto se reintenta dentro de la página mientras se espera.
    pub retry_ms:      u64,
    pub cdp_margin_ms: u64,
    /// Pasos intermedios de un arrastre. Menos de dos y las librerías de
    /// drag-and-drop no reciben `dragover`.
    pub drag_steps:    u32,
    /// Capas superpuestas que `force` llega a atravesar.
    pub force_layers:  u32,
    /// Profundidad máxima de iframes anidados que se recorre.
    pub iframe_depth:  u32,
    /// Margen en píxeles al probar puntos dentro de un elemento.
    pub hit_inset:     f64,
    pub nav_settle_ms: u64,

    pub max_events:    usize,
    /// Techo del sondeo del hilo lector cuando no hay nada en vuelo. Subirlo
    /// baja la CPU en reposo a cambio de algo de latencia.
    pub idle_poll_ms:  u64,
    /// Plazo para las operaciones de cierre, que no deben colgar la salida.
    pub close_ms:      u64,
    /// Plazo para que un envío por el socket progrese.
    pub send_ms:       u64,
    /// Intentos de borrar el perfil temporal al cerrar. En Windows los procesos
    /// hijos del navegador lo retienen unos instantes.
    pub cleanup_tries: u32,
    /// Bytes que el navegador guarda del cuerpo de UNA respuesta, para poder
    /// leerlo después con `capture`. Un listado grande necesita más sitio.
    pub body_buffer:   u64,
    /// Bytes que guarda entre todas las respuestas de la pestaña.
    pub total_buffer:  u64,
    /// Edad, en minutos, a partir de la cual un perfil temporal abandonado se
    /// considera basura y se barre al abrir el siguiente navegador. Cero
    /// desactiva el barrido.
    pub stale_profile_mins: u64,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning {
            wait_ms:       10_000,
            retry_ms:      50,
            cdp_margin_ms: 5_000,
            drag_steps:    10,
            force_layers:  12,
            iframe_depth:  8,
            hit_inset:     24.0,
            nav_settle_ms: 5_000,
            max_events:    512,
            idle_poll_ms:  5,
            close_ms:      2_000,
            send_ms:       5_000,
            cleanup_tries: 12,
            body_buffer:   10 * 1024 * 1024,
            total_buffer:  50 * 1024 * 1024,
            stale_profile_mins: 60,
        }
    }
}

pub struct Launched {
    pub child:     Child,
    pub ws_url:    String,
    pub exe:       String,
    pub user_data: PathBuf,
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

    if !opts.sin.is_empty() {
        let nombre = |s: &str| s.split('=').next().unwrap_or(s).to_string();
        let fuera: Vec<String> = opts.sin.iter().map(|s| nombre(s)).collect();
        a.retain(|f| !fuera.contains(&nombre(f)));
    }

    a.push("about:blank".into());
    a.extend(opts.extra.iter().cloned());
    a
}

/// Borra los perfiles temporales de Orion más viejos que `minutos`.
fn barrer_perfiles_viejos(minutos: u64) {
    if minutos == 0 { return; }
    let umbral = Duration::from_secs(minutos * 60);
    let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) else { return };

    for e in rd.flatten() {
        let nombre = e.file_name();
        let Some(n) = nombre.to_str() else { continue };
        if !n.starts_with("orion-browser-") { continue; }

        let viejo = e.metadata().ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .map(|edad| edad > umbral)
            .unwrap_or(false);
        if viejo {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

fn leer_puerto(archivo: &PathBuf) -> Option<String> {
    let contenido = std::fs::read_to_string(archivo).ok()?;
    let mut lineas = contenido.lines();
    let puerto: u16 = lineas.next()?.trim().parse().ok()?;
    let ruta = lineas.next()?.trim();
    if ruta.is_empty() { return None; }
    Some(format!("ws://127.0.0.1:{puerto}{ruta}"))
}

/// Arranca el navegador y devuelve su endpoint CDP.
pub fn launch(opts: &LaunchOpts, tuning: &Tuning) -> Result<Launched, String> {
    let exe = resolve_browser(opts.chrome.as_deref())?;

    // Barrido de perfiles abandonados.
    barrer_perfiles_viejos(tuning.stale_profile_mins);

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

    let puerto_file = user_data.join("DevToolsActivePort");
    let _ = std::fs::remove_file(&puerto_file);

    let mut child = Command::new(&exe)
        .args(args(opts, &user_data))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo arrancar {}: {e}", exe.display()))?;

    let stderr = child.stderr.take()
        .ok_or("no se pudo leer la salida del navegador")?;

    let (tx, rx) = mpsc::channel::<String>();
    let diag = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let diag_hilo = std::sync::Arc::clone(&diag);
    std::thread::Builder::new()
        .name("orion-chrome-stderr".into())
        .spawn(move || {
            for linea in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Some(url) = linea.split("DevTools listening on ").nth(1) {
                    let _ = tx.send(url.trim().to_string());
                    continue;
                }
                let mut d = diag_hilo.lock().unwrap();
                if d.len() < 20 { d.push(linea); }
            }
        })
        .map_err(|e| format!("no se pudo leer la salida del navegador: {e}"))?;

    let limite = std::time::Instant::now() + opts.timeout;
    let mut encontrado: Option<String> = None;
    let mut relanzo = false;
    while std::time::Instant::now() < limite {
        if let Ok(url) = rx.try_recv() { encontrado = Some(url); break; }
        if let Some(url) = leer_puerto(&puerto_file) { encontrado = Some(url); break; }
        if !relanzo && matches!(child.try_wait(), Ok(Some(_))) { relanzo = true; }
        std::thread::sleep(Duration::from_millis(25));
    }

    match encontrado {
        Some(ws_url) => Ok(Launched {
            child, ws_url,
            exe: exe.display().to_string(),
            user_data, temporal,
        }),
        None => {
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

#[cfg(test)]
mod tests_tuning {
    use super::*;

    #[test]
    fn los_defaults_siguen_siendo_los_de_antes() {
        let t = Tuning::default();
        assert_eq!(t.wait_ms, 10_000);
        assert_eq!(t.retry_ms, 50);
        assert_eq!(t.cdp_margin_ms, 5_000);
        assert_eq!(t.drag_steps, 10);
        assert_eq!(t.force_layers, 12);
        assert_eq!(t.iframe_depth, 8);
        assert_eq!(t.hit_inset, 24.0);
        assert_eq!(t.max_events, 512);
        assert_eq!(t.idle_poll_ms, 5);
    }

    #[test]
    fn se_pueden_quitar_banderas_por_defecto() {
        let dir = PathBuf::from("/tmp/p");

        let normal = args(&LaunchOpts::default(), &dir);
        assert!(normal.iter().any(|a| a == "--disable-extensions"));

        let sin = LaunchOpts {
            sin: vec!["--disable-extensions".into()],
            ..Default::default()
        };
        let a = args(&sin, &dir);
        assert!(!a.iter().any(|x| x == "--disable-extensions"),
                "la bandera debería haberse quitado: {a:?}");
        // Y no debe llevarse por delante nada más.
        assert!(a.iter().any(|x| x == "--disable-sync"));
    }

    #[test]
    fn quitar_una_bandera_no_exige_repetir_su_valor() {
        let opts = LaunchOpts {
            sin: vec!["--blink-settings".into()],
            ..Default::default()
        };
        let a = args(&opts, &PathBuf::from("/tmp/p"));
        assert!(!a.iter().any(|x| x.starts_with("--blink-settings")),
                "no se quitó por nombre: {a:?}");
    }

    #[test]
    fn quitar_una_bandera_inexistente_no_rompe_nada() {
        let opts = LaunchOpts { sin: vec!["--que-no-existe".into()], ..Default::default() };
        let a = args(&opts, &PathBuf::from("/tmp/p"));
        assert!(a.iter().any(|x| x == "--disable-extensions"));
    }
}

#[cfg(test)]
mod tests_arranque {
    use super::*;

    #[test]
    fn el_puerto_se_lee_del_archivo_del_perfil() {
        // Chrome lo anuncia por stderr, pero Edge NO: solo escribe este archivo.
        // Leer solo el stderr dejaba Edge inservible.
        let d = std::env::temp_dir().join("orion_test_devtools_port");
        let _ = std::fs::create_dir_all(&d);
        let f = d.join("DevToolsActivePort");

        std::fs::write(&f, "53133\n/devtools/browser/c3fa616c-33d0\n").unwrap();
        assert_eq!(
            leer_puerto(&f).as_deref(),
            Some("ws://127.0.0.1:53133/devtools/browser/c3fa616c-33d0")
        );

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_archivo_a_medio_escribir_no_se_acepta() {
        let d = std::env::temp_dir().join("orion_test_devtools_parcial");
        let _ = std::fs::create_dir_all(&d);
        let f = d.join("DevToolsActivePort");

        std::fs::write(&f, "53133\n").unwrap();
        assert!(leer_puerto(&f).is_none(), "falta la segunda línea");

        std::fs::write(&f, "").unwrap();
        assert!(leer_puerto(&f).is_none(), "archivo vacío");

        std::fs::write(&f, "no-es-un-puerto\n/devtools/x\n").unwrap();
        assert!(leer_puerto(&f).is_none(), "puerto no numérico");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn el_barrido_respeta_los_perfiles_recientes() {
        // Un perfil recién creado es el de un navegador probablemente vivo.
        let reciente = std::env::temp_dir().join("orion-browser-test-reciente");
        let _ = std::fs::create_dir_all(&reciente);

        barrer_perfiles_viejos(60);
        assert!(reciente.is_dir(), "el barrido se llevó un perfil reciente");

        // Y con el barrido desactivado no toca nada.
        barrer_perfiles_viejos(0);
        assert!(reciente.is_dir());

        let _ = std::fs::remove_dir_all(&reciente);
    }
}
