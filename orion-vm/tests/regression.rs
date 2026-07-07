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

// ── Interpolación de strings ────────────────────────────────────────────────

#[test]
fn test_string_interpolation_always_string() {
    // Una interpolación sola "${n}" debe dar un STRING (no el valor crudo Int),
    // para que al pasarla a módulos (gui.heading, etc.) no salga "Int(5)".
    run_ok(
        r#"n = 5
if type("${n}") != "string" { error "interpolacion sola debe ser string" }
if "${n}" != "5" { error "interpolacion sola incorrecta" }
if "${n} items" != "5 items" { error "interpolacion con texto incorrecta" }"#,
    );
}

#[test]
fn test_string_interpolation_adjacent() {
    // Interpolaciones adyacentes deben concatenar como strings, no sumar.
    run_ok(
        r#"a = 1
b = 2
if "${a}${b}" != "12" { error "adyacentes deben concatenar (no sumar)" }"#,
    );
}

// ── Acceso/escritura de atributos en dicts y slicing ────────────────────────

#[test]
fn test_dict_attr_write_and_read() {
    // v.campo = x (escritura) y v.campo (lectura) en dicts.
    run_ok(
        r#"v = {"venta": 100}
v.estado = "Cumple"
v.cumplimiento = 95
if v.estado != "Cumple" { error "escritura de atributo en dict falló" }
if v.cumplimiento != 95 { error "segunda escritura falló" }
if v.venta != 100 { error "el campo original se perdió" }"#,
    );
}

#[test]
fn test_dict_attr_write_in_loop() {
    // Patrón de enriquecimiento: mutar dicts dentro de un for y acumular.
    run_ok(
        r#"filas = [{"n": 1}, {"n": 2}, {"n": 3}]
salida = []
for f in filas {
    f.doble = f.n * 2
    salida = salida.push(f)
}
if salida[1].doble != 4 { error "enriquecimiento en loop falló" }"#,
    );
}

#[test]
fn test_slicing_list_and_string() {
    run_ok(
        r#"nums = [10, 20, 30, 40, 50]
if len(nums[0:3]) != 3 { error "slice [0:3] falló" }
if nums[2:][0] != 30 { error "slice [2:] falló" }
if len(nums[:2]) != 2 { error "slice [:2] falló" }
if nums[-2:][0] != 40 { error "slice negativo falló" }
if "orion"[0:3] != "ori" { error "slice de string falló" }"#,
    );
}

// ── super: llamada al método del shape padre ────────────────────────────────

#[test]
fn test_super_calls_parent_method() {
    // Hija override saludar() y llama super.saludar() del padre Base.
    run_ok(
        r#"shape Base {
    act saludar() {
        return "hola desde base"
    }
}
shape Hija using [Base] {
    act saludar() {
        return "hija + " + super.saludar()
    }
}
h = Hija()
r = h.saludar()
if r != "hija + hola desde base" { error "super falló: " + r }"#,
    );
}

#[test]
fn test_super_with_args_and_shared_state() {
    // super.metodo(args) opera sobre la misma instancia (estado compartido).
    run_ok(
        r#"shape Contador {
    valor: 0
    act incrementar(n) {
        valor = valor + n
        return valor
    }
}
shape ContadorDoble using [Contador] {
    act incrementar(n) {
        a = super.incrementar(n)
        b = super.incrementar(n)
        return a + b
    }
}
c = ContadorDoble()
r = c.incrementar(5)
if r != 15 { error "super con estado falló: " + str(r) }"#,
    );
}

// ── on_error: hook de error a nivel de shape ────────────────────────────────

#[test]
fn test_on_error_catches_act_error() {
    // Un error dentro de un act invoca on_error en vez de propagar.
    run_ok(
        r#"shape Cuenta {
    saldo: 100
    on_error(e) {
        recuperado = yes
    }
    act retirar(monto) {
        if monto > saldo { error "fondos insuficientes" }
        saldo = saldo - monto
    }
}
c = Cuenta()
c.retirar(500)"#,
    );
}

#[test]
fn test_on_error_return_value_used() {
    // El valor que retorna on_error se vuelve el resultado de la llamada fallida.
    run_ok(
        r#"shape Calc {
    on_error(e) {
        return -1
    }
    act dividir(a, b) {
        if b == 0 { error "div cero" }
        return a / b
    }
}
c = Calc()
r = c.dividir(10, 0)
if r != -1 { error "on_error debió devolver -1" }"#,
    );
}

