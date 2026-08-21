//! El registro de builtins tiene que coincidir con el dispatch real.
//!
//! Desde que el typechecker valida `modulo.funcion()` contra el registro, una
//! desincronización deja de ser un detalle de documentación:
//!
//!   - función real que falta en el registro → `orion check` la marca como
//!     inexistente y ABORTA un programa que funcionaba (el typecheck está ON
//!     por defecto al ejecutar). Este es el fallo grave.
//!   - función en el registro que no existe en runtime → se anuncia en el
//!     autocompletado de la extensión y en la referencia del sitio algo que
//!     revienta al llamarlo.
//!
//! Este test extrae los brazos del `match function` de primer nivel de cada
//! módulo y los compara contra `registry()`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Módulos que el runtime atiende bajo más de un nombre (modules/mod.rs).
fn canonical(name: &str) -> &str {
    match name {
        "df" => "table",
        "embeddings" => "embed",
        "tarea" => "task",
        "cola" => "queue",
        "formato" => "format",
        "grafo" => "graph",
        other => other,
    }
}

/// Funciones vivas en el dispatch a propósito pero deliberadamente NO
/// documentadas: solo existen para devolver un error de migración.
fn retirada(module: &str, function: &str) -> bool {
    matches!((module, function), ("timewarp", "measure_time" | "measureMtime"))
}

/// Quita comentarios de línea para que el conteo de llaves no se desvíe.
fn sin_comentarios(line: &str) -> String {
    match line.find("//") {
        Some(i) => line[..i].to_string(),
        None => line.to_string(),
    }
}

/// ¿La línea abre un brazo `"a" | "b" => ...`? Devuelve los nombres.
/// Acepta unicode: hay alias en español ("tamaño") y un solo carácter no ASCII
/// no puede descartar el brazo entero.
fn arm_names(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    if !t.starts_with('"') {
        return None;
    }
    let arrow = t.find("=>")?;
    let head = &t[..arrow];
    let mut names = Vec::new();
    let mut rest = head.trim();
    loop {
        if !rest.starts_with('"') {
            return None;
        }
        let end = rest[1..].find('"')? + 1;
        let name = &rest[1..end];
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }
        names.push(name.to_string());
        rest = rest[end + 1..].trim();
        if rest.is_empty() {
            return Some(names);
        }
        if let Some(stripped) = rest.strip_prefix('|') {
            rest = stripped.trim();
        } else {
            return None; // guardas (`if ...`), rangos, patrones raros: se ignoran
        }
    }
}

/// Brazos del `match function {` de primer nivel dentro de `pub fn call`.
fn dispatch_arms(src: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(i) = src.find("pub fn call") else { return out };
    let Some(j) = src[i..].find("match function").map(|p| p + i) else { return out };
    let Some(k) = src[j..].find('{').map(|p| p + j) else { return out };

    let mut depth = 0i32;
    let mut abierto = false;
    for line in src[k..].lines() {
        if abierto && depth == 1 {
            if let Some(names) = arm_names(line) {
                out.extend(names);
            }
        }
        for ch in sin_comentarios(line).chars() {
            match ch {
                '{' => {
                    depth += 1;
                    abierto = true;
                }
                '}' => {
                    depth -= 1;
                    if abierto && depth == 0 {
                        return out;
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Dónde vive el dispatcher de cada módulo: `<n>_mod.rs`, `<n>/mod.rs` o `<n>.rs`.
fn dispatchers(root: &Path) -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir(root) else { return map };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else { continue };
        if path.is_dir() {
            let inner = path.join("mod.rs");
            if inner.exists() {
                map.insert(name.to_string(), inner); // el directorio manda
            }
        } else if let Some(base) = name.strip_suffix("_mod.rs") {
            map.entry(base.to_string()).or_insert(path);
        } else if let Some(base) = name.strip_suffix(".rs") {
            if base != "mod" {
                map.entry(base.to_string()).or_insert(path);
            }
        }
    }
    map
}

#[test]
fn registry_matches_runtime() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/modules");
    let disp = dispatchers(&root);

    let mut documentado: HashMap<String, HashSet<String>> = HashMap::new();
    for doc in orion_vm::cli::builtins::registry() {
        if !doc.owner.is_empty() {
            documentado
                .entry(doc.owner.clone())
                .or_default()
                .insert(doc.name.clone());
        }
    }

    let mut sin_documentar = Vec::new();
    let mut fantasmas = Vec::new();

    for (modulo, funciones) in &documentado {
        let canon = canonical(modulo);
        let Some(path) = disp.get(canon) else { continue };
        let Ok(src) = std::fs::read_to_string(path) else { continue };
        let runtime = dispatch_arms(&src);
        if runtime.is_empty() {
            continue; // dispatcher no extraíble: no se afirma nada sobre él
        }

        for f in &runtime {
            if !funciones.contains(f) && !retirada(canon, f) {
                sin_documentar.push(format!("{}.{}", modulo, f));
            }
        }
        for f in funciones {
            if !runtime.contains(f) {
                fantasmas.push(format!("{}.{}", modulo, f));
            }
        }
    }

    sin_documentar.sort();
    fantasmas.sort();

    assert!(
        sin_documentar.is_empty(),
        "Funciones REALES que faltan en el registro de builtins.\n\
         El typechecker las marcará como inexistentes y abortará programas que \
         funcionan. Añádelas en cli/builtins_gen.rs:\n  {}",
        sin_documentar.join("\n  ")
    );

    assert!(
        fantasmas.is_empty(),
        "Funciones documentadas que NO existen en el dispatch.\n\
         Se anuncian en el autocompletado y en la web, y revientan al llamarlas. \
         Quítalas del registro o impleméntalas:\n  {}",
        fantasmas.join("\n  ")
    );
}
