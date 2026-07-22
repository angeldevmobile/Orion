use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;
use rusqlite::{Connection, types::Value as SqlValue};
use base64::Engine as _;
use std::sync::{Arc, Mutex, OnceLock};
use std::collections::HashMap as StdHashMap;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    // El primer argumento identifica la base: una URL postgres:// va al backend
    // Postgres; cualquier otra cosa es una ruta de archivo SQLite. La API (query/
    // uno/ejecutar/insertar/transaccion/tablas/cerrar) es idéntica en ambos.
    if args.first().map(|v| is_pg_url(&to_str(v))).unwrap_or(false) {
        return pg::call(function, args);
    }
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

fn is_pg_url(s: &str) -> bool {
    s.starts_with("postgres://") || s.starts_with("postgresql://")
}

//    Backend Postgres
//
// Misma API que SQLite pero contra un servidor Postgres (cliente-servidor). El
// primer argumento es una URL postgres://user:pass@host:puerto/base. Igual que
// en SQLite mantenemos un pool de una conexión persistente por URL. Los `?` del
// SQL se traducen a `$1..$n` (estilo Postgres) para que el código del dev no
// cambie entre motores.

mod pg {
    use super::{EvalValue, extract_params, to_str};
    use indexmap::IndexMap;
    use postgres::{Client, NoTls, Row};
    use postgres::types::{Type, ToSql};
    use std::collections::HashMap as StdHashMap;
    use std::sync::{Arc, Mutex, OnceLock};

    static POOL: OnceLock<Mutex<StdHashMap<String, Arc<Mutex<Client>>>>> = OnceLock::new();

