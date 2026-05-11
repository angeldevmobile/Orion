use crate::eval_value::EvalValue;
use std::collections::HashMap;
use rusqlite::{Connection, types::Value as SqlValue};
use base64::Engine as _;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // query(path, sql, params?) → List<Dict>
        "query" | "consulta" => {
            if args.len() < 2 { return Err("db.query requiere (path, sql, params?)".into()); }
            let conn   = open(&to_str(&args[0]))?;
            let sql    = to_str(&args[1]);
            let params = extract_params(args.get(2));
            run_query(&conn, &sql, params)
        }
        // uno(path, sql, params?) → Dict o Null  — primer resultado
        "uno" | "first" => {
            if args.len() < 2 { return Err("db.uno requiere (path, sql, params?)".into()); }
            let conn   = open(&to_str(&args[0]))?;
            let sql    = to_str(&args[1]);
            let params = extract_params(args.get(2));
            if let EvalValue::List(mut list) = run_query(&conn, &sql, params)? {
                Ok(list.drain(..).next().unwrap_or(EvalValue::Null))
            } else {
                Ok(EvalValue::Null)
            }
        }
        // ejecutar(path, sql, params?) → Int (filas afectadas)
        "ejecutar" | "exec" => {
            if args.len() < 2 { return Err("db.ejecutar requiere (path, sql, params?)".into()); }
            let conn   = open(&to_str(&args[0]))?;
            let sql    = to_str(&args[1]);
            let params = extract_params(args.get(2));
            run_exec(&conn, &sql, params)
        }
        // transaccion(path, [sql, ...]) → Bool
        "transaccion" | "transaction" => {
            if args.len() < 2 { return Err("db.transaccion requiere (path, [sqls])".into()); }
            let sqls = match &args[1] {
                EvalValue::List(l) => l.iter().map(|v| to_str(v)).collect::<Vec<_>>(),
                _ => return Err("db.transaccion: segundo arg debe ser lista de SQL".into()),
            };
            let mut conn = open(&to_str(&args[0]))?;
            let tx = conn.transaction().map_err(|e| format!("db.transaccion: {}", e))?;
            for sql in &sqls {
                tx.execute(sql, []).map_err(|e| format!("db.transaccion '{}': {}", sql, e))?;
            }
            tx.commit().map_err(|e| format!("db.transaccion commit: {}", e))?;
            Ok(EvalValue::Bool(true))
        }
        // tablas(path) → List<Str>
        "tablas" | "tables" => {
            let conn = open(&one_str("db.tablas", &args)?)?;
            let rows = run_query(
                &conn,
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
                vec![],
            )?;
            if let EvalValue::List(list) = rows {
                Ok(EvalValue::List(list.into_iter().filter_map(|r| {
                    if let EvalValue::Dict(mut m) = r { m.remove("name") } else { None }
                }).collect()))
            } else {
                Ok(EvalValue::List(vec![]))
            }
        }
        f => Err(format!("db.{}() no existe", f)),
    }
}

fn open(path: &str) -> Result<Connection, String> {
    Connection::open(path).map_err(|e| format!("db: no se pudo abrir '{}': {}", path, e))
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
