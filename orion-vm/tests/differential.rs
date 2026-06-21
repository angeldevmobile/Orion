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

#[test]
fn diff_arithmetic_int() {
    assert_vm_jit_match("show 7 + 3\nshow 7 * 3 - 2\nshow 10 % 3\nshow -42");
}

#[test]
fn diff_arithmetic_float() {
    assert_vm_jit_match("show 3.5 + 2.25\nshow 10.0 / 4.0\nshow 2.0 * 3.5");
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
