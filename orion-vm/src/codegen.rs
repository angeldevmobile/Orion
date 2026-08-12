#![allow(dead_code)]
/// codegen.rs — Generador de bytecode Orion
/// Convierte Vec<Stmt> (AST de parser.rs) en OrionBytecode listo para la VM.
///
/// Equivale a compiler/bytecode_compiler.py pero en Rust puro.

use indexmap::IndexMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::ast::{ActDef, Expr, FieldDef, Handler, Param, Stmt};

static LAMBDA_COUNTER: AtomicUsize = AtomicUsize::new(0);
use crate::bytecode::{ActDef as BcActDef, ExternFnDef, FieldDef as BcFieldDef, FunctionDef, OrionBytecode, ShapeDef};
use crate::instruction::Instruction;

//    Error de codegen                                                           

#[derive(Debug)]
pub struct CodegenError {
    pub message: String,
    pub line: u32,
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CodegenError [línea {}]: {}", self.line, self.message)
    }
}

//    Punto de entrada                                                           

pub fn compile(mut stmts: Vec<Stmt>) -> Result<OrionBytecode, CodegenError> {
    // Pase previo: reordenar argumentos con nombre (`f(x = 1)`) a posicional.
    crate::named_args::resolve(&mut stmts)?;
    let mut cg = Codegen::new();
    cg.compile_program(stmts)?;
    Ok(cg.into_bytecode())
}

//    Compilador principal                                                       

/// Contexto del loop más interno durante la compilación: acumula los `Jump(0)`
/// de break/continue para parchearlos al cerrar el loop. Sin esto, break y
/// continue saltaban a la instrucción 0 (= reiniciar el programa/función).
#[derive(Default)]
struct LoopCtx {
    breaks:    Vec<usize>,
    continues: Vec<usize>,
}

struct Codegen {
    main_instrs: Vec<Instruction>,
    main_lines:  Vec<u32>,
    functions:   IndexMap<String, FunctionDef>,
    shapes:      IndexMap<String, ShapeDef>,
    extern_fns:  IndexMap<String, ExternFnDef>,
    async_fns:   std::collections::HashSet<String>,
    current_line: u32,
    for_counter:  usize,
    match_counter: usize,
    loop_stack:   Vec<LoopCtx>,
}

impl Codegen {
    fn new() -> Self {
        Codegen {
            main_instrs:   Vec::new(),
            main_lines:    Vec::new(),
            functions:     IndexMap::new(),
            shapes:        IndexMap::new(),
            extern_fns:    IndexMap::new(),
            async_fns:     std::collections::HashSet::new(),
            current_line:  0,
            for_counter:   0,
            match_counter: 0,
            loop_stack:    Vec::new(),
        }
    }

    /// Cierra el loop más interno: parchea sus break → `end` y sus
    /// continue → `cont` (re-evaluación de condición o paso de incremento).
    fn close_loop(&mut self, ctx: LoopCtx, end: usize, cont: usize) {
        for b in ctx.breaks    { self.patch(b, Instruction::Jump(end)); }
        for c in ctx.continues { self.patch(c, Instruction::Jump(cont)); }
    }

    fn emit(&mut self, instr: Instruction) -> usize {
        self.main_instrs.push(instr);
        self.main_lines.push(self.current_line);
        self.main_instrs.len() - 1
    }

    fn patch(&mut self, idx: usize, instr: Instruction) {
        self.main_instrs[idx] = instr;
    }

    fn addr(&self) -> usize {
        self.main_instrs.len()
    }

    fn into_bytecode(self) -> OrionBytecode {
        OrionBytecode {
            main:       self.main_instrs,
            lines:      self.main_lines,
            functions:  self.functions,
            shapes:     self.shapes,
            extern_fns: self.extern_fns,
        }
    }

    //    Dos pasadas                                                            

    fn compile_program(&mut self, stmts: Vec<Stmt>) -> Result<(), CodegenError> {
        // Primer pase: registrar funciones y shapes (sin emitir código principal)
        for stmt in &stmts {
            match stmt {
                Stmt::Fn { name, params, body, .. } => {
                    let fc = self.compile_fn_body(params, body)?;
                    self.functions.insert(name.clone(), fc);
                }
                Stmt::AsyncFn { name, params, body, .. } => {
                    let fc = self.compile_fn_body(params, body)?;
                    self.functions.insert(name.clone(), fc);
                    self.async_fns.insert(name.clone());
                }
                Stmt::Shape { name, fields, on_create, on_error, acts, using, .. } => {
                    // Los `static act` no son métodos: no reciben instancia y no
                    // se buscan sobre un valor. Se registran como funciones
                    // normales con el nombre compuesto "Shape::act", que es
                    // exactamente lo que el parser produce al ver `Shape::act`.
                    // Así el despacho, los defaults de parámetros y el JIT los
                    // tratan como a cualquier función, sin un camino aparte.
                    let (statics, instance_acts): (Vec<&ActDef>, Vec<&ActDef>) =
                        acts.iter().partition(|a| a.is_static);

                    for act in statics {
                        let qualified = format!("{name}::{}", act.name);
                        let f = self.compile_fn_body(&act.params, &act.body)?;
                        self.functions.insert(qualified, f);
                    }

                    let instance_acts: Vec<ActDef> =
                        instance_acts.into_iter().cloned().collect();
                    let shape = self.compile_shape(fields, on_create, on_error, &instance_acts, using)?;
                    self.shapes.insert(name.clone(), shape);
                }
                Stmt::ExternFn { name, params, ret_type, lib, .. } => {
                    let param_types: Vec<String> = params.iter()
                        .map(|p| p.type_hint.clone().unwrap_or_else(|| "ptr".to_string()))
                        .collect();
                    self.extern_fns.insert(name.clone(), ExternFnDef {
                        params:   param_types,
                        ret_type: ret_type.clone().unwrap_or_else(|| "void".to_string()),
                        lib:      lib.clone(),
                    });
                }
                _ => {}
            }
        }

        // Segundo pase: emitir código principal
        for stmt in stmts {
            match &stmt {
                Stmt::Fn { .. } | Stmt::AsyncFn { .. } | Stmt::Shape { .. } | Stmt::ExternFn { .. } => {}
                _ => self.compile_stmt(stmt)?,
            }
        }
        self.emit(Instruction::Halt);
        Ok(())
    }

    //    Función / act                                                          

    fn compile_fn_body(&mut self, params: &[Param], body: &[Stmt]) -> Result<FunctionDef, CodegenError> {
        // Defaults como mini-bytecode (igual que FieldDef.default). Regla: un
        // parámetro con default no puede ir seguido de uno obligatorio, para que
        // la ligadura posicional (rellenar la cola) sea inequívoca.
        let mut param_defaults: Vec<Option<Vec<Instruction>>> = Vec::with_capacity(params.len());
        let mut seen_default = false;
        for p in params {
            if let Some(default_expr) = &p.default {
                seen_default = true;
                let mut dfc = FnCompiler::new();
                dfc.compile_expr(default_expr, &self.async_fns)?;
                dfc.emit(Instruction::Return);
                param_defaults.push(Some(dfc.instrs));
            } else {
                if seen_default {
                    return Err(CodegenError {
                        message: format!(
                            "el parámetro '{}' no puede ir después de uno con valor por defecto",
                            p.name
                        ),
                        line: 0,
                    });
                }
                param_defaults.push(None);
            }
        }

        let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
        let mut fc = FnCompiler::new();
        for stmt in body {
            fc.compile_stmt(stmt, &self.async_fns)?;
        }
        fc.emit(Instruction::LoadNull);
        fc.emit(Instruction::Return);
        // Register any lambdas discovered inside the function body
        for (name, func) in fc.pending_lambdas.drain(..) {
            self.functions.insert(name, func);
        }
        Ok(FunctionDef { params: param_names, body: fc.instrs, lines: fc.lines, param_defaults })
    }

    //    Shape                                                                  

