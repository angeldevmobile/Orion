//! Archivos: subir, descargar e imprimir a PDF.
//!
//! El problema que resuelve este archivo no es escribir en disco, es que **el
//! navegador delega estas tres cosas en el sistema operativo**. Al pulsar
//! "Adjuntar" se abre el explorador de archivos de Windows; al pulsar "Descargar"
//! se abre "Guardar como"; al imprimir, el diálogo de impresión. Son ventanas
//! nativas, fuera del DOM: ningún clic ni ninguna tecla sintética las alcanza.
//!
//! Ahí es donde se atasca la automatización web. La salida habitual en Python es
//! `pyautogui` mandando pulsaciones a ciegas a una ventana cuyo título depende
//! del idioma del Windows, que además muere en headless y depende de que nadie
//! toque el ratón mientras tanto.
//!
//! La solución de aquí es la contraria: **no se maneja la ventana, se impide que
//! exista**. CDP permite interceptar las tres antes de que el navegador se las
//! pida al sistema.
//!
//! | Ventana nativa      | Cómo se evita                            |
//! |---------------------|------------------------------------------|
//! | Abrir archivo       | `Page.setInterceptFileChooserDialog`     |
//! | Guardar como        | `Browser.setDownloadBehavior`            |
//! | Imprimir            | `Page.printToPDF`                        |
//!
//! Nada de esto depende del idioma del sistema, ni de la resolución, ni de que
//! haya escritorio: funciona igual en headless y en un servidor sin pantalla.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::cdp::Conn;
use super::dom;
use super::input::{self, Force};
use super::launch::Tuning;

//    Rutas

fn absoluta(p: &str) -> PathBuf {
    let ruta = Path::new(p);
    let abs = if ruta.is_absolute() {
        ruta.to_path_buf()
    } else {
        std::env::current_dir().map(|d| d.join(ruta)).unwrap_or_else(|_| ruta.to_path_buf())
    };
    match std::fs::canonicalize(&abs) {
        Ok(c) => {
            let s = c.display().to_string();
            PathBuf::from(s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s))
        }
        Err(_) => abs,
    }
}

//    Subir archivos

pub fn upload(
    conn: &Conn, session: &str, sel: &str, rutas: &[String],
    espera_ms: u64, t: &Tuning, timeout: Duration,
) -> Result<Vec<String>, String> {
    if rutas.is_empty() {
        return Err("browser.upload: no file was given".into());
    }

    let mut abs = Vec::with_capacity(rutas.len());
    for r in rutas {
        let a = absoluta(r);
        if !a.exists() {
            return Err(format!(
                "browser.upload: the file '{r}' does not exist\n  looked in: {}",
                a.display()
            ));
        }
        if a.is_dir() {
            return Err(format!("browser.upload: '{r}' is a folder, not a file"));
        }
        abs.push(a.display().to_string());
    }

    if !dom::wait_for(conn, session, sel, espera_ms, t)? {
        return Err(format!(
            "browser.upload: '{sel}' did not appear within {espera_ms} ms"
        ));
    }

    // `DOM.setFileInputFiles` necesita el dominio activo. Es idempotente.
    conn.call("DOM.enable", serde_json::json!({}), Some(session), timeout)?;

    if es_input_de_archivo(conn, session, sel, t, timeout)? {
        let object_id = object_id_de(conn, session, sel, t, timeout)?;
        conn.call(
            "DOM.setFileInputFiles",
            serde_json::json!({ "files": abs, "objectId": object_id }),
            Some(session), timeout,
        )?;
        return Ok(abs);
    }

    upload_interceptando(conn, session, sel, &abs, espera_ms, t, timeout)?;
    Ok(abs)
}

