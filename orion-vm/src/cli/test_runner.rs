use std::path::{Path, PathBuf};
use std::time::Instant;
use std::fs;
use crate::{lexer, parser, codegen, vm};
use crate::ast::Stmt;
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

    banner::info(&format!("Encontrados {} archivo(s) de prueba\n", files.len()));

    let mut total_passed = 0usize;
    let mut total_failed = 0usize;

    for path in &files {
        let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let result = run_test_file(path, &file_name, &mut total_passed, &mut total_failed);
        if let Err(e) = result {
            banner::fail(&format!("{file_name}: {e}"));
            total_failed += 1;
        }
    }

    // Resumen final
    let total = total_passed + total_failed;
    println!();
    if total_failed == 0 {
        banner::ok(&format!("{total_passed}/{total} pruebas pasaron"));
    } else {
        banner::fail(&format!("{total_passed}/{total} pasaron  —  {total_failed} fallaron"));
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

/// Ejecuta un archivo de test. Si tiene funciones `fn test_*()`, las corre individualmente.
/// Si no, ejecuta el archivo completo como un test único.
fn run_test_file(
    path: &Path,
    file_name: &str,
    passed: &mut usize,
    failed: &mut usize,
) -> Result<(), String> {
    let src = fs::read_to_string(path)
        .map_err(|e| format!("No se puede leer: {e}"))?;

    let tokens = lexer::lex(&src)
        .map_err(|e| format!("Léxico línea {}:{} — {}", e.line, e.col, e.message))?;

    let stmts = parser::parse(tokens)
        .map_err(|e| format!("Parse línea {} — {}", e.line, e.message))?;

    // Detectar funciones test_*
    let test_fns: Vec<String> = stmts.iter()
        .filter_map(|s| {
            if let Stmt::Fn { name, .. } = s {
                if name.starts_with("test_") { Some(name.clone()) } else { None }
            } else { None }
        })
        .collect();

    if test_fns.is_empty() {
        // Sin funciones test_*: correr el archivo completo como un test
        println!("  {DIM}{file_name}{RESET}", DIM = banner::DIM, RESET = banner::RESET);
        let t = Instant::now();
        match run_stmts(stmts) {
            Ok(()) => {
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                println!("    {GREEN}{BOLD}PASS{RESET}  {DIM}({ms:.1} ms){RESET}",
                    GREEN = banner::GREEN, BOLD = banner::BOLD,
                    RESET = banner::RESET, DIM = banner::DIM);
                *passed += 1;
            }
            Err(e) => {
                println!("    {RED}{BOLD}FAIL{RESET}", RED = banner::RED,
                    BOLD = banner::BOLD, RESET = banner::RESET);
                print_error(&e);
                *failed += 1;
            }
        }
        return Ok(());
    }

    // Hay funciones test_*: correr cada una en aislamiento
    println!("  {DIM}{file_name}{RESET}  {DIM}({} tests){RESET}",
        test_fns.len(), DIM = banner::DIM, RESET = banner::RESET);

    for fn_name in &test_fns {
        print!("    {DIM}{fn_name:<38}{RESET}", DIM = banner::DIM, RESET = banner::RESET);
        use std::io::Write;
        std::io::stdout().flush().ok();

        let t = Instant::now();
        // Agregar Call(fn_name, 0) al final del programa para invocar el test
        let mut run_stmts_with_call = stmts.clone();
        run_stmts_with_call.push(Stmt::Expr {
            expr: crate::ast::Expr::Call {
                callee: Box::new(crate::ast::Expr::Ident(fn_name.clone())),
                args: vec![],
                kwargs: vec![],
            },
            line: 0,
        });

        match run_stmts(run_stmts_with_call) {
            Ok(()) => {
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                println!(" {GREEN}{BOLD}PASS{RESET}  {DIM}({ms:.1} ms){RESET}",
                    GREEN = banner::GREEN, BOLD = banner::BOLD,
                    RESET = banner::RESET, DIM = banner::DIM);
                *passed += 1;
            }
            Err(e) => {
                println!(" {RED}{BOLD}FAIL{RESET}", RED = banner::RED,
                    BOLD = banner::BOLD, RESET = banner::RESET);
                print_error(&e);
                *failed += 1;
            }
        }
    }

    Ok(())
}

fn run_stmts(stmts: Vec<Stmt>) -> Result<(), String> {
    let bc = codegen::compile(stmts)
        .map_err(|e| format!("Codegen línea {} — {}", e.line, e.message))?;
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes);
    machine.run()
}

fn print_error(e: &str) {
    for line in e.lines() {
        println!("       {DIM}{line}{RESET}", DIM = banner::DIM, RESET = banner::RESET);
    }
}
