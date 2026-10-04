//! Runtime del JIT. Un valor es un i64 con NaN-boxing (ver `alloc_val`): los
//! enteros de 48 bits, los decimales, `null` y los booleanos van dentro del
//! i64; lo demás es un puntero a un `OrionVal` en el heap.

use std::cell::RefCell;
use indexmap::IndexMap as HashMap;
use std::io::{self, BufRead, Write as IoWrite};
use std::sync::{Arc, Mutex, Condvar, OnceLock};

//     Tags                                                                     

pub const TAG_NULL:  u8 = 0;
pub const TAG_INT:   u8 = 1;
pub const TAG_FLOAT: u8 = 2;
pub const TAG_BOOL:  u8 = 3;
pub const TAG_STR:   u8 = 4;
pub const TAG_LIST:    u8 = 5;  // data_i = Box::into_raw(Box<Vec<i64>>)
pub const TAG_DICT:    u8 = 6;  // data_i = Box::into_raw(Box<Vec<(String, i64)>>)
// TAG_INSTANCE = 7 definido en runtime_oop
pub const TAG_CLOSURE: u8 = 9;  // data_i = fn_ptr (i64) de la función capturada
pub const TAG_TASK:    u8 = 10; // data_i = Box<Arc<JitTask>>

// Buffer thread-local para pasar N argumentos variádicos a MakeList/MakeDict.
thread_local! {
    pub(crate) static ARG_BUF: RefCell<Vec<i64>> = RefCell::new(Vec::new());
}

/// Extrae los primeros `n` argumentos del ARG_BUF (elem_0 primero), en orden.
pub(crate) fn drain_arg_buf(n: usize) -> Vec<i64> {
    ARG_BUF.with(|b| b.borrow_mut().drain(..n).collect())
}

// Error activo para attempt/handle — almacena el OrionVal* del mensaje.
thread_local! {
    static ORION_ERROR: RefCell<Option<i64>> = RefCell::new(None);
}

//     OrionVal                                                                 

#[derive(Clone, Copy)]
pub struct OrionVal {
    pub tag:    u8,
    pub _pad:   [u8; 7],
    pub data_i: i64,
    pub data_f: f64,
}

//     Helpers internos                                                         

/// Decimal: sus bits + 2^48. Entero de 48 bits: 0xFFFE en los 16 bits altos.
/// Los punteros (< 2^48) y las constantes de abajo quedan por debajo, y el 0
/// sigue libre para marcar un error pendiente.
pub const DOUBLE_OFFSET: i64 = 1 << 48;
pub const INT_TAG: i64 = 0xFFFE_0000_0000_0000_u64 as i64;
pub const VAL_NULL:  i64 = 0x02;
pub const VAL_FALSE: i64 = 0x06;
pub const VAL_TRUE:  i64 = 0x07;
const INT48_MIN: i64 = -(1 << 47);
const INT48_MAX: i64 = (1 << 47) - 1;

/// Codifica un valor. Solo reserva memoria si no cabe en el i64.
pub(crate) fn alloc_val(tag: u8, data_i: i64, data_f: f64) -> i64 {
    match tag {
        TAG_INT if (INT48_MIN..=INT48_MAX).contains(&data_i) =>
            INT_TAG | (data_i & 0xFFFF_FFFF_FFFF),
        TAG_FLOAT => encode_f64(data_f),
        TAG_NULL  => VAL_NULL,
        TAG_BOOL  => if data_i != 0 { VAL_TRUE } else { VAL_FALSE },
        _ => Box::into_raw(Box::new(OrionVal { tag, _pad: [0; 7], data_i, data_f })) as i64,
    }
}

/// Un NaN se normaliza: con el signo puesto chocaría con los enteros.
pub(crate) fn encode_f64(f: f64) -> i64 {
    let bits = if f.is_nan() { 0x7FF8_0000_0000_0000 } else { f.to_bits() };
    bits.wrapping_add(DOUBLE_OFFSET as u64) as i64
}

/// Decodifica un valor a su forma `OrionVal` (por copia).
pub(crate) unsafe fn decode_val(v: i64) -> OrionVal {
    let u = v as u64;
    let (tag, data_i, data_f) = if u >= INT_TAG as u64 {
        (TAG_INT, (v << 16) >> 16, 0.0)
    } else if u >= DOUBLE_OFFSET as u64 {
        (TAG_FLOAT, 0, f64::from_bits(u.wrapping_sub(DOUBLE_OFFSET as u64)))
    } else {
        match v {
            VAL_NULL  => (TAG_NULL, 0, 0.0),
            VAL_FALSE => (TAG_BOOL, 0, 0.0),
            VAL_TRUE  => (TAG_BOOL, 1, 0.0),
            _ => return *(v as *const OrionVal),
        }
    };
    OrionVal { tag, _pad: [0; 7], data_i, data_f }
}

