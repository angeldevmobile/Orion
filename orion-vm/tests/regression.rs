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

// ── Valores por defecto en parámetros ───────────────────────────────────────

#[test]
fn test_default_params_ok() {
    run_ok("fn f(a, b = 5) { return a + b }\nx = f(1)\ny = f(1, 2)");
}

#[test]
fn test_default_falta_arg_obligatorio() {
    // 'a' es obligatorio; llamar f() debe fallar en runtime con mensaje claro.
    let err = run_err("fn f(a, b = 5) { return a + b }\nshow f()");
    assert!(err.contains("espera") && err.contains("recibió"),
        "mensaje inesperado: {}", err);
}

#[test]
fn test_default_demasiados_args() {
    let err = run_err("fn f(a, b = 5) { return a + b }\nshow f(1, 2, 3)");
    assert!(err.contains("espera 2"), "mensaje inesperado: {}", err);
}

#[test]
fn test_default_orden_invalido_es_error_de_compilacion() {
    // Un parámetro obligatorio después de uno con default: error en codegen.
    let tokens = lexer::lex("fn mal(a = 1, b) { return a }").unwrap();
    let stmts = parser::parse(tokens).unwrap();
    let err = codegen::compile(stmts).expect_err("se esperaba error de compilación");
    assert!(err.message.contains("no puede ir después"), "mensaje: {}", err.message);
}

// ── Argumentos con nombre (named args) ──────────────────────────────────────

#[test]
fn test_named_args_ok() {
    run_ok("fn f(a, b = 2, c = 3) { return a + b + c }\nx = f(1, c = 9)\ny = f(c = 1, a = 2)");
}

fn compile_err(src: &str) -> String {
    let tokens = lexer::lex(src).unwrap();
    let stmts = parser::parse(tokens).unwrap();
    codegen::compile(stmts).expect_err("se esperaba error de compilación").message
}

#[test]
fn test_named_arg_param_inexistente() {
    let e = compile_err("fn f(a, b = 2) { return a }\nshow f(a = 1, zzz = 9)");
    assert!(e.contains("no tiene un parámetro"), "mensaje: {}", e);
}

#[test]
fn test_named_arg_duplicado() {
    let e = compile_err("fn f(a, b = 2) { return a }\nshow f(1, a = 9)");
    assert!(e.contains("dado dos veces"), "mensaje: {}", e);
}

