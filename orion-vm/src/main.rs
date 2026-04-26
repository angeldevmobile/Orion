mod instruction;
mod value;
mod vm;
mod bytecode;
mod eval_value;
mod env;
mod builtins;
mod stdlib_bridge;
mod eval;
mod ai;
mod token;
mod ast;
mod lexer;
mod parser;
mod codegen;
mod pkg;
mod cli;

extern crate tiny_http;

use std::env as std_env;
use std::fs;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std_env::args().collect();

    if args.len() < 2 {
        print_help();
        std::process::exit(1);
    }

    match args[1].as_str() {

        "--help" | "-h" => {
            print_help();
        }

        "--version" | "-v" => {
            println!("Orion VM v0.4.0 (Rust) — pipeline completo: lexer + parser + codegen + VM");
        }

        // ── Verificar sintaxis ────────────────────────────────────────────────
        "--check" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --check <archivo.orx> [--types]");
                std::process::exit(1);
            }
            let check_types = args.iter().any(|a| a == "--types");
            cli::check::run_check(&args[2], check_types);
        }

        // ── Hot reload ────────────────────────────────────────────────────────
        "--watch" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --watch <archivo.orx>");
                std::process::exit(1);
            }
            cli::watch::run_watch(&args[2]);
        }

        // ── Benchmark ─────────────────────────────────────────────────────────
        "--bench" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --bench <archivo.orx> [--runs=N]");
                std::process::exit(1);
            }
            let runs = parse_runs_flag(&args, 10);
            cli::bench::run_bench(&args[2], runs);
        }

        // ── Test runner ───────────────────────────────────────────────────────
        "--test" => {
            let folder = args.get(2).map(String::as_str).unwrap_or(".");
            cli::test_runner::run_tests(folder);
        }

        // ── Doctor ────────────────────────────────────────────────────────────
        "--doctor" => {
            cli::doctor::run_doctor();
        }

        // ── Scaffold proyecto ─────────────────────────────────────────────────
        "--new" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --new <nombre-proyecto>");
                std::process::exit(1);
            }
            cli::new_project::run_new(&args[2]);
        }

        // ── Package manager ───────────────────────────────────────────────────
        "--add" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --add <paquete> [--force]");
                std::process::exit(1);
            }
            let force = args.iter().any(|a| a == "--force");
            pkg::add_package(&args[2], force);
        }

        "--remove" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --remove <paquete>");
                std::process::exit(1);
            }
            pkg::remove_package(&args[2]);
        }

        "--list" => pkg::list_packages(),

        "--search" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --search <consulta>");
                std::process::exit(1);
            }
            pkg::search_packages(&args[2]);
        }

        "--update" => {
            let target = args.get(2).map(String::as_str);
            pkg::update_packages(target);
        }

        // ── REPL ──────────────────────────────────────────────────────────────
        "--repl" => run_repl(),

        // ── Lexer ─────────────────────────────────────────────────────────────
        "--lex" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --lex <archivo.orx>");
                std::process::exit(1);
            }
            let src = read_src(&args[2]);
            match lexer::lex(&src) {
                Ok(tokens) => {
                    for tok in &tokens {
                        println!("[{:>4}:{:<3}] {:?}", tok.line, tok.col, tok.kind);
                    }
                    eprintln!("[Orion] {} tokens", tokens.len());
                }
                Err(e) => {
                    cli::banner::fail(&format!("Léxico línea {}:{} — {}", e.line, e.col, e.message));
                    std::process::exit(1);
                }
            }
        }

        // ── Tree-walker evaluator ─────────────────────────────────────────────
        "--eval" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --eval <ast.json>");
                std::process::exit(1);
            }
            run_eval(&args[2]);
        }

        // ── Compile .orx → .orbc ─────────────────────────────────────────────
        "--compile" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --compile <archivo.orx>");
                std::process::exit(1);
            }
            let src_path = &args[2];
            let out_path = src_path.replace(".orx", ".orbc");
            let src = read_src(src_path);
            let bc = compile_source(&src, src_path);
            let json = serde_json::to_string_pretty(&bc).expect("serializar bytecode");
            fs::write(&out_path, &json).expect("escribir .orbc");
            cli::banner::ok(&format!("Compilado → {out_path}"));
        }

        // ── Run .orx en memoria ───────────────────────────────────────────────
        "--run" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --run <archivo.orx>");
                std::process::exit(1);
            }
            let src_path = &args[2];
            let t_total = Instant::now();
            let src = read_src(src_path);
            let bc = compile_source(&src, src_path);
            let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes);
            match machine.run() {
                Ok(_) => {}
                Err(e) => {
                    eprintln!();
                    eprintln!("  [!] Error de Orion");
                    eprintln!("  {}", e.replace('\n', "\n  "));
                    eprintln!();
                    std::process::exit(1);
                }
            }
            eprintln!("[Orion] {:.3} ms", t_total.elapsed().as_secs_f64() * 1000.0);
        }

        // ── Bytecode VM (.orbc) ───────────────────────────────────────────────
        path => {
            let t_total = Instant::now();
            let t0 = Instant::now();
            let instructions = match bytecode::load(path) {
                Ok(i) => i,
                Err(e) => {
                    cli::banner::fail(&e);
                    std::process::exit(1);
                }
            };
            let t_load = t0.elapsed();

            let t0 = Instant::now();
            let mut machine = vm::VM::new(
                instructions.main,
                instructions.lines,
                instructions.functions,
                instructions.shapes,
            );
            match machine.run() {
                Ok(_) => {}
                Err(e) => {
                    eprintln!();
                    eprintln!("  [!] Error de Orion");
                    eprintln!("  {}", e.replace('\n', "\n  "));
                    eprintln!();
                    std::process::exit(1);
                }
            }
            let t_exec = t0.elapsed();

            eprintln!();
            eprintln!("  Carga : {:.3} ms", t_load.as_secs_f64() * 1000.0);
            eprintln!("  Exec  : {:.3} ms", t_exec.as_secs_f64() * 1000.0);
            eprintln!("  Total : {:.3} ms", t_total.elapsed().as_secs_f64() * 1000.0);
            eprintln!();
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn print_help() {
    cli::banner::animate_startup();
    cli::banner::print_banner();
    println!("  {BOLD}Uso:{RESET}  orion <comando> [opciones]",
        BOLD = cli::banner::BOLD, RESET = cli::banner::RESET);
    println!();

    let cmds = [
        ("--run  <archivo.orx>",         "Compilar y ejecutar"),
        ("--compile <archivo.orx>",       "Compilar a .orbc"),
        ("--check <archivo.orx>",         "Verificar sintaxis  [--types]"),
        ("--watch <archivo.orx>",         "Hot reload automático"),
        ("--bench <archivo.orx>",         "Benchmark  [--runs=N]"),
        ("--test [carpeta]",              "Ejecutar tests (test_*.orx)"),
        ("--doctor",                      "Verificar entorno"),
        ("--new <proyecto>",              "Crear scaffold de proyecto"),
        ("--repl",                        "Modo interactivo"),
        ("--lex  <archivo.orx>",          "Imprimir tokens"),
        ("--eval <ast.json>",             "Evaluador de árbol (tree-walker)"),
        ("--add  <paquete>",              "Instalar paquete  [--force]"),
        ("--remove <paquete>",            "Desinstalar paquete"),
        ("--list",                        "Listar paquetes disponibles"),
        ("--search <consulta>",           "Buscar paquetes"),
        ("--update [paquete]",            "Actualizar uno o todos"),
        ("--version",                     "Versión del runtime"),
    ];

    let dim  = cli::banner::DIM;
    let rst  = cli::banner::RESET;
    let cyan = cli::banner::CYAN;
    for (cmd, desc) in &cmds {
        println!("  {cyan}{cmd:<32}{rst} {dim}{desc}{rst}");
    }
    println!();
}

fn read_src(path: &str) -> String {
    match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            cli::banner::fail(&format!("No se puede leer '{path}': {e}"));
            std::process::exit(1);
        }
    }
}