pub(crate) unsafe fn cstr_to_str(ptr: i64) -> &'static str {
    let p = ptr as *const u8;
    if p.is_null() { return ""; }
    let mut len = 0;
    while *p.add(len) != 0 { len += 1; }
    let slice = std::slice::from_raw_parts(p, len);
    std::str::from_utf8_unchecked(slice)
}

pub(crate) fn string_to_cptr(s: String) -> i64 {
    let mut bytes = s.into_bytes();
    bytes.push(0);
    let boxed = bytes.into_boxed_slice();
    Box::into_raw(boxed) as *mut u8 as i64
}

pub(crate) fn val_to_display(v: &OrionVal) -> String {
    match v.tag {
        TAG_INT   => v.data_i.to_string(),
        TAG_FLOAT => format!("{}", v.data_f),
        TAG_BOOL  => if v.data_i != 0 { "yes".to_string() } else { "no".to_string() },
        TAG_STR   => unsafe { cstr_to_str(v.data_i).to_string() },
        TAG_NULL  => "null".to_string(),
        TAG_LIST  => unsafe {
            let items = &*(v.data_i as *const Vec<i64>);
            let parts: Vec<String> = items.iter().map(|&p| val_to_display(&decode_val(p))).collect();
            format!("[{}]", parts.join(", "))
        },
        TAG_DICT  => unsafe {
            let entries = &*(v.data_i as *const Vec<(String, i64)>);
            let parts: Vec<String> = entries.iter()
                .map(|(k, p)| format!("{}: {}", k, val_to_display(&decode_val(*p))))
                .collect();
            format!("{{{}}}", parts.join(", "))
        },
        TAG_CLOSURE => "<closure>".to_string(),
        TAG_TASK    => "<task>".to_string(),
        _           => "<value>".to_string(),
    }
}

pub(crate) fn is_truthy_val(v: &OrionVal) -> bool {
    match v.tag {
        TAG_NULL  => false,
        TAG_INT   => v.data_i != 0,
        TAG_FLOAT => v.data_f != 0.0,
        TAG_BOOL  => v.data_i != 0,
        TAG_STR   => unsafe { !cstr_to_str(v.data_i).is_empty() },
        TAG_LIST  => unsafe { !(*(v.data_i as *const Vec<i64>)).is_empty() },
        TAG_DICT    => unsafe { !(*(v.data_i as *const Vec<(String, i64)>)).is_empty() },
        TAG_CLOSURE | TAG_TASK => true,
        _           => true,
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

/// `ptr` apunta a una cadena C (UTF-8, terminada en '\0') ya en el heap.
#[no_mangle]
pub extern "C" fn rt_make_str(ptr: i64) -> i64 {
    alloc_val(TAG_STR, ptr, 0.0)
}

//     I/O                                                                      

#[no_mangle]
pub extern "C" fn rt_show(val: i64) {
    unsafe {
        println!("{}", crate::modules::secret_mod::redact(&val_to_display(&decode_val(val))));
    }
    let _ = io::stdout().flush();
}

/// Devuelve 1 si el valor es truthy, 0 si es falsy.
/// Usado por JumpIfFalse / JumpIfTrue.
#[no_mangle]
pub extern "C" fn rt_is_truthy(val: i64) -> i64 {
    unsafe { if is_truthy_val(&decode_val(val)) { 1 } else { 0 } }
}

//     Aritmética                                                               

#[no_mangle]
pub extern "C" fn rt_add(a: i64, b: i64) -> i64 {
    unsafe {
        let av = decode_val(a);
        let bv = decode_val(b);
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT) => match av.data_i.checked_add(bv.data_i) {
                Some(r) => alloc_val(TAG_INT, r, 0.0),
                None => fallar("Desbordamiento aritmético en suma de enteros"),
            },
            (TAG_FLOAT, TAG_FLOAT) => alloc_val(TAG_FLOAT, 0, av.data_f + bv.data_f),
            (TAG_INT, TAG_FLOAT)   => alloc_val(TAG_FLOAT, 0, av.data_i as f64 + bv.data_f),
            (TAG_FLOAT, TAG_INT)   => alloc_val(TAG_FLOAT, 0, av.data_f + bv.data_i as f64),
            // Concatenación: solo si al menos un operando es Str (igual que la VM).
            // bool/null/etc. en aritmética → error de tipo, NO coerción silenciosa.
            (TAG_STR, _) | (_, TAG_STR) => {
                let result = format!("{}{}", val_to_display(&av), val_to_display(&bv));
                alloc_val(TAG_STR, string_to_cptr(result), 0.0)
            }
            _ => fallar(format!("Cannot add {} + {}", nombre_tipo(&av), nombre_tipo(&bv))),
        }
    }
}