#[test]
fn test_named_arg_en_funcion_desconocida_es_error() {
    let e = compile_err("show desconocida(x = 1)");
    assert!(e.contains("no soportado"), "mensaje: {}", e);
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

// ── GC: estructuras cíclicas y profundas no deben tumbar la VM ──────────────

#[test]
fn test_gc_self_referential_list_survives_collection() {
    // push(a, a): la lista se contiene a sí misma (legal con listas por
    // referencia). El bucle asigna >512 instancias para forzar al menos una
    // colección con la lista cíclica en los roots — antes el mark recursivo
    // sin set de visitados para listas no terminaba nunca (stack overflow).
    run_ok(
        r#"shape P { x: 0 }
a = [1]
push(a, a)
i = 0
while i < 600 {
    p = P()
    i = i + 1
}"#,
    );
}

#[test]
fn test_gc_deep_instance_chain_survives() {
    // Lista enlazada de 5000 instancias: fuerza varias colecciones con una
    // cadena cada vez más profunda en los roots, y al final el drop de toda
    // la cadena. (La profundidad extrema de 200k se cubre en los unit tests
    // de gc.rs; aquí validamos el pipeline completo sin hacer lento el suite.)
    run_ok(
        r#"shape Node { next: 0 }
head = Node()
i = 0
while i < 5000 {
    n = Node()
    n.next = head
    head = n
    i = i + 1
}"#,
    );
}

#[test]
fn test_deep_nested_lists_survive_drop() {
    // Torre de listas anidadas construida en Orion: al terminar el programa
    // se suelta entera de golpe. Con el drop recursivo esto desbordaba el
    // stack nativo; el Drop iterativo de ListData lo desmonta en un bucle.
    run_ok(
        r#"a = [0]
i = 0
while i < 50000 {
    a = [a]
    i = i + 1
}"#,
    );
}

#[test]
fn test_gc_safepoint_no_corrompe_instancias_nuevas() {
    // Regresión del bug del safepoint: gc_collect() corría DENTRO de
    // instantiate_shape, cuando la instancia recién creada y los args aún
    // viven en locals de Rust (fuera de los roots) → el sweep les vaciaba
    // los fields y cada instancia nº 512 nacía sin campos. Cruzamos varios
    // umbrales (512) verificando el campo en cada una.
    run_ok(
        r#"shape P { v: 7 }
i = 0
while i < 1200 {
    p = P()
    if p.v != 7 { error "instancia corrupta en la iteracion ${i}" }
    i = i + 1
}"#,
    );
}

#[test]
fn test_gc_args_sobreviven_instanciacion_anidada() {
    // Variante: la instancia interior Q(9) viaja como arg de P(...) justo
    // cuando el umbral dispara la colección; antes el sweep podía vaciarla.
    run_ok(
        r#"shape Q { w: 0 on_create(x) { w = x } }
shape P { q: 0 on_create(a) { q = a } }
i = 0
while i < 700 {
    p = P(Q(9))
    if p.q.w != 9 { error "arg corrupto en la iteracion ${i}" }
    i = i + 1
}"#,
    );
}

// ── `with` — recursos de módulo con ámbito (liberación garantizada) ──────────

#[test]
fn test_with_libera_al_salir() {
    // El frame creado por with debe liberarse al cerrar el bloque:
    // frames() vuelve a su valor previo. El resultado calculado dentro
    // sobrevive (las vars son de función, no del bloque).
    run_ok(
        r#"use "frame" as frame
antes = frame.frames()
with f = frame.from_list([{ "x": 10 }, { "x": 20 }]) {
    k = frame.count(f)
}
if k != 2 { error "count dentro de with esperaba 2" }
if frame.frames() != antes { error "with no liberó el frame" }"#,
    );
}

#[test]
fn test_with_libera_con_error_y_relanza() {
    // Si el cuerpo lanza, el recurso se libera IGUAL y el error se re-lanza
    // (capturable por un attempt exterior).
    run_ok(
        r#"use "frame" as frame
antes = frame.frames()
capturado = ""
attempt {
    with f = frame.from_list([{ "x": 1 }]) {
        error "boom"
    }
} handle e {
    capturado = e
}
if capturado != "boom" { error "el error del cuerpo no se propagó: ${capturado}" }
if frame.frames() != antes { error "with no liberó el frame en el camino de error" }"#,
    );
}

#[test]
fn test_with_anidado() {
    run_ok(
        r#"use "frame" as frame
antes = frame.frames()
with a = frame.from_list([{ "x": 1 }]) {
    with b = frame.from_list([{ "y": 2 }, { "y": 3 }]) {
        total = frame.count(a) + frame.count(b)
    }
}
if total != 3 { error "anidado esperaba 3, dio ${total}" }
if frame.frames() != antes { error "with anidado dejó frames vivos" }"#,
    );
}

#[test]
fn test_with_handle_int_quantum() {
    // Los handles de quantum.circuit son Int, no string: with no depende
    // del tipo del handle porque conoce el módulo estáticamente.
    run_ok(
        r#"use "quantum" as quantum
with q = quantum.circuit(2) {
    quantum.h(q, 0)
    quantum.cnot(q, 0, 1)
    p = quantum.probs(q)
}
if len(p) != 4 { error "probs de 2 qubits esperaba 4 amplitudes" }"#,
    );
}

#[test]
fn test_with_loop_interno_puede_usar_break() {
    // break dentro de un loop DEL CUERPO es legal (no sale del with).
    run_ok(
        r#"use "frame" as frame
antes = frame.frames()
with f = frame.from_list([{ "x": 1 }, { "x": 2 }]) {
    i = 0
    while i < 100 {
        if i == 3 { break }
        i = i + 1
    }
}
if i != 3 { error "el break interno no cortó en 3" }
if frame.frames() != antes { error "with con loop interno no liberó" }"#,
    );
}

#[test]
fn test_with_rechaza_return_en_el_cuerpo() {
    // return se saltaría el free → error de parseo con mensaje claro.
    let tokens = orion_vm::lexer::lex(
        r#"use "frame" as frame
fn f() {
    with h = frame.from_list([{ "x": 1 }]) {
        return 1
    }
}"#,
    ).unwrap();
    let err = orion_vm::parser::parse(tokens).expect_err("return dentro de with debe rechazarse");
    assert!(err.message.contains("liberación"), "mensaje: {}", err.message);
}

#[test]
fn test_with_rechaza_break_fuera_de_loop_interno() {
    let tokens = orion_vm::lexer::lex(
        r#"use "frame" as frame
while yes {
    with h = frame.from_list([{ "x": 1 }]) {
        break
    }
}"#,
    ).unwrap();
    let err = orion_vm::parser::parse(tokens).expect_err("break que escapa del with debe rechazarse");
    assert!(err.message.contains("liberar"), "mensaje: {}", err.message);
}

#[test]
fn test_with_rechaza_init_que_no_es_modulo() {
    let tokens = orion_vm::lexer::lex("with h = 42 { show h }").unwrap();
    let err = orion_vm::parser::parse(tokens).expect_err("init sin módulo debe rechazarse");
    assert!(err.message.contains("recurso de módulo"), "mensaje: {}", err.message);
}

// ── Handlers huérfanos: return dentro de attempt (fix de la VM) ──────────────

#[test]
fn test_return_dentro_de_attempt_no_deja_handler_huerfano() {
    // f() retorna DESDE DENTRO de un attempt → su handler debe morir con el
    // frame. Antes quedaba huérfano y un error posterior sin attempt saltaba
    // a una dirección de OTRA función (el programa "terminaba" en silencio
    // en vez de reportar el error).
    let err = run_err(
        r#"fn f() {
    attempt {
        return 7
    } handle e {
        show e
    }
    return 0
}
a = f()
if a != 7 { error "f debía retornar 7" }
error "bang""#,
    );
    assert!(err.contains("bang"),
        "el error posterior debía propagarse limpio, dio: {}", err);
}

#[test]
fn test_return_dentro_de_attempt_con_handler_exterior_valido() {
    // El attempt del CALLER sí debe seguir funcionando tras el return interno.
    run_ok(
        r#"fn f() {
    attempt {
        return 7
    } handle e {
        show e
    }
    return 0
}
r = ""
attempt {
    a = f()
    error "bang"
} handle e {
    r = e
}
if r != "bang" { error "el handler exterior no atrapó: ${r}" }"#,
    );
}