#[test]
fn test_on_error_not_triggered_on_success() {
    // Si el act no falla, on_error no se ejecuta y el resultado es normal.
    run_ok(
        r#"shape Calc {
    on_error(e) {
        return -999
    }
    act suma(a, b) {
        return a + b
    }
}
c = Calc()
if c.suma(2, 3) != 5 { error "on_error no debió dispararse" }"#,
    );
}

#[test]
fn test_on_error_inner_attempt_takes_priority() {
    // Un attempt/handle DENTRO del act captura el error antes que on_error.
    run_ok(
        r#"shape Svc {
    marca: "ninguna"
    on_error(e) {
        marca = "on_error"
    }
    act run() {
        attempt {
            error "boom"
        } handle err {
            marca = "interno"
        }
        return marca
    }
}
s = Svc()
if s.run() != "interno" { error "el attempt interno debió ganar" }"#,
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

// ── Listas por referencia (mutación real + aliasing) ────────────────────────

#[test]
fn test_list_push_mutates_variable() {
    // xs.push(x) muta la variable in-place (semántica por referencia).
    run_ok(
        r#"xs = [1, 2]
xs.push(3)
if len(xs) != 3 { error "push no mutó xs" }
if xs[2] != 3 { error "push no agregó el valor correcto" }"#,
    );
}

#[test]
fn test_list_aliasing() {
    // ys = xs comparte el backing: mutar xs se ve en ys.
    run_ok(
        r#"xs = [1, 2]
ys = xs
xs.push(3)
if len(ys) != 3 { error "el alias no vio la mutación" }"#,
    );
}

#[test]
fn test_list_mutation_through_function() {
    // Pasar una lista a una función y mutarla afecta al llamador.
    run_ok(
        r#"fn agregar(lista, v) {
    lista.push(v)
}
zs = [10]
agregar(zs, 20)
if len(zs) != 2 { error "la mutación dentro de la función no persistió" }
if zs[1] != 20 { error "valor incorrecto tras mutación en función" }"#,
    );
}

#[test]
fn test_list_concat_does_not_alias() {
    // p + q produce una lista NUEVA e independiente; mutar p no toca el resultado.
    run_ok(
        r#"p = [1, 2]
q = p + [3]
p.push(99)
if len(q) != 3 { error "concat aliasó incorrectamente" }
if len(p) != 3 { error "push tras concat no mutó p" }"#,
    );
}

#[test]
fn test_list_equality_is_structural() {
    // == sigue comparando contenido, no identidad de referencia.
    run_ok(
        r#"a = [1, 2, 3]
b = [1, 2, 3]
if not (a == b) { error "igualdad estructural rota" }"#,
    );
}

#[test]
fn test_list_set_index_mutates() {
    // m[i] = v muta el backing compartido in-place.
    run_ok(
        r#"m = [0, 0, 0]
n = m
m[1] = 7
if n[1] != 7 { error "set-index no se reflejó en el alias" }"#,
    );
}

// ── Ecosistema de paquetes: fixes a nivel de lenguaje ─────────────────────────

#[test]
fn test_split_empty_yields_chars() {
    // split("") debe partir en caracteres, sin strings vacíos en los bordes.
    run_ok(
        r#"p = "abc".split("")
if len(p) != 3 { error "split vacío: longitud incorrecta" }
if p[0] != "a" { error "split vacío: primer carácter incorrecto" }
if p[2] != "c" { error "split vacío: último carácter incorrecto" }"#,
    );
}

#[test]
fn test_unicode_and_hex_escapes() {
    // \uXXXX y \xHH deben decodificarse a su carácter.
    run_ok(
        r#"if "\u0041" != "A" { error "escape \u roto" }
if "\x41" != "A" { error "escape \x roto" }"#,
    );
}

#[test]
fn test_dict_function_field_shadows_native_method() {
    // Una función almacenada en un dict (p.ej. namespace de módulo) tiene
    // prioridad sobre el método nativo de dict del mismo nombre.
    run_ok(
        r#"d = {"contains": fn(x) { return x * 2 }}
if d.contains(21) != 42 { error "función de dict eclipsada por método nativo" }"#,
    );
}

#[test]
fn test_type_name_as_member_after_dot() {
    // Nombres de tipo (int, list, dict, ...) son válidos como nombre de miembro
    // tras un punto, y un namespace puede llamarse `list`/`dict`.
    run_ok(
        r#"obj = {"int": fn() { return 7 }, "list": fn() { return 9 }}
if obj.int() != 7 { error "int como miembro falló" }
if obj.list() != 9 { error "list como miembro falló" }"#,
    );
}
