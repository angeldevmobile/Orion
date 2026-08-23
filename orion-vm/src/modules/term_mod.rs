//! term — primitivos de terminal para construir UIs de consola (barras de
//! progreso, spinners, etc.) DESDE Orion, sin hardcodear nada en el runtime.
//!
//! El runtime solo aporta la I/O cruda (la syscall): escribir sin salto de
//! línea, hacer flush y saber si la salida es una terminal. La lógica visual
//! (spinner, barra, colores) se escribe en Orion — ver `packages/progress.orx`.

use crate::eval_value::EvalValue;
use std::io::{IsTerminal, Write};

fn as_text(v: Option<&EvalValue>) -> String {
    match v {
        Some(EvalValue::Str(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // write(texto) → escribe a stdout SIN salto de línea, con flush.
        // Con esto (+ el escape "\r" y códigos ANSI) se dibuja cualquier UI.
        "write" => {
            let s = as_text(args.first());
            let mut out = std::io::stdout().lock();
            out.write_all(s.as_bytes()).map_err(|e| format!("term.write: {}", e))?;
            out.flush().ok();
            Ok(EvalValue::Null)
        }

        // err(texto) → igual pero a stderr. Convención para barras de progreso:
        // así no ensucian la salida real cuando se redirige stdout a un archivo.
        "err" => {
            let s = as_text(args.first());
            let mut out = std::io::stderr().lock();
            out.write_all(s.as_bytes()).map_err(|e| format!("term.err: {}", e))?;
            out.flush().ok();
            Ok(EvalValue::Null)
        }

        // flush() → vacía los búferes de stdout y stderr.
        "flush" => {
            std::io::stdout().flush().ok();
            std::io::stderr().flush().ok();
            Ok(EvalValue::Null)
        }

        // is_tty() → yes si stdout es una terminal interactiva (no un pipe).
        "is_tty" => Ok(EvalValue::Bool(std::io::stdout().is_terminal())),

        // is_etty() → yes si stderr es una terminal (para gatear progreso).
        "is_etty" => Ok(EvalValue::Bool(std::io::stderr().is_terminal())),

        // clear_line() → limpia la línea actual (útil al terminar una barra).
        "clear_line" => {
            let mut out = std::io::stderr().lock();
            out.write_all(b"\r\x1b[2K").map_err(|e| format!("term.clear_line: {}", e))?;
            out.flush().ok();
            Ok(EvalValue::Null)
        }

        other => Err(format!("term.{} does not exist (use: write, err, flush, is_tty, is_etty, clear_line)", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_primitivos_no_fallan() {
        // write/err/flush devuelven Null; no deben entrar en pánico.
        assert!(matches!(call("write", vec![EvalValue::Str("".into())]), Ok(EvalValue::Null)));
        assert!(matches!(call("err", vec![EvalValue::Str("".into())]), Ok(EvalValue::Null)));
        assert!(matches!(call("flush", vec![]), Ok(EvalValue::Null)));
        // is_tty/is_etty devuelven bool (false bajo captura de tests).
        assert!(matches!(call("is_tty", vec![]), Ok(EvalValue::Bool(_))));
        assert!(matches!(call("is_etty", vec![]), Ok(EvalValue::Bool(_))));
        // función desconocida → error claro.
        assert!(call("inexistente", vec![]).is_err());
    }
}