    fn compile_shape(
        &self,
        fields:    &[FieldDef],
        on_create: &Option<(Vec<crate::ast::Param>, Vec<Stmt>)>,
        on_error:  &Option<(Vec<crate::ast::Param>, Vec<Stmt>)>,
        acts:      &[ActDef],
        using:     &[String],
    ) -> Result<ShapeDef, CodegenError> {
        // Campos con su mini-bytecode de valor por defecto
        let mut bc_fields = Vec::new();
        for f in fields {
            let default_instrs = if let Some(default_expr) = &f.default {
                let mut fc = FnCompiler::new();
                fc.compile_expr(default_expr, &self.async_fns)?;
                fc.emit(Instruction::Return);
                fc.instrs
            } else {
                vec![Instruction::LoadNull, Instruction::Return]
            };
            bc_fields.push(BcFieldDef {
                name:      f.name.clone(),
                type_hint: f.type_hint.clone(),
                default:   default_instrs,
            });
        }

        // on_create — versión clone-friendly
        let bc_on_create = if let Some((oc_params, oc_body)) = on_create.as_ref() {
            let mut fc = FnCompiler::new();
            for stmt in oc_body {
                fc.compile_stmt(stmt, &self.async_fns)?;
            }
            fc.emit(Instruction::LoadNull);
            fc.emit(Instruction::Return);
            let params = oc_params.iter().map(|p| p.name.clone()).collect();
            Some(BcActDef { params, body: fc.instrs, lines: fc.lines })
        } else { None };

        // on_error — mismo esquema que on_create; recibe el mensaje del error
        let bc_on_error = if let Some((oe_params, oe_body)) = on_error.as_ref() {
            let mut fc = FnCompiler::new();
            for stmt in oe_body {
                fc.compile_stmt(stmt, &self.async_fns)?;
            }
            fc.emit(Instruction::LoadNull);
            fc.emit(Instruction::Return);
            let params = oe_params.iter().map(|p| p.name.clone()).collect();
            Some(BcActDef { params, body: fc.instrs, lines: fc.lines })
        } else { None };

        // acts
        let mut bc_acts = IndexMap::new();
        for act in acts {
            let mut fc = FnCompiler::new();
            for stmt in &act.body {
                fc.compile_stmt(stmt, &self.async_fns)?;
            }
            fc.emit(Instruction::LoadNull);
            fc.emit(Instruction::Return);
            bc_acts.insert(act.name.clone(), BcActDef {
                params: act.params.iter().map(|p| p.name.clone()).collect(),
                body: fc.instrs,
                lines: fc.lines,
            });
        }

        Ok(ShapeDef {
            fields:    bc_fields,
            on_create: bc_on_create,
            on_error:  bc_on_error,
            acts:      bc_acts,
            using:     using.to_vec(),
        })
    }

    //    Statements (código principal)                                          

