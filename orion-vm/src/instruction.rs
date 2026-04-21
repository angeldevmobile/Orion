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

    //   Async
    CallAsync(String, u8), // llama fn async en nuevo hilo → push Task
    Await,                  // pop Task → bloquea hasta completar → push resultado

    //   Colecciones                
    MakeList(u8),        // construye lista con N elementos del stack
    MakeDict(u8),        // construye dict con N pares clave-valor
    GetIndex,            // lista[índice]
    SetIndex,            // lista[índice] = valor

    //   Atributos
    GetAttr(String),     // obj.attr
    SetAttr(String),     // obj.attr = valor

    //   OOP
    DefineShape(String),     // registrar shape (no-op, ya cargado desde bytecode)
    CallMethod(String, u8),  // obj.method(args) — nombre del método, num args
    IsInstance(String),      // obj is ShapeName → bool

    //   Stack
    Pop,                 // descarta el top del stack
    Dup,                 // duplica el top del stack

    //   Manejo de errores
    BeginAttempt(usize), // push handler — si hay error, salta a usize (error en stack)
    EndAttempt(usize),   // pop handler (bloque attempt ok), salta a usize (fin del handle)
    Raise,               // pop mensaje del stack y lanza error explícito

    //   I/O nativo
    Show,                // muestra el top del stack

    //   IO del lenguaje: ask / read / write / env
    ReadInput { cast: Option<String>, choices: bool }, // pop prompt (+ lista si choices=true) → stdin → push String
    ReadFile(String),    // pop path → lee archivo → push String (cast: "text","json","lines")
    WriteFile(String),   // pop data, pop path → escribe archivo (mode: "write","append")
    ReadEnv(String),     // pop key → lee var de entorno → push String (cast: "text","int","float")

    //   IA nativa (Fase 4)
    AiAsk,               // pop prompt → llama AI → push String respuesta
    AiLearn,             // pop texto  → guarda en memoria AI de sesión → push String confirmación
    AiSense,             // pop query  → busca en memoria + llama AI → push String respuesta

    //   Servidor HTTP nativo (Fase 7)
    ServeHTTP(String),   // pop puerto (int) → levanta servidor HTTP, handler = String fn_name

    //   Fin
    Halt,
}
