use std::collections::HashSet;
use indexmap::IndexMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use crate::instruction::Instruction;
use crate::value::{Value, InstanceData, SendValue, from_send};
use crate::bytecode::{FunctionDef, ShapeDef};

struct CallFrame {
    instructions: Vec<Instruction>,
    lines: Vec<u32>,
    ip: usize,
    vars: IndexMap<String, Value>,
    consts: HashSet<String>,
    /// Si es un frame de act/on_create, referencia a la instancia actual
    self_instance: Option<Rc<RefCell<InstanceData>>>,
    /// Nombres de los campos de la instancia (para sincronizar al salir del frame)
    instance_fields: Vec<String>,
    /// Nombre del contexto de ejecución (función, act, etc.)
    name: String,
}

impl CallFrame {
    fn new(instructions: Vec<Instruction>, lines: Vec<u32>) -> Self {
        CallFrame {
            instructions, lines, ip: 0,
            vars: IndexMap::new(),
            consts: HashSet::new(),
            self_instance: None,
            instance_fields: Vec::new(),
            name: String::from("<main>"),
        }
    }

    fn named(instructions: Vec<Instruction>, lines: Vec<u32>, name: &str) -> Self {
        let mut frame = Self::new(instructions, lines);
        frame.name = name.to_string();
        frame
    }

    fn with_args(instructions: Vec<Instruction>, lines: Vec<u32>, params: &[String], args: Vec<Value>) -> Self {
        let mut frame = Self::new(instructions, lines);
        for (param, val) in params.iter().zip(args.into_iter()) {
            frame.vars.insert(param.clone(), val);
        }
        frame
    }

    fn with_args_named(instructions: Vec<Instruction>, lines: Vec<u32>, name: &str, params: &[String], args: Vec<Value>) -> Self {
        let mut frame = Self::with_args(instructions, lines, params, args);
        frame.name = name.to_string();
        frame
    }

    fn current_line(&self) -> u32 {
        self.lines.get(self.ip.saturating_sub(1)).copied().unwrap_or(0)
    }

    fn sync_to_instance(&self) {
        if let Some(inst_rc) = &self.self_instance {
            let mut inst = inst_rc.borrow_mut();
            for field_name in &self.instance_fields {
                if let Some(val) = self.vars.get(field_name) {
                    inst.fields.insert(field_name.clone(), val.clone());
                }
            }
        }
    }
}

struct ErrorHandler {
    handler_addr: usize,
    frame_depth: usize,
}

pub struct VM {
    value_stack: Vec<Value>,
    call_stack: Vec<CallFrame>,
    functions: IndexMap<String, FunctionDef>,
    shapes: IndexMap<String, ShapeDef>,
    current_line: u32,
    error_handlers: Vec<ErrorHandler>,
    /// Memoria de sesión para instrucciones AiLearn / AiSense
    ai_memory: Vec<String>,
}

impl VM {
    pub fn new(
        main: Vec<Instruction>,
        main_lines: Vec<u32>,
        functions: IndexMap<String, FunctionDef>,
        shapes: IndexMap<String, ShapeDef>,
    ) -> Self {
        VM {
            value_stack: Vec::new(),
            call_stack: vec![CallFrame::new(main, main_lines)],
            functions,
            shapes,
            current_line: 0,
            error_handlers: Vec::new(),
            ai_memory: Vec::new(),
        }
    }

    /// Construye una cadena de stack trace con todos los frames activos.
    pub fn stack_trace(&self) -> String {
        let frames: Vec<String> = self.call_stack.iter().rev().map(|f| {
            let line = f.current_line();
            if line > 0 {
                format!("    en {} (linea {})", f.name, line)
            } else {
                format!("    en {}", f.name)
            }
        }).collect();
        frames.join("\n")
    }

    pub fn run(&mut self) -> Result<(), String> {
        let run_result: Result<(), String> = loop {
            match self.step() {
                Ok(true) => break Ok(()),
                Ok(false) => {}
                Err(e) => break Err(e),
            }
        };
        run_result.map_err(|e| {
            let trace = self.stack_trace();
            let line_info = if self.current_line > 0 {
                format!("Linea {} | ", self.current_line)
            } else {
                String::new()
            };
            if trace.is_empty() {
                format!("{}{}", line_info, e)
            } else {
                format!("{}{}\n{}", line_info, e, trace)
            }
        })
    }

    /// Ejecuta sin formatear errores — usar en subtareas async para evitar doble-prefijo
    pub fn run_raw(&mut self) -> Result<(), String> {
        loop {
            let done = self.step()?;
            if done { break; }
        }
        Ok(())
    }

    /// Ejecuta un solo ciclo del loop principal. Retorna Ok(true) si el programa terminó.
    fn step(&mut self) -> Result<bool, String> {
        // Fin de frame
        {
            let frame = match self.call_stack.last_mut() {
                Some(f) => f,
                None => return Ok(true),
            };
            if frame.ip >= frame.instructions.len() {
                let frame = self.call_stack.pop().unwrap();
                frame.sync_to_instance();
                return Ok(false);
            }
        }

        let instr = {
            let frame = self.call_stack.last_mut().unwrap();
            let line = frame.lines.get(frame.ip).copied().unwrap_or(0);
            let instr = frame.instructions[frame.ip].clone();
            frame.ip += 1;
            if line > 0 { self.current_line = line; }
            instr
        };

        match self.dispatch_instr(instr) {
            Ok(done) => Ok(done),
            Err(e) => {
                if let Some(handler) = self.error_handlers.pop() {
                    while self.call_stack.len() > handler.frame_depth {
                        let f = self.call_stack.pop().unwrap();
                        f.sync_to_instance();
                    }
                    self.value_stack.push(Value::Str(e));
                    let frame = self.call_stack.last_mut()
                        .ok_or("Sin frame activo para handle")?;
                    frame.ip = handler.handler_addr;
                    Ok(false)
                } else {
                    Err(e)
                }
            }
        }
    }


