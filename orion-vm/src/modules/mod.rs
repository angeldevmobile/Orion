// Módulos stdlib básicos
pub mod fs;
pub mod json_mod;
pub mod random_mod;
pub mod strings_mod;
pub mod datetime_mod;
pub mod process_mod;
pub mod env_mod;
pub mod net_mod;

// Bloque D — Sistema moderno
pub mod log_mod;
pub mod config_mod;
pub mod secret_mod;
pub mod zip_mod;
pub mod stream_mod;
pub mod crypto2_mod;

// Bloque B — Web moderna
pub mod router_mod;
pub mod middleware_mod;
pub mod sse_mod;
pub mod proto_mod;

// Bloque C — AI nativa avanzada
pub mod llm_mod;
pub mod embed_mod;
pub mod vector_mod;

// Módulos stdlib avanzados
pub mod ai_mod;
pub mod crypto_mod;
pub mod matrix_mod;
pub mod quantum_mod;
pub mod cosmos_mod;
pub mod timewarp_mod;
pub mod vision_mod;
pub mod insight_mod;

// Módulos de datos
pub mod csv_mod;
pub mod excel_mod;
pub mod regex_mod;
pub mod table_mod;

// Interfaces nativas de Orion
pub mod gui;

//   Backend core                                
pub mod db_mod;
pub mod auth_mod;
pub mod cache_mod;
pub mod mail_mod;

//   Automatización                               
pub mod tarea_mod;
pub mod cola_mod;
pub mod watch_mod;

//   Validación                                 
pub mod validate_mod;

//   Utilidades modernas                             
pub mod ws_mod;
pub mod template_mod;
pub mod formato_mod;
pub mod grafo_mod;
pub mod pdf_mod;

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
        // Datos
        "csv"      => csv_mod::call(function, args),
        "excel"    => excel_mod::call(function, args),
        "regex"    => regex_mod::call(function, args),
        "table" | "df" => table_mod::call(function, args),
        // Interfaces nativas
        "gui"      => gui::call(function, args),
        // Backend core
        "db"       => db_mod::call(function, args),
        "auth"     => auth_mod::call(function, args),
        "cache"    => cache_mod::call(function, args),
        "mail"     => mail_mod::call(function, args),
        // Automatización
        "tarea"    => tarea_mod::call(function, args),
        "cola"     => cola_mod::call(function, args),
        "watch"    => watch_mod::call(function, args),
        // Validación
        "validate" => validate_mod::call(function, args),
        // Utilidades modernas
        "ws"       => ws_mod::call(function, args),
        "template" => template_mod::call(function, args),
        "formato"  => formato_mod::call(function, args),
        "grafo"    => grafo_mod::call(function, args),
        "pdf"      => pdf_mod::call(function, args),
        // Bloque D — Sistema moderno
        "log"      => log_mod::call(function, args),
        "config"   => config_mod::call(function, args),
        "secret"   => secret_mod::call(function, args),
        "zip"      => zip_mod::call(function, args),
        "stream"   => stream_mod::call(function, args),
        "crypto2"  => crypto2_mod::call(function, args),
        // Bloque B — Web moderna
        "router"     => router_mod::call(function, args),
        "middleware" => middleware_mod::call(function, args),
        "sse"        => sse_mod::call(function, args),
        "proto"      => proto_mod::call(function, args),
        // Bloque C — AI nativa avanzada
        "llm"                  => llm_mod::call(function, args),
        "embed" | "embeddings" => embed_mod::call(function, args),
        "vector"               => vector_mod::call(function, args),
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
        "cosmos" | "timewarp" | "vision" | "insight" |
        // Datos
        "csv" | "excel" | "regex" | "table" | "df" |
        // Interfaces
        "gui" |
        // Backend core
        "db" | "auth" | "cache" | "mail" |
        // Automatización
        "tarea" | "cola" | "watch" |
        // Validación
        "validate" |
        // Utilidades modernas
        "ws" | "template" | "formato" | "grafo" | "pdf" |
        // Bloque D
        "log" | "config" | "secret" | "zip" | "stream" | "crypto2" |
        // Bloque B
        "router" | "middleware" | "sse" | "proto" |
        // Bloque C
        "llm" | "embed" | "embeddings" | "vector"
    )
}