    fn compile_stmt(&mut self, stmt: Stmt) -> Result<(), CodegenError> {
        match stmt {
            Stmt::Assign { name, value, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&value)?;
                self.emit(Instruction::StoreVar(name));
            }
            Stmt::TypedAssign { name, type_hint: _, value, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&value)?;
                self.emit(Instruction::StoreVar(name));
            }
            Stmt::Const { name, value, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&value)?;
                self.emit(Instruction::StoreConst(name));
            }
            Stmt::AugAssign { name, op, value, line, .. } => {
                self.current_line = line;
                self.emit(Instruction::LoadVar(name.clone()));
                self.compile_expr_main(&value)?;
                let instr = op_instr(&op).ok_or_else(|| CodegenError {
                    message: format!("operador de asignación no soportado: '{op}='"),
                    line,
                })?;
                self.emit(instr);
                self.emit(Instruction::StoreVar(name));
            }
            Stmt::AssignIndex { object, index, value, line, .. } => {
                self.current_line = line;
                let var_name = if let Expr::Ident(n) = &object { Some(n.clone()) } else { None };
                self.compile_expr_main(&object)?;
                self.compile_expr_main(&index)?;
                self.compile_expr_main(&value)?;
                self.emit(Instruction::SetIndex);
                match var_name {
                    Some(name) => { self.emit(Instruction::StoreVar(name)); }
                    None => { self.emit(Instruction::Pop); }
                }
            }
            Stmt::AssignAttr { object, attr, value, line, .. } => {
                self.current_line = line;
                // Si el objeto es una variable (no `me`), reescribimos su valor tras
                // SetAttr: necesario para dicts (por valor); inofensivo para instancias.
                let var_name = match &object {
                    Expr::Ident(n) if n != "me" => Some(n.clone()),
                    _ => None,
                };
                self.compile_expr_main(&object)?;
                self.compile_expr_main(&value)?;
                self.emit(Instruction::SetAttr(attr));
                match var_name {
                    Some(name) => { self.emit(Instruction::StoreVar(name)); }
                    None => { self.emit(Instruction::Pop); }
                }
            }
            Stmt::Show { value, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&value)?;
                self.emit(Instruction::Show);
            }
            Stmt::Return { value, line, .. } => {
                self.current_line = line;
                if let Some(v) = value {
                    self.compile_expr_main(&v)?;
                } else {
                    self.emit(Instruction::LoadNull);
                }
                self.emit(Instruction::Return);
            }
            Stmt::Break { line, .. } => {
                if self.loop_stack.is_empty() {
                    return Err(CodegenError { message: "break fuera de un loop".into(), line });
                }
                let j = self.emit(Instruction::Jump(0));
                self.loop_stack.last_mut().unwrap().breaks.push(j);
            }
            Stmt::Continue { line, .. } => {
                if self.loop_stack.is_empty() {
                    return Err(CodegenError { message: "continue fuera de un loop".into(), line });
                }
                let j = self.emit(Instruction::Jump(0));
                self.loop_stack.last_mut().unwrap().continues.push(j);
            }

            Stmt::If { cond, then_body, else_body, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&cond)?;
                let jf = self.emit(Instruction::JumpIfFalse(0));
                for s in then_body { self.compile_stmt(s)?; }
                if !else_body.is_empty() {
                    let je = self.emit(Instruction::Jump(0));
                    let else_addr = self.addr();
                    self.patch(jf, Instruction::JumpIfFalse(else_addr));
                    for s in else_body { self.compile_stmt(s)?; }
                    let end = self.addr();
                    self.patch(je, Instruction::Jump(end));
                } else {
                    let end = self.addr();
                    self.patch(jf, Instruction::JumpIfFalse(end));
                }
            }

            Stmt::While { cond, body, line, .. } => {
                self.current_line = line;
                let loop_start = self.addr();
                self.compile_expr_main(&cond)?;
                let jf = self.emit(Instruction::JumpIfFalse(0));
                self.loop_stack.push(LoopCtx::default());
                for s in body { self.compile_stmt(s)?; }
                let ctx = self.loop_stack.pop().unwrap();
                self.emit(Instruction::Jump(loop_start));
                let end = self.addr();
                self.patch(jf, Instruction::JumpIfFalse(end));
                // continue re-evalúa la condición; break sale del loop
                self.close_loop(ctx, end, loop_start);
            }

            Stmt::For { var, iter, body, line, .. } => {
                self.current_line = line;
                let ctr = self.for_counter;
                self.for_counter += 1;

                // range syntax: for i in start..end  (y ..<, ...)
                //
                // Recorrer el rango con un contador evita construir la lista, así
                // que `for i in 0..1000000` no reserva un millón de elementos. La
                // única diferencia entre las tres formas es el comparador: los
                // exclusivos paran antes del extremo, el inclusivo lo incluye.
                if let Expr::BinaryOp { op, left, right } = &iter {
                    if op == ".." || op == "..<" || op == "..." {
                        let cur_var = format!("__cur_{ctr}__");
                        let end_var = format!("__end_{ctr}__");
                        self.compile_expr_main(left)?;
                        self.emit(Instruction::StoreVar(cur_var.clone()));
                        self.compile_expr_main(right)?;
                        self.emit(Instruction::StoreVar(end_var.clone()));
                        let loop_start = self.addr();
                        self.emit(Instruction::LoadVar(cur_var.clone()));
                        self.emit(Instruction::LoadVar(end_var.clone()));
                        self.emit(if op == "..." { Instruction::LtEq } else { Instruction::Lt });
                        let jf = self.emit(Instruction::JumpIfFalse(0));
                        self.emit(Instruction::LoadVar(cur_var.clone()));
                        self.emit(Instruction::StoreVar(var));
                        self.loop_stack.push(LoopCtx::default());
                        for s in body { self.compile_stmt(s)?; }
                        let ctx = self.loop_stack.pop().unwrap();
                        let cont = self.addr(); // continue salta al incremento
                        self.emit(Instruction::LoadVar(cur_var.clone()));
                        self.emit(Instruction::LoadInt(1));
                        self.emit(Instruction::Add);
                        self.emit(Instruction::StoreVar(cur_var));
                        self.emit(Instruction::Jump(loop_start));
                        let end = self.addr();
                        self.patch(jf, Instruction::JumpIfFalse(end));
                        self.close_loop(ctx, end, cont);
                        return Ok(());
                    }
                }

                let list_var = format!("__list_{ctr}__");
                let len_var  = format!("__len_{ctr}__");
                let idx_var  = format!("__idx_{ctr}__");

                self.compile_expr_main(&iter)?;
                self.emit(Instruction::StoreVar(list_var.clone()));
                self.emit(Instruction::LoadVar(list_var.clone()));
                self.emit(Instruction::Call("len".into(), 1));
                self.emit(Instruction::StoreVar(len_var.clone()));
                self.emit(Instruction::LoadInt(0));
                self.emit(Instruction::StoreVar(idx_var.clone()));

                let loop_start = self.addr();
                self.emit(Instruction::LoadVar(idx_var.clone()));
                self.emit(Instruction::LoadVar(len_var.clone()));
                self.emit(Instruction::Lt);
                let jf = self.emit(Instruction::JumpIfFalse(0));

                self.emit(Instruction::LoadVar(list_var.clone()));
                self.emit(Instruction::LoadVar(idx_var.clone()));
                self.emit(Instruction::GetIndex);
                self.emit(Instruction::StoreVar(var));

                self.loop_stack.push(LoopCtx::default());
                for s in body { self.compile_stmt(s)?; }
                let ctx = self.loop_stack.pop().unwrap();

                let cont = self.addr(); // continue salta al incremento
                self.emit(Instruction::LoadVar(idx_var.clone()));
                self.emit(Instruction::LoadInt(1));
                self.emit(Instruction::Add);
                self.emit(Instruction::StoreVar(idx_var));
                self.emit(Instruction::Jump(loop_start));
                let end = self.addr();
                self.patch(jf, Instruction::JumpIfFalse(end));
                self.close_loop(ctx, end, cont);
            }

            Stmt::Match { expr, arms, line, .. } => {
                self.current_line = line;
                let ctr = self.match_counter;
                self.match_counter += 1;
                let subj = format!("__match_{ctr}__");

                self.compile_expr_main(&expr)?;
                self.emit(Instruction::StoreVar(subj.clone()));

                let mut end_jumps = Vec::new();
                for arm in arms {
                    self.emit(Instruction::LoadVar(subj.clone()));
                    self.compile_expr_main(&arm.pattern)?;
                    self.emit(Instruction::Eq);
                    let skip = self.emit(Instruction::JumpIfFalse(0));
                    for s in arm.body { self.compile_stmt(s)?; }
                    end_jumps.push(self.emit(Instruction::Jump(0)));
                    let next = self.addr();
                    self.patch(skip, Instruction::JumpIfFalse(next));
                }
                let end = self.addr();
                for j in end_jumps { self.patch(j, Instruction::Jump(end)); }
            }

            Stmt::Attempt { body, handler, line, .. } => {
                self.current_line = line;
                let begin_patch = self.emit(Instruction::BeginAttempt(0));
                for s in body { self.compile_stmt(s)?; }
                let end_patch = self.emit(Instruction::EndAttempt(0));

                let handler_addr = self.addr();
                self.patch(begin_patch, Instruction::BeginAttempt(handler_addr));

                if let Some(Handler { err_name, body: hbody }) = handler {
                    self.emit(Instruction::StoreVar(err_name));
                    for s in hbody { self.compile_stmt(s)?; }
                } else {
                    self.emit(Instruction::Pop);
                }
                let end = self.addr();
                self.patch(end_patch, Instruction::EndAttempt(end));
            }

            // `with` compila vía desugar (assign + attempt + free); el JIT
            // hereda la semántica porque compila desde este mismo bytecode.
            Stmt::With { var, init, body, line, col } => {
                self.current_line = line;
                let receiver = match &init {
                    Expr::CallMethod { receiver, .. } => receiver.as_ref().clone(),
                    _ => return Err(CodegenError {
                        message: "with: el inicializador debe ser modulo.fn(...)".into(),
                        line,
                    }),
                };
                for s in crate::ast::desugar_with(&var, &init, &receiver, body, line, col) {
                    self.compile_stmt(s)?;
                }
            }

            Stmt::ErrorStmt { msg, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&msg)?;
                self.emit(Instruction::Raise);
            }

            Stmt::Think { prompt, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&prompt)?;
                self.emit(Instruction::AiAsk);
                self.emit(Instruction::Show);
            }
            Stmt::Learn { text, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&text)?;
                self.emit(Instruction::AiLearn);
                self.emit(Instruction::Show);
            }
            Stmt::Sense { query, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&query)?;
                self.emit(Instruction::AiSense);
                self.emit(Instruction::Show);
            }

            Stmt::Spawn { call, line, .. } => {
                self.current_line = line;
                if let Expr::Call { callee, args, .. } = &call {
                    if let Expr::Ident(fn_name) = callee.as_ref() {
                        for a in args { self.compile_expr_main(a)?; }
                        self.emit(Instruction::CallAsync(fn_name.clone(), args.len() as u8));
                        self.emit(Instruction::Pop);
                        return Ok(());
                    }
                }
                self.compile_expr_main(&call)?;
                self.emit(Instruction::Pop);
            }

            Stmt::Await { expr, var, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&expr)?;
                self.emit(Instruction::Await);
                if let Some(v) = var {
                    self.emit(Instruction::StoreVar(v));
                } else {
                    self.emit(Instruction::Pop);
                }
            }

            Stmt::Ask { prompt, var, cast, choices, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&prompt)?;
                if let Some(choices_expr) = choices {
                    self.compile_expr_main(&choices_expr)?;
                    self.emit(Instruction::ReadInput { cast, choices: true });
                } else {
                    self.emit(Instruction::ReadInput { cast, choices: false });
                }
                self.emit(Instruction::StoreVar(var));
            }

            Stmt::Read { path, var, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&path)?;
                self.emit(Instruction::ReadFile("text".into()));
                self.emit(Instruction::StoreVar(var));
            }

            Stmt::Write { path, content, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&path)?;
                self.compile_expr_main(&content)?;
                self.emit(Instruction::WriteFile("write".into()));
            }

            Stmt::Append { path, content, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&path)?;
                self.compile_expr_main(&content)?;
                self.emit(Instruction::WriteFile("append".into()));
            }

            Stmt::Serve { port, routes, line, .. } => {
                self.current_line = line;
                // El handler es el primer statement si es una función
                let fn_name = routes.first().and_then(|s| {
                    if let Stmt::Expr { expr: Expr::Ident(n), .. } = s { Some(n.clone()) } else { None }
                }).unwrap_or_else(|| "__serve_handler__".into());
                self.compile_expr_main(&port)?;
                self.emit(Instruction::ServeHTTP(fn_name));
            }

            Stmt::Use { path, alias, selective, .. } => {
                let ns = alias.clone().unwrap_or_else(|| {
                    std::path::Path::new(path.as_str())
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(path.as_str())
                        .to_string()
                });
                let take = selective.clone().unwrap_or_default();
                self.emit(Instruction::UseModule(path.clone(), ns, take));
            }
            Stmt::Fn { .. }      => {} // compilado en primer pase
            Stmt::AsyncFn { .. } => {} // compilado en primer pase
            Stmt::ExternFn { .. }=> {} // registrado en primer pase
            Stmt::Shape { name, .. } => {
                self.emit(Instruction::DefineShape(name));
            }

            Stmt::Expr { expr, line, .. } => {
                self.current_line = line;
                self.compile_expr_main(&expr)?;
                self.emit(Instruction::Pop);
            }
        }
        Ok(())
    }

    //    Expresiones (código principal)                                         
    // Delega a FnCompiler pero emitiendo en self.main_instrs

    fn compile_expr_main(&mut self, expr: &Expr) -> Result<(), CodegenError> {
        // Para el código principal usamos el mismo FnCompiler inlinado sobre self
        let mut extra_fns: Vec<(String, FunctionDef)> = Vec::new();
        compile_expr_into(
            &mut self.main_instrs,
            &mut self.main_lines,
            self.current_line,
            &self.async_fns,
            &mut extra_fns,
            expr,
        )?;
        for (name, func) in extra_fns {
            self.functions.insert(name, func);
        }
        Ok(())
    }
}

