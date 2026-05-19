use std::collections::{HashMap, HashSet};
use crate::ast::{Expr, Stmt, Handler};

//    Resultado

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
    fn warning(msg: impl Into<String>, line: u32) -> Self {
        TypeIssue { message: msg.into(), kind: "warning", line }
    }
}

//    Firma de función                                                           

#[derive(Debug, Clone)]
struct FnSig {
    type_params: Vec<String>,               // parámetros de tipo: [T, U]
    params: Vec<(String, Option<String>)>,  // (nombre, type_hint)
    return_type: Option<String>,
}

//    Type Checker                                                               

pub struct TypeChecker {
    issues: Vec<TypeIssue>,
    fn_sigs: HashMap<String, FnSig>,
    shape_names: HashSet<String>,
    shape_type_params: HashMap<String, Vec<String>>, // shape → sus type params
    scope_stack: Vec<HashMap<String, String>>,
    current_line: u32,
    /// Variables asignadas pero nunca leídas en el scope actual
    written_not_read: Vec<HashMap<String, u32>>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            issues: Vec::new(),
            fn_sigs: HashMap::new(),
            shape_names: HashSet::new(),
            shape_type_params: HashMap::new(),
            scope_stack: vec![HashMap::new()],
            current_line: 0,
            written_not_read: vec![HashMap::new()],
        }
    }

    pub fn check(mut self, stmts: &[Stmt]) -> Vec<TypeIssue> {
        self.collect_fn_sigs(stmts);
        self.check_stmts(stmts, None);
        self.issues
    }

    //    Scope                                                                   

    fn scope_get(&mut self, name: &str) -> Option<String> {
        // Marcar como leída en written_not_read
        for usage in self.written_not_read.iter_mut().rev() {
            usage.remove(name);
        }
        for scope in self.scope_stack.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(t.clone());
            }
        }
        None
    }

    fn scope_get_no_mark(&self, name: &str) -> Option<String> {
        for scope in self.scope_stack.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(t.clone());
            }
        }
        None
    }

    fn scope_set(&mut self, name: String, ty: String) {
        if let Some(top) = self.scope_stack.last_mut() {
            top.insert(name.clone(), ty);
        }
        if let Some(top) = self.written_not_read.last_mut() {
            top.insert(name, self.current_line);
        }
    }

    fn push_scope(&mut self) {
        self.scope_stack.push(HashMap::new());
        self.written_not_read.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
        // Variables escritas y nunca leídas en este scope → warning
        if let Some(unused) = self.written_not_read.pop() {
            for (name, line) in unused {
                // Ignorar variables con _ prefix (convención de descarte)
                if !name.starts_with('_') {
                    self.issues.push(TypeIssue::warning(
                        format!("Variable '{name}' asignada pero nunca usada"),
                        line,
                    ));
                }
            }
        }
    }

    fn report(&mut self, msg: impl Into<String>, line: u32) {
        self.issues.push(TypeIssue::error(msg, line));
    }

    //    Recolección de firmas (primer pase)                                     

    fn collect_fn_sigs(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match stmt {
                Stmt::Fn { name, type_params, params, ret_type, .. } |
                Stmt::AsyncFn { name, type_params, params, ret_type, .. } => {
                    let sig = FnSig {
                        type_params: type_params.clone(),
                        params: params.iter()
                            .map(|p| (p.name.clone(), p.type_hint.clone()))
                            .collect(),
                        return_type: ret_type.clone(),
                    };
                    self.fn_sigs.insert(name.clone(), sig);
                }
                Stmt::Shape { name, type_params, .. } => {
                    self.shape_names.insert(name.clone());
                    self.shape_type_params.insert(name.clone(), type_params.clone());
                    // Registrar el shape como constructor callable
                    self.fn_sigs.entry(name.clone()).or_insert(FnSig {
                        type_params: type_params.clone(),
                        params: vec![],
                        return_type: Some(name.clone()),
                    });
                }
                _ => {}
            }
        }
    }

    //    Statements                                                             

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
                // Inferir primero (puede leer 'name' en el lado derecho, ej: x = x + 1)
                let ty = self.infer_type(value);
                if let Some(t) = ty {
                    self.scope_set(name.clone(), t);
                } else {
                    // Tipo desconocido: registrar como "any" para no bloquear
                    self.scope_set(name.clone(), "any".into());
                }
            }

            Stmt::Const { name, value, line, .. } => {
                self.current_line = *line;
                if let Some(ty) = self.infer_type(value) {
                    self.scope_set(name.clone(), ty);
                }
            }

            // definición de función: registra firma, verifica cuerpo
            Stmt::Fn { name, type_params, params, body, ret_type, line, .. } |
            Stmt::AsyncFn { name, type_params, params, body, ret_type, line, .. } => {
                self.current_line = *line;
                let sig = FnSig {
                    type_params: type_params.clone(),
                    params: params.iter()
                        .map(|p| (p.name.clone(), p.type_hint.clone()))
                        .collect(),
                    return_type: ret_type.clone(),
                };
                self.fn_sigs.insert(name.clone(), sig);
                self.push_scope();
                // Los type params se registran como tipo "any" dentro del cuerpo
                for tp in type_params {
                    self.scope_set(tp.clone(), "any".to_string());
                }
                for p in params {
                    if let Some(th) = &p.type_hint {
                        // Si el type hint es un type param (T, U...), registrar como "any"
                        let resolved = if type_params.contains(th) { "any".to_string() } else { normalize(th) };
                        self.scope_set(p.name.clone(), resolved);
                    }
                }
                self.check_stmts(body, ret_type.as_deref());
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

            Stmt::For { var, iter, body, line } => {
                self.current_line = *line;
                // Inferir el tipo del elemento a partir del iterador
                let elem_type = self.infer_iter_elem_type(iter);
                self.push_scope();
                self.scope_set(var.clone(), elem_type);
                // La variable del for-loop siempre se "usa" (no reportar como unused)
                if let Some(top) = self.written_not_read.last_mut() {
                    top.remove(var);
                }
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

            Stmt::Shape { name, type_params, fields, on_create, acts, using: _, line, .. } => {
                self.current_line = *line;
                self.shape_type_params.insert(name.clone(), type_params.clone());
                // Verificar on_create y acts con type params en scope
                let check_with_type_params = |checker: &mut TypeChecker, params: &[crate::ast::Param], body: &[Stmt]| {
                    checker.push_scope();
                    for tp in type_params { checker.scope_set(tp.clone(), "any".to_string()); }
                    for p in params {
                        if let Some(th) = &p.type_hint {
                            let resolved = if type_params.contains(th) { "any".to_string() } else { normalize(th) };
                            checker.scope_set(p.name.clone(), resolved);
                        }
                    }
                    checker.check_stmts(body, None);
                    checker.pop_scope();
                };
                if let Some((params, body)) = on_create {
                    check_with_type_params(self, params, body);
                }
                for act in acts {
                    check_with_type_params(self, &act.params, &act.body);
                }
                let _ = fields;
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
                        // Unificar type params: T → tipo concreto inferido del primer arg que lo usa
                        let bindings = self.unify_type_params(&sig, args);
                        for (idx, arg) in args.iter().enumerate() {
                            if let Some((pname, Some(declared))) = sig.params.get(idx) {
                                // Resolver el declared con los bindings de generics
                                let resolved = resolve_generic(declared, &bindings);
                                if let Some(actual) = self.infer_type(arg) {
                                    if !types_compatible(&resolved, &actual) {
                                        let line = self.current_line;
                                        self.report(
                                            format!(
                                                "Llamada a '{fn_name}': argumento #{} \
                                                 ('{pname}: {declared}') — se esperaba \
                                                 '{resolved}', se recibió '{actual}'",
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

    //    Inferencia de tipos                                                    

    /// Infiere el tipo de los elementos de un iterador (para `for x in iter`)
    fn infer_iter_elem_type(&mut self, iter: &Expr) -> String {
        match iter {
            Expr::Str(_) => "string".into(),
            Expr::List(items) => {
                // Inferir tipo del primer elemento homogéneo
                if let Some(first) = items.first() {
                    self.infer_type(first).unwrap_or("any".into())
                } else { "any".into() }
            }
            Expr::Ident(name) => {
                match self.scope_get_no_mark(name).as_deref() {
                    Some("list")   => "any".into(),
                    Some("string") => "string".into(),
                    Some(other)    => other.to_string(),
                    None           => "any".into(),
                }
            }
            // rango 1..10 → int
            Expr::BinaryOp { op, .. } if op == ".." => "int".into(),
            _ => "any".into(),
        }
    }

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

            Expr::Ident(name)  => {
                // Detectar variables no definidas (excluir builtins conocidos)
                let ty = self.scope_get(name);
                if ty.is_none() && !is_builtin(name) && !self.fn_sigs.contains_key(name) && !self.shape_names.contains(name) {
                    let line = self.current_line;
                    self.issues.push(TypeIssue::warning(
                        format!("Variable '{name}' usada pero no definida en este scope"),
                        line,
                    ));
                }
                ty
            }

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

    //    Unificación de type params                                              

    /// Dado `fn f[T, U](a: T, b: U)` y los args reales, devuelve {T→"int", U→"string"}.
    fn unify_type_params(&mut self, sig: &FnSig, args: &[Expr]) -> HashMap<String, String> {
        let mut bindings: HashMap<String, String> = HashMap::new();
        let type_param_set: HashSet<&str> = sig.type_params.iter().map(|s| s.as_str()).collect();
        for (idx, (_, declared_opt)) in sig.params.iter().enumerate() {
            if let Some(declared) = declared_opt {
                if type_param_set.contains(declared.as_str()) {
                    if let Some(arg) = args.get(idx) {
                        if let Some(actual) = self.infer_type(arg) {
                            // Solo ligar si no hay conflicto
                            let entry = bindings.entry(declared.clone()).or_insert_with(|| actual.clone());
                            if *entry != actual && actual != "any" {
                                let line = self.current_line;
                                self.report(
                                    format!("Generic '{declared}' usado como '{}' y '{}' en la misma llamada", entry, actual),
                                    line,
                                );
                            }
                        }
                    }
                }
            }
        }
        bindings
    }
}

//    Helpers de tipos                                                          

/// Builtins que siempre existen sin declaración explícita
fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "show" | "print" | "len" | "type" | "int" | "float" | "str" | "bool"
        | "list" | "dict" | "range" | "input" | "read" | "write" | "append"
        | "push" | "pop" | "keys" | "values" | "items" | "contains" | "remove"
        | "sort" | "reverse" | "map" | "filter" | "reduce" | "zip" | "enumerate"
        | "min" | "max" | "sum" | "abs" | "round" | "floor" | "ceil"
        | "split" | "join" | "trim" | "upper" | "lower" | "replace" | "starts_with"
        | "ends_with" | "find" | "slice" | "format" | "parse"
        | "spawn" | "await" | "task" | "sleep"
        | "yes" | "no" | "null" | "true" | "false"
        | "self" | "super"
    )
}

fn normalize(t: &str) -> String {
    match t {
        "str"     => "string",
        "integer" => "int",
        "boolean" => "bool",
        "num" | "number" => "number",
        other => other,
    }.to_string()
}

/// Resuelve un tipo declarado usando los bindings de type params.
/// Ej: declared="T", bindings={"T":"int"} → "int"
/// Ej: declared="List[T]", bindings={"T":"int"} → "List[int]"
fn resolve_generic(declared: &str, bindings: &HashMap<String, String>) -> String {
    if let Some(concrete) = bindings.get(declared) {
        return concrete.clone();
    }
    // Intento simple para tipos compuestos como "List[T]"
    if let Some(bracket) = declared.find('[') {
        let base = &declared[..bracket];
        let inner = &declared[bracket + 1..declared.len().saturating_sub(1)];
        let resolved_inner = resolve_generic(inner, bindings);
        return format!("{}[{}]", base, resolved_inner);
    }
    declared.to_string()
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

//    API pública                                                                

pub fn type_check(stmts: &[Stmt]) -> Vec<TypeIssue> {
    TypeChecker::new().check(stmts)
}
