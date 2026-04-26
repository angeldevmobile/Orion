use std::path::{Path, PathBuf};
use std::time::Instant;
use std::fs;
use crate::{lexer, parser, codegen, vm};
use super::banner;

pub fn run_tests(folder: &str) {
    banner::section("Test Runner Orion");

    let dir = Path::new(folder);
    if !dir.exists() {
        banner::fail(&format!("Carpeta no encontrada: '{folder}'"));
        std::process::exit(1);
    }

    let files = collect_test_files(dir);
    if files.is_empty() {
        banner::warn(&format!("No se encontraron archivos test_*.orx en '{folder}'"));
        return;
    }

    banner::info(&format!("Encontrados {} archivos de prueba\n", files.len()));

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut errors: Vec<(String, String)> = Vec::new();

    for path in &files {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        print!("  {DIM}{name:<40}{RESET}", DIM = banner::DIM, RESET = banner::RESET);
        use std::io::Write;
        std::io::stdout().flush().ok();

        let t = Instant::now();
        match run_test_file(path) {
            Ok(()) => {
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                println!(" {BOLD}{GREEN}PASS{RESET}  {DIM}({ms:.1} ms){RESET}",
                    BOLD = banner::BOLD, GREEN = banner::GREEN,
                    RESET = banner::RESET, DIM = banner::DIM);
                passed += 1;
            }
            Err(e) => {
                println!(" {BOLD}{RED}FAIL{RESET}", BOLD = banner::BOLD,
                    RED = banner::RED, RESET = banner::RESET);
                errors.push((name.to_string(), e));
                failed += 1;
            }
        }
    }

    // Error details
    if !errors.is_empty() {
        println!();
        for (name, err) in &errors {
            banner::fail(&format!("{name}"));
            for line in err.lines() {
                println!("       {DIM}{line}{RESET}", DIM = banner::DIM, RESET = banner::RESET);
            }
        }
    }

    // Summary
    let total = passed + failed;
    println!();
    if failed == 0 {
        banner::ok(&format!("{passed}/{total} pruebas pasaron"));
    } else {
        banner::fail(&format!("{passed}/{total} pasaron — {failed} fallaron"));
        std::process::exit(1);
    }
}

fn collect_test_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().map(|e| e == "orx").unwrap_or(false) {
                let stem = p.file_stem().unwrap_or_default().to_string_lossy();
                if stem.starts_with("test_") || stem.ends_with("_test") {
                    files.push(p);
                }
            }
        }
    }
    files.sort();
    files
}

fn run_test_file(path: &Path) -> Result<(), String> {
    let src = fs::read_to_string(path)
        .map_err(|e| format!("No se puede leer: {e}"))?;

    let tokens = lexer::lex(&src)
        .map_err(|e| format!("Léxico línea {}:{} — {}", e.line, e.col, e.message))?;

    let stmts = parser::parse(tokens)
        .map_err(|e| format!("Parse línea {} — {}", e.line, e.message))?;

    let bc = codegen::compile(stmts)
        .map_err(|e| format!("Codegen línea {} — {}", e.line, e.message))?;

    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes);
    machine.run().map_err(|e| e)
}
