//! Fuzz target para el parser de Orion.
//!
//! Propiedad: lexer + parser nunca hacen panic con ninguna entrada UTF-8.
//!
//! Uso:
//!   cargo fuzz run fuzz_parser
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(src) = std::str::from_utf8(data) {
        let _ = orion_vm::parser_fuzz(src);
    }
});
