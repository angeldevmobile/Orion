//! Pruebas end-to-end del modelo de concurrencia de Orion.
//!
//! Cubren las cuatro mejoras del modelo async (2026-07-23):
//!   1. `await` con parking real (Condvar) — sin busy-wait.
//!   2. `spawn` sobre un pool de hilos cacheado — sin un hilo de SO por tarea.
//!   3. Cancelación cooperativa (`t.cancelar()` / `t.esperar()`).
//!   4. Canales (`chan`): productor/consumidor, backpressure, cierre y `select`.
//!
//! Cada programa se auto-verifica con la sentencia `error`: si un invariante no
//! se cumple, el runtime devuelve Err y `run_ok` hace fallar la prueba.

use orion_vm::{codegen, lexer, parser, vm};

fn run_ok(src: &str) {
    let tokens = lexer::lex(src)
        .unwrap_or_else(|e| panic!("lex error: {} | src:\n{}", e.message, src));
    let stmts = parser::parse(tokens)
        .unwrap_or_else(|e| panic!("parse error: {} | src:\n{}", e.message, src));
    let bc = codegen::compile(stmts)
        .unwrap_or_else(|e| panic!("codegen error: {} | src:\n{}", e.message, src));
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    machine
        .run()
        .unwrap_or_else(|e| panic!("runtime error: {} | src:\n{}", e, src));
}

fn run_err(src: &str) -> String {
    let tokens = lexer::lex(src).unwrap();
    let stmts = parser::parse(tokens).unwrap();
    let bc = codegen::compile(stmts).unwrap();
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    machine.run().expect_err("se esperaba error en runtime")
}

// ── 1 + 2. spawn/await con parking + pool de hilos ──────────────────────────

#[test]
fn spawn_await_valor_basico() {
    run_ok(r#"
        async fn cuadrado(n) { return n * n }
        t = cuadrado(9)
        r = await t
        if r != 81 { error "await devolvió un valor incorrecto" }
    "#);
}

#[test]
fn rafaga_de_tareas_reusa_el_pool() {
    // 200 tareas en ráfaga: antes = 200 hilos de SO; ahora = pool reutilizado.
    // Verificamos también que await de todas devuelve resultados correctos.
    run_ok(r#"
        async fn inc(n) { return n + 1 }
        tareas = []
        i = 0
        while i < 200 {
            tareas.push(inc(i))
            i = i + 1
        }
        suma = 0
        for t in tareas {
            x = await t
            suma = suma + x
        }
        -- sum(1..200) = 20100
        if suma != 20100 { error "la ráfaga de tareas dio un total incorrecto" }
    "#);
}

#[test]
fn await_inline_como_expresion() {
    // Regresión: `await f(x)` en posición de expresión debe capturar la llamada
    // completa (antes parseaba `await f` y aplicaba `(x)` sobre el resultado →
    // "Función '__call__' no definida"). Verifica también la precedencia:
    // `await f(x) + n` = (await f(x)) + n.
    run_ok(r#"
        async fn calcular(n) { return n * n + 42 }
        val = await calcular(5)
        if val != 67 { error "await inline dio un valor incorrecto" }
        suma = await calcular(3) + 100
        if suma != 151 { error "await inline con precedencia incorrecta" }
    "#);
}

#[test]
fn spawn_anidado_no_deadlock() {
    // Una tarea que a su vez lanza y espera otra: el pool debe crear un worker
    // bajo demanda para no bloquearse.
    run_ok(r#"
        async fn hoja(n) { return n * 2 }
        async fn rama(n) {
            t = hoja(n)
            r = await t
            return r + 1
        }
        t = rama(10)
        r = await t
        if r != 21 { error "spawn anidado dio un valor incorrecto" }
    "#);
}

// ── 3. Cancelación cooperativa ──────────────────────────────────────────────

#[test]
fn cancelar_aborta_la_tarea() {
    // Cancelamos una tarea con un bucle largo; esperar() debe propagar el error
    // de cancelación en vez de devolver un resultado.
    let err = run_err(r#"
        async fn eterna() {
            k = 0
            while k < 1000000000 { k = k + 1 }
            return k
        }
        t = eterna()
        t.cancelar()
        r = t.esperar()
        show r
    "#);
    assert!(err.contains("cancel"), "mensaje inesperado: {}", err);
}

// ── 4. Canales ──────────────────────────────────────────────────────────────

#[test]
fn canal_productor_consumidor() {
    run_ok(r#"
        use "chan" as chan
        c = chan.crear()
        async fn productor(canal) {
            j = 0
            while j < 5 {
                chan.enviar(canal, j)
                j = j + 1
            }
            chan.cerrar(canal)
            return 0
        }
        spawn productor(c)
        total = 0
        v = chan.recibir(c)
        while v != null {
            total = total + v
            v = chan.recibir(c)
        }
        if total != 10 { error "el canal entregó un total incorrecto" }
    "#);
}

#[test]
fn canal_con_capacidad_y_backpressure() {
    // Canal acotado (cap=1): el productor se bloquea hasta que el consumidor
    // saca. Al final deben haberse transferido los 4 valores.
    run_ok(r#"
        use "chan" as chan
        c = chan.crear(1)
        async fn prod(canal) {
            j = 1
            while j <= 4 {
                chan.enviar(canal, j)
                j = j + 1
            }
            chan.cerrar(canal)
            return 0
        }
        spawn prod(c)
        total = 0
        v = chan.recibir(c)
        while v != null {
            total = total + v
            v = chan.recibir(c)
        }
        if total != 10 { error "backpressure perdió o duplicó valores" }
    "#);
}

#[test]
fn canal_recibir_tras_cerrar_da_null() {
    run_ok(r#"
        use "chan" as chan
        c = chan.crear()
        chan.enviar(c, 42)
        chan.cerrar(c)
        a = chan.recibir(c)
        b = chan.recibir(c)
        if a != 42 { error "recibir tras cerrar no drenó el valor pendiente" }
        if b != null { error "recibir en canal cerrado y vacío debe dar null" }
    "#);
}

#[test]
fn canal_try_recibir_no_bloquea() {
    run_ok(r#"
        use "chan" as chan
        c = chan.crear()
        vacio = chan.try_recibir(c)
        if vacio != null { error "try_recibir en canal vacío debe dar null" }
        chan.enviar(c, 7)
        x = chan.try_recibir(c)
        if x != 7 { error "try_recibir no devolvió el valor encolado" }
    "#);
}

#[test]
fn select_elige_canal_listo() {
    run_ok(r#"
        use "chan" as chan
        a = chan.crear()
        b = chan.crear()
        chan.enviar(b, 77)
        res = chan.select([a, b])
        if res["valor"] != 77 { error "select no devolvió el valor listo" }
        if res["canal"] != b { error "select no identificó el canal correcto" }
    "#);
}

#[test]
fn select_espera_a_una_tarea() {
    // select debe bloquear (parking) hasta que una tarea emita en algún canal.
    run_ok(r#"
        use "chan" as chan
        done = chan.crear()
        async fn trabajador(canal) {
            chan.enviar(canal, 99)
            return 0
        }
        spawn trabajador(done)
        res = chan.select([done])
        if res["valor"] != 99 { error "select no recibió el valor de la tarea" }
    "#);
}