//    FnCompiler — compilador de cuerpos de función                             

struct FnCompiler {
    instrs: Vec<Instruction>,
    lines:  Vec<u32>,
    current_line: u32,
    for_counter:  usize,
    match_counter: usize,
    pending_lambdas: Vec<(String, FunctionDef)>,
    loop_stack: Vec<LoopCtx>,
}

impl FnCompiler {
    fn new() -> Self {
        FnCompiler { instrs: Vec::new(), lines: Vec::new(), current_line: 0, for_counter: 0, match_counter: 0, pending_lambdas: Vec::new(), loop_stack: Vec::new() }
    }

    /// Igual que Codegen::close_loop: parchea break → end, continue → cont.
    fn close_loop(&mut self, ctx: LoopCtx, end: usize, cont: usize) {
        for b in ctx.breaks    { self.patch(b, Instruction::Jump(end)); }
        for c in ctx.continues { self.patch(c, Instruction::Jump(cont)); }
    }

    fn emit(&mut self, instr: Instruction) -> usize {
        self.instrs.push(instr);
        self.lines.push(self.current_line);
        self.instrs.len() - 1
    }

    fn patch(&mut self, idx: usize, instr: Instruction) {
        self.instrs[idx] = instr;
    }

    fn addr(&self) -> usize { self.instrs.len() }

