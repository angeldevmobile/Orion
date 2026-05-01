//! Runtime Orion JIT — Fase JIT-4: ReadInput, ReadFile, WriteFile, ReadEnv, UseModule
//!
//! OrionVal: valor boxeado en heap que representa cualquier valor Orion.
//! Todas las funciones de runtime reciben y devuelven punteros como i64.
//! Fase JIT-5 añadirá: TAG_SHAPE, TAG_CLOSURE, TAG_TASK.

use std::cell::RefCell;
use std::io::{self, BufRead, Write as IoWrite};

//     Tags                                                                     

pub const TAG_NULL:  u8 = 0;
pub const TAG_INT:   u8 = 1;
pub const TAG_FLOAT: u8 = 2;
pub const TAG_BOOL:  u8 = 3;
pub const TAG_STR:   u8 = 4;
pub const TAG_LIST:  u8 = 5;  // data_i = Box::into_raw(Box<Vec<i64>>)
pub const TAG_DICT:  u8 = 6;  // data_i = Box::into_raw(Box<Vec<(String, i64)>>)
// Reservados JIT-5:
// pub const TAG_SHAPE:   u8 = 7;
// pub const TAG_CLOSURE: u8 = 8;
// pub const TAG_TASK:    u8 = 9;

// Buffer thread-local para pasar N argumentos variádicos a MakeList/MakeDict.
thread_local! {
    static ARG_BUF: RefCell<Vec<i64>> = RefCell::new(Vec::new());
}

// Error activo para attempt/handle — almacena el OrionVal* del mensaje.
thread_local! {
    static ORION_ERROR: RefCell<Option<i64>> = RefCell::new(None);
}

//     OrionVal                                                                 

/// Valor Orion boxeado en heap. Tamaño fijo: 24 bytes.
/// Siempre pasado como puntero crudo (i64) en el ABI del JIT.
///
/// Distribución de campos:
/// - INT:   data_i = valor i64
/// - BOOL:  data_i = 0 (no) ó 1 (yes)
/// - STR:   data_i = puntero a bytes UTF-8 terminados en '\0'
/// - FLOAT: data_f = valor f64  (data_i sin uso)
/// - NULL:  ambos = 0
#[repr(C)]
pub struct OrionVal {
    pub tag:    u8,
    pub _pad:   [u8; 7],
    pub data_i: i64,
    pub data_f: f64,
}

//     Helpers internos                                                         

fn alloc_val(tag: u8, data_i: i64, data_f: f64) -> i64 {
    Box::into_raw(Box::new(OrionVal { tag, _pad: [0; 7], data_i, data_f })) as i64
}

unsafe fn val_ref(ptr: i64) -> &'static OrionVal {
    &*(ptr as *const OrionVal)
}

unsafe fn cstr_to_str(ptr: i64) -> &'static str {
    let p = ptr as *const u8;
    if p.is_null() { return ""; }
    let mut len = 0;
    while *p.add(len) != 0 { len += 1; }
    let slice = std::slice::from_raw_parts(p, len);
    std::str::from_utf8_unchecked(slice)
}

fn string_to_cptr(s: String) -> i64 {
    let mut bytes = s.into_bytes();
    bytes.push(0);
    let boxed = bytes.into_boxed_slice();
    Box::into_raw(boxed) as *mut u8 as i64
}

fn val_to_display(v: &OrionVal) -> String {
    match v.tag {
        TAG_INT   => v.data_i.to_string(),
        TAG_FLOAT => format!("{}", v.data_f),
        TAG_BOOL  => if v.data_i != 0 { "yes".to_string() } else { "no".to_string() },
        TAG_STR   => unsafe { cstr_to_str(v.data_i).to_string() },
        TAG_NULL  => "null".to_string(),
        TAG_LIST  => unsafe {
            let items = &*(v.data_i as *const Vec<i64>);
            let parts: Vec<String> = items.iter().map(|&p| val_to_display(val_ref(p))).collect();
            format!("[{}]", parts.join(", "))
        },
        TAG_DICT  => unsafe {
            let entries = &*(v.data_i as *const Vec<(String, i64)>);
            let parts: Vec<String> = entries.iter()
                .map(|(k, p)| format!("{}: {}", k, val_to_display(val_ref(*p))))
                .collect();
            format!("{{{}}}", parts.join(", "))
        },
        _         => "<valor>".to_string(),
    }
}

