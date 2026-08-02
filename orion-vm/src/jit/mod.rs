//! API pública del módulo JIT de Orion — Fase 5.
//!
//! Uso recomendado (programa completo):
//! ```ignore
//! match jit::run_program(&bc) {
//!     Ok(true)  => { /* JIT ejecutó */ }
//!     Ok(false) => { /* fallback al intérprete */ }
//!     Err(e)    => { /* error de compilación JIT */ }
//! }
//! ```

pub mod aot_backend;
pub mod bridge;
pub mod compiler;
pub mod runtime;
pub mod runtime_oop;

pub use compiler::JitCompiler;

use crate::bytecode::OrionBytecode;

/// Compila y ejecuta un programa completo (main + funciones) con Cranelift JIT.
///
/// - `Ok(true)`  → JIT compiló y ejecutó con éxito.
/// - `Ok(false)` → hay instrucciones no soportadas → usar intérprete.
/// - `Err(msg)`  → error real de compilación JIT.
pub fn run_program(bc: &OrionBytecode) -> Result<bool, String> {
    let mut jit = JitCompiler::new()?;
    jit.run_program(bc)
}
