use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};

struct Route {
    method:  String,
    pattern: String,
    handler: EvalValue,   // Function o Str (nombre de función en env)
}

struct RouterData {
    routes:      Vec<Route>,
    middlewares: Vec<EvalValue>,
    statics:     Vec<(String, String)>,   // (prefijo URL, carpeta en disco)
    guards:      Vec<(String, String)>,   // (prefijo URL, secret JWT)
}

static ROUTERS: OnceLock<Mutex<HashMap<u64, RouterData>>> = OnceLock::new();
static ACTIVE:  OnceLock<Mutex<Option<u64>>>              = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn store() -> &'static Mutex<HashMap<u64, RouterData>> {
    ROUTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn active_id() -> &'static Mutex<Option<u64>> {
    ACTIVE.get_or_init(|| Mutex::new(None))
}


//    call() — API Orion                                                         

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // new() → Int
        "new" => {
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            store().lock().unwrap().insert(id, RouterData {
                routes: Vec::new(), middlewares: Vec::new(),
                statics: Vec::new(), guards: Vec::new(),
            });
            Ok(EvalValue::Int(id as i64))
        }

        // static(id, "/static", "carpeta") → Bool — sirve archivos de la carpeta
        // bajo el prefijo, con MIME automático e index.html en directorios.
        // Los paths se validan contra path traversal (../ no escapa la carpeta).
        "static" | "static_dir" => {
            if args.len() < 3 { return Err("router.static requires (id, prefijo_url, carpeta)".into()); }
            let id     = to_u64(&args[0])?;
            let mut prefix = to_str(&args[1]);
            let dir    = to_str(&args[2]);
            if !prefix.starts_with('/') { prefix.insert(0, '/'); }
            let prefix = prefix.trim_end_matches('/').to_string();
            with_router_mut(id, |r| {
                r.statics.push((prefix, dir));
                Ok(EvalValue::Bool(true))
            })
        }

        // guard(id, "/prefijo", secret) → Bool — protege todo lo que cuelga del
        // prefijo con JWT Bearer: sin token válido serve responde 401 solo;
        // con token válido el handler recibe los claims en req["user"].
        "guard" => {
            if args.len() < 3 { return Err("router.guard requires (id, prefijo_url, secret)".into()); }
            let id     = to_u64(&args[0])?;
            let mut prefix = to_str(&args[1]);
            let secret = to_str(&args[2]);
            if !prefix.starts_with('/') { prefix.insert(0, '/'); }
            let prefix = prefix.trim_end_matches('/').to_string();
            with_router_mut(id, |r| {
                r.guards.push((prefix, secret));
                Ok(EvalValue::Bool(true))
            })
        }

        // add(id, method, path, handler) → Bool
        "add" => {
            if args.len() < 4 { return Err("router.add requires (id, method, path, handler)".into()); }
            let id      = to_u64(&args[0])?;
            let method  = to_str(&args[1]).to_uppercase();
            let pattern = to_str(&args[2]);
            let handler = args[3].clone();
            check_handler("add", "el handler", &handler)?;
            with_router_mut(id, |r| {
                r.routes.push(Route { method, pattern, handler });
                Ok(EvalValue::Bool(true))
            })
        }

        // get/post/put/delete/patch(id, path, handler) → Bool
        "get" | "post" | "put" | "delete" | "patch" => {
            if args.len() < 3 { return Err(format!("router.{} requires (id, path, handler)", function)); }
            let id      = to_u64(&args[0])?;
            let method  = function.to_uppercase();
            let pattern = to_str(&args[1]);
            let handler = args[2].clone();
            check_handler(function, "el handler", &handler)?;
            with_router_mut(id, |r| {
                r.routes.push(Route { method, pattern, handler });
                Ok(EvalValue::Bool(true))
            })
        }

        // use_middleware(id, fn) → Bool
        "use_middleware" => {
            if args.len() < 2 { return Err("router.use_middleware requires (id, handler_fn)".into()); }
            let id = to_u64(&args[0])?;
            let mw = args[1].clone();
            check_handler("use_middleware", "el middleware", &mw)?;
            with_router_mut(id, |r| { r.middlewares.push(mw); Ok(EvalValue::Bool(true)) })
        }

        // attach(id) → Bool  — activa el router para el próximo serve
        "attach" => {
            if args.is_empty() { return Err("router.attach requires (id)".into()); }
            let id = to_u64(&args[0])?;
            if !store().lock().unwrap().contains_key(&id) {
                return Err(format!("router: ID {} does not exist", id));
            }
            *active_id().lock().unwrap() = Some(id);
            Ok(EvalValue::Bool(true))
        }

        // detach() → Bool
        "detach" => {
            *active_id().lock().unwrap() = None;
            Ok(EvalValue::Bool(true))
        }

        // match(id, method, path) → Dict {handler_name, params, method, path} | Null
        // (mantiene la versión anterior para uso manual)
        "match" => {
            if args.len() < 3 { return Err("router.match requires (id, method, path)".into()); }
            let id     = to_u64(&args[0])?;
            let method = to_str(&args[1]).to_uppercase();
            let path   = to_str(&args[2]);
            let store  = store().lock().unwrap();
            let data   = store.get(&id).ok_or_else(|| format!("router: ID {} does not exist", id))?;
            for route in &data.routes {
                if route.method == method || route.method == "*" {
                    if let Some(params) = match_path(&route.pattern, &path) {
                        let mut d = HashMap::new();
                        d.insert("method".into(), EvalValue::Str(method.clone()));
                        d.insert("path".into(),   EvalValue::Str(path.clone()));
                        let params_dict: HashMap<String, EvalValue> = params.into_iter()
                            .map(|(k, v)| (k, EvalValue::Str(v)))
                            .collect();
                        d.insert("params".into(), EvalValue::Dict(params_dict));
                        // handler: si es Str devolverlo, si es Function devolver nombre
                        let handler_label = match &route.handler {
                            EvalValue::Str(s)             => s.clone(),
                            EvalValue::Function { name, .. } => name.clone(),
                            _                             => "<fn>".into(),
                        };
                        d.insert("handler".into(), EvalValue::Str(handler_label));
                        return Ok(EvalValue::Dict(d));
                    }
                }
            }
            Ok(EvalValue::Null)
        }

        // routes(id) → List de Dicts
        "routes" => {
            if args.is_empty() { return Err("router.routes requires (id)".into()); }
            let id    = to_u64(&args[0])?;
            let store = store().lock().unwrap();
            let data  = store.get(&id).ok_or_else(|| format!("router: ID {} does not exist", id))?;
            let list  = data.routes.iter().map(|r| {
                let mut d = HashMap::new();
                d.insert("method".into(),  EvalValue::Str(r.method.clone()));
                d.insert("pattern".into(), EvalValue::Str(r.pattern.clone()));
                let label = match &r.handler {
                    EvalValue::Str(s)             => s.clone(),
                    EvalValue::Function { name, .. } => format!("<fn {}>", name),
                    _                             => "<fn>".into(),
                };
                d.insert("handler".into(), EvalValue::Str(label));
                EvalValue::Dict(d)
            }).collect();
            Ok(EvalValue::List(list))
        }

        // clear(id) → Bool
        "clear" => {
            if args.is_empty() { return Err("router.clear requires (id)".into()); }
            with_router_mut(to_u64(&args[0])?, |r| { r.routes.clear(); Ok(EvalValue::Bool(true)) })
        }

        // drop(id) → Bool
        "drop" => {
            if args.is_empty() { return Err("router.drop requires (id)".into()); }
            let id = to_u64(&args[0])?;
            store().lock().unwrap().shift_remove(&id);
            // Desactivar si era el activo
            let mut act = active_id().lock().unwrap();
            if *act == Some(id) { *act = None; }
            Ok(EvalValue::Bool(true))
        }

        f => Err(format!("router.{}() does not exist", f)),
    }
}