fn is_truthy_val(v: &OrionVal) -> bool {
    match v.tag {
        TAG_NULL  => false,
        TAG_INT   => v.data_i != 0,
        TAG_FLOAT => v.data_f != 0.0,
        TAG_BOOL  => v.data_i != 0,
        TAG_STR   => unsafe { !cstr_to_str(v.data_i).is_empty() },
        TAG_LIST  => unsafe { !(*(v.data_i as *const Vec<i64>)).is_empty() },
        TAG_DICT  => unsafe { !(*(v.data_i as *const Vec<(String, i64)>)).is_empty() },
        _         => true,
    }
}

//     Constructores                                                            

#[no_mangle]
pub extern "C" fn rt_make_null() -> i64 {
    alloc_val(TAG_NULL, 0, 0.0)
}

#[no_mangle]
pub extern "C" fn rt_make_int(v: i64) -> i64 {
    alloc_val(TAG_INT, v, 0.0)
}

/// Recibe los bits del f64 como i64 para evitar problemas de ABI con F64/I64 en Cranelift.
#[no_mangle]
pub extern "C" fn rt_make_float_bits(bits: i64) -> i64 {
    let f = f64::from_bits(bits as u64);
    alloc_val(TAG_FLOAT, 0, f)
}

#[no_mangle]
pub extern "C" fn rt_make_bool(v: i64) -> i64 {
    alloc_val(TAG_BOOL, if v != 0 { 1 } else { 0 }, 0.0)
}

/// `ptr` apunta a una cadena C (UTF-8, terminada en '\0') ya en el heap.
#[no_mangle]
pub extern "C" fn rt_make_str(ptr: i64) -> i64 {
    alloc_val(TAG_STR, ptr, 0.0)
}

//     I/O                                                                      

#[no_mangle]
pub extern "C" fn rt_show(val: i64) {
    unsafe {
        println!("{}", val_to_display(val_ref(val)));
    }
    let _ = io::stdout().flush();
}

/// Devuelve 1 si el valor es truthy, 0 si es falsy.
/// Usado por JumpIfFalse / JumpIfTrue.
#[no_mangle]
pub extern "C" fn rt_is_truthy(val: i64) -> i64 {
    unsafe { if is_truthy_val(val_ref(val)) { 1 } else { 0 } }
}

//     Aritmética                                                               

#[no_mangle]
pub extern "C" fn rt_add(a: i64, b: i64) -> i64 {
    unsafe {
        let av = val_ref(a);
        let bv = val_ref(b);
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT)     => alloc_val(TAG_INT, av.data_i.wrapping_add(bv.data_i), 0.0),
            (TAG_FLOAT, TAG_FLOAT) => alloc_val(TAG_FLOAT, 0, av.data_f + bv.data_f),
            (TAG_INT, TAG_FLOAT)   => alloc_val(TAG_FLOAT, 0, av.data_i as f64 + bv.data_f),
            (TAG_FLOAT, TAG_INT)   => alloc_val(TAG_FLOAT, 0, av.data_f + bv.data_i as f64),
            (TAG_STR, TAG_STR) => {
                let result = format!("{}{}", cstr_to_str(av.data_i), cstr_to_str(bv.data_i));
                alloc_val(TAG_STR, string_to_cptr(result), 0.0)
            }
            // Str + cualquier cosa: concatena como strings (igual que la VM)
            _ => {
                let sa = val_to_display(av);
                let sb = val_to_display(bv);
                let ptr = string_to_cptr(format!("{sa}{sb}"));
                alloc_val(TAG_STR, ptr, 0.0)
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn rt_sub(a: i64, b: i64) -> i64 {
    unsafe {
        let av = val_ref(a);
        let bv = val_ref(b);
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT)     => alloc_val(TAG_INT, av.data_i.wrapping_sub(bv.data_i), 0.0),
            (TAG_FLOAT, TAG_FLOAT) => alloc_val(TAG_FLOAT, 0, av.data_f - bv.data_f),
            (TAG_INT, TAG_FLOAT)   => alloc_val(TAG_FLOAT, 0, av.data_i as f64 - bv.data_f),
            (TAG_FLOAT, TAG_INT)   => alloc_val(TAG_FLOAT, 0, av.data_f - bv.data_i as f64),
            _ => { eprintln!("[JIT] tipos incompatibles en -"); std::process::exit(1) }
        }
    }
}

