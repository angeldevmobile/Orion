//! Fuzz target para el pipeline completo: lexer → parser → codegen → VM.
//!
//! Propiedad: ninguna entrada causa panic. Los errores de runtime son aceptables.
//!
//! Uso:
//!   cargo fuzz run fuzz_full_pipeline
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(src) = std::str::from_utf8(data) {
        let _ = orion_vm::pipeline_fuzz(src);
    }
});
