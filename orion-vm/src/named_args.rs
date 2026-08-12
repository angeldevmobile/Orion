//! Pase de argumentos con nombre (named args).
//!
//! Reordena `f(x = 1, y = 2)` a forma posicional usando la firma de `f`,
//! rellenando huecos intermedios con los valores por defecto del parámetro. Se
//! ejecuta sobre el AST antes de compilar. Lo que no se pueda resolver (módulo
//! nativo, método, callee dinámico o función desconocida) conserva sus kwargs;
//! codegen los rechaza luego con un error claro.

use crate::ast::{Expr, Param, Stmt};
use crate::codegen::CodegenError;
use indexmap::IndexMap as HashMap;

type Sigs = HashMap<String, Vec<Param>>;

fn err(msg: String) -> CodegenError {
    CodegenError { message: msg, line: 0 }
}

pub fn resolve(stmts: &mut [Stmt]) -> Result<(), CodegenError> {
    let sigs = collect_sigs(stmts);
    for s in stmts.iter_mut() {
        walk_stmt(s, &sigs)?;
    }
    Ok(())
}

fn collect_sigs(stmts: &[Stmt]) -> Sigs {
    let mut m = HashMap::new();
    for s in stmts {
        if let Stmt::Fn { name, params, .. } | Stmt::AsyncFn { name, params, .. } = s {
            m.insert(name.clone(), params.clone());
        }
    }
    m
}

/// Reordena los args de UNA llamada a forma posicional. No-op si no hay kwargs o
/// si la función no es de usuario conocida (se deja para el error de codegen).
fn resolve_call(
    callee: &Expr,
    args: &mut Vec<Expr>,
    kwargs: &mut Vec<(String, Expr)>,
    sigs: &Sigs,
) -> Result<(), CodegenError> {
    if kwargs.is_empty() {
        return Ok(());
    }
    let fname = match callee {
        Expr::Ident(n) => n,
        _ => return Ok(()),
    };
    let params = match sigs.get(fname) {
        Some(p) => p,
        None => return Ok(()),
    };

    if args.len() > params.len() {
        return Err(err(format!("'{}' recibió demasiados argumentos", fname)));
    }
    // índice de cada parámetro por nombre
    let mut by_name: HashMap<&str, usize> = HashMap::new();
    for (i, p) in params.iter().enumerate() {
        by_name.insert(p.name.as_str(), i);
    }

    // colocar posicionales y luego nombrados en sus ranuras
    let mut slots: Vec<Option<Expr>> = (0..params.len()).map(|_| None).collect();
    for (i, a) in args.drain(..).enumerate() {
        slots[i] = Some(a);
    }
    for (k, v) in kwargs.drain(..) {
        let idx = *by_name
            .get(k.as_str())
            .ok_or_else(|| err(format!("'{}' no tiene un parámetro llamado '{}'", fname, k)))?;
        if slots[idx].is_some() {
            return Err(err(format!("argumento '{}' de '{}' dado dos veces", k, fname)));
        }
        slots[idx] = Some(v);
    }

    // construir posicional hasta la ranura más alta provista; rellenar huecos
    // intermedios con el default del parámetro (las ranuras de cola sin valor se
    // dejan al relleno de defaults de la VM).
    let upto = match slots.iter().rposition(|s| s.is_some()) {
        Some(i) => i,
        None => return Ok(()),
    };
    let mut new_args = Vec::with_capacity(upto + 1);
    for i in 0..=upto {
        if let Some(e) = slots[i].take() {
            new_args.push(e);
        } else if let Some(def) = &params[i].default {
            new_args.push(def.clone());
        } else {
            return Err(err(format!("falta el argumento '{}' de '{}'", params[i].name, fname)));
        }
    }
    *args = new_args;
    Ok(())
}

fn walk_exprs(es: &mut [Expr], sigs: &Sigs) -> Result<(), CodegenError> {
    for e in es.iter_mut() {
        walk_expr(e, sigs)?;
    }
    Ok(())
}

fn walk_stmts(ss: &mut [Stmt], sigs: &Sigs) -> Result<(), CodegenError> {
    for s in ss.iter_mut() {
        walk_stmt(s, sigs)?;
    }
    Ok(())
}

