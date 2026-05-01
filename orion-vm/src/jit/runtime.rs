//! Runtime Orion JIT — Fase JIT-1: Sistema de Valores Unificado
//!
//! OrionVal: valor boxeado en heap que representa cualquier valor Orion.
//! Todas las funciones de runtime reciben y devuelven punteros como i64.
//! Fase JIT-2 añadirá: TAG_LIST, TAG_DICT.
//! Fase JIT-5 añadirá: TAG_SHAPE, TAG_CLOSURE, TAG_TASK.

use std::io::{self, Write as IoWrite};

//     Tags                                                                     

pub const TAG_NULL:  u8 = 0;
pub const TAG_INT:   u8 = 1;
pub const TAG_FLOAT: u8 = 2;
pub const TAG_BOOL:  u8 = 3;
pub const TAG_STR:   u8 = 4;
// Reservados:
// pub const TAG_LIST:    u8 = 5;
// pub const TAG_DICT:    u8 = 6;
// pub const TAG_SHAPE:   u8 = 7;
// pub const TAG_CLOSURE: u8 = 8;
// pub const TAG_TASK:    u8 = 9;

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