    fn compile_stmt(&mut self, stmt: &Stmt, async_fns: &std::collections::HashSet<String>) -> Result<(), CodegenError> {
        match stmt {
            Stmt::Assign { name, value, line, .. } => {
                self.current_line = *line;
                self.compile_expr(value, async_fns)?;
                self.emit(Instruction::StoreVar(name.clone()));
            }
            Stmt::TypedAssign { name, type_hint: _, value, line, .. } => {
                self.current_line = *line;
                self.compile_expr(value, async_fns)?;
                self.emit(Instruction::StoreVar(name.clone()));
            }
            Stmt::Const { name, value, line, .. } => {
                self.current_line = *line;
                self.compile_expr(value, async_fns)?;
                self.emit(Instruction::StoreConst(name.clone()));
            }
            Stmt::AugAssign { name, op, value, line, .. } => {
                self.current_line = *line;
                self.emit(Instruction::LoadVar(name.clone()));
                self.compile_expr(value, async_fns)?;
                let instr = op_instr(op).ok_or_else(|| CodegenError {
                    message: format!("operador de asignación no soportado: '{op}='"),
                    line: *line,
                })?;
                self.emit(instr);
                self.emit(Instruction::StoreVar(name.clone()));
            }
            Stmt::AssignIndex { object, index, value, line, .. } => {
                self.current_line = *line;
                let var_name = if let Expr::Ident(n) = object { Some(n.clone()) } else { None };
                self.compile_expr(object, async_fns)?;
                self.compile_expr(index, async_fns)?;
                self.compile_expr(value, async_fns)?;
                self.emit(Instruction::SetIndex);
                match var_name {
                    Some(name) => { self.emit(Instruction::StoreVar(name)); }
                    None => { self.emit(Instruction::Pop); }
                }
            }
            Stmt::AssignAttr { object, attr, value, line, .. } => {
                self.current_line = *line;
                let var_name = match object {
                    Expr::Ident(n) if n != "me" => Some(n.clone()),
                    _ => None,
                };
                self.compile_expr(object, async_fns)?;
                self.compile_expr(value, async_fns)?;
                self.emit(Instruction::SetAttr(attr.clone()));
                match var_name {
                    Some(name) => { self.emit(Instruction::StoreVar(name)); }
                    None => { self.emit(Instruction::Pop); }
                }
            }
            Stmt::Show { value, line, .. } => {
                self.current_line = *line;
                self.compile_expr(value, async_fns)?;
                self.emit(Instruction::Show);
            }
            Stmt::Return { value, line, .. } => {
                self.current_line = *line;
                if let Some(v) = value {
                    self.compile_expr(v, async_fns)?;
                } else {
                    self.emit(Instruction::LoadNull);
                }
                self.emit(Instruction::Return);
            }
            Stmt::Break { line, .. } => {
                if self.loop_stack.is_empty() {
                    return Err(CodegenError { message: "break fuera de un loop".into(), line: *line });
                }
                let j = self.emit(Instruction::Jump(0));
                self.loop_stack.last_mut().unwrap().breaks.push(j);
            }
            Stmt::Continue { line, .. } => {
                if self.loop_stack.is_empty() {
                    return Err(CodegenError { message: "continue fuera de un loop".into(), line: *line });
                }
                let j = self.emit(Instruction::Jump(0));
                self.loop_stack.last_mut().unwrap().continues.push(j);
            }

            Stmt::If { cond, then_body, else_body, line, .. } => {
                self.current_line = *line;
                self.compile_expr(cond, async_fns)?;
                let jf = self.emit(Instruction::JumpIfFalse(0));
                for s in then_body { self.compile_stmt(s, async_fns)?; }
                if !else_body.is_empty() {
                    let je = self.emit(Instruction::Jump(0));
                    let else_addr = self.addr();
                    self.patch(jf, Instruction::JumpIfFalse(else_addr));
                    for s in else_body { self.compile_stmt(s, async_fns)?; }
                    let end = self.addr();
                    self.patch(je, Instruction::Jump(end));
                } else {
                    let end = self.addr();
                    self.patch(jf, Instruction::JumpIfFalse(end));
                }
            }

            Stmt::While { cond, body, line, .. } => {
                self.current_line = *line;
                let loop_start = self.addr();
                self.compile_expr(cond, async_fns)?;
                let jf = self.emit(Instruction::JumpIfFalse(0));
                self.loop_stack.push(LoopCtx::default());
                for s in body { self.compile_stmt(s, async_fns)?; }
                let ctx = self.loop_stack.pop().unwrap();
                self.emit(Instruction::Jump(loop_start));
                let end = self.addr();
                self.patch(jf, Instruction::JumpIfFalse(end));
                self.close_loop(ctx, end, loop_start);
            }

            Stmt::For { var, iter, body, line, .. } => {
                self.current_line = *line;
                let ctr = self.for_counter;
                self.for_counter += 1;

                // range syntax: for i in start..end  (y ..<, ...)
                // Mismo criterio que en el camino de `main`: contador en vez de
                // lista, y el comparador es lo único que separa el inclusivo de
                // los dos exclusivos.
                if let Expr::BinaryOp { op, left, right } = iter {
                    if op == ".." || op == "..<" || op == "..." {
                        let cur_var = format!("__cur_{ctr}__");
                        let end_var = format!("__end_{ctr}__");
                        self.compile_expr(left, async_fns)?;
                        self.emit(Instruction::StoreVar(cur_var.clone()));
                        self.compile_expr(right, async_fns)?;
                        self.emit(Instruction::StoreVar(end_var.clone()));
                        let loop_start = self.addr();
                        self.emit(Instruction::LoadVar(cur_var.clone()));
                        self.emit(Instruction::LoadVar(end_var.clone()));
                        self.emit(if op == "..." { Instruction::LtEq } else { Instruction::Lt });
                        let jf = self.emit(Instruction::JumpIfFalse(0));
                        self.emit(Instruction::LoadVar(cur_var.clone()));
                        self.emit(Instruction::StoreVar(var.clone()));
                        self.loop_stack.push(LoopCtx::default());
                        for s in body { self.compile_stmt(s, async_fns)?; }
                        let ctx = self.loop_stack.pop().unwrap();
                        let cont = self.addr(); // continue salta al incremento
                        self.emit(Instruction::LoadVar(cur_var.clone()));
                        self.emit(Instruction::LoadInt(1));
                        self.emit(Instruction::Add);
                        self.emit(Instruction::StoreVar(cur_var));
                        self.emit(Instruction::Jump(loop_start));
                        let end = self.addr();
                        self.patch(jf, Instruction::JumpIfFalse(end));
                        self.close_loop(ctx, end, cont);
                        return Ok(());
                    }
                }

                let list_var = format!("__list_{ctr}__");
                let len_var  = format!("__len_{ctr}__");
                let idx_var  = format!("__idx_{ctr}__");

                self.compile_expr(iter, async_fns)?;
                self.emit(Instruction::StoreVar(list_var.clone()));
                self.emit(Instruction::LoadVar(list_var.clone()));
                self.emit(Instruction::Call("len".into(), 1));
                self.emit(Instruction::StoreVar(len_var.clone()));
                self.emit(Instruction::LoadInt(0));
                self.emit(Instruction::StoreVar(idx_var.clone()));

                let loop_start = self.addr();
                self.emit(Instruction::LoadVar(idx_var.clone()));
                self.emit(Instruction::LoadVar(len_var.clone()));
                self.emit(Instruction::Lt);
                let jf = self.emit(Instruction::JumpIfFalse(0));

                self.emit(Instruction::LoadVar(list_var.clone()));
                self.emit(Instruction::LoadVar(idx_var.clone()));
                self.emit(Instruction::GetIndex);
                self.emit(Instruction::StoreVar(var.clone()));

                self.loop_stack.push(LoopCtx::default());
                for s in body { self.compile_stmt(s, async_fns)?; }
                let ctx = self.loop_stack.pop().unwrap();

                let cont = self.addr(); // continue salta al incremento
                self.emit(Instruction::LoadVar(idx_var.clone()));
                self.emit(Instruction::LoadInt(1));
                self.emit(Instruction::Add);
                self.emit(Instruction::StoreVar(idx_var.clone()));
                self.emit(Instruction::Jump(loop_start));
                let end = self.addr();
                self.patch(jf, Instruction::JumpIfFalse(end));
                self.close_loop(ctx, end, cont);
            }

            Stmt::Match { expr, arms, line, .. } => {
                self.current_line = *line;
                let ctr = self.match_counter;
                self.match_counter += 1;
                let subj = format!("__match_{ctr}__");

                self.compile_expr(expr, async_fns)?;
                self.emit(Instruction::StoreVar(subj.clone()));

                let mut end_jumps = Vec::new();
                for arm in arms {
                    self.emit(Instruction::LoadVar(subj.clone()));
                    self.compile_expr(&arm.pattern, async_fns)?;
                    self.emit(Instruction::Eq);
                    let skip = self.emit(Instruction::JumpIfFalse(0));
                    for s in &arm.body { self.compile_stmt(s, async_fns)?; }
                    end_jumps.push(self.emit(Instruction::Jump(0)));
                    let next = self.addr();
                    self.patch(skip, Instruction::JumpIfFalse(next));
                }
                let end = self.addr();
                for j in end_jumps { self.patch(j, Instruction::Jump(end)); }
            }

            Stmt::Attempt { body, handler, line, .. } => {
                self.current_line = *line;
                let begin_patch = self.emit(Instruction::BeginAttempt(0));
                for s in body { self.compile_stmt(s, async_fns)?; }
                let end_patch = self.emit(Instruction::EndAttempt(0));

                let handler_addr = self.addr();
                self.patch(begin_patch, Instruction::BeginAttempt(handler_addr));

                if let Some(h) = handler {
                    self.emit(Instruction::StoreVar(h.err_name.clone()));
                    for s in &h.body { self.compile_stmt(s, async_fns)?; }
                } else {
                    self.emit(Instruction::Pop);
                }
                let end = self.addr();
                self.patch(end_patch, Instruction::EndAttempt(end));
            }

            // `with` compila vía desugar (assign + attempt + free), igual que
            // en el codegen del script principal.
            Stmt::With { var, init, body, line, col } => {
                self.current_line = *line;
                let receiver = match init {
                    Expr::CallMethod { receiver, .. } => receiver.as_ref().clone(),
                    _ => return Err(CodegenError {
                        message: "with: el inicializador debe ser modulo.fn(...)".into(),
                        line: *line,
                    }),
                };
                for s in crate::ast::desugar_with(var, init, &receiver, body.clone(), *line, *col) {
                    self.compile_stmt(&s, async_fns)?;
                }
            }

            Stmt::ErrorStmt { msg, line, .. } => {
                self.current_line = *line;
                self.compile_expr(msg, async_fns)?;
                self.emit(Instruction::Raise);
            }

            Stmt::Think { prompt, line, .. } => {
                self.current_line = *line;
                self.compile_expr(prompt, async_fns)?;
                self.emit(Instruction::AiAsk);
                self.emit(Instruction::Show);
            }
            Stmt::Learn { text, line, .. } => {
                self.current_line = *line;
                self.compile_expr(text, async_fns)?;
                self.emit(Instruction::AiLearn);
                self.emit(Instruction::Show);
            }
            Stmt::Sense { query, line, .. } => {
                self.current_line = *line;
                self.compile_expr(query, async_fns)?;
                self.emit(Instruction::AiSense);
                self.emit(Instruction::Show);
            }

            Stmt::Spawn { call, line, .. } => {
                self.current_line = *line;
                if let Expr::Call { callee, args, .. } = call {
                    if let Expr::Ident(fn_name) = callee.as_ref() {
                        for a in args { self.compile_expr(a, async_fns)?; }
                        self.emit(Instruction::CallAsync(fn_name.clone(), args.len() as u8));
                        self.emit(Instruction::Pop);
                        return Ok(());
                    }
                }
                self.compile_expr(call, async_fns)?;
                self.emit(Instruction::Pop);
            }

            Stmt::Await { expr, var, line, .. } => {
                self.current_line = *line;
                self.compile_expr(expr, async_fns)?;
                self.emit(Instruction::Await);
                if let Some(v) = var {
                    self.emit(Instruction::StoreVar(v.clone()));
                } else {
                    self.emit(Instruction::Pop);
                }
            }

            Stmt::Ask { prompt, var, cast, choices, line, .. } => {
                self.current_line = *line;
                self.compile_expr(prompt, async_fns)?;
                if let Some(choices_expr) = choices {
                    self.compile_expr(choices_expr, async_fns)?;
                    self.emit(Instruction::ReadInput { cast: cast.clone(), choices: true });
                } else {
                    self.emit(Instruction::ReadInput { cast: cast.clone(), choices: false });
                }
                self.emit(Instruction::StoreVar(var.clone()));
            }

            Stmt::Read { path, var, line, .. } => {
                self.current_line = *line;
                self.compile_expr(path, async_fns)?;
                self.emit(Instruction::ReadFile("text".into()));
                self.emit(Instruction::StoreVar(var.clone()));
            }

            Stmt::Write { path, content, line, .. } => {
                self.current_line = *line;
                self.compile_expr(path, async_fns)?;
                self.compile_expr(content, async_fns)?;
                self.emit(Instruction::WriteFile("write".into()));
            }

            Stmt::Append { path, content, line, .. } => {
                self.current_line = *line;
                self.compile_expr(path, async_fns)?;
                self.compile_expr(content, async_fns)?;
                self.emit(Instruction::WriteFile("append".into()));
            }

            Stmt::Serve { port, routes, line, .. } => {
                self.current_line = *line;
                let fn_name = routes.first().and_then(|s| {
                    if let Stmt::Expr { expr: Expr::Ident(n), .. } = s { Some(n.clone()) } else { None }
                }).unwrap_or_else(|| "__serve_handler__".into());
                self.compile_expr(port, async_fns)?;
                self.emit(Instruction::ServeHTTP(fn_name));
            }

            Stmt::Shape { name, .. } => { self.emit(Instruction::DefineShape(name.clone())); }
            Stmt::Use { .. } | Stmt::ExternFn { .. } => {}

            // fn interna dentro de un cuerpo de función → closure accesible como variable local
            Stmt::Fn { name, params, body, .. } | Stmt::AsyncFn { name, params, body, .. } => {
                let fn_name = name.clone();
                let mut inner = FnCompiler::new();
                let last_is_expr = matches!(body.last(), Some(Stmt::Expr { .. }));
                let (main_body, last_stmt) = if last_is_expr && !body.is_empty() {
                    (&body[..body.len()-1], body.last())
                } else {
                    (&body[..], None)
                };
                for s in main_body { inner.compile_stmt(s, async_fns).ok(); }
                if let Some(Stmt::Expr { expr, .. }) = last_stmt {
                    inner.compile_expr(expr, async_fns).ok();
                } else {
                    inner.emit(Instruction::LoadNull);
                }
                inner.emit(Instruction::Return);
                self.pending_lambdas.push((fn_name.clone(), FunctionDef {
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    body: inner.instrs,
                    lines: inner.lines,
                    param_defaults: Vec::new(),
                }));
                self.pending_lambdas.extend(inner.pending_lambdas);
                // Crear closure y almacenarla como variable local con el nombre de la función
                self.emit(Instruction::MakeClosure(fn_name.clone()));
                self.emit(Instruction::StoreVar(fn_name));
            }

            Stmt::Expr { expr, line, .. } => {
                self.current_line = *line;
                self.compile_expr(expr, async_fns)?;
                self.emit(Instruction::Pop);
            }
        }
        Ok(())
    }

