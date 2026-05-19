use std::fs;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use crate::instruction::Instruction;

/// Magic number que identifica archivos .orbc en formato binario
const MAGIC: &[u8] = b"ORBC";

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
    pub params: Vec<String>,
    pub ret_type: String,
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

/// Carga bytecode desde disco.
/// Detecta automáticamente el formato: binario (ORBC magic) o JSON legacy.
pub fn load(path: &str) -> Result<OrionBytecode, String> {
    let bytes = fs::read(path)
        .map_err(|e| format!("No se pudo leer '{}': {}", path, e))?;

    if bytes.starts_with(MAGIC) {
        // Formato binario: saltar los 4 bytes del magic
        bincode::deserialize(&bytes[MAGIC.len()..])
            .map_err(|e| format!("Error leyendo bytecode binario: {}", e))
    } else {
        // Formato JSON legado
        let text = String::from_utf8(bytes)
            .map_err(|e| format!("Bytecode no es UTF-8 válido: {}", e))?;
        serde_json::from_str(&text)
            .map_err(|e| format!("Error leyendo bytecode JSON: {}", e))
    }
}

/// Guarda bytecode en formato binario eficiente (.orbc).
/// ~10-50x más rápido de deserializar que JSON.
pub fn save(bc: &OrionBytecode, path: &str) -> Result<(), String> {
    let payload = bincode::serialize(bc)
        .map_err(|e| format!("Error serializando bytecode: {}", e))?;
    let mut out = Vec::with_capacity(MAGIC.len() + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&payload);
    fs::write(path, out)
        .map_err(|e| format!("No se pudo escribir '{}': {}", path, e))
}

/// Guarda bytecode en formato JSON (útil para depuración / inspección humana).
pub fn save_json(bc: &OrionBytecode, path: &str) -> Result<(), String> {
    let text = serde_json::to_string_pretty(bc)
        .map_err(|e| format!("Error serializando bytecode JSON: {}", e))?;
    fs::write(path, text)
        .map_err(|e| format!("No se pudo escribir '{}': {}", path, e))
}
