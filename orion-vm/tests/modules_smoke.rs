//! Smoke tests de la stdlib (sweep de cobertura, 2026-06-26).
//!
//! La biblioteca estándar (~56 módulos) es el diferenciador de Orion. Estos
//! smoke tests verifican vía `modules::call` qué módulos funcionan de verdad,
//! para anunciar solo lo verificado. ~43 módulos probados offline con salida
//! real; los externos (net/s3/ssh/docker/mail/ws/llm/ai/vision) se verifican
//! "cableados" (despachan y validan args) sin disparar I/O. Ver ESTADO_MODULOS.md.

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

// ════════════════════════════════════════════════════════════════════════
//  Sweep de cobertura (2026-06-26): mapear qué módulos funcionan de verdad
//  para anunciar solo lo verificado. Helpers:
// ════════════════════════════════════════════════════════════════════════

fn ok(m: &str, f: &str, args: Vec<EvalValue>) -> bool { modules::call(m, f, args).is_ok() }
fn i(n: i64) -> EvalValue { EvalValue::Int(n) }
fn ilist(xs: &[i64]) -> EvalValue { EvalValue::List(xs.iter().map(|&x| EvalValue::Int(x)).collect()) }
fn flist(xs: &[f64]) -> EvalValue { EvalValue::List(xs.iter().map(|&x| EvalValue::Float(x)).collect()) }
fn mat(rows: &[&[i64]]) -> EvalValue {
    EvalValue::List(rows.iter().map(|r| ilist(r)).collect())
}
fn dict(pairs: &[(&str, EvalValue)]) -> EvalValue {
    let mut m = HashMap::new();
    for (k, v) in pairs { m.insert(k.to_string(), v.clone()); }
    EvalValue::Dict(m)
}

#[test]
fn smoke_crypto() {
    assert!(matches!(call("crypto", "hash", vec![s("orion")]), EvalValue::Str(_)));
    let h = as_str(call("crypto", "sha256", vec![s("orion")]));
    assert_eq!(h.len(), 64, "sha256 debe ser 64 hex chars");
}

#[test]
fn smoke_crypto2_aes_roundtrip() {
    let key = s("0123456789abcdef0123456789abcdef");
    let enc = call("crypto2", "aes_encrypt", vec![s("secreto"), key.clone()]);
    let cipher = match enc { EvalValue::Str(c) => c, o => panic!("aes_encrypt: {o:?}") };
    let dec = call("crypto2", "aes_decrypt", vec![s(&cipher), key]);
    assert_eq!(as_str(dec), "secreto");
}

#[test]
fn smoke_matrix() {
    assert!(ok("matrix", "add", vec![mat(&[&[1,2],&[3,4]]), mat(&[&[1,1],&[1,1]])]));
    assert!(ok("matrix", "mul", vec![mat(&[&[1,2],&[3,4]]), mat(&[&[1,0],&[0,1]])]));
    assert!(ok("matrix", "transpose", vec![mat(&[&[1,2,3]])]));
}

#[test]
fn smoke_validate() {
    assert!(as_bool(call("validate", "email", vec![s("a@b.com")])));
    assert!(!as_bool(call("validate", "email", vec![s("noesmail")])));
    assert!(as_bool(call("validate", "length", vec![s("abc"), i(1), i(5)])));
}

#[test]
fn smoke_random() {
    let n = as_int(call("random", "int", vec![i(1), i(10)]));
    assert!((1..=10).contains(&n), "random.int fuera de rango: {n}");
    assert!(ok("random", "choice", vec![ilist(&[1,2,3])]));
    assert!(ok("random", "shuffle", vec![ilist(&[1,2,3,4])]));
}

#[test]
fn smoke_zip_gzip_roundtrip() {
    let mut base = std::env::temp_dir();
    base.push(format!("orion_smoke_zip_{}", std::process::id()));
    let src = format!("{}.txt", base.to_str().unwrap());
    let gz  = format!("{}.gz", base.to_str().unwrap());
    let out = format!("{}.out", base.to_str().unwrap());
    std::fs::write(&src, "contenido orion para comprimir").unwrap();
    assert!(ok("zip", "gzip",   vec![s(&src), s(&gz)]),  "zip.gzip falló");
    assert!(ok("zip", "gunzip", vec![s(&gz),  s(&out)]), "zip.gunzip falló");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "contenido orion para comprimir");
    for f in [&src, &gz, &out] { let _ = std::fs::remove_file(f); }
}

