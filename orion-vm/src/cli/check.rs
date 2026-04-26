use std::fs;
use crate::{lexer, parser, codegen};
use super::banner;

pub fn run_check(path: &str, check_types: bool) {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            banner::fail(&format!("No se puede leer '{path}': {e}"));
            std::process::exit(1);
        }
    };

    banner::info(&format!("Verificando: {BOLD}{path}{RESET}",
        BOLD = super::banner::BOLD, RESET = super::banner::RESET, path = path));

    // Phase 1: lex
    let tokens = match lexer::lex(&src) {
        Ok(t) => t,
        Err(e) => {
            banner::fail(&format!("Error léxico  línea {}:{} — {}", e.line, e.col, e.message));
            std::process::exit(1);
        }
    };

    // Phase 2: parse
    let stmts = match parser::parse(tokens) {
        Ok(s) => s,
        Err(e) => {
            banner::fail(&format!("Error sintáctico  línea {} — {}", e.line, e.message));
            std::process::exit(1);
        }
    };

    // Phase 3: codegen (catches undefined jump targets, duplicate names, etc.)
    if let Err(e) = codegen::compile(stmts) {
        banner::fail(&format!("Error semántico  línea {} — {}", e.line, e.message));
        std::process::exit(1);
    }

    if check_types {
        banner::warn("Verificación de tipos (--types): en desarrollo — coming soon");
    }

    banner::ok(&format!("'{path}' — sin errores"));
}