#[no_mangle]
pub extern "C" fn rt_mul(a: i64, b: i64) -> i64 {
    unsafe {
        let av = val_ref(a);
        let bv = val_ref(b);
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT)     => alloc_val(TAG_INT, av.data_i.wrapping_mul(bv.data_i), 0.0),
            (TAG_FLOAT, TAG_FLOAT) => alloc_val(TAG_FLOAT, 0, av.data_f * bv.data_f),
            (TAG_INT, TAG_FLOAT)   => alloc_val(TAG_FLOAT, 0, av.data_i as f64 * bv.data_f),
            (TAG_FLOAT, TAG_INT)   => alloc_val(TAG_FLOAT, 0, av.data_f * bv.data_i as f64),
            _ => { eprintln!("[JIT] tipos incompatibles en *"); std::process::exit(1) }
        }
    }
}

/// int / int → float (igual que la VM — división real, no entera).
#[no_mangle]
pub extern "C" fn rt_div(a: i64, b: i64) -> i64 {
    unsafe {
        let av = val_ref(a);
        let bv = val_ref(b);
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT) => {
                if bv.data_i == 0 { eprintln!("[JIT] Error: división por cero"); std::process::exit(1); }
                alloc_val(TAG_FLOAT, 0, av.data_i as f64 / bv.data_i as f64)
            }
            (TAG_FLOAT, TAG_FLOAT) => {
                if bv.data_f == 0.0 { eprintln!("[JIT] Error: división por cero"); std::process::exit(1); }
                alloc_val(TAG_FLOAT, 0, av.data_f / bv.data_f)
            }
            (TAG_INT, TAG_FLOAT)   => alloc_val(TAG_FLOAT, 0, av.data_i as f64 / bv.data_f),
            (TAG_FLOAT, TAG_INT)   => {
                if bv.data_i == 0 { eprintln!("[JIT] Error: división por cero"); std::process::exit(1); }
                alloc_val(TAG_FLOAT, 0, av.data_f / bv.data_i as f64)
            }
            _ => { eprintln!("[JIT] tipos incompatibles en /"); std::process::exit(1) }
        }
    }
}

#[no_mangle]
pub extern "C" fn rt_mod(a: i64, b: i64) -> i64 {
    unsafe {
        let av = val_ref(a);
        let bv = val_ref(b);
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT) => {
                if bv.data_i == 0 { eprintln!("[JIT] Error: módulo por cero"); std::process::exit(1); }
                alloc_val(TAG_INT, av.data_i % bv.data_i, 0.0)
            }
            _ => { eprintln!("[JIT] % solo soporta enteros"); std::process::exit(1) }
        }
    }
}

