use std::time::{Duration, Instant, SystemTime};
use std::thread;
use std::fs;
use std::sync::atomic::Ordering;
use crate::{lexer, parser, codegen, vm};
use crate::modules::gui;
use super::banner;

pub fn run_watch(path: &str) {
    banner::info(&format!(
        "Watch activo: {BOLD}{path}{RESET}  {DIM}(Ctrl+C para detener){RESET}",
        BOLD = banner::BOLD, RESET = banner::RESET, DIM = banner::DIM
    ));
    println!();

    // Activar watch mode: gui.run() no bloqueará, solo registra los componentes
    gui::state::IS_WATCH_MODE.store(true, Ordering::Relaxed);

    // Primera evaluación
    compile_and_run(path);

    // Si era un script GUI, lanzamos la ventana con hot-reload integrado.
    // launch_watch bloquea hasta que se cierra la ventana (eframe::run_native).
    if gui::try_launch_watch(path) {
        return;
    }

    // Script no-GUI: loop de polling tradicional
    let mut last_mtime = mtime(path);
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

    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    match machine.run() {
        Ok(_) => banner::ok(&format!("OK  {DIM}({:.1} ms){RESET}",
            t.elapsed().as_secs_f64() * 1000.0,
            DIM = banner::DIM, RESET = banner::RESET)),
        Err(e) => banner::fail(&format!("Runtime — {}", e)),
    }
}
