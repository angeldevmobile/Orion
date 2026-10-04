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

/// Exige que `--jit` haya compilado `src` a nativo: si cayera al intérprete,
/// la salida coincidiría igual y `assert_vm_jit_match` no lo notaría.
fn assert_jit_nativo(src: &str) {
    let path = write_temp(src);
    let out = Command::new(env!("CARGO_BIN_EXE_orion"))
        .args(["--jit", path.to_str().unwrap()])
        .output()
        .expect("ejecutar binario orion");
    let _ = fs::remove_file(&path);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Cranelift nativo"), "no compiló a nativo:\n{err}");
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

// ── Paridad VM ↔ JIT del ecosistema de paquetes ──────────────────────────────
//
// `use "packages/..."` y los módulos nativos deben dar el mismo resultado en
// ambos backends. El JIT puentea los módulos `.orx` ejecutándolos vía VM, así
// que aquí cazamos cualquier divergencia del puente. Se corre desde la raíz del
// repo (donde vive packages/), no desde el crate.

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("raíz del repo")
        .to_path_buf()
}

/// Igual que `assert_vm_jit_match` pero con cwd = raíz del repo, para que
/// `use "packages/..."` resuelva los archivos reales del registry.
fn assert_vm_jit_match_pkg(src: &str) {
    let path = write_temp(src);
    let p = path.to_str().unwrap();
    let root = repo_root();

    let run_in = |args: &[&str]| -> (String, bool) {
        let out = Command::new(env!("CARGO_BIN_EXE_orion"))
            .args(args)
            .current_dir(&root)
            .output()
            .expect("ejecutar binario orion");
        (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.success())
    };

    let (vm_out, vm_ok) = run_in(&[p]);
    let (jit_out, jit_ok) = run_in(&["--jit", p]);
    let _ = fs::remove_file(&path);

    assert!(vm_ok, "VM falló para:\n{src}\n--- stdout ---\n{vm_out}");
    assert!(jit_ok, "JIT falló para:\n{src}\n--- stdout ---\n{jit_out}");
    assert_eq!(
        vm_out, jit_out,
        "VM y JIT DIVERGEN en paquetes.\n--- programa ---\n{src}\n--- VM ---\n{vm_out}--- JIT ---\n{jit_out}"
    );
}

#[test]
fn diff_pkg_math_orx() {
    // math.orx vía puente JIT→VM: recursión (factorial) y helpers internos.
    assert_vm_jit_match_pkg(
        "use \"packages/math\"\nshow math.factorial(5)\nshow math.clamp(120, 0, 100)\nshow math.pow(2, 10)",
    );
}

#[test]
fn diff_pkg_list_orx() {
    // list.orx: función `contains` no debe ser eclipsada por el método nativo de dict.
    assert_vm_jit_match_pkg(
        "use \"packages/list\"\nshow list.sum([1, 2, 3, 4])\nshow list.contains([1, 2, 3], 2)\nshow list.contains([1, 2, 3], 9)",
    );
}

#[test]
fn diff_pkg_validate_orx() {
    // validate.orx: el paquete `.orx` (is_email) no debe ser eclipsado por el módulo nativo.
    assert_vm_jit_match_pkg(
        "use \"packages/validate\"\nshow validate.is_email(\"a@b.com\")\nshow validate.is_digits(\"123\")\nshow validate.is_digits(\"12a\")",
    );
}

#[test]
fn diff_pkg_native_module() {
    // Módulo nativo directo bajo JIT (random.int determinista en rango unitario).
    assert_vm_jit_match_pkg("use \"random\"\nshow random.int(7, 7)");
}

#[test]
fn diff_pkg_wrapper_over_native() {
    // dates.orx envuelve el módulo nativo `datetime` con `use` interno.
    assert_vm_jit_match_pkg(
        "use \"packages/dates\"\nshow dates.is_weekend(\"2026-06-27\")\nshow dates.add_days(\"2026-06-29\", 7)",
    );
}

// ── Builtins bajo JIT (puente a la VM) ────────────────────────────────────────
// Antes, CUALQUIER llamada a un builtin (str, len, push, …) descalificaba el
// programa → fallback a la VM. Ahora el JIT los despacha vía `rt_call_builtin`.
// Estos tests fijan la paridad exacta VM↔JIT del puente, incluida la mutación
// in-place con aliasing (push/pop/sort/reverse escriben el backing compartido).