#[no_mangle]
pub extern "C" fn rt_sub(a: i64, b: i64) -> i64 {
    unsafe {
        let av = decode_val(a);
        let bv = decode_val(b);
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT) => match av.data_i.checked_sub(bv.data_i) {
                Some(r) => alloc_val(TAG_INT, r, 0.0),
                None => fallar("Integer subtraction overflow"),
            },
            (TAG_FLOAT, TAG_FLOAT) => alloc_val(TAG_FLOAT, 0, av.data_f - bv.data_f),
            (TAG_INT, TAG_FLOAT)   => alloc_val(TAG_FLOAT, 0, av.data_i as f64 - bv.data_f),
            (TAG_FLOAT, TAG_INT)   => alloc_val(TAG_FLOAT, 0, av.data_f - bv.data_i as f64),
            _ => fallar(format!("Cannot subtract {} - {}", nombre_tipo(&av), nombre_tipo(&bv))),
        }
    }
}

#[no_mangle]
pub extern "C" fn rt_mul(a: i64, b: i64) -> i64 {
    unsafe {
        let av = decode_val(a);
        let bv = decode_val(b);
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT) => match av.data_i.checked_mul(bv.data_i) {
                Some(r) => alloc_val(TAG_INT, r, 0.0),
                None => fallar("Desbordamiento aritmético en multiplicación de enteros"),
            },
            (TAG_FLOAT, TAG_FLOAT) => alloc_val(TAG_FLOAT, 0, av.data_f * bv.data_f),
            (TAG_INT, TAG_FLOAT)   => alloc_val(TAG_FLOAT, 0, av.data_i as f64 * bv.data_f),
            (TAG_FLOAT, TAG_INT)   => alloc_val(TAG_FLOAT, 0, av.data_f * bv.data_i as f64),
            _ => fallar(format!("Cannot multiply {} * {}", nombre_tipo(&av), nombre_tipo(&bv))),
        }
    }
}

/// int / int → float (igual que la VM — división real, no entera).
#[no_mangle]
pub extern "C" fn rt_div(a: i64, b: i64) -> i64 {
    unsafe {
        let av = decode_val(a);
        let bv = decode_val(b);
        if (bv.tag == TAG_INT && bv.data_i == 0) || (bv.tag == TAG_FLOAT && bv.data_f == 0.0) {
            return fallar("División por cero");
        }
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT)     => alloc_val(TAG_FLOAT, 0, av.data_i as f64 / bv.data_i as f64),
            (TAG_FLOAT, TAG_FLOAT) => alloc_val(TAG_FLOAT, 0, av.data_f / bv.data_f),
            (TAG_INT, TAG_FLOAT)   => alloc_val(TAG_FLOAT, 0, av.data_i as f64 / bv.data_f),
            (TAG_FLOAT, TAG_INT)   => alloc_val(TAG_FLOAT, 0, av.data_f / bv.data_i as f64),
            _ => fallar(format!("Cannot divide {} / {}", nombre_tipo(&av), nombre_tipo(&bv))),
        }
    }
}

#[no_mangle]
pub extern "C" fn rt_mod(a: i64, b: i64) -> i64 {
    unsafe {
        let av = decode_val(a);
        let bv = decode_val(b);
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT) => {
                if bv.data_i == 0 { return fallar("Modulo by zero"); }
                match av.data_i.checked_rem(bv.data_i) {
                    Some(r) => alloc_val(TAG_INT, r, 0.0),
                    None => fallar("Desbordamiento aritmético en módulo"),
                }
            }
            _ => fallar("Modulo only supports integers"),
        }
    }
}

#[no_mangle]
pub extern "C" fn rt_pow(a: i64, b: i64) -> i64 {
    unsafe {
        let av = decode_val(a);
        let bv = decode_val(b);
        match (av.tag, bv.tag) {
            (TAG_INT, TAG_INT) => {
                if bv.data_i < 0 { return fallar("Negative exponent in integer power (use floats)"); }
                match u32::try_from(bv.data_i).ok().and_then(|e| av.data_i.checked_pow(e)) {
                    Some(r) => alloc_val(TAG_INT, r, 0.0),
                    None => fallar("Desbordamiento aritmético en potencia"),
                }
            }
            (TAG_FLOAT, TAG_FLOAT) => alloc_val(TAG_FLOAT, 0, av.data_f.powf(bv.data_f)),
            (TAG_INT, TAG_FLOAT)   => alloc_val(TAG_FLOAT, 0, (av.data_i as f64).powf(bv.data_f)),
            (TAG_FLOAT, TAG_INT)   => alloc_val(TAG_FLOAT, 0, av.data_f.powi(bv.data_i as i32)),
            _ => fallar("Power expects numbers"),
        }
    }
}

