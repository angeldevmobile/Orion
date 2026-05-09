use std::fs;
use crate::{lexer, parser};
use crate::ast::{Expr, Stmt, Param, MatchArm, FieldDef, ActDef};
use super::banner;

const INDENT: &str = "    ";

//   Punto de entrada                    

pub fn run_format(path: &str, write_back: bool) {
    let src = match fs::read_to_string(path) {
        Ok(s) => s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string(),
        Err(e) => {
            banner::fail(&format!("No se puede leer '{path}': {e}"));
            std::process::exit(1);
        }
    };

    let tokens = match lexer::lex(&src) {
        Ok(t) => t,
        Err(e) => {
            banner::fail(&format!("Error léxico en '{path}' (línea {}): {}", e.line, e.message));
            std::process::exit(1);
        }
    };

    let stmts = match parser::parse(tokens) {
        Ok(s) => s,
        Err(e) => {
            banner::fail(&format!("Error de sintaxis en '{path}' (línea {}): {}", e.line, e.message));
            std::process::exit(1);
        }
    };

    let formatted = format_program(&stmts);

    if write_back {
        match fs::write(path, &formatted) {
            Ok(_) => banner::ok(&format!("Formateado: {path}")),
            Err(e) => {
                banner::fail(&format!("No se puede escribir '{path}': {e}"));
                std::process::exit(1);
            }
        }
    } else {
        print!("{formatted}");
    }
}

pub fn format_source(src: &str) -> Result<String, String> {
    let tokens = lexer::lex(src)
        .map_err(|e| format!("línea {}: {}", e.line, e.message))?;
    let stmts = parser::parse(tokens)
        .map_err(|e| format!("línea {}: {}", e.line, e.message))?;
    Ok(format_program(&stmts))
}

//   Núcleo del formatter                  ──

pub fn format_program(stmts: &[Stmt]) -> String {
    let mut f = Formatter::new();
    f.write_top_level(stmts);
    f.finish()
}

struct Formatter {
    out: String,
    indent: usize,
}

impl Formatter {
    fn new() -> Self {
        Formatter { out: String::new(), indent: 0 }
    }

    fn finish(mut self) -> String {
        while self.out.ends_with("\n\n\n") {
            self.out.pop();
        }
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.out
    }

    fn push(&mut self, s: &str) { self.out.push_str(s); }
    fn nl(&mut self) { self.out.push('\n'); }

