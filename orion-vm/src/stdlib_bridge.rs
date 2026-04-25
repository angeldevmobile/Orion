/// Puente hacia los módulos Python de Orion (server, net, fs, etc.).
///
/// En Fase 5B, los módulos externos siguen corriendo en Python.
/// Este módulo centraliza las llamadas de inter-proceso para cuando
/// el evaluador Rust necesite invocar funcionalidad de módulos.
///
/// Por ahora expone stubs con errores descriptivos; cada módulo
/// se implementará en fases posteriores.

use crate::eval_value::EvalValue;
use std::collections::HashMap;

/// Representa un módulo externo cargado.
pub struct ExternalModule {
    pub name: String,
    /// Funciones exportadas como closures Rust (para módulos nativos futuros).
    pub functions: HashMap<String, fn(Vec<EvalValue>) -> Result<EvalValue, String>>,
}

/// Intenta cargar un módulo por nombre.
/// Retorna None si el módulo no está soportado en el evaluador Rust aún.
pub fn load_module(name: &str) -> Option<ExternalModule> {
    match name {
        // Módulos que aún delegan a Python
        "server" | "net" | "fs" | "json" | "env" |
        "random" | "datetime" | "process" | "strings" => None,
        _ => None,
    }
}

/// Llama una función de módulo externo via subprocess Python.
/// Se usa como fallback cuando el módulo no tiene implementación nativa Rust.
pub fn call_python_module(
    module: &str,
    function: &str,
    _args: Vec<EvalValue>,
) -> Result<EvalValue, String> {
    Err(format!(
        "Módulo '{}' (función '{}') requiere el runtime Python. \
         Usa `orion {}` en vez de `orion --eval` para programas con módulos externos.",
        module, function, module
    ))
}
