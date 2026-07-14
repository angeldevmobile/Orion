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

// La clave AES se deriva con Argon2id + salt aleatorio, NO con SHA-256 plano.
// Verificamos: (1) formato v1 con salt, (2) dos cifrados del mismo texto+clave
// difieren (salt+nonce frescos), (3) sigue descifrando el formato legacy viejo.
#[test]
fn smoke_crypto2_aes_kdf_hardened() {
    // (1) el output empieza por el byte de versión 0x01 tras decodificar base64
    use base64::Engine as _;
    let enc = as_str(call("crypto2", "aes_encrypt", vec![s("x"), s("pw")]));
    let raw = base64::engine::general_purpose::STANDARD
        .decode(&enc).expect("base64 válido");
    assert_eq!(raw[0], 0x01, "el ciphertext debe llevar el marcador de versión v1");
    assert!(raw.len() >= 1 + 16 + 12, "debe incluir salt(16)+nonce(12)");

    // (2) mismo plaintext + misma clave ⇒ ciphertexts distintos (salt aleatorio)
    let a = as_str(call("crypto2", "aes_encrypt", vec![s("igual"), s("k")]));
    let b = as_str(call("crypto2", "aes_encrypt", vec![s("igual"), s("k")]));
    assert_ne!(a, b, "el salt/nonce aleatorio debe hacer cada cifrado único");

    // (3) compatibilidad: un dato del formato legacy (SHA-256, sin versión) se
    // sigue descifrando. Construimos uno con la derivación vieja.
    use aes_gcm::{Aes256Gcm, Key, Nonce, aead::{Aead, KeyInit}};
    use sha2::{Sha256, Digest};
    let legacy_key = Sha256::digest(b"klegacy");
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&legacy_key));
    let nonce = [7u8; 12];
    let ct = cipher.encrypt(Nonce::from_slice(&nonce), b"antiguo".as_ref()).unwrap();
    let mut blob = nonce.to_vec();
    blob.extend_from_slice(&ct);
    let legacy_b64 = base64::engine::general_purpose::STANDARD.encode(&blob);
    let dec = call("crypto2", "aes_decrypt", vec![s(&legacy_b64), s("klegacy")]);
    assert_eq!(as_str(dec), "antiguo", "debe descifrar el formato legacy");
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

// ── ai: lo que funciona SIN API key (estado, memoria, chat local) ────────────

