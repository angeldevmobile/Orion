//! Regresiones del lenguaje encontradas construyendo la demo `comercio`:
//!
//!   1. `and` / `or` no cortocircuitaban: `has_key(d, "k") and d["k"] > 0`
//!      evaluaba la derecha aunque la izquierda fuese falsa, y reventaba justo
//!      en el caso que la izquierda existía para evitar.
//!   2. Un `return` dentro de `attempt` dejaba su manejador vivo, y un error
//!      posterior FUERA del `attempt` saltaba a ese `handle`.
//!   3. Un módulo importado solo por otro módulo no llegaba a la VM principal:
//!      "Function 'b__x' not found".
//!
//! Cada programa se autoverifica con `error`: si algo no cuadra, la VM
//! devuelve Err y el test falla con el mensaje.

use orion_vm::{codegen, jit, lexer, parser, vm};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn compilar(src: &str) -> orion_vm::bytecode::OrionBytecode {
    let tokens = lexer::lex(src)
        .unwrap_or_else(|e| panic!("lex error: {} | src:\n{}", e.message, src));
    let stmts = parser::parse(tokens)
        .unwrap_or_else(|e| panic!("parse error: {} | src:\n{}", e.message, src));
    codegen::compile(stmts)
        .unwrap_or_else(|e| panic!("codegen error: {} | src:\n{}", e.message, src))
}

fn run_ok(src: &str) {
    let bc = compilar(src);
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    machine.run().unwrap_or_else(|e| panic!("runtime error: {} | src:\n{}", e, src));
}

fn run_err(src: &str) -> String {
    let bc = compilar(src);
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    machine.run().expect_err("se esperaba un error en tiempo de ejecución")
}

// ── 1. Cortocircuito ─────────────────────────────────────────────────────────

// Si la derecha llegara a evaluarse, la división por cero pararía el
// programa: que termine bien ES la prueba de que no se evaluó.

#[test]
fn and_no_evalua_la_derecha_si_la_izquierda_es_falsa() {
    run_ok(r##"
        x = no and (1 / 0 == 1)
        if x { error("no and ... tenía que ser no") }
    "##);
}

#[test]
fn or_no_evalua_la_derecha_si_la_izquierda_es_cierta() {
    run_ok(r##"
        x = yes or (1 / 0 == 1)
        if not x { error("yes or ... tenía que ser yes") }
    "##);
}

#[test]
fn and_or_si_evaluan_la_derecha_cuando_hace_falta() {
    let err = run_err("x = yes and (1 / 0 == 1)");
    assert!(err.to_lowercase().contains("cero") || err.to_lowercase().contains("zero"), "{err}");
    let err = run_err("x = no or (1 / 0 == 1)");
    assert!(err.to_lowercase().contains("cero") || err.to_lowercase().contains("zero"), "{err}");
    run_ok(r##"
        if (yes and no) { error("yes and no tenía que ser no") }
        if not (no or yes) { error("no or yes tenía que ser yes") }
    "##);
}

#[test]
fn el_caso_real_has_key_and_indexar() {
    // El patrón exacto que rompía la demo: con la clave ausente, la derecha
    // indexaba y la VM respondía "Key 'user' not found".
    run_ok(r##"
        fn es_admin(req) {
            return has_key(req, "user") and req["user"]["rol"] == "admin"
        }
        if es_admin({}) { error("sin user no es admin") }
        if not es_admin({ "user": { "rol": "admin" } }) { error("con rol admin sí") }
        d = null
        if d != null and d["x"] > 0 { error("no debía entrar") }
    "##);
}

#[test]
fn and_or_devuelven_booleanos_y_respetan_precedencia() {
    run_ok(r##"
        if (1 and "x") != yes { error("1 and 'x' tenía que ser yes") }
        if (0 or "") != no { error("0 or '' tenía que ser no") }
        -- and liga más fuerte que or: yes or (no and no) = yes
        if not (yes or no and no) { error("precedencia de and/or") }
        c = 0
        i = 0
        while i < 1000 {
            if i % 3 == 0 and i % 5 == 0 or i == 7 { c = c + 1 }
            i = i + 1
        }
        if c != 68 { error("cuenta mal: " + str(c)) }
    "##);
}

#[test]
fn and_or_anidados_en_funciones_recursivas() {
    // Cada llamada tiene su propio frame: la variable oculta del
    // cortocircuito no se puede pisar entre niveles.
    run_ok(r##"
        fn par(n) {
            return n == 0 or (n != 1 and par(n - 2))
        }
        if not par(10) { error("10 es par") }
        if par(7) { error("7 no es par") }
    "##);
}

#[test]
fn and_or_siguen_compilando_con_el_jit() {
    // Se bajan a saltos con una variable oculta precisamente para que el JIT
    // (que vacía la pila en cada frontera de bloque) los siga compilando.
    let bc = compilar(r##"
        fn cuenta(n) {
            c = 0
            i = 0
            while i < n {
                if i % 3 == 0 and i % 5 == 0 or i == 7 { c = c + 1 }
                i = i + 1
            }
            return c
        }
        if cuenta(30000) != 2001 { error("resultado distinto en JIT") }
    "##);
    match jit::run_program(&bc) {
        Ok(true) => {}
        Ok(false) => panic!("el JIT no compiló el programa y cayó al intérprete"),
        Err(e) => panic!("el JIT falló: {e}"),
    }
}

// ── 2. return dentro de attempt ──────────────────────────────────────────────

#[test]
fn return_dentro_de_attempt_no_deja_el_manejador_colgado() {
    let err = run_err(r##"
        fn f() {
            attempt {
                return 1
            } handle e {
                return 2
            }
        }
        if f() != 1 { error("f tenía que devolver 1") }
        x = 1 / 0
        error("la división por cero tenía que parar el programa")
    "##);
    assert!(err.to_lowercase().contains("cero") || err.to_lowercase().contains("zero"),
        "el error de fuera del attempt tenía que salir tal cual, salió: {err}");
}

#[test]
fn attempt_sigue_capturando_errores_nativos() {
    run_ok(r##"
        use "json" as json
        fn leer(t) {
            attempt {
                return json.parse(t)
            } handle e {
                return null
            }
        }
        if leer("no es json") != null { error("tenía que capturarlo") }
        if leer("[1]")[0] != 1 { error("tenía que parsear") }
    "##);
}

// ── 3. Módulos importados por otro módulo ────────────────────────────────────

fn orion_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_orion"))
}

#[test]
fn modulo_importado_solo_por_otro_modulo_esta_disponible() {
    let dir = std::env::temp_dir().join("orion_tests_modulos_anidados");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("lib")).unwrap();
    fs::write(dir.join("lib/b.orx"), "LIMITE = 7\nfn hola() {\n    return \"hola desde b\"\n}\n").unwrap();
    fs::write(dir.join("lib/a.orx"),
        "use \"lib/b\" as b\nfn llama() {\n    return b.hola() + \" \" + str(b.LIMITE)\n}\n").unwrap();
    // main.orx NO importa lib/b: solo lib/a, que es quien lo usa.
    fs::write(dir.join("main.orx"), "use \"lib/a\" as a\nshow a.llama()\n").unwrap();

    let out = Command::new(orion_bin())
        .args(["--run", "main.orx"])
        .current_dir(&dir)
        .output()
        .expect("no se pudo ejecutar orion");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "falló:\n{stdout}\n{stderr}");
    assert!(stdout.contains("hola desde b 7"), "salida inesperada:\n{stdout}\n{stderr}");
}
