//! Runtime OOP — JIT-5: DefineShape, GetAttr, SetAttr, IsInstance, PushSelf, CallMethod
//!
//! Las instancias son OrionVal con tag=TAG_INSTANCE, data_i → OrionInstance heap.
//! Los acts compilados se registran en METHOD_TABLE y se llaman via dispatch de puntero.

use std::cell::RefCell;
use std::collections::HashMap;

use super::runtime::{
    alloc_val, cstr_to_str, string_to_cptr, val_to_display, val_ref,
    ARG_BUF, TAG_BOOL, TAG_DICT, TAG_FLOAT, TAG_INT, TAG_LIST, TAG_NULL, TAG_STR,
};

pub const TAG_INSTANCE: u8 = 7;

//     Estructura de instancia

/// Instancia de un shape. Apuntada por `OrionVal.data_i`.
pub struct OrionInstance {
    pub shape_name: String,
    pub fields: Vec<(String, i64)>,
    pub parents: Vec<String>,
}

//     TLS de OOP

thread_local! {
    /// Pila de self (soporta llamadas a métodos anidadas).
    pub(crate) static SELF_STACK: RefCell<Vec<i64>> = RefCell::new(Vec::new());
    /// "ShapeName::act_name" → fn_ptr como i64.
    pub(crate) static METHOD_TABLE: RefCell<HashMap<String, i64>> = RefCell::new(HashMap::new());
    /// Campos registrados por shape: "ShapeName" → Vec<field_name>.
    pub(crate) static SHAPE_FIELDS: RefCell<HashMap<String, Vec<String>>> = RefCell::new(HashMap::new());
    /// Padres por shape: "ShapeName" → Vec<parent_name>.
    pub(crate) static SHAPE_PARENTS: RefCell<HashMap<String, Vec<String>>> = RefCell::new(HashMap::new());
}

//     Helpers internos

unsafe fn get_inst(val_ptr: i64) -> &'static OrionInstance {
    let oval = val_ref(val_ptr);
    &*(oval.data_i as *const OrionInstance)
}

unsafe fn get_inst_mut(val_ptr: i64) -> &'static mut OrionInstance {
    let oval = val_ref(val_ptr);
    &mut *(oval.data_i as *mut OrionInstance)
}

fn alloc_instance(inst: OrionInstance) -> i64 {
    let raw = Box::into_raw(Box::new(inst)) as i64;
    alloc_val(TAG_INSTANCE, raw, 0.0)
}

/// Llama un act compilado (fn ptr) con 0-8 argumentos desde un slice.
unsafe fn call_act_ptr(fn_ptr: i64, args: &[i64]) -> i64 {
    type F0 = extern "C" fn() -> i64;
    type F1 = extern "C" fn(i64) -> i64;
    type F2 = extern "C" fn(i64, i64) -> i64;
    type F3 = extern "C" fn(i64, i64, i64) -> i64;
    type F4 = extern "C" fn(i64, i64, i64, i64) -> i64;
    type F5 = extern "C" fn(i64, i64, i64, i64, i64) -> i64;
    type F6 = extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64;
    type F7 = extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64;
    type F8 = extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64;
    match args.len() {
        0 => std::mem::transmute::<i64, F0>(fn_ptr)(),
        1 => std::mem::transmute::<i64, F1>(fn_ptr)(args[0]),
        2 => std::mem::transmute::<i64, F2>(fn_ptr)(args[0], args[1]),
        3 => std::mem::transmute::<i64, F3>(fn_ptr)(args[0], args[1], args[2]),
        4 => std::mem::transmute::<i64, F4>(fn_ptr)(args[0], args[1], args[2], args[3]),
        5 => std::mem::transmute::<i64, F5>(fn_ptr)(args[0], args[1], args[2], args[3], args[4]),
        6 => std::mem::transmute::<i64, F6>(fn_ptr)(args[0], args[1], args[2], args[3], args[4], args[5]),
        7 => std::mem::transmute::<i64, F7>(fn_ptr)(args[0], args[1], args[2], args[3], args[4], args[5], args[6]),
        8 => std::mem::transmute::<i64, F8>(fn_ptr)(args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7]),
        _ => { eprintln!("[JIT] CallMethod: demasiados argumentos (max 8)"); std::process::exit(1) }
    }
}