    /// Ejecuta una sola instrucción. Devuelve Ok(true) para Halt/Return-en-main.
    fn dispatch_instr(&mut self, instr: Instruction) -> Result<bool, String> {
        match instr {
            // ── Constantes ──────────────────────────────────────────────────
            Instruction::LoadInt(n)   => self.value_stack.push(Value::Int(n)),
            Instruction::LoadFloat(f) => self.value_stack.push(Value::Float(f)),
            Instruction::LoadStr(s)   => self.value_stack.push(Value::Str(s)),
            Instruction::LoadBool(b)  => self.value_stack.push(Value::Bool(b)),
            Instruction::LoadNull     => self.value_stack.push(Value::Null),

            // ── Variables ───────────────────────────────────────────────────
            Instruction::LoadVar(name) => {
                // 1. Frame local
                let val = self.call_stack.last()
                    .and_then(|f| f.vars.get(&name).cloned());
                // 2. Main (global) frame
                let val = val.or_else(|| {
                    if self.call_stack.len() > 1 {
                        self.call_stack.first().and_then(|f| f.vars.get(&name).cloned())
                    } else { None }
                });
                // 3. Nombre de función registrada → push Str(name)
                let val = val.or_else(|| {
                    if self.functions.contains_key(&name) {
                        Some(Value::Str(name.clone()))
                    } else {
                        None
                    }
                });
                // 4. Nombre de shape registrado → push Str(name)
                let val = val.or_else(|| {
                    if self.shapes.contains_key(&name) {
                        Some(Value::Str(name.clone()))
                    } else {
                        None
                    }
                });
                let val = val.ok_or_else(|| format!("Variable '{}' no definida", name))?;
                self.value_stack.push(val);
            }
            Instruction::StoreVar(name) => {
                let val = self.pop()?;
                let frame = self.call_stack.last_mut().ok_or("Sin frame activo")?;
                if frame.consts.contains(&name) {
                    return Err(format!("No se puede reasignar '{}': es una constante", name));
                }
                frame.vars.insert(name, val);
            }
            Instruction::StoreConst(name) => {
                let val = self.pop()?;
                let frame = self.call_stack.last_mut().ok_or("Sin frame activo")?;
                frame.consts.insert(name.clone());
                frame.vars.insert(name, val);
            }

            // ── Aritmética ──────────────────────────────────────────────────
            Instruction::Add => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(a.add(&b)?); }
            Instruction::Sub => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(a.sub(&b)?); }
            Instruction::Mul => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(a.mul(&b)?); }
            Instruction::Div => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(a.div(&b)?); }
            Instruction::Mod => {
                let b = self.pop()?; let a = self.pop()?;
                match (a, b) {
                    (Value::Int(x), Value::Int(y)) => self.value_stack.push(Value::Int(x % y)),
                    _ => return Err("Módulo solo soporta enteros".to_string()),
                }
            }
            Instruction::Pow => {
                let b = self.pop()?; let a = self.pop()?;
                match (a, b) {
                    (Value::Int(x), Value::Int(y))     => self.value_stack.push(Value::Int(x.pow(y as u32))),
                    (Value::Float(x), Value::Float(y)) => self.value_stack.push(Value::Float(x.powf(y))),
                    (Value::Int(x), Value::Float(y))   => self.value_stack.push(Value::Float((x as f64).powf(y))),
                    _ => return Err("Potencia requiere números".to_string()),
                }
            }
            Instruction::Neg => {
                let a = self.pop()?;
                match a {
                    Value::Int(n)   => self.value_stack.push(Value::Int(-n)),
                    Value::Float(f) => self.value_stack.push(Value::Float(-f)),
                    _ => return Err("Negación solo aplica a números".to_string()),
                }
            }

            // ── Comparación ─────────────────────────────────────────────────
            Instruction::Eq    => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(a.compare_eq(&b))); }
            Instruction::NotEq => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(!a.compare_eq(&b))); }
            Instruction::Lt    => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(a.compare_lt(&b)?)); }
            Instruction::LtEq  => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(a.compare_lt(&b)? || a.compare_eq(&b))); }
            Instruction::Gt    => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(!a.compare_lt(&b)? && !a.compare_eq(&b))); }
            Instruction::GtEq  => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(!a.compare_lt(&b)?)); }

            // ── Lógica ──────────────────────────────────────────────────────
            Instruction::And => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(a.is_truthy() && b.is_truthy())); }
            Instruction::Or  => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(a.is_truthy() || b.is_truthy())); }
            Instruction::Not => { let a = self.pop()?; self.value_stack.push(Value::Bool(!a.is_truthy())); }

            // ── Control de flujo ────────────────────────────────────────────
            Instruction::Jump(addr) => {
                self.call_stack.last_mut().ok_or("Sin frame activo")?.ip = addr;
            }
            Instruction::JumpIfFalse(addr) => {
                let cond = self.pop()?;
                if !cond.is_truthy() {
                    self.call_stack.last_mut().ok_or("Sin frame activo")?.ip = addr;
                }
            }
            Instruction::JumpIfTrue(addr) => {
                let cond = self.pop()?;
                if cond.is_truthy() {
                    self.call_stack.last_mut().ok_or("Sin frame activo")?.ip = addr;
                }
            }

            // ── Manejo de errores ───────────────────────────────────────────
            Instruction::BeginAttempt(handler_addr) => {
                self.error_handlers.push(ErrorHandler {
                    handler_addr,
                    frame_depth: self.call_stack.len(),
                });
            }
            Instruction::EndAttempt(end_addr) => {
                // Attempt completado sin error — quitar handler y saltar al fin
                self.error_handlers.pop();
                self.call_stack.last_mut().ok_or("Sin frame activo")?.ip = end_addr;
            }
            Instruction::Raise => {
                // error "mensaje"  →  lanza error explícito (igual que runtime error)
                let msg = self.pop()?;
                return Err(msg.to_string());
            }

            // ── Funciones ───────────────────────────────────────────────────
            Instruction::Call(name, argc) => {
                let mut args: Vec<Value> = (0..argc)
                    .map(|_| self.pop())
                    .collect::<Result<Vec<_>, _>>()?;
                args.reverse();

                // Resolver el valor real: variable local que puede ser Str, Closure o directo
                let local_val = self.call_stack.last()
                    .and_then(|f| f.vars.get(&name).cloned())
                    .or_else(|| {
                        if self.call_stack.len() > 1 {
                            self.call_stack.first().and_then(|f| f.vars.get(&name).cloned())
                        } else { None }
                    });

                // Extraer nombre resuelto y env de closure (si aplica)
                let (resolved_name, closure_env) = match local_val {
                    Some(Value::Closure { fn_name, env }) => (fn_name, Some(env)),
                    Some(Value::Str(s))                   => (s, None),
                    _                                     => (name.clone(), None),
                };

                if self.shapes.contains_key(&resolved_name) {
                    let inst_rc = self.instantiate_shape(&resolved_name, args)?;
                    self.value_stack.push(Value::Instance(inst_rc));
                } else if let Some(func) = self.functions.get(&resolved_name).cloned() {
                    if args.len() != func.params.len() {
                        return Err(format!(
                            "'{}' espera {} argumento(s), recibió {}",
                            resolved_name, func.params.len(), args.len()
                        ));
                    }
                    let mut frame = CallFrame::with_args_named(
                        func.body, func.lines, &resolved_name, &func.params, args
                    );
                    // Inyectar env capturado (los params tienen prioridad)
                    if let Some(env) = closure_env {
                        for (k, v) in env {
                            frame.vars.entry(k).or_insert(v);
                        }
                    }
                    self.call_stack.push(frame);
                } else {
                    let result = self.call_builtin(&resolved_name, args)?;
                    if let Some(val) = result {
                        self.value_stack.push(val);
                    }
                }
            }
            Instruction::Return => {
                let frame = self.call_stack.pop().ok_or("Return sin frame")?;
                frame.sync_to_instance();
                if self.call_stack.is_empty() {
                    return Ok(true);
                }
            }
            Instruction::Halt => return Ok(true),

            // ── Módulos ──────────────────────────────────────────────────────
            Instruction::UseModule(path) => {
                let module_val = self.load_module(&path)?;
                // Determinar nombre del namespace: basename sin extensión
                let ns_name = std::path::Path::new(&path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&path)
                    .to_string();
                let frame = self.call_stack.last_mut().unwrap();
                frame.vars.insert(ns_name, module_val);
            }

            // ── OOP ─────────────────────────────────────────────────────────
            Instruction::DefineShape(_) => {}

            Instruction::GetAttr(attr) => {
                let obj = self.pop()?;
                match obj {
                    Value::Instance(inst_rc) => {
                        let inst = inst_rc.borrow();
                        let val = inst.fields.get(&attr).cloned()
                            .ok_or_else(|| format!("Atributo '{}' no encontrado en '{}'", attr, inst.shape_name))?;
                        self.value_stack.push(val);
                    }
                    Value::Dict(map) => {
                        let val = map.get(&attr).cloned()
                            .ok_or_else(|| format!("Atributo '{}' no encontrado en dict/módulo", attr))?;
                        self.value_stack.push(val);
                    }
                    _ => return Err(format!("GetAttr '{}': no es una instancia ni módulo", attr)),
                }
            }
            Instruction::SetAttr(attr) => {
                let val = self.pop()?;
                let obj = self.pop()?;
                match obj {
                    Value::Instance(inst_rc) => {
                        inst_rc.borrow_mut().fields.insert(attr.clone(), val.clone());
                        // Si es la instancia actual del frame, sincronizar también la var local
                        // para que sync_to_instance no sobreescriba al retornar el método.
                        if let Some(frame) = self.call_stack.last_mut() {
                            if frame.self_instance.as_ref()
                                .map(|r| Rc::ptr_eq(r, &inst_rc))
                                .unwrap_or(false)
                            {
                                frame.vars.insert(attr, val);
                            }
                        }
                    }
                    _ => return Err(format!("SetAttr '{}': no es una instancia", attr)),
                }
            }
            Instruction::PushSelf => {
                let frame = self.call_stack.last()
                    .ok_or("PushSelf: sin frame activo")?;
                match &frame.self_instance {
                    Some(inst_rc) => self.value_stack.push(Value::Instance(Rc::clone(inst_rc))),
                    None          => self.value_stack.push(Value::Null),
                }
            }

            Instruction::MakeClosure(fn_name) => {
                // Captura una copia del scope actual como entorno de la closure
                let env = self.call_stack.last()
                    .map(|f| f.vars.clone())
                    .unwrap_or_default();
                self.value_stack.push(Value::Closure { fn_name, env });
            }
            Instruction::IsInstance(shape_name) => {
                let obj = self.pop()?;
                let result = match &obj {
                    Value::Instance(inst_rc) => {
                        let actual = inst_rc.borrow().shape_name.clone();
                        actual == shape_name || self.shape_uses(&actual, &shape_name)
                    }
                    _ => false,
                };
                self.value_stack.push(Value::Bool(result));
            }
            Instruction::CallMethod(method_name, argc) => {
                let mut args: Vec<Value> = (0..argc)
                    .map(|_| self.pop())
                    .collect::<Result<Vec<_>, _>>()?;
                args.reverse();

                let obj = self.pop()?;
                match obj {
                    // ── Métodos de String ────────────────────────────────
                    Value::Str(s) => {
                        let result = match method_name.as_str() {
                            "trim"        => Value::Str(s.trim().to_string()),
                            "trim_start"  => Value::Str(s.trim_start().to_string()),
                            "trim_end"    => Value::Str(s.trim_end().to_string()),
                            "lower"       => Value::Str(s.to_lowercase()),
                            "upper"       => Value::Str(s.to_uppercase()),
                            "len"         => Value::Int(s.chars().count() as i64),
                            "is_empty"    => Value::Bool(s.trim().is_empty()),
                            "reverse"     => Value::Str(s.chars().rev().collect()),
                            "contains" => {
                                let needle = args.into_iter().next()
                                    .ok_or("string.contains() requiere 1 argumento")?;
                                Value::Bool(s.contains(needle.to_string().as_str()))
                            }
                            "starts_with" => {
                                let prefix = args.into_iter().next()
                                    .ok_or("string.starts_with() requiere 1 argumento")?;
                                Value::Bool(s.starts_with(prefix.to_string().as_str()))
                            }
                            "ends_with" => {
                                let suffix = args.into_iter().next()
                                    .ok_or("string.ends_with() requiere 1 argumento")?;
                                Value::Bool(s.ends_with(suffix.to_string().as_str()))
                            }
                            "split" => {
                                let sep = args.into_iter().next()
                                    .ok_or("string.split() requiere 1 argumento")?;
                                let parts: Vec<Value> = s.split(sep.to_string().as_str())
                                    .map(|p| Value::Str(p.to_string()))
                                    .collect();
                                Value::List(parts)
                            }
                            "replace" => {
                                let mut it = args.into_iter();
                                let from = it.next().ok_or("string.replace() requiere 2 argumentos")?;
                                let to   = it.next().ok_or("string.replace() requiere 2 argumentos")?;
                                Value::Str(s.replace(from.to_string().as_str(), &to.to_string()))
                            }
                            "index_of" | "find" => {
                                let needle = args.into_iter().next()
                                    .ok_or("string.find() requiere 1 argumento")?;
                                match s.find(needle.to_string().as_str()) {
                                    Some(i) => Value::Int(i as i64),
                                    None    => Value::Int(-1),
                                }
                            }
                            "slice" => {
                                let mut it = args.into_iter();
                                let start = match it.next() {
                                    Some(Value::Int(n)) => n as usize,
                                    _ => return Err("string.slice() requiere índice int".to_string()),
                                };
                                let end = match it.next() {
                                    Some(Value::Int(n)) => n as usize,
                                    None => s.chars().count(),
                                    _ => return Err("string.slice() índice inválido".to_string()),
                                };
                                let sliced: String = s.chars().skip(start).take(end - start).collect();
                                Value::Str(sliced)
                            }
                            "repeat" => {
                                let n = match args.into_iter().next() {
                                    Some(Value::Int(n)) => n as usize,
                                    _ => return Err("string.repeat() requiere un int".to_string()),
                                };
                                Value::Str(s.repeat(n))
                            }
                            "to_int" | "parse_int" => {
                                match s.trim().parse::<i64>() {
                                    Ok(n) => Value::Int(n),
                                    Err(_) => return Err(format!("No se puede convertir '{}' a int", s)),
                                }
                            }
                            "to_float" | "parse_float" => {
                                match s.trim().parse::<f64>() {
                                    Ok(n) => Value::Float(n),
                                    Err(_) => return Err(format!("No se puede convertir '{}' a float", s)),
                                }
                            }
                            _ => return Err(format!("String no tiene método '{}'", method_name)),
                        };
                        self.value_stack.push(result);
                    }

                    // ── Métodos de List ──────────────────────────────────
                    Value::List(mut list) => {
                        let result = match method_name.as_str() {
                            "len"      => Value::Int(list.len() as i64),
                            "is_empty" => Value::Bool(list.is_empty()),
                            "push" | "append" => {
                                let item = args.into_iter().next()
                                    .ok_or("list.push() requiere 1 argumento")?;
                                list.push(item);
                                Value::List(list)
                            }
                            "first" => list.first().cloned().unwrap_or(Value::Null),
                            "last"  => list.last().cloned().unwrap_or(Value::Null),
                            "reverse" => { list.reverse(); Value::List(list) }
                            "contains" => {
                                let item = args.into_iter().next()
                                    .ok_or("list.contains() requiere 1 argumento")?;
                                Value::Bool(list.contains(&item))
                            }
                            "join" => {
                                let sep = match args.into_iter().next() {
                                    Some(Value::Str(s)) => s,
                                    None => String::new(),
                                    Some(v) => v.to_string(),
                                };
                                Value::Str(list.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(&sep))
                            }
                            "map" => {
                                let cb = args.into_iter().next()
                                    .ok_or("list.map() requiere una función/lambda")?;
                                let mut out = Vec::with_capacity(list.len());
                                for item in list {
                                    let r = self.call_value(cb.clone(), vec![item])?;
                                    out.push(r);
                                }
                                Value::List(out)
                            }
                            "filter" => {
                                let cb = args.into_iter().next()
                                    .ok_or("list.filter() requiere una función/lambda")?;
                                let mut out = Vec::new();
                                for item in list {
                                    let r = self.call_value(cb.clone(), vec![item.clone()])?;
                                    if r.is_truthy() { out.push(item); }
                                }
                                Value::List(out)
                            }
                            "reduce" => {
                                let mut it = args.into_iter();
                                let cb  = it.next().ok_or("list.reduce() requiere función y acumulador")?;
                                let acc = it.next().ok_or("list.reduce() requiere acumulador inicial")?;
                                let mut acc = acc;
                                for item in list {
                                    acc = self.call_value(cb.clone(), vec![acc, item])?;
                                }
                                acc
                            }
                            "sort" => {
                                list.sort_by(|a, b| match (a, b) {
                                    (Value::Int(x), Value::Int(y))     => x.cmp(y),
                                    (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
                                    (Value::Str(x), Value::Str(y))     => x.cmp(y),
                                    _ => std::cmp::Ordering::Equal,
                                });
                                Value::List(list)
                            }
                            "sum" => {
                                let mut total = 0.0f64;
                                let mut is_int = true;
                                for v in &list {
                                    match v {
                                        Value::Int(n)   => total += *n as f64,
                                        Value::Float(n) => { total += n; is_int = false; }
                                        _ => {}
                                    }
                                }
                                if is_int { Value::Int(total as i64) } else { Value::Float(total) }
                            }
                            "min" => list.iter().cloned().reduce(|a, b| match (&a, &b) {
                                (Value::Int(x), Value::Int(y))     => if x <= y { a } else { b },
                                (Value::Float(x), Value::Float(y)) => if x <= y { a } else { b },
                                _ => a,
                            }).unwrap_or(Value::Null),
                            "max" => list.iter().cloned().reduce(|a, b| match (&a, &b) {
                                (Value::Int(x), Value::Int(y))     => if x >= y { a } else { b },
                                (Value::Float(x), Value::Float(y)) => if x >= y { a } else { b },
                                _ => a,
                            }).unwrap_or(Value::Null),
                            _ => return Err(format!("List no tiene método '{}'", method_name)),
                        };
                        self.value_stack.push(result);
                    }

                    // ── Métodos de Dict ──────────────────────────────────
                    Value::Dict(map) => {
                        // Primero probar métodos builtin de dict
                        match method_name.as_str() {
                            "len"      => { self.value_stack.push(Value::Int(map.len() as i64)); }
                            "is_empty" => { self.value_stack.push(Value::Bool(map.is_empty())); }
                            "keys"     => { self.value_stack.push(Value::List(map.keys().map(|k| Value::Str(k.clone())).collect())); }
                            "values"   => { self.value_stack.push(Value::List(map.values().cloned().collect())); }
                            "contains" | "has_key" => {
                                let key = args.into_iter().next()
                                    .ok_or("dict.contains() requiere 1 argumento")?
                                    .to_string();
                                self.value_stack.push(Value::Bool(map.contains_key(&key)));
                            }
                            "get" => {
                                let key = args.into_iter().next()
                                    .ok_or("dict.get() requiere 1 argumento")?
                                    .to_string();
                                self.value_stack.push(map.get(&key).cloned().unwrap_or(Value::Null));
                            }
                            _ => {
                                // Buscar función en el dict (módulo namespace)
                                if let Some(fn_val) = map.get(method_name.as_str()).cloned() {
                                    let result = self.call_value(fn_val, args)?;
                                    self.value_stack.push(result);
                                } else {
                                    return Err(format!("Dict no tiene método '{}'", method_name));
                                }
                            }
                        }
                    }

                    Value::Instance(inst_rc) => {
                        let shape_name = inst_rc.borrow().shape_name.clone();
                        let act = self.find_act(&shape_name, &method_name)
                            .ok_or_else(|| format!("Método '{}' no encontrado en '{}'", method_name, shape_name))?
                            .clone();

                        if args.len() != act.params.len() {
                            return Err(format!(
                                "'{}' espera {} argumento(s), recibió {}",
                                method_name, act.params.len(), args.len()
                            ));
                        }

                        let mut frame = CallFrame::new(act.body, act.lines);
                        let field_names: Vec<String> = {
                            let inst = inst_rc.borrow();
                            for (k, v) in &inst.fields {
                                frame.vars.insert(k.clone(), v.clone());
                            }
                            inst.fields.keys().cloned().collect()
                        };
                        for (param, val) in act.params.iter().zip(args.into_iter()) {
                            frame.vars.insert(param.clone(), val);
                        }
                        frame.self_instance = Some(Rc::clone(&inst_rc));
                        frame.instance_fields = field_names;
                        self.call_stack.push(frame);
                    }
                    _ => return Err(format!("CallMethod '{}': no es una instancia", method_name)),
                }
            }

            // ── Colecciones ─────────────────────────────────────────────────
            Instruction::MakeList(n) => {
                let mut items: Vec<Value> = (0..n).map(|_| self.pop()).collect::<Result<Vec<_>, _>>()?;
                items.reverse();
                self.value_stack.push(Value::List(items));
            }
            Instruction::MakeDict(n) => {
                let mut map = IndexMap::new();
                for _ in 0..n {
                    let val = self.pop()?;
                    let key = match self.pop()? {
                        Value::Str(s) => s,
                        other => other.to_string(),
                    };
                    map.insert(key, val);
                }
                self.value_stack.push(Value::Dict(map));
            }
            Instruction::GetIndex => {
                let idx = self.pop()?;
                let obj = self.pop()?;
                match (obj, idx) {
                    (Value::List(items), Value::Int(i)) => {
                        let i_usize = if i < 0 {
                            let len = items.len() as i64;
                            (len + i) as usize
                        } else {
                            i as usize
                        };
                        let item = items.get(i_usize).cloned()
                            .ok_or_else(|| format!("Índice {} fuera de rango", i))?;
                        self.value_stack.push(item);
                    }
                    (Value::Dict(map), Value::Str(key)) => {
                        let val = map.get(&key).cloned()
                            .ok_or_else(|| format!("Clave '{}' no encontrada", key))?;
                        self.value_stack.push(val);
                    }
                    (Value::Str(s), Value::Int(i)) => {
                        let i_usize = if i < 0 {
                            let len = s.len() as i64;
                            (len + i) as usize
                        } else {
                            i as usize
                        };
                        let ch = s.chars().nth(i_usize)
                            .ok_or_else(|| format!("Índice {} fuera de rango en string", i))?;
                        self.value_stack.push(Value::Str(ch.to_string()));
                    }
                    _ => return Err("GetIndex: tipo no soportado".to_string()),
                }
            }
            Instruction::SetIndex => {
                let val = self.pop()?;
                let idx = self.pop()?;
                let obj = self.pop()?;
                match (obj, idx) {
                    (Value::List(mut items), Value::Int(i)) => {
                        let i_usize = if i < 0 {
                            (items.len() as i64 + i) as usize
                        } else {
                            i as usize
                        };
                        if i_usize >= items.len() {
                            return Err(format!("Índice {} fuera de rango en SetIndex", i));
                        }
                        items[i_usize] = val;
                        self.value_stack.push(Value::List(items));
                    }
                    (Value::Dict(mut map), idx) => {
                        let key = match idx {
                            Value::Str(s) => s,
                            other => other.to_string(),
                        };
                        map.insert(key, val);
                        self.value_stack.push(Value::Dict(map));
                    }
                    _ => return Err("SetIndex: tipo no soportado".to_string()),
                }
            }

            // ── Stack ────────────────────────────────────────────────────────
            Instruction::Pop => { self.pop()?; }
            Instruction::Dup => {
                let top = self.value_stack.last().cloned().ok_or("Stack vacío en Dup")?;
                self.value_stack.push(top);
            }

            // ── Async ────────────────────────────────────────────────────────
            Instruction::CallAsync(fn_name, argc) => {
                let argc = argc as usize;
                let mut args: Vec<Value> = (0..argc)
                    .map(|_| self.pop())
                    .collect::<Result<Vec<_>, _>>()?;
                args.reverse();

                let func = self.functions.get(&fn_name).cloned()
                    .ok_or_else(|| format!("función async '{}' no existe", fn_name))?;

                // Convertir args a SendValue (thread-safe)
                let send_args: Vec<SendValue> = args.iter()
                    .map(|v| v.to_send())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("Error en argumentos async '{}': {}", fn_name, e))?;

                let functions_clone = self.functions.clone();
                let shapes_clone    = self.shapes.clone();
                let fn_name_clone   = fn_name.clone();

                let slot: Arc<Mutex<Option<Result<SendValue, String>>>> =
                    Arc::new(Mutex::new(None));
                let slot_clone = Arc::clone(&slot);

                std::thread::spawn(move || {
                    let mut sub_vm = VM::new(
                        func.body.clone(),
                        func.lines.clone(),
                        functions_clone,
                        shapes_clone,
                    );
                    // Inyectar argumentos como vars del frame principal
                    for (param, val) in func.params.iter()
                        .zip(send_args.into_iter().map(from_send))
                    {
                        if let Some(frame) = sub_vm.call_stack.first_mut() {
                            frame.vars.insert(param.clone(), val);
                        }
                    }
                    if let Some(frame) = sub_vm.call_stack.first_mut() {
                        frame.name = fn_name_clone.clone();
                    }
                    let result = match sub_vm.run_raw() {
                        Ok(_) => {
                            let ret = sub_vm.value_stack.pop().unwrap_or(Value::Null);
                            match ret.to_send() {
                                Ok(sv)  => Ok(sv),
                                Err(_)  => Ok(SendValue::Null),
                            }
                        }
                        Err(e) => Err(e),
                    };
                    *slot_clone.lock().unwrap() = Some(result);
                });

                self.value_stack.push(Value::Task(slot));
            }

            Instruction::Await => {
                let val = self.pop()?;
                match val {
                    Value::Task(slot) => {
                        // Espera activa hasta que la tarea termine
                        let result = loop {
                            let guard = slot.lock().unwrap();
                            if let Some(ref r) = *guard {
                                let r = r.clone();
                                drop(guard);
                                break r;
                            }
                            drop(guard);
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        };
                        match result {
                            Ok(sv)  => self.value_stack.push(from_send(sv)),
                            Err(e)  => return Err(e),
                        }
                    }
                    other => {
                        // await en un valor no-Task → lo devuelve tal cual
                        self.value_stack.push(other);
                    }
                }
            }

            // ── I/O ──────────────────────────────────────────────────────────
            Instruction::Show => {
                let val = self.pop()?;
                println!("{}", val);
            }

            // ── IO nativo: ask / read / write / env ───────────────────────────
            Instruction::ReadInput { cast, choices } => {
                let prompt = self.pop()?;

                // Si hay choices, están en el stack debajo del prompt (ya extraímos prompt)
                let choices_list: Option<Vec<Value>> = if choices {
                    let c = self.pop()?;
                    if let Value::List(v) = c { Some(v) } else { None }
                } else {
                    None
                };

                // Mostrar opciones si hay
                if let Some(ref opts) = choices_list {
                    let opts_str: Vec<String> = opts.iter().map(|v| v.to_string()).collect();
                    print!("{} [{}]: ", prompt, opts_str.join(" / "));
                } else {
                    print!("{}", prompt);
                }
                io::stdout().flush().ok();

                let mut input = String::new();
                io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
                let raw = input.trim().to_string();

                // Validar choices si se definieron
                if let Some(ref opts) = choices_list {
                    let opts_str: Vec<String> = opts.iter().map(|v| v.to_string()).collect();
                    if !opts_str.contains(&raw) {
                        return Err(format!("Opción inválida '{}'. Elige entre: {}", raw, opts_str.join(", ")));
                    }
                }

                // Cast de tipo
                let result = match cast.as_deref() {
                    Some("int")   => Value::Int(raw.parse::<i64>().map_err(|_| format!("No se puede convertir '{}' a int", raw))?),
                    Some("float") => Value::Float(raw.parse::<f64>().map_err(|_| format!("No se puede convertir '{}' a float", raw))?),
                    Some("bool")  => Value::Bool(matches!(raw.as_str(), "yes" | "true" | "1")),
                    _             => Value::Str(raw),
                };
                self.value_stack.push(result);
            }

            Instruction::ReadFile(fmt) => {
                let path_val = self.pop()?;
                let path = path_val.to_string();
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("read: no se pudo leer '{}': {}", path, e))?;
                let result = match fmt.as_str() {
                    "json" => {
                        let parsed: serde_json::Value = serde_json::from_str(&content)
                            .map_err(|e| format!("read as json: {}", e))?;
                        json_to_value(parsed)
                    }
                    "lines" => {
                        let lines: Vec<Value> = content.lines().map(|l| Value::Str(l.to_string())).collect();
                        Value::List(lines)
                    }
                    _ => Value::Str(content),
                };
                self.value_stack.push(result);
            }

            Instruction::WriteFile(mode) => {
                let data_val = self.pop()?;
                let path_val = self.pop()?;
                let path = path_val.to_string();
                let data = data_val.to_string();
                match mode.as_str() {
                    "append" => {
                        use std::io::Write;
                        let mut f = std::fs::OpenOptions::new()
                            .create(true).append(true).open(&path)
                            .map_err(|e| format!("write append '{}': {}", path, e))?;
                        writeln!(f, "{}", data).map_err(|e| e.to_string())?;
                    }
                    _ => {
                        std::fs::write(&path, format!("{}\n", data))
                            .map_err(|e| format!("write '{}': {}", path, e))?;
                    }
                }
            }

            Instruction::ReadEnv(cast) => {
                let key_val = self.pop()?;
                let key = key_val.to_string();
                let raw = std::env::var(&key).unwrap_or_default();
                let result = match cast.as_str() {
                    "int"   => Value::Int(raw.parse::<i64>().unwrap_or(0)),
                    "float" => Value::Float(raw.parse::<f64>().unwrap_or(0.0)),
                    _       => Value::Str(raw),
                };
                self.value_stack.push(result);
            }

            // ── IA nativa (Fase 4) ────────────────────────────────────────────
            Instruction::AiAsk => {
                let prompt = self.pop()?;
                let response = ai_call(prompt.to_string(), None)?;
                self.value_stack.push(Value::Str(response));
            }

            Instruction::AiLearn => {
                let text = self.pop()?;
                self.ai_memory.push(text.to_string());
                let msg = format!("[aprendido: {} entradas en memoria]", self.ai_memory.len());
                self.value_stack.push(Value::Str(msg));
            }

            Instruction::AiSense => {
                let query = self.pop()?;
                if self.ai_memory.is_empty() {
                    self.value_stack.push(Value::Str(
                        "[sense: memoria vacía — usa 'learn' primero]".to_string(),
                    ));
                } else {
                    let context = self.ai_memory.join("\n---\n");
                    let response = ai_call(query.to_string(), Some(context))?;
                    self.value_stack.push(Value::Str(response));
                }
            }

            // ── Servidor HTTP nativo (Fase 7) ──────────────────────────────
            Instruction::ServeHTTP(fn_name) => {
                let port_val = self.pop()?;
                let port: u16 = match port_val {
                    Value::Int(n) => n as u16,
                    _ => return Err("serve: el puerto debe ser un entero".to_string()),
                };
                self.serve_http(port, fn_name)?;
            }

            Instruction::MakeFunction(_, _, _) => {
                // MakeFunction se registra en el primer pase del compilador — no-op en VM
            }
        }
        Ok(false)
    }

    fn pop(&mut self) -> Result<Value, String> {
        self.value_stack.pop().ok_or_else(|| "Stack vacío".to_string())
    }

    /// Carga un módulo por su path/nombre y devuelve un Value::Dict namespace.
    fn load_module(&mut self, path: &str) -> Result<Value, String> {
        use std::path::Path;
        let base_name = Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or(path);
        let prefix = format!("{}__", base_name);

        // 1) Módulos builtin Rust tienen prioridad sobre archivos
        match base_name {
            "math" => return Ok(self.builtin_math_module()),
            _ => {}
        }

        // 2) Buscar archivo .orx en packages/ o ruta relativa
        let orx_candidates = [
            format!("packages/{}.orx", path),
            format!("{}.orx", path),
            format!("lib/{}.orx", path),
        ];

        for candidate in &orx_candidates {
            if std::path::Path::new(candidate).exists() {
                return self.load_orx_module(candidate, base_name, &prefix);
            }
        }

        Err(format!("Módulo '{}' no encontrado", path))
    }

    /// Carga un módulo .orx: compila, ejecuta en sub-VM, extrae vars y fns en un dict.
    fn load_orx_module(&mut self, path: &str, module_name: &str, prefix: &str) -> Result<Value, String> {
        use crate::lexer::lex;
        use crate::parser::parse;
        use crate::codegen::compile;

        let src = std::fs::read_to_string(path)
            .map_err(|e| format!("No se pudo leer '{}': {}", path, e))?;
        let tokens = lex(&src).map_err(|e| format!("Error lexando '{}': {:?}", path, e))?;
        let ast = parse(tokens).map_err(|e| format!("Error parseando '{}': {:?}", path, e))?;
        let bc = compile(ast).map_err(|e| format!("Error compilando '{}': {:?}", path, e))?;

        // Copiar funciones del módulo al namespace actual con prefijo
        let mut ns: IndexMap<String, Value> = IndexMap::new();
        for (fname, fdef) in &bc.functions {
            let prefixed = format!("{}{}", prefix, fname);
            ns.insert(fname.clone(), Value::Str(prefixed.clone()));
            self.functions.insert(prefixed, fdef.clone());
        }

        // Ejecutar el módulo para obtener variables globales
        let mut sub_vm = VM::new(bc.main.clone(), bc.lines.clone(), bc.functions.clone(), bc.shapes.clone());
        sub_vm.run().ok(); // ignorar errores de side effects
        // Extraer vars del frame principal del sub_vm
        if let Some(frame) = sub_vm.call_stack.first() {
            for (k, v) in &frame.vars {
                if !k.starts_with('_') {
                    ns.insert(k.clone(), v.clone());
                }
            }
        }

        Ok(Value::Dict(ns))
    }

    /// Módulo math builtin con funciones nativas Rust.
    fn builtin_math_module(&mut self) -> Value {
        use std::f64::consts;
        let mut ns: IndexMap<String, Value> = IndexMap::new();

        // Constantes
        ns.insert("PI".to_string(), Value::Float(consts::PI));
        ns.insert("E".to_string(), Value::Float(consts::E));
        ns.insert("TAU".to_string(), Value::Float(consts::TAU));
        ns.insert("PHI".to_string(), Value::Float(1.6180339887498948));
        ns.insert("INF".to_string(), Value::Float(f64::INFINITY));

        // Registrar funciones builtin en self.functions
        let math_fns: &[(&str, &[&str])] = &[
            ("sqrt",     &["x"]),
            ("abs",      &["x"]),
            ("floor",    &["x"]),
            ("ceil",     &["x"]),
            ("round",    &["x"]),
            ("sin",      &["x"]),
            ("cos",      &["x"]),
            ("tan",      &["x"]),
            ("log",      &["x"]),
            ("log10",    &["x"]),
            ("log2",     &["x"]),
            ("exp",      &["x"]),
            ("pow",      &["a", "b"]),
            ("max",      &["a", "b"]),
            ("min",      &["a", "b"]),
            ("clamp",    &["x", "lo", "hi"]),
            ("factorial",&["n"]),
            ("sign",     &["x"]),
            ("degrees",  &["r"]),
            ("radians",  &["d"]),
            ("hypot",    &["a", "b"]),
            ("rand",     &[]),
            ("randint",  &["a", "b"]),
        ];

        for (fname, params) in math_fns {
            let key = format!("__math__{}", fname);
            ns.insert(fname.to_string(), Value::Str(key.clone()));
            // Registrar función con un body especial: usamos la instrucción Call con nombre especial
            // La VM resolverá __math__X directamente en call_value / Call handler
        }

        // Registrar un FunctionDef nativo fake para cada función math que haga dispatch
        // La forma más simple: crear FunctionDef con body = [CallBuiltin, Return]
        // Como no tenemos CallBuiltin, usamos una estrategia diferente:
        // Añadimos la lógica en call_value para __math__* nombres

        Value::Dict(ns)
    }

    /// Invoca un callable por nombre (string) con los argumentos dados
    /// y devuelve el resultado. Útil para map/filter/reduce sobre colecciones.
    fn call_value(&mut self, callee: Value, args: Vec<Value>) -> Result<Value, String> {
        let (fn_name, closure_env) = match callee {
            Value::Closure { fn_name, env } => (fn_name, Some(env)),
            Value::Str(s) => (s, None),
            other => return Err(format!("No es un callable: {:?}", other)),
        };

        if fn_name.starts_with("__math__") {
            return self.call_math_builtin(&fn_name[8..], args);
        }

        let func_def = self.functions.get(&fn_name)
            .ok_or_else(|| format!("Función '{}' no encontrada", fn_name))?
            .clone();
        let stack_depth = self.call_stack.len();
        let mut frame = CallFrame::with_args_named(
            func_def.body, func_def.lines, &fn_name, &func_def.params, args
        );
        if let Some(env) = closure_env {
            for (k, v) in env {
                frame.vars.entry(k).or_insert(v);
            }
        }
        self.call_stack.push(frame);
        loop {
            if self.call_stack.len() <= stack_depth { break; }
            let done = self.step()?;
            if done { break; }
        }
        Ok(self.value_stack.pop().unwrap_or(Value::Null))
    }

    /// Dispatch de funciones math builtin
    fn call_math_builtin(&self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        fn to_f64(v: &Value) -> Result<f64, String> {
            match v {
                Value::Float(f) => Ok(*f),
                Value::Int(i) => Ok(*i as f64),
                _ => Err(format!("Se esperaba número, no {:?}", v)),
            }
        }
        match name {
            "sqrt"      => Ok(Value::Float(to_f64(&args[0])?.sqrt())),
            "abs"       => match &args[0] {
                Value::Int(i)   => Ok(Value::Int(i.abs())),
                Value::Float(f) => Ok(Value::Float(f.abs())),
                _ => Err("abs() requiere número".into()),
            },
            "floor"     => Ok(Value::Int(to_f64(&args[0])?.floor() as i64)),
            "ceil"      => Ok(Value::Int(to_f64(&args[0])?.ceil() as i64)),
            "round"     => Ok(Value::Int(to_f64(&args[0])?.round() as i64)),
            "sin"       => Ok(Value::Float(to_f64(&args[0])?.sin())),
            "cos"       => Ok(Value::Float(to_f64(&args[0])?.cos())),
            "tan"       => Ok(Value::Float(to_f64(&args[0])?.tan())),
            "log"       => Ok(Value::Float(to_f64(&args[0])?.ln())),
            "log10"     => Ok(Value::Float(to_f64(&args[0])?.log10())),
            "log2"      => Ok(Value::Float(to_f64(&args[0])?.log2())),
            "exp"       => Ok(Value::Float(to_f64(&args[0])?.exp())),
            "pow"       => Ok(Value::Float(to_f64(&args[0])?.powf(to_f64(&args[1])?))),
            "max"       => {
                let a = to_f64(&args[0])?; let b = to_f64(&args[1])?;
                if a >= b { Ok(args[0].clone()) } else { Ok(args[1].clone()) }
            }
            "min"       => {
                let a = to_f64(&args[0])?; let b = to_f64(&args[1])?;
                if a <= b { Ok(args[0].clone()) } else { Ok(args[1].clone()) }
            }
            "clamp"     => {
                let x = to_f64(&args[0])?; let lo = to_f64(&args[1])?; let hi = to_f64(&args[2])?;
                Ok(Value::Float(x.clamp(lo, hi)))
            }
            "factorial" => {
                let n = match &args[0] { Value::Int(i) => *i, _ => to_f64(&args[0])? as i64 };
                if n < 0 { return Err("factorial de negativo".into()); }
                let mut r: i64 = 1;
                for i in 2..=n { r *= i; }
                Ok(Value::Int(r))
            }
            "sign"      => {
                let f = to_f64(&args[0])?;
                Ok(Value::Int(if f > 0.0 { 1 } else if f < 0.0 { -1 } else { 0 }))
            }
            "degrees"   => Ok(Value::Float(to_f64(&args[0])?.to_degrees())),
            "radians"   => Ok(Value::Float(to_f64(&args[0])?.to_radians())),
            "hypot"     => Ok(Value::Float(to_f64(&args[0])?.hypot(to_f64(&args[1])?))),
            "rand"      => {
                // Simple LCG random (no external crate)
                use std::time::SystemTime;
                let seed = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().subsec_nanos();
                let r = (seed as f64) / (u32::MAX as f64);
                Ok(Value::Float(r))
            }
            "randint"   => {
                use std::time::SystemTime;
                let a = match &args[0] { Value::Int(i) => *i, _ => to_f64(&args[0])? as i64 };
                let b = match &args[1] { Value::Int(i) => *i, _ => to_f64(&args[1])? as i64 };
                let seed = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().subsec_nanos();
                let range = (b - a + 1).max(1);
                Ok(Value::Int(a + (seed as i64 % range)))
            }
            _ => Err(format!("math.{} no implementado", name)),
        }
    }



    fn instantiate_shape(&mut self, shape_name: &str, args: Vec<Value>) -> Result<Rc<RefCell<InstanceData>>, String> {
        let all_fields = self.resolve_fields(shape_name)?;

        let mut fields: IndexMap<String, Value> = IndexMap::new();
        for field in &all_fields {
            let default_val = self.eval_mini_bytecode(&field.default)?;
            fields.insert(field.name.clone(), default_val);
        }

        let inst_rc = Rc::new(RefCell::new(InstanceData {
            shape_name: shape_name.to_string(),
            fields,
        }));

        let on_create = self.find_on_create(shape_name).cloned();

        if let Some(oc) = on_create {
            if !args.is_empty() && args.len() != oc.params.len() {
                return Err(format!(
                    "'{}' on_create espera {} argumento(s), recibió {}",
                    shape_name, oc.params.len(), args.len()
                ));
            }
            let mut frame = CallFrame::new(oc.body, oc.lines);
            let field_names: Vec<String> = {
                let inst = inst_rc.borrow();
                for (k, v) in &inst.fields {
                    frame.vars.insert(k.clone(), v.clone());
                }
                inst.fields.keys().cloned().collect()
            };
            for (param, val) in oc.params.iter().zip(args.into_iter()) {
                frame.vars.insert(param.clone(), val);
            }
            frame.self_instance = Some(Rc::clone(&inst_rc));
            frame.instance_fields = field_names;
            self.call_stack.push(frame);
            self.run_until_frame_done()?;
        } else if !args.is_empty() {
            let field_order: Vec<String> = all_fields.iter().map(|f| f.name.clone()).collect();
            if args.len() > field_order.len() {
                return Err(format!(
                    "'{}' tiene {} campo(s), recibió {} argumento(s)",
                    shape_name, field_order.len(), args.len()
                ));
            }
            let mut inst = inst_rc.borrow_mut();
            for (field_name, val) in field_order.iter().zip(args.into_iter()) {
                inst.fields.insert(field_name.clone(), val);
            }
        }

        Ok(inst_rc)
    }

    fn run_until_frame_done(&mut self) -> Result<(), String> {
        let target_depth = self.call_stack.len();
        loop {
            if self.call_stack.len() < target_depth { break; }

            let instr_opt = {
                let frame = match self.call_stack.last_mut() {
                    Some(f) => f,
                    None => break,
                };
                if frame.ip >= frame.instructions.len() {
                    None
                } else {
                    let line = frame.lines.get(frame.ip).copied().unwrap_or(0);
                    let instr = frame.instructions[frame.ip].clone();
                    frame.ip += 1;
                    Some((instr, line))
                }
            };

            match instr_opt {
                None => {
                    let frame = self.call_stack.pop().unwrap();
                    frame.sync_to_instance();
                }
                Some((Instruction::Return, _)) => {
                    let frame = self.call_stack.pop().ok_or("Return sin frame")?;
                    frame.sync_to_instance();
                }
                Some((other, line)) => {
                    if line > 0 { self.current_line = line; }
                    self.dispatch_instr(other)?;
                }
            }
        }
        Ok(())
    }

    fn eval_mini_bytecode(&mut self, instructions: &[Instruction]) -> Result<Value, String> {
        let stack_base = self.value_stack.len();
        for instr in instructions {
            match instr {
                Instruction::LoadInt(n)   => self.value_stack.push(Value::Int(*n)),
                Instruction::LoadFloat(f) => self.value_stack.push(Value::Float(*f)),
                Instruction::LoadStr(s)   => self.value_stack.push(Value::Str(s.clone())),
                Instruction::LoadBool(b)  => self.value_stack.push(Value::Bool(*b)),
                Instruction::LoadNull     => self.value_stack.push(Value::Null),
                Instruction::Return       => break,
                _ => {}
            }
        }
        if self.value_stack.len() > stack_base {
            Ok(self.value_stack.pop().unwrap())
        } else {
            Ok(Value::Null)
        }
    }

    fn resolve_fields(&self, shape_name: &str) -> Result<Vec<crate::bytecode::FieldDef>, String> {
        let shape = self.shapes.get(shape_name)
            .ok_or_else(|| format!("Shape '{}' no definido", shape_name))?
            .clone();

        let mut all_fields = Vec::new();
        for parent_name in &shape.using {
            let parent_fields = self.resolve_fields(parent_name)?;
            all_fields.extend(parent_fields);
        }
        all_fields.extend(shape.fields.clone());
        Ok(all_fields)
    }

    fn find_act(&self, shape_name: &str, method_name: &str) -> Option<&crate::bytecode::ActDef> {
        let shape = self.shapes.get(shape_name)?;
        if let Some(act) = shape.acts.get(method_name) {
            return Some(act);
        }
        for parent_name in &shape.using {
            if let Some(act) = self.find_act(parent_name, method_name) {
                return Some(act);
            }
        }
        None
    }

    fn find_on_create(&self, shape_name: &str) -> Option<&crate::bytecode::ActDef> {
        let shape = self.shapes.get(shape_name)?;
        if shape.on_create.is_some() {
            return shape.on_create.as_ref();
        }
        None
    }

    fn shape_uses(&self, shape_name: &str, target: &str) -> bool {
        if let Some(shape) = self.shapes.get(shape_name) {
            for parent in &shape.using {
                if parent == target || self.shape_uses(parent, target) {
                    return true;
                }
            }
        }
        false
    }

    // -------------------------------------------------------------------------
    // Builtins
    // -------------------------------------------------------------------------

    // -------------------------------------------------------------------------
    // Servidor HTTP nativo
    // -------------------------------------------------------------------------

    fn serve_http(&mut self, port: u16, fn_name: String) -> Result<(), String> {
        use tiny_http::{Server, Response, Header};
        use std::str::FromStr;

        let addr = format!("0.0.0.0:{}", port);
        let server = Server::http(&addr)
            .map_err(|e| format!("serve: no se pudo iniciar el servidor en {}: {}", addr, e))?;

        eprintln!("[Orion] Servidor escuchando en http://{}  (Ctrl+C para detener)", addr);

        for mut request in server.incoming_requests() {
            // Construir req dict
            let url = request.url().to_string();
            let method = request.method().to_string();

            // Parsear path y query params
            let (path, query) = if let Some(pos) = url.find('?') {
                (url[..pos].to_string(), url[pos+1..].to_string())
            } else {
                (url.clone(), String::new())
            };

            let mut params = IndexMap::new();
            for pair in query.split('&') {
                if let Some((k, v)) = pair.split_once('=') {
                    params.insert(k.to_string(), Value::Str(v.to_string()));
                }
            }

            let mut body_str = String::new();
            {
                request.as_reader().read_to_string(&mut body_str).ok();
            }

            let mut req_map = IndexMap::new();
            req_map.insert("path".to_string(),   Value::Str(path));
            req_map.insert("method".to_string(), Value::Str(method));
            req_map.insert("body".to_string(),   Value::Str(body_str));
            req_map.insert("params".to_string(), Value::Dict(params));
            let req_val = Value::Dict(req_map);

            // Llamar la función handler
            let func = self.functions.get(&fn_name).cloned()
                .ok_or_else(|| format!("serve: handler '{}' no encontrado", fn_name))?;

            if func.params.len() != 1 {
                return Err(format!("serve: handler '{}' debe tener exactamente 1 parámetro (req)", fn_name));
            }

            let frame = CallFrame::with_args(func.body, func.lines, &func.params, vec![req_val]);
            self.call_stack.push(frame);
            self.run_until_frame_done()?;

            // Obtener resultado del stack (el Return pone el valor en el stack si lo hay)
            let result = self.value_stack.pop().unwrap_or(Value::Null);

            // Extraer status y body de la respuesta
            let (status_code, resp_body, content_type) = match result {
                Value::Dict(ref m) => {
                    let status = match m.get("status") {
                        Some(Value::Int(n)) => *n as u16,
                        _ => 200,
                    };
                    let body = m.get("body").map(|v| v.to_string()).unwrap_or_default();
                    let ct = m.get("content_type")
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "text/plain; charset=utf-8".to_string());
                    (status, body, ct)
                }
                Value::Str(s) => (200, s, "text/plain; charset=utf-8".to_string()),
                Value::Null   => (204, String::new(), "text/plain".to_string()),
                other         => (200, other.to_string(), "text/plain; charset=utf-8".to_string()),
            };

            let header = Header::from_str(&format!("Content-Type: {}", content_type)).ok();
            let mut response = Response::from_string(resp_body).with_status_code(status_code);
            if let Some(h) = header {
                response = response.with_header(h);
            }

            eprintln!("[Orion] {} {} → {}", request.method(), request.url(), status_code);
            request.respond(response)
                .map_err(|e| format!("serve: error al responder: {}", e))?;
        }
        Ok(())
    }

    fn call_builtin(&self, name: &str, args: Vec<Value>) -> Result<Option<Value>, String> {
        match name {
            "str" => {
                let val = args.into_iter().next().ok_or("str() requiere un argumento")?;
                Ok(Some(Value::Str(val.to_string())))
            }
            "int" => {
                let val = args.into_iter().next().ok_or("int() requiere un argumento")?;
                match val {
                    Value::Int(n)   => Ok(Some(Value::Int(n))),
                    Value::Float(f) => Ok(Some(Value::Int(f as i64))),
                    Value::Str(s)   => s.parse::<i64>()
                        .map(|n| Some(Value::Int(n)))
                        .map_err(|_| format!("No se puede convertir '{}' a int", s)),
                    _ => Err("int(): tipo no convertible".to_string()),
                }
            }
            "float" => {
                let val = args.into_iter().next().ok_or("float() requiere un argumento")?;
                match val {
                    Value::Float(f) => Ok(Some(Value::Float(f))),
                    Value::Int(n)   => Ok(Some(Value::Float(n as f64))),
                    Value::Str(s)   => s.parse::<f64>()
                        .map(|f| Some(Value::Float(f)))
                        .map_err(|_| format!("No se puede convertir '{}' a float", s)),
                    _ => Err("float(): tipo no convertible".to_string()),
                }
            }
            "len" => {
                let val = args.into_iter().next().ok_or("len() requiere un argumento")?;
                match val {
                    Value::List(v) => Ok(Some(Value::Int(v.len() as i64))),
                    Value::Str(s)  => Ok(Some(Value::Int(s.len() as i64))),
                    Value::Dict(m) => Ok(Some(Value::Int(m.len() as i64))),
                    _ => Err("len(): tipo no soportado".to_string()),
                }
            }
            "type" => {
                let val = args.into_iter().next().ok_or("type() requiere un argumento")?;
                Ok(Some(Value::Str(val.type_name())))
            }
            "show" => {
                for arg in args { print!("{} ", arg); }
                println!();
                Ok(None)
            }
            // ── Listas ───────────────────────────────────────────────────────
            "push" | "append" => {
                let mut it = args.into_iter();
                let list = it.next().ok_or("push() requiere al menos 2 argumentos")?;
                let val  = it.next().ok_or("push() requiere al menos 2 argumentos")?;
                match list {
                    Value::List(mut v) => { v.push(val); Ok(Some(Value::List(v))) }
                    _ => Err("push(): el primer argumento debe ser una lista".to_string()),
                }
            }
            "pop" => {
                let list = args.into_iter().next().ok_or("pop() requiere un argumento")?;
                match list {
                    Value::List(mut v) => {
                        let item = v.pop().unwrap_or(Value::Null);
                        // devuelve [item, nueva_lista] para permitir acceso a ambos
                        Ok(Some(Value::List(vec![item, Value::List(v)])))
                    }
                    _ => Err("pop(): requiere una lista".to_string()),
                }
            }
            "first" => {
                let list = args.into_iter().next().ok_or("first() requiere un argumento")?;
                match list {
                    Value::List(v) => Ok(Some(v.into_iter().next().unwrap_or(Value::Null))),
                    _ => Err("first(): requiere una lista".to_string()),
                }
            }
            "last" => {
                let list = args.into_iter().next().ok_or("last() requiere un argumento")?;
                match list {
                    Value::List(v) => Ok(Some(v.into_iter().last().unwrap_or(Value::Null))),
                    _ => Err("last(): requiere una lista".to_string()),
                }
            }
            "reverse" => {
                let list = args.into_iter().next().ok_or("reverse() requiere un argumento")?;
                match list {
                    Value::List(mut v) => { v.reverse(); Ok(Some(Value::List(v))) }
                    Value::Str(s)      => Ok(Some(Value::Str(s.chars().rev().collect()))),
                    _ => Err("reverse(): requiere una lista o string".to_string()),
                }
            }
            "range" => {
                let mut it = args.into_iter();
                let a = it.next().ok_or("range() requiere al menos 1 argumento")?;
                let b = it.next();
                let (start, end) = match (a, b) {
                    (Value::Int(n), None)           => (0i64, n),
                    (Value::Int(s), Some(Value::Int(e))) => (s, e),
                    _ => return Err("range() requiere argumentos enteros".to_string()),
                };
                let v: Vec<Value> = (start..end).map(Value::Int).collect();
                Ok(Some(Value::List(v)))
            }
            "contains" => {
                let mut it = args.into_iter();
                let container = it.next().ok_or("contains() requiere 2 argumentos")?;
                let item      = it.next().ok_or("contains() requiere 2 argumentos")?;
                match container {
                    Value::List(v) => Ok(Some(Value::Bool(v.contains(&item)))),
                    Value::Str(s)  => {
                        let needle = item.to_string();
                        Ok(Some(Value::Bool(s.contains(needle.as_str()))))
                    }
                    Value::Dict(m) => {
                        let key = item.to_string();
                        Ok(Some(Value::Bool(m.contains_key(&key))))
                    }
                    _ => Err("contains(): tipo no soportado".to_string()),
                }
            }
            // ── Dicts ───────────────────────────────────────────────────────
            "keys" => {
                let val = args.into_iter().next().ok_or("keys() requiere un argumento")?;
                match val {
                    Value::Dict(m) => Ok(Some(Value::List(m.keys().map(|k| Value::Str(k.clone())).collect()))),
                    _ => Err("keys(): requiere un dict".to_string()),
                }
            }
            "values" => {
                let val = args.into_iter().next().ok_or("values() requiere un argumento")?;
                match val {
                    Value::Dict(m) => Ok(Some(Value::List(m.into_values().collect()))),
                    _ => Err("values(): requiere un dict".to_string()),
                }
            }
            "has_key" => {
                let mut it = args.into_iter();
                let dict = it.next().ok_or("has_key() requiere 2 argumentos")?;
                let key  = it.next().ok_or("has_key() requiere 2 argumentos")?;
                match dict {
                    Value::Dict(m) => Ok(Some(Value::Bool(m.contains_key(&key.to_string())))),
                    _ => Err("has_key(): requiere un dict".to_string()),
                }
            }
            // ── Strings ─────────────────────────────────────────────────────
            "upper" => {
                let val = args.into_iter().next().ok_or("upper() requiere un argumento")?;
                match val {
                    Value::Str(s) => Ok(Some(Value::Str(s.to_uppercase()))),
                    _ => Err("upper(): requiere un string".to_string()),
                }
            }
            "lower" => {
                let val = args.into_iter().next().ok_or("lower() requiere un argumento")?;
                match val {
                    Value::Str(s) => Ok(Some(Value::Str(s.to_lowercase()))),
                    _ => Err("lower(): requiere un string".to_string()),
                }
            }
            "trim" => {
                let val = args.into_iter().next().ok_or("trim() requiere un argumento")?;
                match val {
                    Value::Str(s) => Ok(Some(Value::Str(s.trim().to_string()))),
                    _ => Err("trim(): requiere un string".to_string()),
                }
            }
            "split" => {
                let mut it = args.into_iter();
                let s   = it.next().ok_or("split() requiere 2 argumentos")?;
                let sep = it.next().ok_or("split() requiere 2 argumentos")?;
                match (s, sep) {
                    (Value::Str(text), Value::Str(delimiter)) => {
                        let parts: Vec<Value> = text.split(delimiter.as_str())
                            .map(|p| Value::Str(p.to_string()))
                            .collect();
                        Ok(Some(Value::List(parts)))
                    }
                    _ => Err("split(): requiere dos strings".to_string()),
                }
            }
            "join" => {
                let mut it = args.into_iter();
                let list = it.next().ok_or("join() requiere 2 argumentos")?;
                let sep  = it.next().unwrap_or(Value::Str(" ".to_string()));
                match (list, sep) {
                    (Value::List(v), Value::Str(s)) => {
                        let parts: Vec<String> = v.iter().map(|x| x.to_string()).collect();
                        Ok(Some(Value::Str(parts.join(&s))))
                    }
                    _ => Err("join(): join(lista, sep)".to_string()),
                }
            }
            "starts_with" => {
                let mut it = args.into_iter();
                let s      = it.next().ok_or("starts_with() requiere 2 argumentos")?;
                let prefix = it.next().ok_or("starts_with() requiere 2 argumentos")?;
                match (s, prefix) {
                    (Value::Str(a), Value::Str(b)) => Ok(Some(Value::Bool(a.starts_with(b.as_str())))),
                    _ => Err("starts_with(): requiere strings".to_string()),
                }
            }
            "ends_with" => {
                let mut it = args.into_iter();
                let s      = it.next().ok_or("ends_with() requiere 2 argumentos")?;
                let suffix = it.next().ok_or("ends_with() requiere 2 argumentos")?;
                match (s, suffix) {
                    (Value::Str(a), Value::Str(b)) => Ok(Some(Value::Bool(a.ends_with(b.as_str())))),
                    _ => Err("ends_with(): requiere strings".to_string()),
                }
            }
            "replace" => {
                let mut it = args.into_iter();
                let s    = it.next().ok_or("replace() requiere 3 argumentos")?;
                let from = it.next().ok_or("replace() requiere 3 argumentos")?;
                let to   = it.next().ok_or("replace() requiere 3 argumentos")?;
                match (s, from, to) {
                    (Value::Str(text), Value::Str(f), Value::Str(t)) => {
                        Ok(Some(Value::Str(text.replace(f.as_str(), t.as_str()))))
                    }
                    _ => Err("replace(): requiere strings".to_string()),
                }
            }
            // ── Matemáticas ─────────────────────────────────────────────────
            "abs" => {
                let val = args.into_iter().next().ok_or("abs() requiere un argumento")?;
                match val {
                    Value::Int(n)   => Ok(Some(Value::Int(n.abs()))),
                    Value::Float(f) => Ok(Some(Value::Float(f.abs()))),
                    _ => Err("abs(): requiere un número".to_string()),
                }
            }
            "max" => {
                if args.is_empty() { return Err("max() requiere argumentos".to_string()); }
                // max(a, b) o max(lista)
                let items = if args.len() == 1 {
                    match args.into_iter().next().unwrap() {
                        Value::List(v) => v,
                        other => vec![other],
                    }
                } else { args };
                let best = items.into_iter().next().unwrap();
                Ok(Some(best))
            }
            "min" => {
                if args.is_empty() { return Err("min() requiere argumentos".to_string()); }
                let items = if args.len() == 1 {
                    match args.into_iter().next().unwrap() {
                        Value::List(v) => v,
                        other => vec![other],
                    }
                } else { args };
                let best = items.into_iter().next().unwrap();
                Ok(Some(best))
            }
            "floor" => {
                let val = args.into_iter().next().ok_or("floor() requiere un argumento")?;
                match val {
                    Value::Float(f) => Ok(Some(Value::Int(f.floor() as i64))),
                    Value::Int(n)   => Ok(Some(Value::Int(n))),
                    _ => Err("floor(): requiere un número".to_string()),
                }
            }
            "ceil" => {
                let val = args.into_iter().next().ok_or("ceil() requiere un argumento")?;
                match val {
                    Value::Float(f) => Ok(Some(Value::Int(f.ceil() as i64))),
                    Value::Int(n)   => Ok(Some(Value::Int(n))),
                    _ => Err("ceil(): requiere un número".to_string()),
                }
            }
            "sqrt" => {
                let val = args.into_iter().next().ok_or("sqrt() requiere un argumento")?;
                match val {
                    Value::Float(f) => Ok(Some(Value::Float(f.sqrt()))),
                    Value::Int(n)   => Ok(Some(Value::Float((n as f64).sqrt()))),
                    _ => Err("sqrt(): requiere un número".to_string()),
                }
            }
            // ── I/O ─────────────────────────────────────────────────────────
            "input" => {
                use std::io::{self, BufRead};
                let prompt = args.into_iter().next().unwrap_or(Value::Null);
                if prompt != Value::Null { print!("{}", prompt); }
                use std::io::Write;
                io::stdout().flush().ok();
                let stdin = io::stdin();
                let line = stdin.lock().lines().next()
                    .unwrap_or(Ok(String::new()))
                    .unwrap_or_default();
                Ok(Some(Value::Str(line)))
            }
            // ── Tests ───────────────────────────────────────────────────────
            "assert" => {
                let mut it = args.into_iter();
                let cond = it.next().ok_or("assert() requiere al menos 1 argumento")?;
                let msg  = it.next();
                if !cond.is_truthy() {
                    let text = msg.map(|v| v.to_string())
                        .unwrap_or_else(|| "Aserción falló".to_string());
                    return Err(format!("assert: {}", text));
                }
                Ok(Some(Value::Null))
            }
            "assert_eq" => {
                let mut it = args.into_iter();
                let a = it.next().ok_or("assert_eq() requiere 2 argumentos")?;
                let b = it.next().ok_or("assert_eq() requiere 2 argumentos")?;
                let msg = it.next();
                if a != b {
                    let header = msg.map(|v| format!("{} — ", v)).unwrap_or_default();
                    return Err(format!(
                        "assert_eq: {}esperado: {}\n  obtenido: {}",
                        header, b, a
                    ));
                }
                Ok(Some(Value::Null))
            }
            "assert_ne" => {
                let mut it = args.into_iter();
                let a = it.next().ok_or("assert_ne() requiere 2 argumentos")?;
                let b = it.next().ok_or("assert_ne() requiere 2 argumentos")?;
                if a == b {
                    return Err(format!("assert_ne: se esperaban valores distintos, ambos son: {}", a));
                }
                Ok(Some(Value::Null))
            }

            other => Err(format!("Función '{}' no definida", other)),
        }
    }
} // fin impl VM

