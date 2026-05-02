use std::fs;
use std::path::{Path, PathBuf};
use crate::{lexer, parser};
use crate::ast::Stmt;
use super::banner;

pub fn run_docs(input: &str, output_dir: &str) {
    let input_path = Path::new(input);
    let out_dir = Path::new(output_dir);

    let sources: Vec<PathBuf> = if input_path.is_dir() {
        collect_orx(input_path)
    } else {
        vec![input_path.to_path_buf()]
    };

    if sources.is_empty() {
        banner::warn("No se encontraron archivos .orx");
        return;
    }

    if let Err(e) = fs::create_dir_all(out_dir) {
        banner::fail(&format!("No se pudo crear directorio de salida '{}': {}", output_dir, e));
        std::process::exit(1);
    }

    let mut generated = 0usize;
    for src_path in &sources {
        let src = match fs::read_to_string(src_path) {
            Ok(s) => s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string(),
            Err(e) => {
                banner::warn(&format!("No se pudo leer {}: {}", src_path.display(), e));
                continue;
            }
        };

        let stmts = match lexer::lex(&src).and_then(|t| parser::parse(t).map_err(|e| {
            crate::token::LexError { message: e.message, line: e.line, col: e.col }
        })) {
            Ok(s) => s,
            Err(e) => {
                banner::warn(&format!("{}: error de parse — {}", src_path.display(), e));
                continue;
            }
        };

        let md = render_markdown(src_path, &stmts);
        let stem = src_path.file_stem().unwrap_or_default().to_string_lossy();
        let out_file = out_dir.join(format!("{stem}.md"));

        match fs::write(&out_file, &md) {
            Ok(_) => {
                banner::ok(&format!("{} → {}", src_path.display(), out_file.display()));
                generated += 1;
            }
            Err(e) => banner::warn(&format!("No se pudo escribir {}: {}", out_file.display(), e)),
        }
    }

    println!();
    banner::ok(&format!("{generated} archivo(s) generado(s) en '{output_dir}'"));
}

fn collect_orx(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().map_or(false, |e| e == "orx") {
                result.push(p);
            } else if p.is_dir() {
                result.extend(collect_orx(&p));
            }
        }
    }
    result.sort();
    result
}

fn render_markdown(src_path: &Path, stmts: &[Stmt]) -> String {
    let module_name = src_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut md = format!("# {module_name}\n\n");
    md.push_str(&format!(
        "*Generado automáticamente desde `{}`*\n\n---\n\n",
        src_path.display()
    ));

    let mut has_content = false;

    for stmt in stmts {
        match stmt {
            Stmt::Fn { name, params, ret_type, doc, .. } => {
                has_content = true;
                md.push_str(&format!("## fn `{name}`\n\n"));
                if let Some(d) = doc {
                    md.push_str(&format!("{d}\n\n"));
                }
                md.push_str("**Firma:**\n```orion\n");
                md.push_str(&fn_signature(name, params, ret_type.as_deref()));
                md.push_str("\n```\n\n");
                if !params.is_empty() {
                    md.push_str("**Parámetros:**\n\n");
                    for p in params {
                        let type_str = p.type_hint.as_deref().unwrap_or("any");
                        md.push_str(&format!("- `{}` — `{}`\n", p.name, type_str));
                    }
                    md.push('\n');
                }
                if let Some(r) = ret_type {
                    md.push_str(&format!("**Retorna:** `{r}`\n\n"));
                }
                md.push_str("---\n\n");
            }
            Stmt::AsyncFn { name, params, ret_type, doc, .. } => {
                has_content = true;
                md.push_str(&format!("## async fn `{name}`\n\n"));
                if let Some(d) = doc {
                    md.push_str(&format!("{d}\n\n"));
                }
                md.push_str("**Firma:**\n```orion\n");
                md.push_str(&format!("async {}", fn_signature(name, params, ret_type.as_deref())));
                md.push_str("\n```\n\n");
                if !params.is_empty() {
                    md.push_str("**Parámetros:**\n\n");
                    for p in params {
                        let type_str = p.type_hint.as_deref().unwrap_or("any");
                        md.push_str(&format!("- `{}` — `{}`\n", p.name, type_str));
                    }
                    md.push('\n');
                }
                md.push_str("---\n\n");
            }
            Stmt::Shape { name, fields, acts, doc, .. } => {
                has_content = true;
                md.push_str(&format!("## shape `{name}`\n\n"));
                if let Some(d) = doc {
                    md.push_str(&format!("{d}\n\n"));
                }
                if !fields.is_empty() {
                    md.push_str("**Campos:**\n\n");
                    for f in fields {
                        let type_str = f.type_hint.as_deref().unwrap_or("any");
                        md.push_str(&format!("- `{}` — `{}`\n", f.name, type_str));
                    }
                    md.push('\n');
                }
                if !acts.is_empty() {
                    md.push_str("**Métodos:**\n\n");
                    for a in acts {
                        let param_list: Vec<_> = a.params.iter()
                            .map(|p| {
                                if let Some(t) = &p.type_hint {
                                    format!("{}: {}", p.name, t)
                                } else {
                                    p.name.clone()
                                }
                            })
                            .collect();
                        md.push_str(&format!("- `{}({})`\n", a.name, param_list.join(", ")));
                    }
                    md.push('\n');
                }
                md.push_str("---\n\n");
            }
            Stmt::Const { name, doc, .. } => {
                has_content = true;
                md.push_str(&format!("## const `{name}`\n\n"));
                if let Some(d) = doc {
                    md.push_str(&format!("{d}\n\n"));
                }
                md.push_str("---\n\n");
            }
            _ => {}
        }
    }

    if !has_content {
        md.push_str("*Este módulo no tiene símbolos documentados.*\n");
    }

    md
}

fn fn_signature(name: &str, params: &[crate::ast::Param], ret: Option<&str>) -> String {
    let param_list: Vec<_> = params.iter()
        .map(|p| {
            if let Some(t) = &p.type_hint {
                format!("{}: {}", p.name, t)
            } else {
                p.name.clone()
            }
        })
        .collect();
    let ret_str = ret.map_or(String::new(), |r| format!(" -> {r}"));
    format!("fn {}({}){}", name, param_list.join(", "), ret_str)
}