#[test]
fn diff_builtin_str_len() {
    assert_vm_jit_match("a = [1, 2, 3]\nshow \"len = \" + str(len(a))\nshow str(a)");
}

#[test]
fn diff_builtin_range_sum() {
    assert_vm_jit_match("show str(sum(range(1, 5)))\nshow str(range(0, 3))");
}

#[test]
fn diff_builtin_min_max_abs() {
    assert_vm_jit_match("show str(min([4, 1, 7]))\nshow str(max([4, 1, 7]))\nshow str(abs(0 - 9))");
}

#[test]
fn diff_builtin_push_aliasing() {
    // El caso estrella: push muta in-place y el alias `b` debe ver el cambio,
    // igual que la semántica por referencia de la VM.
    assert_vm_jit_match(
        "a = [1, 2, 3]\nb = a\npush(a, 99)\nshow a\nshow b\nshow \"len=\" + str(len(a))",
    );
}

#[test]
fn diff_builtin_sort_pop() {
    assert_vm_jit_match(
        "xs = [5, 2, 8, 1]\nsort(xs)\nshow xs\nr = pop(xs)\nshow xs\nshow str(r[0])",
    );
}

#[test]
fn diff_builtin_reverse() {
    assert_vm_jit_match("xs = [1, 2, 3, 4]\nreverse(xs)\nshow xs");
}

#[test]
fn diff_builtin_strings() {
    assert_vm_jit_match(
        "s = \"Hola Mundo\"\nshow upper(s)\nshow lower(s)\nshow str(len(s))\nshow join(split(s, \" \"), \"-\")",
    );
}

#[test]
fn diff_builtin_mixed_with_userfn() {
    // Builtins y funciones de usuario en el mismo programa JIT-compilado.
    assert_vm_jit_match(
        "fn doble(n) { return n * 2 }\nxs = [1, 2, 3]\npush(xs, doble(5))\nshow xs\nshow str(sum(xs))",
    );
}

// ── Valores por defecto en parámetros ────────────────────────────────────────
// Una llamada que omite args (usa defaults) hace que el JIT caiga al intérprete
// (no rellena defaults); estos tests fijan que VM y JIT dan lo mismo igual.

#[test]
fn diff_default_params_omitidos() {
    assert_vm_jit_match(
        "fn saluda(nombre, saludo = \"Hola\", signo = \"!\") { return saludo + \", \" + nombre + signo }\n\
         show saluda(\"Ana\")\n\
         show saluda(\"Luis\", \"Buenas\")\n\
         show saluda(\"Zoe\", \"Hey\", \"?\")",
    );
}

#[test]
fn diff_default_params_numericos() {
    assert_vm_jit_match(
        "fn suma(a, b = 10, c = 100) { return a + b + c }\n\
         show str(suma(1))\n\
         show str(suma(1, 2))\n\
         show str(suma(1, 2, 3))",
    );
}

// ── Argumentos con nombre (named args) ───────────────────────────────────────
// El pase de named_args los reordena a posicional (rellenando huecos con
// defaults) antes de compilar, así que VM y JIT ven el mismo bytecode posicional.

#[test]
fn diff_named_args_reordenados() {
    assert_vm_jit_match(
        "fn area(ancho, alto) { return ancho * alto }\n\
         show str(area(alto = 3, ancho = 5))\n\
         show str(area(5, alto = 4))",
    );
}

#[test]
fn diff_named_args_salta_opcional() {
    assert_vm_jit_match(
        "fn con(host, puerto = 5432, timeout = 30) { return puerto + timeout }\n\
         show str(con(\"a\"))\n\
         show str(con(\"a\", timeout = 60))\n\
         show str(con(\"a\", puerto = 100))",
    );
}

// ── break / continue (fix 2026-07-15: Jump(0) sin parchear = bucle infinito) ─

#[test]
fn diff_break_en_while() {
    assert_vm_jit_match(
        r#"i = 0
while i < 100 {
    if i == 3 { break }
    i = i + 1
}
show i"#,
    );
}

