use std::fs;
use crate::{lexer, parser, codegen, typechecker};
use crate::token::{Token, TokenKind};
use super::banner;

//   Lint de indentación engañosa
//
// La indentación en Orion es cosmética (los bloques los delimitan las
// llaves), pero cuando MIENTE sobre la estructura real confunde al lector —
// la clase de bug del célebre `goto fail` de Apple. Reglas conservadoras,
// pensadas para cero falsos positivos con los estilos habituales:
//   1. Mezcla de tabs y espacios en la sangría de una misma línea.
//   2. Una `}` que abre línea debe alinear con la línea que abrió su bloque
//      (cubre `}`, `} else {`, `} handle err {`, cierres de dicts…).
//   3. La primera línea dentro de un bloque `{` multilínea debe estar MÁS
//      indentada que la línea que lo abre (si no, parece estar fuera).

/// Ancho visual de la sangría de una línea (tab = 4) y si mezcla tab/espacio.
fn sangria(linea: &str) -> (usize, bool) {
    let mut ancho = 0;
    let (mut tabs, mut espacios) = (false, false);
    for c in linea.chars() {
        match c {
            ' '  => { espacios = true; ancho += 1; }
            '\t' => { tabs = true; ancho += 4; }
            _    => break,
        }
    }
    (ancho, tabs && espacios)
}

