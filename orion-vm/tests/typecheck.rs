//! Tests del type checker (Sprint 2).
//!
//! El type checker alimenta los diagnósticos `--check --types` de la extensión
//! VS Code. Si genera falsos positivos, la extensión "miente"; si no detecta
//! errores reales, no sirve. Estos tests fijan ambos lados:
//!   - código bien tipado / sin anotaciones → CERO errores
//!   - desajustes de tipo reales → AL MENOS un error
//!
//! Solo se cuentan los issues con kind == "error" (los "warning" como variable
//! sin usar son ruido para estos casos).

use orion_vm::{lexer, parser, typechecker};

fn errors(src: &str) -> Vec<String> {
    let tokens = lexer::lex(src).expect("lex");
    let stmts = parser::parse(tokens).expect("parse");
    typechecker::type_check(&stmts)
        .into_iter()
        .filter(|i| i.kind == "error")
        .map(|i| i.message)
        .collect()
}

fn assert_ok(src: &str) {
    let errs = errors(src);
    assert!(
        errs.is_empty(),
        "esperaba CERO errores de tipo, se obtuvieron {:?}\n--- src ---\n{src}",
        errs
    );
}

fn assert_has_error(src: &str) {
    let errs = errors(src);
    assert!(
        !errs.is_empty(),
        "esperaba al menos un error de tipo, no se detectó ninguno\n--- src ---\n{src}"
    );
}

/// Mensajes de warning (no error) emitidos por el type checker.
fn warnings(src: &str) -> Vec<String> {
    let tokens = lexer::lex(src).expect("lex");
    let stmts = parser::parse(tokens).expect("parse");
    typechecker::type_check(&stmts)
        .into_iter()
        .filter(|i| i.kind == "warning")
        .map(|i| i.message)
        .collect()
}

/// Falla si algún warning contiene `needle`.
fn assert_no_warning_containing(src: &str, needle: &str) {
    let warns = warnings(src);
    let hits: Vec<_> = warns.iter().filter(|w| w.contains(needle)).collect();
    assert!(
        hits.is_empty(),
        "no se esperaba ningún warning con '{needle}', se obtuvieron {hits:?}\n--- src ---\n{src}"
    );
}

//    Deben PASAR (sin errores)

#[test]
fn tc_ok_typed_assignment() {
    assert_ok("x: int = 5");
    assert_ok(r#"s: string = "hola""#);
    assert_ok("b: bool = yes");
}

#[test]
fn tc_ok_untyped_code() {
    // El código sin anotaciones (la mayoría de Orion) no debe producir errores.
    assert_ok("x = 5\ny = x + 10\nz = y * 2\nshow z");
}

#[test]
fn tc_ok_function_well_typed() {
    assert_ok(
        r#"fn doble(n: int) -> int {
    return n * 2
}
r = doble(21)
show r"#,
    );
}

#[test]
fn tc_ok_call_with_correct_arg_type() {
    assert_ok(
        r#"fn saluda(nombre: string) -> string {
    return "hola " + nombre
}
show saluda("orion")"#,
    );
}

// Regresión: parámetros de función SIN type hint usados en el cuerpo no deben
// reportarse como "usada pero no definida en este scope" (el tipado es opcional
// en Orion). Antes el type checker solo registraba en scope los parámetros con
// anotación, marcando a los demás como indefinidos. Ver demo/demo_calc.orx.
#[test]
fn tc_ok_untyped_params_not_flagged_as_undefined() {
    let src = r#"fn aplicar(a, b, op) {
    if op == "+" { return a + b }
    if op == "-" { return a - b }
    if b == 0 { return 0 }
    return b
}
show aplicar(2, 3, "+")"#;
    assert_no_warning_containing(src, "usada pero no definida");
    // y desde luego no debe ser un error duro
    assert_ok(src);
}

// Regresión inversa: un parámetro no leído NO debe disparar el warning de
// "asignada pero nunca usada" (los parámetros no son variables locales muertas).
#[test]
fn tc_ok_unused_param_not_flagged_as_unused() {
    let src = r#"fn no_usa(x, y) {
    return 0
}
show no_usa(1, 2)"#;
    assert_no_warning_containing(src, "nunca usada");
}

//    Deben FALLAR (con error)

#[test]
fn tc_err_assignment_type_mismatch() {
    // x declarado int pero se le asigna un string.
    assert_has_error(r#"x: int = "hola""#);
}

#[test]
fn tc_err_return_type_mismatch() {
    // función declara -> int pero retorna string.
    assert_has_error(
        r#"fn f() -> int {
    return "no soy int"
}"#,
    );
}

#[test]
fn tc_err_call_argument_type_mismatch() {
    // saluda espera string, se le pasa int.
    assert_has_error(
        r#"fn saluda(nombre: string) -> string {
    return "hola " + nombre
}
show saluda(42)"#,
    );
}

#[test]
fn tc_err_bool_assigned_to_int() {
    assert_has_error("x: int = yes");
}
