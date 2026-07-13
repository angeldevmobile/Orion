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

    // Servidores: `serve` bloquea dentro de la evaluación, así que el script
    // corre como proceso hijo que se mata y relanza en cada cambio (estilo
    // nodemon). Hay que detectarlo ANTES de la primera evaluación in-process.
    if script_has_serve(path) {
        run_watch_server(path);
        return;
    }

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

/// ¿El script usa `serve`? Se decide a nivel de tokens (no hace falta parsear,
/// y así cuenta también un serve dentro de una función). Strings y comentarios
/// no llegan como keyword, por lo que no dan falsos positivos.
fn script_has_serve(path: &str) -> bool {
    let Ok(src) = fs::read_to_string(path) else { return false };
    let Ok(tokens) = lexer::lex(&src) else { return false };
    tokens.iter().any(|t| matches!(t.kind, crate::token::TokenKind::Serve))
}

/// Watch para servidores: el script corre en un proceso hijo (`orion run`) que
/// se termina y relanza en cada cambio. Si el servidor muere solo (error de
/// arranque, puerto ocupado…), el watcher queda esperando el próximo guardado.
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
            Err(e) => { banner::fail(&format!("No se pudo lanzar el servidor: {e}")); None }
        }
    };

    let mut child = spawn("Servidor iniciado");
    let mut last_mtime = mtime(path);

    loop {
        thread::sleep(Duration::from_millis(400));

        // ¿El servidor murió solo? Avisar una vez y esperar cambios.
        if let Some(c) = child.as_mut() {
            if let Ok(Some(status)) = c.try_wait() {
                banner::fail(&format!(
                    "El servidor terminó ({status}) — esperando cambios para reiniciar"
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
            child = spawn("Servidor reiniciado");
        }
    }
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
