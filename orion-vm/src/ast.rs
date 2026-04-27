#![allow(dead_code)]
/// AST de Orion — nodos de expresión y declaración
/// Espejo exacto de las tuplas Python que produce core/parser.py

//   Expresiones                                 

#[derive(Debug, Clone)]
pub enum Expr {
    // Literales
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
    Undefined,

    // Variables / identificadores
    Ident(String),

    // Operaciones
    BinaryOp { op: String, left: Box<Expr>, right: Box<Expr> },
    UnaryOp  { op: String, expr: Box<Expr> },

    // Colecciones
    List(Vec<Expr>),
    Dict(Vec<(String, Expr)>),

    // Llamadas
    Call   { callee: Box<Expr>, args: Vec<Expr>, kwargs: Vec<(String, Expr)> },
    CallMethod { method: String, receiver: Box<Expr>, args: Vec<Expr>, kwargs: Vec<(String, Expr)> },

    // Acceso a miembros
    AttrAccess  { object: Box<Expr>, attr: String },
    Index       { object: Box<Expr>, index: Box<Expr> },
    SliceAccess { object: Box<Expr>, start: Option<Box<Expr>>, end: Option<Box<Expr>> },
    NullSafe    { object: Box<Expr>, attr: String },

    // Funciones
    Lambda { params: Vec<String>, body: Vec<Stmt> },

    // Comprobación de tipo
    IsCheck { expr: Box<Expr>, shape: String },

    // Async
    Await(Box<Expr>),
}

//   Declaraciones                                

#[derive(Debug, Clone)]
pub enum Stmt {
    // Asignación
    Assign      { name: String, value: Expr, line: u32 },
    TypedAssign { name: String, type_hint: String, value: Expr, line: u32 },
    AssignIndex { object: Expr, index: Expr, value: Expr, line: u32 },
    AssignAttr  { object: Expr, attr: String, value: Expr, line: u32 },
    AugAssign   { name: String, op: String, value: Expr, line: u32 },
    Const   { name: String, value: Expr, line: u32 },

    // Control de flujo
    If      { cond: Expr, then_body: Vec<Stmt>, else_body: Vec<Stmt>, line: u32 },
    While   { cond: Expr, body: Vec<Stmt>, line: u32 },
    For     { var: String, iter: Expr, body: Vec<Stmt>, line: u32 },
    Match   { expr: Expr, arms: Vec<MatchArm>, line: u32 },
    Return  { value: Option<Expr>, line: u32 },
    Break   { line: u32 },
    Continue { line: u32 },

    // Funciones / clases
    Fn      { name: String, params: Vec<Param>, body: Vec<Stmt>, line: u32 },
    AsyncFn { name: String, params: Vec<Param>, body: Vec<Stmt>, line: u32 },
    Shape   { name: String, fields: Vec<FieldDef>, on_create: Option<(Vec<Param>, Vec<Stmt>)>, acts: Vec<ActDef>, using: Vec<String>, line: u32 },

    // Módulos
    Use     { path: String, alias: Option<String>, selective: Option<Vec<String>>, line: u32 },

    // Salida
    Show    { value: Expr, line: u32 },

    // Errores
    ErrorStmt { msg: Expr, line: u32 },
    Attempt   { body: Vec<Stmt>, handler: Option<Handler>, line: u32 },

    // I/O nativo
    Ask     { prompt: Expr, var: String, cast: Option<String>, choices: Option<Expr>, line: u32 },
    Read    { path: Expr, var: String, line: u32 },
    Write   { path: Expr, content: Expr, line: u32 },
    Append  { path: Expr, content: Expr, line: u32 },

    // Servidor / red
    Serve   { port: Expr, routes: Vec<Stmt>, line: u32 },
    Route   { method: String, path: String, body: Vec<Stmt>, line: u32 },

    // IA / simbiótico
    Think   { prompt: Expr, line: u32 },
    Learn   { text: Expr, line: u32 },
    Sense   { query: Expr, line: u32 },

    // Concurrencia
    Spawn   { call: Expr, line: u32 },
    Await   { expr: Expr, var: Option<String>, line: u32 },

    // Expresión como declaración (llamadas de función sueltas, etc.)
    Expr    { expr: Expr, line: u32 },
}

//   Tipos auxiliares                              

/// Parámetro de función con tipo opcional y valor por defecto opcional
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub type_hint: Option<String>,
    pub default: Option<Expr>,
}

impl Param {
    pub fn simple(name: impl Into<String>) -> Self {
        Param { name: name.into(), type_hint: None, default: None }
    }
}

/// Brazo de un `match`
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Expr,
    pub body: Vec<Stmt>,
}

/// Bloque `handle` de un `attempt`
#[derive(Debug, Clone)]
pub struct Handler {
    pub err_name: String,
    pub body: Vec<Stmt>,
}

/// Campo de un `shape`
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub type_hint: Option<String>,
    pub default: Option<Expr>,
}

/// Método (`act`) de un `shape`
#[derive(Debug, Clone)]
pub struct ActDef {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
}
