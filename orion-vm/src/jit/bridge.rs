//! Puente JIT ↔ VM para módulos `.orx`: el módulo se ejecuta en una sub-VM, sus
//! constantes pasan a `OrionVal` y sus funciones quedan como marcadores
//! `TAG_VMFN` que se ejecutan en la VM al llamarlos.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::bytecode::{FunctionDef, ShapeDef};
use crate::instruction::Instruction;
use crate::value::Value;
use crate::vm::VM;

use super::runtime::{
    alloc_val, fallar, cstr_to_str, string_to_cptr, decode_val, OrionVal, TAG_BOOL, TAG_DICT, TAG_FLOAT,
    TAG_INT, TAG_LIST, TAG_NULL, TAG_STR,
};

/// Marcador de una función de módulo `.orx` ejecutable vía VM.
/// `data_i` = índice en `VM_FN_REFS`.
pub const TAG_VMFN: u8 = 12;

/// Contexto compilado de un módulo `.orx`: funciones, shapes y globales.
pub struct ModuleCtx {
    pub functions: IndexMap<String, FunctionDef>,
    pub shapes: IndexMap<String, ShapeDef>,
    pub globals: IndexMap<String, Value>,
}

thread_local! {
    /// Referencias a funciones de módulo: (contexto, nombre de la función).
    static VM_FN_REFS: RefCell<Vec<(Rc<ModuleCtx>, String)>> = RefCell::new(Vec::new());
}

pub fn load_orx_module_jit(path: &str) -> i64 {
    use crate::codegen::compile;
    use crate::lexer::lex;
    use crate::parser::parse;

    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return fallar(format!("Could not read '{}': {}", path, e)),
    };
    let bc = match lex(&src).map_err(|e| format!("lex '{}': {:?}", path, e))
        .and_then(|t| parse(t).map_err(|e| format!("parse '{}': {:?}", path, e)))
        .and_then(|a| compile(a).map_err(|e| format!("compile '{}': {:?}", path, e)))
    {
        Ok(bc) => bc,
        Err(e) => return fallar(e),
    };

    let sub_vm = VM::new(
        bc.main.clone(),
        bc.lines.clone(),
        bc.functions.clone(),
        bc.shapes.clone(),
        bc.extern_fns.clone(),
    );
    let globals = sub_vm.into_globals();

    let ctx = Rc::new(ModuleCtx {
        functions: bc.functions.clone(),
        shapes: bc.shapes.clone(),
        globals,
    });

    let mut entries: Vec<(String, i64)> = Vec::new();
    for fname in ctx.functions.keys() {
        entries.push((fname.clone(), make_vmfn(&ctx, fname)));
    }
    for (k, v) in &ctx.globals {
        entries.push((k.clone(), value_to_orion(v)));
    }
    let raw = Box::into_raw(Box::new(entries)) as i64;
    alloc_val(TAG_DICT, raw, 0.0)
}

/// Registra una función de módulo y devuelve un `OrionVal` TAG_VMFN que la apunta.
pub fn make_vmfn(ctx: &Rc<ModuleCtx>, fn_name: &str) -> i64 {
    let idx = VM_FN_REFS.with(|r| {
        let mut v = r.borrow_mut();
        v.push((Rc::clone(ctx), fn_name.to_string()));
        v.len() - 1
    });
    alloc_val(TAG_VMFN, idx as i64, 0.0)
}

/// Invoca una función de módulo (marcador TAG_VMFN) con los args dados (OrionVal*).
/// Devuelve el resultado convertido a OrionVal.
pub fn call_vmfn(idx: i64, args: &[i64]) -> i64 {
    let (ctx, fn_name) = match VM_FN_REFS.with(|r| r.borrow().get(idx as usize).cloned()) {
        Some(r) => r,
        None => return fallar(format!("JIT: invalid module function reference ({})", idx)),
    };

    let vm_args: Vec<Value> = args.iter().map(|&p| orion_to_value(p)).collect();

    match VM::call_named(
        ctx.functions.clone(),
        ctx.shapes.clone(),
        ctx.globals.clone(),
        &fn_name,
        vm_args,
    ) {
        Ok(v) => value_to_orion(&v),
        Err(e) => fallar(e),
    }
}

//     Conversión OrionVal → Value

pub fn orion_to_value(ptr: i64) -> Value {
    unsafe {
        let v: &OrionVal = &decode_val(ptr);
        match v.tag {
            TAG_NULL => Value::Null,
            TAG_INT => Value::Int(v.data_i),
            TAG_FLOAT => Value::Float(v.data_f),
            TAG_BOOL => Value::Bool(v.data_i != 0),
            TAG_STR => Value::Str(cstr_to_str(v.data_i).to_string()),
            TAG_LIST => {
                let items = &*(v.data_i as *const Vec<i64>);
                Value::list(items.iter().map(|&p| orion_to_value(p)).collect())
            }
            TAG_DICT => {
                let entries = &*(v.data_i as *const Vec<(String, i64)>);

                if let Some((_, marca)) = entries.iter().find(|(k, _)| k == "__native_module__") {
                    return Value::Module(crate::jit::runtime::val_to_display(&decode_val(*marca)));
                }

                let mut map: IndexMap<String, Value> = IndexMap::new();
                for (k, p) in entries {
                    // Las funciones de módulo (TAG_VMFN) no se convierten a Value.
                    if decode_val(*p).tag == TAG_VMFN {
                        continue;
                    }
                    map.insert(k.clone(), orion_to_value(*p));
                }
                Value::Dict(map)
            }
            _ => Value::Null,
        }
    }
}