// ---------------------------------------------------------------------------
// AI HTTP helper — usado por AiAsk / AiLearn / AiSense
// ---------------------------------------------------------------------------

/// Convierte un serde_json::Value en un Value de Orion.
fn json_to_value(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Value::Int(i) }
            else { Value::Float(n.as_f64().unwrap_or(0.0)) }
        }
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(arr) => {
            Value::List(arr.into_iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(map) => {
            let mut hm = IndexMap::new();
            for (k, val) in map {
                hm.insert(k, json_to_value(val));
            }
            Value::Dict(hm)
        }
    }
}

/// Carga variables de un archivo .env (sin dependencias externas).
fn load_dotenv() {
    use std::io::{BufRead, BufReader};
    let candidates = ["..env", ".env", "../.env", "../../.env"];
    for path in &candidates {
        if let Ok(f) = std::fs::File::open(path) {
            for line in BufReader::new(f).lines().flatten() {
                let line = line.trim().to_string();
                if line.is_empty() || line.starts_with('#') { continue; }
                if let Some((k, v)) = line.split_once('=') {
                    let key = k.trim();
                    let val = v.trim().trim_matches('"').trim_matches('\'');
                    if std::env::var(key).is_err() {
                        std::env::set_var(key, val);
                    }
                }
            }
            return;
        }
    }
}

