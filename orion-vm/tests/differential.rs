//! Differential testing VM ↔ JIT (Sprint 2).
//!
//! El mismo programa Orion debe producir EXACTAMENTE la misma salida estándar
//! ejecutado por el intérprete (`orion archivo`) y por el JIT Cranelift
//! (`orion --jit archivo`). Cualquier divergencia es un bug en uno de los dos
//! backends — la clase de heisenbug más difícil de cazar sin esta red.
//!
//! Los programas usan solo el subconjunto que el JIT compila hoy: escalares,
//! aritmética, comparaciones, lógica, if/else, while, funciones y recursión.
//! (for..in sobre listas, dicts y `len` aún no están soportados por el JIT.)

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn write_temp(src: &str) -> std::path::PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut p = std::env::temp_dir();
    p.push(format!("orion_diff_{}_{}.orx", std::process::id(), id));
    fs::write(&p, src).expect("escribir archivo temporal");
    p
}

fn run(args: &[&str]) -> (String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_orion"))
        .args(args)
        .output()
        .expect("ejecutar binario orion");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.success(),
    )
}

/// Ejecuta `src` por VM y por JIT y verifica que la salida coincida.
fn assert_vm_jit_match(src: &str) {
    let path = write_temp(src);
    let p = path.to_str().unwrap();
    let (vm_out, vm_ok) = run(&[p]);
    let (jit_out, jit_ok) = run(&["--jit", p]);
    let _ = fs::remove_file(&path);

    assert!(vm_ok, "VM falló para:\n{src}\n--- stdout ---\n{vm_out}");
    assert!(jit_ok, "JIT falló para:\n{src}\n--- stdout ---\n{jit_out}");
    assert_eq!(
        vm_out, jit_out,
        "VM y JIT DIVERGEN.\n--- programa ---\n{src}\n--- VM ---\n{vm_out}--- JIT ---\n{jit_out}"
    );
}

/// Verifica que VM y JIT CONCUERDAN en el resultado, sea éxito o error:
/// mismo estado de salida (ambos ok o ambos fallan) Y mismo stdout.
///
/// Más estricto que `assert_vm_jit_match` para programas que deben FALLAR:
/// un backend que envuelve/coacciona en silencio mientras el otro aborta es
/// precisamente la divergencia que esta red caza (overflow, módulo por cero,
/// bool en aritmética, etc.).
fn assert_vm_jit_agree(src: &str) {
    let path = write_temp(src);
    let p = path.to_str().unwrap();
    let (vm_out, vm_ok) = run(&[p]);
    let (jit_out, jit_ok) = run(&["--jit", p]);
    let _ = fs::remove_file(&path);

    assert_eq!(
        vm_ok, jit_ok,
        "VM y JIT DISCREPAN en éxito/fallo (vm_ok={vm_ok}, jit_ok={jit_ok}).\n\
         --- programa ---\n{src}\n--- VM stdout ---\n{vm_out}--- JIT stdout ---\n{jit_out}"
    );
    assert_eq!(
        vm_out, jit_out,
        "VM y JIT DIVERGEN en stdout.\n--- programa ---\n{src}\n--- VM ---\n{vm_out}--- JIT ---\n{jit_out}"
    );
}

#[test]
fn diff_arithmetic_int() {
    assert_vm_jit_match("show 7 + 3\nshow 7 * 3 - 2\nshow 10 % 3\nshow -42");
}

#[test]
fn diff_arithmetic_float() {
    assert_vm_jit_match("show 3.5 + 2.25\nshow 10.0 / 4.0\nshow 2.0 * 3.5");
}

// Regresión: `float ** int`. La VM no tenía el caso (Float, Int) en `Pow` y
// erraba ("Potencia requiere números") mientras el JIT lo resolvía con `powi`.
// Ahora ambos devuelven el mismo flotante.
#[test]
fn diff_pow_float_base_int_exponent() {
    assert_vm_jit_match("x = 2.5\nshow x ** 3\nshow (-1.5) ** 2\nshow 4.0 ** 0");
}

// Regresión: igualdad numérica mixta int↔float. La VM caía a `_ => false`
// (`5 == 5.0` daba `no`) mientras el JIT promovía (`yes`). Ahora `compare_eq`
// promueve, consistente con `compare_lt`/`rt_eq`, e incluye los derivados
// `<=`/`>=` que se apoyan en la igualdad.
#[test]
fn diff_mixed_int_float_equality() {
    assert_vm_jit_match(
        "show 5 == 5.0\nshow 5 != 5.0\nshow 5.0 == 5\nshow 5 <= 5.0\nshow 5 >= 5.0\nshow 6 > 5.5",
    );
}

#[test]
fn diff_comparisons() {
    assert_vm_jit_match(
        "show 3 < 4\nshow 5 <= 5\nshow 7 > 2\nshow 9 >= 10\nshow 4 == 4\nshow 4 != 5",
    );
}