#[no_mangle]
pub extern "C" fn rt_neg(a: i64) -> i64 {
    unsafe {
        let av = decode_val(a);
        match av.tag {
            TAG_INT   => match av.data_i.checked_neg() {
                Some(r) => alloc_val(TAG_INT, r, 0.0),
                None => fallar("Desbordamiento aritmético en negación"),
            },
            TAG_FLOAT => alloc_val(TAG_FLOAT, 0, -av.data_f),
            _ => fallar("Negation only applies to numbers"),
        }
    }
}

//     Comparación                                                              

unsafe fn jit_vals_equal(a: i64, b: i64) -> bool {
    let av = decode_val(a);
    let bv = decode_val(b);
    match (av.tag, bv.tag) {
        (TAG_NULL,  TAG_NULL)  => true,
        (TAG_INT,   TAG_INT)   => av.data_i == bv.data_i,
        (TAG_FLOAT, TAG_FLOAT) => av.data_f == bv.data_f,
        (TAG_INT,   TAG_FLOAT) => (av.data_i as f64) == bv.data_f,
        (TAG_FLOAT, TAG_INT)   => av.data_f == (bv.data_i as f64),
        (TAG_BOOL,  TAG_BOOL)  => av.data_i == bv.data_i,
        (TAG_STR,   TAG_STR)   => cstr_to_str(av.data_i) == cstr_to_str(bv.data_i),
        (TAG_LIST,  TAG_LIST)  => {
            let la = &*(av.data_i as *const Vec<i64>);
            let lb = &*(bv.data_i as *const Vec<i64>);
            la.len() == lb.len()
                && la.iter().zip(lb.iter()).all(|(&x, &y)| jit_vals_equal(x, y))
        }
        (TAG_DICT,  TAG_DICT)  => {
            // Como IndexMap en la VM: igualdad independiente del orden.
            let da = &*(av.data_i as *const Vec<(String, i64)>);
            let db = &*(bv.data_i as *const Vec<(String, i64)>);
            da.len() == db.len()
                && da.iter().all(|(k, v)| {
                    db.iter().any(|(k2, v2)| k == k2 && jit_vals_equal(*v, *v2))
                })
        }
        _ => false,
    }
}

#[no_mangle]
pub extern "C" fn rt_eq(a: i64, b: i64) -> i64 {
    unsafe {
        let eq = jit_vals_equal(a, b);
        alloc_val(TAG_BOOL, if eq { 1 } else { 0 }, 0.0)
    }
}

#[no_mangle]
pub extern "C" fn rt_neq(a: i64, b: i64) -> i64 {
    unsafe {
        let neq = !jit_vals_equal(a, b);
        alloc_val(TAG_BOOL, if neq { 1 } else { 0 }, 0.0)
    }
}

/// Compara dos números y aplica `pred`; otro tipo es error, como en la VM.
fn numeric_cmp(av: &OrionVal, bv: &OrionVal, pred: fn(std::cmp::Ordering) -> bool) -> i64 {
    use std::cmp::Ordering::Equal;
    let ord = match (av.tag, bv.tag) {
        (TAG_INT,   TAG_INT)   => av.data_i.cmp(&bv.data_i),
        (TAG_FLOAT, TAG_FLOAT) => av.data_f.partial_cmp(&bv.data_f).unwrap_or(Equal),
        (TAG_INT,   TAG_FLOAT) => (av.data_i as f64).partial_cmp(&bv.data_f).unwrap_or(Equal),
        (TAG_FLOAT, TAG_INT)   => av.data_f.partial_cmp(&(bv.data_i as f64)).unwrap_or(Equal),
        _ => return fallar(format!("Cannot compare {} < {}", nombre_tipo(av), nombre_tipo(bv))),
    };
    alloc_val(TAG_BOOL, if pred(ord) { 1 } else { 0 }, 0.0)
}

#[no_mangle]
pub extern "C" fn rt_lt(a: i64, b: i64) -> i64 {
    unsafe { numeric_cmp(&decode_val(a), &decode_val(b), std::cmp::Ordering::is_lt) }
}