    fn compile_expr(&mut self, expr: &Expr, async_fns: &std::collections::HashSet<String>) -> Result<(), CodegenError> {
        let mut extra_fns: Vec<(String, FunctionDef)> = Vec::new();
        compile_expr_into(&mut self.instrs, &mut self.lines, self.current_line, async_fns, &mut extra_fns, expr)?;
        // Lambdas within function bodies are stored in the outer functions map.
        // We store them in a temporary list on `self` to be picked up by the caller.
        self.pending_lambdas.extend(extra_fns);
        Ok(())
    }
}

//    compile_expr_into — compartida entre Codegen y FnCompiler                  

fn compile_expr_into(
    instrs: &mut Vec<Instruction>,
    lines:  &mut Vec<u32>,
    current_line: u32,
    async_fns: &std::collections::HashSet<String>,
    extra_fns: &mut Vec<(String, FunctionDef)>,
    expr: &Expr,
) -> Result<(), CodegenError> {
    macro_rules! emit {
        ($i:expr) => {{ instrs.push($i); lines.push(current_line); }}
    }
    macro_rules! recurse {
        ($e:expr) => { compile_expr_into(instrs, lines, current_line, async_fns, extra_fns, $e)? }
    }

    match expr {
        Expr::Int(n)   => emit!(Instruction::LoadInt(*n)),
        Expr::Float(f) => emit!(Instruction::LoadFloat(*f)),
        Expr::Str(s)   => {
            // interpolación básica: si contiene ${ ... }
            if s.contains("${") {
                compile_interpolated(instrs, lines, current_line, async_fns, s)?;
            } else {
                emit!(Instruction::LoadStr(s.clone()));
            }
        }
        Expr::Bool(b)  => emit!(Instruction::LoadBool(*b)),
        Expr::Null      => emit!(Instruction::LoadNull),
        Expr::Undefined => emit!(Instruction::LoadNull),

        Expr::Ident(name) => {
            if name == "me" {
                emit!(Instruction::PushSelf)
            } else {
                emit!(Instruction::LoadVar(name.clone()))
            }
        }

        // Un rango como EXPRESIÓN se materializa en la lista `range(a, b)`.
        //
        // El `for` tiene su propio camino en `compile_stmt`, que itera con un
        // contador y no llega aquí: recorrer un rango no reserva la lista. Este
        // caso cubre todo lo demás (`x = 1..4`, `len(1..n)`, pasarlo a una
        // función), que hasta ahora caía al operador genérico y, con el
        // fallback a `Add`, devolvía la SUMA de los extremos en silencio.
        //
        // `range` excluye el extremo derecho —`range(1,4)` es `[1,2,3]`—, así
        // que el inclusivo se pide como `range(a, b+1)`. Está en
        // `is_jit_builtin`, con lo que este desugar vale igual en VM, JIT y AOT
        // sin tocar ninguno de los tres.
        Expr::BinaryOp { op, left, right } if op == ".." || op == "..<" || op == "..." => {
            recurse!(left);
            recurse!(right);
            if op == "..." {
                emit!(Instruction::LoadInt(1));
                emit!(Instruction::Add);
            }
            emit!(Instruction::Call("range".into(), 2));
        }

        Expr::BinaryOp { op, left, right } => {
            recurse!(left);
            recurse!(right);
            let instr = op_instr(op).ok_or_else(|| CodegenError {
                message: format!("operador binario no soportado: '{op}'"),
                line: current_line,
            })?;
            emit!(instr);
        }

        // `cond ? a : b` — solo se evalúa la rama que toca.
        //
        // Se baja a los mismos saltos que un if/else, con una diferencia que
        // importa: aquí las dos ramas dejan un valor en la pila y el bloque de
        // fusión lo tiene que encontrar. La VM no se inmuta, pero el JIT limpia
        // la pila en cada frontera de bloque, así que una función con ternario
        // no compila a nativo y cae al intérprete. Cae con error, no con un
        // resultado inventado, que es lo que importa; hacerlo nativo pediría
        // parámetros de bloque en el punto de fusión y eso es un análisis de
        // flujo que el JIT todavía no hace.
        Expr::Ternary { cond, then_e, else_e } => {
            recurse!(cond);
            let jf = instrs.len();
            emit!(Instruction::JumpIfFalse(0));

            recurse!(then_e);
            let jend = instrs.len();
            emit!(Instruction::Jump(0));

            let else_addr = instrs.len();
            instrs[jf] = Instruction::JumpIfFalse(else_addr);
            recurse!(else_e);

            let end = instrs.len();
            instrs[jend] = Instruction::Jump(end);
        }

        Expr::UnaryOp { op, expr } => {
            recurse!(expr);
            match op.as_str() {
                "!" => emit!(Instruction::Not),
                "-" => emit!(Instruction::Neg),
                _   => {}
            }
        }

        Expr::List(elems) if elems.iter().any(|e| matches!(e, Expr::Spread(_))) => {
            compile_list_with_spread(instrs, lines, current_line, async_fns, extra_fns, elems)?;
        }

        Expr::List(elems) => {
            let n = elems.len();
            for e in elems { recurse!(e); }
            emit!(Instruction::MakeList(n as u8));
        }

        // Un `...` que llega hasta aquí está fuera de una lista o de unos
        // argumentos, que son los dos únicos sitios donde expandir significa
        // algo. El parser ya no lo produce en otras posiciones; esto cubre el
        // caso por si alguna vez lo hiciera.
        Expr::Spread(_) => {
            return Err(CodegenError {
                message: "'...' solo se puede usar dentro de una lista o de los \
                          argumentos de una llamada".to_string(),
                line: current_line,
            });
        }

        Expr::Dict(items) => {
            let n = items.len();
            for (k, v) in items {
                emit!(Instruction::LoadStr(k.clone()));
                recurse!(v);
            }
            emit!(Instruction::MakeDict(n as u8));
        }

        Expr::Call { callee, args, kwargs } => {
            // Tras el pase de named_args, unos kwargs que sobreviven = no se
            // pudieron resolver (función desconocida o callee dinámico).
            if let Some((k, _)) = kwargs.first() {
                return Err(CodegenError {
                    message: format!(
                        "argumento con nombre '{} = ...' no soportado aquí; solo en funciones de usuario conocidas",
                        k
                    ),
                    line: current_line,
                });
            }
            // Argumentos con `...`: el número de argumentos deja de conocerse en
            // compilación, así que no se puede emitir `Call(f, n)`. Se construye
            // la lista completa —mezclando sueltos y expandidos— y se despacha
            // por `__apply__`, que en la VM saca los elementos de la lista y
            // llama con ellos. Un callee con nombre viaja como cadena, que es
            // una de las formas que `__call__` ya sabe resolver.
            if args.iter().any(|a| matches!(a, Expr::Spread(_))) {
                match callee.as_ref() {
                    Expr::Ident(fn_name) => emit!(Instruction::LoadStr(fn_name.clone())),
                    other => recurse!(other),
                }
                compile_list_with_spread(instrs, lines, current_line, async_fns, extra_fns, args)?;
                emit!(Instruction::Call("__apply__".into(), 2));
                return Ok(());
            }

            if let Expr::Ident(fn_name) = callee.as_ref() {
                if fn_name == "show" {
                    for a in args { recurse!(a); }
                    emit!(Instruction::Show);
                    return Ok(());
                }
                for a in args { recurse!(a); }
                if async_fns.contains(fn_name) {
                    emit!(Instruction::CallAsync(fn_name.clone(), args.len() as u8));
                } else {
                    emit!(Instruction::Call(fn_name.clone(), args.len() as u8));
                }
            } else {
                // expresión callable genérica
                recurse!(callee);
                for a in args { recurse!(a); }
                emit!(Instruction::Call("__call__".into(), args.len() as u8));
            }
        }

        Expr::CallMethod { method, receiver, args, kwargs } => {
            if let Some((k, _)) = kwargs.first() {
                return Err(CodegenError {
                    message: format!(
                        "argumento con nombre '{} = ...' no soportado en métodos; usa posicionales",
                        k
                    ),
                    line: current_line,
                });
            }
            // super.metodo(args) → CallSuper (resuelve en el shape padre sobre self)
            if matches!(receiver.as_ref(), Expr::Ident(n) if n == "super") {
                for a in args { recurse!(a); }
                emit!(Instruction::CallSuper(method.clone(), args.len() as u8));
            } else {
                recurse!(receiver);
                for a in args { recurse!(a); }
                emit!(Instruction::CallMethod(method.clone(), args.len() as u8));
            }
        }

        Expr::AttrAccess { object, attr } => {
            recurse!(object);
            emit!(Instruction::GetAttr(attr.clone()));
        }

        Expr::Index { object, index } => {
            recurse!(object);
            recurse!(index);
            emit!(Instruction::GetIndex);
        }

        Expr::SliceAccess { object, start, end } => {
            recurse!(object);
            if let Some(s) = start { recurse!(s); } else { emit!(Instruction::LoadNull); }
            if let Some(e) = end   { recurse!(e); } else { emit!(Instruction::LoadNull); }
            emit!(Instruction::Call("slice".into(), 3));
        }

        Expr::NullSafe { object, attr } => {
            recurse!(object);
            emit!(Instruction::GetAttr(attr.clone())); // simplificado: sin null-check
        }

        Expr::IsCheck { expr, shape } => {
            recurse!(expr);
            emit!(Instruction::IsInstance(shape.clone()));
        }

        Expr::Await(inner) => {
            recurse!(inner);
            emit!(Instruction::Await);
        }

        Expr::Lambda { params, body } => {
            // Lambda → closure con nombre único global + captura del scope actual
            let name = format!("__lambda_{}__", LAMBDA_COUNTER.fetch_add(1, Ordering::Relaxed));
            let mut fc = FnCompiler::new();
            // Si el cuerpo es una sola expr-stmt, tratarla como return implícito
            let last_is_expr = matches!(body.last(), Some(Stmt::Expr { .. }));
            let (main_body, last_stmt) = if last_is_expr && !body.is_empty() {
                (&body[..body.len()-1], body.last())
            } else {
                (&body[..], None)
            };
            for stmt in main_body {
                fc.compile_stmt(stmt, async_fns).ok();
            }
            if let Some(Stmt::Expr { expr, .. }) = last_stmt {
                let mut extra2: Vec<(String, FunctionDef)> = Vec::new();
                compile_expr_into(&mut fc.instrs, &mut fc.lines, fc.current_line, async_fns, &mut extra2, expr).ok();
                fc.pending_lambdas.extend(extra2);
            } else {
                fc.emit(Instruction::LoadNull);
            }
            fc.emit(Instruction::Return);
            extra_fns.push((name.clone(), FunctionDef {
                params: params.clone(),
                body: fc.instrs,
                lines: fc.lines,
                param_defaults: Vec::new(),
            }));
            extra_fns.extend(fc.pending_lambdas);
            // MakeClosure captura el scope actual en tiempo de ejecución
            emit!(Instruction::MakeClosure(name));
        }
    }
    Ok(())
}