#[test]
fn smoke_vector_store() {
    let h = call("vector", "new", vec![]);
    let handle = match h { EvalValue::Str(s) => s, o => panic!("vector.new: {o:?}") };
    assert!(ok("vector", "add", vec![s(&handle), s("a"), flist(&[1.0, 0.0, 0.0])]));
    assert!(ok("vector", "add", vec![s(&handle), s("b"), flist(&[0.0, 1.0, 0.0])]));
    let res = call("vector", "search", vec![s(&handle), flist(&[1.0, 0.0, 0.0]), i(1)]);
    assert!(list_len(res) >= 1, "vector.search debe devolver resultados");
}

#[test]
fn smoke_auth_jwt() {
    let h = as_str(call("auth", "hash", vec![s("clave123")]));
    assert!(as_bool(call("auth", "verify", vec![s("clave123"), s(&h)])), "auth verify roundtrip");
    let tok = call("auth", "token", vec![dict(&[("uid", i(7))]), s("secreto")]);
    assert!(matches!(tok, EvalValue::Str(_)), "auth.token debe ser string");
}

#[test]
fn smoke_cola_queue() {
    call("cola", "create", vec![s("q1")]);
    call("cola", "push", vec![s("q1"), s("primero")]);
    call("cola", "push", vec![s("q1"), s("segundo")]);
    assert_eq!(as_str(call("cola", "pop", vec![s("q1")])), "primero", "FIFO");
    assert_eq!(as_int(call("cola", "size", vec![s("q1")])), 1);
}

#[test]
fn smoke_stat_correlation() {
    let r = call("stat", "correlation", vec![flist(&[1.0,2.0,3.0]), flist(&[2.0,4.0,6.0])]);
    assert!(matches!(r, EvalValue::Float(_) | EvalValue::Int(_)), "correlation numérica");
}

#[test]
fn smoke_proto_roundtrip() {
    let d = dict(&[("n", i(42)), ("nombre", s("orion"))]);
    let enc = call("proto", "encode", vec![d]);
    assert!(ok("proto", "decode", vec![enc]), "proto decode del encode");
}

#[test]
fn smoke_formato() {
    assert!(matches!(call("formato", "centrar", vec![s("hi"), i(10)]), EvalValue::Str(_)));
    assert!(matches!(call("formato", "separador", vec![i(10)]), EvalValue::Str(_)));
}

#[test]
fn smoke_template() {
    let vars = dict(&[("nombre", s("Orion"))]);
    let r = call("template", "render", vec![s("Hola {{nombre}}"), vars]);
    assert_eq!(as_str(r), "Hola Orion");
}

