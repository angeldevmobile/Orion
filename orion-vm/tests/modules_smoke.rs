//! Smoke tests de la stdlib (Sprint 3).
//!
//! La biblioteca estándar (56 módulos) es el diferenciador de Orion, pero tenía
//! 0 tests. Estos smoke tests verifican que los módulos clave se despachan y
//! ejecutan operaciones básicas correctamente vía `modules::call`.

use orion_vm::eval_value::EvalValue;
use orion_vm::modules;
use std::collections::HashMap;

fn call(m: &str, f: &str, args: Vec<EvalValue>) -> EvalValue {
    modules::call(m, f, args).unwrap_or_else(|e| panic!("{m}.{f} falló: {e}"))
}
fn s(x: &str) -> EvalValue {
    EvalValue::Str(x.into())
}

// EvalValue no implementa PartialEq → extraemos el valor primitivo.
fn as_str(v: EvalValue) -> String {
    match v { EvalValue::Str(s) => s, o => panic!("se esperaba Str, fue {o:?}") }
}
fn as_int(v: EvalValue) -> i64 {
    match v { EvalValue::Int(n) => n, o => panic!("se esperaba Int, fue {o:?}") }
}
fn as_bool(v: EvalValue) -> bool {
    match v { EvalValue::Bool(b) => b, o => panic!("se esperaba Bool, fue {o:?}") }
}
fn list_len(v: EvalValue) -> usize {
    match v { EvalValue::List(l) => l.len(), o => panic!("se esperaba List, fue {o:?}") }
}

#[test]
fn smoke_strings() {
    assert_eq!(as_str(call("strings", "upper", vec![s("orion")])), "ORION");
    assert_eq!(as_str(call("strings", "lower", vec![s("ORION")])), "orion");
    assert_eq!(as_int(call("strings", "length", vec![s("orion")])), 5);
    assert_eq!(list_len(call("strings", "split", vec![s("a,b,c"), s(",")])), 3);
}

#[test]
fn smoke_json_roundtrip() {
    let parsed = call("json", "parse", vec![s("[1, 2, 3]")]);
    assert_eq!(list_len(parsed.clone()), 3);
    assert_eq!(as_str(call("json", "forge", vec![parsed])), "[1,2,3]");
}

#[test]
fn smoke_datetime() {
    assert!(matches!(call("datetime", "timestamp", vec![]), EvalValue::Int(_)));
    assert!(matches!(call("datetime", "now", vec![]), EvalValue::Str(_)));
}

#[test]
fn smoke_regex() {
    // is_match(text, pattern) → bool
    assert!(as_bool(call("regex", "is_match", vec![s("abc123"), s(r"\d+")])));
    assert!(!as_bool(call("regex", "is_match", vec![s("abc"), s(r"\d+")])));
    // replace(text, pattern, repl) → string
    assert_eq!(as_str(call("regex", "replace", vec![s("a1b2"), s(r"\d"), s("#")])), "a#b#");
}

#[test]
fn smoke_excel_stats() {
    let row = |x: i64| {
        let mut m = HashMap::new();
        m.insert("x".to_string(), EvalValue::Int(x));
        EvalValue::Dict(m)
    };
    let data = EvalValue::List(vec![row(10), row(20), row(30)]);
    let stats = modules::call("excel", "estadisticas", vec![data, s("x")]);
    assert!(stats.is_ok(), "excel.estadisticas falló: {:?}", stats.err());
}

