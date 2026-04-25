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

extern crate tiny_http;

use std::env as std_env;
use std::fs;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std_env::args().collect();

    if args.len() < 2 {
        eprintln!("Uso: orion <archivo.orbc>");
        eprintln!("     orion --compile <archivo.orx>   (compila a .orbc sin Python)");
        eprintln!("     orion --run     <archivo.orx>   (compila y ejecuta en memoria)");
        eprintln!("     orion --repl                    (modo interactivo)");
        eprintln!("     orion --lex     <archivo.orx>   (imprime tokens)");
        eprintln!("     orion --eval    <ast.json>      (evaluador de árbol)");
        eprintln!("     orion --version");
        std::process::exit(1);
    }

    if args[1] == "--version" {
        println!("Orion VM v0.4.0 (Rust) — pipeline completo: lexer + parser + codegen + VM");
        return;
    }

    //    Modo REPL                                                             
    if args[1] == "--repl" {
        run_repl();
        return;
    }

    //    Modo compile: .orx → .orbc                                            
    if args[1] == "--compile" {
        if args.len() < 3 {
            eprintln!("Uso: orion --compile <archivo.orx>");
            std::process::exit(1);
        }
        let src_path = &args[2];
        let out_path = src_path.replace(".orx", ".orbc");
        let src = match fs::read_to_string(src_path) {
            Ok(s) => s,
            Err(e) => { eprintln!("[ORION ERROR] {}", e); std::process::exit(1); }
        };
        let bc = compile_source(&src, src_path);
        let json = serde_json::to_string_pretty(&bc).expect("serializar bytecode");
        fs::write(&out_path, &json).expect("escribir .orbc");
        eprintln!("[Orion] Compilado → {}", out_path);
        return;
    }

    //    Modo run: .orx → compilar + ejecutar en memoria                       
    if args[1] == "--run" {
        if args.len() < 3 {
            eprintln!("Uso: orion --run <archivo.orx>");
            std::process::exit(1);
        }
        let src_path = &args[2];
        let t_total = Instant::now();
        let src = match fs::read_to_string(src_path) {
            Ok(s) => s,
            Err(e) => { eprintln!("[ORION ERROR] {}", e); std::process::exit(1); }
        };
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
        return;
    }

    //    Modo lexer                                                            
    if args[1] == "--lex" {
        if args.len() < 3 {
            eprintln!("Uso: orion --lex <archivo.orn>");
            std::process::exit(1);
        }
        let src = match fs::read_to_string(&args[2]) {
            Ok(s) => s,
            Err(e) => { eprintln!("[ORION ERROR] {}", e); std::process::exit(1); }
        };
        match lexer::lex(&src) {
            Ok(tokens) => {
                for tok in &tokens {
                    println!("[{:>4}:{:<3}] {:?}", tok.line, tok.col, tok.kind);
                }
                eprintln!("[Orion] {} tokens", tokens.len());
            }
            Err(e) => { eprintln!("[ORION ERROR] {}", e); std::process::exit(1); }
        }
        return;
    }

    //    Modo evaluador de árbol (Fase 5B)                                
    if args[1] == "--eval" {
        if args.len() < 3 {
            eprintln!("Uso: orion --eval <ast.json>");
            std::process::exit(1);
        }
        let ast_path = &args[2];
        let t0 = Instant::now();

        let json_src = match fs::read_to_string(ast_path) {
            Ok(s)  => s,
            Err(e) => {
                eprintln!("[ORION ERROR] No se pudo leer '{}': {}", ast_path, e);
                std::process::exit(1);
            }
        };

        let ast: serde_json::Value = match serde_json::from_str(&json_src) {
            Ok(v)  => v,
            Err(e) => {
                eprintln!("[ORION ERROR] JSON inválido en '{}': {}", ast_path, e);
                std::process::exit(1);
            }
        };

        let stmts = match ast.as_array() {
            Some(a) => a,
            None    => {
                eprintln!("[ORION ERROR] El AST debe ser un array de sentencias");
                std::process::exit(1);
            }
        };

        match eval::run_program(stmts) {
            Ok(_) => {}
            Err(e) => {
                eprintln!();
                eprintln!("  [!] Error de Orion (tree-walker)");
                eprintln!("  {}", e.replace('\n', "\n  "));
                eprintln!();
                std::process::exit(1);
            }
        }

        let elapsed = t0.elapsed();
        eprintln!("[Orion] Exec: {:.3} ms", elapsed.as_secs_f64() * 1000.0);
        return;
    }

    //    Modo bytecode VM (original)                                       
    let path    = &args[1];
    let t_total = Instant::now();

    let t0 = Instant::now();
    let instructions = match bytecode::load(path) {
        Ok(instrs) => instrs,
        Err(e) => {
            eprintln!("[ORION ERROR] {}", e);
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
    let t_exec  = t0.elapsed();
    let t_total = t_total.elapsed();

    eprintln!();
    eprintln!("  Carga   : {:.3} ms", t_load.as_secs_f64() * 1000.0);
    eprintln!("  Exec    : {:.3} ms", t_exec.as_secs_f64() * 1000.0);
    eprintln!("  Total   : {:.3} ms", t_total.as_secs_f64() * 1000.0);
    eprintln!();
}

//    Helpers compartidos                                                        

/// Lex + parse + codegen de una cadena fuente.
/// Termina el proceso si hay errores.
fn compile_source(src: &str, path: &str) -> bytecode::OrionBytecode {
    let tokens = match lexer::lex(src) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[ORION LEX ERROR] {}:{} — {}", path, e.line, e.message);
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

//    REPL                                                                       

fn run_repl() {
    use std::io::{self, BufRead, Write};

    println!("Orion REPL v0.4.0  (Ctrl+C / Ctrl+D para salir)");
    println!("  Escribe código Orion línea a línea.");
    println!("  Bloque multilínea: termina con una línea en blanco.");
    println!();

    let stdin  = io::stdin();
    let stdout = io::stdout();
    let mut buf = String::new();

    loop {
        // Prompt
        {
            let mut out = stdout.lock();
            if buf.is_empty() {
                write!(out, "orion> ").ok();
            } else {
                write!(out, "  ... ").ok();
            }
            out.flush().ok();
        }

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                break; // EOF (Ctrl+D)
            }
            Err(_) => break,
            Ok(_) => {}
        }

        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');

        if trimmed.is_empty() && !buf.is_empty() {
            // Línea en blanco → ejecutar lo acumulado
            let source = buf.clone();
            buf.clear();
            repl_exec(&source);
        } else {
            buf.push_str(trimmed);
            buf.push('\n');

            // Intentar ejecutar si la línea parece completa (no termina en { ni ,)
            let last_char = trimmed.chars().last().unwrap_or(' ');
            if last_char != '{' && last_char != ',' && last_char != '\\' {
                let source = buf.clone();
                buf.clear();
                repl_exec(&source);
            }
        }
    }
}

fn repl_exec(source: &str) {
    let tokens = match lexer::lex(source) {
        Ok(t) => t,
        Err(e) => { eprintln!("[error léxico] línea {} — {}", e.line, e.message); return; }
    };
    let stmts = match parser::parse(tokens) {
        Ok(s) => s,
        Err(e) => { eprintln!("[error sintáctico] línea {} — {}", e.line, e.message); return; }
    };
    let bc = match codegen::compile(stmts) {
        Ok(b) => b,
        Err(e) => { eprintln!("[error codegen] línea {} — {}", e.line, e.message); return; }
    };
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes);
    if let Err(e) = machine.run() {
        eprintln!("[error runtime] {}", e.replace('\n', "\n  "));
    }
}