    fn ind(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str(INDENT);
        }
    }

    fn line(&mut self, s: &str) {
        self.ind();
        self.push(s);
        self.nl();
    }

    //   Nivel superior: agrupa `use` primero, blank lines entre declaraciones ──

    fn write_top_level(&mut self, stmts: &[Stmt]) {
        let mut uses: Vec<&Stmt> = Vec::new();
        let mut rest: Vec<&Stmt> = Vec::new();
        for s in stmts {
            if matches!(s, Stmt::Use { .. }) { uses.push(s); } else { rest.push(s); }
        }

        for s in &uses { self.write_stmt(s); }
        if !uses.is_empty() && !rest.is_empty() { self.nl(); }

        let mut prev_block = false;
        for (i, s) in rest.iter().enumerate() {
            let is_block = is_block(s);
            if i > 0 && (is_block || prev_block) { self.nl(); }
            self.write_stmt(s);
            prev_block = is_block;
        }
    }

    //   Cuerpo de función/bloque                

    fn write_body(&mut self, stmts: &[Stmt]) {
        let mut prev_block = false;
        for (i, s) in stmts.iter().enumerate() {
            let is_bl = is_block(s);
            if i > 0 && (is_bl || prev_block) { self.nl(); }
            self.write_stmt(s);
            prev_block = is_bl;
        }
    }

    //   Declaraciones                   ──

    fn write_stmt(&mut self, stmt: &Stmt) {
        match stmt {

            Stmt::Assign { name, value, .. } => {
                self.ind(); self.push(name); self.push(" = ");
                self.push(&fmt_expr(value)); self.nl();
            }

            Stmt::TypedAssign { name, type_hint, value, .. } => {
                self.ind(); self.push(name); self.push(": "); self.push(type_hint);
                self.push(" = "); self.push(&fmt_expr(value)); self.nl();
            }

            Stmt::AssignIndex { object, index, value, .. } => {
                self.ind();
                self.push(&fmt_expr(object)); self.push("["); self.push(&fmt_expr(index));
                self.push("] = "); self.push(&fmt_expr(value)); self.nl();
            }

            Stmt::AssignAttr { object, attr, value, .. } => {
                self.ind();
                self.push(&fmt_expr(object)); self.push("."); self.push(attr);
                self.push(" = "); self.push(&fmt_expr(value)); self.nl();
            }

            Stmt::AugAssign { name, op, value, .. } => {
                self.ind(); self.push(name); self.push(" "); self.push(op);
                self.push("= "); self.push(&fmt_expr(value)); self.nl();
            }

            Stmt::Const { name, value, doc, .. } => {
                self.write_doc(doc);
                self.ind(); self.push("const "); self.push(name);
                self.push(" = "); self.push(&fmt_expr(value)); self.nl();
            }

            Stmt::Use { path, alias, selective, .. } => {
                self.ind(); self.push("use \""); self.push(path); self.push("\"");
                if let Some(sel) = selective {
                    self.push(" take ["); self.push(&sel.join(", ")); self.push("]");
                } else if let Some(a) = alias {
                    self.push(" as "); self.push(a);
                }
                self.nl();
            }

            Stmt::Show { value, .. } => {
                self.ind(); self.push("show "); self.push(&fmt_expr(value)); self.nl();
            }

            Stmt::Return { value, .. } => {
                self.ind();
                match value {
                    Some(v) => { self.push("return "); self.push(&fmt_expr(v)); }
                    None    => self.push("return"),
                }
                self.nl();
            }

            Stmt::Break    { .. } => self.line("break"),
            Stmt::Continue { .. } => self.line("continue"),

            Stmt::If { cond, then_body, else_body, .. } => {
                self.ind(); self.push("if "); self.push(&fmt_expr(cond)); self.push(" {"); self.nl();
                self.indent += 1; self.write_body(then_body); self.indent -= 1;
                self.write_else_chain(else_body);
            }

            Stmt::While { cond, body, .. } => {
                self.ind(); self.push("while "); self.push(&fmt_expr(cond)); self.push(" {"); self.nl();
                self.indent += 1; self.write_body(body); self.indent -= 1;
                self.line("}");
            }

            Stmt::For { var, iter, body, .. } => {
                self.ind(); self.push("for "); self.push(var);
                self.push(" in "); self.push(&fmt_expr(iter)); self.push(" {"); self.nl();
                self.indent += 1; self.write_body(body); self.indent -= 1;
                self.line("}");
            }

            Stmt::Match { expr, arms, .. } => {
                self.ind(); self.push("match "); self.push(&fmt_expr(expr)); self.push(" {"); self.nl();
                self.indent += 1;
                for arm in arms { self.write_match_arm(arm); }
                self.indent -= 1;
                self.line("}");
            }

            Stmt::Fn { name, type_params, params, body, ret_type, doc, .. } => {
                self.write_doc(doc);
                self.ind(); self.push("fn "); self.push(name);
                self.write_type_params(type_params);
                self.push("("); self.push(&fmt_params(params)); self.push(")");
                self.write_ret_type(ret_type);
                self.push(" {"); self.nl();
                self.indent += 1; self.write_body(body); self.indent -= 1;
                self.line("}");
            }

            Stmt::AsyncFn { name, type_params, params, body, ret_type, doc, .. } => {
                self.write_doc(doc);
                self.ind(); self.push("async fn "); self.push(name);
                self.write_type_params(type_params);
                self.push("("); self.push(&fmt_params(params)); self.push(")");
                self.write_ret_type(ret_type);
                self.push(" {"); self.nl();
                self.indent += 1; self.write_body(body); self.indent -= 1;
                self.line("}");
            }

            Stmt::Shape { name, type_params, fields, on_create, acts, using, doc, .. } => {
                self.write_doc(doc);
                self.ind(); self.push("shape "); self.push(name);
                self.write_type_params(type_params);
                if !using.is_empty() {
                    self.push(" using "); self.push(&using.join(", "));
                }
                self.push(" {"); self.nl();
                self.indent += 1;
                self.write_shape_body(fields, on_create, acts);
                self.indent -= 1;
                self.line("}");
            }

            Stmt::ExternFn { name, params, ret_type, lib, .. } => {
                self.ind(); self.push("extern fn "); self.push(name);
                self.push("("); self.push(&fmt_params(params)); self.push(")");
                self.write_ret_type(ret_type);
                if !lib.is_empty() {
                    self.push(" from \""); self.push(lib); self.push("\"");
                }
                self.nl();
            }

            Stmt::Attempt { body, handler, .. } => {
                self.line("attempt {");
                self.indent += 1; self.write_body(body); self.indent -= 1;
                if let Some(h) = handler {
                    self.ind(); self.push("} handle "); self.push(&h.err_name); self.push(" {"); self.nl();
                    self.indent += 1; self.write_body(&h.body); self.indent -= 1;
                }
                self.line("}");
            }

            Stmt::ErrorStmt { msg, .. } => {
                self.ind(); self.push("error "); self.push(&fmt_expr(msg)); self.nl();
            }

            Stmt::Ask { prompt, var, cast, choices, .. } => {
                self.ind(); self.push("ask "); self.push(&fmt_expr(prompt));
                if let Some(c) = cast { self.push(" as "); self.push(c); }
                if let Some(ch) = choices { self.push(" choices "); self.push(&fmt_expr(ch)); }
                self.push(" -> "); self.push(var); self.nl();
            }

            Stmt::Read { path, var, .. } => {
                self.ind(); self.push("read "); self.push(&fmt_expr(path));
                self.push(" -> "); self.push(var); self.nl();
            }

            Stmt::Write { path, content, .. } => {
                self.ind(); self.push("write "); self.push(&fmt_expr(path));
                self.push(" with "); self.push(&fmt_expr(content)); self.nl();
            }

            Stmt::Append { path, content, .. } => {
                self.ind(); self.push("append "); self.push(&fmt_expr(path));
                self.push(" with "); self.push(&fmt_expr(content)); self.nl();
            }

            Stmt::Serve { port, routes, .. } => {
                self.ind(); self.push("serve "); self.push(&fmt_expr(port)); self.push(" {"); self.nl();
                self.indent += 1;
                for r in routes { self.write_stmt(r); }
                self.indent -= 1;
                self.line("}");
            }

            Stmt::Route { method, path, body, .. } => {
                self.ind(); self.push("route \""); self.push(method);
                self.push(" "); self.push(path); self.push("\" {"); self.nl();
                self.indent += 1; self.write_body(body); self.indent -= 1;
                self.line("}");
            }

            Stmt::Think { prompt, .. } => {
                self.ind(); self.push("think "); self.push(&fmt_expr(prompt)); self.nl();
            }

            Stmt::Learn { text, .. } => {
                self.ind(); self.push("learn "); self.push(&fmt_expr(text)); self.nl();
            }

            Stmt::Sense { query, .. } => {
                self.ind(); self.push("sense "); self.push(&fmt_expr(query)); self.nl();
            }

            Stmt::Spawn { call, .. } => {
                self.ind(); self.push("spawn "); self.push(&fmt_expr(call)); self.nl();
            }

            Stmt::Await { expr, var, .. } => {
                self.ind(); self.push("await "); self.push(&fmt_expr(expr));
                if let Some(v) = var { self.push(" -> "); self.push(v); }
                self.nl();
            }

            Stmt::Expr { expr, .. } => {
                self.ind(); self.push(&fmt_expr(expr)); self.nl();
            }
        }
    }

    //   Helpers de bloque                  ─

    fn write_else_chain(&mut self, else_body: &[Stmt]) {
        if else_body.is_empty() {
            self.line("}");
            return;
        }
        if else_body.len() == 1 {
            if let Stmt::If { cond, then_body, else_body: nested, .. } = &else_body[0] {
                self.ind(); self.push("} else if "); self.push(&fmt_expr(cond)); self.push(" {"); self.nl();
                self.indent += 1; self.write_body(then_body); self.indent -= 1;
                self.write_else_chain(nested);
                return;
            }
        }
        self.ind(); self.push("} else {"); self.nl();
        self.indent += 1; self.write_body(else_body); self.indent -= 1;
        self.line("}");
    }

    fn write_match_arm(&mut self, arm: &MatchArm) {
        self.ind();
        self.push(&fmt_expr(&arm.pattern));
        if arm.body.len() == 1 {
            self.push(": ");
            let single = fmt_stmt_inline(&arm.body[0]);
            if let Some(s) = single {
                self.push("{ "); self.push(&s); self.push(" }"); self.nl();
                return;
            }
        }
        self.push(": {"); self.nl();
        self.indent += 1; self.write_body(&arm.body); self.indent -= 1;
        self.line("}");
    }

    fn write_shape_body(
        &mut self,
        fields: &[FieldDef],
        on_create: &Option<(Vec<Param>, Vec<Stmt>)>,
        acts: &[ActDef],
    ) {
        for field in fields {
            self.ind(); self.push(&field.name);
            if let Some(t) = &field.type_hint { self.push(": "); self.push(t); }
            if let Some(d) = &field.default   { self.push(" = "); self.push(&fmt_expr(d)); }
            self.nl();
        }

        if let Some((params, body)) = on_create {
            if !fields.is_empty() { self.nl(); }
            self.ind(); self.push("on_create("); self.push(&fmt_params(params)); self.push(") {"); self.nl();
            self.indent += 1; self.write_body(body); self.indent -= 1;
            self.line("}");
        }

        for act in acts {
            self.nl();
            self.ind(); self.push("act "); self.push(&act.name);
            self.push("("); self.push(&fmt_params(&act.params)); self.push(") {"); self.nl();
            self.indent += 1; self.write_body(&act.body); self.indent -= 1;
            self.line("}");
        }
    }

    //   Micro-helpers                   ──

    fn write_doc(&mut self, doc: &Option<String>) {
        if let Some(d) = doc {
            for raw_line in d.lines() {
                let trimmed = raw_line.trim();
                self.ind();
                if trimmed.starts_with("///") {
                    self.push(trimmed);
                } else {
                    self.push("/// "); self.push(trimmed);
                }
                self.nl();
            }
        }
    }

    fn write_type_params(&mut self, tp: &[String]) {
        if !tp.is_empty() {
            self.push("<"); self.push(&tp.join(", ")); self.push(">");
        }
    }

    fn write_ret_type(&mut self, rt: &Option<String>) {
        if let Some(r) = rt { self.push(" -> "); self.push(r); }
    }
}

