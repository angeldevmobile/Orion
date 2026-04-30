//! Funciones de runtime en C ABI llamadas desde código JIT compilado con Cranelift.
//! Cada función es `#[no_mangle] extern "C"` para que el linker dinámico del JIT
//! pueda resolverla por nombre de símbolo.

use std::io::{self, Write};

// ── Mostrar valores ────────────────────────────────────────────────────────────

/// Imprime un entero (i64) desde código JIT.
#[no_mangle]
pub extern "C" fn orion_rt_show_int(v: i64) {
    println!("{v}");
    let _ = io::stdout().flush();
}

/// Imprime un booleano (i64: 0 = no, distinto de 0 = yes) desde código JIT.
#[no_mangle]
pub extern "C" fn orion_rt_show_bool(v: i64) {
    println!("{}", if v != 0 { "yes" } else { "no" });
    let _ = io::stdout().flush();
}

/// Imprime una cadena terminada en null (puntero pasado como i64) desde código JIT.
#[no_mangle]
pub extern "C" fn orion_rt_show_str(raw_ptr: i64) {
    let ptr = raw_ptr as *const u8;
    if ptr.is_null() {
        println!("null");
        let _ = io::stdout().flush();
        return;
    }
    // Calcular longitud hasta el '\0'
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        match std::str::from_utf8(slice) {
            Ok(s) => println!("{s}"),
            Err(_) => println!("<cadena inválida UTF-8>"),
        }
    }
    let _ = io::stdout().flush();
}

// ── Aritmética segura ──────────────────────────────────────────────────────────

/// División entera con verificación de divisor cero.
#[no_mangle]
pub extern "C" fn orion_rt_div_int(a: i64, b: i64) -> i64 {
    if b == 0 {
        eprintln!("[JIT Error] División por cero");
        std::process::exit(1);
    }
    a / b
}

/// Módulo entero con verificación de divisor cero.
#[no_mangle]
pub extern "C" fn orion_rt_mod_int(a: i64, b: i64) -> i64 {
    if b == 0 {
        eprintln!("[JIT Error] Módulo por cero");
        std::process::exit(1);
    }
    a % b
}

/// Potencia entera (base^exp). Exp negativo devuelve 0.
#[no_mangle]
pub extern "C" fn orion_rt_pow_int(base: i64, exp: i64) -> i64 {
    if exp < 0 {
        return 0;
    }
    base.pow(exp as u32)
}

// ── Flotantes ─────────────────────────────────────────────────────────────────

/// Imprime un f64 desde código JIT.
#[no_mangle]
pub extern "C" fn orion_rt_show_float(v: f64) {
    println!("{v}");
    let _ = io::stdout().flush();
}

/// Potencia f64 (base^exp) desde código JIT.
#[no_mangle]
pub extern "C" fn orion_rt_pow_f64(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}
