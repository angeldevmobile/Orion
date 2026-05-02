use std::fs;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use crate::instruction::Instruction;

/// Definición de una función de usuario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub params: Vec<String>,
    pub body: Vec<Instruction>,
    #[serde(default)]
    pub lines: Vec<u32>,
}

/// Valor por defecto de un campo de shape (mini-bytecode que evalúa al default)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    #[serde(rename = "type")]
    pub type_hint: Option<String>,
    pub default: Vec<Instruction>,
}

/// Definición de un act (método) de un shape
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActDef {
    pub params: Vec<String>,
    pub body: Vec<Instruction>,
    #[serde(default)]
    pub lines: Vec<u32>,
}

/// Definición completa de un shape
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShapeDef {
    pub fields: Vec<FieldDef>,
    pub on_create: Option<ActDef>,
    pub acts: IndexMap<String, ActDef>,
    #[serde(default)]
    pub using: Vec<String>,
}

/// Definición de una función C externa (FFI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternFnDef {
    /// Tipos de los parámetros: "int", "ptr", "string", "bool"
    pub params: Vec<String>,
    /// Tipo de retorno: "int", "ptr", "string", "bool", "void"
    pub ret_type: String,
    /// Nombre de la librería dinámica, ej: "sqlite3"
    pub lib: String,
}

/// Formato completo del archivo .orbc
#[derive(Debug, Serialize, Deserialize)]
pub struct OrionBytecode {
    pub main: Vec<Instruction>,
    #[serde(default)]
    pub lines: Vec<u32>,
    pub functions: IndexMap<String, FunctionDef>,
    #[serde(default)]
    pub shapes: IndexMap<String, ShapeDef>,
    #[serde(default)]
    pub extern_fns: IndexMap<String, ExternFnDef>,
}

pub fn load(path: &str) -> Result<OrionBytecode, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("No se pudo leer '{}': {}", path, e))?;

    serde_json::from_str(&content)
        .map_err(|e| format!("Error leyendo bytecode: {}", e))
}
