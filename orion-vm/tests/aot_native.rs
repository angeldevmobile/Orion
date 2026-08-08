//! Compilación AOT end-to-end: `orion --build` debe producir un ejecutable
//! standalone cuya salida coincida con la del intérprete.
//!
//! Requiere un linker del sistema (MSVC o cc). Cuando no lo hay, los tests se
//! saltan en vez de fallar: la ausencia de toolchain no es un defecto de Orion.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn orion_bin() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") { p.pop(); }
    p.join(if cfg!(windows) { "orion.exe" } else { "orion" })
}

fn tmp_dir() -> PathBuf {
    let d = std::env::temp_dir().join("orion_aot_tests");
    fs::create_dir_all(&d).unwrap();
    d
}

/// Compila `src` a nativo y devuelve (salida del exe, modo de compilación).
/// `None` si no hay linker disponible.
fn build_and_run(name: &str, src: &str) -> Option<(String, String)> {
    let dir = tmp_dir();
    let orx = dir.join(format!("{name}.orx"));
    let exe = dir.join(format!("{name}{}", if cfg!(windows) { ".exe" } else { "" }));
    fs::write(&orx, src).unwrap();

    let build = Command::new(orion_bin())
        .args(["--build", orx.to_str().unwrap(), "-o", exe.to_str().unwrap()])
        .output()
        .expect("no se pudo ejecutar orion --build");

    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    if !build.status.success() || !exe.exists() {
        // Sin linker en el sistema no hay nada que verificar.
        if log.contains("Linker") || log.contains("linker") || log.contains("program not found") {
            eprintln!("aot: sin linker disponible, test omitido");
            return None;
        }
        panic!("orion --build falló:\n{log}");
    }

    // "Bytecode:" solo lo reporta el modo bundle; el mensaje de fallback también
    // contiene la palabra "nativo", así que no sirve para distinguirlos.
    let modo = if log.contains("Bytecode:") { "bundle" } else { "nativo" }.to_string();

    let run = Command::new(&exe).output().expect("no se pudo ejecutar el binario");
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = fs::remove_file(&orx);
    let _ = fs::remove_file(&exe);
    Some((out.replace("\r\n", "\n"), modo))
}

/// Salida del intérprete para el mismo programa.
fn interp(name: &str, src: &str) -> String {
    let dir = tmp_dir();
    let orx = dir.join(format!("{name}_i.orx"));
    fs::write(&orx, src).unwrap();
    let out = Command::new(orion_bin())
        .arg(orx.to_str().unwrap())
        .output()
        .expect("no se pudo ejecutar orion");
    let _ = fs::remove_file(&orx);
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

fn assert_aot_matches(name: &str, src: &str, modo_esperado: &str) {
    let Some((aot_out, modo)) = build_and_run(name, src) else { return };
    let vm_out = interp(name, src);
    assert_eq!(
        vm_out, aot_out,
        "AOT ({modo}) y el intérprete divergen.\n--- programa ---\n{src}\n\
         --- intérprete ---\n{vm_out}--- AOT ---\n{aot_out}"
    );
    assert_eq!(
        modo_esperado, modo,
        "se esperaba compilación '{modo_esperado}' y fue '{modo}' para:\n{src}"
    );
}

#[test]
fn aot_aritmetica_y_bucles() {
    assert_aot_matches(
        "arit",
        "x = 0\nfor i in [1, 2, 3, 4, 5] { x = x + i }\nshow x\nshow x * 2",
        "nativo",
    );
}

#[test]
fn aot_recursion() {
    assert_aot_matches(
        "recur",
        "fn fib(n) {\n    if n < 2 { return n }\n    return fib(n-1) + fib(n-2)\n}\nshow fib(20)",
        "nativo",
    );
}

#[test]
fn aot_shapes_con_on_create_y_acts() {
    // Ejercita el prólogo del AOT: registro de shapes y de punteros de acts,
    // que en JIT hace el compilador y aquí tiene que hacer el propio binario.
    assert_aot_matches(
        "oop",
        r#"shape Contador {
    n: int = 0
    paso: int = 1
    on_create(inicial: int, p: int) {
        n = inicial
        paso = p
    }
    act sube() -> int {
        n = n + paso
        return n
    }
}
c = Contador(10, 5)
show c.sube()
show c.sube()"#,
        "nativo",
    );
}

#[test]
fn aot_shape_constructor_posicional() {
    assert_aot_matches(
        "shape_pos",
        "shape P {\n    x\n    y\n}\np = P(3, 7)\nshow p.x\nshow p.y",
        "nativo",
    );
}

