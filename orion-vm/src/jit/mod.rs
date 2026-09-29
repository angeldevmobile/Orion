//! API pública del JIT. Uso: `jit::run_program(&bc)` → Ok(true) si lo ejecutó,
//! Ok(false) para volver al intérprete, Err si falló la compilación.

pub mod aot_backend;
pub mod bridge;
pub mod compiler;
pub mod runtime;
pub mod runtime_oop;

pub use compiler::JitCompiler;

use crate::bytecode::OrionBytecode;

/// Compila y ejecuta el programa con Cranelift. Ok(true): ejecutado; Ok(false):
/// hay instrucciones no soportadas (usar el intérprete); Err: error de compilación.
pub fn run_program(bc: &OrionBytecode) -> Result<bool, String> {
    let mut jit = JitCompiler::new()?;
    jit.run_program(bc)
}
