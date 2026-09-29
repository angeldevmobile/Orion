//! Orion VM como biblioteca: entrada C-ABI de los ejecutables AOT, targets de
//! fuzzing y los módulos públicos (lexer, parser, codegen…).

pub mod token;
pub mod ast;
pub mod paths;
pub mod instruction;
pub mod bytecode;
pub mod value;
pub mod gc;
pub mod task_pool;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod codegen;
pub mod named_args;
pub mod deprecated;
pub mod typechecker;
pub mod vm;
pub mod eval_value;
pub mod modules;
pub mod ai;
pub mod jit;

/// El typechecker consulta `crate::cli::builtins`: aquí se declaran solo los
/// dos archivos del registro, no todo el CLI.
pub mod cli {
    pub mod builtins;
    pub mod builtins_gen;
}

//    Punto de entrada C-ABI para ejecutables AOT
// El main() que genera aot.rs llama aquí con el bytecode embebido:
// (bytecode_ptr: *const u8, bytecode_len: usize) -> i32 (código de salida).

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

    // Compilar a nativo con Cranelift antes de recurrir al intérprete: el
    // ejecutable AOT ya paga el arranque, no tiene sentido que además
    // interprete lo que el JIT puede compilar. `ORION_NO_JIT=1` lo desactiva.
    if std::env::var_os("ORION_NO_JIT").is_none() {
        match jit::run_program(&bc) {
            Ok(true) => return 0,
            // No elegible o el JIT no pudo compilar: la VM sí sabe ejecutarlo.
            // A diferencia de `orion --jit`, aquí un Err no es fatal — un
            // binario distribuido debe correr igual, no abortar.
            Ok(false) => {}
            Err(e) => eprintln!("[orion] JIT no disponible ({e}) → intérprete"),
        }
    }

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
