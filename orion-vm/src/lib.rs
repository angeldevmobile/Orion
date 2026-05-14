//! Interfaz de biblioteca de Orion VM.
//!
//! Usos:
//!   - `orion_rt_exec`: punto de entrada C-ABI para ejecutables nativos AOT
//!   - `lexer_fuzz`, `parser_fuzz`, `pipeline_fuzz`: targets de fuzzing
//!   - Módulos públicos: lexer, parser, codegen, etc.

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
pub mod vm;
pub mod builtins;
pub mod eval_value;
pub mod env;
pub mod stdlib_bridge;
pub mod modules;
pub mod eval;
pub mod ai;
pub mod jit;

//    Punto de entrada C-ABI para ejecutables AOT                               
//
// El compilador AOT (aot.rs) genera un main() en Cranelift IR que llama a
// esta función con el bytecode embebido. La staticlib de orion_vm provee el
// símbolo para el paso de enlazado.
//
// Signature: (bytecode_ptr: *const u8, bytecode_len: usize) -> i32 (exit code)

#[no_mangle]
pub extern "C" fn orion_rt_exec(bytecode_ptr: *const u8, bytecode_len: usize) -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(bytecode_ptr, bytecode_len) };

    let bc: bytecode::OrionBytecode = match serde_json::from_slice(bytes) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[orion] bytecode corrupto: {e}");
            return 1;
        }
    };

    let mut machine = vm::VM::new(
        bc.main,
        bc.lines,
        bc.functions,
        bc.shapes,
        bc.extern_fns,
    );

    match machine.run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("[orion] {e}");
            1
        }
    }
}

//    Funciones de fuzzing                                                       

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