#[no_mangle]
pub extern "C" fn rt_lteq(a: i64, b: i64) -> i64 {
    unsafe { numeric_cmp(&decode_val(a), &decode_val(b), std::cmp::Ordering::is_le) }
}

#[no_mangle]
pub extern "C" fn rt_gt(a: i64, b: i64) -> i64 {
    unsafe { numeric_cmp(&decode_val(a), &decode_val(b), std::cmp::Ordering::is_gt) }
}

#[no_mangle]
pub extern "C" fn rt_gteq(a: i64, b: i64) -> i64 {
    unsafe { numeric_cmp(&decode_val(a), &decode_val(b), std::cmp::Ordering::is_ge) }
}

//     Manejo de errores — JIT-3

/// Deja `msg` como error de Orion pendiente y devuelve 0, que ningún valor real
/// usa: el código nativo lo comprueba y salta al `handle` o retorna 0.
pub(crate) fn fallar(msg: impl Into<String>) -> i64 {
    let v = alloc_val(TAG_STR, string_to_cptr(msg.into()), 0.0);
    ORION_ERROR.with(|e| *e.borrow_mut() = Some(v));
    0
}

/// 1 si hay un error pendiente: para las llamadas que no devuelven valor.
#[no_mangle]
pub extern "C" fn rt_error_pending() -> i64 {
    ORION_ERROR.with(|e| e.borrow().is_some() as i64)
}

pub(crate) fn nombre_tipo(v: &OrionVal) -> &'static str {
    match v.tag {
        TAG_INT => "int", TAG_FLOAT => "float", TAG_STR => "string", TAG_BOOL => "bool",
        TAG_LIST => "list", TAG_DICT => "dict", TAG_NULL => "null", TAG_CLOSURE => "fn",
        TAG_TASK => "task", _ => "instance",
    }
}

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
        eprintln!("Error: {}", val_to_display(&decode_val(msg)));
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
                let kv = decode_val(key_ptr);
                if kv.tag == TAG_STR { cstr_to_str(kv.data_i).to_string() }
                else { val_to_display(&kv) }
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
        let ov = decode_val(obj);
        let iv = decode_val(idx);
        match ov.tag {
            TAG_LIST => {
                let items = &*(ov.data_i as *const Vec<i64>);
                let i = iv.data_i;
                let i_usize = if i < 0 { (items.len() as i64 + i) as usize } else { i as usize };
                match items.get(i_usize) {
                    Some(&p) => p,
                    None => fallar(format!("Index {} out of range", i)),
                }
            }
            TAG_DICT => {
                let entries = &*(ov.data_i as *const Vec<(String, i64)>);
                let key_str = if iv.tag == TAG_STR { cstr_to_str(iv.data_i).to_string() }
                              else { val_to_display(&iv) };
                for (k, p) in entries {
                    if k == &key_str { return *p; }
                }
                fallar(format!("Key '{}' not found", key_str))
            }
            TAG_STR => {
                let s = cstr_to_str(ov.data_i);
                let i = iv.data_i;
                let i_usize = if i < 0 { (s.len() as i64 + i) as usize } else { i as usize };
                match s.chars().nth(i_usize) {
                    Some(ch) => alloc_val(TAG_STR, string_to_cptr(ch.to_string()), 0.0),
                    None => fallar(format!("Index {} out of range in string", i)),
                }
            }
            _ => fallar("GetIndex: unsupported type"),
        }
    }
}

/// `obj[idx] = val` — retorna el objeto modificado (semántica por valor, igual que el intérprete).
#[no_mangle]
pub extern "C" fn rt_set_index(obj: i64, idx: i64, val: i64) -> i64 {
    unsafe {
        let ov = decode_val(obj);
        let iv = decode_val(idx);
        match ov.tag {
            TAG_LIST => {
                // Mutación in-place + mismo puntero → los alias ven el cambio
                // (paridad con la VM).
                let items = &mut *(ov.data_i as *mut Vec<i64>);
                let i = iv.data_i;
                let i_usize = if i < 0 { (items.len() as i64 + i) as usize } else { i as usize };
                if i_usize >= items.len() {
                    return fallar(format!("Index {} out of range in SetIndex", i));
                }
                items[i_usize] = val;
                obj
            }
            TAG_DICT => {
                let entries = &*(ov.data_i as *const Vec<(String, i64)>);
                let mut new_entries = entries.clone();
                let key_str = if iv.tag == TAG_STR { cstr_to_str(iv.data_i).to_string() }
                              else { val_to_display(&iv) };
                let mut found = false;
                for entry in &mut new_entries {
                    if entry.0 == key_str { entry.1 = val; found = true; break; }
                }
                if !found { new_entries.push((key_str, val)); }
                let raw = Box::into_raw(Box::new(new_entries)) as i64;
                alloc_val(TAG_DICT, raw, 0.0)
            }
            _ => fallar("SetIndex: unsupported type"),
        }
    }
}

