//! Interfaz de biblioteca para fuzzing y tests externos de Orion VM.
//! Expone lexer, parser y codegen como funciones puras sin side effects.

pub mod token;
pub mod ast;
pub mod instruction;
pub mod bytecode;
pub mod value;
pub mod gc;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod codegen;
pub mod typechecker;

/// Tokeniza `src`. Nunca debe hacer panic — los errores son Err.
pub fn lexer_fuzz(src: &str) -> Result<Vec<token::Token>, String> {
    lexer::lex(src).map_err(|e| e.message)
}

/// Tokeniza y parsea `src`. Nunca debe hacer panic.
pub fn parser_fuzz(src: &str) -> Result<Vec<ast::Stmt>, String> {
    let tokens = lexer::lex(src).map_err(|e| e.message)?;
    parser::parse(tokens).map_err(|e| e.message)
}

/// Pipeline lexer → parser → codegen sin ejecutar la VM (sin side effects).
pub fn pipeline_fuzz(src: &str) -> Result<(), String> {
    let tokens = lexer::lex(src).map_err(|e| e.message)?;
    let ast    = parser::parse(tokens).map_err(|e| e.message)?;
    codegen::compile(ast).map_err(|e| e.message)?;
    Ok(())
}
