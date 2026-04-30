// Módulos stdlib básicos
pub mod fs;
pub mod json_mod;
pub mod random_mod;
pub mod strings_mod;
pub mod datetime_mod;
pub mod process_mod;
pub mod env_mod;
pub mod net_mod;

// Módulos stdlib avanzados
pub mod ai_mod;
pub mod crypto_mod;
pub mod matrix_mod;
pub mod quantum_mod;
pub mod cosmos_mod;
pub mod timewarp_mod;
pub mod vision_mod;
pub mod insight_mod;

use crate::eval_value::EvalValue;

/// Dispatcher principal: módulo → función → args.
pub fn call(module: &str, function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match module {
        // Básicos
        "fs"       => fs::call(function, args),
        "json"     => json_mod::call(function, args),
        "random"   => random_mod::call(function, args),
        "strings"  => strings_mod::call(function, args),
        "datetime" => datetime_mod::call(function, args),
        "process"  => process_mod::call(function, args),
        "env"      => env_mod::call(function, args),
        "net"      => net_mod::call(function, args),
        // Avanzados
        "ai"       => ai_mod::call(function, args),
        "crypto"   => crypto_mod::call(function, args),
        "matrix"   => matrix_mod::call(function, args),
        "quantum"  => quantum_mod::call(function, args),
        "cosmos"   => cosmos_mod::call(function, args),
        "timewarp" => timewarp_mod::call(function, args),
        "vision"   => vision_mod::call(function, args),
        "insight"  => insight_mod::call(function, args),
        _ => Err(format!("Módulo '{}' no encontrado en la stdlib de Orion.", module)),
    }
}

pub fn is_known_module(name: &str) -> bool {
    matches!(
        name,
        // Básicos
        "fs" | "json" | "random" | "strings" | "datetime" |
        "process" | "env" | "net" |
        // Avanzados
        "ai" | "crypto" | "matrix" | "quantum" |
        "cosmos" | "timewarp" | "vision" | "insight"
    )
}
