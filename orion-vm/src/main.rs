mod instruction;
mod value;
mod gc;
mod vm;
mod aot;
mod bytecode;
mod eval_value;
mod env;
mod builtins;
mod stdlib_bridge;
mod modules;
mod eval;
mod ai;
mod token;
mod ast;
mod lexer;
mod parser;
mod codegen;
mod pkg;
mod typechecker;
mod cli;
mod jit;
mod error;

extern crate tiny_http;

use std::env as std_env;
use std::fs;
use std::time::Instant;
use serde::Serialize;

// ─── Structs para --symbols-json ─────────────────────────────────────────────

#[derive(Serialize)]
struct SymbolParam {
    name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    type_hint: Option<String>,
}

#[derive(Serialize)]
struct ActInfo {
    name: String,
    params: Vec<SymbolParam>,
}

#[derive(Serialize)]
struct SymbolInfo {
    kind: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Vec<SymbolParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<Vec<SymbolParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    acts: Option<Vec<ActInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc: Option<String>,
    line: u32,
}

#[derive(Serialize)]
struct SymbolsResult {
    ok: bool,
    symbols: Vec<SymbolInfo>,
}

fn extract_symbols(stmts: &[ast::Stmt]) -> Vec<SymbolInfo> {
    let mut out = Vec::new();
    for stmt in stmts {
        match stmt {
            ast::Stmt::Fn { name, params, ret_type, doc, line, .. } => {
                out.push(SymbolInfo {
                    kind: "fn".into(),
                    name: name.clone(),
                    params: Some(params.iter().map(|p| SymbolParam {
                        name: p.name.clone(),
                        type_hint: p.type_hint.clone(),
                    }).collect()),
                    ret: ret_type.clone(),
                    fields: None, acts: None, data_type: None,
                    doc: doc.clone(),
                    line: *line,
                });
            }
            ast::Stmt::AsyncFn { name, params, ret_type, doc, line, .. } => {
                out.push(SymbolInfo {
                    kind: "async_fn".into(),
                    name: name.clone(),
                    params: Some(params.iter().map(|p| SymbolParam {
                        name: p.name.clone(),
                        type_hint: p.type_hint.clone(),
                    }).collect()),
                    ret: ret_type.clone(),
                    fields: None, acts: None, data_type: None,
                    doc: doc.clone(),
                    line: *line,
                });
            }
            ast::Stmt::Shape { name, fields, acts, doc, line, .. } => {
                out.push(SymbolInfo {
                    kind: "shape".into(),
                    name: name.clone(),
                    params: None, ret: None,
                    fields: Some(fields.iter().map(|f| SymbolParam {
                        name: f.name.clone(),
                        type_hint: f.type_hint.clone(),
                    }).collect()),
                    acts: Some(acts.iter().map(|a| ActInfo {
                        name: a.name.clone(),
                        params: a.params.iter().map(|p| SymbolParam {
                            name: p.name.clone(),
                            type_hint: p.type_hint.clone(),
                        }).collect(),
                    }).collect()),
                    data_type: None,
                    doc: doc.clone(),
                    line: *line,
                });
            }
            ast::Stmt::Const { name, doc, line, .. } => {
                out.push(SymbolInfo {
                    kind: "const".into(),
                    name: name.clone(),
                    params: None, ret: None, fields: None, acts: None, data_type: None,
                    doc: doc.clone(),
                    line: *line,
                });
            }
            ast::Stmt::Assign { name, line, .. } => {
                out.push(SymbolInfo {
                    kind: "var".into(),
                    name: name.clone(),
                    params: None, ret: None, fields: None, acts: None, data_type: None,
                    doc: None,
                    line: *line,
                });
            }
            ast::Stmt::TypedAssign { name, type_hint, line, .. } => {
                out.push(SymbolInfo {
                    kind: "var".into(),
                    name: name.clone(),
                    params: None, ret: None, fields: None, acts: None,
                    data_type: Some(type_hint.clone()),
                    doc: None,
                    line: *line,
                });
            }
            _ => {}
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std_env::args().collect();

    if args.len() < 2 {
        run_repl();
        return;
    }

    match args[1].as_str() {

        "--help" | "-h" => {
            print_help();
        }

        "--version" | "-v" => {
            println!("Orion VM v0.4.0 (Rust) — pipeline completo: lexer + parser + codegen + VM");
        }

        //    Verificar sintaxis (salida legible para humanos)
        "--check" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --check <archivo.orx> [--types]");
                std::process::exit(1);
            }
            let check_types = args.iter().any(|a| a == "--types");
            cli::check::run_check(&args[2], check_types);
        }

        //    Verificar sintaxis (salida JSON para LSP / tooling)
        "--check-json" => {
            // El archivo puede estar en cualquier posición después de --check-json
            // (los flags como --types pueden aparecer antes o después)
            let src_path = match args[2..].iter().find(|a| !a.starts_with("--")) {
                Some(p) => p.as_str(),
                None => {
                    let result = error::CheckResult { ok: true, diagnostics: vec![] };
                    println!("{}", serde_json::to_string(&result).unwrap());
                    return;
                }
            };
            let src = match fs::read_to_string(src_path) {
                Ok(s) => s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string(),
                Err(e) => {
                    let result = error::CheckResult {
                        ok: false,
                        diagnostics: vec![error::LspDiagnostic {
                            severity: 1,
                            kind: "IO".into(),
                            message: format!("No se puede leer '{src_path}': {e}"),
                            line: 0, col: 0, len: 0, hint: None,
                        }],
                    };
                    println!("{}", serde_json::to_string(&result).unwrap());
                    return;
                }
            };

            let mut diagnostics: Vec<error::LspDiagnostic> = Vec::new();

            // Fase 1-3: lex + parse + codegen
            match compile_source(&src, src_path) {
                Err(e) => {
                    diagnostics.push(e.to_lsp_diagnostic());
                }
                Ok(bc) => {
                    // Fase 4: type checker (si se solicita con --types)
                    if args.iter().any(|a| a == "--types") {
                        if let Ok(tokens) = lexer::lex(&src) {
                            if let Ok(stmts) = parser::parse(tokens) {
                                for issue in typechecker::type_check(&stmts) {
                                    diagnostics.push(error::type_issue_to_lsp(&issue));
                                }
                            }
                        }
                    }
                    let _ = bc;
                }
            }

            let result = error::CheckResult {
                ok: diagnostics.iter().all(|d| d.severity > 1),
                diagnostics,
            };
            println!("{}", serde_json::to_string(&result).unwrap());
        }

        //    Exportar tabla de símbolos (salida JSON para LSP hover/definition)
        "--symbols-json" => {
            let src_path = match args[2..].iter().find(|a| !a.starts_with("--")) {
                Some(p) => p.as_str(),
                None => {
                    println!("{}", serde_json::to_string(&SymbolsResult { ok: true, symbols: vec![] }).unwrap());
                    return;
                }
            };
            let src = match fs::read_to_string(src_path) {
                Ok(s) => s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string(),
                Err(_) => {
                    println!("{}", serde_json::to_string(&SymbolsResult { ok: false, symbols: vec![] }).unwrap());
                    return;
                }
            };
            let symbols = match lexer::lex(&src) {
                Ok(tokens) => match parser::parse(tokens) {
                    Ok(stmts) => extract_symbols(&stmts),
                    Err(_)    => vec![],
                },
                Err(_) => vec![],
            };
            println!("{}", serde_json::to_string(&SymbolsResult { ok: true, symbols }).unwrap());
        }

        //    Hot reload
        "--watch" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --watch <archivo.orx>");
                std::process::exit(1);
            }
            cli::watch::run_watch(&args[2]);
        }

        //    Benchmark                                                          
        "--bench" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --bench <archivo.orx> [--runs=N]");
                std::process::exit(1);
            }
            let runs = parse_runs_flag(&args, 10);
            cli::bench::run_bench(&args[2], runs);
        }

        //    Test runner                                                        
        "--test" => {
            let folder = args.get(2).map(String::as_str).unwrap_or(".");
            cli::test_runner::run_tests(folder);
        }

        //    Doctor                                                             
        "--doctor" => {
            cli::doctor::run_doctor();
        }

        //    Scaffold proyecto                                                  
        "--new" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --new <nombre-proyecto>");
                std::process::exit(1);
            }
            cli::new_project::run_new(&args[2]);
        }

        //    Package manager                                                    
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

        "--publish" => {
            pkg::publish_package();
        }

        "--build" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --build <archivo.orx> [-o <salida>]");
                std::process::exit(1);
            }
            let output = args.windows(2)
                .find(|w| w[0] == "-o")
                .map(|w| w[1].as_str());
            cli::build_native::run_build(&args[2], output);
        }

        //    Formatear código fuente
        "--format" => {
            let src_path = match args[2..].iter().find(|a| !a.starts_with("--")) {
                Some(p) => p.as_str(),
                None => {
                    cli::banner::fail("Uso: orion --format <archivo.orx> [--write]");
                    std::process::exit(1);
                }
            };
            let write_back = args.iter().any(|a| a == "--write");
            cli::format::run_format(src_path, write_back);
        }

        //    Generar documentación Markdown
        "--docs" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --docs <archivo.orx|carpeta> [--output=<dir>]");
                std::process::exit(1);
            }
            let output = args.iter()
                .find(|a| a.starts_with("--output="))
                .and_then(|a| a.strip_prefix("--output="))
                .unwrap_or("docs");
            let input = args.iter()
                .find(|a| !a.starts_with("--") && *a != &args[0] && *a != &args[1])
                .map(String::as_str)
                .unwrap_or(&args[2]);
            cli::docs::run_docs(input, output);
        }

        //    REPL
        "--repl" => run_repl(),

        //    Lexer                                                              
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
                    eprint!("{}", error::OrionError::from(e).with_file(&args[2]).render(&src));
                    std::process::exit(1);
                }
            }
        }

        //    Tree-walker evaluator                                              
        "--eval" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --eval <ast.json>");
                std::process::exit(1);
            }
            run_eval(&args[2]);
        }

        //    Compile .orx → .orbc
        "--compile" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --compile <archivo.orx>");
                std::process::exit(1);
            }
            let src_path = &args[2];
            let out_path = src_path.replace(".orx", ".orbc");
            let src = read_src(src_path);
            let bc = match compile_source(&src, src_path) {
                Ok(bc) => bc,
                Err(e) => { eprint!("{}", e.render(&src)); std::process::exit(1); }
            };
            let json = serde_json::to_string_pretty(&bc).expect("serializar bytecode");
            fs::write(&out_path, &json).expect("escribir .orbc");
            cli::banner::ok(&format!("Compilado → {out_path}"));
        }

        //    JIT (Cranelift)
        "--jit" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --jit <archivo.orx>");
                std::process::exit(1);
            }
            let src_path = &args[2];
            let t0 = Instant::now();
            let src = read_src(src_path);
            let bc = match compile_source(&src, src_path) {
                Ok(bc) => bc,
                Err(e) => { eprint!("{}", e.render(&src)); std::process::exit(1); }
            };

            match jit::run_program(&bc) {
                Ok(true) => {
                    eprintln!("[JIT] {:.3} ms — Cranelift nativo", t0.elapsed().as_secs_f64() * 1000.0);
                }
                Ok(false) => {
                    eprintln!("[JIT] Instrucciones no soportadas → fallback al intérprete");
                    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
                    match machine.run() {
                        Ok(_) => {}
                        Err(e) => {
                            eprint!("{}", error::parse_vm_error(&e, src_path).render(&src));
                            std::process::exit(1);
                        }
                    }
                    eprintln!("[Intérprete] {:.3} ms", t0.elapsed().as_secs_f64() * 1000.0);
                }
                Err(e) => {
                    cli::banner::fail(&format!("Error JIT: {e}"));
                    std::process::exit(1);
                }
            }
        }

        //    Run .orx en memoria
        "--run" => {
            if args.len() < 3 {
                cli::banner::fail("Uso: orion --run <archivo.orx>");
                std::process::exit(1);
            }
            let src_path = &args[2];
            let t_total = Instant::now();
            let src = read_src(src_path);
            let bc = match compile_source(&src, src_path) {
                Ok(bc) => bc,
                Err(e) => { eprint!("{}", e.render(&src)); std::process::exit(1); }
            };
            let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
            match machine.run() {
                Ok(_) => {}
                Err(e) => {
                    eprint!("{}", error::parse_vm_error(&e, src_path).render(&src));
                    std::process::exit(1);
                }
            }
            eprintln!("[Orion] {:.3} ms", t_total.elapsed().as_secs_f64() * 1000.0);
        }

        //    Ejecutar .orx directamente o cargar .orbc
        path => {
            let t_total = Instant::now();

            // Guardamos el source para poder renderizar errores con contexto
            let (bc, src) = if path.ends_with(".orx") {
                let src = read_src(path);
                let bc = match compile_source(&src, path) {
                    Ok(bc) => bc,
                    Err(e) => { eprint!("{}", e.render(&src)); std::process::exit(1); }
                };
                (bc, src)
            } else {
                let t0 = Instant::now();
                let instructions = match bytecode::load(path) {
                    Ok(i) => i,
                    Err(e) => {
                        cli::banner::fail(&e);
                        std::process::exit(1);
                    }
                };
                eprintln!("  Carga : {:.3} ms", t0.elapsed().as_secs_f64() * 1000.0);
                (instructions, String::new())
            };

            let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
            match machine.run() {
                Ok(_) => {}
                Err(e) => {
                    eprint!("{}", error::parse_vm_error(&e, path).render(&src));
                    std::process::exit(1);
                }
            }

            eprintln!("[Orion] {:.3} ms", t_total.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

//    Helpers                                                                   

fn print_help() {
    cli::banner::animate_startup();
    cli::banner::print_banner();
    println!("  {BOLD}Uso:{RESET}  orion <comando> [opciones]",
        BOLD = cli::banner::BOLD, RESET = cli::banner::RESET);
    println!();

    let cmds = [
        ("--run  <archivo.orx>",         "Compilar y ejecutar"),
        ("--jit  <archivo.orx>",         "Compilar y ejecutar con JIT Cranelift"),
        ("--compile <archivo.orx>",       "Compilar a .orbc (bytecode)"),
        ("--build <archivo.orx>",         "Compilar a ejecutable nativo  [-o salida]"),
        ("--check <archivo.orx>",         "Verificar sintaxis  [--types]"),
        ("--watch <archivo.orx>",         "Hot reload automático"),
        ("--bench <archivo.orx>",         "Benchmark  [--runs=N]"),
        ("--test [carpeta]",              "Ejecutar tests (test_*.orx)"),
        ("--doctor",                      "Verificar entorno"),
        ("--new <proyecto>",              "Crear scaffold de proyecto"),
        ("--repl",                        "Modo interactivo"),
        ("--lex  <archivo.orx>",          "Imprimir tokens"),
        ("--eval <ast.json>",             "Evaluador de árbol (tree-walker)"),
        ("--format <archivo.orx>",          "Formatear código fuente  [--write]"),
        ("--docs <archivo|carpeta>",       "Generar docs Markdown  [--output=dir]"),
        ("--add  <paquete>",              "Instalar paquete  [--force]"),
        ("--remove <paquete>",            "Desinstalar paquete"),
        ("--list",                        "Listar paquetes disponibles"),
        ("--search <consulta>",           "Buscar paquetes"),
        ("--update [paquete]",            "Actualizar uno o todos"),
        ("--publish",                     "Publicar paquete al registry (requiere orion.json)"),
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
        Ok(s) => s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string(),
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

/// Lex + parse + codegen → OrionBytecode, o un error estructurado con span.
pub fn compile_source(src: &str, path: &str) -> Result<bytecode::OrionBytecode, error::OrionError> {
    let tokens = lexer::lex(src)
        .map_err(|e| error::OrionError::from(e).with_file(path))?;

    let stmts = parser::parse(tokens)
        .map_err(|e| error::OrionError::from(e).with_file(path))?;

    codegen::compile(stmts)
        .map_err(|e| error::OrionError::from(e).with_file(path))
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

//    REPL                                                                      

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
    let bc = match compile_source(source, "<repl>") {
        Ok(b) => b,
        Err(e) => { eprint!("{}", e.render(source)); return; }
    };
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    match machine.run() {
        Ok(_) => session.record(source),
        Err(e) => eprint!("{}", error::parse_vm_error(&e, "<repl>").render(source)),
    }
}
