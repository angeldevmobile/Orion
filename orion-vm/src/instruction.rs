use serde::{Deserialize, Serialize};

/// Set completo de instrucciones de la VM Orion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Instruction {
    //   Constantes                
    LoadInt(i64),        // push int literal
    LoadFloat(f64),      // push float literal
    LoadStr(String),     // push string literal
    LoadBool(bool),      // push yes/no
    LoadNull,            // push null

    //   Variables                 
    LoadVar(String),     // push variable del scope
    StoreVar(String),    // pop → guardar en variable
    StoreConst(String),  // pop → guardar como constante (inmutable)

    //   Aritmética                
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Neg,                 // unary minus

    //   Comparación                
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,

    //   Lógica                  
    And,
    Or,
    Not,

    //   Control de flujo             
    Jump(usize),         // salto incondicional a índice
    JumpIfFalse(usize),  // salto si top del stack es falsy
    JumpIfTrue(usize),   // salto si top del stack es truthy

    //   Funciones                 
    Call(String, u8),    // nombre de función, num args
    Return,
    MakeFunction(String, Vec<String>, usize), // nombre, params, índice del cuerpo

    //   Colecciones                
    MakeList(u8),        // construye lista con N elementos del stack
    MakeDict(u8),        // construye dict con N pares clave-valor
    GetIndex,            // lista[índice]
    SetIndex,            // lista[índice] = valor

    //   Atributos                 
    GetAttr(String),     // obj.attr
    SetAttr(String),     // obj.attr = valor

    //   Stack                   
    Pop,                 // descarta el top del stack
    Dup,                 // duplica el top del stack

    //   I/O nativo                
    Show,                // muestra el top del stack

    //   Fin                    
    Halt,
}