#[no_mangle]
pub extern "C" fn rt_pow(a: i64, b: i64) -> i64 {
    unsafe {
        let av = val_ref(a);
        let bv = val_ref(b);
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT) => {
                let result = if bv.data_i < 0 { 0 } else { av.data_i.pow(bv.data_i as u32) };
                alloc_val(TAG_INT, result, 0.0)
            }
            (TAG_FLOAT, TAG_FLOAT) => alloc_val(TAG_FLOAT, 0, av.data_f.powf(bv.data_f)),
            (TAG_INT, TAG_FLOAT)   => alloc_val(TAG_FLOAT, 0, (av.data_i as f64).powf(bv.data_f)),
            (TAG_FLOAT, TAG_INT)   => alloc_val(TAG_FLOAT, 0, av.data_f.powi(bv.data_i as i32)),
            _ => { eprintln!("[JIT] tipos incompatibles en **"); std::process::exit(1) }
        }
    }
}

#[no_mangle]
pub extern "C" fn rt_neg(a: i64) -> i64 {
    unsafe {
        let av = val_ref(a);
        match av.tag {
            TAG_INT   => alloc_val(TAG_INT, -av.data_i, 0.0),
            TAG_FLOAT => alloc_val(TAG_FLOAT, 0, -av.data_f),
            _ => { eprintln!("[JIT] - requiere número"); std::process::exit(1) }
        }
    }
}

//     Comparación                                                              

#[no_mangle]
pub extern "C" fn rt_eq(a: i64, b: i64) -> i64 {
    unsafe {
        let av = val_ref(a);
        let bv = val_ref(b);
        let eq = match (av.tag, bv.tag) {
            (TAG_NULL,  TAG_NULL)  => true,
            (TAG_INT,   TAG_INT)   => av.data_i == bv.data_i,
            (TAG_FLOAT, TAG_FLOAT) => av.data_f == bv.data_f,
            (TAG_INT,   TAG_FLOAT) => (av.data_i as f64) == bv.data_f,
            (TAG_FLOAT, TAG_INT)   => av.data_f == (bv.data_i as f64),
            (TAG_BOOL,  TAG_BOOL)  => av.data_i == bv.data_i,
            (TAG_STR,   TAG_STR)   => cstr_to_str(av.data_i) == cstr_to_str(bv.data_i),
            _ => false,
        };
        alloc_val(TAG_BOOL, if eq { 1 } else { 0 }, 0.0)
    }
}

#[no_mangle]
pub extern "C" fn rt_neq(a: i64, b: i64) -> i64 {
    unsafe {
        let av = val_ref(a);
        let bv = val_ref(b);
        let neq = match (av.tag, bv.tag) {
            (TAG_NULL,  TAG_NULL)  => false,
            (TAG_INT,   TAG_INT)   => av.data_i != bv.data_i,
            (TAG_FLOAT, TAG_FLOAT) => av.data_f != bv.data_f,
            (TAG_INT,   TAG_FLOAT) => (av.data_i as f64) != bv.data_f,
            (TAG_FLOAT, TAG_INT)   => av.data_f != (bv.data_i as f64),
            (TAG_BOOL,  TAG_BOOL)  => av.data_i != bv.data_i,
            (TAG_STR,   TAG_STR)   => cstr_to_str(av.data_i) != cstr_to_str(bv.data_i),
            _ => true,
        };
        alloc_val(TAG_BOOL, if neq { 1 } else { 0 }, 0.0)
    }
}

fn numeric_cmp(av: &OrionVal, bv: &OrionVal) -> std::cmp::Ordering {
    match (av.tag, bv.tag) {
        (TAG_INT,   TAG_INT)   => av.data_i.cmp(&bv.data_i),
        (TAG_FLOAT, TAG_FLOAT) => av.data_f.partial_cmp(&bv.data_f).unwrap_or(std::cmp::Ordering::Equal),
        (TAG_INT,   TAG_FLOAT) => (av.data_i as f64).partial_cmp(&bv.data_f).unwrap_or(std::cmp::Ordering::Equal),
        (TAG_FLOAT, TAG_INT)   => av.data_f.partial_cmp(&(bv.data_i as f64)).unwrap_or(std::cmp::Ordering::Equal),
        _ => { eprintln!("[JIT] comparación numérica inválida"); std::process::exit(1) }
    }
}

