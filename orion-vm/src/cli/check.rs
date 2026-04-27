use std::fs;
use crate::{lexer, parser, codegen, typechecker};
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

    // Phase 3: type check (antes de codegen para errores más claros)
    if check_types {
        let issues = typechecker::type_check(&stmts);
        if issues.is_empty() {
            banner::ok("Type check — sin errores de tipos");
        } else {
            let errors: Vec<_> = issues.iter().filter(|i| i.kind == "error").collect();
            let warnings: Vec<_> = issues.iter().filter(|i| i.kind == "warning").collect();
            for w in &warnings {
                let prefix = if w.line > 0 { format!("línea {} — ", w.line) } else { String::new() };
                banner::warn(&format!("[advertencia] {}{}", prefix, w.message));
            }
            for e in &errors {
                let prefix = if e.line > 0 { format!("línea {} — ", e.line) } else { String::new() };
                banner::fail(&format!("[tipo] {}{}", prefix, e.message));
            }
            if !errors.is_empty() {
                std::process::exit(1);
            }
        }
    }

    // Phase 4: codegen (detecta errores semánticos adicionales)
    if let Err(e) = codegen::compile(stmts) {
        banner::fail(&format!("Error semántico  línea {} — {}", e.line, e.message));
        std::process::exit(1);
    }

    banner::ok(&format!("'{path}' — sin errores"));
}