fn walk_expr(e: &mut Expr, sigs: &Sigs) -> Result<(), CodegenError> {
    match e {
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null
        | Expr::Undefined | Expr::Ident(_) => {}
        Expr::BinaryOp { left, right, .. } => {
            walk_expr(left, sigs)?;
            walk_expr(right, sigs)?;
        }
        Expr::UnaryOp { expr, .. } => walk_expr(expr, sigs)?,
        Expr::Ternary { cond, then_e, else_e } => {
            walk_expr(cond, sigs)?;
            walk_expr(then_e, sigs)?;
            walk_expr(else_e, sigs)?;
        }
        Expr::List(v) => walk_exprs(v, sigs)?,
        Expr::Dict(pairs) => {
            for (_, v) in pairs.iter_mut() {
                walk_expr(v, sigs)?;
            }
        }
        Expr::Call { callee, args, kwargs } => {
            walk_expr(callee, sigs)?;
            walk_exprs(args, sigs)?;
            for (_, v) in kwargs.iter_mut() {
                walk_expr(v, sigs)?;
            }
            resolve_call(callee, args, kwargs, sigs)?;
        }
        Expr::CallMethod { receiver, args, kwargs, .. } => {
            walk_expr(receiver, sigs)?;
            walk_exprs(args, sigs)?;
            for (_, v) in kwargs.iter_mut() {
                walk_expr(v, sigs)?;
            }
            // Los métodos no resuelven named args aquí; codegen los rechaza.
        }
        Expr::AttrAccess { object, .. } | Expr::NullSafe { object, .. } => walk_expr(object, sigs)?,
        Expr::Index { object, index } => {
            walk_expr(object, sigs)?;
            walk_expr(index, sigs)?;
        }
        Expr::SliceAccess { object, start, end } => {
            walk_expr(object, sigs)?;
            if let Some(s) = start {
                walk_expr(s, sigs)?;
            }
            if let Some(en) = end {
                walk_expr(en, sigs)?;
            }
        }
        Expr::Lambda { body, .. } => walk_stmts(body, sigs)?,
        Expr::IsCheck { expr, .. } => walk_expr(expr, sigs)?,
        Expr::Await(inner) => walk_expr(inner, sigs)?,
    }
    Ok(())
}

fn walk_stmt(s: &mut Stmt, sigs: &Sigs) -> Result<(), CodegenError> {
    match s {
        Stmt::Assign { value, .. }
        | Stmt::TypedAssign { value, .. }
        | Stmt::AugAssign { value, .. }
        | Stmt::Const { value, .. } => walk_expr(value, sigs)?,
        Stmt::AssignIndex { object, index, value, .. } => {
            walk_expr(object, sigs)?;
            walk_expr(index, sigs)?;
            walk_expr(value, sigs)?;
        }
        Stmt::AssignAttr { object, value, .. } => {
            walk_expr(object, sigs)?;
            walk_expr(value, sigs)?;
        }
        Stmt::If { cond, then_body, else_body, .. } => {
            walk_expr(cond, sigs)?;
            walk_stmts(then_body, sigs)?;
            walk_stmts(else_body, sigs)?;
        }
        Stmt::While { cond, body, .. } => {
            walk_expr(cond, sigs)?;
            walk_stmts(body, sigs)?;
        }
        Stmt::For { iter, body, .. } => {
            walk_expr(iter, sigs)?;
            walk_stmts(body, sigs)?;
        }
        Stmt::Match { expr, arms, .. } => {
            walk_expr(expr, sigs)?;
            for arm in arms.iter_mut() {
                walk_expr(&mut arm.pattern, sigs)?;
                walk_stmts(&mut arm.body, sigs)?;
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                walk_expr(v, sigs)?;
            }
        }
        Stmt::With { init, body, .. } => {
            walk_expr(init, sigs)?;
            walk_stmts(body, sigs)?;
        }
        Stmt::Break { .. } | Stmt::Continue { .. } | Stmt::Use { .. } | Stmt::ExternFn { .. } => {}
        Stmt::Fn { params, body, .. } | Stmt::AsyncFn { params, body, .. } => {
            for p in params.iter_mut() {
                if let Some(d) = &mut p.default {
                    walk_expr(d, sigs)?;
                }
            }
            walk_stmts(body, sigs)?;
        }
        Stmt::Shape { fields, on_create, on_error, acts, .. } => {
            for f in fields.iter_mut() {
                if let Some(d) = &mut f.default {
                    walk_expr(d, sigs)?;
                }
            }
            if let Some((_, body)) = on_create {
                walk_stmts(body, sigs)?;
            }
            if let Some((_, body)) = on_error {
                walk_stmts(body, sigs)?;
            }
            for a in acts.iter_mut() {
                walk_stmts(&mut a.body, sigs)?;
            }
        }
        Stmt::Show { value, .. } => walk_expr(value, sigs)?,
        Stmt::ErrorStmt { msg, .. } => walk_expr(msg, sigs)?,
        Stmt::Attempt { body, handler, .. } => {
            walk_stmts(body, sigs)?;
            if let Some(h) = handler {
                walk_stmts(&mut h.body, sigs)?;
            }
        }
        Stmt::Ask { prompt, choices, .. } => {
            walk_expr(prompt, sigs)?;
            if let Some(c) = choices {
                walk_expr(c, sigs)?;
            }
        }
        Stmt::Read { path, .. } => walk_expr(path, sigs)?,
        Stmt::Write { path, content, .. } | Stmt::Append { path, content, .. } => {
            walk_expr(path, sigs)?;
            walk_expr(content, sigs)?;
        }
        Stmt::Serve { port, routes, .. } => {
            walk_expr(port, sigs)?;
            walk_stmts(routes, sigs)?;
        }
        Stmt::Think { prompt, .. } => walk_expr(prompt, sigs)?,
        Stmt::Learn { text, .. } => walk_expr(text, sigs)?,
        Stmt::Sense { query, .. } => walk_expr(query, sigs)?,
        Stmt::Spawn { call, .. } => walk_expr(call, sigs)?,
        Stmt::Await { expr, .. } => walk_expr(expr, sigs)?,
        Stmt::Expr { expr, .. } => walk_expr(expr, sigs)?,
    }
    Ok(())
}
