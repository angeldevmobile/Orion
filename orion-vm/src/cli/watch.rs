use std::time::{Duration, Instant, SystemTime};
use std::thread;
use std::fs;
use crate::{lexer, parser, codegen, vm};
use super::banner;

pub fn run_watch(path: &str) {
    banner::info(&format!(
        "Watch activo: {BOLD}{path}{RESET}  {DIM}(Ctrl+C para detener){RESET}",
        BOLD = banner::BOLD, RESET = banner::RESET, DIM = banner::DIM, path = path
    ));
    println!();

    let mut last_mtime = mtime(path);
    compile_and_run(path);

    loop {
        thread::sleep(Duration::from_millis(400));
        let cur = mtime(path);
        if cur != last_mtime {
            last_mtime = cur;
            println!("\n  {DIM}{}  cambio detectado{RESET}", "─".repeat(44),
                DIM = banner::DIM, RESET = banner::RESET);
            compile_and_run(path);
        }
    }
}

fn mtime(path: &str) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

fn compile_and_run(path: &str) {
    let t = Instant::now();

    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => { banner::fail(&format!("No se puede leer: {e}")); return; }
    };

    let tokens = match lexer::lex(&src) {
        Ok(t) => t,
        Err(e) => { banner::fail(&format!("Léxico  línea {}:{} — {}", e.line, e.col, e.message)); return; }
    };
    let stmts = match parser::parse(tokens) {
        Ok(s) => s,
        Err(e) => { banner::fail(&format!("Parse  línea {} — {}", e.line, e.message)); return; }
    };
    let bc = match codegen::compile(stmts) {
        Ok(b) => b,
        Err(e) => { banner::fail(&format!("Codegen  línea {} — {}", e.line, e.message)); return; }
    };

    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes);
    match machine.run() {
        Ok(_) => banner::ok(&format!("OK  {DIM}({:.1} ms){RESET}",
            t.elapsed().as_secs_f64() * 1000.0,
            DIM = banner::DIM, RESET = banner::RESET)),
        Err(e) => banner::fail(&format!("Runtime — {}", e)),
    }
}