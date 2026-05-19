//! Pruebas de regresión end-to-end: lex → parse → codegen → VM.
//!
//! Cada test corre un programa Orion completo y verifica que el resultado
//! (Ok / Err) sea el esperado. stdout queda capturado por el harness de cargo.

use orion_vm::{codegen, lexer, parser, vm};

fn run_ok(src: &str) {
    let tokens = lexer::lex(src)
        .unwrap_or_else(|e| panic!("lex error: {} | src: {}", e.message, src));
    let stmts = parser::parse(tokens)
        .unwrap_or_else(|e| panic!("parse error: {} | src: {}", e.message, src));
    let bc = codegen::compile(stmts)
        .unwrap_or_else(|e| panic!("codegen error: {} | src: {}", e.message, src));
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    machine
        .run()
        .unwrap_or_else(|e| panic!("runtime error: {} | src: {}", e, src));
}

fn run_err(src: &str) -> String {
    let tokens = lexer::lex(src)
        .unwrap_or_else(|e| panic!("lex error: {}", e.message));
    let stmts = parser::parse(tokens)
        .unwrap_or_else(|e| panic!("parse error: {}", e.message));
    let bc = codegen::compile(stmts)
        .unwrap_or_else(|e| panic!("codegen error: {}", e.message));
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    machine.run().expect_err("se esperaba un error en tiempo de ejecución")
}

// ── Literales y aritmética ──────────────────────────────────────────────────

#[test]
fn test_literal_int() {
    run_ok("x = 42");
}

#[test]
fn test_arithmetic_precedence() {
    // 2 + 3 * 4 = 14: verifica que no haya pánico y que el pipeline funcione
    run_ok("resultado = 2 + 3 * 4");
}

#[test]
fn test_show_string() {
    run_ok(r#"show "hola orion""#);
}

// ── Variables y funciones ───────────────────────────────────────────────────

#[test]
fn test_variable_reassignment() {
    run_ok("x = 1\nx = x + 1\nx = x + 1");
}

#[test]
fn test_function_definition_and_call() {
    run_ok("fn doble(n) { return n * 2 }\nresultado = doble(7)");
}

#[test]
fn test_recursive_function() {
    run_ok(
        r#"fn fact(n) {
    if n <= 1 { return 1 }
    return n * fact(n - 1)
}
r = fact(5)"#,
    );
}

// ── Control de flujo ────────────────────────────────────────────────────────

#[test]
fn test_if_else_branches() {
    run_ok("if 10 > 5 { x = 1 } else { x = 0 }");
    run_ok("if 1 > 5 { x = 1 } else { x = 0 }");
}

#[test]
fn test_for_in_list() {
    run_ok(
        r#"suma = 0
for n in [1, 2, 3, 4, 5] {
    suma = suma + n
}"#,
    );
}

// ── Manejo de errores ───────────────────────────────────────────────────────

#[test]
fn test_attempt_handle_catches_error() {
    // Un error dentro de attempt debe ser capturado: run_ok, no run_err.
    run_ok(
        r#"attempt {
    x = 1 / 0
} handle err {
    x = -1
}"#,
    );
}

#[test]
fn test_unhandled_error_propagates() {
    // División por cero sin attempt debe devolver Err.
    let msg = run_err("x = 1 / 0");
    assert!(
        !msg.is_empty(),
        "se esperaba un mensaje de error, got vacío"
    );
}
