//! Conformidad de SPEC.md con el compilador.
//!
//! SPEC.md afirma cosas concretas sobre el lenguaje. Este test las ejecuta.
//! Si el compilador cambia de comportamiento, aquí falla y SPEC.md queda
//! formalmente desmentida, que es justo lo que una spec sin test no consigue.
//!
//! El fixture es `tests/spec_examples.orx` y emite una línea `clave=valor` por
//! afirmación. Se comparan valores exactos, no subcadenas.

use std::process::Command;

/// (clave, valor esperado, sección de SPEC.md que lo afirma)
const ESPERADO: &[(&str, &str, &str)] = &[
    ("hex",                    "42",     "5 literales"),
    ("bin",                    "42",     "5 literales"),
    ("bool_si",                "yes",    "5 literales"),
    ("bool_no",                "no",     "5 literales"),
    ("interp",                 "7",      "5.1 strings"),
    ("escape_hex",             "AB",     "5.1 strings"),
    ("bitwise_vs_cmp",         "yes",    "6 precedencia: (a & b) == c, no como C"),
    ("mul_antes_de_suma",      "14",     "6 precedencia"),
    ("power_asocia_derecha",   "512",    "6 precedencia: ** asocia a la derecha"),
    ("unario_antes_de_and",    "no",     "6 precedencia"),
    ("global_intacto",         "0",      "7 scope: asignar dentro crea un local"),
    ("tipo_lambda",            "fn",     "8 valores función"),
    ("tipo_nombrada",          "string", "8 una función nombrada ES su nombre"),
    ("fn_igual_a_su_nombre",   "yes",    "8 valores función"),
    ("string_invocable",       "hi 2",   "8 un string que nombra una función es invocable"),
    ("lambda_como_arg",        "11",     "8 valores función"),
    ("lambda_literal_arg",     "11",     "8 lambda literal como argumento SÍ funciona"),
    ("lambda_en_interpolacion","ok",     "8 lambda literal dentro de ${...}"),
    ("nombrada_como_arg",      "hi 10",  "8 valores función"),
    ("t_int",                  "int",    "9 nombres de tipo"),
    ("t_float",                "float",  "9 nombres de tipo"),
    ("t_string",               "string", "9 nombres de tipo"),
    ("t_bool",                 "bool",   "9 nombres de tipo"),
    ("t_list",                 "list",   "9 nombres de tipo"),
    ("t_dict",                 "dict",   "9 nombres de tipo"),
    ("t_null",                 "null",   "9 nombres de tipo"),
    ("pipe_ident",             "10",     "6.1 x |> f  =>  f(x)"),
    ("pipe_call",              "8",      "6.1 x |> f(a)  =>  f(x, a)"),
    ("pipe_flecha",            "15",     "6.1 x |> (n) => ..."),
    ("flecha_parens",          "2",      "8 lambda de flecha"),
    ("flecha_sin_parens",      "3",      "8 lambda de flecha sin parentesis"),
    ("orden_args",             "a,b",    "9.1 argumentos de izquierda a derecha"),
    ("orden_dict",             "c,d",    "9.1 valores de dict de izquierda a derecha"),
    ("int_mas_float",          "float",  "9.2 int + float = float"),
    ("division_real",          "1.5",    "9.2 / es division real, no entera"),
    ("tipo_division",          "float",  "9.2 / devuelve float"),
    ("overflow",               "error",  "9.2 el overflow es error, no wraparound"),
    ("pow_neg_int",            "error",  "9.2 ** con exponente negativo entero"),
    ("pow_neg_float",          "0.5",    "9.2 con base float si funciona"),
    ("null_eq_undefined",      "yes",    "9.3 null y undefined son el mismo valor"),
    ("tipo_undefined",         "null",   "9.3 type(undefined) es null"),
    ("for_sobre_dict",         "no_soportado", "9.4 for..in no itera dicts"),
    ("orden_insercion",        "zeta,alfa,medio", "9.4 los dict conservan orden de insercion"),
    ("exponente",              "1000",   "5 notacion exponencial"),
    ("exponente_float",        "150",    "5 notacion exponencial con decimales"),
];

#[test]
fn spec_md_describe_el_compilador_real() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/spec_examples.orx");
    let out = Command::new(env!("CARGO_BIN_EXE_orion"))
        .args(["run", script])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
        .output()
        .expect("no se pudo ejecutar el binario de orion");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "el fixture de la spec no llegó a terminar\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut fallos = Vec::new();
    for (clave, esperado, seccion) in ESPERADO {
        let prefijo = format!("{clave}=");
        match stdout.lines().find(|l| l.trim().starts_with(&prefijo)) {
            None => fallos.push(format!("  falta '{clave}'  (SPEC {seccion})")),
            Some(linea) => {
                let real = linea.trim().strip_prefix(&prefijo).unwrap();
                if real != *esperado {
                    fallos.push(format!(
                        "  {clave}: SPEC dice '{esperado}', el compilador da '{real}'  (SPEC {seccion})"
                    ));
                }
            }
        }
    }

    assert!(
        fallos.is_empty(),
        "SPEC.md ya no describe al compilador:\n{}\n\nActualiza SPEC.md o arregla el compilador.\nSalida completa:\n{stdout}",
        fallos.join("\n")
    );
}