//   Formatter de expresiones (puro, sin estado)           

pub fn fmt_expr(expr: &Expr) -> String {
    match expr {
        Expr::Int(n)        => n.to_string(),
        Expr::Float(f)      => fmt_float(*f),
        Expr::Str(s)        => format!("\"{}\"", s),
        Expr::Bool(true)    => "true".into(),
        Expr::Bool(false)   => "false".into(),
        Expr::Null          => "null".into(),
        Expr::Undefined     => "undefined".into(),
        Expr::Ident(name)   => name.clone(),

        Expr::BinaryOp { op, left, right } => {
            let l = fmt_child(left, op, Side::Left);
            let r = fmt_child(right, op, Side::Right);
            format!("{l} {op} {r}")
        }

        Expr::UnaryOp { op, expr } => {
            let inner = fmt_expr(expr);
            // space only for word-ops (not, ...)
            if op.chars().all(|c| c.is_alphabetic()) {
                format!("{op} {inner}")
            } else {
                format!("{op}{inner}")
            }
        }

        Expr::List(items) => {
            if items.is_empty() { return "[]".into(); }
            let parts: Vec<String> = items.iter().map(fmt_expr).collect();
            let inline = format!("[{}]", parts.join(", "));
            if inline.len() <= 72 { inline } else {
                format!("[\n{}\n]", parts.iter().map(|p| format!("    {p}")).collect::<Vec<_>>().join(",\n"))
            }
        }

        Expr::Dict(pairs) => {
            if pairs.is_empty() { return "{}".into(); }
            let parts: Vec<String> = pairs.iter().map(|(k, v)| format!("{k}: {}", fmt_expr(v))).collect();
            let inline = format!("{{ {} }}", parts.join(", "));
            if inline.len() <= 72 { inline } else {
                format!("{{\n{}\n}}", parts.iter().map(|p| format!("    {p}")).collect::<Vec<_>>().join(",\n"))
            }
        }

        Expr::Call { callee, args, kwargs } => {
            let mut all: Vec<String> = args.iter().map(fmt_expr).collect();
            for (k, v) in kwargs { all.push(format!("{k}: {}", fmt_expr(v))); }
            format!("{}({})", fmt_expr(callee), all.join(", "))
        }

        Expr::CallMethod { method, receiver, args, kwargs } => {
            let mut all: Vec<String> = args.iter().map(fmt_expr).collect();
            for (k, v) in kwargs { all.push(format!("{k}: {}", fmt_expr(v))); }
            format!("{}.{}({})", fmt_expr(receiver), method, all.join(", "))
        }

        Expr::AttrAccess { object, attr }   => format!("{}.{}", fmt_expr(object), attr),
        Expr::NullSafe   { object, attr }   => format!("{}?.{}", fmt_expr(object), attr),
        Expr::Index { object, index }       => format!("{}[{}]", fmt_expr(object), fmt_expr(index)),

        Expr::SliceAccess { object, start, end } => {
            let s = start.as_deref().map(fmt_expr).unwrap_or_default();
            let e = end.as_deref().map(fmt_expr).unwrap_or_default();
            format!("{}[{}..{}]", fmt_expr(object), s, e)
        }

        Expr::IsCheck { expr, shape }       => format!("{} is {}", fmt_expr(expr), shape),
        Expr::Await(inner)                  => format!("await {}", fmt_expr(inner)),

        Expr::Lambda { params, body } => {
            let ps = params.join(", ");
            // Inline lambda: (x) => expr
            if body.len() == 1 {
                if let Stmt::Return { value: Some(v), .. } = &body[0] {
                    return format!("({ps}) => {}", fmt_expr(v));
                }
                if let Stmt::Expr { expr, .. } = &body[0] {
                    return format!("({ps}) => {}", fmt_expr(expr));
                }
            }
            // Multi-statement lambda — emit as block
            let mut inner = Formatter::new();
            inner.indent = 1;
            inner.write_body(body);
            let block = inner.out.trim_end().to_string();
            format!("({ps}) => {{\n{block}\n}}")
        }
    }
}

