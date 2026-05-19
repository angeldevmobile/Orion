use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use ssh2::Session;
use std::net::TcpStream;
use std::io::{Read, Write};
use std::path::Path;

// ── Configuración de conexión (se guarda para reconectar si es necesario) ──

struct ConnConfig {
    host: String,
    port: u16,
    user: String,
    auth: SshAuth,
}

enum SshAuth {
    Password(String),
    Key(String),
}

// ── Sesión persistente con su configuración ──

struct LiveSession {
    session: Session,
    config: ConnConfig,
}

static SESSIONS: OnceLock<Mutex<HashMap<String, LiveSession>>> = OnceLock::new();
static SESSION_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

fn sessions() -> &'static Mutex<HashMap<String, LiveSession>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn new_id() -> String {
    let n = SESSION_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("ssh_{}", n)
}

/// Abre una sesión SSH real con la configuración dada.
fn connect_session(cfg: &ConnConfig) -> Result<Session, String> {
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let tcp = TcpStream::connect(&addr)
        .map_err(|e| format!("ssh: no se pudo conectar a {}: {}", addr, e))?;
    // Timeout de 30s para operaciones bloqueantes
    tcp.set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok();
    tcp.set_write_timeout(Some(std::time::Duration::from_secs(30)))
        .ok();
    let mut sess = Session::new()
        .map_err(|e| format!("ssh: error creando sesión: {}", e))?;
    sess.set_tcp_stream(tcp);
    sess.handshake()
        .map_err(|e| format!("ssh: handshake falló: {}", e))?;
    match &cfg.auth {
        SshAuth::Password(pass) => {
            sess.userauth_password(&cfg.user, pass)
                .map_err(|e| format!("ssh: autenticación por contraseña falló: {}", e))?;
        }
        SshAuth::Key(key_path) => {
            sess.userauth_pubkey_file(&cfg.user, None, Path::new(key_path), None)
                .map_err(|e| format!("ssh: autenticación por clave falló: {}", e))?;
        }
    }
    if !sess.authenticated() {
        return Err("ssh: autenticación fallida".into());
    }
    Ok(sess)
}