fn parse_runs_flag(args: &[String], default: u32) -> u32 {
    for a in args {
        if let Some(rest) = a.strip_prefix("--runs=") {
            if let Ok(n) = rest.parse::<u32>() {
                return n;
            }
        }
    }
    default
}

/// Lex + parse + codegen. Exits on any error.
pub fn compile_source(src: &str, path: &str) -> bytecode::OrionBytecode {
    let tokens = match lexer::lex(src) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[ORION LEX ERROR] {}:{}:{} — {}", path, e.line, e.col, e.message);
            std::process::exit(1);
        }
    };
    let stmts = match parser::parse(tokens) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[ORION PARSE ERROR] {}:{} — {}", path, e.line, e.message);
            std::process::exit(1);
        }
    };
    match codegen::compile(stmts) {
        Ok(bc) => bc,
        Err(e) => {
            eprintln!("[ORION CODEGEN ERROR] {}:{} — {}", path, e.line, e.message);
            std::process::exit(1);
        }
    }
}

fn run_eval(ast_path: &str) {
    let t0 = Instant::now();
    let json_src = match fs::read_to_string(ast_path) {
        Ok(s) => s,
        Err(e) => {
            cli::banner::fail(&format!("No se pudo leer '{ast_path}': {e}"));
            std::process::exit(1);
        }
    };
    let ast: serde_json::Value = match serde_json::from_str(&json_src) {
        Ok(v) => v,
        Err(e) => {
            cli::banner::fail(&format!("JSON inválido en '{ast_path}': {e}"));
            std::process::exit(1);
        }
    };
    let stmts = match ast.as_array() {
        Some(a) => a,
        None => {
            cli::banner::fail("El AST debe ser un array de sentencias");
            std::process::exit(1);
        }
    };
    match eval::run_program(stmts) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("\n  [!] Error de Orion (tree-walker)\n  {}\n", e.replace('\n', "\n  "));
            std::process::exit(1);
        }
    }
    eprintln!("[Orion] Exec: {:.3} ms", t0.elapsed().as_secs_f64() * 1000.0);
}

