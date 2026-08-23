use std::time::{Duration, Instant, SystemTime};
use std::thread;
use std::fs;
use std::sync::atomic::Ordering;
use crate::{lexer, parser, codegen, vm};
use crate::modules::gui;
use super::banner;

pub fn run_watch(path: &str) {
    banner::info(&format!(
        "Watch activo: {BOLD}{path}{RESET}  {DIM}(Ctrl+C to stop){RESET}",
        BOLD = banner::BOLD, RESET = banner::RESET, DIM = banner::DIM
    ));
    println!();

    if script_has_serve(path) {
        run_watch_server(path);
        return;
    }

    gui::state::IS_WATCH_MODE.store(true, Ordering::Relaxed);

    // Primera evaluación
    compile_and_run(path);

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

fn script_has_serve(path: &str) -> bool {
    let Ok(src) = fs::read_to_string(path) else { return false };
    let Ok(tokens) = lexer::lex(&src) else { return false };
    tokens.iter().any(|t| matches!(t.kind, crate::token::TokenKind::Serve))
}

fn run_watch_server(path: &str) {
    use std::process::{Child, Command};

    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "orion".to_string());

    let spawn = |reason: &str| -> Option<Child> {
        banner::info(&format!(
            "{reason}  {DIM}(servidor como proceso hijo){RESET}",
            DIM = banner::DIM, RESET = banner::RESET
        ));
        match Command::new(&exe).arg("run").arg(path).spawn() {
            Ok(c) => Some(c),
            Err(e) => { banner::fail(&format!("Could not start the server: {e}")); None }
        }
    };

    let mut child = spawn("Server started");
    let mut last_mtime = mtime(path);

    loop {
        thread::sleep(Duration::from_millis(400));

        // ¿El servidor murió solo? Avisar una vez y esperar cambios.
        if let Some(c) = child.as_mut() {
            if let Ok(Some(status)) = c.try_wait() {
                banner::fail(&format!(
                    "The server exited ({status}) — waiting for changes to restart"
                ));
                child = None;
            }
        }

        let cur = mtime(path);
        if cur != last_mtime {
            last_mtime = cur;
            // Pausa breve para que el editor termine de escribir el archivo
            thread::sleep(Duration::from_millis(80));
            println!("\n  {DIM}{}  cambio detectado{RESET}", "─".repeat(44),
                DIM = banner::DIM, RESET = banner::RESET);
            if let Some(mut c) = child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            child = spawn("Server restarted");
        }
    }
}

fn compile_and_run(path: &str) {
    let t = Instant::now();

    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => { banner::fail(&format!("Cannot read: {e}")); return; }
    };

    let tokens = match lexer::lex(&src) {
        Ok(t) => t,
        Err(e) => { banner::fail(&format!("Lexical  line {}:{} — {}", e.line, e.col, e.message)); return; }
    };
    let stmts = match parser::parse(tokens) {
        Ok(s) => s,
        Err(e) => { banner::fail(&format!("Parse  line {} — {}", e.line, e.message)); return; }
    };
    let bc = match codegen::compile_entry(stmts) {
        Ok(b) => b,
        Err(e) => { banner::fail(&format!("Codegen  line {} — {}", e.line, e.message)); return; }
    };

    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    match machine.run() {
        Ok(_) => banner::ok(&format!("OK  {DIM}({:.1} ms){RESET}",
            t.elapsed().as_secs_f64() * 1000.0,
            DIM = banner::DIM, RESET = banner::RESET)),
        Err(e) => banner::fail(&format!("Runtime — {}", e)),
    }
}
