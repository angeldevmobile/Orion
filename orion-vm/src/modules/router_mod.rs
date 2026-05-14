use crate::eval_value::EvalValue;
use std::collections::HashMap;
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

//    API pública para eval.rs                                                   

/// Resultado de un despacho exitoso del router activo.
pub struct Dispatch {
    pub handler:     EvalValue,
    pub middlewares: Vec<EvalValue>,
    pub params:      HashMap<String, String>,
}

/// Indica si hay un router activo (para diferenciar "no match" de "sin router").
pub fn is_active() -> bool {
    active_id().lock().unwrap().is_some()
}

/// Intenta despachar `method` + `path` contra el router activo.
/// Devuelve `None` si no hay router activo o ninguna ruta coincide.
pub fn try_dispatch(method: &str, path: &str) -> Option<Dispatch> {
    let router_id = (*active_id().lock().unwrap())?;
    let store = store().lock().unwrap();
    let data  = store.get(&router_id)?;

    for route in &data.routes {
        if route.method == method || route.method == "*" {
            if let Some(params) = match_path(&route.pattern, path) {
                return Some(Dispatch {
                    handler:     route.handler.clone(),
                    middlewares: data.middlewares.clone(),
                    params,
                });
            }
        }
    }
    None
}

//    call() — API Orion                                                         

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // new() → Int
        "new" => {
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            store().lock().unwrap().insert(id, RouterData {
                routes: Vec::new(), middlewares: Vec::new(),
            });
            Ok(EvalValue::Int(id as i64))
        }

        // add(id, method, path, handler) → Bool
        "add" => {
            if args.len() < 4 { return Err("router.add requiere (id, method, path, handler)".into()); }
            let id      = to_u64(&args[0])?;
            let method  = to_str(&args[1]).to_uppercase();
            let pattern = to_str(&args[2]);
            let handler = args[3].clone();
            with_router_mut(id, |r| {
                r.routes.push(Route { method, pattern, handler });
                Ok(EvalValue::Bool(true))
            })
        }

        // get/post/put/delete/patch(id, path, handler) → Bool
        "get" | "post" | "put" | "delete" | "patch" => {
            if args.len() < 3 { return Err(format!("router.{} requiere (id, path, handler)", function)); }
            let id      = to_u64(&args[0])?;
            let method  = function.to_uppercase();
            let pattern = to_str(&args[1]);
            let handler = args[2].clone();
            with_router_mut(id, |r| {
                r.routes.push(Route { method, pattern, handler });
                Ok(EvalValue::Bool(true))
            })
        }

        // use_middleware(id, fn) → Bool
        "use_middleware" => {
            if args.len() < 2 { return Err("router.use_middleware requiere (id, handler_fn)".into()); }
            let id = to_u64(&args[0])?;
            let mw = args[1].clone();
            with_router_mut(id, |r| { r.middlewares.push(mw); Ok(EvalValue::Bool(true)) })
        }

        // attach(id) → Bool  — activa el router para el próximo serve
        "attach" => {
            if args.is_empty() { return Err("router.attach requiere (id)".into()); }
            let id = to_u64(&args[0])?;
            if !store().lock().unwrap().contains_key(&id) {
                return Err(format!("router: ID {} no existe", id));
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
            if args.len() < 3 { return Err("router.match requiere (id, method, path)".into()); }
            let id     = to_u64(&args[0])?;
            let method = to_str(&args[1]).to_uppercase();
            let path   = to_str(&args[2]);
            let store  = store().lock().unwrap();
            let data   = store.get(&id).ok_or_else(|| format!("router: ID {} no existe", id))?;
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
            if args.is_empty() { return Err("router.routes requiere (id)".into()); }
            let id    = to_u64(&args[0])?;
            let store = store().lock().unwrap();
            let data  = store.get(&id).ok_or_else(|| format!("router: ID {} no existe", id))?;
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
            if args.is_empty() { return Err("router.clear requiere (id)".into()); }
            with_router_mut(to_u64(&args[0])?, |r| { r.routes.clear(); Ok(EvalValue::Bool(true)) })
        }

        // drop(id) → Bool
        "drop" => {
            if args.is_empty() { return Err("router.drop requiere (id)".into()); }
            let id = to_u64(&args[0])?;
            store().lock().unwrap().remove(&id);
            // Desactivar si era el activo
            let mut act = active_id().lock().unwrap();
            if *act == Some(id) { *act = None; }
            Ok(EvalValue::Bool(true))
        }

        f => Err(format!("router.{}() no existe", f)),
    }
}

//    Helpers                                                                    

fn with_router_mut<F>(id: u64, f: F) -> Result<EvalValue, String>
where F: FnOnce(&mut RouterData) -> Result<EvalValue, String>
{
    f(store().lock().unwrap()
        .get_mut(&id)
        .ok_or_else(|| format!("router: ID {} no existe", id))?)
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
        _ => Err("router: ID debe ser un Int positivo".into()),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
