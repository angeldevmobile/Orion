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

//    Regresiones: falsos positivos de "variable no usada / no definida"
//    (descubiertos construyendo orion-tasks-api). El análisis de uso no visitaba
//    `return`, llamadas a método (CallMethod), el binding de `handle`, y trataba
//    los nombres de builtins en posición de llamada como variables.

#[test]
fn tc_no_false_unused_in_return() {
    // `return x` cuenta como uso de x aunque la fn no declare tipo de retorno.
    assert_no_warning_containing(
        "fn f() {\n    a = 5\n    return a\n}\nshow f()",
        "nunca usada",
    );
}

#[test]
fn tc_no_false_unused_in_method_call() {
    // Una variable usada solo como arg de un método (`xs.push(v)`) cuenta.
    assert_no_warning_containing(
        "fn f() {\n    v = 9\n    xs = [1].push(v)\n    return xs\n}\nshow f()",
        "nunca usada",
    );
}

#[test]
fn tc_no_false_unused_handle_binding() {
    // `handle err { }` sin inspeccionar el error es idiomático → sin warning.
    assert_no_warning_containing(
        "attempt {\n    x = 1\n    show x\n} handle err {\n    show \"falló\"\n}",
        "nunca usada",
    );
}

#[test]
fn tc_no_false_undefined_on_builtin_call() {
    // Builtins en posición de llamada no deben reportarse como "no definida".
    for b in ["has_key(d, \"k\")", "get(d, \"k\")", "first(xs)", "is_empty(xs)"] {
        assert_no_warning_containing(
            &format!("d = {{ \"k\": 1 }}\nxs = [1, 2]\nshow {b}"),
            "no definida",
        );
    }
}

#[test]
fn tc_err_bool_assigned_to_int() {
    assert_has_error("x: int = yes");
}

//    Regresiones: falsos positivos de "usada pero no definida" descubiertos
//    auditando los 91 ejemplos .orx (use, campos de shape, herencia `using`,
//    params de act sin tipo, y bindings de read/ask/await).

#[test]
fn tc_no_false_undefined_on_use_module() {
    // `use "math"` liga `math` como namespace; `math.sqrt(...)` no es indefinido.
    assert_no_warning_containing(
        "use \"math\"\nshow math.sqrt(25)",
        "no definida",
    );
}

#[test]
fn tc_no_false_undefined_on_use_alias_and_selective() {
    assert_no_warning_containing(
        "use \"strings\" as s\nshow s.upper(\"hola\")",
        "no definida",
    );
    assert_no_warning_containing(
        "use \"math\" take [sqrt]\nshow sqrt(9)",
        "no definida",
    );
}

#[test]
fn tc_no_false_undefined_on_shape_fields() {
    // Dentro de un act, los campos del shape se acceden sin `self.`.
    let src = r#"shape Circulo {
    radio: int = 0
    on_create(r) {
        radio = r
    }
    act area() -> int {
        return radio * radio
    }
}
c = Circulo(5)
show c.area()"#;
    assert_no_warning_containing(src, "no definida");
    assert_ok(src);
}

#[test]
fn tc_no_false_undefined_on_shape_untyped_params() {
    // Params de on_create/act sin anotación no deben marcarse indefinidos.
    let src = r#"shape Cuenta {
    balance: int = 0
    on_create(o, initial) {
        balance = initial
    }
    act buscar(objeto) {
        return objeto
    }
}
c = Cuenta("ana", 100)
show c.buscar("x")"#;
    assert_no_warning_containing(src, "no definida");
}

#[test]
fn tc_no_false_undefined_on_inherited_fields() {
    // Campos heredados vía `using` son visibles en el shape hijo.
    let src = r#"shape Animal {
    nombre: string = ""
}
shape Perro {
    using Animal
    act ladra() -> string {
        return nombre + " ladra"
    }
}"#;
    assert_no_warning_containing(src, "no definida");
}

//    Inferencia de retorno para funciones SIN anotación (propaga entre fns)

#[test]
fn tc_infers_untyped_fn_return_type() {
    // `nombre()` no declara retorno pero claramente devuelve string → asignarlo
    // a un int debe ser error (la inferencia cruza la función).
    assert_has_error("fn nombre() { return \"orion\" }\nx: int = nombre()");
    // caso correcto: el mismo retorno asignado a string → sin error.
    assert_ok("fn nombre() { return \"orion\" }\nx: string = nombre()");
}

#[test]
fn tc_infers_untyped_fn_return_through_chain() {
    // Cadena A→B: A llama a B (ambas sin anotar). El punto fijo propaga el tipo.
    assert_has_error(
        "fn base() { return 3.14 }\nfn wrap() { return base() }\nx: int = wrap()",
    );
}

#[test]
fn tc_conservative_no_infer_on_param_return() {
    // Retorno que depende de un parámetro sin tipo → NO se infiere (queda any),
    // así no se inventa un tipo que dispare falsos errores. La recursión típica
    // (fib) entra aquí y debe quedar sin error.
    assert_ok(
        "fn fib(n) {\n    if n < 2 { return n }\n    return fib(n - 1) + fib(n - 2)\n}\nx: int = fib(10)",
    );
}

#[test]
fn tc_conservative_no_infer_on_mixed_returns() {
    // Returns de tipos distintos → no se infiere (queda any) → sin falso error.
    assert_ok(
        "fn f(c) {\n    if c { return 1 }\n    return \"dos\"\n}\ny = f(yes)",
    );
}

#[test]
fn tc_no_false_undefined_on_read_binding() {
    // `read ruta -> contenido` liga `contenido`.
    assert_no_warning_containing(
        "fn leer(ruta) {\n    read ruta -> contenido\n    return contenido\n}",
        "no definida",
    );
}
