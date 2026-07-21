use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;
use rusqlite::{Connection, types::Value as SqlValue};
use base64::Engine as _;
use std::sync::{Arc, Mutex, OnceLock};
use std::collections::HashMap as StdHashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // query(path, sql, params?) → List<Dict>
        "query" | "consulta" => {
            if args.len() < 2 { return Err("db.query requiere (path, sql, params?)".into()); }
            let sql    = to_str(&args[1]);
            let params = extract_params(args.get(2));
            with_conn(&to_str(&args[0]), |conn| run_query(conn, &sql, params))
        }
        // uno(path, sql, params?) → Dict o Null  — primer resultado
        "uno" | "first" => {
            if args.len() < 2 { return Err("db.uno requiere (path, sql, params?)".into()); }
            let sql    = to_str(&args[1]);
            let params = extract_params(args.get(2));
            with_conn(&to_str(&args[0]), |conn| {
                if let EvalValue::List(mut list) = run_query(conn, &sql, params)? {
                    Ok(list.drain(..).next().unwrap_or(EvalValue::Null))
                } else {
                    Ok(EvalValue::Null)
                }
            })
        }
        // ejecutar(path, sql, params?) → Int (filas afectadas)
        "ejecutar" | "exec" => {
            if args.len() < 2 { return Err("db.ejecutar requiere (path, sql, params?)".into()); }
            let sql    = to_str(&args[1]);
            let params = extract_params(args.get(2));
            with_conn(&to_str(&args[0]), |conn| run_exec(conn, &sql, params))
        }
        // insertar(path, sql, params?) → Int (rowid de la fila insertada)
        // Azúcar sobre INSERT: devuelve el last_insert_rowid, no las filas.
        "insertar" | "insert" => {
            if args.len() < 2 { return Err("db.insertar requiere (path, sql, params?)".into()); }
            let sql    = to_str(&args[1]);
            let params = extract_params(args.get(2));
            with_conn(&to_str(&args[0]), |conn| {
                let sql_params: Vec<SqlValue> = params.iter().map(eval_to_sql).collect();
                conn.execute(&sql, rusqlite::params_from_iter(sql_params.iter()))
                    .map_err(|e| format!("db.insertar: {}", e))?;
                Ok(EvalValue::Int(conn.last_insert_rowid()))
            })
        }
        // transaccion(path, [sql, ...]) → Bool  — lista de SQL en una transacción.
        // Cada elemento puede ser "SQL" o [ "SQL", [params...] ].
        "transaccion" | "transaction" => {
            if args.len() < 2 { return Err("db.transaccion requiere (path, [sqls])".into()); }
            let pasos = match &args[1] {
                EvalValue::List(l) => l.clone(),
                _ => return Err("db.transaccion: segundo arg debe ser lista de SQL".into()),
            };
            with_conn_mut(&to_str(&args[0]), |conn| {
                let tx = conn.transaction().map_err(|e| format!("db.transaccion: {}", e))?;
                for paso in &pasos {
                    let (sql, params) = match paso {
                        EvalValue::List(par) if par.len() >= 2 => {
                            (to_str(&par[0]), extract_params(par.get(1)))
                        }
                        otro => (to_str(otro), vec![]),
                    };
                    let sql_params: Vec<SqlValue> = params.iter().map(eval_to_sql).collect();
                    tx.execute(&sql, rusqlite::params_from_iter(sql_params.iter()))
                        .map_err(|e| format!("db.transaccion '{}': {}", sql, e))?;
                }
                tx.commit().map_err(|e| format!("db.transaccion commit: {}", e))?;
                Ok(EvalValue::Bool(true))
            })
        }
        // tablas(path) → List<Str>
        "tablas" | "tables" => {
            with_conn(&one_str("db.tablas", &args)?, |conn| {
                let rows = run_query(
                    conn,
                    "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
                    vec![],
                )?;
                if let EvalValue::List(list) = rows {
                    Ok(EvalValue::List(list.into_iter().filter_map(|r| {
                        if let EvalValue::Dict(mut m) = r { m.shift_remove("name") } else { None }
                    }).collect()))
                } else {
                    Ok(EvalValue::List(vec![]))
                }
            })
        }
        // cerrar(path) → Bool  — descarta la conexión del pool (libera el archivo).
        "cerrar" | "close" => {
            if args.is_empty() { return Err("db.cerrar requiere (path)".into()); }
            let removed = pool().lock().unwrap().remove(&to_str(&args[0])).is_some();
            Ok(EvalValue::Bool(removed))
        }
        f => Err(format!("db.{}() no existe", f)),
    }
}