/// Camino 2: el selector abre el diálogo en vez de ser el input.
fn upload_interceptando(
    conn: &Conn, session: &str, sel: &str, abs: &[String],
    espera_ms: u64, t: &Tuning, timeout: Duration,
) -> Result<(), String> {
    conn.call(
        "Page.setInterceptFileChooserDialog",
        serde_json::json!({ "enabled": true }),
        Some(session), timeout,
    )?;

    let r = (|| -> Result<(), String> {
        let marca = conn.event_mark();
        input::click(conn, session, sel, "left", 1, espera_ms, Force::No, t, timeout)
            .map_err(|e| e.replace("browser.click", "browser.upload"))?;

        let ev = conn
            .wait_event("Page.fileChooserOpened", Some(session), marca, timeout)?
            .ok_or_else(|| format!(
                "browser.upload: '{sel}' opened no file chooser.\n  \
                 If it is the input itself, check that it is an <input type=\"file\">; \
                 if it is a button, that clicking it opens the dialog."
            ))?;

        let modo = ev.params.get("mode").and_then(|m| m.as_str()).unwrap_or("");
        if modo == "selectSingle" && abs.len() > 1 {
            return Err(format!(
                "browser.upload: '{sel}' accepts a single file and {} were given",
                abs.len()
            ));
        }

        let backend = ev.params.get("backendNodeId").and_then(|b| b.as_u64())
            .ok_or("browser.upload: the browser did not say which input the files were for")?;

        conn.call(
            "DOM.setFileInputFiles",
            serde_json::json!({ "files": abs, "backendNodeId": backend }),
            Some(session), timeout,
        )?;
        Ok(())
    })();

    let _ = conn.call(
        "Page.setInterceptFileChooserDialog",
        serde_json::json!({ "enabled": false }),
        Some(session), timeout,
    );
    r
}

fn es_input_de_archivo(
    conn: &Conn, session: &str, sel: &str, t: &Tuning, timeout: Duration,
) -> Result<bool, String> {
    let cuerpo = r#"
    const e = __find(sel);
    return !!e && e.tagName === 'INPUT' && (e.type || '').toLowerCase() === 'file';
    "#;
    let r = conn.call(
        "Runtime.evaluate",
        serde_json::json!({ "expression": dom::expr(sel, cuerpo, t), "returnByValue": true }),
        Some(session), timeout,
    )?;
    Ok(r.get("result").and_then(|x| x.get("value")).and_then(|v| v.as_bool()).unwrap_or(false))
}

/// Referencia viva al elemento, para las llamadas del dominio `DOM`.
fn object_id_de(
    conn: &Conn, session: &str, sel: &str, t: &Tuning, timeout: Duration,
) -> Result<String, String> {
    let r = conn.call(
        "Runtime.evaluate",
        serde_json::json!({ "expression": dom::expr(sel, "return __find(sel);", t) }),
        Some(session), timeout,
    )?;
    r.get("result").and_then(|x| x.get("objectId")).and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("browser: could not get a reference to '{sel}'"))
}

//    Descargas

pub struct Descarga {
    pub path:  String,
    pub name:  String,
    pub bytes: u64,
    pub url:   String,
}

pub struct DescargaOpts {
    pub dir:       Option<String>,
    pub name:      Option<String>,
    pub overwrite: bool,
    pub wait_ms:   u64,
}

pub fn download(
    conn: &Conn, session: &str, sel: &str, o: &DescargaOpts,
    espera_ms: u64, t: &Tuning, timeout: Duration,
) -> Result<Descarga, String> {
    let dir = match &o.dir {
        Some(d) => {
            let p = absoluta(d);
            std::fs::create_dir_all(&p)
                .map_err(|e| format!("browser.download: could not create '{}': {e}", p.display()))?;
            absoluta(&p.display().to_string())
        }
        None => std::env::current_dir()
            .map_err(|e| format!("browser.download: could not read the current directory: {e}"))?,
    };
    let dir_txt = dir.display().to_string();

    let mut nombrado = true;
    if conn.call(
        "Browser.setDownloadBehavior",
        serde_json::json!({
            "behavior": "allowAndName", "downloadPath": dir_txt, "eventsEnabled": true,
        }),
        None, timeout,
    ).is_err() {
        nombrado = false;
        conn.call(
            "Browser.setDownloadBehavior",
            serde_json::json!({
                "behavior": "allow", "downloadPath": dir_txt, "eventsEnabled": true,
            }),
            None, timeout,
        )?;
    }

    let marca = conn.event_mark();
    input::click(conn, session, sel, "left", 1, espera_ms, Force::No, t, timeout)
        .map_err(|e| e.replace("browser.click", "browser.download"))?;

    let plazo = Duration::from_millis(o.wait_ms);

    let inicio = conn
        .wait_event("Browser.downloadWillBegin", None, marca, plazo)?
        .ok_or_else(|| format!(
            "browser.download: clicking '{sel}' started no download within {} ms.\n  \
             Check that the element is the one that downloads, and not a link that \
             opens the file in a page.",
            o.wait_ms
        ))?;

    let guid = inicio.params.get("guid").and_then(|g| g.as_str()).unwrap_or("").to_string();
    let sugerido = inicio.params.get("suggestedFilename").and_then(|s| s.as_str())
        .unwrap_or("descarga").to_string();
    let url = inicio.params.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();

    let g = guid.clone();
    let fin = conn
        .wait_event_where("Browser.downloadProgress", None, marca, plazo, move |e| {
            e.params.get("guid").and_then(|x| x.as_str()) == Some(g.as_str())
                && matches!(
                    e.params.get("state").and_then(|x| x.as_str()),
                    Some("completed") | Some("canceled")
                )
        })?
        .ok_or_else(|| format!(
            "browser.download: '{sugerido}' did not finish downloading within {} ms.\n  \
             If the file is large, raise the deadline with {{ wait: ms }}.",
            o.wait_ms
        ))?;

    if fin.params.get("state").and_then(|s| s.as_str()) == Some("canceled") {
        return Err(format!("browser.download: the browser cancelled the download of '{sugerido}'"));
    }

    let escrito = if nombrado { dir.join(&guid) } else { dir.join(&sugerido) };
    let quiere = o.name.clone().unwrap_or(sugerido);
    let destino = destino_libre(&dir, &quiere, o.overwrite);

    if escrito != destino {
        std::fs::rename(&escrito, &destino).map_err(|e| format!(
            "browser.download: the download finished but could not be placed in '{}': {e}",
            destino.display()
        ))?;
    }

    let bytes = std::fs::metadata(&destino).map(|m| m.len()).unwrap_or(0);
    Ok(Descarga {
        path:  destino.display().to_string(),
        name:  destino.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        bytes,
        url,
    })
}

