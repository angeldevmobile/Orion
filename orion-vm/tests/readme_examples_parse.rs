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
//! Además, algunos bloques auto-contenidos se ejecutan en modo opt-in para
//! detectar fallos de runtime en ejemplos publicados.

use std::{
    fs,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

const README_RUN_BLOCK_INDICES: &[usize] = &[];
const RUN_TIMEOUT: Duration = Duration::from_secs(3);
static TEMP_FILE_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct OrionBlock {
    index: usize,
    start_line: usize,
    code: String,
    run_opt_in: bool,
}

fn parse_orion_fence(line: &str) -> Option<bool> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("```orion")?;
    if let Some(c) = rest.chars().next() {
        if !c.is_whitespace() {
            return None;
        }
    }
    Some(rest.split_whitespace().any(|flag| flag == "run"))
}

/// Extrae los bloques ```orion con su línea de inicio en el README.
fn orion_blocks(src: &str) -> Vec<OrionBlock> {
    let mut out = Vec::new();
    let mut lines = src.lines().enumerate();
    let mut index = 0usize;
    while let Some((i, line)) = lines.next() {
        if let Some(run_opt_in) = parse_orion_fence(line) {
            let start = i + 2; // 1-indexado, y el cuerpo empieza en la siguiente
            let mut body = String::new();
            for (_, l) in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                body.push_str(l);
                body.push('\n');
            }
            out.push(OrionBlock {
                index,
                start_line: start,
                code: body,
                run_opt_in,
            });
            index += 1;
        }
    }
    out
}

fn is_executable_block(block: &OrionBlock, extra_indices: &[usize]) -> bool {
    block.run_opt_in || extra_indices.contains(&block.index)
}

fn run_orion_with_timeout(src: &str, timeout: Duration) -> Result<(), String> {
    let mut path = std::env::temp_dir();
    let seq = TEMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed);
    path.push(format!(
        "orion_readme_run_{}_{}.orx",
        std::process::id(),
        seq
    ));
    fs::write(&path, src).map_err(|e| format!("no se pudo escribir temp: {e}"))?;

    let mut child = Command::new(env!("CARGO_BIN_EXE_orion"))
        .arg(&path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar orion: {e}"))?;
    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| "no se pudo capturar stdout".to_string())?;
    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| "no se pudo capturar stderr".to_string())?;

    let start = Instant::now();
    let result = loop {
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let mut stdout = String::new();
            let mut stderr = String::new();
            let _ = stdout_pipe.read_to_string(&mut stdout);
            let _ = stderr_pipe.read_to_string(&mut stderr);
            break Err(format!(
                "timeout después de {:?}\nstdout:\n{}\nstderr:\n{}",
                timeout,
                stdout,
                stderr
            ));
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                let _ = stdout_pipe.read_to_string(&mut stdout);
                let _ = stderr_pipe.read_to_string(&mut stderr);
                if status.success() {
                    break Ok(());
                }
                break Err(format!(
                    "exit code {:?}\nstdout:\n{}\nstderr:\n{}",
                    status.code(),
                    stdout,
                    stderr
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(e) => break Err(format!("no se pudo esperar proceso: {e}")),
        }
    };

    let _ = fs::remove_file(&path);
    result
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

    let mut parse_failures = Vec::new();
    let mut run_failures = Vec::new();

    for block in &blocks {
        let result = orion_vm::lexer::lex(&block.code)
            .map_err(|e| format!("error léxico: {:?}", e))
            .and_then(|tokens| {
                orion_vm::parser::parse(tokens)
                    .map(|_| ())
                    .map_err(|e| format!("línea {} del bloque: {}", e.line, e.message))
            });

        if let Err(msg) = result {
            let head = block
                .code
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("");
            parse_failures.push(format!(
                "  README.md:{} (bloque #{})  {}\n      {}",
                block.start_line,
                block.index,
                head.trim(),
                msg
            ));
            continue;
        }

        if is_executable_block(block, README_RUN_BLOCK_INDICES) {
            if let Err(msg) = run_orion_with_timeout(&block.code, RUN_TIMEOUT) {
                let head = block
                    .code
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("");
                run_failures.push(format!(
                    "  README.md:{} (bloque #{})  {}\n      {}",
                    block.start_line,
                    block.index,
                    head.trim(),
                    msg
                ));
            }
        }
    }

    assert!(
        parse_failures.is_empty(),
        "{} de {} bloques ```orion del README no parsean.\n\
         Alguien los va a copiar y no le van a compilar:\n{}",
        parse_failures.len(),
        blocks.len(),
        parse_failures.join("\n")
    );

    let selected = blocks
        .iter()
        .filter(|b| is_executable_block(b, README_RUN_BLOCK_INDICES))
        .count();
    assert!(
        run_failures.is_empty(),
        "{} de {} bloques ```orion marcados para ejecución fallaron en runtime.\n\
         Revisa los ejemplos marcados con `run` o los índices explícitos:\n{}",
        run_failures.len(),
        selected,
        run_failures.join("\n")
    );
}

#[test]
fn orion_blocks_detect_run_marker_and_indices() {
    let src = r#"
```orion
show "solo parse"
```
```orion run
show "run marker"
```
```orion
show "run by index"
```
"#;

    let blocks = orion_blocks(src);
    assert_eq!(blocks.len(), 3);
    assert!(!is_executable_block(&blocks[0], &[]));
    assert!(is_executable_block(&blocks[1], &[]));
    assert!(!is_executable_block(&blocks[2], &[]));
    assert!(is_executable_block(&blocks[2], &[2]));
}