#[test]
fn diff_continue_en_while() {
    assert_vm_jit_match(
        r#"i = 0
pares = 0
while i < 10 {
    i = i + 1
    if i % 2 != 0 { continue }
    pares = pares + 1
}
show pares"#,
    );
}

#[test]
fn diff_break_dentro_de_fn() {
    assert_vm_jit_match(
        r#"fn corta(n) {
    i = 0
    while i < n {
        if i * i > 20 { break }
        i = i + 1
    }
    return i
}
show corta(100)"#,
    );
}

//     Constructor implícito de shapes (sin on_create)

#[test]
fn diff_shape_constructor_posicional() {
    // El JIT ignoraba los args cuando el shape no declara on_create: creaba la
    // instancia con todos los campos en null y devolvía null en vez del valor.
    assert_vm_jit_match(
        r#"shape P {
    x
}
p = P(5)
show p.x"#,
    );
}

#[test]
fn diff_shape_constructor_posicional_multi_campo() {
    assert_vm_jit_match(
        r#"shape Punto {
    x
    y
}
p = Punto(3, 7)
show p.x
show p.y"#,
    );
}

#[test]
fn diff_shape_constructor_posicional_dos_instancias() {
    // Los args se drenan del buffer compartido: una instancia no debe heredar
    // los argumentos de la anterior.
    assert_vm_jit_match(
        r#"shape P {
    x
}
a = P(5)
b = P(9)
show a.x
show b.x"#,
    );
}

#[test]
fn diff_shape_constructor_posicional_desde_fn() {
    assert_vm_jit_match(
        r#"shape P {
    x
}
fn crea(v) {
    return P(v)
}
q = crea(42)
show q.x"#,
    );
}

#[test]
fn diff_shape_constructor_sin_args_deja_null() {
    assert_vm_jit_match(
        r#"shape P {
    x
}
p = P()
show p.x"#,
    );
}

#[test]
fn diff_shape_constructor_demasiados_args_falla() {
    // Más argumentos que campos: ambos backends deben rechazar, no truncar.
    assert_vm_jit_agree(
        r#"shape P {
    x
}
p = P(1, 2)
show p.x"#,
    );
}

#[test]
fn dict_conserva_el_orden_de_escritura_en_ambos_backends() {
    // Los pares salen de la pila al revés que en el literal. La VM los voltea;
    // el JIT decía replicarla y se saltaba justo ese paso, así que un
    // diccionario salía con las claves invertidas SOLO en el ejecutable
    // compilado. De ese orden dependen cosas que se ven: el JSON que se genera,
    // las columnas de un CSV y lo que imprime un `show`.
    assert_vm_jit_match(
        r#"d = { zeta: 1, alfa: 2, medio: 3 }
show d
show keys(d)"#,
    );
}

#[test]
fn dict_devuelto_por_una_funcion_conserva_el_orden() {
    assert_vm_jit_match(
        r#"fn ficha() {
    return { sku: "X1", nombre: "cosa", precio: 9 }
}
show ficha()
show keys(ficha())"#,
    );
}

#[test]
fn el_namespace_de_un_modulo_es_del_mismo_tipo_en_ambos_backends() {
    // En el JIT el namespace se representa como un dict con un marcador, pero
    // para la VM es un `Value::Module`. Sin traducirlo de vuelta, `type(fs)`
    // decía `dict` en el ejecutable compilado y `module<fs>` con `orion run`.
    assert_vm_jit_match(
        r#"use "strings"
fn tipo() { return str(type(strings)) }
show tipo()
show type(strings)"#,
    );
}

#[test]
fn builtins_de_lista_dan_lo_mismo_en_ambos_backends() {
    // En el JIT van directos sobre la lista, sin pasar por la VM.
    assert_vm_jit_match(
        r#"xs = [1, 2]
push(xs, 3)
push(xs, 4)
show xs
show len(xs)
show first(xs)
show last(xs)
show pop(xs)
show xs
show len("hola")
show len({ a: 1, b: 2 })
v = []
show first(v)
show last(v)
show pop(v)"#,
    );
}

#[test]
fn push_devuelve_la_misma_lista_en_ambos_backends() {
    // El JIT devolvía una copia: lo que se añadía después no llegaba a `xs`.
    assert_vm_jit_match(
        r#"xs = [1]
ys = push(xs, 2)
push(ys, 3)
show xs
show len(xs)"#,
    );
}