/// Obtiene la sesión del pool. Si está caída la reconecta automáticamente.
fn with_session<F, R>(id: &str, f: F) -> Result<R, String>
where
    F: Fn(&Session) -> Result<R, String>,
{
    let mut lock = sessions().lock().unwrap();
    let live = lock.get_mut(id)
        .ok_or_else(|| format!("ssh: sesión '{}' no encontrada. Usa ssh.connect() primero.", id))?;

    // Intentar usar la sesión existente
    match f(&live.session) {
        Ok(r) => Ok(r),
        Err(_) => {
            // La sesión puede estar muerta: reconectar y reintentar una vez
            let new_sess = connect_session(&live.config)?;
            live.session = new_sess;
            f(&live.session)
        }
    }
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // ssh.connect(host, port, user, password) → session_id
        "connect" => {
            if args.len() < 4 {
                return Err("ssh.connect requiere (host, port, user, password)".into());
            }
            let host = to_str(&args[0]);
            let port = args[1].to_i64()? as u16;
            let user = to_str(&args[2]);
            let pass = to_str(&args[3]);
            let cfg  = ConnConfig { host, port, user, auth: SshAuth::Password(pass) };
            let sess = connect_session(&cfg)?;
            let id   = new_id();
            sessions().lock().unwrap().insert(id.clone(), LiveSession { session: sess, config: cfg });
            Ok(EvalValue::Str(id))
        }

        // ssh.connect_key(host, port, user, key_path) → session_id
        "connect_key" => {
            if args.len() < 4 {
                return Err("ssh.connect_key requiere (host, port, user, key_path)".into());
            }
            let host     = to_str(&args[0]);
            let port     = args[1].to_i64()? as u16;
            let user     = to_str(&args[2]);
            let key_path = to_str(&args[3]);
            let cfg  = ConnConfig { host, port, user, auth: SshAuth::Key(key_path) };
            let sess = connect_session(&cfg)?;
            let id   = new_id();
            sessions().lock().unwrap().insert(id.clone(), LiveSession { session: sess, config: cfg });
            Ok(EvalValue::Str(id))
        }

        // ssh.exec(session_id, command) → {out, err, code}
        "exec" => {
            if args.len() < 2 {
                return Err("ssh.exec requiere (session_id, command)".into());
            }
            let id  = to_str(&args[0]);
            let cmd = to_str(&args[1]);

            // Reutilizar la sesión persistente (reconecta si es necesario)
            with_session(&id, |sess| {
                let mut channel = sess.channel_session()
                    .map_err(|e| format!("ssh.exec: error abriendo canal: {}", e))?;
                channel.exec(&cmd)
                    .map_err(|e| format!("ssh.exec: error ejecutando '{}': {}", cmd, e))?;
                let mut out = String::new();
                let mut err = String::new();
                channel.read_to_string(&mut out)
                    .map_err(|e| format!("ssh.exec: error leyendo stdout: {}", e))?;
                channel.stderr().read_to_string(&mut err)
                    .map_err(|e| format!("ssh.exec: error leyendo stderr: {}", e))?;
                channel.wait_close()
                    .map_err(|e| format!("ssh.exec: error cerrando canal: {}", e))?;
                let code = channel.exit_status()
                    .map_err(|e| format!("ssh.exec: error obteniendo código: {}", e))?;
                let mut m = HashMap::new();
                m.insert("out".into(),  EvalValue::Str(out.trim_end().to_string()));
                m.insert("err".into(),  EvalValue::Str(err.trim_end().to_string()));
                m.insert("code".into(), EvalValue::Int(code as i64));
                Ok(EvalValue::Dict(m))
            })
        }

        // ssh.exec_many(session_id, commands) → [{out, err, code}]
        // Ejecuta múltiples comandos reutilizando la misma sesión sin reconectar
        "exec_many" => {
            if args.len() < 2 {
                return Err("ssh.exec_many requiere (session_id, [commands])".into());
            }
            let id = to_str(&args[0]);
            let cmds = match &args[1] {
                EvalValue::List(v) => v.iter().map(|x| to_str(x)).collect::<Vec<_>>(),
                other => vec![other.to_string()],
            };
            let mut results = Vec::new();
            for cmd in cmds {
                let result = with_session(&id, |sess| {
                    let mut channel = sess.channel_session()
                        .map_err(|e| format!("ssh.exec_many: error abriendo canal: {}", e))?;
                    channel.exec(&cmd)
                        .map_err(|e| format!("ssh.exec_many: error ejecutando '{}': {}", cmd, e))?;
                    let mut out = String::new();
                    let mut err = String::new();
                    channel.read_to_string(&mut out).ok();
                    channel.stderr().read_to_string(&mut err).ok();
                    channel.wait_close().ok();
                    let code = channel.exit_status().unwrap_or(-1);
                    let mut m = HashMap::new();
                    m.insert("cmd".into(),  EvalValue::Str(cmd.clone()));
                    m.insert("out".into(),  EvalValue::Str(out.trim_end().to_string()));
                    m.insert("err".into(),  EvalValue::Str(err.trim_end().to_string()));
                    m.insert("code".into(), EvalValue::Int(code as i64));
                    Ok(EvalValue::Dict(m))
                })?;
                results.push(result);
            }
            Ok(EvalValue::List(results))
        }

        // ssh.upload(session_id, local_path, remote_path) → {ok, bytes}
        "upload" => {
            if args.len() < 3 {
                return Err("ssh.upload requiere (session_id, local_path, remote_path)".into());
            }
            let id     = to_str(&args[0]);
            let local  = to_str(&args[1]);
            let remote = to_str(&args[2]);
            let data = std::fs::read(&local)
                .map_err(|e| format!("ssh.upload: no se pudo leer '{}': {}", local, e))?;
            let bytes = data.len() as i64;
            with_session(&id, |sess| {
                let mut channel = sess.scp_send(
                    Path::new(&remote),
                    0o644,
                    data.len() as u64,
                    None,
                ).map_err(|e| format!("ssh.upload: error SCP send: {}", e))?;
                channel.write_all(&data)
                    .map_err(|e| format!("ssh.upload: error escribiendo: {}", e))?;
                channel.send_eof().ok();
                channel.wait_eof().ok();
                channel.close().ok();
                channel.wait_close().ok();
                let mut m = HashMap::new();
                m.insert("ok".into(),    EvalValue::Bool(true));
                m.insert("bytes".into(), EvalValue::Int(bytes));
                Ok(EvalValue::Dict(m))
            })
        }

        // ssh.download(session_id, remote_path, local_path) → {ok, bytes}
        "download" => {
            if args.len() < 3 {
                return Err("ssh.download requiere (session_id, remote_path, local_path)".into());
            }
            let id     = to_str(&args[0]);
            let remote = to_str(&args[1]);
            let local  = to_str(&args[2]);
            with_session(&id, |sess| {
                let (mut channel, _stat) = sess.scp_recv(Path::new(&remote))
                    .map_err(|e| format!("ssh.download: error SCP recv '{}': {}", remote, e))?;
                let mut buf = Vec::new();
                channel.read_to_end(&mut buf)
                    .map_err(|e| format!("ssh.download: error leyendo: {}", e))?;
                std::fs::write(&local, &buf)
                    .map_err(|e| format!("ssh.download: error guardando '{}': {}", local, e))?;
                let mut m = HashMap::new();
                m.insert("ok".into(),    EvalValue::Bool(true));
                m.insert("bytes".into(), EvalValue::Int(buf.len() as i64));
                Ok(EvalValue::Dict(m))
            })
        }

        // ssh.close(session_id) → null  — libera la conexión del pool
        "close" => {
            if args.is_empty() {
                return Err("ssh.close requiere (session_id)".into());
            }
            let id = to_str(&args[0]);
            sessions().lock().unwrap().remove(&id);
            Ok(EvalValue::Null)
        }

        // ssh.test(session_id) → bool  — verifica que la conexión sigue viva
        "test" => {
            if args.is_empty() {
                return Err("ssh.test requiere (session_id)".into());
            }
            let id = to_str(&args[0]);
            let result = with_session(&id, |sess| {
                // Enviar un keepalive para verificar la conexión
                sess.keepalive_send()
                    .map(|_| EvalValue::Bool(true))
                    .map_err(|e| e.to_string())
            });
            Ok(result.unwrap_or(EvalValue::Bool(false)))
        }

        // ssh.sessions() → [session_id]  — lista sesiones activas
        "sessions" => {
            let ids: Vec<EvalValue> = sessions()
                .lock().unwrap()
                .keys()
                .map(|k| EvalValue::Str(k.clone()))
                .collect();
            Ok(EvalValue::List(ids))
        }

        f => Err(format!("ssh.{}() no existe. Funciones: connect, connect_key, exec, exec_many, upload, download, test, sessions, close", f)),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v {
        EvalValue::Str(s) => s.clone(),
        other => other.to_string(),
    }
}