#[test]
fn smoke_ai_offline() {
    // status() → dict estructurado (no string), con claves estables
    let st = call("ai", "status", vec![]);
    match st {
        EvalValue::Dict(m) => {
            assert!(matches!(m.get("configured"), Some(EvalValue::Bool(_))), "status.configured es Bool");
            assert!(matches!(m.get("provider"),   Some(EvalValue::Str(_))),  "status.provider es Str");
            assert!(matches!(m.get("model"),      Some(EvalValue::Str(_))),  "status.model es Str");
            assert!(matches!(m.get("memory"),     Some(EvalValue::Int(_))),  "status.memory es Int");
        }
        o => panic!("ai.status debe devolver Dict, fue {o:?}"),
    }
    // provider() → "anthropic" | "openai" | "none"
    let p = as_str(call("ai", "provider", vec![]));
    assert!(["anthropic", "openai", "none"].contains(&p.as_str()), "provider inesperado: {p}");
    // learn/memory_size/memory_clear (memoria de sesión, sin red)
    call("ai", "memory_clear", vec![]);
    call("ai", "learn", vec![s("orion es un lenguaje")]);
    assert_eq!(as_int(call("ai", "memory_size", vec![])), 1);
    call("ai", "memory_clear", vec![]);
    assert_eq!(as_int(call("ai", "memory_size", vec![])), 0);
    // chat_start guarda el system prompt de la sesión (bug: antes lo descartaba)
    call("ai", "chat_start", vec![s("responde en mayúsculas")]);
    call("ai", "chat_reset", vec![]);
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

// ── Regresión validación 2026-07-13 (lote no-cripto) ─────────────────────────

fn as_float(v: EvalValue) -> f64 {
    match v { EvalValue::Float(f) => f, EvalValue::Int(n) => n as f64, o => panic!("se esperaba número, fue {o:?}") }
}

// det pasó de cofactores O(n!) a eliminación gaussiana O(n³): 20×20 era
// inviable (~10⁹ años) y ahora es instantáneo. Verifica valores exactos.
#[test]
fn smoke_matrix_det_correct_and_fast() {
    assert_eq!(as_float(call("matrix", "det", vec![mat(&[&[1,2],&[3,4]])])), -2.0);
    assert_eq!(as_float(call("matrix", "det", vec![mat(&[&[6,1,1],&[4,-2,5],&[2,8,7]])])), -306.0);
    // singular → 0, no error
    assert_eq!(as_float(call("matrix", "det", vec![mat(&[&[1,2],&[2,4]])])), 0.0);
    // 20×20 identidad: solo termina si det es polinómico
    let n = 20usize;
    let rows: Vec<EvalValue> = (0..n).map(|r| {
        EvalValue::List((0..n).map(|c| i(if r == c { 1 } else { 0 })).collect())
    }).collect();
    let d = as_float(call("matrix", "det", vec![EvalValue::List(rows)]));
    assert!((d - 1.0).abs() < 1e-9, "det(I20) = {d}");
}

// Sin argumentos las funciones de matrix devolvían panic (crash de la VM);
// ahora deben devolver Err.
#[test]
fn smoke_matrix_no_args_is_err_not_panic() {
    for f in ["det", "inverse", "transpose", "trace", "shape", "flatten", "collapse"] {
        assert!(modules::call("matrix", f, vec![]).is_err(), "matrix.{f}() debe dar Err");
    }
    assert!(modules::call("matrix", "inverse", vec![mat(&[&[1,2],&[2,4]])]).is_err(), "inversa de singular");
}

// serie.free/count: las transformaciones acumulan handles en un mapa global;
// free permite soltarlos en procesos largos.
#[test]
fn smoke_serie_free_count() {
    // Los tests corren en paralelo y SERIES es global: no comparamos counts
    // exactos (otro test puede crear series entre medias), solo monotonicidad
    // y el contrato de free.
    let before = as_int(call("serie", "count", vec![]));
    let h = call("serie", "new", vec![flist(&[1.0, 2.0])]);
    assert!(as_int(call("serie", "count", vec![])) >= before + 1);
    assert!(as_bool(call("serie", "free", vec![h.clone()])), "free primera vez → yes");
    assert!(!as_bool(call("serie", "free", vec![h])), "free doble → no");
}

// csv.stats interpolaba percentiles por índice truncado (median de [1,2,3,4]
// daba 2); ahora interpola linealmente como serie. fs.rmdir ahora es
// idempotente: no-existe → Bool(no), no Err.
#[test]
fn smoke_csv_stats_interpolated_and_fs_rmdir_idempotent() {
    let rows = EvalValue::List(vec![
        dict(&[("v", i(1))]), dict(&[("v", i(2))]),
        dict(&[("v", i(3))]), dict(&[("v", i(4))]),
    ]);
    let st = call("csv", "stats", vec![rows, s("v")]);
    let EvalValue::Dict(m) = st else { panic!("stats debe ser dict") };
    assert_eq!(as_float(m.get("median").unwrap().clone()), 2.5);
    assert_eq!(as_float(m.get("p75").unwrap().clone()), 3.25);

    assert!(!as_bool(call("fs", "rmdir", vec![s("dir_que_jamas_existio_xyz")])));
}

// ── Regresión validación 2026-07-13 (lote HPC: stat/vector/grafo/quantum) ────

// quantum.qubit documentaba (a_re, a_im, b_re, b_im) pero IGNORABA los args
// (qubit(0,0,1,0) devolvía |0>). Ahora los honra, normaliza, y rechaza el
// estado nulo.
#[test]
fn smoke_quantum_qubit_honors_args() {
    let q = call("quantum", "qubit", vec![i(0), i(0), i(1), i(0)]);
    let one = call("quantum", "one", vec![]);
    let fid = as_float(call("quantum", "fidelity", vec![q, one]));
    assert_eq!(fid, 1.0, "qubit(0,0,1,0) debe ser |1>");
    // (3,0,4,0) normaliza a probs 0.36/0.64
    let q2 = call("quantum", "qubit", vec![i(3), i(0), i(4), i(0)]);
    let EvalValue::Dict(p) = call("quantum", "measure_probs", vec![q2]) else { panic!() };
    assert_eq!(as_float(p.get("1").unwrap().clone()), 0.64);
    assert!(modules::call("quantum", "qubit", vec![i(0), i(0), i(0), i(0)]).is_err(), "estado nulo");
}

// stat.correlation devolvía 0.9999999999999998 para correlación perfecta
// (ruido f64); ahora redondea a 1e-12 como quantum/vector.
#[test]
fn smoke_stat_correlation_rounded() {
    let x = flist(&[1.0, 2.0, 3.0]);
    let y = flist(&[2.0, 4.0, 6.0]);
    assert_eq!(as_float(call("stat", "correlation", vec![x.clone(), y])), 1.0);
    let y_inv = flist(&[6.0, 4.0, 2.0]);
    assert_eq!(as_float(call("stat", "correlation", vec![x, y_inv])), -1.0);
}

// grafo: camino mínimo respeta pesos (a→b→c coste 2 gana a a→c coste 5)
// y el grafo es dirigido (sin retorno).
#[test]
fn smoke_grafo_weighted_directed_path() {
    let g = call("grafo", "create", vec![]);
    for (from, to, w) in [("a", "b", 1), ("b", "c", 1), ("a", "c", 5)] {
        call("grafo", "edge", vec![g.clone(), s(from), s(to), i(w)]);
    }
    let EvalValue::List(p) = call("grafo", "path", vec![g.clone(), s("a"), s("c")]) else { panic!() };
    assert_eq!(p.len(), 3, "debe ir por b (coste 2), no directo (coste 5)");
    assert!(matches!(call("grafo", "path", vec![g.clone(), s("c"), s("a")]), EvalValue::Null), "dirigido");
    call("grafo", "delete", vec![g]);
}