pub fn destino_libre(dir: &Path, nombre: &str, overwrite: bool) -> PathBuf {
    let base = dir.join(nombre);
    if overwrite || !base.exists() {
        return base;
    }
    let p = Path::new(nombre);
    let tallo = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| nombre.into());
    let ext = p.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    for n in 2u32.. {
        let c = dir.join(format!("{tallo} ({n}){ext}"));
        if !c.exists() { return c; }
    }
    base
}

//    Impresión a PDF
pub fn pdf(
    conn: &Conn, session: &str, ruta: &str, opts: serde_json::Value, timeout: Duration,
) -> Result<String, String> {
    let r = conn.call("Page.printToPDF", opts, Some(session), timeout)
        .map_err(|e| format!("browser.pdf: {e}"))?;

    let b64 = r.get("data").and_then(|d| d.as_str())
        .ok_or("browser.pdf: the browser returned no document")?;

    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let bytes = B64.decode(b64).map_err(|e| format!("browser.pdf: documento ilegible ({e})"))?;
    std::fs::write(ruta, &bytes)
        .map_err(|e| format!("browser.pdf: could not write '{ruta}': {e}"))?;
    Ok(ruta.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_ruta_relativa_se_vuelve_absoluta() {
        let a = absoluta("archivo.txt");
        assert!(a.is_absolute(), "quedó relativa: {}", a.display());
    }

    #[test]
    fn windows_no_deja_el_prefijo_extendido() {
        let d = std::env::temp_dir();
        let a = absoluta(&d.display().to_string());
        assert!(!a.display().to_string().starts_with(r"\\?\"), "{}", a.display());
    }

    #[test]
    fn el_destino_no_pisa_un_archivo_que_ya_esta() {
        let dir = std::env::temp_dir().join("orion_dl_destino");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        assert_eq!(destino_libre(&dir, "f.pdf", false), dir.join("f.pdf"));

        std::fs::write(dir.join("f.pdf"), b"x").unwrap();
        assert_eq!(destino_libre(&dir, "f.pdf", false), dir.join("f (2).pdf"));
        // Con permiso explícito sí se pisa.
        assert_eq!(destino_libre(&dir, "f.pdf", true), dir.join("f.pdf"));

        std::fs::write(dir.join("f (2).pdf"), b"x").unwrap();
        assert_eq!(destino_libre(&dir, "f.pdf", false), dir.join("f (3).pdf"));

        // Un nombre sin extensión no debe acabar como "informe (2)." ni perderla.
        std::fs::write(dir.join("informe"), b"x").unwrap();
        assert_eq!(destino_libre(&dir, "informe", false), dir.join("informe (2)"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