//     Conversión Value → OrionVal

pub fn value_to_orion(v: &Value) -> i64 {
    match v {
        Value::Null => alloc_val(TAG_NULL, 0, 0.0),
        Value::Int(n) => alloc_val(TAG_INT, *n, 0.0),
        Value::Float(f) => alloc_val(TAG_FLOAT, 0, *f),
        Value::Bool(b) => alloc_val(TAG_BOOL, if *b { 1 } else { 0 }, 0.0),
        Value::Str(s) => alloc_val(TAG_STR, string_to_cptr(s.clone()), 0.0),
        Value::List(items) => {
            let elems: Vec<i64> = items.borrow().iter().map(value_to_orion).collect();
            let raw = Box::into_raw(Box::new(elems)) as i64;
            alloc_val(TAG_LIST, raw, 0.0)
        }
        Value::Dict(map) => {
            let entries: Vec<(String, i64)> =
                map.iter().map(|(k, val)| (k.clone(), value_to_orion(val))).collect();
            let raw = Box::into_raw(Box::new(entries)) as i64;
            alloc_val(TAG_DICT, raw, 0.0)
        }
        // Cierres, módulos nativos, instancias, etc. no se puentean.
        _ => alloc_val(TAG_NULL, 0, 0.0),
    }
}

//     Builtins vía puente VM

pub fn is_jit_builtin(name: &str) -> bool {
    matches!(name,
        // conversión / tipos
        "str" | "int" | "float" | "bool" | "type" |
        // numéricos
        "abs" | "sqrt" | "floor" | "ceil" | "round" | "pow" | "factorial" |
        "min" | "max" | "sum" |
        // secuencias (lectura)
        "len" | "range" | "first" | "last" | "contains" | "is_empty" |
        "get" | "slice" | "join" | "keys" | "values" | "has_key" | "repeat" |
        // secuencias (mutan su 1er argumento in-place)
        "push" | "append" | "pop" | "reverse" | "sort" |
        // strings
        "upper" | "lower" | "trim" | "replace" | "split" | "lines" |
        "starts_with" | "ends_with"
    )
}

/// True si el builtin muta in-place su primer argumento (una lista).
fn mutates_first_arg(name: &str) -> bool {
    matches!(name, "push" | "append" | "pop" | "reverse" | "sort")
}

/// Builtins de lista sobre el `Vec` del JIT, sin copiarlo a la VM (eso hacía
/// cada `push` O(n)). Misma semántica que `VM::call_builtin`.
fn builtin_directo(name: &str, args: &[i64]) -> Option<i64> {
    let primero = *args.first()?;
    let v = unsafe { decode_val(primero) };
    let null = || alloc_val(TAG_NULL, 0, 0.0);
    match (v.tag, name, args.len()) {
        (TAG_STR, "len", 1) => {
            let n = unsafe { cstr_to_str(v.data_i) }.len();
            Some(alloc_val(TAG_INT, n as i64, 0.0))
        }
        (TAG_DICT, "len", 1) => {
            let n = unsafe { &*(v.data_i as *const Vec<(String, i64)>) }.len();
            Some(alloc_val(TAG_INT, n as i64, 0.0))
        }
        (TAG_LIST, _, _) => {
            let items = unsafe { &mut *(v.data_i as *mut Vec<i64>) };
            match (name, args.len()) {
                ("len", 1) => Some(alloc_val(TAG_INT, items.len() as i64, 0.0)),
                ("push" | "append", 2) => { items.push(args[1]); Some(primero) }
                // Como la VM: devuelve [elemento, lista].
                ("pop", 1) => {
                    let item = items.pop().unwrap_or_else(null);
                    let par = Box::into_raw(Box::new(vec![item, primero])) as i64;
                    Some(alloc_val(TAG_LIST, par, 0.0))
                }
                ("first", 1) => Some(items.first().copied().unwrap_or_else(null)),
                ("last", 1) => Some(items.last().copied().unwrap_or_else(null)),
                _ => None,
            }
        }
        _ => None,
    }
}

#[no_mangle]
pub extern "C" fn rt_call_builtin(name_ptr: i64, argc: i64) -> i64 {
    let name = unsafe { cstr_to_str(name_ptr) };
    let argc = argc as usize;
    let arg_ptrs = super::runtime::drain_arg_buf(argc);
    if let Some(r) = builtin_directo(name, &arg_ptrs) { return r; }
    let name = name.to_string();
    let vm_args: Vec<Value> = arg_ptrs.iter().map(|&p| orion_to_value(p)).collect();

    let list_handle: Option<Value> = if mutates_first_arg(&name) {
        match vm_args.first() {
            Some(v @ Value::List(_)) => Some(v.clone()),
            _ => None,
        }
    } else {
        None
    };

    let mut vm = VM::new(
        vec![Instruction::Halt],
        vec![0],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let result = vm.call_builtin(&name, vm_args);

    // Write-back: reflejar la mutación en el OrionVal-lista original.
    if let Some(Value::List(rc)) = list_handle {
        let first_ptr = arg_ptrs[0];
        unsafe {
            let ov = decode_val(first_ptr);
            if ov.tag == TAG_LIST {
                let new_elems: Vec<i64> = rc.borrow().iter().map(value_to_orion).collect();
                *(ov.data_i as *mut Vec<i64>) = new_elems;
            }
        }
    }

    match result {
        Ok(Some(v)) => value_to_orion(&v),
        Ok(None) => alloc_val(TAG_NULL, 0, 0.0),
        Err(e) => fallar(e),
    }
}