//   Helpers                       

#[derive(Clone, Copy)]
enum Side { Left, Right }

fn fmt_child(expr: &Expr, parent_op: &str, side: Side) -> String {
    if let Expr::BinaryOp { op: child_op, .. } = expr {
        let cp = precedence(child_op);
        let pp = precedence(parent_op);
        let needs = match side {
            Side::Left  => cp < pp,
            Side::Right => cp < pp || (cp == pp && !is_right_assoc(parent_op)),
        };
        if needs { return format!("({})", fmt_expr(expr)); }
    }
    fmt_expr(expr)
}

fn precedence(op: &str) -> u8 {
    match op {
        "or"  | "||"              => 1,
        "and" | "&&"              => 2,
        "==" | "!=" | "<" | ">"
            | "<=" | ">="        => 3,
        "+"  | "-"               => 4,
        "*"  | "/" | "%"         => 5,
        "**"                     => 6,
        _                        => 7,
    }
}

fn is_right_assoc(op: &str) -> bool { op == "**" }

fn fmt_float(f: f64) -> String {
    let s = format!("{f}");
    if s.contains('.') || s.contains('e') { s } else { format!("{f}.0") }
}

fn fmt_params(params: &[Param]) -> String {
    params.iter().map(|p| {
        let mut s = p.name.clone();
        if let Some(t) = &p.type_hint { s.push_str(": "); s.push_str(t); }
        if let Some(d) = &p.default   { s.push_str(" = "); s.push_str(&fmt_expr(d)); }
        s
    }).collect::<Vec<_>>().join(", ")
}

/// Para match arms con un solo stmt: intenta emitirlo en una línea
fn fmt_stmt_inline(stmt: &Stmt) -> Option<String> {
    match stmt {
        Stmt::Return { value: Some(v), .. } => Some(format!("return {}", fmt_expr(v))),
        Stmt::Return { value: None,    .. } => Some("return".into()),
        Stmt::Show   { value, .. }          => Some(format!("show {}", fmt_expr(value))),
        Stmt::Expr   { expr, .. }           => Some(fmt_expr(expr)),
        _                                   => None,
    }
}

fn is_block(stmt: &Stmt) -> bool {
    matches!(stmt,
        Stmt::Fn      { .. } | Stmt::AsyncFn { .. } | Stmt::Shape   { .. } |
        Stmt::If      { .. } | Stmt::While   { .. } | Stmt::For     { .. } |
        Stmt::Match   { .. } | Stmt::Attempt { .. } | Stmt::Serve   { .. }
    )
}