#[no_mangle]
pub extern "C" fn rt_lt(a: i64, b: i64) -> i64 {
    unsafe {
        let ord = numeric_cmp(val_ref(a), val_ref(b));
        alloc_val(TAG_BOOL, if ord.is_lt() { 1 } else { 0 }, 0.0)
    }
}

#[no_mangle]
pub extern "C" fn rt_lteq(a: i64, b: i64) -> i64 {
    unsafe {
        let ord = numeric_cmp(val_ref(a), val_ref(b));
        alloc_val(TAG_BOOL, if ord.is_le() { 1 } else { 0 }, 0.0)
    }
}

#[no_mangle]
pub extern "C" fn rt_gt(a: i64, b: i64) -> i64 {
    unsafe {
        let ord = numeric_cmp(val_ref(a), val_ref(b));
        alloc_val(TAG_BOOL, if ord.is_gt() { 1 } else { 0 }, 0.0)
    }
}

#[no_mangle]
pub extern "C" fn rt_gteq(a: i64, b: i64) -> i64 {
    unsafe {
        let ord = numeric_cmp(val_ref(a), val_ref(b));
        alloc_val(TAG_BOOL, if ord.is_ge() { 1 } else { 0 }, 0.0)
    }
}

//     Manejo de errores — JIT-3

/// Guarda el mensaje de error en TLS. Llamada por Raise antes de saltar al handler.
#[no_mangle]
pub extern "C" fn rt_set_error(msg: i64) {
    ORION_ERROR.with(|e| *e.borrow_mut() = Some(msg));
}

/// Recupera y limpia el error de TLS. Llamada al entrar al handler block.
/// Si no hay error (no debería pasar), retorna null.
#[no_mangle]
pub extern "C" fn rt_take_error() -> i64 {
    ORION_ERROR.with(|e| {
        e.borrow_mut().take().unwrap_or_else(|| alloc_val(TAG_NULL, 0, 0.0))
    })
}

/// Raise sin handler activo: imprime el error y termina el proceso.
#[no_mangle]
pub extern "C" fn rt_raise_exit(msg: i64) {
    unsafe {
        eprintln!("Error: {}", val_to_display(val_ref(msg)));
    }
    std::process::exit(1);
}

//     Colecciones — JIT-2

/// Acumula un argumento en el buffer thread-local para MakeList/MakeDict.
#[no_mangle]
pub extern "C" fn rt_push_arg(val: i64) {
    ARG_BUF.with(|b| b.borrow_mut().push(val));
}

/// Construye una Lista con los N primeros elementos del buffer.
/// El compilador empuja los elementos en orden (elem_0 primero).
#[no_mangle]
pub extern "C" fn rt_make_list_n(n: i64) -> i64 {
    ARG_BUF.with(|b| {
        let mut buf = b.borrow_mut();
        let n = n as usize;
        let items: Vec<i64> = buf.drain(..n).collect();
        let raw = Box::into_raw(Box::new(items)) as i64;
        alloc_val(TAG_LIST, raw, 0.0)
    })
}

/// Construye un Diccionario con N pares del buffer.
/// El compilador empuja en orden: val_{n-1}, key_{n-1}, ..., val_0, key_0
/// (mismo orden de pop del stack → mismo orden de inserción que el intérprete).
#[no_mangle]
pub extern "C" fn rt_make_dict_n(n: i64) -> i64 {
    ARG_BUF.with(|b| {
        let mut buf = b.borrow_mut();
        let n = n as usize;
        let flat: Vec<i64> = buf.drain(..n * 2).collect();
        let mut entries: Vec<(String, i64)> = Vec::with_capacity(n);
        for i in 0..n {
            let val_ptr = flat[i * 2];
            let key_ptr = flat[i * 2 + 1];
            let key_str = unsafe {
                let kv = val_ref(key_ptr);
                if kv.tag == TAG_STR { cstr_to_str(kv.data_i).to_string() }
                else { val_to_display(kv) }
            };
            entries.push((key_str, val_ptr));
        }
        let raw = Box::into_raw(Box::new(entries)) as i64;
        alloc_val(TAG_DICT, raw, 0.0)
    })
}

