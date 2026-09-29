//! Los errores de Postgres tienen que llegar a Orion con su mensaje.
//!
//! `postgres::Error` imprime solo "db error" con `{}`; el mensaje del servidor
//! va dentro. Así, un CHECK violado, una clave duplicada o una fila mala en un
//! COPY llegaban como "db error" a secas. Y `db.insert` añadía "is 'RETURNING
//! id' missing?" a CUALQUIER fallo, que casi nunca era el motivo.
//!
//! Necesita un Postgres real. Se salta si no hay uno en ORION_TEST_PG:
//!
//!   ORION_TEST_PG=postgres://usuario:clave@127.0.0.1:5432/base cargo test --test postgres_errors
//!
//! No tener Postgres a mano no es un defecto de Orion; por eso se salta en vez
//! de fallar.

use orion_vm::{codegen, lexer, parser, vm};

fn run_ok(src: &str) {
    let tokens = lexer::lex(src).unwrap_or_else(|e| panic!("lex error: {} | src:\n{}", e.message, src));
    let stmts = parser::parse(tokens).unwrap_or_else(|e| panic!("parse error: {} | src:\n{}", e.message, src));
    let bc = codegen::compile(stmts).unwrap_or_else(|e| panic!("codegen error: {} | src:\n{}", e.message, src));
    let mut machine = vm::VM::new(bc.main, bc.lines, bc.functions, bc.shapes, bc.extern_fns);
    machine.run().unwrap_or_else(|e| panic!("runtime error: {} | src:\n{}", e, src));
}

fn url() -> Option<String> {
    match std::env::var("ORION_TEST_PG") {
        Ok(u) if !u.trim().is_empty() => Some(u),
        _ => {
            eprintln!("ORION_TEST_PG no está definida: se salta el test de Postgres");
            None
        }
    }
}

#[test]
fn errores_de_postgres_con_mensaje_y_sqlstate() {
    let Some(url) = url() else { return };
    let tabla = format!("orion_test_err_{}", std::process::id());
    let dir = std::env::temp_dir().join("orion_tests_pg");
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("malo.csv");
    // La fila mala es la 3 del archivo (la 1 es la cabecera). SKUs nuevos,
    // para que lo único que falle sea el CHECK y no la clave única.
    std::fs::write(&csv, "sku,stock\nC,1\nD,-5\n").unwrap();
    let csv = csv.to_string_lossy().replace('\\', "/");

    run_ok(&format!(r##"
        use "db" as db
        BD = "{url}"
        db.exec(BD, "DROP TABLE IF EXISTS {tabla}")
        db.exec(BD, "CREATE TABLE {tabla} (id SERIAL PRIMARY KEY, sku TEXT UNIQUE, stock INTEGER CHECK (stock >= 0))")
        db.exec(BD, "INSERT INTO {tabla} (sku, stock) VALUES ('A', 1)")

        fn fallo(sql) {{
            attempt {{
                db.exec(BD, sql)
            }} handle err {{
                return str(err)
            }}
            return ""
        }}

        -- CHECK violado: el mensaje del servidor y su SQLSTATE.
        e = fallo("UPDATE {tabla} SET stock = -1 WHERE sku = 'A'")
        if not e.contains("SQLSTATE 23514") {{ error("CHECK sin SQLSTATE 23514: " + e) }}
        if e.contains("db error") and not e.contains("check") {{ error("mensaje opaco: " + e) }}

        -- Clave duplicada, con el detalle de qué clave.
        e = fallo("INSERT INTO {tabla} (sku, stock) VALUES ('A', 2)")
        if not e.contains("SQLSTATE 23505") {{ error("duplicado sin SQLSTATE 23505: " + e) }}
        if not e.contains("(sku)=(A)") {{ error("duplicado sin el detalle de la clave: " + e) }}

        -- db.insert ya no culpa a un RETURNING que sí estaba.
        e = ""
        attempt {{
            db.insert(BD, "INSERT INTO {tabla} (sku, stock) VALUES ('A', 3) RETURNING id")
        }} handle err {{
            e = str(err)
        }}
        if e.contains("RETURNING id' missing") {{ error("db.insert sigue culpando a RETURNING: " + e) }}
        if not e.contains("23505") {{ error("db.insert sin el error real: " + e) }}

        -- COPY con una fila mala: el contexto dice la línea.
        e = ""
        attempt {{
            db.copy_file(BD, "{tabla}", ["sku", "stock"], "{csv}", {{ "header": yes }})
        }} handle err {{
            e = str(err)
        }}
        if not e.contains("23514") {{ error("COPY sin el error real: " + e) }}
        if not e.contains("line 3") {{ error("COPY sin la línea del fallo: " + e) }}

        db.exec(BD, "DROP TABLE {tabla}")
    "##));
}