//    Pool de conexiones persistentes
//
// Reabrir SQLite en cada query costaba ms por llamada y borraba las bases
// `:memory:`. Ahora cada ruta guarda UNA conexión viva reutilizable (tras un
// Mutex para el pool de workers de serve). Las escrituras a un mismo archivo
// se serializan — el modelo de SQLite — pero WAL permite lectores concurrentes
// y busy_timeout evita el error "database is locked" bajo contención.

static POOL: OnceLock<Mutex<StdHashMap<String, Arc<Mutex<Connection>>>>> = OnceLock::new();

fn pool() -> &'static Mutex<StdHashMap<String, Arc<Mutex<Connection>>>> {
    POOL.get_or_init(|| Mutex::new(StdHashMap::new()))
}

/// Conexión viva para `path`, creándola con pragmas modernos la primera vez.
fn conn_for(path: &str) -> Result<Arc<Mutex<Connection>>, String> {
    let mut guard = pool().lock().unwrap();
    if let Some(c) = guard.get(path) {
        return Ok(c.clone());
    }
    let conn = Connection::open(path)
        .map_err(|e| format!("db: no se pudo abrir '{}': {}", path, e))?;
    // WAL solo aplica a bases en disco; en :memory: se ignora sin error.
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    let _ = conn.pragma_update(None, "synchronous", "NORMAL");
    let _ = conn.pragma_update(None, "foreign_keys", "ON");
    let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
    let arc = Arc::new(Mutex::new(conn));
    guard.insert(path.to_string(), arc.clone());
    Ok(arc)
}

fn with_conn<T>(path: &str, f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
    let arc = conn_for(path)?;
    let conn = arc.lock().unwrap();
    f(&conn)
}

fn with_conn_mut<T>(path: &str, f: impl FnOnce(&mut Connection) -> Result<T, String>) -> Result<T, String> {
    let arc = conn_for(path)?;
    let mut conn = arc.lock().unwrap();
    f(&mut conn)
}

fn run_query(conn: &Connection, sql: &str, params: Vec<EvalValue>) -> Result<EvalValue, String> {
    let sql_params: Vec<SqlValue> = params.iter().map(eval_to_sql).collect();
    let mut stmt = conn.prepare(sql).map_err(|e| format!("db.query prepare: {}", e))?;
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

    let rows = stmt.query_map(
        rusqlite::params_from_iter(sql_params.iter()),
        |row| {
            let mut m = HashMap::new();
            for (i, col) in col_names.iter().enumerate() {
                let val: SqlValue = row.get(i)?;
                m.insert(col.clone(), sql_to_eval(val));
            }
            Ok(EvalValue::Dict(m))
        },
    ).map_err(|e| format!("db.query: {}", e))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| format!("db.query row: {}", e))?);
    }
    Ok(EvalValue::List(result))
}

fn run_exec(conn: &Connection, sql: &str, params: Vec<EvalValue>) -> Result<EvalValue, String> {
    let sql_params: Vec<SqlValue> = params.iter().map(eval_to_sql).collect();
    let n = conn.execute(sql, rusqlite::params_from_iter(sql_params.iter()))
        .map_err(|e| format!("db.ejecutar: {}", e))?;
    Ok(EvalValue::Int(n as i64))
}

fn eval_to_sql(v: &EvalValue) -> SqlValue {
    match v {
        EvalValue::Int(n)   => SqlValue::Integer(*n),
        EvalValue::Float(f) => SqlValue::Real(*f),
        EvalValue::Bool(b)  => SqlValue::Integer(*b as i64),
        EvalValue::Null     => SqlValue::Null,
        other               => SqlValue::Text(format!("{}", other)),
    }
}

fn sql_to_eval(v: SqlValue) -> EvalValue {
    match v {
        SqlValue::Null       => EvalValue::Null,
        SqlValue::Integer(n) => EvalValue::Int(n),
        SqlValue::Real(f)    => EvalValue::Float(f),
        SqlValue::Text(s)    => EvalValue::Str(s),
        SqlValue::Blob(b)    => EvalValue::Str(
            base64::engine::general_purpose::STANDARD.encode(&b)
        ),
    }
}

fn extract_params(v: Option<&EvalValue>) -> Vec<EvalValue> {
    match v {
        Some(EvalValue::List(l)) => l.clone(),
        _ => vec![],
    }
}

fn one_str(fn_name: &str, args: &[EvalValue]) -> Result<String, String> {
    if args.is_empty() { return Err(format!("{} requiere (path)", fn_name)); }
    Ok(to_str(&args[0]))
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