// ── REPL ─────────────────────────────────────────────────────────────────────

struct ReplSession {
    history: Vec<String>,  // successfully executed source snippets
}

impl ReplSession {
    fn new() -> Self { ReplSession { history: Vec::new() } }

    fn record(&mut self, src: &str) {
        self.history.push(src.to_string());
    }

    fn vars(&self) -> Vec<String> {
        let mut names = Vec::new();
        for src in &self.history {
            for line in src.lines() {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("let ").or_else(|| t.strip_prefix("const ")) {
                    let name = rest.split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next().unwrap_or("").to_string();
                    if !name.is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
        names
    }

    fn fns(&self) -> Vec<String> {
        let mut names = Vec::new();
        for src in &self.history {
            for line in src.lines() {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("fn ").or_else(|| t.strip_prefix("task ")) {
                    let name = rest.split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next().unwrap_or("").to_string();
                    if !name.is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
        names
    }
}

fn run_repl() {
    use std::io::{self, BufRead, Write};

    cli::banner::animate_startup();
    cli::banner::print_banner();
    println!("  REPL v0.4.0  —  {DIM}Ctrl+C / Ctrl+D para salir{RESET}",
        DIM = cli::banner::DIM, RESET = cli::banner::RESET);
    println!("  Comandos: {DIM}:help  :vars  :fns  :clear  :history{RESET}",
        DIM = cli::banner::DIM, RESET = cli::banner::RESET);
    println!();

    let stdin  = io::stdin();
    let stdout = io::stdout();
    let mut buf = String::new();
    let mut session = ReplSession::new();

    loop {
        {
            let mut out = stdout.lock();
            if buf.is_empty() {
                write!(out, "{CYAN}{BOLD}orion>{RESET} ",
                    CYAN = cli::banner::CYAN,
                    BOLD = cli::banner::BOLD,
                    RESET = cli::banner::RESET).ok();
            } else {
                write!(out, "  {DIM}...{RESET}   ",
                    DIM = cli::banner::DIM,
                    RESET = cli::banner::RESET).ok();
            }
            out.flush().ok();
        }

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => { println!(); break; }
            Err(_) => break,
            Ok(_) => {}
        }

        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r').trim_end();

        // REPL meta-commands
        if buf.is_empty() {
            match trimmed {
                ":help" => { repl_help(); continue; }
                ":clear" => { print!("\x1b[2J\x1b[H"); continue; }
                ":vars" => {
                    let vars = session.vars();
                    if vars.is_empty() {
                        cli::banner::info("Sin variables en esta sesión");
                    } else {
                        cli::banner::section("Variables de sesión");
                        for v in vars { println!("    {v}"); }
                    }
                    continue;
                }
                ":fns" => {
                    let fns = session.fns();
                    if fns.is_empty() {
                        cli::banner::info("Sin funciones en esta sesión");
                    } else {
                        cli::banner::section("Funciones de sesión");
                        for f in fns { println!("    {f}(...)"); }
                    }
                    continue;
                }
                ":history" => {
                    if session.history.is_empty() {
                        cli::banner::info("Historial vacío");
                    } else {
                        cli::banner::section("Historial");
                        for (i, src) in session.history.iter().enumerate() {
                            println!("  {DIM}[{i}]{RESET} {}",
                                src.lines().next().unwrap_or("").trim(),
                                DIM = cli::banner::DIM, RESET = cli::banner::RESET);
                        }
                    }
                    continue;
                }
                _ => {}
            }
        }

        if trimmed.is_empty() && !buf.is_empty() {
            let source = buf.clone();
            buf.clear();
            repl_exec(&source, &mut session);
        } else {
            buf.push_str(trimmed);
            buf.push('\n');
            let last_char = trimmed.chars().last().unwrap_or(' ');
            if last_char != '{' && last_char != ',' && last_char != '\\' {
                let source = buf.clone();
                buf.clear();
                repl_exec(&source, &mut session);
            }
        }
    }
}

fn repl_help() {
    println!();
    println!("  {BOLD}Comandos REPL:{RESET}", BOLD = cli::banner::BOLD, RESET = cli::banner::RESET);
    let cmds = [
        (":help",    "Mostrar esta ayuda"),
        (":vars",    "Listar variables definidas en la sesión"),
        (":fns",     "Listar funciones definidas en la sesión"),
        (":clear",   "Limpiar pantalla"),
        (":history", "Mostrar historial de la sesión"),
    ];
    let dim = cli::banner::DIM;
    let rst = cli::banner::RESET;
    let cy  = cli::banner::CYAN;
    for (cmd, desc) in &cmds {
        println!("  {cy}{cmd:<12}{rst} {dim}{desc}{rst}");
    }
    println!();
    println!("  {dim}Bloque multilínea: termina con una línea en blanco.{rst}");
    println!();
}

fn repl_exec(source: &str, session: &mut ReplSession) {
    let tokens = match lexer::lex(source) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("  {RED}error léxico{RESET}  línea {} — {}",
                e.line, e.message, RED = cli::banner::RED, RESET = cli::banner::RESET);
            return;
        }
    };
    let stmts = match parser::parse(tokens) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("  {RED}error sintáctico{RESET}  línea {} — {}",
                e.line, e.message, RED = cli::banner::RED, RESET = cli::banner::RESET);
            return;
        }
    };
    let bc = match codegen::compile(stmts) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("  {RED}error codegen{RESET}  línea {} — {}",
                e.line, e.message, RED = cli::banner::RED, RESET = cli::banner::RESET);
            return;
        }
    };
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes);
    match machine.run() {
        Ok(_) => session.record(source),
        Err(e) => eprintln!("  {RED}error runtime{RESET}  {}",
            e.replace('\n', "\n  "), RED = cli::banner::RED, RESET = cli::banner::RESET),
    }
}