//    Dispatch desde serve

/// Resultado de rutear un request contra el router activo.
pub struct ActiveMatch {
    /// Nombre de la función handler (los handlers deben ser funciones con
    /// nombre: los workers de serve las invocan por nombre en su propia VM).
    pub handler: String,
    /// Parámetros de ruta extraídos (`:id`, `*rest`).
    pub params: Vec<(String, String)>,
    /// Nombres de los middlewares registrados (se ejecutan en orden).
    pub middlewares: Vec<String>,
}

fn handler_name(h: &EvalValue) -> Option<String> {
    match h {
        EvalValue::Str(s) => Some(s.clone()),
        EvalValue::Function { name, .. } if !name.is_empty() => Some(name.clone()),
        _ => None,
    }
}

/// ¿El path cae bajo un prefijo estático del router activo?
/// Devuelve (carpeta, resto_del_path) para que serve resuelva el archivo.
pub fn active_static(path: &str) -> Option<(String, String)> {
    let id = (*active_id().lock().unwrap())?;
    let store = store().lock().unwrap();
    let data = store.get(&id)?;
    for (prefix, dir) in &data.statics {
        if path == prefix || path.starts_with(&format!("{}/", prefix)) {
            let rest = path[prefix.len()..].trim_start_matches('/').to_string();
            return Some((dir.clone(), rest));
        }
    }
    None
}

/// ¿El path cae bajo un prefijo protegido con router.guard?
/// Devuelve el secret JWT para que serve valide el token.
pub fn active_guard(path: &str) -> Option<String> {
    let id = (*active_id().lock().unwrap())?;
    let store = store().lock().unwrap();
    let data = store.get(&id)?;
    for (prefix, secret) in &data.guards {
        if path == prefix || path.starts_with(&format!("{}/", prefix)) {
            return Some(secret.clone());
        }
    }
    None
}