//     Lógica

#[no_mangle]
pub extern "C" fn rt_and(a: i64, b: i64) -> i64 {
    unsafe {
        let t = is_truthy_val(&decode_val(a)) && is_truthy_val(&decode_val(b));
        alloc_val(TAG_BOOL, if t { 1 } else { 0 }, 0.0)
    }
}

#[no_mangle]
pub extern "C" fn rt_or(a: i64, b: i64) -> i64 {
    unsafe {
        let t = is_truthy_val(&decode_val(a)) || is_truthy_val(&decode_val(b));
        alloc_val(TAG_BOOL, if t { 1 } else { 0 }, 0.0)
    }
}

#[no_mangle]
pub extern "C" fn rt_not(a: i64) -> i64 {
    unsafe {
        let t = !is_truthy_val(&decode_val(a));
        alloc_val(TAG_BOOL, if t { 1 } else { 0 }, 0.0)
    }
}

//     I/O nativo — JIT-4

/// Lee una línea de stdin y aplica cast opcional.
/// `cast_ptr`: puntero C-string con "int"/"float"/"bool", o 0 para string puro.
#[no_mangle]
pub extern "C" fn rt_read_input(prompt: i64, cast_ptr: i64) -> i64 {
    unsafe {
        let prompt_str = val_to_display(&decode_val(prompt));
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
        let choices_val = decode_val(choices);
        let choice_strings: Vec<String> = if choices_val.tag == TAG_LIST {
            let items = &*(choices_val.data_i as *const Vec<i64>);
            items.iter().map(|&p| val_to_display(&decode_val(p))).collect()
        } else {
            vec![]
        };
        let prompt_str = val_to_display(&decode_val(prompt));
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
        let path_str = val_to_display(&decode_val(path));
        let content = match std::fs::read_to_string(&path_str) {
            Ok(c) => c,
            Err(e) => return fallar(format!("read: could not read '{}': {}", path_str, e)),
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
        let path_str = val_to_display(&decode_val(path));
        let data_str = val_to_display(&decode_val(data));
        let mode_str = cstr_to_str(mode_ptr);
        match mode_str {
            "append" => {
                match std::fs::OpenOptions::new().append(true).create(true).open(&path_str) {
                    Ok(mut f) => { let _ = writeln!(f, "{}", data_str); }
                    Err(e) => { fallar(format!("write append '{}': {}", path_str, e)); }
                }
            }
            _ => {
                if let Err(e) = std::fs::write(&path_str, format!("{}\n", data_str)) {
                    fallar(format!("write '{}': {}", path_str, e));
                }
            }
        }
    }
}

/// Lee una variable de entorno y aplica cast opcional.
#[no_mangle]
pub extern "C" fn rt_read_env(key: i64, cast_ptr: i64) -> i64 {
    unsafe {
        let key_str = val_to_display(&decode_val(key));
        let raw = std::env::var(&key_str).unwrap_or_default();
        let cast_str = cstr_to_str(cast_ptr);
        apply_cast(raw, cast_str)
    }
}

/// Carga un módulo por nombre/path y devuelve un dict con su namespace.
#[no_mangle]
pub extern "C" fn rt_use_module(path_ptr: i64) -> i64 {
    unsafe {
        let path_str = cstr_to_str(path_ptr);
        let base_name = std::path::Path::new(path_str)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(path_str);
        let explicit_path = path_str.contains('/') || path_str.contains('\\');
        let resolved = crate::paths::resolve_module_file(path_str)
            .map(|p| p.to_string_lossy().to_string());
        if explicit_path {
            if let Some(file) = &resolved {
                return super::bridge::load_orx_module_jit(file);
            }
        }

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
            // Módulos nativos Rust (fs, json, random, datetime, ...) → marcador Module.
            name if crate::modules::is_known_module(name) => {
                let entries: Vec<(String, i64)> = vec![(
                    "__native_module__".to_string(),
                    alloc_val(TAG_STR, string_to_cptr(name.to_string()), 0.0),
                )];
                let raw = Box::into_raw(Box::new(entries)) as i64;
                alloc_val(TAG_DICT, raw, 0.0)
            }
            _ => {
                match &resolved {
                    Some(file) => super::bridge::load_orx_module_jit(file),
                    None => fallar(format!("Module '{}' not found", path_str)),
                }
            }
        }
    }
}