//     Registro (llamado desde Rust antes de ejecutar)

/// Registra los campos de un shape para que `rt_create_instance` pueda pre-crearlos.
pub fn register_shape_info(shape_name: &str, fields: Vec<String>, parents: Vec<String>) {
    SHAPE_FIELDS.with(|sf| sf.borrow_mut().insert(shape_name.to_string(), fields));
    SHAPE_PARENTS.with(|sp| sp.borrow_mut().insert(shape_name.to_string(), parents));
}

/// Registra el puntero de función de un act compilado.
pub fn register_method(shape_name: &str, act_name: &str, fn_ptr: i64) {
    let key = format!("{}::{}", shape_name, act_name);
    METHOD_TABLE.with(|mt| mt.borrow_mut().insert(key, fn_ptr));
}

//     Instanciación

/// Crea una instancia con todos sus campos en null.
/// Llamada por `rt_create_instance_and_init` (que también corre el on_create).
#[no_mangle]
pub extern "C" fn rt_create_instance(shape_name_ptr: i64) -> i64 {
    unsafe {
        let shape_name = cstr_to_str(shape_name_ptr).to_string();
        let field_names = SHAPE_FIELDS.with(|sf| sf.borrow().get(&shape_name).cloned().unwrap_or_default());
        let parents     = SHAPE_PARENTS.with(|sp| sp.borrow().get(&shape_name).cloned().unwrap_or_default());
        let fields = field_names.iter().map(|f| (f.clone(), alloc_val(TAG_NULL, 0, 0.0))).collect();
        alloc_instance(OrionInstance { shape_name, fields, parents })
    }
}

/// Crea la instancia y, si existe on_create, lo invoca con los N args del ARG_BUF.
#[no_mangle]
pub extern "C" fn rt_create_instance_and_init(shape_name_ptr: i64, n_args: i64) -> i64 {
    let inst_ptr = rt_create_instance(shape_name_ptr);
    unsafe {
        let shape_name = cstr_to_str(shape_name_ptr).to_string();
        let oc_key = format!("{}::on_create", shape_name);
        let fn_ptr = METHOD_TABLE.with(|mt| mt.borrow().get(&oc_key).copied());
        if let Some(fp) = fn_ptr {
            let args: Vec<i64> = ARG_BUF.with(|b| {
                let mut buf = b.borrow_mut();
                let n = n_args as usize;
                let take = n.min(buf.len());
                buf.drain(..take).collect()
            });
            SELF_STACK.with(|ss| ss.borrow_mut().push(inst_ptr));
            call_act_ptr(fp, &args);
            SELF_STACK.with(|ss| ss.borrow_mut().pop());
        }
    }
    inst_ptr
}

//     Atributos

/// `obj.attr` — funciona en Instance (campo) y Dict (clave).
#[no_mangle]
pub extern "C" fn rt_get_attr(obj: i64, name_ptr: i64) -> i64 {
    unsafe {
        let oval = val_ref(obj);
        match oval.tag {
            TAG_INSTANCE => {
                let inst = get_inst(obj);
                let name = cstr_to_str(name_ptr);
                for (k, v) in &inst.fields {
                    if k == name { return *v; }
                }
                eprintln!("[JIT] GetAttr '{}': campo no encontrado en '{}'", name, inst.shape_name);
                std::process::exit(1)
            }
            TAG_DICT => {
                let entries = &*(oval.data_i as *const Vec<(String, i64)>);
                let name = cstr_to_str(name_ptr);
                for (k, v) in entries {
                    if k == name { return *v; }
                }
                eprintln!("[JIT] GetAttr '{}': clave no encontrada en dict/módulo", name);
                std::process::exit(1)
            }
            _ => {
                eprintln!("[JIT] GetAttr: no es instancia ni dict (tag={})", oval.tag);
                std::process::exit(1)
            }
        }
    }
}