#[test]
fn push_en_bucle_no_es_cuadratico_en_el_jit() {
    // Cada push copiaba la lista entera: 200k elementos tardaban minutos.
    let t0 = std::time::Instant::now();
    assert_vm_jit_match(
        r#"fn llenar(n) {
    xs = []
    i = 0
    while i < n {
        push(xs, i)
        i = i + 1
    }
    return len(xs)
}
show llenar(200000)"#,
    );
    assert!(t0.elapsed().as_secs() < 30, "tardó {:?}", t0.elapsed());
}

// ── Errores: el JIT los lleva al `handle`, también a través de llamadas ─────

#[test]
fn un_error_en_una_funcion_llega_al_attempt_de_quien_llama() {
    // El JIT terminaba el proceso: `Raise` solo veía los attempt de su función.
    assert_vm_jit_match(
        r#"fn f() {
    error "fallo en f"
}
fn g(x) {
    return f()
}
attempt { g(1) } handle e { show "atrapado: " + e }
show "sigue""#,
    );
}

#[test]
fn errores_del_runtime_van_al_handle_con_el_mismo_mensaje() {
    assert_vm_jit_match(
        r#"fn div(a, b) { return a / b }
attempt { div(1, 0) } handle e { show e }
attempt { x = 1 / 0.0 } handle e { show e }
attempt { x = 7 % 0 } handle e { show e }
attempt { x = 9223372036854775807 + 1 } handle e { show e }
attempt { x = "a" - 1 } handle e { show e }
attempt { x = [1, 2][5] } handle e { show e }
attempt { x = { a: 1 }["b"] } handle e { show e }
attempt { x = 1 < "b" } handle e { show e }
show "fin""#,
    );
}

#[test]
fn un_error_a_mitad_de_un_bucle_no_corta_el_bucle() {
    assert_vm_jit_match(
        r#"fn contar(n) {
    fallos = 0
    vueltas = 0
    i = 0
    while i < n {
        attempt {
            x = 10 / (i - 700)
        } handle e {
            fallos = fallos + 1
        }
        vueltas = vueltas + 1
        i = i + 1
    }
    show fallos
    return vueltas
}
show contar(3000)"#,
    );
}

#[test]
fn un_error_en_un_act_llega_al_attempt() {
    assert_vm_jit_match(
        r#"shape Cuenta {
    saldo
    act retirar(n) {
        if n > saldo { error "saldo insuficiente" }
        saldo = saldo - n
        return saldo
    }
}
c = Cuenta(10)
attempt { c.retirar(50) } handle e { show "atrapado: " + e }
show c.retirar(3)"#,
    );
}

#[test]
fn un_error_sin_attempt_termina_igual_en_ambos_backends() {
    // Lo impreso antes del error sale, y los dos terminan con fallo.
    assert_vm_jit_agree(
        r#"fn f(x) { return 10 / x }
show "antes"
show f(0)
show "nunca""#,
    );
}

#[test]
fn un_error_en_una_tarea_async_llega_al_await() {
    assert_vm_jit_match(
        r#"async fn dividir(a, b) { return a / b }
t = dividir(1, 0)
attempt { r = await t } handle e { show "atrapado: " + e }
show "sigue""#,
    );
}

#[test]
fn enteros_que_cruzan_los_48_bits_y_decimales_especiales() {
    // El JIT guarda los enteros de 48 bits dentro del valor y los demás en el
    // heap: las cuentas que cruzan esa frontera tienen que dar lo mismo.
    assert_vm_jit_match(
        r#"fn crecer(x, n) {
    i = 0
    while i < n {
        x = x * 2
        i = i + 1
    }
    return x
}
b = 140737488355327
show b
show b + 1
show crecer(1, 50)
show crecer(-1, 62)
show 9223372036854775807
show -9223372036854775807 - 1
show crecer(1, 50) - crecer(1, 50) + 7
show 140737488355328 == 140737488355327 + 1
show -0.0
show 1.0e308 * 10.0
show 0.1 + 0.2"#,
    );
}