//     Variables globales

static GLOBALS: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();

fn globals() -> &'static Mutex<HashMap<String, i64>> {
    GLOBALS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Publica un global. Lo emite el nivel superior del programa al asignar.
#[no_mangle]
pub extern "C" fn rt_store_global(name_ptr: i64, val: i64) {
    let name = unsafe { cstr_to_str(name_ptr) };
    globals().lock().unwrap().insert(name.to_string(), val);
}

/// Lee un global. Lo emite una función al usar un nombre que no es suyo.
#[no_mangle]
pub extern "C" fn rt_load_global(name_ptr: i64) -> i64 {
    let name = unsafe { cstr_to_str(name_ptr) };
    globals().lock().unwrap()
        .get(name).copied()
        .unwrap_or_else(|| alloc_val(TAG_NULL, 0, 0.0))
}

//     JIT-6: Tabla global de punteros de funciones JIT

static JIT_FN_TABLE: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();

fn jit_fn_table() -> &'static Mutex<HashMap<String, i64>> {
    JIT_FN_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registra el puntero de una función JIT compilada. Llamado por run_program() tras finalize.
pub fn register_jit_fn(name: &str, fn_ptr: i64) {
    jit_fn_table().lock().unwrap().insert(name.to_string(), fn_ptr);
}

/// `rt_register_fn(name, fn_ptr)` — versión C-ABI para binarios AOT, donde el
#[no_mangle]
pub extern "C" fn rt_register_fn(name_ptr: i64, fn_ptr: i64) {
    let name = unsafe { cstr_to_str(name_ptr) };
    register_jit_fn(name, fn_ptr);
}

pub struct JitTask {
    /// `Err`: el mensaje del error que terminó la tarea en su hilo.
    result: Mutex<Option<Result<i64, String>>>,
    done:   Condvar,
}

impl JitTask {
    fn complete(&self, r: Result<i64, String>) {
        let mut g = self.result.lock().unwrap();
        *g = Some(r);
        self.done.notify_all();
    }
    fn wait(&self) -> Result<i64, String> {
        let mut g = self.result.lock().unwrap();
        while g.is_none() {
            g = self.done.wait(g).unwrap();
        }
        g.clone().unwrap()
    }
}

/// Boxea un Arc de tarea en un OrionVal TAG_TASK.
fn alloc_task(task: Arc<JitTask>) -> i64 {
    let raw = Box::into_raw(Box::new(task)) as i64;
    alloc_val(TAG_TASK, raw, 0.0)
}

/// Invoca una función JIT con arity 0-8 de forma dinámica.
unsafe fn call_fn_n(fn_ptr: i64, args: &[i64]) -> i64 {
    let p = fn_ptr as usize;
    match args.len() {
        0 => std::mem::transmute::<usize, extern "C" fn() -> i64>(p)(),
        1 => { type F = extern "C" fn(i64) -> i64;
               std::mem::transmute::<usize, F>(p)(args[0]) }
        2 => { type F = extern "C" fn(i64, i64) -> i64;
               std::mem::transmute::<usize, F>(p)(args[0], args[1]) }
        3 => { type F = extern "C" fn(i64, i64, i64) -> i64;
               std::mem::transmute::<usize, F>(p)(args[0], args[1], args[2]) }
        4 => { type F = extern "C" fn(i64, i64, i64, i64) -> i64;
               std::mem::transmute::<usize, F>(p)(args[0], args[1], args[2], args[3]) }
        5 => { type F = extern "C" fn(i64, i64, i64, i64, i64) -> i64;
               std::mem::transmute::<usize, F>(p)(args[0], args[1], args[2], args[3], args[4]) }
        6 => { type F = extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64;
               std::mem::transmute::<usize, F>(p)(args[0], args[1], args[2], args[3], args[4], args[5]) }
        7 => { type F = extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64;
               std::mem::transmute::<usize, F>(p)(args[0], args[1], args[2], args[3], args[4], args[5], args[6]) }
        8 => { type F = extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64;
               std::mem::transmute::<usize, F>(p)(args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7]) }
        n => fallar(format!("JIT: calls with {} arguments are not supported (max 8)", n)),
    }
}

//     JIT-6: Closures

/// Crea un valor Closure que apunta al fn_ptr de la función capturada.
#[no_mangle]
pub extern "C" fn rt_make_closure(fn_name_ptr: i64) -> i64 {
    let fn_name = unsafe { cstr_to_str(fn_name_ptr).to_string() };
    let fn_ptr = jit_fn_table().lock().unwrap().get(&fn_name).copied().unwrap_or(0);
    alloc_val(TAG_CLOSURE, fn_ptr, 0.0)
}

