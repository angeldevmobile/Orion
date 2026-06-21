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