//    String interpolation                                                       

fn compile_interpolated(
    instrs: &mut Vec<Instruction>,
    lines:  &mut Vec<u32>,
    current_line: u32,
    async_fns: &std::collections::HashSet<String>,
    s: &str,
) -> Result<(), CodegenError> {
    // Parsear partes: texto literal y ${expr}
    // El extractor es consciente de strings anidados dentro de ${ } para
    // soportar expresiones como ${csv.column(data, "col")}.
    let mut parts: Vec<(bool, String)> = Vec::new(); // (is_expr, content)
    let mut i = 0;
    let chars: Vec<char> = s.chars().collect();
    let mut cur = String::new();

    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
            if !cur.is_empty() { parts.push((false, cur.clone())); cur.clear(); }
            i += 2;
            let mut depth = 1usize;
            let mut expr = String::new();
            while i < chars.len() && depth > 0 {
                let ch = chars[i];
                if ch == '"' {
                    // string anidado — capturarlo completo incluyendo las comillas
                    expr.push(ch);
                    i += 1;
                    while i < chars.len() {
                        let ic = chars[i];
                        expr.push(ic);
                        i += 1;
                        if ic == '"' { break; }
                    }
                    continue;
                }
                if ch == '{' { depth += 1; }
                else if ch == '}' {
                    depth -= 1;
                    if depth == 0 { i += 1; break; }
                }
                expr.push(ch);
                i += 1;
            }
            parts.push((true, expr));
        } else {
            cur.push(chars[i]);
            i += 1;
        }
    }
    if !cur.is_empty() { parts.push((false, cur)); }

    if parts.is_empty() {
        instrs.push(Instruction::LoadStr(String::new()));
        lines.push(current_line);
        return Ok(());
    }

    // Arrancamos con un string vacío y concatenamos cada parte con Add.
    // Add coacciona a string cuando un operando es string, así que el resultado
    // SIEMPRE es String — incluso con una sola interpolación ("${n}" → "5", no Int)
    // o con interpolaciones adyacentes ("${a}${b}" → concat, no suma numérica).
    instrs.push(Instruction::LoadStr(String::new()));
    lines.push(current_line);
    for (is_expr, content) in &parts {
        if *is_expr {
            compile_sub_expr(instrs, lines, current_line, async_fns, content)?;
        } else {
            instrs.push(Instruction::LoadStr(content.clone()));
            lines.push(current_line);
        }
        instrs.push(Instruction::Add);
        lines.push(current_line);
    }
    Ok(())
}

