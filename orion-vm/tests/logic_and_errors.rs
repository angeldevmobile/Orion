//! Regresiones del lenguaje: cortocircuito de `and`/`or`, `return` dentro
//! de `attempt` y módulos importados por otro módulo.

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

#[test]
fn and_or_anidados_dan_lo_mismo_en_jit_que_en_el_interprete() {
    // Cada `and`/`or` tiene su punto de llegada, al que el valor llega por dos
    // caminos. Anidados, los puntos se encadenan: si el JIT confundiera uno
    // con otro, la cuenta saldría distinta.
    let src = r##"
        fn cuenta(n) {
            c = 0
            i = 0
            while i < n {
                a = i % 2 == 0
                b = i % 3 == 0
                d = i % 5 == 0
                if (a and b) or (d and not a) { c = c + 1 }
                if a and (b or d) { c = c + 10 }
                x = (a or b) and (b or d)
                if x { c = c + 100 }
                i = i + 1
            }
            return c
        }
        if cuenta(3000) != 127800 { error("cuenta distinta: " + str(cuenta(3000))) }
    "##;
    run_ok(src);
    match jit::run_program(&compilar(src)) {
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

// Un módulo que pasa sus propias funciones como valor: lo que necesita para
// montar sus rutas (`router.get(r, "/x", handler)`). Antes daba "Variable
// 'doble' is not defined", porque se buscaba sin el prefijo del módulo.
#[test]
fn un_modulo_puede_usar_sus_funciones_como_valor() {
    let dir = std::env::temp_dir().join("orion_tests_funcion_como_valor");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("lib")).unwrap();
    fs::write(dir.join("lib/m.orx"),
        "fn doble(x) {\n    return x * 2\n}\nfn todas(lista) {\n    return lista.map(doble)\n}\nfn cual() {\n    return doble\n}\n").unwrap();
    fs::write(dir.join("main.orx"),
        "use \"lib/m\" as m\nshow m.todas([1, 2, 3])\nf = m.cual()\nshow f(21)\n").unwrap();
    let out = Command::new(orion_bin())
        .args(["--run", "main.orx"])
        .current_dir(&dir)
        .output()
        .expect("no se pudo ejecutar orion");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "falló:\n{stdout}\n{stderr}");
    assert!(stdout.contains("[2, 4, 6]") && stdout.contains("42"), "salida inesperada:\n{stdout}");
}

// ── Errores dentro de un módulo importado ───────────────────────────────────

// Antes el error decía `main.orx:4` y enseñaba la línea 4 del programa, que no
// tenía nada que ver: el fallo estaba en la línea 4 del módulo.
fn error_de_modulo(dir_nombre: &str, archivos: &[(&str, &str)]) -> String {
    let dir = std::env::temp_dir().join(dir_nombre);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("lib")).unwrap();
    for (ruta, src) in archivos {
        fs::write(dir.join(ruta), src).unwrap();
    }
    let out = Command::new(orion_bin())
        .args(["--run", "main.orx"])
        .current_dir(&dir)
        .output()
        .expect("no se pudo ejecutar orion");
    assert!(!out.status.success(), "tenía que fallar");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

const MAIN_CUATRO_LINEAS: &str = "use \"lib/b\" as b\n-- linea dos del main\n-- linea tres del main\nb.falla()\n";

#[test]
fn un_error_en_un_modulo_senala_el_modulo() {
    let err = error_de_modulo("orion_tests_error_modulo", &[
        ("lib/b.orx", "fn falla() {\n    x = 1\n    -- linea tres de b\n    error(\"fallo en b\")\n}\n"),
        ("main.orx", MAIN_CUATRO_LINEAS),
    ]);
    assert!(err.contains("b.orx"), "no nombra el módulo:\n{err}");
    assert!(err.contains("error(\"fallo en b\")"), "no enseña la línea del módulo:\n{err}");
    assert!(!err.contains("main.orx:4"), "sigue señalando al programa:\n{err}");
}