/// Devuelve avisos `(línea, mensaje)` de indentación engañosa.
pub fn lint_indentation(src: &str, tokens: &[Token]) -> Vec<(u32, String)> {
    let lineas: Vec<&str> = src.lines().collect();
    let ancho_de = |line: u32| -> Option<usize> {
        lineas.get(line as usize - 1).map(|l| sangria(l).0)
    };

    let mut avisos: Vec<(u32, String)> = Vec::new();

    // Regla 1: tabs y espacios mezclados (solo líneas que abren con un token,
    // así una línea de comentario no genera ruido).
    let mut vistas = std::collections::HashSet::new();
    for t in tokens {
        if vistas.insert(t.line) {
            if let Some(l) = lineas.get(t.line as usize - 1) {
                if sangria(l).1 {
                    avisos.push((t.line, "sangría con tabs Y espacios mezclados".into()));
                }
            }
        }
    }

    // Reglas 2 y 3: seguimiento de llaves en orden de fuente. Las llaves
    // dentro de strings/comentarios no llegan aquí (no son tokens).
    let mut stack: Vec<u32> = Vec::new(); // línea que abrió cada bloque
    let mut primero_en_linea = std::collections::HashSet::new();
    for (i, t) in tokens.iter().enumerate() {
        let es_primero = primero_en_linea.insert(t.line);
        match t.kind {
            TokenKind::LBrace => {
                // Regla 3: primer token del bloque en línea posterior
                if let Some(sig) = tokens.get(i + 1) {
                    if sig.line > t.line && sig.kind != TokenKind::RBrace {
                        if let (Some(a_abre), Some(a_dentro)) = (ancho_de(t.line), ancho_de(sig.line)) {
                            if a_dentro <= a_abre {
                                avisos.push((sig.line, format!(
                                    "indentada como si estuviera FUERA del bloque abierto en la línea {}",
                                    t.line
                                )));
                            }
                        }
                    }
                }
                stack.push(t.line);
            }
            TokenKind::RBrace => {
                if let Some(abre) = stack.pop() {
                    // Regla 2: `}` que abre línea debe alinear con su apertura
                    if es_primero && t.line != abre {
                        if let (Some(a_abre), Some(a_cierra)) = (ancho_de(abre), ancho_de(t.line)) {
                            if a_cierra != a_abre {
                                avisos.push((t.line, format!(
                                    "la llave de cierre no alinea con su apertura (línea {})",
                                    abre
                                )));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    avisos.sort_by_key(|(l, _)| *l);
    avisos
}

pub fn run_check(path: &str, check_types: bool) {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            banner::fail(&format!("No se puede leer '{path}': {e}"));
            std::process::exit(1);
        }
    };

    banner::info(&format!("Verificando: {BOLD}{path}{RESET}",
        BOLD = super::banner::BOLD, RESET = super::banner::RESET, path = path));

    // Phase 1: lex
    let tokens = match lexer::lex(&src) {
        Ok(t) => t,
        Err(e) => {
            banner::fail(&format!("Error léxico  línea {}:{} — {}", e.line, e.col, e.message));
            std::process::exit(1);
        }
    };

    // Phase 1.5: indentación engañosa (avisos, nunca fatal — la indentación
    // en Orion es cosmética, pero no debe mentir sobre la estructura)
    for (line, msg) in lint_indentation(&src, &tokens) {
        banner::warn(&format!("[indentación] línea {line} — {msg}"));
    }

    // Phase 2: parse
    let stmts = match parser::parse(tokens) {
        Ok(s) => s,
        Err(e) => {
            banner::fail(&format!("Error sintáctico  línea {} — {}", e.line, e.message));
            std::process::exit(1);
        }
    };

    // Phase 3: type check (antes de codegen para errores más claros)
    //
    // El chequeo corre SIEMPRE. Antes estaba detrás de `--types`, y el efecto
    // era que `orion check` daba "sin errores" en un archivo que `orion run`
    // se negaba a ejecutar: el comando que existe para validar era más laxo que
    // el que ejecuta, justo al revés de lo que uno espera de un CI. Nada de lo
    // que se rechace aquí pasaba antes de verdad; solo pasaba desapercibido.
    //
    // `--types` ya no decide SI se chequea, sino cuánto se cuenta: sin él se
    // muestran los errores, con él también las advertencias.
    let issues = typechecker::type_check(&stmts);
    let errors: Vec<_> = issues.iter().filter(|i| i.kind == "error").collect();
    let warnings: Vec<_> = issues.iter().filter(|i| i.kind == "warning").collect();

    if check_types {
        for w in &warnings {
            let prefix = if w.line > 0 { format!("línea {} — ", w.line) } else { String::new() };
            banner::warn(&format!("[advertencia] {}{}", prefix, w.message));
        }
        if issues.is_empty() {
            banner::ok("Type check — sin errores de tipos");
        }
    }
    for e in &errors {
        let prefix = if e.line > 0 { format!("línea {} — ", e.line) } else { String::new() };
        banner::fail(&format!("[tipo] {}{}", prefix, e.message));
    }
    if !errors.is_empty() {
        std::process::exit(1);
    }

    // Phase 4: codegen (detecta errores semánticos adicionales)
    if let Err(e) = codegen::compile(stmts) {
        banner::fail(&format!("Error semántico  línea {} — {}", e.line, e.message));
        std::process::exit(1);
    }

    banner::ok(&format!("'{path}' — sin errores"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn avisos(src: &str) -> Vec<(u32, String)> {
        let tokens = lexer::lex(src).expect("lex");
        lint_indentation(src, &tokens)
    }

    #[test]
    fn indent_correcta_sin_avisos() {
        let src = "fn f(x) {\n    if x > 1 {\n        return x\n    } else {\n        return 0\n    }\n}\nshow f(2)\n";
        assert!(avisos(src).is_empty(), "código bien indentado no debe avisar");
    }

    #[test]
    fn interpolacion_y_dicts_sin_avisos() {
        // Llaves de interpolación (dentro del string) y dicts multilínea con
        // estilo colgante: cero falsos positivos.
        let src = "x = 1\nshow \"valor ${x}\"\nd = {\n    \"a\": 1,\n    \"b\": 2\n}\n";
        assert!(avisos(src).is_empty(), "interpolación/dict no deben avisar: {:?}", avisos(src));
    }

    #[test]
    fn cierre_desalineado_avisa() {
        let src = "if x > 1 {\n    show x\n  }\n";
        let a = avisos(src);
        assert_eq!(a.len(), 1, "{:?}", a);
        assert_eq!(a[0].0, 3);
        assert!(a[0].1.contains("no alinea"));
    }

    #[test]
    fn cuerpo_que_parece_fuera_avisa() {
        // El cuerpo al mismo nivel que el `if`: parece estar fuera del bloque.
        let src = "if x > 1 {\nshow x\n}\n";
        let a = avisos(src);
        assert_eq!(a.len(), 1, "{:?}", a);
        assert_eq!(a[0].0, 2);
        assert!(a[0].1.contains("FUERA"));
    }

    #[test]
    fn tabs_y_espacios_mezclados_avisa() {
        let src = "if x > 1 {\n \tshow x\n}\n";
        let a = avisos(src);
        assert!(a.iter().any(|(l, m)| *l == 2 && m.contains("mezclados")), "{:?}", a);
    }

    #[test]
    fn else_en_linea_de_cierre_sin_avisos() {
        let src = "if x > 1 {\n    show 1\n} else {\n    show 2\n}\n";
        assert!(avisos(src).is_empty(), "{:?}", avisos(src));
    }
}
