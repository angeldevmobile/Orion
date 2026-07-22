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
    if args.first().map(|v| is_mysql_url(&to_str(v))).unwrap_or(false) {
        return my::call(function, args);
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
        // copiar(path, tabla, [columnas], [[fila],…]) → Int  — carga masiva.
        // En SQLite se hace con una transacción + sentencia preparada reusada
        // (el equivalente rápido a COPY): una sola tx para miles de filas.
        "copiar" | "copy" => {
            if args.len() < 4 { return Err("db.copiar requiere (path, tabla, columnas, filas)".into()); }
            let tabla = to_str(&args[1]);
            let cols = match &args[2] {
                EvalValue::List(l) => l.iter().map(to_str).collect::<Vec<_>>(),
                _ => return Err("db.copiar: columnas debe ser una lista".into()),
            };
            let filas = match &args[3] {
                EvalValue::List(l) => l.clone(),
                _ => return Err("db.copiar: filas debe ser una lista de listas".into()),
            };
            let placeholders = vec!["?"; cols.len()].join(", ");
            let sql = format!("INSERT INTO {} ({}) VALUES ({})", tabla, cols.join(", "), placeholders);
            with_conn_mut(&to_str(&args[0]), |conn| {
                let tx = conn.transaction().map_err(|e| format!("db.copiar: {}", e))?;
                {
                    let mut stmt = tx.prepare(&sql).map_err(|e| format!("db.copiar prepare: {}", e))?;
                    for fila in &filas {
                        let campos = match fila {
                            EvalValue::List(f) => f,
                            _ => return Err("db.copiar: cada fila debe ser una lista".into()),
                        };
                        let sqlp: Vec<SqlValue> = campos.iter().map(eval_to_sql).collect();
                        stmt.execute(rusqlite::params_from_iter(sqlp.iter()))
                            .map_err(|e| format!("db.copiar fila: {}", e))?;
                    }
                }
                tx.commit().map_err(|e| format!("db.copiar commit: {}", e))?;
                Ok(EvalValue::Int(filas.len() as i64))
            })
        }
        // copiar_archivo(path, tabla, [columnas], ruta_csv, opts?) → Int
        // Carga en STREAMING con RAM constante: el crate csv lee fila a fila y
        // se insertan en una transacción con sentencia preparada. No materializa
        // el CSV en memoria — vale para archivos enormes.
        "copiar_archivo" | "copy_file" => {
            if args.len() < 4 { return Err("db.copiar_archivo requiere (path, tabla, columnas, ruta, opts?)".into()); }
            let tabla = to_str(&args[1]);
            let cols = match &args[2] {
                EvalValue::List(l) => l.iter().map(to_str).collect::<Vec<_>>(),
                _ => return Err("db.copiar_archivo: columnas debe ser una lista".into()),
            };
            let ruta = to_str(&args[3]);
            let (header, delim) = copy_file_opts(args.get(4));
            let placeholders = vec!["?"; cols.len()].join(", ");
            let sql = format!("INSERT INTO {} ({}) VALUES ({})", tabla, cols.join(", "), placeholders);
            with_conn_mut(&to_str(&args[0]), |conn| {
                let mut rdr = csv::ReaderBuilder::new()
                    .delimiter(delim)
                    .has_headers(header)
                    .flexible(true)
                    .from_path(&ruta)
                    .map_err(|e| format!("db.copiar_archivo: no se pudo abrir '{}': {}", ruta, e))?;
                let tx = conn.transaction().map_err(|e| format!("db.copiar_archivo: {}", e))?;
                let mut total: i64 = 0;
                {
                    let mut stmt = tx.prepare(&sql).map_err(|e| format!("db.copiar_archivo prepare: {}", e))?;
                    let mut record = csv::StringRecord::new();
                    // Streaming real: un registro vivo a la vez, RAM constante.
                    while rdr.read_record(&mut record).map_err(|e| format!("db.copiar_archivo csv: {}", e))? {
                        let vals: Vec<SqlValue> = record.iter()
                            .map(|s| SqlValue::Text(s.to_string())).collect();
                        stmt.execute(rusqlite::params_from_iter(vals.iter()))
                            .map_err(|e| format!("db.copiar_archivo fila: {}", e))?;
                        total += 1;
                    }
                }
                tx.commit().map_err(|e| format!("db.copiar_archivo commit: {}", e))?;
                Ok(EvalValue::Int(total))
            })
        }
        // pool(path, n?) → Int  — en SQLite es no-op (una conexión persistente
        // por archivo); existe para que el mismo código sirva en Postgres.
        "pool" => Ok(EvalValue::Int(1)),
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

fn is_mysql_url(s: &str) -> bool {
    s.starts_with("mysql://")
}

/// Opciones de carga desde archivo (arg opcional Dict): `header`/`encabezado`
/// (saltar 1ª fila) y `delim`/`sep` (delimitador, por defecto ','). Devuelve
/// (header, delimitador_byte).
fn copy_file_opts(arg: Option<&EvalValue>) -> (bool, u8) {
    let mut header = false;
    let mut delim = b',';
    if let Some(EvalValue::Dict(m)) = arg {
        if let Some(v) = m.get("header").or_else(|| m.get("encabezado")) {
            header = matches!(v, EvalValue::Bool(true))
                || matches!(v, EvalValue::Int(n) if *n != 0);
        }
        if let Some(EvalValue::Str(s)) = m.get("delim").or_else(|| m.get("sep")) {
            if let Some(b) = s.bytes().next() { delim = b; }
        }
    }
    (header, delim)
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex, OnceLock};

    const DEFAULT_MAX: usize = 8;

    /// Pool de N conexiones por URL. Los workers de serve toman una conexión
    /// libre (o crean una hasta el tope) y la devuelven al terminar, así las
    /// peticiones concurrentes NO se serializan en una sola conexión. Al llegar
    /// al tope, un checkout espera a que otro devuelva la suya.
    struct PgPool {
        url:     String,
        max:     AtomicUsize,
        idle:    Mutex<Vec<Client>>,
        created: Mutex<usize>,
        cv:      Condvar,
    }

    impl PgPool {
        fn new(url: &str, max: usize) -> Self {
            PgPool {
                url: url.to_string(),
                max: AtomicUsize::new(max),
                idle: Mutex::new(Vec::new()),
                created: Mutex::new(0),
                cv: Condvar::new(),
            }
        }

        fn checkout(&self) -> Result<Client, String> {
            loop {
                if let Some(c) = self.idle.lock().unwrap().pop() {
                    return Ok(c);
                }
                // ¿Podemos crear una nueva sin pasar el tope?
                {
                    let mut n = self.created.lock().unwrap();
                    if *n < self.max.load(Ordering::Relaxed) {
                        *n += 1;
                        drop(n);
                        match connect_tls_aware(&self.url) {
                            Ok(c) => return Ok(c),
                            Err(e) => {
                                *self.created.lock().unwrap() -= 1;
                                return Err(format!("db(postgres): no se pudo conectar: {}", e));
                            }
                        }
                    }
                }
                // Tope alcanzado: esperar a que alguien devuelva una conexión.
                let guard = self.idle.lock().unwrap();
                let _ = self.cv.wait_timeout(guard, std::time::Duration::from_millis(200)).unwrap();
            }
        }

        fn checkin(&self, c: Client) {
            // Una conexión muerta no vuelve al pool: se descarta y se libera cupo.
            if c.is_closed() {
                let mut n = self.created.lock().unwrap();
                if *n > 0 { *n -= 1; }
                self.cv.notify_one();
                return;
            }
            self.idle.lock().unwrap().push(c);
            self.cv.notify_one();
        }
    }

    static POOL: OnceLock<Mutex<StdHashMap<String, Arc<PgPool>>>> = OnceLock::new();

    fn pool() -> &'static Mutex<StdHashMap<String, Arc<PgPool>>> {
        POOL.get_or_init(|| Mutex::new(StdHashMap::new()))
    }

    fn pool_for(url: &str) -> Arc<PgPool> {
        let mut guard = pool().lock().unwrap();
        if let Some(p) = guard.get(url) {
            return p.clone();
        }
        let p = Arc::new(PgPool::new(url, DEFAULT_MAX));
        guard.insert(url.to_string(), p.clone());
        p
    }

    /// Conecta usando TLS o no según `sslmode` en la URL (semántica libpq):
    /// - `disable` (o ausente) → sin cifrar (por defecto, dev local).
    /// - `require`/`prefer`    → cifrado, SIN verificar el certificado (acepta
    ///   self-signed — común en dev y en muchos Postgres cloud).
    /// - `verify-ca`/`verify-full` → cifrado y verificando el certificado.
    /// En Windows native-tls usa SChannel: no hace falta OpenSSL.
    fn connect_tls_aware(url: &str) -> Result<Client, Box<dyn std::error::Error + Sync + Send>> {
        let mode = sslmode(url);
        if mode == "disable" || mode.is_empty() {
            return Ok(Client::connect(url, NoTls)?);
        }
        let verify = mode.starts_with("verify");
        let connector = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(!verify)
            .danger_accept_invalid_hostnames(!verify)
            .build()?;
        let tls = postgres_native_tls::MakeTlsConnector::new(connector);
        Ok(Client::connect(url, tls)?)
    }

    /// Valor de `sslmode` en la query de la URL, en minúscula ("" si no está).
    fn sslmode(url: &str) -> String {
        url.split(|c| c == '?' || c == '&')
            .find_map(|p| p.trim().strip_prefix("sslmode="))
            .unwrap_or("")
            .to_lowercase()
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
            // pool(url, n) → Int  — fija el máximo de conexiones concurrentes.
            "pool" => {
                if args.is_empty() { return Err("db.pool requiere (url, n?)".into()); }
                let n = args.get(1).and_then(|v| v.to_i64().ok()).unwrap_or(DEFAULT_MAX as i64).max(1) as usize;
                pool_for(&to_str(&args[0])).max.store(n, Ordering::Relaxed);
                Ok(EvalValue::Int(n as i64))
            }
            // copiar(url, tabla, [columnas], [[fila],…]) → Int  — carga masiva
            // vía COPY FROM STDIN (mucho más rápido que INSERT para millones).
            "copiar" | "copy" => {
                if args.len() < 4 { return Err("db.copiar requiere (url, tabla, columnas, filas)".into()); }
                let tabla = to_str(&args[1]);
                let cols = match &args[2] {
                    EvalValue::List(l) => l.iter().map(to_str).collect::<Vec<_>>(),
                    _ => return Err("db.copiar: columnas debe ser una lista".into()),
                };
                let filas = match &args[3] {
                    EvalValue::List(l) => l.clone(),
                    _ => return Err("db.copiar: filas debe ser una lista de listas".into()),
                };
                with_client(&to_str(&args[0]), |c| {
                    use std::io::Write;
                    let sql = format!("COPY {} ({}) FROM STDIN", tabla, cols.join(", "));
                    let mut w = c.copy_in(sql.as_str())
                        .map_err(|e| format!("db.copiar: {}", e))?;
                    let mut line = Vec::new();
                    for fila in &filas {
                        let campos = match fila {
                            EvalValue::List(f) => f,
                            _ => return Err("db.copiar: cada fila debe ser una lista".into()),
                        };
                        line.clear();
                        for (i, val) in campos.iter().enumerate() {
                            if i > 0 { line.push(b'\t'); }
                            line.extend_from_slice(copy_encode(val).as_bytes());
                        }
                        line.push(b'\n');
                        w.write_all(&line).map_err(|e| format!("db.copiar write: {}", e))?;
                    }
                    let n = w.finish().map_err(|e| format!("db.copiar finish: {}", e))?;
                    Ok(EvalValue::Int(n as i64))
                })
            }
            // copiar_archivo(url, tabla, [columnas], ruta_csv, opts?) → Int
            // Carga en STREAMING con RAM constante: lee el CSV en trozos de 64KB
            // y los empuja a COPY … FROM STDIN (Postgres parsea el CSV). No
            // materializa el archivo en memoria — sirve para millones de filas.
            "copiar_archivo" | "copy_file" => {
                if args.len() < 4 { return Err("db.copiar_archivo requiere (url, tabla, columnas, ruta, opts?)".into()); }
                let tabla = to_str(&args[1]);
                let cols = match &args[2] {
                    EvalValue::List(l) => l.iter().map(to_str).collect::<Vec<_>>(),
                    _ => return Err("db.copiar_archivo: columnas debe ser una lista".into()),
                };
                let ruta = to_str(&args[3]);
                let (header, delim) = super::copy_file_opts(args.get(4));
                let delim_ch = delim as char;
                with_client(&to_str(&args[0]), |c| {
                    use std::io::{Read, Write};
                    let mut opts = format!("FORMAT csv, DELIMITER '{}'", delim_ch);
                    if header { opts.push_str(", HEADER true"); }
                    let sql = format!("COPY {} ({}) FROM STDIN WITH ({})", tabla, cols.join(", "), opts);
                    let file = std::fs::File::open(&ruta)
                        .map_err(|e| format!("db.copiar_archivo: no se pudo abrir '{}': {}", ruta, e))?;
                    let mut reader = std::io::BufReader::new(file);
                    let mut w = c.copy_in(sql.as_str())
                        .map_err(|e| format!("db.copiar_archivo: {}", e))?;
                    let mut buf = [0u8; 65536];
                    loop {
                        let n = reader.read(&mut buf).map_err(|e| format!("db.copiar_archivo read: {}", e))?;
                        if n == 0 { break; }
                        w.write_all(&buf[..n]).map_err(|e| format!("db.copiar_archivo write: {}", e))?;
                    }
                    let filas = w.finish().map_err(|e| format!("db.copiar_archivo finish: {}", e))?;
                    Ok(EvalValue::Int(filas as i64))
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
        let pool = pool_for(url);
        let mut c = pool.checkout()?;
        // La conexión vuelve al pool tanto si f va bien como si falla: un error
        // de SQL (constraint, etc.) no rompe la conexión; is_closed la filtra.
        let r = f(&mut c);
        pool.checkin(c);
        r
    }

    /// Codifica un valor para el formato texto de COPY (TSV con escapes libpq).
    fn copy_encode(v: &EvalValue) -> String {
        match v {
            EvalValue::Null    => "\\N".to_string(),
            EvalValue::Bool(b) => if *b { "t".into() } else { "f".into() },
            EvalValue::Int(n)  => n.to_string(),
            EvalValue::Float(f) => f.to_string(),
            EvalValue::Str(s)  => copy_escape(s),
            other => copy_escape(&crate::modules::json_mod::eval_to_json(other.clone()).to_string()),
        }
    }

    fn copy_escape(s: &str) -> String {
        let mut o = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '\\' => o.push_str("\\\\"),
                '\t' => o.push_str("\\t"),
                '\n' => o.push_str("\\n"),
                '\r' => o.push_str("\\r"),
                _    => o.push(c),
            }
        }
        o
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

//    Backend MySQL / MariaDB
//
// Misma API que SQLite/Postgres, contra un servidor MySQL. La URL es
// mysql://user:pass@host:puerto/base. MySQL usa `?` como placeholder igual que
// SQLite, así que NO hace falta traducir. El crate `mysql` ya trae su propio
// pool interno (mysql::Pool), guardamos uno por URL.

mod my {
    use super::{EvalValue, extract_params, to_str};
    use indexmap::IndexMap;
    use mysql::prelude::*;
    use mysql::{Pool, Row, Value as MyValue, Params};
    use std::collections::HashMap as StdHashMap;
    use std::sync::{Mutex, OnceLock};

    static POOL: OnceLock<Mutex<StdHashMap<String, Pool>>> = OnceLock::new();

    fn pools() -> &'static Mutex<StdHashMap<String, Pool>> {
        POOL.get_or_init(|| Mutex::new(StdHashMap::new()))
    }

    fn pool_for(url: &str) -> Result<Pool, String> {
        let mut guard = pools().lock().unwrap();
        if let Some(p) = guard.get(url) {
            return Ok(p.clone());
        }
        let pool = Pool::new(url)
            .map_err(|e| format!("db(mysql): no se pudo conectar: {}", e))?;
        guard.insert(url.to_string(), pool.clone());
        Ok(pool)
    }

    fn conn(url: &str) -> Result<mysql::PooledConn, String> {
        pool_for(url)?.get_conn().map_err(|e| format!("db(mysql): {}", e))
    }

    pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
        match function {
            "query" | "consulta" => {
                if args.len() < 2 { return Err("db.query requiere (url, sql, params?)".into()); }
                let sql = to_str(&args[1]);
                let params = to_params(&extract_params(args.get(2)));
                let mut c = conn(&to_str(&args[0]))?;
                let rows: Vec<Row> = c.exec(sql, params)
                    .map_err(|e| format!("db.query(mysql): {}", e))?;
                Ok(rows_to_eval(rows))
            }
            "uno" | "first" => {
                if args.len() < 2 { return Err("db.uno requiere (url, sql, params?)".into()); }
                let sql = to_str(&args[1]);
                let params = to_params(&extract_params(args.get(2)));
                let mut c = conn(&to_str(&args[0]))?;
                let rows: Vec<Row> = c.exec(sql, params)
                    .map_err(|e| format!("db.uno(mysql): {}", e))?;
                Ok(rows.into_iter().next().map(row_to_dict).unwrap_or(EvalValue::Null))
            }
            "ejecutar" | "exec" => {
                if args.len() < 2 { return Err("db.ejecutar requiere (url, sql, params?)".into()); }
                let sql = to_str(&args[1]);
                let params = to_params(&extract_params(args.get(2)));
                let mut c = conn(&to_str(&args[0]))?;
                c.exec_drop(sql, params).map_err(|e| format!("db.ejecutar(mysql): {}", e))?;
                Ok(EvalValue::Int(c.affected_rows() as i64))
            }
            // insertar: MySQL sí tiene last_insert_id() → devuelve el id autogenerado.
            "insertar" | "insert" => {
                if args.len() < 2 { return Err("db.insertar requiere (url, sql, params?)".into()); }
                let sql = to_str(&args[1]);
                let params = to_params(&extract_params(args.get(2)));
                let mut c = conn(&to_str(&args[0]))?;
                c.exec_drop(sql, params).map_err(|e| format!("db.insertar(mysql): {}", e))?;
                Ok(EvalValue::Int(c.last_insert_id() as i64))
            }
            "transaccion" | "transaction" => {
                if args.len() < 2 { return Err("db.transaccion requiere (url, [sqls])".into()); }
                let pasos = match &args[1] {
                    EvalValue::List(l) => l.clone(),
                    _ => return Err("db.transaccion: segundo arg debe ser lista de SQL".into()),
                };
                let mut c = conn(&to_str(&args[0]))?;
                let mut tx = c.start_transaction(mysql::TxOpts::default())
                    .map_err(|e| format!("db.transaccion(mysql): {}", e))?;
                for paso in &pasos {
                    let (sql, params) = match paso {
                        EvalValue::List(par) if par.len() >= 2 => (to_str(&par[0]), extract_params(par.get(1))),
                        otro => (to_str(otro), vec![]),
                    };
                    tx.exec_drop(sql.as_str(), to_params(&params))
                        .map_err(|e| format!("db.transaccion(mysql) '{}': {}", sql, e))?;
                }
                tx.commit().map_err(|e| format!("db.transaccion commit(mysql): {}", e))?;
                Ok(EvalValue::Bool(true))
            }
            "tablas" | "tables" => {
                if args.is_empty() { return Err("db.tablas requiere (url)".into()); }
                let mut c = conn(&to_str(&args[0]))?;
                let rows: Vec<Row> = c.query("SHOW TABLES")
                    .map_err(|e| format!("db.tablas(mysql): {}", e))?;
                Ok(EvalValue::List(rows.into_iter().map(|mut r| my_cell(r.take(0).unwrap_or(MyValue::NULL))).collect()))
            }
            // copiar(url, tabla, [columnas], [[fila],…]) → Int — carga masiva.
            // MySQL no tiene COPY FROM STDIN; se usa INSERT multi-fila por lotes
            // dentro de una transacción (el camino rápido en MySQL).
            "copiar" | "copy" => {
                if args.len() < 4 { return Err("db.copiar requiere (url, tabla, columnas, filas)".into()); }
                let tabla = to_str(&args[1]);
                let cols = match &args[2] {
                    EvalValue::List(l) => l.iter().map(to_str).collect::<Vec<_>>(),
                    _ => return Err("db.copiar: columnas debe ser una lista".into()),
                };
                let filas = match &args[3] {
                    EvalValue::List(l) => l.clone(),
                    _ => return Err("db.copiar: filas debe ser una lista de listas".into()),
                };
                if filas.is_empty() { return Ok(EvalValue::Int(0)); }
                let ncols = cols.len();
                let row_ph = format!("({})", vec!["?"; ncols].join(","));
                let mut c = conn(&to_str(&args[0]))?;
                let mut tx = c.start_transaction(mysql::TxOpts::default())
                    .map_err(|e| format!("db.copiar(mysql): {}", e))?;
                for chunk in filas.chunks(500) {
                    let valores = vec![row_ph.as_str(); chunk.len()].join(",");
                    let sql = format!("INSERT INTO {} ({}) VALUES {}", tabla, cols.join(","), valores);
                    let mut flat: Vec<EvalValue> = Vec::with_capacity(chunk.len() * ncols);
                    for fila in chunk {
                        match fila {
                            EvalValue::List(f) => flat.extend(f.iter().cloned()),
                            _ => return Err("db.copiar: cada fila debe ser una lista".into()),
                        }
                    }
                    tx.exec_drop(sql.as_str(), to_params(&flat))
                        .map_err(|e| format!("db.copiar(mysql) lote: {}", e))?;
                }
                tx.commit().map_err(|e| format!("db.copiar commit(mysql): {}", e))?;
                Ok(EvalValue::Int(filas.len() as i64))
            }
            // copiar_archivo(url, tabla, [columnas], ruta_csv, opts?) → Int
            // Streaming con RAM constante: el crate csv lee fila a fila y se
            // insertan por lotes de 500 en una transacción.
            "copiar_archivo" | "copy_file" => {
                if args.len() < 4 { return Err("db.copiar_archivo requiere (url, tabla, columnas, ruta, opts?)".into()); }
                let tabla = to_str(&args[1]);
                let cols = match &args[2] {
                    EvalValue::List(l) => l.iter().map(to_str).collect::<Vec<_>>(),
                    _ => return Err("db.copiar_archivo: columnas debe ser una lista".into()),
                };
                let ruta = to_str(&args[3]);
                let (header, delim) = super::copy_file_opts(args.get(4));
                let ncols = cols.len();
                let row_ph = format!("({})", vec!["?"; ncols].join(","));
                let mut rdr = csv::ReaderBuilder::new()
                    .delimiter(delim).has_headers(header).flexible(true)
                    .from_path(&ruta)
                    .map_err(|e| format!("db.copiar_archivo: no se pudo abrir '{}': {}", ruta, e))?;
                let mut c = conn(&to_str(&args[0]))?;
                let mut tx = c.start_transaction(mysql::TxOpts::default())
                    .map_err(|e| format!("db.copiar_archivo(mysql): {}", e))?;
                let mut total: i64 = 0;
                let mut lote: Vec<EvalValue> = Vec::new();
                let mut record = csv::StringRecord::new();
                let flush = |tx: &mut mysql::Transaction, lote: &mut Vec<EvalValue>| -> Result<(), String> {
                    if lote.is_empty() { return Ok(()); }
                    let filas = lote.len() / ncols;
                    let valores = vec![row_ph.as_str(); filas].join(",");
                    let sql = format!("INSERT INTO {} ({}) VALUES {}", tabla, cols.join(","), valores);
                    tx.exec_drop(sql.as_str(), to_params(lote))
                        .map_err(|e| format!("db.copiar_archivo(mysql) lote: {}", e))?;
                    lote.clear();
                    Ok(())
                };
                while rdr.read_record(&mut record).map_err(|e| format!("db.copiar_archivo csv: {}", e))? {
                    for field in record.iter() { lote.push(EvalValue::Str(field.to_string())); }
                    total += 1;
                    if lote.len() >= 500 * ncols { flush(&mut tx, &mut lote)?; }
                }
                flush(&mut tx, &mut lote)?;
                tx.commit().map_err(|e| format!("db.copiar_archivo commit(mysql): {}", e))?;
                Ok(EvalValue::Int(total))
            }
            // pool(url, n?) → Int — el crate mysql ya gestiona su propio pool
            // interno; aquí es informativo (no-op) para paridad de API.
            "pool" => Ok(EvalValue::Int(
                args.get(1).and_then(|v| v.to_i64().ok()).unwrap_or(10)
            )),
            "cerrar" | "close" => {
                if args.is_empty() { return Err("db.cerrar requiere (url)".into()); }
                let removed = pools().lock().unwrap().remove(&to_str(&args[0])).is_some();
                Ok(EvalValue::Bool(removed))
            }
            f => Err(format!("db.{}() no existe para MySQL", f)),
        }
    }

    fn rows_to_eval(rows: Vec<Row>) -> EvalValue {
        EvalValue::List(rows.into_iter().map(row_to_dict).collect())
    }

    fn row_to_dict(mut row: Row) -> EvalValue {
        let cols = row.columns();
        let mut m = IndexMap::new();
        for (i, col) in cols.iter().enumerate() {
            let v = row.take::<MyValue, usize>(i).unwrap_or(MyValue::NULL);
            m.insert(col.name_str().to_string(), my_cell(v));
        }
        EvalValue::Dict(m)
    }

    /// EvalValue → parámetros posicionales de MySQL.
    fn to_params(params: &[EvalValue]) -> Params {
        if params.is_empty() { return Params::Empty; }
        Params::Positional(params.iter().map(|v| match v {
            EvalValue::Int(n)   => MyValue::Int(*n),
            EvalValue::Float(f) => MyValue::Double(*f),
            EvalValue::Bool(b)  => MyValue::Int(*b as i64),
            EvalValue::Null     => MyValue::NULL,
            EvalValue::Str(s)   => MyValue::Bytes(s.clone().into_bytes()),
            other => MyValue::Bytes(
                crate::modules::json_mod::eval_to_json(other.clone()).to_string().into_bytes()
            ),
        }).collect())
    }

    /// mysql::Value → EvalValue. El texto llega como Bytes (UTF-8 → Str, si no base64).
    fn my_cell(v: MyValue) -> EvalValue {
        use base64::Engine as _;
        match v {
            MyValue::NULL      => EvalValue::Null,
            MyValue::Int(n)    => EvalValue::Int(n),
            MyValue::UInt(n)   => EvalValue::Int(n as i64),
            MyValue::Float(f)  => EvalValue::Float(f as f64),
            MyValue::Double(f) => EvalValue::Float(f),
            MyValue::Bytes(b)  => match String::from_utf8(b) {
                Ok(s)  => EvalValue::Str(s),
                Err(e) => EvalValue::Str(base64::engine::general_purpose::STANDARD.encode(e.as_bytes())),
            },
            MyValue::Date(y, mo, d, h, mi, s, _us) =>
                EvalValue::Str(format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", y, mo, d, h, mi, s)),
            MyValue::Time(neg, days, h, mi, s, _us) =>
                EvalValue::Str(format!("{}{}:{:02}:{:02}", if neg { "-" } else { "" },
                    days * 24 + h as u32, mi, s)),
        }
    }
}
