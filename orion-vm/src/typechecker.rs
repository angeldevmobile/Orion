use std::collections::HashMap;
use crate::ast::{Expr, Stmt, Handler};

// ── Resultado ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TypeIssue {
    pub message: String,
    pub kind: &'static str,   // "error" | "warning"
    pub line: u32,
}

impl TypeIssue {
    fn error(msg: impl Into<String>, line: u32) -> Self {
        TypeIssue { message: msg.into(), kind: "error", line }
    }
}

// ── Firma de función ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct FnSig {
    params: Vec<(String, Option<String>)>,  // (nombre, type_hint)
    return_type: Option<String>,
}

// ── Type Checker ──────────────────────────────────────────────────────────────

pub struct TypeChecker {
    issues: Vec<TypeIssue>,
    fn_sigs: HashMap<String, FnSig>,
    scope_stack: Vec<HashMap<String, String>>,
    current_line: u32,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            issues: Vec::new(),
            fn_sigs: HashMap::new(),
            scope_stack: vec![HashMap::new()],
            current_line: 0,
        }
    }

    pub fn check(mut self, stmts: &[Stmt]) -> Vec<TypeIssue> {
        self.collect_fn_sigs(stmts);
        self.check_stmts(stmts, None);
        self.issues
    }

    // ── Scope ──────────────────────────────────────────────────────────────────

    fn scope_get(&self, name: &str) -> Option<String> {
        for scope in self.scope_stack.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(t.clone());
            }
        }
        None
    }

    fn scope_set(&mut self, name: String, ty: String) {
        if let Some(top) = self.scope_stack.last_mut() {
            top.insert(name, ty);
        }
    }

    fn push_scope(&mut self) { self.scope_stack.push(HashMap::new()); }
    fn pop_scope(&mut self)  { self.scope_stack.pop(); }

    fn report(&mut self, msg: impl Into<String>, line: u32) {
        self.issues.push(TypeIssue::error(msg, line));
    }

    // ── Recolección de firmas (primer pase) ────────────────────────────────────

    fn collect_fn_sigs(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match stmt {
                Stmt::Fn { name, params, body: _, line: _ } |
                Stmt::AsyncFn { name, params, body: _, line: _ } => {
                    let sig = FnSig {
                        params: params.iter()
                            .map(|p| (p.name.clone(), p.type_hint.clone()))
                            .collect(),
                        return_type: None,
                    };
                    self.fn_sigs.insert(name.clone(), sig);
                }
                _ => {}
            }
        }
    }

    // ── Statements ────────────────────────────────────────────────────────────

    fn check_stmts(&mut self, stmts: &[Stmt], return_type: Option<&str>) {
        for stmt in stmts {
            self.check_stmt(stmt, return_type);
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt, return_type: Option<&str>) {
        match stmt {

            // variable con type hint: nombre: tipo = valor
            Stmt::TypedAssign { name, type_hint, value, line } => {
                self.current_line = *line;
                let actual = self.infer_type(value);
                if let Some(actual_ty) = &actual {
                    if !types_compatible(type_hint, actual_ty) {
                        self.report(
                            format!("'{name}: {type_hint}' — se asignó valor de tipo '{actual_ty}'"),
                            *line,
                        );
                    }
                }
                self.scope_set(name.clone(), normalize(type_hint));
            }

            // asignación sin tipo: registra el tipo inferido en scope
            Stmt::Assign { name, value, line } => {
                self.current_line = *line;
                if let Some(ty) = self.infer_type(value) {
                    self.scope_set(name.clone(), ty);
                }
            }

            Stmt::Const { name, value, line } => {
                self.current_line = *line;
                if let Some(ty) = self.infer_type(value) {
                    self.scope_set(name.clone(), ty);
                }
            }

            // definición de función: registra firma, verifica cuerpo
            Stmt::Fn { name, params, body, line } |
            Stmt::AsyncFn { name, params, body, line } => {
                self.current_line = *line;
                let sig = FnSig {
                    params: params.iter()
                        .map(|p| (p.name.clone(), p.type_hint.clone()))
                        .collect(),
                    return_type: None,
                };
                self.fn_sigs.insert(name.clone(), sig);
                self.push_scope();
                for p in params {
                    if let Some(th) = &p.type_hint {
                        self.scope_set(p.name.clone(), normalize(th));
                    }
                }
                self.check_stmts(body, None);
                self.pop_scope();
            }

            Stmt::Return { value, line } => {
                self.current_line = *line;
                if let (Some(rt), Some(expr)) = (return_type, value) {
                    if rt != "void" && rt != "any" {
                        if let Some(actual) = self.infer_type(expr) {
                            if !types_compatible(rt, &actual) {
                                self.report(
                                    format!("RETURN: se esperaba '{rt}', pero es de tipo '{actual}'"),
                                    *line,
                                );
                            }
                        }
                    }
                }
            }

            Stmt::If { cond, then_body, else_body, line } => {
                self.current_line = *line;
                self.infer_type(cond);
                self.push_scope();
                self.check_stmts(then_body, return_type);
                self.pop_scope();
                if !else_body.is_empty() {
                    self.push_scope();
                    self.check_stmts(else_body, return_type);
                    self.pop_scope();
                }
            }

            Stmt::While { cond, body, line } => {
                self.current_line = *line;
                self.infer_type(cond);
                self.push_scope();
                self.check_stmts(body, return_type);
                self.pop_scope();
            }

            Stmt::For { var: _, iter: _, body, line } => {
                self.current_line = *line;
                self.push_scope();
                self.check_stmts(body, return_type);
                self.pop_scope();
            }

            Stmt::Attempt { body, handler, line } => {
                self.current_line = *line;
                self.push_scope();
                self.check_stmts(body, return_type);
                self.pop_scope();
                if let Some(Handler { err_name, body: hbody }) = handler {
                    self.push_scope();
                    self.scope_set(err_name.clone(), "string".to_string());
                    self.check_stmts(hbody, return_type);
                    self.pop_scope();
                }
            }

            Stmt::Show { value, line } => {
                self.current_line = *line;
                self.infer_type(value);
            }

            Stmt::Expr { expr, line } => {
                self.current_line = *line;
                self.check_call_types(expr);
            }

            Stmt::Shape { name: _, fields, on_create, acts, using: _, line } => {
                self.current_line = *line;
                if let Some((params, body)) = on_create {
                    self.push_scope();
                    for p in params {
                        if let Some(th) = &p.type_hint {
                            self.scope_set(p.name.clone(), normalize(th));
                        }
                    }
                    self.check_stmts(body, None);
                    self.pop_scope();
                }
                for act in acts {
                    self.push_scope();
                    for p in &act.params {
                        if let Some(th) = &p.type_hint {
                            self.scope_set(p.name.clone(), normalize(th));
                        }
                    }
                    self.check_stmts(&act.body, None);
                    self.pop_scope();
                }
                let _ = fields; // campos: type hints almacenados pero no verificados aún
            }

            _ => {}
        }
    }

    // Verifica los tipos de argumentos en una llamada a función
    fn check_call_types(&mut self, expr: &Expr) {
        match expr {
            Expr::Call { callee, args, kwargs: _ } => {
                if let Expr::Ident(fn_name) = callee.as_ref() {
                    let sig = self.fn_sigs.get(fn_name).cloned();
                    if let Some(sig) = sig {
                        for (idx, arg) in args.iter().enumerate() {
                            if let Some((pname, Some(declared))) = sig.params.get(idx) {
                                if let Some(actual) = self.infer_type(arg) {
                                    if !types_compatible(declared, &actual) {
                                        let line = self.current_line;
                                        self.report(
                                            format!(
                                                "Llamada a '{fn_name}': argumento #{} \
                                                 ('{pname}: {declared}') — se esperaba \
                                                 '{declared}', se recibió '{actual}'",
                                                idx + 1
                                            ),
                                            line,
                                        );
                                    }
                                }
                            }
                            self.check_call_types(arg);
                        }
                        return;
                    }
                }
                // función desconocida o expresión compleja: verificar args recursivamente
                for arg in args { self.check_call_types(arg); }
            }
            Expr::BinaryOp { op: _, left, right } => {
                self.check_call_types(left);
                self.check_call_types(right);
            }
            Expr::UnaryOp { op: _, expr } => self.check_call_types(expr),
            Expr::List(items) => { for e in items { self.check_call_types(e); } }
            Expr::Dict(pairs) => { for (_, v) in pairs { self.check_call_types(v); } }
            Expr::Index { object, index } => {
                self.check_call_types(object);
                self.check_call_types(index);
            }
            Expr::AttrAccess { object, attr: _ } => self.check_call_types(object),
            _ => {}
        }
    }

    // ── Inferencia de tipos ───────────────────────────────────────────────────

    fn infer_type(&mut self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Int(_)       => Some("int".into()),
            Expr::Float(_)     => Some("float".into()),
            Expr::Str(_)       => Some("string".into()),
            Expr::Bool(_)      => Some("bool".into()),
            Expr::Null         => Some("any".into()),
            Expr::List(_)      => Some("list".into()),
            Expr::Dict(_)      => Some("dict".into()),
            Expr::Lambda { .. } => Some("fn".into()),

            Expr::Ident(name)  => self.scope_get(name),

            Expr::BinaryOp { op, left, right } => {
                let lt = self.infer_type(left);
                let rt = self.infer_type(right);
                match op.as_str() {
                    "+" | "-" | "*" | "/" | "%" | "**" => {
                        match (lt.as_deref(), rt.as_deref()) {
                            (Some("float"), _) | (_, Some("float")) => Some("float".into()),
                            (Some("int"), Some("int"))              => Some("int".into()),
                            (Some("string"), _) if op == "+"        => Some("string".into()),
                            _ => None,
                        }
                    }
                    "<" | ">" | "<=" | ">=" | "==" | "!=" | "and" | "or" => Some("bool".into()),
                    _ => None,
                }
            }

            Expr::UnaryOp { op, expr } => {
                if op == "not" { Some("bool".into()) } else { self.infer_type(expr) }
            }

            Expr::Call { callee, args: _, kwargs: _ } => {
                if let Expr::Ident(fn_name) = callee.as_ref() {
                    self.fn_sigs.get(fn_name).and_then(|s| s.return_type.clone())
                } else {
                    None
                }
            }

            _ => None,
        }
    }
}

// ── Helpers de tipos ─────────────────────────────────────────────────────────

fn normalize(t: &str) -> String {
    match t {
        "str"     => "string",
        "integer" => "int",
        "boolean" => "bool",
        "num" | "number" => "number",
        other => other,
    }.to_string()
}

fn types_compatible(declared: &str, actual: &str) -> bool {
    let d = normalize(declared);
    let a = normalize(actual);
    if d == "any" || d == "void" { return true; }
    if a == "any"                { return true; }
    if d == a                    { return true; }
    if d == "number" && (a == "int" || a == "float" || a == "number") { return true; }
    if d == "float"  && a == "int" { return true; }
    false
}

// ── API pública ───────────────────────────────────────────────────────────────

pub fn type_check(stmts: &[Stmt]) -> Vec<TypeIssue> {
    TypeChecker::new().check(stmts)
}
