use std::fs;
use std::path::Path;
use super::banner;

pub fn run_new(name: &str) {
    banner::section(&format!("New Orion project: {name}"));

    let root = Path::new(name);
    if root.exists() {
        banner::fail(&format!("A directory '{name}' already exists"));
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
    write_file(
        &root.join("orion.json"),
        &manifest_template(name),
    );

    println!();
    banner::ok(&format!("Project '{name}' created"));
    println!();
    println!("  {DIM}Para empezar:{RESET}", DIM = banner::DIM, RESET = banner::RESET);
    println!("    cd {name}");
    println!("    orion --run {name}.orx");
    println!("    orion --test tests/");
    println!("    orion --publish          (cuando quieras compartirlo)");
    println!();
}

fn create_dir(path: &Path) {
    fs::create_dir_all(path)
        .unwrap_or_else(|e| {
            banner::fail(&format!("Could not create directory '{}': {e}", path.display()));
            std::process::exit(1);
        });
    banner::info(&format!("Created  {}/", path.display()));
}

fn write_file(path: &Path, content: &str) {
    fs::write(path, content)
        .unwrap_or_else(|e| {
            banner::fail(&format!("Could not write '{}': {e}", path.display()));
            std::process::exit(1);
        });
    banner::info(&format!("Created  {}", path.display()));
}

fn main_template(name: &str) -> String {
    format!(
r#"-- Proyecto: {name}
-- Punto de entrada principal

fn greet(nombre) {{
    show "Hola, " + nombre + "!"
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
        error "AssertionError: " + msg + " — esperado: " + str(b) + ", obtenido: " + str(a)
    }}
}}

-- Tests básicos
assert_eq(1 + 1, 2, "basic addition")
assert_eq("hola", "hola", "strings iguales")

show "{name} tests: OK"
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

fn manifest_template(name: &str) -> String {
    format!(
r#"{{
  "name": "{name}",
  "version": "0.1.0",
  "description": "Short description of the {name} package",
  "author": "",
  "license": "MIT",
  "tags": [],
  "file": "{name}.orx"
}}
"#
    )
}