#[test]
fn aot_strings_y_builtins() {
    // Los literales de cadena viajan como datos del objeto, no como punteros
    // del proceso compilador.
    assert_aot_matches(
        "strs",
        r#"a = "hola"
b = "mundo"
show a + " " + b
show len([1, 2, 3])
show sort([3, 1, 2])"#,
        "nativo",
    );
}

#[test]
fn aot_fallback_a_bytecode_sigue_correcto() {
    // Los valores por defecto no son elegibles para el backend nativo: debe
    // caer al modo bundle y aun así dar el resultado correcto.
    assert_aot_matches(
        "fallback",
        r#"fn saluda(nombre, saludo = "hola") {
    return saludo + " " + nombre
}
show saluda("ana")
show saluda("beto", "buenas")"#,
        "bundle",
    );
}

//    Globales vistos desde dentro de una función
//
// El JIT compila cada función con variables locales de Cranelift, así que una
// función no tenía forma de ver nada definido fuera de ella. Con `orion run` no
// se nota, porque ahí manda la VM; pero un ejecutable compilado va entero por
// ese camino y **daba otro resultado**.
//
// La cobertura anterior no lo pillaba porque ningún test leía un global dentro
// de una función: los programas de prueba eran aritmética, recursión, shapes y
// cadenas, todos autocontenidos.

/// Compara con el intérprete sin exigir un modo de compilación concreto.
///
/// Lo que importa aquí es la **paridad**: que el binario diga lo mismo que
/// `orion run`. Si el programa es elegible para nativo o cae a bundle es una
/// decisión del compilador que puede cambiar, y atarla convertiría una mejora
/// en un test rojo.
fn assert_aot_igual_que_vm(name: &str, src: &str) {
    let Some((aot_out, modo)) = build_and_run(name, src) else { return };
    let vm_out = interp(name, src);
    assert_eq!(
        vm_out, aot_out,
        "AOT ({modo}) y el intérprete divergen.\n--- programa ---\n{src}\n\
         --- intérprete ---\n{vm_out}--- AOT ---\n{aot_out}"
    );
}

#[test]
fn aot_una_funcion_ve_una_constante_global() {
    // El caso que daba un resultado distinto SIN avisar, que es peor que
    // caerse: un binario entregado calculando otra cosa.
    assert_aot_igual_que_vm(
        "global_const",
        r#"IVA = 0.21
fn con_iva(base) {
    return base * (1 + IVA)
}
show con_iva(100)"#,
    );
}

#[test]
fn aot_una_funcion_ve_una_cadena_global() {
    assert_aot_igual_que_vm(
        "global_str",
        r#"saludo = "hola"
fn saluda(nombre) {
    return saludo + " " + nombre
}
show saluda("ana")"#,
    );
}

#[test]
fn aot_una_funcion_llama_a_un_modulo() {
    // `use "strings"` define un global con el namespace del módulo, así que
    // este es el mismo defecto con otra cara — y el que rompía cualquier
    // programa real, porque todos envuelven su lógica en funciones.
    assert_aot_igual_que_vm(
        "global_modulo",
        r#"use "strings"
fn grita(t) {
    return strings.upper(t)
}
show grita("hola")"#,
    );
}

#[test]
fn aot_una_asignacion_local_no_pisa_el_global() {
    // La otra mitad de la regla: asignar dentro de una función crea una
    // variable suya y NO toca el global. Si `LoadVar` se hubiera resuelto
    // siempre contra la tabla global, este test saldría "cambiado/original"
    // en vez de "cambiado/global".
    assert_aot_igual_que_vm(
        "global_sombra",
        r#"v = "global"
fn cambia() {
    v = "cambiado"
    return v
}
show cambia()
show v"#,
    );
}

#[test]
fn aot_el_global_se_ve_con_el_valor_del_momento_de_la_llamada() {
    // Se publica en cada asignación y no una vez al final: una función puede
    // llamarse a mitad del programa, cuando el valor todavía va por la mitad.
    assert_aot_igual_que_vm(
        "global_secuencia",
        r#"n = 1
fn lee() { return n }
show lee()
n = 2
show lee()"#,
    );
}

#[test]
fn aot_una_funcion_llamada_main_compila_nativa() {
    // El objeto generado comparte espacio de nombres con el `main` de C que
    // arranca el ejecutable. Sin prefijar los símbolos de usuario, un programa
    // con `fn main()` declaraba el mismo símbolo dos veces con firmas
    // distintas, el compilador se pasaba al modo bytecode y ninguna aplicación
    // real —que se escriben así— llegaba a compilarse nativa.
    let src = r#"fn main() {
    show "desde main"
}
main()"#;
    let Some((salida, modo)) = build_and_run("main_nativo", src) else { return };
    assert_eq!(salida, "desde main\n", "salida incorrecta");
    assert_eq!(modo, "nativo", "un programa con fn main() debería compilar nativo");
}
