//! Fuzz target para el lexer de Orion.
//!
//! Propiedad: el lexer nunca debe hacer panic con ninguna entrada.
//! Los errores de lexing son válidos (Err), pero panic/unwrap no lo son.
//!
//! Uso:
//!   cargo install cargo-fuzz
//!   cargo fuzz run fuzz_lexer
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convertir bytes arbitrarios a string (ignorar si no es UTF-8 válido)
    if let Ok(src) = std::str::from_utf8(data) {
        let _ = orion_vm::lexer_fuzz(src);
    }
});