/// `obj.attr = val` — muta la instancia en-lugar. Retorna el puntero al objeto (mismo).
#[no_mangle]
pub extern "C" fn rt_set_attr(obj: i64, name_ptr: i64, val: i64) {
    unsafe {
        let oval = val_ref(obj);
        if oval.tag != TAG_INSTANCE {
            eprintln!("[JIT] SetAttr: no es una instancia (tag={})", oval.tag);
            std::process::exit(1);
        }
        let inst = get_inst_mut(obj);
        let name = cstr_to_str(name_ptr).to_string();
        for (k, v) in &mut inst.fields {
            if *k == name { *v = val; return; }
        }
        inst.fields.push((name, val));
    }
}

//     IsInstance

/// `obj is ShapeName` → bool.
#[no_mangle]
pub extern "C" fn rt_is_instance(obj: i64, shape_name_ptr: i64) -> i64 {
    unsafe {
        let oval = val_ref(obj);
        if oval.tag != TAG_INSTANCE {
            return alloc_val(TAG_BOOL, 0, 0.0);
        }
        let inst = get_inst(obj);
        let target = cstr_to_str(shape_name_ptr);
        let matches = inst.shape_name == target || inst.parents.iter().any(|p| p == target);
        alloc_val(TAG_BOOL, if matches { 1 } else { 0 }, 0.0)
    }
}

//     Self TLS

/// Empuja self al stack de frames (se llama al iniciar un método de instancia).
#[no_mangle]
pub extern "C" fn rt_push_self(inst_ptr: i64) {
    SELF_STACK.with(|ss| ss.borrow_mut().push(inst_ptr));
}

/// Elimina el self del top del stack (se llama al salir de un método).
#[no_mangle]
pub extern "C" fn rt_pop_self() {
    SELF_STACK.with(|ss| { ss.borrow_mut().pop(); });
}

/// Devuelve el self activo (instrucción PushSelf / `me`).
#[no_mangle]
pub extern "C" fn rt_get_current_self() -> i64 {
    SELF_STACK.with(|ss| ss.borrow().last().copied().unwrap_or_else(|| alloc_val(TAG_NULL, 0, 0.0)))
}

/// Lee un campo del self activo — usado para inicializar vars de campo en act bodies.
#[no_mangle]
pub extern "C" fn rt_get_self_field(name_ptr: i64) -> i64 {
    let self_ptr = SELF_STACK.with(|ss| ss.borrow().last().copied());
    match self_ptr {
        Some(p) => rt_get_attr(p, name_ptr),
        None    => alloc_val(TAG_NULL, 0, 0.0),
    }
}

/// Escribe un campo en el self activo — sincronización en `StoreVar` de campo.
#[no_mangle]
pub extern "C" fn rt_set_self_field(name_ptr: i64, val: i64) {
    let self_ptr = SELF_STACK.with(|ss| ss.borrow().last().copied());
    if let Some(p) = self_ptr {
        rt_set_attr(p, name_ptr, val);
    }
}

//     Dispatch de métodos

/// Dispatch unificado: Str / List / Dict builtins + Instance acts.
/// Los N args ya están en ARG_BUF (empujados por el compilador con rt_push_arg).
#[no_mangle]
pub extern "C" fn rt_call_method(obj: i64, name_ptr: i64, n_args: i64) -> i64 {
    unsafe {
        let n = n_args as usize;
        let args: Vec<i64> = ARG_BUF.with(|b| {
            let mut buf = b.borrow_mut();
            let take = n.min(buf.len());
            buf.drain(..take).collect()
        });
        let oval = val_ref(obj);
        match oval.tag {
            TAG_STR      => call_method_str(oval.data_i, name_ptr, &args),
            TAG_LIST     => call_method_list(oval.data_i, name_ptr, &args),
            TAG_DICT     => call_method_dict(oval.data_i, name_ptr, &args),
            TAG_INSTANCE => call_method_instance(obj, name_ptr, &args),
            _ => {
                eprintln!("[JIT] CallMethod: tipo no soportado (tag={})", oval.tag);
                std::process::exit(1)
            }
        }
    }
}

