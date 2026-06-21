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

// ── OOP: shapes, acts, instancias (Sprint 1 — P0) ───────────────────────────
// Patrón auto-verificante: si la semántica es correcta, `run_ok` pasa; si está
// rota (valor inesperado), el programa lanza `error` y `run_ok` hace panic.

#[test]
fn test_oop_shape_construct_and_method() {
    // on_create asigna campos; act los lee. p.sum() debe dar 7.
    run_ok(
        r#"shape Point {
    x: 0
    y: 0
    on_create(a, b) {
        x = a
        y = b
    }
    act sum() {
        return x + y
    }
}
p = Point(3, 4)
if p.sum() != 7 { error "OOP: p.sum() != 7" }
if p.x != 3 { error "OOP: acceso a campo p.x falló" }"#,
    );
}

#[test]
fn test_oop_instances_are_independent() {
    // Dos instancias del mismo shape no comparten estado.
    run_ok(
        r#"shape Counter {
    count: 0
    act increment() {
        count = count + 1
    }
    act value() {
        return count
    }
}
c1 = Counter()
c2 = Counter()
c1.increment()
c1.increment()
c1.increment()
c2.increment()
if c1.value() != 3 { error "OOP: c1 esperaba 3" }
if c2.value() != 1 { error "OOP: c2 esperaba 1 (estado compartido!)" }"#,
    );
}

#[test]
fn test_oop_is_operator() {
    // `is` debe distinguir el tipo correcto del incorrecto.
    run_ok(
        r#"shape Dog { name: "" }
shape Cat { name: "" }
d = Dog()
if d is Cat { error "is: dio true para tipo incorrecto" }
ok = no
if d is Dog { ok = yes }
if ok != yes { error "is: dio false para el tipo correcto" }"#,
    );
}

// ── Closures: captura y mutación de variables del frame externo (Sprint 1 — P0)

#[test]
fn test_closure_captures_and_persists_state() {
    // La fn interna captura `inicio` y debe persistir su estado entre llamadas.
    run_ok(
        r#"fn hacer_contador(inicio) {
    fn siguiente() {
        inicio = inicio + 1
        return inicio
    }
    return siguiente
}
c = hacer_contador(10)
a = c()
b = c()
if a != 11 { error "closure: primera llamada esperaba 11" }
if b != 12 { error "closure: el estado no persistió, esperaba 12" }"#,
    );
}

#[test]
fn test_closures_are_independent() {
    // Dos closures del mismo factory tienen entornos capturados separados.
    run_ok(
        r#"fn hacer_contador(inicio) {
    fn siguiente() {
        inicio = inicio + 1
        return inicio
    }
    return siguiente
}
c1 = hacer_contador(0)
c2 = hacer_contador(100)
r1 = c1()
r2 = c2()
if r1 != 1 { error "closure indep: c1 esperaba 1" }
if r2 != 101 { error "closure indep: c2 esperaba 101 (entornos mezclados!)" }"#,
    );
}