/// Llama a la API de IA (Anthropic o OpenAI).
/// `context`: si se proporciona, se usa como system prompt de recuperación de memoria.
fn ai_call(prompt: String, context: Option<String>) -> Result<String, String> {
    load_dotenv();

    let anthropic_key = std::env::var("ANTHROPIC_API_KEY").ok();
    let openai_key    = std::env::var("OPENAI_API_KEY").ok();

    match (anthropic_key, openai_key) {
        (Some(key), _) => ai_call_anthropic(prompt, context, key),
        (None, Some(key)) => ai_call_openai(prompt, context, key),
        _ => Err(
            "No hay API key configurada.\n\
             Agrega en tu .env:\n  ANTHROPIC_API_KEY=sk-ant-...\no\n  OPENAI_API_KEY=sk-...".to_string()
        ),
    }
}

fn ai_call_anthropic(prompt: String, context: Option<String>, key: String) -> Result<String, String> {
    let model = std::env::var("ANTHROPIC_MODEL")
        .unwrap_or_else(|_| "claude-3-5-haiku-20241022".to_string());

    let system_prompt = context.map(|ctx| {
        format!(
            "Responde usando ÚNICAMENTE la siguiente información almacenada:\n\n{}\n\n\
             Si la respuesta no está en la información, dilo claramente.",
            ctx
        )
    });

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": 1024,
        "messages": [{"role": "user", "content": prompt}]
    });
    if let Some(sys) = system_prompt {
        body["system"] = serde_json::Value::String(sys);
    }

    let resp = ureq::post("https://api.anthropic.com/v1/messages")
        .set("Content-Type", "application/json")
        .set("x-api-key", &key)
        .set("anthropic-version", "2023-06-01")
        .send_json(body)
        .map_err(|e| format!("Error HTTP Anthropic: {}", e))?;

    let json: serde_json::Value = resp.into_json()
        .map_err(|e| format!("Error JSON Anthropic: {}", e))?;

    json["content"][0]["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Respuesta inesperada de Anthropic: {}", json))
}

fn ai_call_openai(prompt: String, context: Option<String>, key: String) -> Result<String, String> {
    let model = std::env::var("OPENAI_MODEL")
        .unwrap_or_else(|_| "gpt-4o-mini".to_string());

    let mut messages = vec![];
    if let Some(ctx) = context {
        messages.push(serde_json::json!({
            "role": "system",
            "content": format!(
                "Responde usando ÚNICAMENTE la siguiente información almacenada:\n\n{}\n\n\
                 Si la respuesta no está en la información, dilo claramente.",
                ctx
            )
        }));
    }
    messages.push(serde_json::json!({ "role": "user", "content": prompt }));

    let resp = ureq::post("https://api.openai.com/v1/chat/completions")
        .set("Content-Type", "application/json")
        .set("Authorization", &format!("Bearer {}", key))
        .send_json(serde_json::json!({
            "model": model,
            "max_tokens": 1024,
            "messages": messages
        }))
        .map_err(|e| format!("Error HTTP OpenAI: {}", e))?;

    let json: serde_json::Value = resp.into_json()
        .map_err(|e| format!("Error JSON OpenAI: {}", e))?;

    json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Respuesta inesperada de OpenAI: {}", json))
}
