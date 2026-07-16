use std::collections::{HashMap, HashSet};
use crate::ast::{Expr, Stmt, Handler};

//    Resultado

#[derive(Debug, Clone)]
pub struct TypeIssue {
    pub message: String,
    pub kind: &'static str,   // "error" | "warning"
    pub line: u32,
    pub col:  u32,
}

impl TypeIssue {
    fn error(msg: impl Into<String>, line: u32, col: u32) -> Self {
        TypeIssue { message: msg.into(), kind: "error", line, col }
    }
    fn warning(msg: impl Into<String>, line: u32, col: u32) -> Self {
        TypeIssue { message: msg.into(), kind: "warning", line, col }
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
    shape_fields: HashMap<String, Vec<(String, Option<String>)>>, // shape → (campo, tipo)
    shape_using:  HashMap<String, Vec<String>>,       // shape → shapes heredados
    scope_stack: Vec<HashMap<String, String>>,
    current_line: u32,
    current_col:  u32,
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
            shape_fields: HashMap::new(),
            shape_using:  HashMap::new(),
            scope_stack: vec![HashMap::new()],
            current_line: 0,
            current_col:  0,
            written_not_read: vec![HashMap::new()],
        }
    }

    pub fn check(mut self, stmts: &[Stmt]) -> Vec<TypeIssue> {
        self.collect_fn_sigs(stmts);
        self.infer_untyped_fn_returns(stmts);
        self.check_stmts(stmts, None);
        self.issues
    }

    //    Inferencia de retorno para funciones SIN anotación

    /// Para cada `fn`/`async fn` sin `-> tipo`, infiere su retorno a partir del
    /// cuerpo, para que la inferencia se propague a través de las funciones del
    /// usuario (hoy una llamada a una fn sin anotar valía `any`).
    ///
    /// CONSERVADOR a propósito: solo fija un tipo cuando TODOS los `return`
    /// coinciden en un mismo tipo concreto. Si hay returns mixtos, vacíos, o
    /// alguno depende de un valor desconocido (p. ej. un parámetro sin tipo),
    /// se deja en `any`. Así nunca infiere un tipo equivocado que dispare un
    /// falso error en el call site (que ahora aborta la ejecución).
    ///
    /// Itera hasta punto fijo (acotado) para resolver cadenas A→B→C.
    fn infer_untyped_fn_returns(&mut self, stmts: &[Stmt]) {
        for _ in 0..6 {
            let mut changed = false;
            for stmt in stmts {
                let (name, body) = match stmt {
                    Stmt::Fn { name, body, ret_type: None, .. } |
                    Stmt::AsyncFn { name, body, ret_type: None, .. } => (name, body),
                    _ => continue,
                };
                // ¿ya inferido en una pasada previa?
                if self.fn_sigs.get(name).and_then(|s| s.return_type.clone()).is_some() {
                    continue;
                }
                let rts = self.collect_return_types(body);
                if rts.is_empty() || rts.iter().any(|t| t.is_none()) {
                    continue;
                }
                let first = rts[0].clone().unwrap();
                if rts.iter().all(|t| t.as_deref() == Some(first.as_str())) {
                    if let Some(sig) = self.fn_sigs.get_mut(name) {
                        sig.return_type = Some(first);
                        changed = true;
                    }
                }
            }
            if !changed { break; }
        }
    }

    /// Tipos de todas las expresiones `return` del cuerpo (sin entrar en `fn`
    /// anidadas). `None` = retorno sin valor o de tipo no determinable.
    fn collect_return_types(&self, body: &[Stmt]) -> Vec<Option<String>> {
        let mut out = Vec::new();
        for s in body {
            match s {
                Stmt::Return { value, .. } => {
                    out.push(value.as_ref().and_then(|e| self.infer_pure(e)));
                }
                Stmt::If { then_body, else_body, .. } => {
                    out.extend(self.collect_return_types(then_body));
                    out.extend(self.collect_return_types(else_body));
                }
                Stmt::While { body, .. } | Stmt::For { body, .. } => {
                    out.extend(self.collect_return_types(body));
                }
                Stmt::Attempt { body, handler, .. } => {
                    out.extend(self.collect_return_types(body));
                    if let Some(h) = handler { out.extend(self.collect_return_types(&h.body)); }
                }
                Stmt::Match { arms, .. } => {
                    for a in arms { out.extend(self.collect_return_types(&a.body)); }
                }
                _ => {} // no recursar en Stmt::Fn anidadas: sus returns son suyos
            }
        }
        out
    }

    /// Inferencia pura (sin scope, sin emitir issues) para el pase de retornos.
    /// Devuelve `None` para todo lo que dependa de variables/params (Ident, etc.),
    /// manteniendo el pase conservador.
    fn infer_pure(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Int(_)        => Some("int".into()),
            Expr::Float(_)      => Some("float".into()),
            Expr::Str(_)        => Some("string".into()),
            Expr::Bool(_)       => Some("bool".into()),
            Expr::List(_)       => Some("list".into()),
            Expr::Dict(_)       => Some("dict".into()),
            Expr::Lambda { .. } => Some("fn".into()),
            Expr::BinaryOp { op, left, right } => {
                let lt = self.infer_pure(left);
                let rt = self.infer_pure(right);
                match op.as_str() {
                    "+" | "-" | "*" | "/" | "%" | "**" => match (lt.as_deref(), rt.as_deref()) {
                        (Some("float"), _) | (_, Some("float")) => Some("float".into()),
                        (Some("int"), Some("int"))              => Some("int".into()),
                        (Some("string"), Some("string")) if op == "+" => Some("string".into()),
                        _ => None,
                    },
                    "<" | ">" | "<=" | ">=" | "==" | "!=" | "and" | "or" => Some("bool".into()),
                    _ => None,
                }
            }
            Expr::UnaryOp { op, expr } => {
                if op == "not" { Some("bool".into()) } else { self.infer_pure(expr) }
            }
            Expr::Call { callee, .. } => match callee.as_ref() {
                Expr::Ident(f) => self.fn_sigs.get(f).and_then(|s| s.return_type.clone()),
                _ => None,
            },
            _ => None,
        }
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
        let n = self.scope_stack.len();
        // Si la variable ya existe en un scope externo, actualizar allí
        let outer_idx = if n > 1 {
            self.scope_stack[..n - 1].iter().rposition(|s| s.contains_key(&name))
        } else {
            None
        };

        if let Some(idx) = outer_idx {
            self.scope_stack[idx].insert(name.clone(), ty);
            if let Some(tracking) = self.written_not_read.get_mut(idx) {
                tracking.insert(name, self.current_line);
            }
        } else {
            if let Some(top) = self.scope_stack.last_mut() {
                top.insert(name.clone(), ty);
            }
            if let Some(top) = self.written_not_read.last_mut() {
                top.insert(name, self.current_line);
            }
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
                        line, 1,
                    ));
                }
            }
        }
    }

    fn report(&mut self, msg: impl Into<String>, line: u32, col: u32) {
        self.issues.push(TypeIssue::error(msg, line, col));
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
                Stmt::Shape { name, type_params, fields, using, .. } => {
                    self.shape_names.insert(name.clone());
                    self.shape_type_params.insert(name.clone(), type_params.clone());
                    self.shape_fields.insert(
                        name.clone(),
                        fields.iter().map(|f| (f.name.clone(), f.type_hint.clone())).collect(),
                    );
                    self.shape_using.insert(name.clone(), using.clone());
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

    /// Campos visibles dentro de un shape: los propios más los heredados vía
    /// `using` (transitivamente). Evita ciclos con un set de visitados.
    fn collect_all_fields(&self, shape: &str) -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![shape.to_string()];
        while let Some(s) = stack.pop() {
            if !seen.insert(s.clone()) { continue; }
            if let Some(fs) = self.shape_fields.get(&s) {
                out.extend(fs.iter().cloned());
            }
            if let Some(parents) = self.shape_using.get(&s) {
                for p in parents { stack.push(p.clone()); }
            }
        }
        out
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
            Stmt::TypedAssign { name, type_hint, value, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.check_call_types(value);
                let actual = self.infer_type(value);
                if let Some(actual_ty) = &actual {
                    if !types_compatible(type_hint, actual_ty) {
                        self.report(
                            format!("'{name}: {type_hint}' — se asignó valor de tipo '{actual_ty}'"),
                            *line, *col,
                        );
                    }
                }
                self.scope_set(name.clone(), normalize(type_hint));
            }

            // asignación sin tipo: registra el tipo inferido en scope
            Stmt::Assign { name, value, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.check_call_types(value);
                let ty = self.infer_type(value);
                if let Some(t) = ty {
                    self.scope_set(name.clone(), t);
                } else {
                    self.scope_set(name.clone(), "any".into());
                }
            }

            Stmt::Const { name, value, line, col, .. } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.check_call_types(value);
                if let Some(ty) = self.infer_type(value) {
                    self.scope_set(name.clone(), ty);
                }
            }

            // definición de función: registra firma, verifica cuerpo
            Stmt::Fn { name, type_params, params, body, ret_type, line, col, .. } |
            Stmt::AsyncFn { name, type_params, params, body, ret_type, line, col, .. } => {
                self.current_line = *line;
                self.current_col  = *col;
                // Preserva el retorno inferido por `infer_untyped_fn_returns`
                // cuando la fn no lo declara (si no, lo borraríamos a None).
                let return_type = ret_type.clone()
                    .or_else(|| self.fn_sigs.get(name).and_then(|s| s.return_type.clone()));
                let sig = FnSig {
                    type_params: type_params.clone(),
                    params: params.iter()
                        .map(|p| (p.name.clone(), p.type_hint.clone()))
                        .collect(),
                    return_type,
                };
                self.fn_sigs.insert(name.clone(), sig);
                self.push_scope();
                for tp in type_params {
                    self.scope_set(tp.clone(), "any".to_string());
                }
                for p in params {
                    let resolved = match &p.type_hint {
                        Some(th) if type_params.contains(th) => "any".to_string(),
                        Some(th) => normalize(th),
                        None => "any".to_string(),
                    };
                    self.scope_set(p.name.clone(), resolved);
                    // Los parámetros no leídos no son "asignados pero nunca usados"
                    if let Some(top) = self.written_not_read.last_mut() {
                        top.remove(&p.name);
                    }
                }
                self.check_stmts(body, ret_type.as_deref());
                self.pop_scope();
            }

            Stmt::Return { value, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                // Recorrer SIEMPRE el valor (aunque la función no declare tipo de
                // retorno) para marcar como leídas las variables usadas en él; sin
                // esto, `return x` no contaba como uso → falso "x nunca usada".
                if let Some(expr) = value {
                    self.check_call_types(expr);
                    let actual = self.infer_type(expr);
                    if let Some(rt) = return_type {
                        if rt != "void" && rt != "any" {
                            if let Some(actual) = actual {
                                if !types_compatible(rt, &actual) {
                                    self.report(
                                        format!("RETURN: se esperaba '{rt}', pero es de tipo '{actual}'"),
                                        *line, *col,
                                    );
                                }
                            }
                        }
                    }
                }
            }

            Stmt::If { cond, then_body, else_body, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
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

            Stmt::While { cond, body, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.infer_type(cond);
                self.push_scope();
                self.check_stmts(body, return_type);
                self.pop_scope();
            }

            Stmt::For { var, iter, body, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                let elem_type = self.infer_iter_elem_type(iter);
                self.push_scope();
                self.scope_set(var.clone(), elem_type);
                if let Some(top) = self.written_not_read.last_mut() {
                    top.remove(var);
                }
                self.check_stmts(body, return_type);
                self.pop_scope();
            }

            Stmt::Attempt { body, handler, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.push_scope();
                self.check_stmts(body, return_type);
                self.pop_scope();
                if let Some(Handler { err_name, body: hbody }) = handler {
                    self.push_scope();
                    self.scope_set(err_name.clone(), "string".to_string());
                    // El binding de error es implícito; `handle err { }` sin
                    // inspeccionar el error es idiomático → no avisar "nunca usado".
                    if let Some(top) = self.written_not_read.last_mut() {
                        top.remove(err_name);
                    }
                    self.check_stmts(hbody, return_type);
                    self.pop_scope();
                }
            }

            // with h = modulo.abrir(...) { } — h vive en el scope del bloque
            // con el tipo inferido del init (los handles de módulo suelen ser
            // string o int; si no se sabe, any — nunca un falso positivo).
            Stmt::With { var, init, body, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.check_call_types(init);
                let ty = self.infer_type(init).unwrap_or_else(|| "any".into());
                self.push_scope();
                self.scope_set(var.clone(), ty);
                // El handle puede usarse solo como recurso implícito (el free
                // del desugar lo lee); no avisar "asignado pero nunca leído".
                if let Some(top) = self.written_not_read.last_mut() {
                    top.remove(var);
                }
                self.check_stmts(body, return_type);
                self.pop_scope();
            }

            Stmt::AssignIndex { object, index, value, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.infer_type(object);
                self.infer_type(index);
                self.infer_type(value);
            }

            Stmt::AssignAttr { object, value, line, col, .. } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.infer_type(object);
                self.infer_type(value);
            }

            Stmt::AugAssign { name, op: _, value, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                // Leer la variable antes de actualizar
                self.scope_get(name);
                self.infer_type(value);
                let ty = self.scope_get_no_mark(name).unwrap_or("any".into());
                self.scope_set(name.clone(), ty);
            }

            // Estas sentencias LIGAN una variable nueva; hay que registrarla para
            // que su uso posterior no se reporte como "no definida".
            Stmt::Read { path, var, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.infer_type(path);
                self.scope_set(var.clone(), "string".to_string());
            }
            Stmt::Ask { prompt, var, cast, choices, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.infer_type(prompt);
                if let Some(c) = choices { self.infer_type(c); }
                let ty = match cast.as_deref() {
                    Some("int") => "int", Some("float") => "float",
                    Some("bool") => "bool", _ => "string",
                };
                self.scope_set(var.clone(), ty.to_string());
            }
            Stmt::Await { expr, var, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.infer_type(expr);
                if let Some(v) = var {
                    self.scope_set(v.clone(), "any".to_string());
                }
            }

            Stmt::Show { value, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.check_call_types(value);
                self.infer_type(value);
            }

            Stmt::Expr { expr, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.check_call_types(expr);
            }

            // import de módulo: registra el namespace (y los nombres selectivos)
            // en scope para que `math.sqrt(...)` no se reporte como "no definido".
            // Imita la resolución del runtime (codegen.rs): alias, o el nombre del
            // archivo sin extensión del path.
            Stmt::Use { path, alias, selective, line, col } => {
                self.current_line = *line;
                self.current_col  = *col;
                let ns = alias.clone().unwrap_or_else(|| {
                    std::path::Path::new(path.as_str())
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(path.as_str())
                        .to_string()
                });
                self.scope_set(ns.clone(), "module".to_string());
                // El namespace importado no es una "variable asignada y no usada".
                if let Some(top) = self.written_not_read.last_mut() {
                    top.remove(&ns);
                }
                // `use "x" take [a, b]` trae a/b como nombres sueltos invocables.
                if let Some(names) = selective {
                    for n in names {
                        self.scope_set(n.clone(), "module".to_string());
                        if let Some(top) = self.written_not_read.last_mut() {
                            top.remove(n);
                        }
                    }
                }
            }

            Stmt::Shape { name, type_params, on_create, acts, line, col, .. } => {
                self.current_line = *line;
                self.current_col  = *col;
                self.shape_type_params.insert(name.clone(), type_params.clone());
                // Verificar on_create y acts con type params + campos en scope.
                // Dentro de un shape los campos se acceden sin `self.` (`top`,
                // `count`...), así que hay que registrarlos para no reportarlos
                // como "no definidos". Incluye los campos heredados vía `using`.
                let all_fields = self.collect_all_fields(name);
                let check_with_type_params = |checker: &mut TypeChecker, params: &[crate::ast::Param], body: &[Stmt]| {
                    checker.push_scope();
                    for tp in type_params { checker.scope_set(tp.clone(), "any".to_string()); }
                    // Campos del shape (propios + heredados): visibles como nombres sueltos.
                    for (fname, fhint) in &all_fields {
                        let fty = match fhint {
                            Some(th) if type_params.contains(th) => "any".to_string(),
                            Some(th) => normalize(th),
                            None => "any".to_string(),
                        };
                        checker.scope_set(fname.clone(), fty);
                    }
                    // `self` siempre disponible dentro del cuerpo.
                    checker.scope_set("self".to_string(), name.clone());
                    for p in params {
                        // Igual que en `Fn`: los params sin anotación valen "any"
                        // (antes solo se registraban los tipados → falsos positivos
                        // sobre params sin tipo como `on_create(o, initial)`).
                        let resolved = match &p.type_hint {
                            Some(th) if type_params.contains(th) => "any".to_string(),
                            Some(th) => normalize(th),
                            None => "any".to_string(),
                        };
                        checker.scope_set(p.name.clone(), resolved);
                    }
                    // Campos/self/params no leídos no son "asignados y nunca usados".
                    if let Some(top) = checker.written_not_read.last_mut() {
                        for (fname, _) in &all_fields { top.remove(fname); }
                        for p in params { top.remove(&p.name); }
                        top.remove("self");
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
            }

            _ => {}
        }
    }

    // Verifica los tipos de argumentos en una llamada a función
    fn check_call_types(&mut self, expr: &Expr) {
        match expr {
            Expr::Call { callee, args, .. } => {
                if let Expr::Ident(fn_name) = callee.as_ref() {
                    let sig = self.fn_sigs.get(fn_name).cloned();
                    if let Some(sig) = sig {
                        // Unificar type params: T → tipo concreto inferido del primer arg que lo usa
                        let bindings = self.unify_type_params(&sig, args);
                        for (idx, arg) in args.iter().enumerate() {
                            if let Some((pname, Some(declared))) = sig.params.get(idx) {
                                let resolved = resolve_generic(declared, &bindings);
                                if let Some(actual) = self.infer_type(arg) {
                                    if !types_compatible(&resolved, &actual) {
                                        let line = self.current_line;
                                        let col  = self.current_col;
                                        self.report(
                                            format!(
                                                "Llamada a '{fn_name}': argumento #{} \
                                                 ('{pname}: {declared}') — se esperaba \
                                                 '{resolved}', se recibió '{actual}'",
                                                idx + 1
                                            ),
                                            line, col,
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
            Expr::CallMethod { receiver, args, kwargs, .. } => {
                self.check_call_types(receiver);
                for arg in args { self.check_call_types(arg); }
                for (_, v) in kwargs { self.check_call_types(v); }
            }
            Expr::Ident(name) => { self.scope_get(name); }
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
                let ty = self.scope_get(name);
                if ty.is_none() && !is_builtin(name) && !self.fn_sigs.contains_key(name) && !self.shape_names.contains(name) && !crate::modules::is_known_module(name) {
                    let line = self.current_line;
                    let col  = self.current_col;
                    self.issues.push(TypeIssue::warning(
                        format!("Variable '{name}' usada pero no definida en este scope"),
                        line, col,
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

            Expr::Call { callee, args, .. } => {
                for arg in args { self.infer_type(arg); }
                match callee.as_ref() {
                    // El nombre en posición de llamada es una FUNCIÓN/builtin, no
                    // una variable: lo marcamos leído (por si es un closure en una
                    // variable) pero NO emitimos "usada pero no definida" —eso
                    // evita falsos positivos sobre builtins (has_key, len, …) sin
                    // tener que mantener una lista hardcodeada completa de ellos.
                    Expr::Ident(fn_name) => {
                        self.scope_get(fn_name);
                        self.fn_sigs.get(fn_name).and_then(|s| s.return_type.clone())
                    }
                    other => { self.infer_type(other); None }
                }
            }

            // Llamada a método `recv.metodo(args)`: hay que visitar receptor y
            // argumentos para marcarlos como leídos (sin esto, una variable usada
            // solo en `x.push(v)` se reportaba como "nunca usada").
            Expr::CallMethod { receiver, args, kwargs, .. } => {
                self.infer_type(receiver);
                for arg in args { self.infer_type(arg); }
                for (_, v) in kwargs { self.infer_type(v); }
                Some("any".into())
            }

            Expr::AttrAccess { object, attr: _ } => {
                self.infer_type(object);
                Some("any".into())
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
                            let entry = bindings.entry(declared.clone()).or_insert_with(|| actual.clone());
                            if *entry != actual && actual != "any" {
                                let line = self.current_line;
                                let col  = self.current_col;
                                self.report(
                                    format!("Generic '{declared}' usado como '{}' y '{}' en la misma llamada", entry, actual),
                                    line, col,
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

/// Builtins que siempre existen sin declaración explícita.
///
/// Solo se consulta en posición de VALOR (pasar un builtin como dato, p. ej.
/// `map(double, xs)`). En posición de LLAMADA (`has_key(d, k)`) ya no se valida
/// el nombre contra esta lista —ver `infer_type`/`Expr::Call`—, así que no hace
/// falta que esté 100% completa para evitar falsos "no definido".
///
/// La fuente de verdad real es `VM::call_builtin` (vm.rs). El test
/// `builtins_in_sync_with_runtime` verifica que estos nombres sigan siendo
/// builtins reales del runtime.
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
        // Colecciones / acceso seguro
        | "has_key" | "get" | "first" | "last" | "is_empty" | "repeat"
        // Conversión / parseo
        | "parse_int" | "parse_float" | "to_int" | "to_float"
        // Numéricos / math
        | "sqrt" | "pow" | "sign" | "clamp" | "factorial" | "hypot"
        | "sin" | "cos" | "tan" | "exp" | "log" | "log2" | "log10"
        | "degrees" | "radians" | "rand" | "randint"
        // Strings
        | "lines" | "index_of" | "trim_start" | "trim_end"
        // Aserciones
        | "assert" | "assert_eq" | "assert_ne"
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