//     Métodos builtin de String

unsafe fn call_method_str(data_i: i64, name_ptr: i64, args: &[i64]) -> i64 {
    let s = cstr_to_str(data_i).to_string();
    let name = cstr_to_str(name_ptr);
    match name {
        "len"        => alloc_val(TAG_INT, s.chars().count() as i64, 0.0),
        "is_empty"   => alloc_val(TAG_BOOL, if s.trim().is_empty() { 1 } else { 0 }, 0.0),
        "trim"       => alloc_val(TAG_STR, string_to_cptr(s.trim().to_string()), 0.0),
        "trim_start" => alloc_val(TAG_STR, string_to_cptr(s.trim_start().to_string()), 0.0),
        "trim_end"   => alloc_val(TAG_STR, string_to_cptr(s.trim_end().to_string()), 0.0),
        "lower"      => alloc_val(TAG_STR, string_to_cptr(s.to_lowercase()), 0.0),
        "upper"      => alloc_val(TAG_STR, string_to_cptr(s.to_uppercase()), 0.0),
        "reverse"    => alloc_val(TAG_STR, string_to_cptr(s.chars().rev().collect()), 0.0),
        "contains" => {
            let needle = args.first().map(|&p| val_to_display(val_ref(p))).unwrap_or_default();
            alloc_val(TAG_BOOL, if s.contains(&needle as &str) { 1 } else { 0 }, 0.0)
        }
        "starts_with" => {
            let prefix = args.first().map(|&p| val_to_display(val_ref(p))).unwrap_or_default();
            alloc_val(TAG_BOOL, if s.starts_with(&prefix as &str) { 1 } else { 0 }, 0.0)
        }
        "ends_with" => {
            let suffix = args.first().map(|&p| val_to_display(val_ref(p))).unwrap_or_default();
            alloc_val(TAG_BOOL, if s.ends_with(&suffix as &str) { 1 } else { 0 }, 0.0)
        }
        "split" => {
            let sep = args.first().map(|&p| val_to_display(val_ref(p))).unwrap_or_default();
            let parts: Vec<i64> = s.split(&sep as &str)
                .map(|p| alloc_val(TAG_STR, string_to_cptr(p.to_string()), 0.0))
                .collect();
            let raw = Box::into_raw(Box::new(parts)) as i64;
            alloc_val(TAG_LIST, raw, 0.0)
        }
        "replace" => {
            let from = args.first().map(|&p| val_to_display(val_ref(p))).unwrap_or_default();
            let to   = args.get(1).map(|&p| val_to_display(val_ref(p))).unwrap_or_default();
            alloc_val(TAG_STR, string_to_cptr(s.replace(&from as &str, &to)), 0.0)
        }
        "repeat" => {
            let n = args.first().map(|&p| { let v = val_ref(p); if v.tag == TAG_INT { v.data_i as usize } else { 1 } }).unwrap_or(1);
            alloc_val(TAG_STR, string_to_cptr(s.repeat(n)), 0.0)
        }
        "slice" => {
            let start = args.first().map(|&p| { let v = val_ref(p); if v.tag == TAG_INT { v.data_i as usize } else { 0 } }).unwrap_or(0);
            let end   = args.get(1).map(|&p| { let v = val_ref(p); if v.tag == TAG_INT { v.data_i as usize } else { s.chars().count() } }).unwrap_or(s.chars().count());
            let sliced: String = s.chars().skip(start).take(end.saturating_sub(start)).collect();
            alloc_val(TAG_STR, string_to_cptr(sliced), 0.0)
        }
        "index_of" | "find" => {
            let needle = args.first().map(|&p| val_to_display(val_ref(p))).unwrap_or_default();
            let idx = s.find(&needle as &str).map(|i| i as i64).unwrap_or(-1);
            alloc_val(TAG_INT, idx, 0.0)
        }
        "to_int" | "parse_int" => {
            alloc_val(TAG_INT, s.trim().parse::<i64>().unwrap_or(0), 0.0)
        }
        "to_float" | "parse_float" => {
            alloc_val(TAG_FLOAT, 0, s.trim().parse::<f64>().unwrap_or(0.0))
        }
        _ => {
            eprintln!("[JIT] String no tiene método '{}'", name);
            std::process::exit(1)
        }
    }
}