#[test]
fn smoke_csv_roundtrip() {
    let mut p = std::env::temp_dir();
    p.push(format!("orion_smoke_csv_{}.csv", std::process::id()));
    let path = p.to_str().unwrap().to_string();
    let rows = EvalValue::List(vec![
        dict(&[("id", i(1)), ("nombre", s("a"))]),
        dict(&[("id", i(2)), ("nombre", s("b"))]),
    ]);
    assert!(ok("csv", "write", vec![s(&path), rows]), "csv.write");
    let read = call("csv", "read", vec![s(&path)]);
    assert_eq!(list_len(read), 2, "csv.read debe ver 2 filas");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn smoke_timewarp_tarea() {
    assert!(matches!(call("timewarp", "timestamp", vec![]), EvalValue::Int(_)));
    assert!(matches!(call("tarea", "now", vec![]), EvalValue::Int(_) | EvalValue::Str(_)));
}

#[test]
fn smoke_log() {
    assert!(ok("log", "info", vec![s("mensaje de prueba")]));
}

#[test]
fn smoke_secret_mask() {
    assert!(matches!(call("secret", "mask", vec![s("supersecreto")]), EvalValue::Str(_)));
}

#[test]
fn smoke_embed_math() {
    let r = call("embed", "similarity", vec![flist(&[1.0,0.0]), flist(&[1.0,0.0])]);
    assert!(matches!(r, EvalValue::Float(_) | EvalValue::Int(_)), "embed.similarity numérica");
}

// Verifica que un módulo/función está CABLEADO (despacha) sin disparar I/O:
// llamar con args vacíos debe dar error de validación, no "no encontrado".
fn wired(m: &str, f: &str) -> bool {
    match modules::call(m, f, vec![]) {
        Ok(_)  => true,
        Err(e) => !e.contains("no encontrado") && !e.contains("no existe"),
    }
}

#[test]
fn smoke_cache() {
    call("cache", "set", vec![s("ck"), s("cv")]);
    assert_eq!(as_str(call("cache", "get", vec![s("ck")])), "cv");
}

#[test]
fn smoke_config_get() {
    let cfg = dict(&[("puerto", i(8080))]);
    assert_eq!(as_int(call("config", "get", vec![cfg, s("puerto")])), 8080);
}

#[test]
fn smoke_grafo() {
    let g = as_int(call("grafo", "create", vec![])); // handle numérico
    assert!(ok("grafo", "node", vec![i(g), s("A")]));
    assert!(ok("grafo", "node", vec![i(g), s("B")]));
    assert!(ok("grafo", "edge", vec![i(g), s("A"), s("B")]));
}

#[test]
fn smoke_quantum() {
    assert!(ok("quantum", "zero", vec![]));
    assert!(ok("quantum", "bell", vec![]));
}

#[test]
fn smoke_router() {
    let r = as_int(call("router", "new", vec![])); // handle numérico
    assert!(ok("router", "get", vec![i(r), s("/health"), s("handler")]));
}

#[test]
fn smoke_sse() {
    assert!(matches!(call("sse", "event", vec![s("hola")]), EvalValue::Str(_)));
    assert!(matches!(call("sse", "named", vec![s("ping"), s("1")]), EvalValue::Str(_)));
}

#[test]
fn smoke_stream() {
    assert!(ok("stream", "range", vec![i(1), i(5)]));
    assert!(ok("stream", "from", vec![ilist(&[1,2,3])]));
}

#[test]
fn smoke_middleware() {
    assert!(ok("middleware", "rate_limit", vec![i(5), i(60)]));
}

#[test]
fn smoke_pdf() {
    let mut p = std::env::temp_dir();
    p.push(format!("orion_smoke_{}.pdf", std::process::id()));
    let path = p.to_str().unwrap().to_string();
    assert!(ok("pdf", "create", vec![s(&path), s("Reporte Orion")]), "pdf.create");
    assert!(std::path::Path::new(&path).exists(), "el PDF debió generarse");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn smoke_watch_stat() {
    let mut p = std::env::temp_dir();
    p.push(format!("orion_smoke_watch_{}.txt", std::process::id()));
    let path = p.to_str().unwrap().to_string();
    std::fs::write(&path, "x").unwrap();
    assert!(ok("watch", "stat", vec![s(&path)]), "watch.stat");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn smoke_excel_f() {
    // funciones financieras/aritméticas sobre números
    assert!(wired("excel_f", "ratio"), "excel_f.ratio cableado");
    assert!(wired("excel_f", "sum"), "excel_f.sum cableado");
}

// ── Módulos externos (necesitan servicio): solo verificamos CABLEADO ──────────
// No disparan I/O real; confirman que el módulo/función existe y valida args.

#[test]
fn smoke_external_modules_wired() {
    for (m, f) in [
        ("net", "get"), ("net", "post"),
        ("mail", "send"), ("s3", "upload"), ("s3", "download"),
        ("ssh", "connect"), ("ssh", "exec"),
        ("docker", "containers"), ("docker", "start"),
        ("ws", "connect"), ("ws", "send"),
        ("llm", "query"), ("llm", "chat"),
        ("ai", "ask"), ("ai", "think"),
        ("vision", "info"), ("vision", "resize"),
        ("insight", "analyze"),
        ("search", "csv"),
    ] {
        assert!(wired(m, f), "módulo externo '{m}.{f}' NO está cableado");
    }
}

#[test]
fn smoke_cosmos() {
    assert!(ok("cosmos", "star", vec![i(3)]), "cosmos.star genera estrellas");
    assert!(ok("cosmos", "universe", vec![]), "cosmos.universe crea simulación");
}

#[test]
fn smoke_table_from() {
    let rows = EvalValue::List(vec![
        dict(&[("a", i(1))]), dict(&[("a", i(2))]),
    ]);
    assert!(ok("table", "from", vec![rows]), "table.from con lista de dicts");
}

#[test]
fn smoke_serie() {
    let h = call("serie", "new", vec![flist(&[1.0, 2.0, 3.0, 4.0])]);
    assert!(matches!(h, EvalValue::Str(_) | EvalValue::Int(_) | EvalValue::Dict(_)), "serie.new crea serie");
}

#[test]
fn smoke_frame() {
    let rows = EvalValue::List(vec![
        dict(&[("x", i(10))]), dict(&[("x", i(20))]),
    ]);
    assert!(ok("frame", "from_list", vec![rows]), "frame.from_list");
}

// ── GUI headless: el árbol de widgets se construye sin abrir ventana ─────────
// (No se llama gui.run — eso abriría eframe. Verificamos que cada widget y el
// tema despachan vía modules::call.)

#[test]
fn smoke_gui_headless_widgets() {
    // Tema configurable por el developer
    assert!(ok("gui", "theme", vec![dict(&[("accent", s("#ff7a00")), ("rounding", i(14)), ("light", EvalValue::Bool(false))])]));
    assert!(ok("gui", "panel", vec![s("Test"), i(420), i(320)]));

    // Tipografía con color/size por widget
    assert!(ok("gui", "heading", vec![s("Título"), s("accent")]));
    assert!(ok("gui", "text", vec![s("cuerpo"), dict(&[("color", s("success")), ("size", i(18))])]));
    assert!(ok("gui", "caption", vec![s("CAP")]));

    // Contenedor + widgets internos
    assert!(ok("gui", "card", vec![]));
    assert!(ok("gui", "field", vec![s("escribe…")]));
    assert!(ok("gui", "toggle", vec![s("activo")]));
    assert!(ok("gui", "pick", vec![s("cat"), EvalValue::List(vec![s("A"), s("B")])]));
    assert!(ok("gui", "press", vec![s("OK"), s("accent"), s("white")]));
    assert!(ok("gui", "ghost", vec![s("Cancelar")]));
    assert!(ok("gui", "end", vec![]));

    // Widgets nuevos
    assert!(ok("gui", "progress", vec![i(50)]));
    assert!(ok("gui", "progress", vec![EvalValue::Float(0.3), s("success")]));
    assert!(ok("gui", "tabs", vec![EvalValue::List(vec![s("Uno"), s("Dos")]), s("Uno")]));
    assert!(ok("gui", "image", vec![s("demo/logo.png"), i(120)]));
    assert!(ok("gui", "badge", vec![s("nuevo"), s("success")]));
    assert!(ok("gui", "divider", vec![]));

    // Zona con estilo completo por dict (borde/rounding/pad)
    assert!(ok("gui", "zone", vec![dict(&[("border", s("accent")), ("border_w", i(2)), ("rounding", i(16)), ("pad", i(20))])]));
    assert!(ok("gui", "heading", vec![s("dentro de zona")]));
    assert!(ok("gui", "end", vec![]));

    // Modal (contenedor)
    assert!(ok("gui", "modal", vec![s("Confirmar")]));
    assert!(ok("gui", "text", vec![s("¿seguro?")]));
    assert!(ok("gui", "end", vec![]));

    // Chart sin abrir ventana
    let datos = EvalValue::List(vec![
        dict(&[("mes", s("Ene")), ("v", i(10))]),
        dict(&[("mes", s("Feb")), ("v", i(20))]),
    ]);
    assert!(ok("gui", "chart", vec![datos, s("bar"), dict(&[("x", s("mes")), ("y", s("v"))])]));
}
