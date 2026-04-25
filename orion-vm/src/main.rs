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
mod lexer;

extern crate tiny_http;

use std::env as std_env;
use std::fs;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std_env::args().collect();

    if args.len() < 2 {
        eprintln!("Uso: orion <archivo.orbc>");
        eprintln!("     orion --eval <ast.json>   (evaluador de árbol)");
        eprintln!("     orion --version");
        std::process::exit(1);
    }

    if args[1] == "--version" {
        println!("Orion VM v0.3.0 (Rust) — bytecode + tree-walker + lexer");
        return;
    }

    // ── Modo lexer ───────────────────────────────────────────────────────────
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

    // ── Modo evaluador de árbol (Fase 5B) ───────────────────────────────
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

    // ── Modo bytecode VM (original) ──────────────────────────────────────
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