/// `obj[idx]` — soporta List[Int], Dict[Str], Str[Int].
#[no_mangle]
pub extern "C" fn rt_get_index(obj: i64, idx: i64) -> i64 {
    unsafe {
        let ov = val_ref(obj);
        let iv = val_ref(idx);
        match ov.tag {
            TAG_LIST => {
                let items = &*(ov.data_i as *const Vec<i64>);
                let i = iv.data_i;
                let i_usize = if i < 0 { (items.len() as i64 + i) as usize } else { i as usize };
                match items.get(i_usize) {
                    Some(&p) => p,
                    None => { eprintln!("[JIT] Índice {} fuera de rango", i); std::process::exit(1) }
                }
            }
            TAG_DICT => {
                let entries = &*(ov.data_i as *const Vec<(String, i64)>);
                let key_str = if iv.tag == TAG_STR { cstr_to_str(iv.data_i).to_string() }
                              else { val_to_display(iv) };
                for (k, p) in entries {
                    if k == &key_str { return *p; }
                }
                eprintln!("[JIT] Clave '{}' no encontrada", key_str);
                std::process::exit(1)
            }
            TAG_STR => {
                let s = cstr_to_str(ov.data_i);
                let i = iv.data_i;
                let i_usize = if i < 0 { (s.len() as i64 + i) as usize } else { i as usize };
                match s.chars().nth(i_usize) {
                    Some(ch) => alloc_val(TAG_STR, string_to_cptr(ch.to_string()), 0.0),
                    None => { eprintln!("[JIT] Índice {} fuera de rango en string", i); std::process::exit(1) }
                }
            }
            _ => { eprintln!("[JIT] GetIndex: tipo no soportado (tag={})", ov.tag); std::process::exit(1) }
        }
    }
}

/// `obj[idx] = val` — retorna el objeto modificado (semántica por valor, igual que el intérprete).
#[no_mangle]
pub extern "C" fn rt_set_index(obj: i64, idx: i64, val: i64) -> i64 {
    unsafe {
        let ov = val_ref(obj);
        let iv = val_ref(idx);
        match ov.tag {
            TAG_LIST => {
                let items = &*(ov.data_i as *const Vec<i64>);
                let mut new_items = items.clone();
                let i = iv.data_i;
                let i_usize = if i < 0 { (new_items.len() as i64 + i) as usize } else { i as usize };
                if i_usize >= new_items.len() {
                    eprintln!("[JIT] Índice {} fuera de rango en SetIndex", i);
                    std::process::exit(1);
                }
                new_items[i_usize] = val;
                let raw = Box::into_raw(Box::new(new_items)) as i64;
                alloc_val(TAG_LIST, raw, 0.0)
            }
            TAG_DICT => {
                let entries = &*(ov.data_i as *const Vec<(String, i64)>);
                let mut new_entries = entries.clone();
                let key_str = if iv.tag == TAG_STR { cstr_to_str(iv.data_i).to_string() }
                              else { val_to_display(iv) };
                let mut found = false;
                for entry in &mut new_entries {
                    if entry.0 == key_str { entry.1 = val; found = true; break; }
                }
                if !found { new_entries.push((key_str, val)); }
                let raw = Box::into_raw(Box::new(new_entries)) as i64;
                alloc_val(TAG_DICT, raw, 0.0)
            }
            _ => { eprintln!("[JIT] SetIndex: tipo no soportado (tag={})", ov.tag); std::process::exit(1) }
        }
    }
}

