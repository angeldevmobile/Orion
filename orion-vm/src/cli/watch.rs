use std::time::{Duration, Instant, SystemTime};
use std::thread;
use std::fs;
use std::sync::atomic::Ordering;
use crate::{lexer, parser, codegen, vm};
use crate::modules::gui;
use super::banner;

pub fn run_watch(path: &str) {
    let lista = vigilados(path);
    let extra = lista.len().saturating_sub(1);
    banner::info(&format!(
        "Watching {BOLD}{path}{RESET}{}  {DIM}(Ctrl+C to stop){RESET}",
        if extra > 0 {
            format!(" {DIM}+ {extra} imported{RESET}", DIM = banner::DIM, RESET = banner::RESET)
        } else {
            String::new()
        },
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
    let mut lista = vigilados(path);
    let mut huella_ant = huella(&lista);
    let mut ultimo_cambio = Instant::now();
    loop {
        thread::sleep(intervalo(ultimo_cambio.elapsed()));
        if huella(&lista) != huella_ant {
            // La lista se rehace AQUÍ, no en cada vuelta: solo puede cambiar
            // cuando cambia un archivo, y recalcularla exige leer y lexar cada
            // uno. Hacerlo en cada sondeo costaba un 7% de núcleo; sondear
            // stats no llega a medirse.
            lista = vigilados(path);
            huella_ant = huella(&lista);
            ultimo_cambio = Instant::now();
            aviso_cambio();
            compile_and_run(path);
        }
    }
}

/// Cada cuánto se mira. Rápido mientras se está editando, espaciado en
/// reposo: medido, 375 `stat` en 30 s no llegan a gastar 1 ms de CPU, así que
/// el margen para ir rápido es enorme y lo que se gana es latencia.
fn intervalo(desde_ultimo_cambio: Duration) -> Duration {
    if desde_ultimo_cambio < Duration::from_secs(90) {
        Duration::from_millis(120)
    } else {
        Duration::from_millis(800)
    }
}

fn aviso_cambio() {
    println!("\n  {DIM}{}  change detected{RESET}", "─".repeat(44),
        DIM = banner::DIM, RESET = banner::RESET);
}

/// Los archivos que hay que vigilar: el de entrada y todo lo que importa,
/// recursivamente.
///
/// Sin esto solo se miraba el archivo de entrada, así que tocar un módulo no
/// recargaba nada: el desarrollador guardaba, no pasaba nada, y acababa
/// dudando de si el watch funcionaba. La lista se recalcula en cada vuelta
/// porque un `use` nuevo también tiene que empezar a vigilarse.
fn vigilados(entrada: &str) -> Vec<std::path::PathBuf> {
    use std::collections::HashSet;

    let raiz = std::path::PathBuf::from(entrada);
    let mut fuera: Vec<std::path::PathBuf> = vec![raiz.clone()];
    let mut vistos: HashSet<std::path::PathBuf> = HashSet::new();
    vistos.insert(raiz.clone());

    let mut cola = vec![raiz];
    // Tope de profundidad: un ciclo de imports no debe colgar el watch.
    let mut vueltas = 0;

    while let Some(actual) = cola.pop() {
        vueltas += 1;
        if vueltas > 200 { break; }

        let Ok(src) = fs::read_to_string(&actual) else { continue };
        for m in imports(&src) {
            // Solo los `use` que resuelven a un .orx del proyecto: los
            // módulos nativos no son archivos y no hay nada que vigilar.
            if let Some(f) = crate::paths::resolve_module_file(&m) {
                if vistos.insert(f.clone()) {
                    fuera.push(f.clone());
                    cola.push(f);
                }
            }
        }
    }
    fuera
}

/// Las rutas de los `use` de un programa, con el lexer y no a ojo: así un
/// `use` dentro de un comentario o de una cadena no cuenta.
fn imports(src: &str) -> Vec<String> {
    use crate::token::TokenKind;
    let Ok(tokens) = lexer::lex(src) else { return Vec::new() };

    let mut fuera = Vec::new();
    let mut i = 0;
    while i + 1 < tokens.len() {
        if matches!(tokens[i].kind, TokenKind::Use) {
            match &tokens[i + 1].kind {
                TokenKind::Str(s)   => fuera.push(s.clone()),
                TokenKind::Ident(n) => fuera.push(n.clone()),
                _ => {}
            }
        }
        i += 1;
    }
    fuera
}

/// Un cambio en cualquiera de los archivos cambia la huella.
fn huella(archivos: &[std::path::PathBuf]) -> Vec<Option<SystemTime>> {
    archivos.iter()
        .map(|f| fs::metadata(f).ok().and_then(|m| m.modified().ok()))
        .collect()
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
            "{reason}  {DIM}(server runs as a child process){RESET}",
            DIM = banner::DIM, RESET = banner::RESET
        ));
        match Command::new(&exe).arg("run").arg(path).spawn() {
            Ok(c) => Some(c),
            Err(e) => { banner::fail(&format!("Could not start the server: {e}")); None }
        }
    };

    let mut child = spawn("Server started");
    let mut lista = vigilados(path);
    let mut huella_ant = huella(&lista);
    let mut ultimo_cambio = Instant::now();

    loop {
        thread::sleep(intervalo(ultimo_cambio.elapsed()));

        // ¿El servidor murió solo? Avisar una vez y esperar cambios.
        if let Some(c) = child.as_mut() {
            if let Ok(Some(status)) = c.try_wait() {
                banner::fail(&format!(
                    "The server exited ({status}) — waiting for changes to restart"
                ));
                child = None;
            }
        }

        if huella(&lista) != huella_ant {
            // Rehacer la lista solo al cambiar algo: leer y lexar los archivos
            // en cada sondeo costaba un 7% de núcleo.
            lista = vigilados(path);
            huella_ant = huella(&lista);
            ultimo_cambio = Instant::now();
            // Pausa breve para que el editor termine de escribir el archivo
            thread::sleep(Duration::from_millis(60));
            aviso_cambio();
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