#[test]
fn smoke_db_roundtrip() {
    let mut path = std::env::temp_dir();
    path.push(format!("orion_smoke_db_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let p = || EvalValue::Str(path.to_str().unwrap().to_string());

    call("db", "ejecutar", vec![p(), s("CREATE TABLE t (id INTEGER, nombre TEXT)")]);
    call("db", "ejecutar", vec![p(), s("INSERT INTO t VALUES (1, 'orion')")]);
    let rows = call("db", "query", vec![p(), s("SELECT id, nombre FROM t")]);
    let _ = std::fs::remove_file(&path);

    assert_eq!(list_len(rows), 1, "db debió devolver 1 fila");
}

#[test]
fn smoke_fs_roundtrip() {
    // Carpeta temporal única: mkdir → write → read → size → append → ls → delete.
    let mut base = std::env::temp_dir();
    base.push(format!("orion_smoke_fs_{}", std::process::id()));
    let dir = base.to_str().unwrap().to_string();
    let file = format!("{dir}/hola.txt");
    let _ = std::fs::remove_dir_all(&dir);

    // crear carpeta
    call("fs", "mkdir", vec![s(&dir)]);
    assert!(as_bool(call("fs", "is_dir", vec![s(&dir)])), "is_dir tras mkdir");
    assert!(as_bool(call("fs", "exists", vec![s(&dir)])));

    // escribir + leer
    call("fs", "write", vec![s(&file), s("hola orion")]);
    assert!(as_bool(call("fs", "is_file", vec![s(&file)])), "is_file tras write");
    assert_eq!(as_str(call("fs", "read", vec![s(&file)])), "hola orion");
    assert_eq!(as_int(call("fs", "size", vec![s(&file)])), 10);

    // append
    call("fs", "append", vec![s(&file), s("!")]);
    assert_eq!(as_str(call("fs", "read", vec![s(&file)])), "hola orion!");

    // listar carpeta
    assert!(list_len(call("fs", "ls", vec![s(&dir)])) >= 1, "ls debe ver el archivo");

    // borrar archivo
    call("fs", "delete", vec![s(&file)]);
    assert!(!as_bool(call("fs", "exists", vec![s(&file)])), "el archivo debió borrarse");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn smoke_process_execute() {
    // execute(cmd) → dict {code, out, err}. `echo` existe en cmd y sh.
    match call("process", "execute", vec![s("echo orion")]) {
        EvalValue::Dict(m) => {
            assert!(matches!(m.get("code"), Some(EvalValue::Int(_))), "execute debe devolver 'code'");
            assert!(m.contains_key("out"), "execute debe devolver 'out'");
        }
        other => panic!("process.execute no devolvió dict: {other:?}"),
    }
}

#[test]
fn smoke_env_set_get() {
    let key = format!("ORION_SMOKE_{}", std::process::id());
    assert!(!as_bool(call("env", "has", vec![s(&key)])), "la var no debería existir aún");
    call("env", "set", vec![s(&key), s("valor123")]);
    assert!(as_bool(call("env", "has", vec![s(&key)])), "has tras set");
    assert_eq!(as_str(call("env", "get", vec![s(&key)])), "valor123");
}

#[test]
fn smoke_state_shared_store() {
    // El módulo `state` es el estado compartido thread-safe para servidores.
    // (Store global: usamos claves propias para no chocar con otros tests.)
    call("state", "set", vec![s("smoke_k"), EvalValue::Int(0)]);
    assert_eq!(as_int(call("state", "get", vec![s("smoke_k")])), 0);

    // incr atómico: get-modify-set bajo un solo lock.
    assert_eq!(as_int(call("state", "incr", vec![s("smoke_k")])), 1);
    assert_eq!(as_int(call("state", "incr", vec![s("smoke_k")])), 2);
    assert_eq!(as_int(call("state", "incr", vec![s("smoke_k"), EvalValue::Int(10)])), 12);
    assert_eq!(as_int(call("state", "decr", vec![s("smoke_k"), EvalValue::Int(2)])), 10);

    assert!(as_bool(call("state", "has", vec![s("smoke_k")])));
    // get con default cuando la clave no existe
    assert_eq!(as_str(call("state", "get", vec![s("smoke_falta"), s("n/a")])), "n/a");

    assert!(as_bool(call("state", "delete", vec![s("smoke_k")])));
    assert!(!as_bool(call("state", "has", vec![s("smoke_k")])));
}