//     JIT-6: Async

/// Lanza la función JIT identificada por `fn_name_ptr` en un hilo nuevo.
#[no_mangle]
pub extern "C" fn rt_call_async(fn_name_ptr: i64, n_args: i64) -> i64 {
    let fn_name = unsafe { cstr_to_str(fn_name_ptr).to_string() };
    let fn_ptr = {
        let table = jit_fn_table().lock().unwrap();
        match table.get(&fn_name).copied() {
            Some(p) => p,
            None => return fallar(format!("async function '{}' does not exist", fn_name)),
        }
    };

    let args: Vec<i64> = ARG_BUF.with(|b| {
        let mut buf = b.borrow_mut();
        let n = n_args as usize;
        buf.drain(..n).collect()
    });

    let task = Arc::new(JitTask { result: Mutex::new(None), done: Condvar::new() });
    let task_worker = Arc::clone(&task);

    // Pool de hilos compartido (reutiliza workers) en vez de un hilo por spawn.
    crate::task_pool::submit(move || {
        let result = unsafe { call_fn_n(fn_ptr, &args) };
        task_worker.complete(if result == 0 {
            Err(unsafe { val_to_display(&decode_val(rt_take_error())) })
        } else {
            Ok(result)
        });
    });

    alloc_task(task)
}

/// Bloquea hasta que la tarea (TAG_TASK) complete y devuelve su resultado.
#[no_mangle]
pub extern "C" fn rt_await(task: i64) -> i64 {
    unsafe {
        let v = decode_val(task);
        if v.tag == TAG_TASK {
            // Parking real vía Condvar: sin espera activa.
            let arc = &*(v.data_i as *const Arc<JitTask>);
            arc.wait().unwrap_or_else(fallar)
        } else {
            task
        }
    }
}

//     Helpers privados — JIT-4

pub(crate) fn apply_cast(raw: String, cast: &str) -> i64 {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ida_y_vuelta(tag: u8, i: i64, f: f64) -> OrionVal {
        let v = alloc_val(tag, i, f);
        assert_ne!(v, 0, "el 0 está reservado para el error pendiente");
        unsafe { decode_val(v) }
    }

    #[test]
    fn enteros_en_los_bordes_de_48_bits_y_fuera() {
        for n in [0, 1, -1, INT48_MIN, INT48_MAX, INT48_MIN - 1, INT48_MAX + 1, i64::MIN, i64::MAX] {
            let d = ida_y_vuelta(TAG_INT, n, 0.0);
            assert_eq!((d.tag, d.data_i), (TAG_INT, n), "entero {n}");
        }
        // Dentro de 48 bits no reserva memoria; fuera, sí.
        assert_eq!(alloc_val(TAG_INT, INT48_MAX, 0.0) & INT_TAG, INT_TAG);
        assert!((alloc_val(TAG_INT, INT48_MAX + 1, 0.0) as u64) < DOUBLE_OFFSET as u64);
    }

    #[test]
    fn decimales_especiales() {
        for f in [0.0, -0.0, 1.5, -1.5, f64::MAX, f64::MIN, f64::MIN_POSITIVE,
                  f64::INFINITY, f64::NEG_INFINITY] {
            let d = ida_y_vuelta(TAG_FLOAT, 0, f);
            assert_eq!(d.tag, TAG_FLOAT);
            assert_eq!(d.data_f.to_bits(), f.to_bits(), "decimal {f}");
        }
        let d = ida_y_vuelta(TAG_FLOAT, 0, -f64::NAN);
        assert!(d.tag == TAG_FLOAT && d.data_f.is_nan());
    }

    #[test]
    fn null_y_booleanos() {
        assert_eq!(ida_y_vuelta(TAG_NULL, 0, 0.0).tag, TAG_NULL);
        let t = ida_y_vuelta(TAG_BOOL, 1, 0.0);
        let f = ida_y_vuelta(TAG_BOOL, 0, 0.0);
        assert_eq!((t.tag, t.data_i, f.tag, f.data_i), (TAG_BOOL, 1, TAG_BOOL, 0));
    }

    #[test]
    fn un_string_sigue_en_el_heap() {
        let d = ida_y_vuelta(TAG_STR, string_to_cptr("hola".into()), 0.0);
        assert_eq!(d.tag, TAG_STR);
        assert_eq!(unsafe { cstr_to_str(d.data_i) }, "hola");
    }
}