//     Métodos builtin de List

unsafe fn call_method_list(data_i: i64, name_ptr: i64, args: &[i64]) -> i64 {
    let items = &*(data_i as *const Vec<i64>);
    let name  = cstr_to_str(name_ptr);
    match name {
        "len"      => alloc_val(TAG_INT, items.len() as i64, 0.0),
        "is_empty" => alloc_val(TAG_BOOL, if items.is_empty() { 1 } else { 0 }, 0.0),
        "first"    => items.first().copied().unwrap_or_else(|| alloc_val(TAG_NULL, 0, 0.0)),
        "last"     => items.last().copied().unwrap_or_else(|| alloc_val(TAG_NULL, 0, 0.0)),
        "push" | "append" => {
            let item = args.first().copied().unwrap_or_else(|| alloc_val(TAG_NULL, 0, 0.0));
            let mut new_items = items.clone();
            new_items.push(item);
            let raw = Box::into_raw(Box::new(new_items)) as i64;
            alloc_val(TAG_LIST, raw, 0.0)
        }
        "reverse" => {
            let mut new_items = items.clone();
            new_items.reverse();
            let raw = Box::into_raw(Box::new(new_items)) as i64;
            alloc_val(TAG_LIST, raw, 0.0)
        }
        "contains" => {
            if let Some(&item_ptr) = args.first() {
                let target = val_to_display(val_ref(item_ptr));
                let found = items.iter().any(|&p| val_to_display(val_ref(p)) == target);
                alloc_val(TAG_BOOL, if found { 1 } else { 0 }, 0.0)
            } else {
                alloc_val(TAG_BOOL, 0, 0.0)
            }
        }
        "join" => {
            let sep = args.first().map(|&p| val_to_display(val_ref(p))).unwrap_or_default();
            let joined = items.iter().map(|&p| val_to_display(val_ref(p))).collect::<Vec<_>>().join(&sep);
            alloc_val(TAG_STR, string_to_cptr(joined), 0.0)
        }
        "sum" => {
            let mut total = 0.0f64;
            let mut is_int = true;
            for &p in items {
                let v = val_ref(p);
                match v.tag {
                    TAG_INT   => total += v.data_i as f64,
                    TAG_FLOAT => { total += v.data_f; is_int = false; }
                    _         => {}
                }
            }
            if is_int { alloc_val(TAG_INT, total as i64, 0.0) }
            else      { alloc_val(TAG_FLOAT, 0, total) }
        }
        "sort" => {
            let mut new_items = items.clone();
            new_items.sort_by(|&a, &b| {
                let av = val_ref(a); let bv = val_ref(b);
                match (av.tag, bv.tag) {
                    (TAG_INT, TAG_INT)     => av.data_i.cmp(&bv.data_i),
                    (TAG_FLOAT, TAG_FLOAT) => av.data_f.partial_cmp(&bv.data_f).unwrap_or(std::cmp::Ordering::Equal),
                    (TAG_STR, TAG_STR)     => cstr_to_str(av.data_i).cmp(cstr_to_str(bv.data_i)),
                    _                      => std::cmp::Ordering::Equal,
                }
            });
            let raw = Box::into_raw(Box::new(new_items)) as i64;
            alloc_val(TAG_LIST, raw, 0.0)
        }
        "min" => {
            items.iter().copied().reduce(|a, b| {
                let av = val_ref(a); let bv = val_ref(b);
                match (av.tag, bv.tag) {
                    (TAG_INT, TAG_INT) => if av.data_i <= bv.data_i { a } else { b },
                    _ => a,
                }
            }).unwrap_or_else(|| alloc_val(TAG_NULL, 0, 0.0))
        }
        "max" => {
            items.iter().copied().reduce(|a, b| {
                let av = val_ref(a); let bv = val_ref(b);
                match (av.tag, bv.tag) {
                    (TAG_INT, TAG_INT) => if av.data_i >= bv.data_i { a } else { b },
                    _ => a,
                }
            }).unwrap_or_else(|| alloc_val(TAG_NULL, 0, 0.0))
        }
        "pop" => {
            if items.is_empty() { return alloc_val(TAG_NULL, 0, 0.0); }
            *items.last().unwrap()
        }
        _ => { eprintln!("[JIT] List no tiene método '{}'", name); std::process::exit(1) }
    }
}