    fn pool() -> &'static Mutex<StdHashMap<String, Arc<Mutex<Client>>>> {
        POOL.get_or_init(|| Mutex::new(StdHashMap::new()))
    }

    fn client_for(url: &str) -> Result<Arc<Mutex<Client>>, String> {
        let mut guard = pool().lock().unwrap();
        if let Some(c) = guard.get(url) {
            return Ok(c.clone());
        }
        let client = Client::connect(url, NoTls)
            .map_err(|e| format!("db(postgres): no se pudo conectar: {}", e))?;
        let arc = Arc::new(Mutex::new(client));
        guard.insert(url.to_string(), arc.clone());
        Ok(arc)
    }

    pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
        match function {
            "query" | "consulta" => {
                if args.len() < 2 { return Err("db.query requiere (url, sql, params?)".into()); }
                let sql    = translate(&to_str(&args[1]));
                let params = extract_params(args.get(2));
                with_client(&to_str(&args[0]), |c| run_query(c, &sql, &params))
            }
            "uno" | "first" => {
                if args.len() < 2 { return Err("db.uno requiere (url, sql, params?)".into()); }
                let sql    = translate(&to_str(&args[1]));
                let params = extract_params(args.get(2));
                with_client(&to_str(&args[0]), |c| {
                    if let EvalValue::List(mut l) = run_query(c, &sql, &params)? {
                        Ok(l.drain(..).next().unwrap_or(EvalValue::Null))
                    } else { Ok(EvalValue::Null) }
                })
            }
            "ejecutar" | "exec" => {
                if args.len() < 2 { return Err("db.ejecutar requiere (url, sql, params?)".into()); }
                let sql    = translate(&to_str(&args[1]));
                let params = extract_params(args.get(2));
                with_client(&to_str(&args[0]), |c| {
                    let boxed = box_params(&params);
                    let refs  = as_refs(&boxed);
                    let n = c.execute(sql.as_str(), &refs)
                        .map_err(|e| format!("db.ejecutar(postgres): {}", e))?;
                    Ok(EvalValue::Int(n as i64))
                })
            }
            // insertar: en Postgres no hay last_insert_rowid. Se espera un
            // `... RETURNING id`: devolvemos el primer valor de la fila devuelta.
            "insertar" | "insert" => {
                if args.len() < 2 { return Err("db.insertar requiere (url, sql RETURNING id, params?)".into()); }
                let sql    = translate(&to_str(&args[1]));
                let params = extract_params(args.get(2));
                with_client(&to_str(&args[0]), |c| {
                    let boxed = box_params(&params);
                    let refs  = as_refs(&boxed);
                    let rows = c.query(sql.as_str(), &refs)
                        .map_err(|e| format!("db.insertar(postgres): {} — ¿falta 'RETURNING id'?", e))?;
                    match rows.first() {
                        Some(r) if !r.is_empty() => Ok(pg_cell(r, 0)),
                        _ => Ok(EvalValue::Null),
                    }
                })
            }
            "transaccion" | "transaction" => {
                if args.len() < 2 { return Err("db.transaccion requiere (url, [sqls])".into()); }
                let pasos = match &args[1] {
                    EvalValue::List(l) => l.clone(),
                    _ => return Err("db.transaccion: segundo arg debe ser lista de SQL".into()),
                };
                with_client(&to_str(&args[0]), |c| {
                    let mut tx = c.transaction()
                        .map_err(|e| format!("db.transaccion(postgres): {}", e))?;
                    for paso in &pasos {
                        let (raw, params) = match paso {
                            EvalValue::List(par) if par.len() >= 2 => (to_str(&par[0]), extract_params(par.get(1))),
                            otro => (to_str(otro), vec![]),
                        };
                        let sql   = translate(&raw);
                        let boxed = box_params(&params);
                        let refs  = as_refs(&boxed);
                        tx.execute(sql.as_str(), &refs)
                            .map_err(|e| format!("db.transaccion(postgres) '{}': {}", sql, e))?;
                    }
                    tx.commit().map_err(|e| format!("db.transaccion commit(postgres): {}", e))?;
                    Ok(EvalValue::Bool(true))
                })
            }
            "tablas" | "tables" => {
                if args.is_empty() { return Err("db.tablas requiere (url)".into()); }
                with_client(&to_str(&args[0]), |c| {
                    let rows = c.query(
                        "SELECT tablename FROM pg_catalog.pg_tables \
                         WHERE schemaname NOT IN ('pg_catalog','information_schema') \
                         ORDER BY tablename", &[])
                        .map_err(|e| format!("db.tablas(postgres): {}", e))?;
                    Ok(EvalValue::List(rows.iter().map(|r| pg_cell(r, 0)).collect()))
                })
            }
            "cerrar" | "close" => {
                if args.is_empty() { return Err("db.cerrar requiere (url)".into()); }
                let removed = pool().lock().unwrap().remove(&to_str(&args[0])).is_some();
                Ok(EvalValue::Bool(removed))
            }
            f => Err(format!("db.{}() no existe para Postgres", f)),
        }
    }

    fn with_client<T>(url: &str, f: impl FnOnce(&mut Client) -> Result<T, String>) -> Result<T, String> {
        let arc = client_for(url)?;
        let mut c = arc.lock().unwrap();
        f(&mut c)
    }

    fn run_query(c: &mut Client, sql: &str, params: &[EvalValue]) -> Result<EvalValue, String> {
        let boxed = box_params(params);
        let refs  = as_refs(&boxed);
        let rows = c.query(sql, &refs)
            .map_err(|e| format!("db.query(postgres): {}", e))?;
        let out = rows.iter().map(|row| {
            let mut m = IndexMap::new();
            for (i, col) in row.columns().iter().enumerate() {
                m.insert(col.name().to_string(), pg_cell(row, i));
            }
            EvalValue::Dict(m)
        }).collect();
        Ok(EvalValue::List(out))
    }

    /// Traduce placeholders `?` → `$1..$n`, respetando literales entre comillas.
    fn translate(sql: &str) -> String {
        let mut out = String::with_capacity(sql.len() + 8);
        let mut n = 0;
        let mut in_str = false;
        for ch in sql.chars() {
            match ch {
                '\'' => { in_str = !in_str; out.push(ch); }
                '?' if !in_str => { n += 1; out.push('$'); out.push_str(&n.to_string()); }
                _ => out.push(ch),
            }
        }
        out
    }

    /// Adaptador de parámetro: codifica un EvalValue según el tipo que la
    /// columna espera (`ty`, que el driver conoce al preparar la sentencia).
    /// Así un número Orion entra correctamente en NUMERIC/INT/FLOAT sin que el
    /// dev tenga que castear, y List/Dict viajan como JSON(b).
    #[derive(Debug)]
    struct Param(EvalValue);

    impl ToSql for Param {
        fn to_sql(
            &self,
            ty: &Type,
            out: &mut bytes::BytesMut,
        ) -> Result<postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
            use rust_decimal::Decimal;
            use rust_decimal::prelude::FromPrimitive;
            match &self.0 {
                EvalValue::Null => Ok(postgres::types::IsNull::Yes),
                EvalValue::Int(n) => match *ty {
                    Type::INT2    => (*n as i16).to_sql(ty, out),
                    Type::INT4    => (*n as i32).to_sql(ty, out),
                    Type::FLOAT4  => (*n as f32).to_sql(ty, out),
                    Type::FLOAT8  => (*n as f64).to_sql(ty, out),
                    Type::NUMERIC => Decimal::from_i64(*n).unwrap_or_default().to_sql(ty, out),
                    Type::BOOL    => (*n != 0).to_sql(ty, out),
                    Type::TEXT | Type::VARCHAR => n.to_string().to_sql(ty, out),
                    _             => n.to_sql(ty, out),
                },
                EvalValue::Float(f) => match *ty {
                    Type::NUMERIC => Decimal::from_f64(*f).unwrap_or_default().to_sql(ty, out),
                    Type::FLOAT4  => (*f as f32).to_sql(ty, out),
                    Type::INT2    => (*f as i16).to_sql(ty, out),
                    Type::INT4    => (*f as i32).to_sql(ty, out),
                    Type::INT8    => (*f as i64).to_sql(ty, out),
                    Type::TEXT | Type::VARCHAR => f.to_string().to_sql(ty, out),
                    _             => f.to_sql(ty, out),
                },
                EvalValue::Bool(b) => match *ty {
                    Type::TEXT | Type::VARCHAR => b.to_string().to_sql(ty, out),
                    _             => b.to_sql(ty, out),
                },
                EvalValue::Str(s) => s.to_sql(ty, out),
                other => crate::modules::json_mod::eval_to_json(other.clone()).to_sql(ty, out),
            }
        }

        fn accepts(_ty: &Type) -> bool { true }

        postgres::types::to_sql_checked!();
    }

    fn box_params(params: &[EvalValue]) -> Vec<Param> {
        params.iter().map(|v| Param(v.clone())).collect()
    }

    fn as_refs<'a>(boxed: &'a [Param]) -> Vec<&'a (dyn ToSql + Sync)> {
        boxed.iter().map(|p| p as &(dyn ToSql + Sync)).collect()
    }

    /// Extrae la celda i de una fila mapeando el tipo Postgres a EvalValue.
    fn pg_cell(row: &Row, i: usize) -> EvalValue {
        let ty = row.columns()[i].type_();
        match *ty {
            Type::BOOL => opt(row.try_get::<_, Option<bool>>(i)).map(EvalValue::Bool),
            Type::INT2 => opt(row.try_get::<_, Option<i16>>(i)).map(|n| EvalValue::Int(n as i64)),
            Type::INT4 => opt(row.try_get::<_, Option<i32>>(i)).map(|n| EvalValue::Int(n as i64)),
            Type::INT8 => opt(row.try_get::<_, Option<i64>>(i)).map(EvalValue::Int),
            Type::OID  => opt(row.try_get::<_, Option<u32>>(i)).map(|n| EvalValue::Int(n as i64)),
            Type::FLOAT4 => opt(row.try_get::<_, Option<f32>>(i)).map(|f| EvalValue::Float(f as f64)),
            Type::FLOAT8 => opt(row.try_get::<_, Option<f64>>(i)).map(EvalValue::Float),
            Type::NUMERIC => {
                use rust_decimal::prelude::ToPrimitive;
                opt(row.try_get::<_, Option<rust_decimal::Decimal>>(i))
                    .and_then(|d| d.to_f64().map(EvalValue::Float))
            }
            Type::TEXT | Type::VARCHAR | Type::NAME | Type::BPCHAR | Type::CHAR =>
                opt(row.try_get::<_, Option<String>>(i)).map(EvalValue::Str),
            Type::JSON | Type::JSONB =>
                opt(row.try_get::<_, Option<serde_json::Value>>(i))
                    .map(crate::modules::json_mod::json_to_eval),
            Type::UUID =>
                opt(row.try_get::<_, Option<uuid::Uuid>>(i)).map(|u| EvalValue::Str(u.to_string())),
            Type::TIMESTAMP =>
                opt(row.try_get::<_, Option<chrono::NaiveDateTime>>(i))
                    .map(|t| EvalValue::Str(t.format("%Y-%m-%dT%H:%M:%S%.f").to_string())),
            Type::TIMESTAMPTZ =>
                opt(row.try_get::<_, Option<chrono::DateTime<chrono::Utc>>>(i))
                    .map(|t| EvalValue::Str(t.to_rfc3339())),
            Type::DATE =>
                opt(row.try_get::<_, Option<chrono::NaiveDate>>(i))
                    .map(|d| EvalValue::Str(d.to_string())),
            Type::TIME =>
                opt(row.try_get::<_, Option<chrono::NaiveTime>>(i))
                    .map(|t| EvalValue::Str(t.to_string())),
            Type::BYTEA => {
                use base64::Engine as _;
                opt(row.try_get::<_, Option<Vec<u8>>>(i))
                    .map(|b| EvalValue::Str(base64::engine::general_purpose::STANDARD.encode(&b)))
            }
            // Tipo no mapeado: intentar texto; si tampoco, Null.
            _ => row.try_get::<_, Option<String>>(i).ok().flatten().map(EvalValue::Str),
        }.unwrap_or(EvalValue::Null)
    }

    /// Aplana un `Result<Option<T>, _>` de try_get a `Option<T>` (error → None).
    fn opt<T>(r: Result<Option<T>, postgres::Error>) -> Option<T> {
        r.ok().flatten()
    }
}
