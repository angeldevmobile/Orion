mod instruction;
mod value;
mod gc;
mod task_pool;
mod vm;
mod aot;
mod bytecode;
mod eval_value;
mod modules;
mod ai;
mod token;
mod ast;
mod lexer;
mod parser;
mod codegen;
mod named_args;
mod paths;
mod pkg;
mod deprecated;
mod typechecker;
mod cli;
mod jit;
mod error;
mod debugger;
mod dap;

extern crate tiny_http;

use std::env as std_env;
use std::fs;
use std::time::Instant;
use serde::Serialize;

//     Structs para --symbols-json                                              

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
    #[serde(skip_serializing_if = "Option::is_none")]
    ret: Option<String>,
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
                        ret: a.ret_type.clone(),
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
    // Forzar UTF-8 en la consola de Windows para que show/print muestre
    // correctamente tildes, eñes y caracteres especiales.
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
            fn SetConsoleCP(wCodePageID: u32) -> i32;
        }
        SetConsoleOutputCP(65001);
        SetConsoleCP(65001);
    }

    let mut args: Vec<String> = std_env::args().collect();

    if args.len() < 2 {
        run_repl();
        return;
    }

    // Subcomandos modernos (estilo cargo/npm/git): `orion run x.orx`. Se
    // normalizan a su flag equivalente para reusar el dispatch; los flags
    // `--run` siguen funcionando (retrocompatibilidad total).
    if let Some(flag) = subcommand_to_flag(&args[1], args.len()) {
        args[1] = flag.to_string();
    }

    match args[1].as_str() {

        "--help" | "-h" => {
            print_help();
        }

        "--version" | "-v" => {
            println!("Orion VM v{} (Rust) — pipeline completo: lexer + parser + codegen + VM",
                     env!("CARGO_PKG_VERSION"));
        }

        //    Verificar sintaxis (salida legible para humanos)
        "--check" => {
            // La ruta es el primer argumento que no es un flag: así
            // `check archivo.orx --types` y `check --types archivo.orx`
            // funcionan igual. Antes se tomaba args[2] a secas y la segunda
            // forma intentaba abrir un archivo llamado "--types".
            let check_types = args.iter().any(|a| a == "--types");
            let path = args.iter().skip(2).find(|a| !a.starts_with("--"));
            match path {
                Some(p) => cli::check::run_check(p, check_types),
                None => {
                    cli::banner::fail("Usage: orion check <file.orx> [--types]");
                    std::process::exit(1);
                }
            }
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
                            message: format!("Cannot read '{src_path}': {e}"),
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

        //    Registro de builtins (typeshed de Orion) → JSON para el LSP
        "--builtins-json" => {
            cli::builtins::run_builtins_json();
        }

        //    Hot reload
        "--watch" => {
            if args.len() < 3 {
                cli::banner::fail("Usage: orion --watch <file.orx>");
                std::process::exit(1);
            }
            cli::watch::run_watch(&args[2]);
        }

        //    Benchmark                                                          
        "--bench" => {
            if args.len() < 3 {
                cli::banner::fail("Usage: orion --bench <file.orx> [--runs=N]");
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
                cli::banner::fail("Usage: orion --new <project-name>");
                std::process::exit(1);
            }
            cli::new_project::run_new(&args[2]);
        }

        //    Package manager                                                    
        "--add" => {
            if args.len() < 3 {
                cli::banner::fail("Usage: orion --add <package|url|gh:owner/repo|path.orx> [--force] [--sha256 <hex>]");
                std::process::exit(1);
            }
            let force = args.iter().any(|a| a == "--force");
            let sha = args.windows(2)
                .find(|w| w[0] == "--sha256")
                .map(|w| w[1].as_str());
            pkg::add_package(&args[2], force, sha);
        }

        "--install" => pkg::install_project(),

        "--remove" => {
            if args.len() < 3 {
                cli::banner::fail("Usage: orion --remove <package>");
                std::process::exit(1);
            }
            pkg::remove_package(&args[2]);
        }

        "--list" => pkg::list_packages(),

        "--search" => {
            if args.len() < 3 {
                cli::banner::fail("Usage: orion --search <query>");
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
                cli::banner::fail("Usage: orion --build <file.orx> [-o <output>]");
                std::process::exit(1);
            }
            let output = args.windows(2)
                .find(|w| w[0] == "-o")
                .map(|w| w[1].as_str());
            cli::build_native::run_build(&args[2], output);
        }

        //    Debugger interactivo
        "--debug" => {
            let src_path = match args.get(2) {
                Some(p) if !p.starts_with("--") => p.as_str(),
                _ => {
                    cli::banner::fail("Usage: orion --debug <file.orx>");
                    std::process::exit(1);
                }
            };
            cli::debug::run_debug(src_path);
        }

        //    DAP server para VS Code
        "--dap" => {
            let src_path = match args.get(2) {
                Some(p) if !p.starts_with("--") => p.as_str(),
                _ => {
                    eprintln!("[dap] Usage: orion --dap <file.orx>");
                    std::process::exit(1);
                }
            };
            dap::run_dap(src_path);
        }

        //    Formatear código fuente
        "--format" => {
            let src_path = match args[2..].iter().find(|a| !a.starts_with("--")) {
                Some(p) => p.as_str(),
                None => {
                    cli::banner::fail("Usage: orion --format <file.orx> [--write | --check]");
                    std::process::exit(1);
                }
            };
            let write_back = args.iter().any(|a| a == "--write");
            let check_only = args.iter().any(|a| a == "--check");
            cli::format::run_format(src_path, write_back, check_only);
        }

        //    Generar documentación Markdown
        "--docs" => {
            if args.len() < 3 {
                cli::banner::fail("Usage: orion --docs <archivo.orx|carpeta> [--output=<dir>]");
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
                cli::banner::fail("Usage: orion --lex <file.orx>");
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

        //    Compile .orx → .orbc
        "--compile" => {
            if args.len() < 3 {
                cli::banner::fail("Usage: orion --compile <file.orx>");
                std::process::exit(1);
            }
            let src_path = &args[2];
            let out_path = src_path.replace(".orx", ".orbc");
            let src = read_src(src_path);
            let bc = match compile_source(&src, src_path) {
                Ok(bc) => bc,
                Err(e) => { eprint!("{}", e.render(&src)); std::process::exit(1); }
            };
            // Formato binario: ~10-50x más rápido de cargar que JSON
            let use_json = args.iter().any(|a| a == "--json");
            if use_json {
                bytecode::save_json(&bc, &out_path).unwrap_or_else(|e| {
                    cli::banner::fail(&e); std::process::exit(1);
                });
            } else {
                bytecode::save(&bc, &out_path).unwrap_or_else(|e| {
                    cli::banner::fail(&e); std::process::exit(1);
                });
            }
            cli::banner::ok(&format!("Compilado → {out_path}{}",
                if use_json { " (JSON)" } else { " (binario)" }));
        }

        //    JIT (Cranelift)
        "--jit" => {
            if args.len() < 3 {
                cli::banner::fail("Usage: orion --jit <file.orx>");
                std::process::exit(1);
            }
            let src_path = &args[2];
            let t0 = Instant::now();
            let src = read_src(src_path);
            typecheck_gate(&src, src_path, &args);
            let bc = match compile_source(&src, src_path) {
                Ok(bc) => bc,
                Err(e) => { eprint!("{}", e.render(&src)); std::process::exit(1); }
            };

            let jit_outcome = jit::run_program(&bc);
            match jit_outcome {
                Ok(true) => {
                    eprintln!("[JIT] {:.3} ms — Cranelift nativo", t0.elapsed().as_secs_f64() * 1000.0);
                }
                other => {
                    match other {
                        Err(e) => eprintln!("[JIT] {e} → falling back to the interpreter"),
                        _      => eprintln!("[JIT] Unsupported instructions → falling back to the interpreter"),
                    }
                    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
                    match machine.run() {
                        Ok(_) => {}
                        Err(e) => {
                            eprint!("{}", error::parse_vm_error(&e, src_path).render(&src));
                            std::process::exit(1);
                        }
                    }
                    eprintln!("[Interpreter] {:.3} ms", t0.elapsed().as_secs_f64() * 1000.0);
                }
            }
        }

        //    Run .orx en memoria
        "--run" => {
            if args.len() < 3 {
                cli::banner::fail("Usage: orion --run <file.orx>");
                std::process::exit(1);
            }
            let src_path = &args[2];
            let t_total = Instant::now();
            let src = read_src(src_path);
            typecheck_gate(&src, src_path, &args);
            let bc = match compile_source(&src, src_path) {
                Ok(bc) => bc,
                Err(e) => { eprint!("{}", e.render(&src)); std::process::exit(1); }
            };
            let profile = args.iter().any(|a| a == "--profile");
            modules::gui::state::set_script_path(src_path);
            let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
            match machine.run() {
                Ok(_) => {}
                Err(e) => {
                    eprint!("{}", error::parse_vm_error(&e, src_path).render(&src));
                    std::process::exit(1);
                }
            }
            eprintln!("[Orion] {:.3} ms", t_total.elapsed().as_secs_f64() * 1000.0);
            if profile { print_hotspots(&machine); }
        }

        //    Ejecutar .orx directamente o cargar .orbc
        path => {
            // Si no parece un archivo y no existe, es un comando mal escrito.
            if !path.ends_with(".orx") && !path.ends_with(".orbc")
                && !std::path::Path::new(path).exists()
            {
                cli::banner::fail(&format!("Comando o archivo desconocido: '{path}'"));
                eprintln!("  Try {BOLD}orion help{RESET} to see the commands.",
                    BOLD = cli::banner::BOLD, RESET = cli::banner::RESET);
                std::process::exit(1);
            }

            let t_total = Instant::now();

            // Guardamos el source para poder renderizar errores con contexto
            let (bc, src) = if path.ends_with(".orx") {
                let src = read_src(path);
                typecheck_gate(&src, path, &args);
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

            let profile = args.iter().any(|a| a == "--profile");
            // Registrar ruta para que gui.run() pueda re-evaluar en modo reactivo
            if path.ends_with(".orx") {
                modules::gui::state::set_script_path(path);
            }
            let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
            match machine.run() {
                Ok(_) => {}
                Err(e) => {
                    eprint!("{}", error::parse_vm_error(&e, path).render(&src));
                    std::process::exit(1);
                }
            }

            eprintln!("[Orion] {:.3} ms", t_total.elapsed().as_secs_f64() * 1000.0);
            if profile { print_hotspots(&machine); }
        }
    }
}

fn print_hotspots(machine: &vm::VM) {
    let spots = machine.hotspots(10);
    if spots.is_empty() { return; }
    eprintln!("\n  Hotspots (most-called functions):");
    eprintln!("  {:<30} {:>8}", "Function", "Llamadas");
    eprintln!("  {}", "-".repeat(42));
    for (name, count) in spots {
        eprintln!("  {:<30} {:>8}", name, count);
    }
    eprintln!("  Tip: use 'orion --jit <file.orx>' to compile with Cranelift.");
}

fn subcommand_to_flag(s: &str, argc: usize) -> Option<&'static str> {
    if s == "install" {
        return Some(if argc > 2 { "--add" } else { "--install" });
    }
    Some(match s {
        "run"                 => "--run",
        "jit"                 => "--jit",
        "compile"             => "--compile",
        "build"               => "--build",
        "check"               => "--check",
        "test"                => "--test",
        "repl"                => "--repl",
        "new"                 => "--new",
        "add"                 => "--add",
        "remove" | "uninstall"=> "--remove",
        "list"                => "--list",
        "search"              => "--search",
        "update" | "upgrade"  => "--update",
        "publish"             => "--publish",
        "fmt" | "format"      => "--format",
        "doctor"              => "--doctor",
        "bench"               => "--bench",
        "watch"               => "--watch",
        "docs"                => "--docs",
        "debug"               => "--debug",
        "dap"                 => "--dap",
        "lex"                 => "--lex",
        "version"             => "--version",
        "help"                => "--help",
        _ => return None,
    })
}

fn print_help() {
    // Sin animación de arranque: `help` debe ser instantáneo.
    cli::banner::print_banner();
    println!("  {BOLD}Usage:{RESET}  orion <command> [options]",
        BOLD = cli::banner::BOLD, RESET = cli::banner::RESET);
    println!();

    // Grupos de subcomandos (estilo moderno). Los flags `--x` siguen valiendo.
    let groups: [(&str, &[(&str, &str)]); 4] = [
        ("Run and build", &[
            ("run <file.orx>",     "Compile and run  [--no-typecheck]"),
            ("jit <file.orx>",     "Run with the Cranelift JIT"),
            ("build <file.orx>",   "Build a native executable  [-o output]"),
            ("compile <file.orx>", "Compile to .orbc (bytecode)"),
            ("check <file.orx>",   "Check syntax and types  [--types]"),
        ]),
        ("Project", &[
            ("new <project>",        "Scaffold a new project"),
            ("test [folder]",        "Run tests (test_*.orx)"),
            ("repl",                  "Interactive mode"),
            ("doctor",                "Check the environment"),
        ]),
        ("Packages", &[
            ("install",               "Install the dependencies in orion.json  (writes orion.lock)"),
            ("add <source>",          "Add a package: name | url | gh:owner/repo | path.orx  [--force] [--sha256 <hex>]"),
            ("remove <package>",      "Uninstall a package  (alias: uninstall)"),
            ("list",                  "List available and installed packages"),
            ("search <query>",     "Search packages"),
            ("update [package]",      "Update one or all"),
            ("publish",               "Publish to the registry (requires orion.json)"),
        ]),
        ("Tools", &[
            ("fmt <file.orx>",     "Format source code  [--write | --check]  (alias: format)"),
            ("watch <file.orx>",   "Automatic hot reload"),
            ("bench <file.orx>",   "Benchmark  [--runs=N]"),
            ("debug <file.orx>",   "Debugger interactivo (breakpoints, step, watch)"),
            ("docs <archivo|carpeta>","Generar docs Markdown  [--output=dir]"),
            ("lex <file.orx>",     "Imprimir tokens"),
        ]),
    ];

    let dim   = cli::banner::DIM;
    let rst   = cli::banner::RESET;
    let cyan  = cli::banner::CYAN;
    let bold  = cli::banner::BOLD;
    for (titulo, cmds) in &groups {
        println!("  {bold}{titulo}{rst}");
        for (cmd, desc) in *cmds {
            println!("    {cyan}{cmd:<26}{rst} {dim}{desc}{rst}");
        }
        println!();
    }
    println!("  {dim}Atajo:{rst}  orion <file.orx>        {dim}ejecuta directamente (= orion run)");
    println!("  {dim}Help:{rst}   orion help  ·  orion version");
    println!("  {dim}The classic flags (--run, --build, …) still work.{rst}");
    println!();
}

fn read_src(path: &str) -> String {
    paths::set_entry_file(path);
    match fs::read_to_string(path) {
        Ok(s) => s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string(),
        Err(e) => {
            cli::banner::fail(&format!("Cannot read '{path}': {e}"));
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

fn typecheck_gate(src: &str, path: &str, args: &[String]) {
    if args.iter().any(|a| a == "--no-typecheck") {
        return;
    }
    let tokens = match lexer::lex(src) { Ok(t) => t, Err(_) => return };
    let stmts  = match parser::parse(tokens) { Ok(s) => s, Err(_) => return };

    let errors: Vec<_> = typechecker::type_check(&stmts)
        .into_iter()
        .filter(|i| i.kind == "error")
        .collect();
    if errors.is_empty() { return; }

    for e in &errors {
        let err = error::OrionError::new(
            error::ErrorKind::Type,
            e.message.clone(),
            error::Span::new(e.line, e.col.max(1)),
        ).with_file(path);
        eprint!("{}", err.render(src));
    }
    cli::banner::fail(&format!(
        "{} type error(s) — execution aborted. Use --no-typecheck to run anyway.",
        errors.len()
    ));
    std::process::exit(1);
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
    println!("  REPL v{V}  —  {DIM}Ctrl+C / Ctrl+D to exit{RESET}",
        V = env!("CARGO_PKG_VERSION"),
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
                        cli::banner::info("No variables in this session");
                    } else {
                        cli::banner::section("Session variables");
                        for v in vars { println!("    {v}"); }
                    }
                    continue;
                }
                ":fns" => {
                    let fns = session.fns();
                    if fns.is_empty() {
                        cli::banner::info("No functions in this session");
                    } else {
                        cli::banner::section("Session functions");
                        for f in fns { println!("    {f}(...)"); }
                    }
                    continue;
                }
                ":history" => {
                    if session.history.is_empty() {
                        cli::banner::info("History is empty");
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
        (":vars",    "List variables defined in the session"),
        (":fns",     "List functions defined in the session"),
        (":clear",   "Limpiar pantalla"),
        (":history", "Show the session history"),
    ];
    let dim = cli::banner::DIM;
    let rst = cli::banner::RESET;
    let cy  = cli::banner::CYAN;
    for (cmd, desc) in &cmds {
        println!("  {cy}{cmd:<12}{rst} {dim}{desc}{rst}");
    }
    println!();
    println!("  {dim}Multi-line block: end it with a blank line.{rst}");
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