//     Métodos builtin de Dict

unsafe fn call_method_dict(data_i: i64, name_ptr: i64, args: &[i64]) -> i64 {
    let entries = &*(data_i as *const Vec<(String, i64)>);
    let name    = cstr_to_str(name_ptr);
    match name {
        "len"      => alloc_val(TAG_INT, entries.len() as i64, 0.0),
        "is_empty" => alloc_val(TAG_BOOL, if entries.is_empty() { 1 } else { 0 }, 0.0),
        "keys"     => {
            let keys: Vec<i64> = entries.iter()
                .map(|(k, _)| alloc_val(TAG_STR, string_to_cptr(k.clone()), 0.0))
                .collect();
            let raw = Box::into_raw(Box::new(keys)) as i64;
            alloc_val(TAG_LIST, raw, 0.0)
        }
        "values"   => {
            let vals: Vec<i64> = entries.iter().map(|(_, v)| *v).collect();
            let raw = Box::into_raw(Box::new(vals)) as i64;
            alloc_val(TAG_LIST, raw, 0.0)
        }
        "contains" | "has_key" => {
            let key = args.first().map(|&p| val_to_display(val_ref(p))).unwrap_or_default();
            let found = entries.iter().any(|(k, _)| k == &key);
            alloc_val(TAG_BOOL, if found { 1 } else { 0 }, 0.0)
        }
        "get" => {
            let key = args.first().map(|&p| val_to_display(val_ref(p))).unwrap_or_default();
            entries.iter().find(|(k, _)| k == &key).map(|(_, v)| *v)
                .unwrap_or_else(|| alloc_val(TAG_NULL, 0, 0.0))
        }
        _ => { eprintln!("[JIT] Dict no tiene método '{}'", name); std::process::exit(1) }
    }
}

//     Dispatch de acts de instancia

unsafe fn call_method_instance(obj: i64, name_ptr: i64, args: &[i64]) -> i64 {
    let inst      = get_inst(obj);
    let method    = cstr_to_str(name_ptr);
    let key       = format!("{}::{}", inst.shape_name, method);
    let fn_ptr    = METHOD_TABLE.with(|mt| mt.borrow().get(&key).copied());
    match fn_ptr {
        Some(fp) => {
            SELF_STACK.with(|ss| ss.borrow_mut().push(obj));
            let result = call_act_ptr(fp, args);
            SELF_STACK.with(|ss| { ss.borrow_mut().pop(); });
            result
        }
        None => {
            eprintln!("[JIT] Método '{}' no encontrado en '{}'", method, inst.shape_name);
            std::process::exit(1)
        }
    }
}

//     Display de instancias (para rt_show, rt_add, etc.)

pub fn instance_to_display(val_ptr: i64) -> String {
    unsafe {
        let inst = get_inst(val_ptr);
        let parts: Vec<String> = inst.fields.iter()
            .map(|(k, v)| format!("{}: {}", k, val_to_display(val_ref(*v))))
            .collect();
        format!("{}{{ {} }}", inst.shape_name, parts.join(", "))
    }
}