/// Rutea method+path contra el router activo. `None` si no hay router activo
/// o ninguna ruta coincide (serve cae al handler global en ese caso).
pub fn active_match(method: &str, path: &str) -> Option<ActiveMatch> {
    let id = (*active_id().lock().unwrap())?;
    let store = store().lock().unwrap();
    let data = store.get(&id)?;
    let method = method.to_uppercase();
    for route in &data.routes {
        if route.method == method || route.method == "*" {
            if let Some(params) = match_path(&route.pattern, path) {
                let handler = handler_name(&route.handler)?;
                let middlewares = data.middlewares.iter()
                    .filter_map(handler_name)
                    .collect();
                return Some(ActiveMatch {
                    handler,
                    params: params.into_iter().collect(),
                    middlewares,
                });
            }
        }
    }
    None
}

//    Helpers

/// Valida un handler EN EL REGISTRO, no en el despacho.
///
/// Los workers de serve corren cada petición en su propia VM e invocan los
/// handlers POR NOMBRE, así que una lambda anónima es inservible: no tiene
/// nombre que buscar. Antes eso se descubría en tiempo de request —
/// `handler_name` devolvía `None`, la ruta caía al fallback y el middleware
/// se descartaba en silencio. Un middleware de autorización escrito como
/// lambda no bloqueaba nada y el endpoint quedaba abierto sin un solo aviso.
///
/// Fallar aquí convierte ese silencio en un error que apunta a la línea
/// exacta donde se registró la ruta.
fn check_handler(fn_name: &str, arg_desc: &str, h: &EvalValue) -> Result<(), String> {
    match h {
        EvalValue::Str(s) if !s.is_empty() => Ok(()),
        EvalValue::Function { name, .. } if !name.is_empty() => Ok(()),
        // Una lambda anónima no sobrevive el paso a un módulo nativo: llega
        // como Null. Es también lo que llega si se pasa una variable sin
        // definir, así que el mensaje cubre los dos casos sin afirmar cuál es.
        EvalValue::Function { .. } | EvalValue::Null => Err(format!(
            "router.{}: {} tiene que ser una función CON NOMBRE. serve corre \
             cada petición en su propia VM e invoca los handlers por nombre, \
             así que una lambda anónima no sirve (y llega aquí como null). \
             Declara `fn mi_handler(req) {{ ... }}` y pásala como `mi_handler` \
             o como \"mi_handler\".",
            fn_name, arg_desc
        )),
        other => Err(format!(
            "router.{}: {} debe ser el nombre de una función (\"mi_handler\") o \
             una función con nombre, no {}.",
            fn_name, arg_desc, type_label(other)
        )),
    }
}

fn type_label(v: &EvalValue) -> &'static str {
    match v {
        EvalValue::Str(_)   => "un empty string",
        EvalValue::Int(_)   => "un entero",
        EvalValue::Float(_) => "un float",
        EvalValue::Bool(_)  => "un booleano",
        EvalValue::List(_)  => "una lista",
        EvalValue::Dict(_)  => "un dict",
        _                   => "ese valor",
    }
}

fn with_router_mut<F>(id: u64, f: F) -> Result<EvalValue, String>
where F: FnOnce(&mut RouterData) -> Result<EvalValue, String>
{
    f(store().lock().unwrap()
        .get_mut(&id)
        .ok_or_else(|| format!("router: ID {} does not exist", id))?)
}

/// Coincide `pattern` con `path`, extrayendo parámetros `:param` y `*wildcard`.
fn match_path(pattern: &str, path: &str) -> Option<HashMap<String, String>> {
    let p_segs: Vec<&str> = pattern.trim_matches('/').split('/').collect();
    let r_segs: Vec<&str> = path.trim_matches('/').split('/').collect();

    let has_wildcard = p_segs.last().map(|s| s.starts_with('*')).unwrap_or(false);

    if !has_wildcard && p_segs.len() != r_segs.len() { return None; }
    if has_wildcard && r_segs.len() < p_segs.len() - 1 { return None; }

    let mut params   = HashMap::new();
    let check_len    = if has_wildcard { p_segs.len() - 1 } else { p_segs.len() };

    for i in 0..check_len {
        let p = p_segs[i];
        let r = r_segs.get(i)?;
        if p.starts_with(':') {
            params.insert(p[1..].to_string(), (*r).to_string());
        } else if p != *r {
            return None;
        }
    }

    if has_wildcard {
        let name = p_segs.last().unwrap().trim_start_matches('*');
        let rest = r_segs[check_len..].join("/");
        if !name.is_empty() { params.insert(name.to_string(), rest); }
    }

    Some(params)
}

fn to_u64(v: &EvalValue) -> Result<u64, String> {
    match v {
        EvalValue::Int(n) if *n > 0 => Ok(*n as u64),
        _ => Err("router: the ID must be a positive Int".into()),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