//     Lógica

#[no_mangle]
pub extern "C" fn rt_and(a: i64, b: i64) -> i64 {
    unsafe {
        let t = is_truthy_val(val_ref(a)) && is_truthy_val(val_ref(b));
        alloc_val(TAG_BOOL, if t { 1 } else { 0 }, 0.0)
    }
}

#[no_mangle]
pub extern "C" fn rt_or(a: i64, b: i64) -> i64 {
    unsafe {
        let t = is_truthy_val(val_ref(a)) || is_truthy_val(val_ref(b));
        alloc_val(TAG_BOOL, if t { 1 } else { 0 }, 0.0)
    }
}

#[no_mangle]
pub extern "C" fn rt_not(a: i64) -> i64 {
    unsafe {
        let t = !is_truthy_val(val_ref(a));
        alloc_val(TAG_BOOL, if t { 1 } else { 0 }, 0.0)
    }
}

//     I/O nativo — JIT-4

/// Lee una línea de stdin y aplica cast opcional.
/// `cast_ptr`: puntero C-string con "int"/"float"/"bool", o 0 para string puro.
#[no_mangle]
pub extern "C" fn rt_read_input(prompt: i64, cast_ptr: i64) -> i64 {
    unsafe {
        let prompt_str = val_to_display(val_ref(prompt));
        print!("{} ", prompt_str);
        let _ = io::stdout().flush();
        let raw = {
            let stdin = io::stdin();
            let mut line = String::new();
            stdin.lock().read_line(&mut line).unwrap_or(0);
            line.trim().to_string()
        };
        let cast_str = cstr_to_str(cast_ptr);
        apply_cast(raw, cast_str)
    }
}

/// Igual que `rt_read_input` pero valida la entrada contra una lista de opciones.
#[no_mangle]
pub extern "C" fn rt_read_input_choices(prompt: i64, choices: i64, cast_ptr: i64) -> i64 {
    unsafe {
        let choices_val = val_ref(choices);
        let choice_strings: Vec<String> = if choices_val.tag == TAG_LIST {
            let items = &*(choices_val.data_i as *const Vec<i64>);
            items.iter().map(|&p| val_to_display(val_ref(p))).collect()
        } else {
            vec![]
        };
        let prompt_str = val_to_display(val_ref(prompt));
        if !choice_strings.is_empty() {
            println!("{}", choice_strings.join(" / "));
        }
        let raw = loop {
            print!("{} ", prompt_str);
            let _ = io::stdout().flush();
            let stdin = io::stdin();
            let mut line = String::new();
            stdin.lock().read_line(&mut line).unwrap_or(0);
            let trimmed = line.trim().to_string();
            if choice_strings.is_empty() || choice_strings.contains(&trimmed) {
                break trimmed;
            }
        };
        let cast_str = cstr_to_str(cast_ptr);
        apply_cast(raw, cast_str)
    }
}

/// Lee un archivo y devuelve su contenido según `fmt_ptr` ("text", "lines", o cualquier otro = text).
#[no_mangle]
pub extern "C" fn rt_read_file(path: i64, fmt_ptr: i64) -> i64 {
    unsafe {
        let path_str = val_to_display(val_ref(path));
        let content = match std::fs::read_to_string(&path_str) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[JIT] read: no se pudo leer '{}': {}", path_str, e);
                std::process::exit(1);
            }
        };
        let fmt_str = cstr_to_str(fmt_ptr);
        match fmt_str {
            "lines" => {
                let items: Vec<i64> = content
                    .lines()
                    .map(|l| alloc_val(TAG_STR, string_to_cptr(l.to_string()), 0.0))
                    .collect();
                let raw = Box::into_raw(Box::new(items)) as i64;
                alloc_val(TAG_LIST, raw, 0.0)
            }
            _ => alloc_val(TAG_STR, string_to_cptr(content), 0.0),
        }
    }
}