#[test]
fn un_error_en_un_modulo_anidado_senala_el_anidado() {
    let err = error_de_modulo("orion_tests_error_anidado", &[
        ("lib/c.orx", "fn rompe() {\n    -- linea dos de c\n    return 1 / 0\n}\n"),
        ("lib/b.orx", "use \"lib/c\" as c\nfn falla() {\n    return c.rompe()\n}\n"),
        ("main.orx", MAIN_CUATRO_LINEAS),
    ]);
    assert!(err.contains("c.orx"), "no nombra el módulo anidado:\n{err}");
    assert!(err.contains("return 1 / 0"), "no enseña la línea del anidado:\n{err}");
    assert!(err.contains("b.orx:3"), "la pila no dice dónde se llamó:\n{err}");
}

#[test]
fn un_error_en_el_programa_sigue_senalando_el_programa() {
    let err = error_de_modulo("orion_tests_error_programa", &[
        ("lib/b.orx", "fn bien() {\n    return 1\n}\n"),
        ("main.orx", "use \"lib/b\" as b\nb.bien()\nerror(\"fallo en main\")\n"),
    ]);
    assert!(err.contains("main.orx"), "no nombra el programa:\n{err}");
    assert!(err.contains("error(\"fallo en main\")"), "no enseña la línea del programa:\n{err}");
}

// ── Secretos ────────────────────────────────────────────────────────────────

fn orion_con_env(dir_nombre: &str, src: &str, env: &[(&str, &str)]) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(dir_nombre);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.orx"), src).unwrap();
    let mut cmd = Command::new(orion_bin());
    cmd.args(["--run", "main.orx"]).current_dir(&dir).env_remove("ORION_ENV");
    for (k, v) in env { cmd.env(k, v); }
    let out = cmd.output().expect("no se pudo ejecutar orion");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn un_secreto_no_sale_en_show_ni_en_los_errores() {
    let (ok, stdout, stderr) = orion_con_env("orion_tests_secreto_salida",
        "use \"secret\" as secret\nt = secret.require(\"TOKEN_PRUEBA\")\nshow \"token: \" + t\nerror(\"fallo con \" + t)\n",
        &[("TOKEN_PRUEBA", "tok_live_123456789abcdef")]);
    assert!(!ok);
    assert!(stdout.contains("token: ***") && !stdout.contains("tok_live"), "{stdout}");
    assert!(stderr.contains("fallo con ***") && !stderr.contains("tok_live_123456789abcdef"), "{stderr}");
}

#[test]
fn en_produccion_no_hay_valores_por_defecto_ni_env() {
    let (ok, _, stderr) = orion_con_env("orion_tests_secreto_produccion",
        "use \"secret\" as secret\nsecret.get(\"NO_DEFINIDO_EN_PRUEBA\", \"valor-dev\")\n",
        &[("ORION_ENV", "production")]);
    assert!(!ok && stderr.contains("defaults are not allowed"), "{stderr}");
    let (ok, _, stderr) = orion_con_env("orion_tests_secreto_produccion_env",
        "use \"secret\" as secret\nsecret.load()\n", &[("ORION_ENV", "production")]);
    assert!(!ok && stderr.contains(".env files are not loaded"), "{stderr}");
}

// Un error al inicializar un módulo detiene el programa con su archivo y línea.
// Antes se ignoraba y el programa seguía sin las variables del módulo.
#[test]
fn un_error_al_cargar_un_modulo_detiene_el_programa() {
    let err = error_de_modulo("orion_tests_error_carga_modulo", &[
        ("lib/conf.orx", "LISTO = 1\nerror(\"falta la configuración\")\nOTRO = 2\n"),
        ("main.orx", "use \"lib/conf\" as conf\nshow \"no debería llegar\"\nshow conf.LISTO\n"),
    ]);
    assert!(err.contains("lib/conf.orx:2") && err.contains("falta la configuración"), "{err}");
}