#[test]
fn las_rutas_rapidas_dan_lo_mismo_y_compilan_a_nativo() {
    // Si Cranelift rechazara el código en línea, el JIT caería al intérprete
    // y la salida coincidiría igual: por eso se exige también "nativo".
    let src = r#"fn f(a, b) {
    show a + b
    show a - b
    show a * b
    attempt { show a / b } handle e { show e }
    show a < b
    show a <= b
    show a > b
    show a >= b
    show a == b
    show a != b
}
fn m(a, b) {
    attempt { show a % b } handle e { show e }
}
f(7, 3)
f(-7, 3)
f(5, 0)
f(2.5, 0.5)
f(7, 2.5)
f(2.5, 0.0)
f(140737488355327, 1)
f(-140737488355328, 1)
f(100000000, 100000000)
show "a" + "b"
m(7, 3)
m(-7, 3)
m(7, 0)
m(7.5, 2)
show 1 == 1.0
fn verdad(v) {
    if v { return "si" }
    return "no"
}
show verdad(yes)
show verdad(no)
show verdad(null)
show verdad(0)
show verdad(1)
show verdad("")
show verdad([1])
i = 0
s = 0.0
while i < 1000 {
    s = s + i * 0.5
    i = i + 1
}
show s"#;
    assert_vm_jit_match(src);
    assert_jit_nativo(src);
}

#[test]
fn el_ternario_compila_a_nativo_en_cualquier_posicion() {
    // El ternario deja un valor vivo al cruzar de bloque; antes eso mandaba el
    // programa entero al intérprete.
    let src = r#"fn signo(n) {
    return n > 0 ? "positivo" : (n < 0 ? "negativo" : "cero")
}
fn suma_pares(n) {
    acc = 0
    i = 0
    while i < n {
        acc = acc + (i % 2 == 0 ? i : 0)
        i = i + 1
    }
    return acc
}
fn elige(a, b) { return a + b }
show signo(5)
show signo(-3)
show signo(0)
show suma_pares(100)
show elige(1 > 0 ? 10 : 20, 2 > 3 ? 100 : 200)
x = 7
show 1 + (x > 5 ? x * 2 : x) * 3
show (x > 5 and x < 10) ? "en rango" : "fuera"
show [x > 0 ? "a" : "b", x > 100 ? "c" : "d"]
attempt {
    show x > 0 ? 10 / 0 : 1
} handle e { show e }"#;
    assert_vm_jit_match(src);
    assert_jit_nativo(src);
}

#[test]
fn las_variables_de_main_que_leen_las_funciones_siguen_visibles() {
    // En el JIT, main solo copia a la tabla de globales lo que alguna función,
    // act o tarea lee; lo demás queda en registros.
    let src = r#"factor = 3
total = 0
fn escala(n) { return n * factor }
async fn escala_async(n) { return n * factor }
shape Caja {
    v
    act doble() { return v * factor }
}
i = 0
while i < 5 {
    total = total + escala(i)
    factor = factor + 1
    i = i + 1
}
show total
show factor
show await escala_async(2)
show Caja(10).doble()
solo_main = 0
j = 0
while j < 1000 {
    solo_main = solo_main + j
    j = j + 1
}
show solo_main"#;
    assert_vm_jit_match(src);
    assert_jit_nativo(src);
}

#[test]
fn el_error_de_una_tarea_spawn_se_escribe_en_stderr() {
    // Nadie puede hacer `await` de `spawn f()`: su error se perdía y, con
    // canales, el programa se colgaba sin decir por qué. La tarea guardada en
    // `t` sí se espera, y su error va al `handle` sin imprimirse.
    let src = r#"use "tarea"
async fn falla(n) { return 10 / n }
spawn falla(0)
t = falla(0)
attempt { r = await t } handle e { show "atrapado: " + e }
tarea.sleep(300)
show "fin""#;
    assert_vm_jit_match(src);
    for modo in [None, Some("--jit")] {
        let path = write_temp(src);
        let mut args: Vec<&str> = modo.into_iter().collect();
        args.push(path.to_str().unwrap());
        let out = Command::new(env!("CARGO_BIN_EXE_orion"))
            .args(&args)
            .output()
            .expect("ejecutar binario orion");
        let _ = fs::remove_file(&path);
        let err = String::from_utf8_lossy(&out.stderr);
        let aviso = "error in spawned task 'falla': División por cero";
        assert_eq!(err.matches(aviso).count(), 1, "{modo:?}: stderr fue:\n{err}");
    }
}
