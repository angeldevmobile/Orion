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

fn is_executable_block(block: &OrionBlock) -> bool {
    block.run_opt_in
}

fn run_orion_with_timeout(src: &str, timeout: Duration) -> Result<(), String> {
    let mut path = std::env::temp_dir();
    let seq = TEMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed);
    path.push(format!("orion_readme_run_{}_{}.orx", std::process::id(), seq));
    fs::write(&path, src).map_err(|e| format!("no se pudo escribir temp: {e}"))?;

    let mut child = Command::new(env!("CARGO_BIN_EXE_orion"))
        .arg(&path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar orion: {e}"))?;

    // Las tuberías se vacían en HILOS APARTE, no al final.
    //
    // Esperar con try_wait() sin leerlas se cuelga en cuanto el ejemplo imprime
    // más de lo que cabe en el búfer del sistema (~64 KB): el hijo se bloquea
    // escribiendo, nunca termina, y el timeout lo mata. El fallo se reportaba
    // como "timeout", que manda a buscar un bucle infinito en un programa que
    // tarda 450 ms. Medido: 20.000 líneas de `show` daban timeout de 3 s.
    let mut stdout_pipe = child.stdout.take().ok_or("no se pudo capturar stdout")?;
    let mut stderr_pipe = child.stderr.take().ok_or("no se pudo capturar stderr")?;
    let h_out = thread::spawn(move || {
        let mut b = String::new();
        let _ = stdout_pipe.read_to_string(&mut b);
        b
    });
    let h_err = thread::spawn(move || {
        let mut b = String::new();
        let _ = stderr_pipe.read_to_string(&mut b);
        b
    });

    let start = Instant::now();
    let veredicto = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("timeout después de {timeout:?}"));
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(e) => break Err(format!("no se pudo esperar proceso: {e}")),
        }
    };

    // Tras kill() o exit, las tuberías cierran y los hilos terminan solos.
    let salida = h_out.join().unwrap_or_default();
    let errores = h_err.join().unwrap_or_default();
    let _ = fs::remove_file(&path);

    match veredicto {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!(
            "exit code {:?}
stdout:
{}
stderr:
{}",
            status.code(), salida, errores
        )),
        Err(msg) => Err(format!("{msg}
stdout:
{salida}
stderr:
{errores}")),
    }
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

        if is_executable_block(block) {
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
        .filter(|b| is_executable_block(b))
        .count();
    assert!(
        run_failures.is_empty(),
        "{} de {} bloques ```orion marcados para ejecución fallaron en runtime.\n\
         Revisa los ejemplos marcados con `run`:\n{}",
        run_failures.len(),
        selected,
        run_failures.join("\n")
    );
}

#[test]
fn solo_se_ejecutan_los_bloques_marcados_con_run() {
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
    assert!(!is_executable_block(&blocks[0]), "```orion a secas NO se ejecuta");
    assert!(is_executable_block(&blocks[1]),  "```orion run SÍ se ejecuta");
    assert!(!is_executable_block(&blocks[2]), "el marcador no se hereda del bloque anterior");
}

/// `orionx` o `orion-algo` no son bloques de Orion: el marcador exige que
/// después de "```orion" venga un espacio o el final de línea.
#[test]
fn una_valla_parecida_no_cuenta_como_bloque_orion() {
    let src = "```orionx
show 1
```
```orion
show 2
```
";
    let blocks = orion_blocks(src);
    assert_eq!(blocks.len(), 1, "solo el segundo bloque es Orion");
}