/// Escribe `data` en el archivo `path`. `mode_ptr`: "append" o cualquier otro = "write".
#[no_mangle]
pub extern "C" fn rt_write_file(path: i64, data: i64, mode_ptr: i64) {
    unsafe {
        let path_str = val_to_display(val_ref(path));
        let data_str = val_to_display(val_ref(data));
        let mode_str = cstr_to_str(mode_ptr);
        match mode_str {
            "append" => {
                match std::fs::OpenOptions::new().append(true).create(true).open(&path_str) {
                    Ok(mut f) => { let _ = writeln!(f, "{}", data_str); }
                    Err(e) => { eprintln!("[JIT] write append '{}': {}", path_str, e); std::process::exit(1); }
                }
            }
            _ => {
                if let Err(e) = std::fs::write(&path_str, format!("{}\n", data_str)) {
                    eprintln!("[JIT] write '{}': {}", path_str, e);
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Lee una variable de entorno y aplica cast opcional.
#[no_mangle]
pub extern "C" fn rt_read_env(key: i64, cast_ptr: i64) -> i64 {
    unsafe {
        let key_str = val_to_display(val_ref(key));
        let raw = std::env::var(&key_str).unwrap_or_default();
        let cast_str = cstr_to_str(cast_ptr);
        apply_cast(raw, cast_str)
    }
}

/// Carga un módulo por nombre/path y devuelve un dict con su namespace.
/// Soporta el módulo builtin "math" con constantes. Los módulos .orx devuelven dict vacío.
#[no_mangle]
pub extern "C" fn rt_use_module(path_ptr: i64) -> i64 {
    unsafe {
        let path_str = cstr_to_str(path_ptr);
        let base_name = std::path::Path::new(path_str)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(path_str);

        match base_name {
            "math" => {
                use std::f64::consts;
                let entries: Vec<(String, i64)> = vec![
                    ("PI".to_string(),  alloc_val(TAG_FLOAT, 0, consts::PI)),
                    ("E".to_string(),   alloc_val(TAG_FLOAT, 0, consts::E)),
                    ("TAU".to_string(), alloc_val(TAG_FLOAT, 0, consts::TAU)),
                    ("PHI".to_string(), alloc_val(TAG_FLOAT, 0, 1.618_033_988_749_895)),
                    ("INF".to_string(), alloc_val(TAG_FLOAT, 0, f64::INFINITY)),
                ];
                let raw = Box::into_raw(Box::new(entries)) as i64;
                alloc_val(TAG_DICT, raw, 0.0)
            }
            _ => {
                let candidates = [
                    format!("packages/{}.orx", path_str),
                    format!("{}.orx", path_str),
                    format!("lib/{}.orx", path_str),
                ];
                let exists = candidates.iter().any(|c| std::path::Path::new(c).exists());
                if exists {
                    // Módulos .orx necesitan sub-VM; JIT devuelve dict vacío como placeholder
                    let entries: Vec<(String, i64)> = Vec::new();
                    let raw = Box::into_raw(Box::new(entries)) as i64;
                    alloc_val(TAG_DICT, raw, 0.0)
                } else {
                    eprintln!("[JIT] Módulo '{}' no encontrado", path_str);
                    std::process::exit(1)
                }
            }
        }
    }
}

//     Helpers privados — JIT-4

fn apply_cast(raw: String, cast: &str) -> i64 {
    match cast {
        "int"   => alloc_val(TAG_INT,  raw.parse::<i64>().unwrap_or(0),   0.0),
        "float" => alloc_val(TAG_FLOAT, 0, raw.parse::<f64>().unwrap_or(0.0)),
        "bool"  => {
            let v = matches!(raw.as_str(), "yes" | "true" | "1");
            alloc_val(TAG_BOOL, if v { 1 } else { 0 }, 0.0)
        }
        _       => alloc_val(TAG_STR, string_to_cptr(raw), 0.0),
    }
}
