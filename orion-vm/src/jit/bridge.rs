//! Puente JIT ↔ VM para módulos `.orx`.
//!
//! El JIT compila a nativo el programa principal, pero las funciones de un
//! paquete `.orx` (`use "packages/math"`) no forman parte de esa unidad de
//! compilación. En vez de dejarlas sin soporte, este puente:
//!
//! 1. Compila el módulo y ejecuta su cuerpo en una sub-VM para obtener sus
//!    variables/constantes globales.
//! 2. Construye un namespace (TAG_DICT del JIT) donde cada constante se
//!    convierte a `OrionVal` y cada función queda como un marcador `TAG_VMFN`.
//! 3. Cuando el código JIT invoca `mod.func(args)`, el marcador ejecuta la
//!    función vía la VM, convirtiendo los argumentos y el resultado entre la
//!    representación del JIT (`OrionVal`) y la de la VM (`Value`).
//!
//! Cada módulo corre en su propio contexto aislado (funciones y globales en
//! nombres simples), así que recursión, helpers internos y `use` dentro del
//! paquete resuelven sin prefijos.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::bytecode::{FunctionDef, ShapeDef};
use crate::instruction::Instruction;
use crate::value::Value;
use crate::vm::VM;

use super::runtime::{
    alloc_val, cstr_to_str, string_to_cptr, val_ref, OrionVal, TAG_BOOL, TAG_DICT, TAG_FLOAT,
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

/// Carga un módulo `.orx` para el JIT: lo compila, ejecuta su cuerpo en una
/// sub-VM para obtener globales, y devuelve un namespace TAG_DICT con cada
/// función como marcador TAG_VMFN y cada constante convertida a OrionVal.
pub fn load_orx_module_jit(path: &str) -> i64 {
    use crate::codegen::compile;
    use crate::lexer::lex;
    use crate::parser::parse;

    let fail = |msg: String| -> ! {
        eprintln!("[JIT] {}", msg);
        std::process::exit(1)
    };

    let src = std::fs::read_to_string(path)
        .unwrap_or_else(|e| fail(format!("No se pudo leer '{}': {}", path, e)));
    let tokens = lex(&src).unwrap_or_else(|e| fail(format!("lex '{}': {:?}", path, e)));
    let ast = parse(tokens).unwrap_or_else(|e| fail(format!("parse '{}': {:?}", path, e)));
    let bc = compile(ast).unwrap_or_else(|e| fail(format!("compile '{}': {:?}", path, e)));

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
    let (ctx, fn_name) = VM_FN_REFS.with(|r| {
        let v = r.borrow();
        v.get(idx as usize).cloned()
    })
    .unwrap_or_else(|| {
        eprintln!("[JIT] referencia a función de módulo inválida ({})", idx);
        std::process::exit(1)
    });

    let vm_args: Vec<Value> = args.iter().map(|&p| orion_to_value(p)).collect();

    match VM::call_named(
        ctx.functions.clone(),
        ctx.shapes.clone(),
        ctx.globals.clone(),
        &fn_name,
        vm_args,
    ) {
        Ok(v) => value_to_orion(&v),
        Err(e) => {
            eprintln!("[JIT] error ejecutando '{}': {}", fn_name, e);
            std::process::exit(1)
        }
    }
}

//     Conversión OrionVal → Value

pub fn orion_to_value(ptr: i64) -> Value {
    unsafe {
        let v: &OrionVal = val_ref(ptr);
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
                let mut map: IndexMap<String, Value> = IndexMap::new();
                for (k, p) in entries {
                    // Las funciones de módulo (TAG_VMFN) no se convierten a Value.
                    if val_ref(*p).tag == TAG_VMFN {
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

/// True si el builtin (`str`, `len`, `push`, `range`, …) puede despacharse a la
/// VM desde el JIT. Se EXCLUYEN los de orden superior (`map`/`filter`/`reduce`/
/// `find`: reciben closures que no cruzan el puente) y los dependientes de estado
/// o E/S (`input`). Ampliar con cuidado: cada nombre debe convertir limpio entre
/// `OrionVal` y `Value`.
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

/// Despacha un builtin del VM desde código JIT. Los argumentos vienen en el
/// ARG_BUF (empujados con `rt_push_arg`, `elem_0` primero). Devuelve el
/// resultado como `OrionVal`.
///
/// Para builtins que mutan su primer argumento (`push`/`pop`/`reverse`/`sort`),
/// reescribe el backing `Vec<i64>` del `OrionVal`-lista original **in-place**, de
/// modo que todos los alias (que comparten ese puntero) vean el cambio — igual
/// que la semántica por referencia de la VM.
#[no_mangle]
pub extern "C" fn rt_call_builtin(name_ptr: i64, argc: i64) -> i64 {
    let name = unsafe { cstr_to_str(name_ptr) }.to_string();
    let argc = argc as usize;
    let arg_ptrs = super::runtime::drain_arg_buf(argc);
    let vm_args: Vec<Value> = arg_ptrs.iter().map(|&p| orion_to_value(p)).collect();

    // Handle Rc del 1er arg-lista (si el builtin muta), para el write-back.
    // Clonar un `Value::List` clona el `Rc` → comparte el mismo backing que
    // mutará `call_builtin`.
    let list_handle: Option<Value> = if mutates_first_arg(&name) {
        match vm_args.first() {
            Some(v @ Value::List(_)) => Some(v.clone()),
            _ => None,
        }
    } else {
        None
    };

    let vm = VM::new(
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
            let ov = val_ref(first_ptr);
            if ov.tag == TAG_LIST {
                let new_elems: Vec<i64> = rc.borrow().iter().map(value_to_orion).collect();
                *(ov.data_i as *mut Vec<i64>) = new_elems;
            }
        }
    }

    match result {
        Ok(Some(v)) => value_to_orion(&v),
        Ok(None) => alloc_val(TAG_NULL, 0, 0.0),
        Err(e) => {
            eprintln!("[JIT] builtin '{}': {}", name, e);
            std::process::exit(1)
        }
    }
}
