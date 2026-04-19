use std::fs;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::instruction::Instruction;

/// Definición de una función de usuario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub params: Vec<String>,
    pub body: Vec<Instruction>,
}

/// Formato completo del archivo .orbc
#[derive(Debug, Serialize, Deserialize)]
pub struct OrionBytecode {
    pub main: Vec<Instruction>,
    pub functions: HashMap<String, FunctionDef>,
}

pub fn load(path: &str) -> Result<OrionBytecode, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("No se pudo leer '{}': {}", path, e))?;

    serde_json::from_str(&content)
        .map_err(|e| format!("Error leyendo bytecode: {}", e))
}
