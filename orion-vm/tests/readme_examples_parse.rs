//! Todo bloque ```orion del README tiene que parsear.
//!
//! El README es lo primero que copia y pega quien llega al proyecto. Hasta
//! ahora sus ejemplos no los verificaba nadie, y doce de cincuenta y uno no
//! compilaban: `elsif` (que no existe, es `else if`), `match` usado como
//! expresión y con flechas, argumentos con nombre por `:` en vez de `=` y en
//! métodos de módulo donde no se admiten, `fn row => ...` mezclando las dos
//! formas de lambda, y `|>` — que el lexer reconoce pero ningún parser
//! implementa.
//!
//! Este test lexea y parsea cada bloque. No ejecuta: comprobar que el
//! programa hace lo correcto es otra cosa, pero que al menos compile es el
//! mínimo que el lector espera.

use std::path::PathBuf;

/// Extrae los bloques ```orion con su línea de inicio en el README.
fn orion_blocks(src: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut lines = src.lines().enumerate();
    while let Some((i, line)) = lines.next() {
        if line.trim_start().starts_with("```orion") {
            let start = i + 2; // 1-indexado, y el cuerpo empieza en la siguiente
            let mut body = String::new();
            for (_, l) in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                body.push_str(l);
                body.push('\n');
            }
            out.push((start, body));
        }
    }
    out
}

#[test]
fn readme_examples_parse() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("orion-vm tiene padre")
        .to_path_buf();

    // El archivo está trackeado como `Readme.md`; en sistemas sensibles a
    // mayúsculas hay que probar las dos formas.
    let path = ["README.md", "Readme.md"]
        .iter()
        .map(|n| root.join(n))
        .find(|p| p.exists())
        .expect("no se encontró el README");

    let src = std::fs::read_to_string(&path).expect("no se pudo leer el README");
    let blocks = orion_blocks(&src);

    assert!(
        blocks.len() > 20,
        "solo se extrajeron {} bloques ```orion del README; el extractor está roto",
        blocks.len()
    );

    let mut failures = Vec::new();
    for (line_no, code) in &blocks {
        let result = orion_vm::lexer::lex(code)
            .map_err(|e| format!("error léxico: {:?}", e))
            .and_then(|tokens| {
                orion_vm::parser::parse(tokens)
                    .map(|_| ())
                    .map_err(|e| format!("línea {} del bloque: {}", e.line, e.message))
            });
        if let Err(msg) = result {
            let head = code.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            failures.push(format!(
                "  README.md:{}  {}\n      {}",
                line_no,
                head.trim(),
                msg
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} de {} bloques ```orion del README no parsean.\n\
         Alguien los va a copiar y no le van a compilar:\n{}",
        failures.len(),
        blocks.len(),
        failures.join("\n")
    );
}
