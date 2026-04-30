/// Puente hacia módulos externos de Orion.
///
/// Todos los módulos stdlib (fs, json, random, strings, datetime,
/// process, env, net) están ahora implementados en Rust nativo
/// bajo `crate::modules`. Este archivo se mantiene como referencia
/// de la arquitectura de migración.

use crate::eval_value::EvalValue;

/// Módulos que están completamente migrados a Rust nativo.
pub const NATIVE_MODULES: &[&str] = &[
    "fs", "json", "random", "strings", "datetime", "process", "env", "net",
];

/// Módulos avanzados que aún no han sido migrados (stdlib avanzada).
pub const PENDING_MODULES: &[&str] = &[
    "vision", "quantum", "ai", "crypto", "matrix", "insight", "cosmos", "timewarp",
    "server",
];

/// Retorna true si el módulo tiene implementación nativa Rust.
pub fn is_native(name: &str) -> bool {
    NATIVE_MODULES.contains(&name)
}

/// Retorna un error descriptivo para módulos avanzados no migrados.
pub fn not_implemented(module: &str, function: &str) -> Result<EvalValue, String> {
    Err(format!(
        "Módulo '{}' (función '{}') aún no migrado a Rust. \
         Está en la lista de módulos avanzados pendientes.",
        module, function
    ))
}