fn compile_sub_expr(
    instrs: &mut Vec<Instruction>,
    lines:  &mut Vec<u32>,
    current_line: u32,
    async_fns: &std::collections::HashSet<String>,
    src: &str,
) -> Result<(), CodegenError> {
    use crate::lexer::lex;
    use crate::parser::parse;

    let tokens = lex(src).map_err(|e| CodegenError { message: e.message, line: current_line })?;
    let stmts  = parse(tokens).map_err(|e| CodegenError { message: e.message, line: e.line })?;
    if let Some(Stmt::Expr { expr, .. }) = stmts.into_iter().next() {
        let mut extra_fns: Vec<(String, FunctionDef)> = Vec::new();
        compile_expr_into(instrs, lines, current_line, async_fns, &mut extra_fns, &expr)?;
        // Note: lambdas in string interpolation are dropped (edge case, uncommon)
    }
    Ok(())
}

//    Helpers

/// Construye en la pila una lista con `...` expandidos, concatenando trozos.
///
/// `[a, ...b, c]` se compila como `[] + [a] + b + [c]`. Los elementos sueltos
/// consecutivos se agrupan en un solo `MakeList`, así que solo hay una suma por
/// expansión, no una por elemento.
///
/// Empieza por una lista vacía a propósito: garantiza que el resultado sea
/// SIEMPRE nuevo. Sin eso, `x = [...b]` compilaría al propio `b`, y como las
/// listas de Orion van por referencia, escribir en `x` habría escrito en `b`.
fn compile_list_with_spread(
    instrs: &mut Vec<Instruction>,
    lines:  &mut Vec<u32>,
    current_line: u32,
    async_fns: &std::collections::HashSet<String>,
    extra_fns: &mut Vec<(String, FunctionDef)>,
    elems: &[Expr],
) -> Result<(), CodegenError> {
    instrs.push(Instruction::MakeList(0));
    lines.push(current_line);

    // Se recorre agrupando: cada racha de elementos normales produce un
    // MakeList(n) seguido de Add; cada `...` produce su lista y un Add.
    let mut pending: Vec<&Expr> = Vec::new();
    let mut emit_pending = |instrs: &mut Vec<Instruction>,
                            lines: &mut Vec<u32>,
                            extra_fns: &mut Vec<(String, FunctionDef)>,
                            pending: &mut Vec<&Expr>|
     -> Result<(), CodegenError> {
        if pending.is_empty() { return Ok(()); }
        let n = pending.len();
        for e in pending.drain(..) {
            compile_expr_into(instrs, lines, current_line, async_fns, extra_fns, e)?;
        }
        instrs.push(Instruction::MakeList(n as u8));
        lines.push(current_line);
        instrs.push(Instruction::Add);
        lines.push(current_line);
        Ok(())
    };

    for e in elems {
        match e {
            Expr::Spread(inner) => {
                emit_pending(instrs, lines, extra_fns, &mut pending)?;
                compile_expr_into(instrs, lines, current_line, async_fns, extra_fns, inner)?;
                instrs.push(Instruction::Add);
                lines.push(current_line);
            }
            other => pending.push(other),
        }
    }
    emit_pending(instrs, lines, extra_fns, &mut pending)?;
    Ok(())
}

/// Instrucción que implementa un operador binario.
///
/// Devuelve `None` para lo que no sea un operador aritmético/lógico. Antes esta
/// función caía a `Add` para todo lo desconocido, y el efecto no era un error
/// sino una respuesta equivocada en silencio: `1..4` fuera de un `for` no se
/// quejaba, devolvía `5`. Cualquier operador que se añada al lexer y no se
/// cablee aquí vuelve a sumar, así que el caso por defecto tiene que ser un
/// error del compilador, no una suma.
fn op_instr(op: &str) -> Option<Instruction> {
    Some(match op {
        "+"  => Instruction::Add,
        "-"  => Instruction::Sub,
        "*"  => Instruction::Mul,
        "/"  => Instruction::Div,
        "%"  => Instruction::Mod,
        "**" => Instruction::Pow,
        "==" => Instruction::Eq,
        "!=" => Instruction::NotEq,
        "<"  => Instruction::Lt,
        "<=" => Instruction::LtEq,
        ">"  => Instruction::Gt,
        ">=" => Instruction::GtEq,
        "&&" => Instruction::And,
        "||" => Instruction::Or,
        "&"  => Instruction::BitAnd,
        "|"  => Instruction::BitOr,
        "^"  => Instruction::BitXor,
        "<<" => Instruction::Shl,
        ">>" => Instruction::Shr,
        _    => return None,
    })
}

//    Tests                                                                      

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn cg(src: &str) -> OrionBytecode {
        let tokens = lex(src).expect("lex");
        let stmts  = parse(tokens).expect("parse");
        compile(stmts).expect("codegen")
    }

    #[test]
    fn test_assign_literal() {
        let bc = cg("x = 42");
        assert!(bc.main.contains(&Instruction::LoadInt(42)));
        assert!(bc.main.contains(&Instruction::StoreVar("x".into())));
    }

    #[test]
    fn test_show() {
        let bc = cg(r#"show "hola""#);
        assert!(bc.main.contains(&Instruction::LoadStr("hola".into())));
        assert!(bc.main.contains(&Instruction::Show));
    }

    #[test]
    fn test_fn_registered() {
        let bc = cg("fn doble(n) { return n * 2 }");
        assert!(bc.functions.contains_key("doble"));
        assert_eq!(bc.functions["doble"].params, vec!["n"]);
    }

    #[test]
    fn test_if_jumps() {
        let bc = cg("if x > 0 { show x }");
        let has_jif = bc.main.iter().any(|i| matches!(i, Instruction::JumpIfFalse(_)));
        assert!(has_jif);
    }

    #[test]
    fn test_for_loop() {
        let bc = cg("for i in lista { show i }");
        assert!(bc.main.contains(&Instruction::Call("len".into(), 1)));
        assert!(bc.main.contains(&Instruction::GetIndex));
    }

    #[test]
    fn test_attempt_handle() {
        let bc = cg("attempt { x = 1 } handle err { show err }");
        let has_begin = bc.main.iter().any(|i| matches!(i, Instruction::BeginAttempt(_)));
        let has_end   = bc.main.iter().any(|i| matches!(i, Instruction::EndAttempt(_)));
        assert!(has_begin && has_end);
    }

    #[test]
    fn test_with_desugar_emite_attempt_y_dos_free() {
        // with = assign + attempt(+free en el handler) + free normal:
        // debe haber BeginAttempt, Raise (re-lanzado) y DOS CallMethod("free").
        let bc = cg("with f = frame.open(\"d.csv\") { k = frame.count(f) }");
        let frees = bc.main.iter().filter(|i| {
            matches!(i, Instruction::CallMethod(m, 1) if m == "free")
        }).count();
        assert_eq!(frees, 2, "un free en el handler y otro en el camino normal");
        assert!(bc.main.iter().any(|i| matches!(i, Instruction::BeginAttempt(_))));
        assert!(bc.main.contains(&Instruction::Raise));
    }

    #[test]
    fn test_think() {
        let bc = cg(r#"think "pregunta""#);
        assert!(bc.main.contains(&Instruction::AiAsk));
        assert!(bc.main.contains(&Instruction::Show));
    }

    #[test]
    fn test_halt() {
        let bc = cg("x = 1");
        assert_eq!(*bc.main.last().unwrap(), Instruction::Halt);
    }
}
