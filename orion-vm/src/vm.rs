use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::cell::RefCell;
use std::io::{self, Write};
use crate::instruction::Instruction;
use crate::value::{Value, InstanceData};
use crate::bytecode::{FunctionDef, ShapeDef};

struct CallFrame {
    instructions: Vec<Instruction>,
    lines: Vec<u32>,
    ip: usize,
    vars: HashMap<String, Value>,
    consts: HashSet<String>,
    /// Si es un frame de act/on_create, referencia a la instancia actual
    self_instance: Option<Rc<RefCell<InstanceData>>>,
    /// Nombres de los campos de la instancia (para sincronizar al salir del frame)
    instance_fields: Vec<String>,
}

impl CallFrame {
    fn new(instructions: Vec<Instruction>, lines: Vec<u32>) -> Self {
        CallFrame {
            instructions, lines, ip: 0,
            vars: HashMap::new(),
            consts: HashSet::new(),
            self_instance: None,
            instance_fields: Vec::new(),
        }
    }

    fn with_args(instructions: Vec<Instruction>, lines: Vec<u32>, params: &[String], args: Vec<Value>) -> Self {
        let mut frame = Self::new(instructions, lines);
        for (param, val) in params.iter().zip(args.into_iter()) {
            frame.vars.insert(param.clone(), val);
        }
        frame
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
    functions: HashMap<String, FunctionDef>,
    shapes: HashMap<String, ShapeDef>,
    current_line: u32,
    error_handlers: Vec<ErrorHandler>,
    /// Memoria de sesión para instrucciones AiLearn / AiSense
    ai_memory: Vec<String>,
}

impl VM {
    pub fn new(
        main: Vec<Instruction>,
        main_lines: Vec<u32>,
        functions: HashMap<String, FunctionDef>,
        shapes: HashMap<String, ShapeDef>,
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

    pub fn run(&mut self) -> Result<(), String> {
        self.run_inner().map_err(|e| {
            if self.current_line > 0 {
                format!("[línea {}] {}", self.current_line, e)
            } else {
                e
            }
        })
    }

    fn run_inner(&mut self) -> Result<(), String> {
        loop {
            // Manejo de fin de frame
            {
                let frame = match self.call_stack.last_mut() {
                    Some(f) => f,
                    None => break,
                };
                if frame.ip >= frame.instructions.len() {
                    let frame = self.call_stack.pop().unwrap();
                    frame.sync_to_instance();
                    continue;
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
                Ok(true) => break,
                Ok(false) => {}
                Err(e) => {
                    if let Some(handler) = self.error_handlers.pop() {
                        // Deshacer call stack hasta la profundidad del handler
                        while self.call_stack.len() > handler.frame_depth {
                            let f = self.call_stack.pop().unwrap();
                            f.sync_to_instance();
                        }
                        // Poner mensaje de error en el stack (lo toma StoreVar)
                        self.value_stack.push(Value::Str(e));
                        // Saltar al bloque handle
                        let frame = self.call_stack.last_mut()
                            .ok_or("Sin frame activo para handle")?;
                        frame.ip = handler.handler_addr;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
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
                let frame = self.call_stack.last().ok_or("Sin frame activo")?;
                let val = frame.vars.get(&name).cloned()
                    .ok_or_else(|| format!("Variable '{}' no definida", name))?;
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

                if self.shapes.contains_key(&name) {
                    let inst_rc = self.instantiate_shape(&name, args)?;
                    self.value_stack.push(Value::Instance(inst_rc));
                } else if let Some(func) = self.functions.get(&name).cloned() {
                    if args.len() != func.params.len() {
                        return Err(format!(
                            "'{}' espera {} argumento(s), recibió {}",
                            name, func.params.len(), args.len()
                        ));
                    }
                    let frame = CallFrame::with_args(func.body, func.lines, &func.params, args);
                    self.call_stack.push(frame);
                } else {
                    let result = self.call_builtin(&name, args)?;
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
                    _ => return Err(format!("GetAttr '{}': no es una instancia", attr)),
                }
            }
            Instruction::SetAttr(attr) => {
                let val = self.pop()?;
                let obj = self.pop()?;
                match obj {
                    Value::Instance(inst_rc) => { inst_rc.borrow_mut().fields.insert(attr, val); }
                    _ => return Err(format!("SetAttr '{}': no es una instancia", attr)),
                }
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
                let mut map = HashMap::new();
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
                        let i_usize = i as usize;
                        if i_usize >= items.len() {
                            return Err(format!("Índice {} fuera de rango en SetIndex", i));
                        }
                        items[i_usize] = val;
                        self.value_stack.push(Value::List(items));
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

    // -------------------------------------------------------------------------
    // OOP helpers
    // -------------------------------------------------------------------------

    fn instantiate_shape(&mut self, shape_name: &str, args: Vec<Value>) -> Result<Rc<RefCell<InstanceData>>, String> {
        let all_fields = self.resolve_fields(shape_name)?;

        let mut fields: HashMap<String, Value> = HashMap::new();
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

            let mut params = HashMap::new();
            for pair in query.split('&') {
                if let Some((k, v)) = pair.split_once('=') {
                    params.insert(k.to_string(), Value::Str(v.to_string()));
                }
            }

            let mut body_str = String::new();
            {
                request.as_reader().read_to_string(&mut body_str).ok();
            }

            let mut req_map = HashMap::new();
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
            let mut hm = std::collections::HashMap::new();
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
