use std::fs;
use std::path::Path;
use super::banner;

pub fn run_new(name: &str) {
    banner::section(&format!("Nuevo proyecto Orion: {name}"));

    let root = Path::new(name);
    if root.exists() {
        banner::fail(&format!("Ya existe un directorio '{name}'"));
        std::process::exit(1);
    }

    create_dir(root);
    create_dir(&root.join("tests"));

    write_file(
        &root.join(format!("{name}.orx")),
        &main_template(name),
    );
    write_file(
        &root.join("tests").join("test_main.orx"),
        &test_template(name),
    );
    write_file(
        &root.join(".orionrc"),
        &orionrc_template(name),
    );

    println!();
    banner::ok(&format!("Proyecto '{name}' creado"));
    println!();
    println!("  {DIM}Para empezar:{RESET}", DIM = banner::DIM, RESET = banner::RESET);
    println!("    cd {name}");
    println!("    orion --run {name}.orx");
    println!("    orion --test tests/");
    println!();
}

fn create_dir(path: &Path) {
    fs::create_dir_all(path)
        .unwrap_or_else(|e| {
            banner::fail(&format!("No se pudo crear directorio '{}': {e}", path.display()));
            std::process::exit(1);
        });
    banner::info(&format!("Creado  {}/", path.display()));
}

fn write_file(path: &Path, content: &str) {
    fs::write(path, content)
        .unwrap_or_else(|e| {
            banner::fail(&format!("No se pudo escribir '{}': {e}", path.display()));
            std::process::exit(1);
        });
    banner::info(&format!("Creado  {}", path.display()));
}

fn main_template(name: &str) -> String {
    format!(
r#"-- Proyecto: {name}
-- Punto de entrada principal

fn greet(name) {{
    print("Hola, " + name + "!")
}}

greet("mundo")
"#
    )
}

fn test_template(name: &str) -> String {
    format!(
r#"-- Tests para: {name}
-- Convención: archivos test_*.orx se ejecutan con `orion --test <carpeta>`
-- Un test falla si lanza un error en runtime.

fn assert_eq(a, b, msg) {{
    if a != b {{
        throw "AssertionError: " + msg + " — esperado: " + b + ", obtenido: " + a
    }}
}}

-- Test básico
assert_eq(1 + 1, 2, "suma básica")
assert_eq("hola", "hola", "strings iguales")

print("Tests de {name}: OK")
"#
    )
}

fn orionrc_template(name: &str) -> String {
    format!(
r#"# Orion project config
name = "{name}"
version = "0.1.0"
entry = "{name}.orx"
"#
    )
}