#[test]
fn diff_boolean_logic() {
    assert_vm_jit_match("show yes and no\nshow yes or no\nshow not no\nshow no and yes");
}

#[test]
fn diff_string_concat() {
    assert_vm_jit_match(r#"show "hola" + " " + "orion""#);
}

// Concatenación mixta string↔número: ambos backends muestran el número con el
// mismo formato `{}` (VM `add` con Display; JIT `rt_add`→`val_to_display`).
#[test]
fn diff_string_num_concat() {
    assert_vm_jit_match(
        "n = 5\nf = 2.5\nshow \"n=\" + n + \" f=\" + f\nshow 42 + \"!\"\nshow \"\" + 0",
    );
}

// Igualdad de strings (`==`/`!=`): VM (`compare_eq`→`PartialEq` Str) y JIT
// (`rt_eq` rama Str) coinciden. El ORDEN de strings NO se prueba: ambos lo
// rechazan (no es una operación soportada), lo cual es acuerdo, no paridad útil.
#[test]
fn diff_string_equality() {
    assert_vm_jit_match(
        "a = \"hola\"\nshow a == \"hola\"\nshow a != \"chau\"\nshow (\"x\" + \"y\") == \"xy\"",
    );
}

#[test]
fn diff_if_else() {
    assert_vm_jit_match(
        r#"n = 15
if n < 10 {
    show "bajo"
} else {
    show "alto"
}"#,
    );
}

#[test]
fn diff_while_loop() {
    assert_vm_jit_match(
        r#"i = 0
total = 0
while i < 5 {
    total = total + i
    i = i + 1
}
show total"#,
    );
}

#[test]
fn diff_function_call() {
    assert_vm_jit_match("fn cuad(n) { return n * n }\nshow cuad(9)\nshow cuad(0)");
}

#[test]
fn diff_recursion_fib() {
    assert_vm_jit_match(
        r#"fn fib(n) {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}
show fib(15)"#,
    );
}

#[test]
fn diff_nested_calls() {
    assert_vm_jit_match(
        r#"fn inc(x) { return x + 1 }
fn doble(x) { return x * 2 }
show doble(inc(inc(5)))"#,
    );
}

// Programas FUERA del subconjunto JIT (usan len/for-in/dict): `--jit` debe hacer
// fallback transparente al intérprete y producir la MISMA salida que el VM.
// Verifica que la robustez del fallback no cambia resultados.

#[test]
fn diff_fallback_for_in_list() {
    assert_vm_jit_match(
        r#"suma = 0
for i in [1, 2, 3, 4, 5] {
    suma = suma + i
}
show suma"#,
    );
}

#[test]
fn diff_fallback_dict_and_len() {
    assert_vm_jit_match(
        r#"d = {"a": 1, "b": 2, "c": 3}
show d["a"] + d["b"] + d["c"]
show len([10, 20, 30])"#,
    );
}

#[test]
fn diff_factorial_loop() {
    assert_vm_jit_match(
        r#"fn fact(n) {
    r = 1
    i = 1
    while i <= n {
        r = r * i
        i = i + 1
    }
    return r
}
show fact(6)"#,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Paridad de ERRORES y casos límite (Sprint: core-hardening).
//
// Antes de este sprint, VM y JIT divergían en silencio en estos casos: la VM
// hacía panic de Rust (backtrace al usuario) o erraba mientras el JIT envolvía
// o coaccionaba sin avisar. La decisión de diseño acordada: ambos deben dar un
// ERROR LIMPIO (mismo éxito/fallo, mismo stdout vacío). Estos tests congelan
// esa semántica para que ningún backend vuelva a desviarse.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn diff_int_overflow_add() {
    // MAX + 1: ni panic (VM) ni wrap silencioso (JIT) — error limpio en ambos.
    assert_vm_jit_agree("show 9223372036854775807 + 1");
}

#[test]
fn diff_int_overflow_mul() {
    assert_vm_jit_agree("show 9223372036854775807 * 2");
}

#[test]
fn diff_int_overflow_sub() {
    assert_vm_jit_agree("show -9223372036854775808 - 1");
}

#[test]
fn diff_mod_by_zero() {
    // VM hacía panic de Rust aquí; ahora ambos dan error limpio.
    assert_vm_jit_agree("show 10 % 0");
}

#[test]
fn diff_div_by_zero() {
    assert_vm_jit_agree("show 10 / 0");
}

#[test]
fn diff_bool_in_arithmetic() {
    // `yes + yes`: la VM erraba, el JIT concatenaba a "yesyes". Ahora ambos
    // rechazan bool en aritmética con error de tipo.
    assert_vm_jit_agree("show yes + yes");
}

#[test]
fn diff_bool_plus_int() {
    assert_vm_jit_agree("show no + 1");
}

#[test]
fn diff_pow_negative_exponent() {
    // Exponente negativo en ints: el JIT devolvía 0 en silencio; ahora error.
    assert_vm_jit_agree("show 2 ** -3");
}

#[test]
fn diff_pow_overflow() {
    assert_vm_jit_agree("show 2 ** 100");
}

// Caso de control: aritmética válida en el límite NO debe fallar y debe coincidir.
#[test]
fn diff_int_max_no_overflow() {
    assert_vm_jit_agree("show 9223372036854775806 + 1");
}

// ── Listas por referencia: paridad VM ↔ JIT del aliasing/mutación ───────────
// Antes el JIT clonaba en push/append/reverse/sort/set-index → divergía de la VM
// (que muta in-place vía Rc<RefCell>). Ahora ambos backends mutan in-place.

#[test]
fn diff_list_push_mutates_self() {
    assert_vm_jit_match("xs = [1, 2]\nxs.push(9)\nshow xs[2]");
}

#[test]
fn diff_list_push_aliasing() {
    // ys comparte backing con xs: el push de xs se ve en ys.
    assert_vm_jit_match("xs = [1, 2]\nys = xs\nxs.push(3)\nshow ys[2]");
}

#[test]
fn diff_list_set_index_aliasing() {
    assert_vm_jit_match("m = [0, 0, 0]\nn = m\nm[1] = 7\nshow n[1]");
}

#[test]
fn diff_list_mutation_through_function() {
    assert_vm_jit_match("fn agg(l, v) {\n  l.push(v)\n}\nzs = [10]\nagg(zs, 20)\nshow zs[1]");
}

#[test]
fn diff_list_reverse_in_place() {
    assert_vm_jit_match("xs = [1, 2, 3]\nxs.reverse()\nshow xs[0]");
}

#[test]
fn diff_list_sort_in_place() {
    assert_vm_jit_match("xs = [3, 1, 2]\nxs.sort()\nshow xs[0]");
}

// Igualdad estructural de listas/dicts con `==`/`!=`. Antes el JIT solo comparaba
// escalares y devolvía `false` para listas/dicts (comparación por identidad de
// puntero) → `[1,2] == [1,2]` daba `no` mientras la VM daba `yes`.

#[test]
fn diff_list_eq_structural() {
    assert_vm_jit_match("show [1, 2, 3] == [1, 2, 3]");
    assert_vm_jit_match("show [1, 2] == [1, 3]");
    assert_vm_jit_match("show [1, 2, 3] == [1, 2]");
}

#[test]
fn diff_list_eq_alias_and_neq() {
    assert_vm_jit_match("a = [1, 2]\nb = a\nshow a == b");
    assert_vm_jit_match("show [1, 2] != [1, 2]");
    assert_vm_jit_match("show [1, 2] != [3, 4]");
}

#[test]
fn diff_list_eq_nested() {
    assert_vm_jit_match("show [1, [2, 3]] == [1, [2, 3]]");
    assert_vm_jit_match("show [[1], [2]] == [[1], [2, 9]]");
}

#[test]
fn diff_dict_eq_structural() {
    assert_vm_jit_match("show { \"a\": 1 } == { \"a\": 1 }");
    assert_vm_jit_match("show { \"a\": 1 } == { \"a\": 2 }");
}

#[test]
fn diff_dict_eq_order_independent() {
    // IndexMap compara sin importar el orden de inserción; el JIT debe igualar.
    assert_vm_jit_match("show { \"a\": 1, \"b\": 2 } == { \"b\": 2, \"a\": 1 }");
}

#[test]
fn diff_eq_mixed_structures() {
    assert_vm_jit_match("show { \"items\": [1, 2] } == { \"items\": [1, 2] }");
    assert_vm_jit_match("show [{ \"x\": 1 }] == [{ \"x\": 1 }]");
}

#[test]
fn diff_eq_after_mutation() {
    assert_vm_jit_match("a = [1, 2, 3]\na.push(4)\nshow a == [1, 2, 3, 4]");
    assert_vm_jit_match("m = [[1], [2]]\nm[0].push(9)\nshow m == [[1, 9], [2]]");
}

#[test]
fn diff_eq_in_conditional() {
    assert_vm_jit_match("if [1] == [1] { show \"igual\" } else { show \"distinto\" }");
}

// `a.pop()` (sintaxis de método): contrato estándar = quita y devuelve el último,
// mutando in-place. Antes la VM erraba ("List no tiene método 'pop'") y el JIT
// devolvía el último sin quitarlo → divergían.
#[test]
fn diff_list_pop_method() {
    assert_vm_jit_match("a = [1, 2, 3]\nx = a.pop()\nshow x\nshow a");
}

#[test]
fn diff_list_pop_until_empty() {
    assert_vm_jit_match("a = [1]\nx = a.pop()\ny = a.pop()\nshow x\nshow y\nshow a");
}
