use std::collections::HashSet;
use indexmap::IndexMap as HashMap;
use indexmap::IndexMap;
use std::rc::Rc;
use std::cell::RefCell;
use std::io::{self, Write};
use std::sync::Arc;
use libloading;
use crate::instruction::Instruction;
use crate::value::{Value, InstanceData, SendValue, TaskHandle, from_send};
use crate::bytecode::{ExternFnDef, FunctionDef, ShapeDef};
use crate::gc::Gc;

fn write_utf8_line(s: &str) {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        extern "system" {
            fn GetStdHandle(nStdHandle: u32) -> *mut c_void;
            fn WriteConsoleW(hConsoleOutput: *mut c_void, lpBuffer: *const u16,
                             nNumberOfCharsToWrite: u32, lpNumberOfCharsWritten: *mut u32,
                             lpReserved: *const c_void) -> i32;
            fn GetConsoleMode(hConsoleHandle: *mut c_void, lpMode: *mut u32) -> i32;
        }
        unsafe {
            let handle = GetStdHandle(0xFFFFFFF5u32); // STD_OUTPUT_HANDLE = -11
            let handle_isize = handle as isize;
            let mut mode: u32 = 0;
            let is_console = handle_isize != -1 && !handle.is_null()
                             && GetConsoleMode(handle, &mut mode) != 0;

            let line = format!("{s}\n");
            let utf16: Vec<u16> = line.encode_utf16().collect();

            if is_console {
                let mut written: u32 = 0;
                let ok = WriteConsoleW(handle, utf16.as_ptr(), utf16.len() as u32,
                                       &mut written, std::ptr::null());
                if ok != 0 { return; }
            }
        }
    }
    // Fallback: bytes UTF-8 crudos (pipe / Output Channel de VS Code)
    let line = format!("{s}\n");
    let _ = io::stdout().lock().write_all(line.as_bytes());
    let _ = io::stdout().lock().flush();
}

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
    /// Si este frame ejecuta un closure, referencia compartida a su entorno
    /// capturado. Al retornar se escriben de vuelta las variables capturadas
    /// para que sus mutaciones persistan entre llamadas.
    closure_env: Option<Rc<RefCell<IndexMap<String, Value>>>>,
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
            closure_env: None,
        }
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

    /// Escribe de vuelta al entorno compartido del closure las variables
    /// capturadas que pudieron mutar durante esta invocación. Solo persiste
    /// las claves ya presentes en el entorno (las capturadas), no los locales nuevos.
    fn sync_to_closure(&self) {
        if let Some(env_rc) = &self.closure_env {
            let mut env = env_rc.borrow_mut();
            let keys: Vec<String> = env.keys().cloned().collect();
            for k in keys {
                if let Some(val) = self.vars.get(&k) {
                    env.insert(k, val.clone());
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
    extern_fns: IndexMap<String, ExternFnDef>,
    extern_libs: IndexMap<String, libloading::Library>,
    current_line: u32,
    error_handlers: Vec<ErrorHandler>,
    /// Mark-and-sweep GC para instancias con ciclos de referencias
    gc: Gc,
    /// Contadores de llamadas por función (hotspot detection)
    call_counts: HashMap<String, u64>,
    /// Token de cancelación cooperativa. Presente solo en sub-VMs lanzadas por
    /// `spawn`/`async fn`: el bucle de instrucciones lo consulta y aborta si se
    /// activa (via `tarea.cancelar` o un canal "done").
    cancel_token: Option<Arc<TaskHandle>>,
}

impl Drop for VM {
    fn drop(&mut self) {
        // No colectar durante un panic: un RefCell podría estar prestado y
        // el borrow_mut del sweep convertiría el panic en abort.
        if !std::thread::panicking() {
            self.gc_teardown();
        }
    }
}

impl VM {
    pub fn new(
        main: Vec<Instruction>,
        main_lines: Vec<u32>,
        functions: IndexMap<String, FunctionDef>,
        shapes: IndexMap<String, ShapeDef>,
        extern_fns: IndexMap<String, ExternFnDef>,
    ) -> Self {
        VM {
            value_stack: Vec::new(),
            call_stack: vec![CallFrame::new(main, main_lines)],
            functions,
            shapes,
            extern_fns,
            extern_libs: IndexMap::new(),
            current_line: 0,
            error_handlers: Vec::new(),
            gc: Gc::new(),
            call_counts: HashMap::new(),
            cancel_token: None,
        }
    }

    /// Ejecuta una función por nombre en un contexto aislado (sus propias
    /// funciones, shapes y variables globales), devolviendo el valor de retorno.
    ///
    /// Pensado como puente para el JIT: un módulo `.orx` se compila y sus
    /// funciones corren bajo la VM cuando el código JIT las invoca. Como todo
    /// vive en nombres simples dentro de este contexto, la recursión, los
    /// helpers internos y las constantes/`use` del módulo resuelven sin prefijos.
    pub fn call_named(
        functions: IndexMap<String, FunctionDef>,
        shapes: IndexMap<String, ShapeDef>,
        globals: IndexMap<String, Value>,
        fn_name: &str,
        args: Vec<Value>,
    ) -> Result<Value, String> {
        let mut vm = VM::new(vec![Instruction::Halt], vec![0], functions, shapes, IndexMap::new());
        if let Some(frame) = vm.call_stack.first_mut() {
            for (k, v) in globals {
                frame.vars.insert(k, v);
            }
        }
        vm.call_value(Value::Str(fn_name.to_string()), args)
    }

    /// Ejecuta el cuerpo principal y devuelve las variables globales resultantes
    /// (sin las que empiezan por `_`). Útil para extraer las constantes/`use` de
    /// un módulo `.orx` al cargarlo desde el JIT.
    pub fn into_globals(mut self) -> IndexMap<String, Value> {
        self.run().ok();
        let mut globals = IndexMap::new();
        if let Some(frame) = self.call_stack.first() {
            for (k, v) in &frame.vars {
                if !k.starts_with('_') {
                    globals.insert(k.clone(), v.clone());
                }
            }
        }
        globals
    }

    /// Devuelve las funciones más llamadas, ordenadas de mayor a menor.
    /// Útil para profiling y para decidir qué compilar con JIT.
    pub fn hotspots(&self, top_n: usize) -> Vec<(&str, u64)> {
        let mut v: Vec<(&str, u64)> = self.call_counts.iter()
            .map(|(k, &c)| (k.as_str(), c))
            .collect();
        v.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        v.truncate(top_n);
        v
    }

    /// Recolecta instancias con ciclos usando mark-and-sweep.
    fn gc_collect(&mut self) {
        let mut roots: Vec<Value> = Vec::with_capacity(
            self.value_stack.len() + self.call_stack.len() * 8,
        );
        roots.extend(self.value_stack.iter().cloned());
        for frame in &self.call_stack {
            roots.extend(frame.vars.values().cloned());
            if let Some(ref inst) = frame.self_instance {
                roots.push(Value::Instance(Rc::clone(inst)));
            }
            // El entorno capturado de un closure es estado vivo alcanzable aunque
            // no se haya copiado a `vars` (p. ej. si un parámetro homónimo lo
            // sombrea). Sin esto, una instancia capturada solo por el closure
            // sería barrida por error → corrupción silenciosa.
            if let Some(ref env_rc) = frame.closure_env {
                roots.extend(env_rc.borrow().values().cloned());
            }
        }
        self.gc.collect(&roots);
    }

    /// Pasada final del GC al morir la VM: sin roots, TODO lo registrado se
    /// barre → los ciclos residuales (push(a,a), closures recursivas…) se
    /// rompen y el Rc devuelve cada byte antes de salir del proceso. Es lo
    /// que mantiene a Orion limpio bajo LeakSanitizer (CI job `sanitizer`).
    fn gc_teardown(&mut self) {
        self.gc.collect(&[]);
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
        // Cancelación cooperativa: solo las sub-VMs de `spawn`/`async fn` llevan
        // token. Si se pidió cancelar, abortamos aquí entre instrucciones (punto
        // seguro, igual que el safepoint del GC).
        if let Some(tok) = &self.cancel_token {
            if tok.is_cancelled() {
                return Err("tarea cancelada".to_string());
            }
        }

        // Safepoint del GC: SOLO aquí es seguro colectar. Entre instrucciones
        // ningún local de Rust retiene Values fuera de los roots; colectar en
        // medio de una instrucción (como hacía instantiate_shape) barría la
        // instancia recién creada o los args aún no insertados en el frame.
        if self.gc.should_collect() {
            self.gc_collect();
        }

        // Fin de frame
        {
            let frame = match self.call_stack.last_mut() {
                Some(f) => f,
                None => return Ok(true),
            };
            if frame.ip >= frame.instructions.len() {
                let frame = self.call_stack.pop().unwrap();
                frame.sync_to_instance();
                // Igual que en Return: descartar handlers de frames muertos
                // (un attempt sin EndAttempt alcanzado no debe sobrevivir).
                self.error_handlers.retain(|h| h.frame_depth <= self.call_stack.len());
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
            //    Constantes                                                   
            Instruction::LoadInt(n)   => self.value_stack.push(Value::Int(n)),
            Instruction::LoadFloat(f) => self.value_stack.push(Value::Float(f)),
            Instruction::LoadStr(s)   => self.value_stack.push(Value::Str(s)),
            Instruction::LoadBool(b)  => self.value_stack.push(Value::Bool(b)),
            Instruction::LoadNull     => self.value_stack.push(Value::Null),

            //    Variables                                                    
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

            //    Aritmética                                                   
            Instruction::Add => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(a.add(&b)?); }
            Instruction::Sub => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(a.sub(&b)?); }
            Instruction::Mul => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(a.mul(&b)?); }
            Instruction::Div => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(a.div(&b)?); }
            Instruction::Mod => {
                let b = self.pop()?; let a = self.pop()?;
                match (a, b) {
                    (Value::Int(_), Value::Int(0)) => return Err("Módulo por cero".to_string()),
                    (Value::Int(x), Value::Int(y)) => {
                        let r = x.checked_rem(y)
                            .ok_or("Desbordamiento aritmético en módulo")?;
                        self.value_stack.push(Value::Int(r));
                    }
                    _ => return Err("Módulo solo soporta enteros".to_string()),
                }
            }
            Instruction::Pow => {
                let b = self.pop()?; let a = self.pop()?;
                match (a, b) {
                    (Value::Int(_), Value::Int(y)) if y < 0 =>
                        return Err("Exponente negativo en potencia de enteros (usa flotantes)".to_string()),
                    (Value::Int(x), Value::Int(y)) => {
                        let r = u32::try_from(y).ok()
                            .and_then(|e| x.checked_pow(e))
                            .ok_or("Desbordamiento aritmético en potencia")?;
                        self.value_stack.push(Value::Int(r));
                    }
                    (Value::Float(x), Value::Float(y)) => self.value_stack.push(Value::Float(x.powf(y))),
                    (Value::Int(x), Value::Float(y))   => self.value_stack.push(Value::Float((x as f64).powf(y))),
                    // Mismo cast `as i32` que el runtime JIT (rt_pow) para coincidir bit a bit.
                    (Value::Float(x), Value::Int(y))   => self.value_stack.push(Value::Float(x.powi(y as i32))),
                    _ => return Err("Potencia requiere números".to_string()),
                }
            }
            Instruction::Neg => {
                let a = self.pop()?;
                match a {
                    Value::Int(n)   => {
                        let r = n.checked_neg()
                            .ok_or("Desbordamiento aritmético en negación")?;
                        self.value_stack.push(Value::Int(r));
                    }
                    Value::Float(f) => self.value_stack.push(Value::Float(-f)),
                    _ => return Err("Negación solo aplica a números".to_string()),
                }
            }

            //    Comparación                                                  
            Instruction::Eq    => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(a.compare_eq(&b))); }
            Instruction::NotEq => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(!a.compare_eq(&b))); }
            Instruction::Lt    => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(a.compare_lt(&b)?)); }
            Instruction::LtEq  => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(a.compare_lt(&b)? || a.compare_eq(&b))); }
            Instruction::Gt    => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(!a.compare_lt(&b)? && !a.compare_eq(&b))); }
            Instruction::GtEq  => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(!a.compare_lt(&b)?)); }

            //    Lógica                                                       
            Instruction::And => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(a.is_truthy() && b.is_truthy())); }
            Instruction::Or  => { let b = self.pop()?; let a = self.pop()?; self.value_stack.push(Value::Bool(a.is_truthy() || b.is_truthy())); }
            Instruction::Not => { let a = self.pop()?; self.value_stack.push(Value::Bool(!a.is_truthy())); }

            //    Control de flujo                                             
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

            //    Manejo de errores                                            
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

            //    Funciones                                                    
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
                    // Hotspot counter: registra cuántas veces se llama cada función
                    *self.call_counts.entry(resolved_name.clone()).or_insert(0) += 1;
                    args = self.bind_args_with_defaults(&func, args, &resolved_name)?;
                    let mut frame = CallFrame::with_args_named(
                        func.body, func.lines, &resolved_name, &func.params, args
                    );
                    // Inyectar env capturado (los params tienen prioridad) y
                    // guardar la referencia compartida para el write-back al retornar.
                    if let Some(env_rc) = closure_env {
                        for (k, v) in env_rc.borrow().iter() {
                            frame.vars.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                        frame.closure_env = Some(env_rc);
                    }
                    self.call_stack.push(frame);
                } else if self.extern_fns.contains_key(&resolved_name) {
                    let result = self.call_extern_fn(&resolved_name, args)?;
                    self.value_stack.push(result);
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
                frame.sync_to_closure();
                // Un return dentro de attempt se salta el EndAttempt → su
                // handler quedaba huérfano y un error POSTERIOR en el caller
                // saltaba a una dirección de otra función (corrupción). Los
                // handlers de frames ya muertos se descartan aquí.
                self.error_handlers.retain(|h| h.frame_depth <= self.call_stack.len());
                if self.call_stack.is_empty() {
                    return Ok(true);
                }
            }
            Instruction::Halt => return Ok(true),
            Instruction::Breakpoint => {} // no-op en modo normal; el debugger lo maneja por línea

            //    Módulos                                                       
            Instruction::UseModule(path, alias, selective) => {
                let module_val = self.load_module(&path)?;
                // Import selectivo `take [a, b]`: trae los nombres indicados al
                // scope actual sin cualificar, además de exponer el namespace.
                if !selective.is_empty() {
                    if let Value::Dict(ns) = &module_val {
                        for name in &selective {
                            match ns.get(name) {
                                Some(v) => {
                                    let v = v.clone();
                                    self.call_stack.last_mut().unwrap().vars.insert(name.clone(), v);
                                }
                                None => return Err(format!(
                                    "use ... take: '{}' no existe en el módulo '{}'", name, path
                                )),
                            }
                        }
                    } else {
                        return Err(format!(
                            "use ... take no está soportado para el módulo nativo '{}'; usa '{}.<fn>'",
                            path, alias
                        ));
                    }
                }
                let frame = self.call_stack.last_mut().unwrap();
                frame.vars.insert(alias, module_val);
            }

            //    OOP                                                          
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
                    Value::Module(mod_name) => {
                        let result = crate::modules::call(&mod_name, &attr, vec![])?;
                        self.value_stack.push(eval_to_value(result));
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
                        // Devolver el objeto para el write-back de AssignAttr (StoreVar).
                        self.value_stack.push(Value::Instance(inst_rc));
                    }
                    // Dicts son por valor: insertamos y devolvemos el dict modificado;
                    // AssignAttr lo re-asigna a la variable (igual que v["k"] = val).
                    Value::Dict(mut map) => {
                        map.insert(attr, val);
                        self.value_stack.push(Value::Dict(map));
                    }
                    _ => return Err(format!(
                        "SetAttr '{}': solo se puede asignar atributo a instancias o dicts", attr
                    )),
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
                // Captura el scope actual como entorno compartido de la closure.
                // Rc<RefCell> permite que las mutaciones persistan entre llamadas.
                let env = self.call_stack.last()
                    .map(|f| f.vars.clone())
                    .unwrap_or_default();
                let env_rc = Rc::new(RefCell::new(env));
                // Un env puede ciclarse (closure recursiva que se captura a sí
                // misma vía write-back); registrarlo permite al GC romperlo.
                self.gc.register_env(&env_rc);
                self.value_stack.push(Value::Closure { fn_name, env: env_rc });
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
            Instruction::CallSuper(method_name, argc) => {
                let mut args: Vec<Value> = (0..argc)
                    .map(|_| self.pop())
                    .collect::<Result<Vec<_>, _>>()?;
                args.reverse();

                // self_instance del act actual
                let inst_rc = self.call_stack.last()
                    .and_then(|f| f.self_instance.clone())
                    .ok_or("super solo puede usarse dentro de un act de shape")?;
                let shape_name = inst_rc.borrow().shape_name.clone();
                let parents = self.shapes.get(&shape_name)
                    .map(|s| s.using.clone())
                    .ok_or_else(|| format!("Shape '{}' no encontrado", shape_name))?;
                // Buscar el método SOLO en los shapes padre (vía using).
                let act = parents.iter()
                    .find_map(|p| self.find_act(p, &method_name))
                    .cloned()
                    .ok_or_else(|| format!(
                        "super.{}(): no se encontró en los padres de '{}'",
                        method_name, shape_name
                    ))?;
                if args.len() != act.params.len() {
                    return Err(format!(
                        "super.{}() espera {} argumento(s), recibió {}",
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
            Instruction::CallMethod(method_name, argc) => {
                let mut args: Vec<Value> = (0..argc)
                    .map(|_| self.pop())
                    .collect::<Result<Vec<_>, _>>()?;
                args.reverse();

                let obj = self.pop()?;
                match obj {
                    //    Métodos de String                                 
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
                                let sep_str = sep.to_string();
                                // split("") → caracteres (sin strings vacíos en los bordes)
                                let parts: Vec<Value> = if sep_str.is_empty() {
                                    s.chars().map(|c| Value::Str(c.to_string())).collect()
                                } else {
                                    s.split(sep_str.as_str())
                                        .map(|p| Value::Str(p.to_string()))
                                        .collect()
                                };
                                Value::list(parts)
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

                    //    Métodos de List
                    // `list` es el backing compartido (Rc<RefCell<Vec>>). Los
                    // métodos mutadores (push/append/reverse/sort) modifican el
                    // Vec in-place y devuelven el MISMO Rc → el alias original ve
                    // el cambio. Los transformadores (map/filter/reduce) producen
                    // listas NUEVAS. Las lecturas usan borrow().
                    Value::List(list) => {
                        let result = match method_name.as_str() {
                            "len"      => Value::Int(list.borrow().len() as i64),
                            "is_empty" => Value::Bool(list.borrow().is_empty()),
                            "push" | "append" => {
                                let item = args.into_iter().next()
                                    .ok_or("list.push() requiere 1 argumento")?;
                                // Guardar un contenedor dentro de la lista puede
                                // cerrar un ciclo → registrarla para el GC.
                                if crate::gc::is_container(&item) {
                                    self.gc.register_list(&list);
                                }
                                list.borrow_mut().push(item);
                                Value::List(list)
                            }
                            "first" => list.borrow().first().cloned().unwrap_or(Value::Null),
                            "last"  => list.borrow().last().cloned().unwrap_or(Value::Null),
                            // Quita y devuelve el último elemento, mutando in-place
                            // (contrato estándar). Antes `a.pop()` como método daba
                            // error en la VM (solo existía el builtin pop(a)).
                            "pop" => list.borrow_mut().pop().unwrap_or(Value::Null),
                            "reverse" => { list.borrow_mut().reverse(); Value::List(list) }
                            "contains" => {
                                let item = args.into_iter().next()
                                    .ok_or("list.contains() requiere 1 argumento")?;
                                Value::Bool(list.borrow().contains(&item))
                            }
                            "join" => {
                                let sep = match args.into_iter().next() {
                                    Some(Value::Str(s)) => s,
                                    None => String::new(),
                                    Some(v) => v.to_string(),
                                };
                                Value::Str(list.borrow().iter().map(|v| v.to_string()).collect::<Vec<_>>().join(&sep))
                            }
                            "map" => {
                                let cb = args.into_iter().next()
                                    .ok_or("list.map() requiere una función/lambda")?;
                                let items = list.borrow().0.clone();
                                let mut out = Vec::with_capacity(items.len());
                                for item in items {
                                    let r = self.call_value(cb.clone(), vec![item])?;
                                    out.push(r);
                                }
                                Value::list(out)
                            }
                            "filter" => {
                                let cb = args.into_iter().next()
                                    .ok_or("list.filter() requiere una función/lambda")?;
                                let items = list.borrow().0.clone();
                                let mut out = Vec::new();
                                for item in items {
                                    let r = self.call_value(cb.clone(), vec![item.clone()])?;
                                    if r.is_truthy() { out.push(item); }
                                }
                                Value::list(out)
                            }
                            "reduce" => {
                                let mut it = args.into_iter();
                                let cb  = it.next().ok_or("list.reduce() requiere función y acumulador")?;
                                let acc = it.next().ok_or("list.reduce() requiere acumulador inicial")?;
                                let mut acc = acc;
                                let items = list.borrow().0.clone();
                                for item in items {
                                    acc = self.call_value(cb.clone(), vec![acc, item])?;
                                }
                                acc
                            }
                            "sort" => {
                                list.borrow_mut().sort_by(|a, b| match (a, b) {
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
                                for v in list.borrow().iter() {
                                    match v {
                                        Value::Int(n)   => total += *n as f64,
                                        Value::Float(n) => { total += n; is_int = false; }
                                        _ => {}
                                    }
                                }
                                if is_int { Value::Int(total as i64) } else { Value::Float(total) }
                            }
                            "min" => list.borrow().iter().cloned().reduce(|a, b| match (&a, &b) {
                                (Value::Int(x), Value::Int(y))     => if x <= y { a } else { b },
                                (Value::Float(x), Value::Float(y)) => if x <= y { a } else { b },
                                _ => a,
                            }).unwrap_or(Value::Null),
                            "max" => list.borrow().iter().cloned().reduce(|a, b| match (&a, &b) {
                                (Value::Int(x), Value::Int(y))     => if x >= y { a } else { b },
                                (Value::Float(x), Value::Float(y)) => if x >= y { a } else { b },
                                _ => a,
                            }).unwrap_or(Value::Null),
                            _ => return Err(format!("List no tiene método '{}'", method_name)),
                        };
                        self.value_stack.push(result);
                    }

                    //    Métodos de Dict                                   
                    Value::Dict(map) => {
                        // Una función definida en el dict (p.ej. un namespace de módulo:
                        // list.contains, list.get, ...) tiene prioridad sobre los métodos
                        // nativos de dict del mismo nombre. Así un paquete puede exponer
                        // `contains`, `get`, `keys`, etc. sin que el método nativo lo eclipse.
                        let user_fn = map.get(method_name.as_str()).cloned().filter(|v| match v {
                            Value::Closure { .. } => true,
                            Value::Str(s) => self.functions.contains_key(s),
                            _ => false,
                        });
                        if let Some(fn_val) = user_fn {
                            let result = self.call_value(fn_val, args)?;
                            self.value_stack.push(result);
                            return Ok(false);
                        }
                        // Si no, probar métodos builtin de dict
                        match method_name.as_str() {
                            "len"      => { self.value_stack.push(Value::Int(map.len() as i64)); }
                            "is_empty" => { self.value_stack.push(Value::Bool(map.is_empty())); }
                            "keys"     => { self.value_stack.push(Value::list(map.keys().map(|k| Value::Str(k.clone())).collect())); }
                            "values"   => { self.value_stack.push(Value::list(map.values().cloned().collect())); }
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

                        // Si el shape define on_error, envolvemos el act: un error no
                        // capturado dentro del act invoca on_error(err) en vez de propagar.
                        if let Some(on_error) = self.find_on_error(&shape_name).cloned() {
                            match self.run_act_isolated(&inst_rc, &act, args) {
                                Ok(v) => self.value_stack.push(v),
                                Err(e) => {
                                    let oe_args = if on_error.params.is_empty() {
                                        vec![]
                                    } else {
                                        vec![Value::Str(e)]
                                    };
                                    let r = self.run_act_isolated(&inst_rc, &on_error, oe_args)?;
                                    self.value_stack.push(r);
                                }
                            }
                        } else {
                            // Camino normal (sin on_error): el frame corre en el loop principal.
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
                    }
                    //    Módulos stdlib nativos
                    Value::Module(mod_name) => {
                        // excel.compute necesita llamar closures desde la VM
                        if mod_name == "excel" && method_name == "compute" {
                            let result = self.excel_compute(args)?;
                            self.value_stack.push(result);
                        } else {
                            let eval_args: Vec<crate::eval_value::EvalValue> =
                                args.into_iter().map(value_to_eval).collect();
                            let result = crate::modules::call(&mod_name, &method_name, eval_args)?;
                            self.value_stack.push(eval_to_value(result));
                        }
                    }
                    //    Métodos de Task (tarea async lanzada con spawn/async fn)
                    Value::Task(handle) => {
                        let result = match method_name.as_str() {
                            // t.cancelar() → pide cancelación cooperativa; la sub-VM
                            // aborta en su próximo punto seguro. No bloquea.
                            "cancelar" | "cancel" => {
                                handle.cancel();
                                Value::Bool(true)
                            }
                            // t.lista() / t.terminada() → ¿ya terminó? (no bloquea)
                            "lista" | "terminada" | "is_done" | "done" =>
                                Value::Bool(handle.is_done()),
                            // t.esperar() → equivalente a `await t` como método.
                            "esperar" | "await" | "resultado" => {
                                match handle.wait() {
                                    Ok(sv) => from_send(sv),
                                    Err(e) => return Err(e),
                                }
                            }
                            other => return Err(format!("tarea.{}() no existe", other)),
                        };
                        self.value_stack.push(result);
                    }
                    _ => return Err(format!("CallMethod '{}': no es una instancia", method_name)),
                }
            }

            //    Colecciones                                                  
            Instruction::MakeList(n) => {
                let mut items: Vec<Value> = (0..n).map(|_| self.pop()).collect::<Result<Vec<_>, _>>()?;
                items.reverse();
                self.value_stack.push(Value::list(items));
            }
            Instruction::MakeDict(n) => {
                // los pares salen de la pila en orden inverso al del literal;
                // se voltean para que el dict conserve el orden escrito por el dev
                let mut pairs = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    let val = self.pop()?;
                    let key = match self.pop()? {
                        Value::Str(s) => s,
                        other => other.to_string(),
                    };
                    pairs.push((key, val));
                }
                let mut map = IndexMap::with_capacity(n as usize);
                for (key, val) in pairs.into_iter().rev() {
                    map.insert(key, val);
                }
                self.value_stack.push(Value::Dict(map));
            }
            Instruction::GetIndex => {
                let idx = self.pop()?;
                let obj = self.pop()?;
                match (obj, idx) {
                    (Value::List(items), Value::Int(i)) => {
                        let items = items.borrow();
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
                    (Value::List(items), Value::Int(i)) => {
                        if crate::gc::is_container(&val) {
                            self.gc.register_list(&items);
                        }
                        {
                            let mut items_mut = items.borrow_mut();
                            let i_usize = if i < 0 {
                                (items_mut.len() as i64 + i) as usize
                            } else {
                                i as usize
                            };
                            if i_usize >= items_mut.len() {
                                return Err(format!("Índice {} fuera de rango en SetIndex", i));
                            }
                            items_mut[i_usize] = val;
                        }
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

            //    Stack                                                         
            Instruction::Pop => { self.pop()?; }
            Instruction::Dup => {
                let top = self.value_stack.last().cloned().ok_or("Stack vacío en Dup")?;
                self.value_stack.push(top);
            }

            //    Async                                                         
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

                let functions_clone  = self.functions.clone();
                let shapes_clone     = self.shapes.clone();
                let extern_fns_clone = self.extern_fns.clone();
                let fn_name_clone    = fn_name.clone();

                // Bindings de módulo (`use "chan" as chan`) visibles en la pila
                // actual: se reinyectan en la sub-VM para que la función lanzada
                // pueda usar módulos (canales, state, etc.), igual que en `serve`.
                let module_bindings: Vec<(String, String)> = self.call_stack.iter()
                    .flat_map(|f| f.vars.iter())
                    .filter_map(|(k, v)| match v {
                        Value::Module(m) => Some((k.clone(), m.clone())),
                        _ => None,
                    })
                    .collect();

                // Handle compartido: parking (Condvar) para await + flag de cancelación.
                let handle = TaskHandle::new();
                let handle_worker = Arc::clone(&handle);

                // La tarea corre en el POOL de hilos (reutiliza workers), no en un
                // hilo nuevo por spawn. El pool crea uno bajo demanda si hace falta,
                // así spawn/await anidados nunca hacen deadlock.
                crate::task_pool::submit(move || {
                    // Si ya la cancelaron antes de arrancar, no ejecutamos nada.
                    if handle_worker.is_cancelled() {
                        handle_worker.complete(Err("tarea cancelada".to_string()));
                        return;
                    }
                    let mut sub_vm = VM::new(
                        func.body.clone(),
                        func.lines.clone(),
                        functions_clone,
                        shapes_clone,
                        extern_fns_clone,
                    );
                    // La sub-VM consulta este token en su bucle para abortar limpiamente.
                    sub_vm.cancel_token = Some(Arc::clone(&handle_worker));
                    // Reinyectar módulos visibles (para chan/state/etc. dentro de la tarea)
                    for (k, m) in &module_bindings {
                        if let Some(frame) = sub_vm.call_stack.first_mut() {
                            frame.vars.insert(k.clone(), Value::Module(m.clone()));
                        }
                    }
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
                    handle_worker.complete(result);
                });

                self.value_stack.push(Value::Task(handle));
            }

            Instruction::Await => {
                let val = self.pop()?;
                match val {
                    Value::Task(handle) => {
                        // Parking real: el hilo se aparca en el Condvar y el SO lo
                        // despierta cuando la tarea termina. Sin espera activa.
                        match handle.wait() {
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

            //    I/O
            Instruction::Show => {
                let val = self.pop()?;
                write_utf8_line(&format!("{}", val));
            }

            //    IO nativo: ask / read / write / env                            
            Instruction::ReadInput { cast, choices } => {
                let prompt = self.pop()?;

                // Si hay choices, están en el stack debajo del prompt (ya extraímos prompt)
                let choices_list: Option<Vec<Value>> = if choices {
                    let c = self.pop()?;
                    if let Value::List(v) = c { Some(v.borrow().0.clone()) } else { None }
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
                        Value::list(lines)
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

            //    IA nativa (Fase 4) — delega en crate::ai, igual que el módulo
            //    `ai`, para compartir memoria de sesión, modelo y proveedor.
            Instruction::AiAsk => {
                let prompt = self.pop()?;
                let response = crate::ai::think(&prompt.to_string())?;
                self.value_stack.push(Value::Str(response));
            }

            Instruction::AiLearn => {
                let text = self.pop()?;
                let msg = crate::ai::learn(&text.to_string());
                self.value_stack.push(Value::Str(msg));
            }

            Instruction::AiSense => {
                let query = self.pop()?;
                let response = crate::ai::sense(&query.to_string())?;
                self.value_stack.push(Value::Str(response));
            }

            //    Servidor HTTP nativo (Fase 7)                               
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

        // Si el path referencia explícitamente un archivo (p.ej. "packages/validate"),
        // el .orx tiene prioridad sobre el módulo nativo del mismo nombre. Así
        // `use "validate"` carga el módulo nativo y `use "packages/validate"` carga
        // el paquete instalado, sin que uno eclipse silenciosamente al otro.
        let explicit_path = path.contains('/') || path.contains('\\');

        // Candidatos de archivo .orx en packages/ o ruta relativa
        let orx_candidates = [
            format!("packages/{}.orx", path),
            format!("{}.orx", path),
            format!("lib/{}.orx", path),
        ];

        if explicit_path {
            for candidate in &orx_candidates {
                if std::path::Path::new(candidate).exists() {
                    return self.load_orx_module(candidate, base_name, &prefix);
                }
            }
        }

        // 1) Módulos builtin Rust tienen prioridad sobre archivos (para imports por nombre)
        match base_name {
            "math" => return Ok(self.builtin_math_module()),
            name if crate::modules::is_known_module(name) => {
                return Ok(Value::Module(name.to_string()));
            }
            _ => {}
        }

        // 2) Buscar archivo .orx (fallback para imports por nombre sin módulo nativo)
        for candidate in &orx_candidates {
            if std::path::Path::new(candidate).exists() {
                return self.load_orx_module(candidate, base_name, &prefix);
            }
        }

        Err(format!("Módulo '{}' no encontrado", path))
    }

    /// Carga un módulo .orx: compila, ejecuta en sub-VM, extrae vars y fns en un dict.
    fn load_orx_module(&mut self, path: &str, _module_name: &str, prefix: &str) -> Result<Value, String> {
        use crate::lexer::lex;
        use crate::parser::parse;
        use crate::codegen::compile;

        let src = std::fs::read_to_string(path)
            .map_err(|e| format!("No se pudo leer '{}': {}", path, e))?;
        let tokens = lex(&src).map_err(|e| format!("Error lexando '{}': {:?}", path, e))?;
        let ast = parse(tokens).map_err(|e| format!("Error parseando '{}': {:?}", path, e))?;
        let bc = compile(ast).map_err(|e| format!("Error compilando '{}': {:?}", path, e))?;

        use std::collections::HashSet;
        let own_fns: HashSet<String> = bc.functions.keys().cloned().collect();

        // 1) Ejecutar el módulo en una sub-VM para obtener sus variables/constantes
        //    globales y los namespaces que haya importado (p.ej. `use "datetime"`).
        let mut sub_vm = VM::new(bc.main.clone(), bc.lines.clone(), bc.functions.clone(), bc.shapes.clone(), bc.extern_fns.clone());
        sub_vm.run().ok(); // ignorar errores de side effects
        let mut module_globals: IndexMap<String, Value> = IndexMap::new();
        if let Some(frame) = sub_vm.call_stack.first() {
            for (k, v) in &frame.vars {
                if !k.starts_with('_') {
                    module_globals.insert(k.clone(), v.clone());
                }
            }
        }
        let global_names: HashSet<String> = module_globals.keys().cloned().collect();

        // 2) Inyectar las globales del módulo en el frame global de la VM principal,
        //    con prefijo para no colisionar con otros módulos ni con el programa.
        if let Some(main_frame) = self.call_stack.first_mut() {
            for (k, v) in &module_globals {
                main_frame.vars.insert(format!("{}{}", prefix, k), v.clone());
            }
        }

        // 3) Registrar las funciones del módulo con prefijo. Se reescriben:
        //    - llamadas internas entre funciones del propio módulo (recursión/helpers)
        //    - referencias a globales/constantes del módulo (salvo que un parámetro o
        //      variable local las haga sombra)
        let mut ns: IndexMap<String, Value> = IndexMap::new();
        for (fname, fdef) in &bc.functions {
            let prefixed = format!("{}{}", prefix, fname);
            let mut fdef = fdef.clone();

            // Nombres locales de la función: parámetros + destinos de StoreVar
            let mut locals: HashSet<String> = fdef.params.iter().cloned().collect();
            for instr in &fdef.body {
                if let Instruction::StoreVar(n) = instr { locals.insert(n.clone()); }
            }

            for instr in &mut fdef.body {
                match instr {
                    Instruction::Call(callee, _) if own_fns.contains(callee.as_str()) => {
                        *callee = format!("{}{}", prefix, callee);
                    }
                    Instruction::LoadVar(n)
                        if global_names.contains(n.as_str()) && !locals.contains(n.as_str()) =>
                    {
                        *n = format!("{}{}", prefix, n);
                    }
                    _ => {}
                }
            }
            ns.insert(fname.clone(), Value::Str(prefixed.clone()));
            self.functions.insert(prefixed, fdef);
        }

        // 4) Exponer también las globales/constantes en el namespace del módulo.
        for (k, v) in module_globals {
            ns.insert(k, v);
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

        for (fname, _params) in math_fns {
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
        let args = self.bind_args_with_defaults(&func_def, args, &fn_name)?;
        let stack_depth = self.call_stack.len();
        let mut frame = CallFrame::with_args_named(
            func_def.body, func_def.lines, &fn_name, &func_def.params, args
        );
        if let Some(env_rc) = closure_env {
            for (k, v) in env_rc.borrow().iter() {
                frame.vars.entry(k.clone()).or_insert_with(|| v.clone());
            }
            frame.closure_env = Some(env_rc);
        }
        self.call_stack.push(frame);
        loop {
            if self.call_stack.len() <= stack_depth { break; }
            let done = self.step()?;
            if done { break; }
        }
        Ok(self.value_stack.pop().unwrap_or(Value::Null))
    }

    // excel.compute(data, { "col": fn(row) { expr }, ... }) → list con nuevas columnas
    fn excel_compute(&mut self, args: Vec<Value>) -> Result<Value, String> {
        let data = match args.get(0) {
            Some(Value::List(l)) => l.borrow().0.clone(),
            Some(other) => return Err(format!(
                "excel.compute: primer argumento debe ser lista, se recibió {:?}", other
            )),
            None => return Err("excel.compute: requiere (data, spec)".into()),
        };
        let spec = match args.get(1) {
            Some(Value::Dict(m)) => m.clone(),
            Some(other) => return Err(format!(
                "excel.compute: segundo argumento debe ser dict de funciones, se recibió {:?}", other
            )),
            None => return Err("excel.compute: requiere (data, spec)".into()),
        };

        let col_names: Vec<String> = spec.keys().cloned().collect(); // IndexMap → insertion order

        let mut result = Vec::with_capacity(data.len());
        for row in &data {
            let mut new_row = match row {
                Value::Dict(m) => m.clone(),
                other => return Err(format!(
                    "excel.compute: cada fila debe ser un dict, se recibió {:?}", other
                )),
            };
            for col_name in &col_names {
                let fn_val = spec[col_name].clone();
                let computed = self.call_value(fn_val, vec![row.clone()])?;
                new_row.insert(col_name.clone(), computed);
            }
            result.push(Value::Dict(new_row));
        }
        Ok(Value::list(result))
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
            "round"     => {
                let f = to_f64(&args[0])?;
                match args.get(1) {
                    Some(d) => {
                        let digits = to_f64(d)? as i32;
                        let factor = 10_f64.powi(digits);
                        Ok(Value::Float((f * factor).round() / factor))
                    }
                    None => Ok(Value::Int(f.round() as i64)),
                }
            }
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
        // Solo registrar: colectar AQUÍ corrompía — inst_rc y args viven en
        // locals de Rust, fuera de los roots, y el sweep les vaciaba los
        // fields (cada instancia nº 512 nacía sin campos). La colección
        // ocurre en el safepoint de step().
        self.gc.register(&inst_rc);

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

    /// Ajusta los argumentos a la aridad de la función: si faltan, rellena la
    /// cola con los valores por defecto (mini-bytecode). Error si sobran o si un
    /// parámetro obligatorio quedó sin valor.
    fn bind_args_with_defaults(
        &mut self,
        func: &FunctionDef,
        mut args: Vec<Value>,
        name: &str,
    ) -> Result<Vec<Value>, String> {
        let n = func.params.len();
        if args.len() > n {
            return Err(format!("'{}' espera {} argumento(s), recibió {}", name, n, args.len()));
        }
        while args.len() < n {
            let i = args.len();
            match func.param_defaults.get(i).and_then(|d| d.as_ref()) {
                Some(instrs) => {
                    let v = self.eval_mini_bytecode(instrs)?;
                    args.push(v);
                }
                None => {
                    return Err(format!("'{}' espera {} argumento(s), recibió {}", name, n, args.len()));
                }
            }
        }
        Ok(args)
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

    /// on_error del shape (hook de error). Hereda del padre vía `using` si el
    /// shape propio no lo define.
    fn find_on_error(&self, shape_name: &str) -> Option<&crate::bytecode::ActDef> {
        let shape = self.shapes.get(shape_name)?;
        if shape.on_error.is_some() {
            return shape.on_error.as_ref();
        }
        for parent in &shape.using {
            if let Some(oe) = self.find_on_error(parent) {
                return Some(oe);
            }
        }
        None
    }

    /// Ejecuta un act de una instancia de forma síncrona y AÍSLA los manejadores
    /// de error externos: si el act lanza un error no capturado internamente,
    /// se devuelve `Err` en vez de saltar a un `attempt/handle` externo. Lo usa
    /// el hook `on_error` para envolver la ejecución de los acts.
    fn run_act_isolated(
        &mut self,
        inst_rc: &Rc<RefCell<InstanceData>>,
        act: &crate::bytecode::ActDef,
        args: Vec<Value>,
    ) -> Result<Value, String> {
        let mut frame = CallFrame::new(act.body.clone(), act.lines.clone());
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
        frame.self_instance = Some(Rc::clone(inst_rc));
        frame.instance_fields = field_names;

        let call_base   = self.call_stack.len();
        let vstack_base = self.value_stack.len();
        // Oculta los handlers externos durante la ejecución del act.
        let saved_handlers = std::mem::take(&mut self.error_handlers);
        self.call_stack.push(frame);

        let mut err: Option<String> = None;
        while self.call_stack.len() > call_base {
            match self.step() {
                Ok(true)  => break,          // Halt (no debería ocurrir en un act)
                Ok(false) => {}
                Err(e)    => { err = Some(e); break; }
            }
        }

        // Restaura los handlers externos y limpia frames residuales del act.
        self.error_handlers = saved_handlers;
        while self.call_stack.len() > call_base {
            let f = self.call_stack.pop().unwrap();
            f.sync_to_instance();
        }

        if let Some(e) = err {
            self.value_stack.truncate(vstack_base);
            return Err(e);
        }
        let result = if self.value_stack.len() > vstack_base {
            self.value_stack.pop().unwrap()
        } else {
            Value::Null
        };
        self.value_stack.truncate(vstack_base);
        Ok(result)
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
        use tiny_http::Server;

        let addr = format!("0.0.0.0:{}", port);
        let server = Arc::new(Server::http(&addr)
            .map_err(|e| format!("serve: no se pudo iniciar el servidor en {}: {}", addr, e))?);

        // Validar el handler una sola vez antes de levantar los hilos.
        let func = self.functions.get(&fn_name).cloned()
            .ok_or_else(|| format!("serve: handler '{}' no encontrado", fn_name))?;
        if func.params.len() != 1 {
            return Err(format!("serve: handler '{}' debe tener exactamente 1 parámetro (req)", fn_name));
        }

        // Snapshot de las variables globales (frame <main>) para sembrar cada
        // worker (config/datos de solo lectura). Los Value normales viajan como
        // SendValue (thread-safe); los módulos importados con `use` se llevan
        // aparte porque SendValue no tiene variante Module y se re-registran como
        // Value::Module en el worker. Las mutaciones de estado compartido van por
        // el módulo `state`.
        let (globals, module_globals): (Vec<(String, SendValue)>, Vec<(String, String)>) = {
            let mut vals = Vec::new();
            let mut mods = Vec::new();
            if let Some(f) = self.call_stack.first() {
                for (k, v) in &f.vars {
                    match v {
                        Value::Module(m) => mods.push((k.clone(), m.clone())),
                        other => if let Ok(sv) = other.to_send() { vals.push((k.clone(), sv)); }
                    }
                }
            }
            (vals, mods)
        };

        // Pool de hilos: tantos workers como CPUs (acotado a [2, 16]). Cada uno
        // tiene su PROPIA VM construida desde el blueprint del programa (bytecode,
        // que es Send); los Value (Rc) nunca cruzan hilos.
        let n_workers = std::thread::available_parallelism()
            .map(|n| n.get()).unwrap_or(4).clamp(2, 16);

        eprintln!("[Orion] Servidor escuchando en http://{}  ({} hilos · Ctrl+C para detener)", addr, n_workers);

        let mut handles = Vec::with_capacity(n_workers);
        for _ in 0..n_workers {
            let server      = Arc::clone(&server);
            let functions   = self.functions.clone();
            let shapes      = self.shapes.clone();
            let extern_fns  = self.extern_fns.clone();
            let globals     = globals.clone();
            let modules     = module_globals.clone();
            let fn_name     = fn_name.clone();
            handles.push(std::thread::spawn(move || {
                let mut vm = VM::new(Vec::new(), Vec::new(), functions, shapes, extern_fns);
                if let Some(main_frame) = vm.call_stack.first_mut() {
                    for (k, sv) in &globals {
                        main_frame.vars.insert(k.clone(), from_send(sv.clone()));
                    }
                    for (k, m) in &modules {
                        main_frame.vars.insert(k.clone(), Value::Module(m.clone()));
                    }
                }
                // Bucle de aceptación: varios hilos llaman recv() sobre el mismo
                // Server (tiny_http reparte los requests entre ellos).
                loop {
                    match server.recv() {
                        Ok(request) => vm.handle_http_request(request, &fn_name),
                        Err(_) => break,
                    }
                }
            }));
        }
        for h in handles { let _ = h.join(); }
        Ok(())
    }

    /// Ejecuta el handler `fn_name` con el dict `req` y devuelve su valor de
    /// retorno. Si falla, deja los stacks limpios para no contaminar el siguiente
    /// request del mismo worker.
    fn run_handler(&mut self, fn_name: &str, req: Value) -> Result<Value, String> {
        let func = self.functions.get(fn_name).cloned()
            .ok_or_else(|| format!("handler '{}' no encontrado", fn_name))?;
        let frame = CallFrame::with_args(func.body, func.lines, &func.params, vec![req]);
        self.call_stack.push(frame);
        match self.run_until_frame_done() {
            Ok(()) => Ok(self.value_stack.pop().unwrap_or(Value::Null)),
            Err(e) => {
                // Restaurar el worker a un estado limpio tras un error de handler.
                self.call_stack.truncate(1);
                self.value_stack.clear();
                self.error_handlers.clear();
                Err(e)
            }
        }
    }

    /// Construye el dict `req`, rutea (router activo primero, handler global
    /// como fallback), ejecuta middlewares y responde. Un error del handler se
    /// traduce a 500 y el worker sigue atendiendo (no tumba el server).
    ///
    /// El dict `req` expone: path, method, body, headers (claves en minúscula),
    /// ip, query (URL-decoded) y params (query + parámetros de ruta `:id`).
    fn handle_http_request(&mut self, mut request: tiny_http::Request, fn_name: &str) {
        use tiny_http::{Response, Header};
        use std::str::FromStr;

        let url = request.url().to_string();
        let method = request.method().to_string();

        // Log de acceso con estilo: hora local, método, ruta, status coloreado y latencia.
        let __t0 = std::time::Instant::now();
        let __log_m = method.clone();
        let __log_u = url.clone();
        let log_req = move |status: u16, note: &str| {
            let ts = chrono::Local::now().format("%H:%M:%S").to_string();
            let color = match status {
                200..=299 => "\x1b[32m", // verde
                300..=399 => "\x1b[36m", // cian
                400..=499 => "\x1b[33m", // amarillo
                _         => "\x1b[31m", // rojo
            };
            let ms = __t0.elapsed().as_millis();
            let extra = if note.is_empty() { String::new() } else { format!(" ({})", note) };
            eprintln!(
                "\x1b[2m[Orion]\x1b[0m \x1b[2m{}\x1b[0m  {:<7}{}  {}{}\x1b[0m \x1b[2m{}ms\x1b[0m{}",
                ts, __log_m, __log_u, color, status, ms, extra
            );
        };

        let (raw_path, query) = if let Some(pos) = url.find('?') {
            (url[..pos].to_string(), url[pos+1..].to_string())
        } else {
            (url.clone(), String::new())
        };
        let path = url_decode(&raw_path, false);

        // Query params URL-decoded ('+' cuenta como espacio solo en la query)
        let mut query_map = IndexMap::new();
        for pair in query.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                query_map.insert(url_decode(k, true), Value::Str(url_decode(v, true)));
            } else if !pair.is_empty() {
                query_map.insert(url_decode(pair, true), Value::Str(String::new()));
            }
        }

        // Headers del request, claves normalizadas a minúscula
        let mut headers_map = IndexMap::new();
        for h in request.headers() {
            headers_map.insert(
                h.field.as_str().as_str().to_lowercase(),
                Value::Str(h.value.as_str().to_string()),
            );
        }

        let client_ip = request.remote_addr()
            .map(|a| a.ip().to_string())
            .unwrap_or_default();

        // Body leído como BYTES: soporta subidas binarias (imágenes, multipart)
        // sin corromper. body_str es la vista UTF-8 (lossy) para texto/JSON.
        let mut body_bytes: Vec<u8> = Vec::new();
        { request.as_reader().read_to_end(&mut body_bytes).ok(); }
        let body_str = String::from_utf8_lossy(&body_bytes).into_owned();

        // multipart/form-data → campos de texto en `form` y archivos en `files`
        // (cada archivo se vuelca a un temporal; el handler lo mueve con fs.move).
        let (form_map, files_list) = {
            let ct = headers_map.get("content-type")
                .and_then(|v| if let Value::Str(s) = v { Some(s.as_str()) } else { None })
                .unwrap_or("");
            if ct.starts_with("multipart/form-data") {
                match multipart_boundary(ct) {
                    Some(b) => parse_multipart(&body_bytes, &b),
                    None => (IndexMap::new(), Vec::new()),
                }
            } else {
                (IndexMap::new(), Vec::new())
            }
        };

        // Router activo primero; si no matchea, cae al handler global de serve.
        let routed = crate::modules::router_mod::active_match(&method, &path);

        // Archivos estáticos (solo GET/HEAD y solo si ninguna ruta matcheó):
        // MIME automático, index.html en directorios, anti path-traversal.
        if routed.is_none() && (method == "GET" || method == "HEAD") {
            if let Some((dir, rest)) = crate::modules::router_mod::active_static(&path) {
                let (st, bytes, ct): (u16, Vec<u8>, String) = match resolve_static(&dir, &rest) {
                    Some((bytes, mime)) => (200, bytes, mime),
                    None => (404, b"archivo no encontrado".to_vec(), "text/plain; charset=utf-8".into()),
                };
                let mut response = Response::from_data(bytes).with_status_code(st);
                if let Ok(h) = Header::from_str(&format!("Content-Type: {}", ct)) {
                    response = response.with_header(h);
                }
                if let Ok(h) = Header::from_str("X-Content-Type-Options: nosniff") {
                    response = response.with_header(h);
                }
                log_req(st, "");
                let _ = request.respond(response);
                return;
            }
        }

        // router.guard: prefijos protegidos con JWT Bearer — sin token válido
        // responde 401 solo; con token válido req["user"] lleva los claims.
        let mut guard_user: Option<Value> = None;
        if let Some(secret) = crate::modules::router_mod::active_guard(&path) {
            let token = headers_map.get("authorization")
                .and_then(|v| match v { Value::Str(s) => Some(s.clone()), _ => None })
                .and_then(|s| {
                    if s.len() > 7 && s[..7].eq_ignore_ascii_case("bearer ") {
                        Some(s[7..].trim().to_string())
                    } else { None }
                });
            let claims = token.and_then(|t| {
                use crate::eval_value::EvalValue as E;
                match crate::modules::auth_mod::call(
                    "verificar_token", vec![E::Str(t), E::Str(secret)],
                ) {
                    Ok(v) => {
                        let val = eval_to_value(v);
                        match &val {
                            Value::Dict(m) if matches!(m.get("valido"), Some(Value::Bool(false))) => None,
                            _ => Some(val),
                        }
                    }
                    Err(_) => None,
                }
            });
            match claims {
                Some(u) => guard_user = Some(u),
                None => {
                    let mut response = Response::from_string("{\"error\":\"no autorizado\"}")
                        .with_status_code(401);
                    if let Ok(h) = Header::from_str("Content-Type: application/json; charset=utf-8") {
                        response = response.with_header(h);
                    }
                    if let Ok(h) = Header::from_str("WWW-Authenticate: Bearer") {
                        response = response.with_header(h);
                    }
                    log_req(401, "guard");
                    let _ = request.respond(response);
                    return;
                }
            }
        }

        let (target_fn, route_params, middlewares) = match &routed {
            Some(m) => (m.handler.as_str(), m.params.clone(), m.middlewares.clone()),
            None    => (fn_name, Vec::new(), Vec::new()),
        };

        // params = query + parámetros de ruta (la ruta gana en colisión)
        let mut params = query_map.clone();
        for (k, v) in &route_params {
            params.insert(k.clone(), Value::Str(v.clone()));
        }

        // cookies: header Cookie parseado a Dict (nombre → valor)
        let mut cookies_map = IndexMap::new();
        if let Some(Value::Str(raw)) = headers_map.get("cookie") {
            for part in raw.split(';') {
                if let Some((k, v)) = part.split_once('=') {
                    cookies_map.insert(k.trim().to_string(), Value::Str(v.trim().to_string()));
                }
            }
        }

        // Sesión: reutiliza el sid de la cookie orion_sid o genera uno nuevo.
        // Se expone en req["sid"]; el Set-Cookie solo se emite (abajo) si el
        // handler guardó datos con session.set(req["sid"], ...).
        let incoming_sid = cookies_map.get("orion_sid")
            .and_then(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None });
        let sid = incoming_sid.clone()
            .unwrap_or_else(crate::modules::session_mod::gen_sid);

        // json: body pre-parseado si parece JSON (Null si no aplica o no parsea)
        let body_json = {
            let t = body_str.trim_start();
            if t.starts_with('{') || t.starts_with('[') {
                serde_json::from_str::<serde_json::Value>(&body_str)
                    .map(json_to_value).unwrap_or(Value::Null)
            } else { Value::Null }
        };

        let mut req_map = IndexMap::new();
        req_map.insert("path".to_string(),    Value::Str(path));
        req_map.insert("method".to_string(),  Value::Str(method));
        req_map.insert("body".to_string(),    Value::Str(body_str));
        req_map.insert("headers".to_string(), Value::Dict(headers_map));
        req_map.insert("ip".to_string(),      Value::Str(client_ip));
        req_map.insert("query".to_string(),   Value::Dict(query_map));
        req_map.insert("params".to_string(),  Value::Dict(params));
        req_map.insert("cookies".to_string(), Value::Dict(cookies_map));
        req_map.insert("json".to_string(),    body_json);
        req_map.insert("form".to_string(),    Value::Dict(form_map));
        req_map.insert("files".to_string(),   Value::list(files_list));
        req_map.insert("sid".to_string(),     Value::Str(sid.clone()));
        if let Some(u) = guard_user {
            req_map.insert("user".to_string(), u);
        }
        let req_val = Value::Dict(req_map);

        // Middlewares del router: mw(req) → null continúa; un dict corta y
        // se responde con él (p. ej. 401 de auth o 429 de rate limit).
        let mut early_response: Option<Value> = None;
        for mw in &middlewares {
            match self.run_handler(mw, req_val.clone()) {
                Ok(Value::Null) => continue,
                Ok(other) => { early_response = Some(other); break; }
                Err(e) => {
                    eprintln!("[Orion] middleware '{}' falló: {}", mw, e);
                    early_response = Some(Value::Str(format!("error interno: {}", e)));
                    break;
                }
            }
        }

        // Ejecutar handler; un error se convierte en 500 sin tumbar al worker.
        let result = match early_response {
            Some(r) => r,
            None => match self.run_handler(target_fn, req_val) {
                Ok(v)  => v,
                Err(e) => {
                    log_req(500, &e.to_string());
                    let resp = Response::from_string(format!("error interno: {}", e))
                        .with_status_code(500);
                    let _ = request.respond(resp);
                    return;
                }
            }
        };

        let (status_code, resp_body, content_type, extra_headers) = match result {
            // Un Dict SIN claves de respuesta (status/body/json/file/redirect/
            // headers/cookies/content_type) es un payload de datos: se responde
            // como JSON directamente, estilo FastAPI — return { "id": 42 }.
            Value::Dict(ref m) if !is_response_spec(m) => (
                200,
                value_json_string(&result).into_bytes(),
                "application/json; charset=utf-8".to_string(),
                Vec::new(),
            ),
            Value::Dict(ref m) => {
                let status_override = match m.get("status") {
                    Some(Value::Int(n)) => Some(*n as u16),
                    _ => None,
                };
                // headers: Dict opcional → se envían tal cual (CORS, SSE, cache…)
                let mut extra: Vec<(String, String)> = match m.get("headers") {
                    Some(Value::Dict(hm)) => hm.iter()
                        .map(|(k, v)| (k.clone(), v.to_string()))
                        .collect(),
                    _ => Vec::new(),
                };
                // cookies: Dict → un Set-Cookie por entrada. Valor simple gana
                // "; Path=/"; un valor con ';' trae sus atributos y va tal cual.
                if let Some(Value::Dict(cm)) = m.get("cookies") {
                    for (k, v) in cm {
                        let val = v.to_string();
                        let cookie = if val.contains(';') {
                            format!("{}={}", k, val)
                        } else {
                            format!("{}={}; Path=/", k, val)
                        };
                        extra.push(("Set-Cookie".to_string(), cookie));
                    }
                }
                // { "redirect": "/destino" } → 302 (u override) + Location
                if let Some(rv) = m.get("redirect") {
                    extra.push(("Location".to_string(), rv.to_string()));
                    (
                        status_override.unwrap_or(302),
                        Vec::new(),
                        "text/plain; charset=utf-8".to_string(),
                        extra,
                    )
                }
                // { "file": "ruta" } → responde el archivo TAL CUAL (binario ok),
                // con MIME automático por extensión salvo content_type explícito.
                else if let Some(fv) = m.get("file") {
                    let fpath = fv.to_string();
                    match std::fs::read(&fpath) {
                        Ok(bytes) => {
                            let ct = m.get("content_type")
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| mime_for(std::path::Path::new(&fpath)));
                            (status_override.unwrap_or(200), bytes, ct, extra)
                        }
                        Err(_) => (
                            404,
                            format!("archivo no encontrado: {}", fpath).into_bytes(),
                            "text/plain; charset=utf-8".to_string(),
                            extra,
                        ),
                    }
                }
                // { "json": valor } → serializa y pone el content-type solo
                else if let Some(jv) = m.get("json") {
                    (
                        status_override.unwrap_or(200),
                        value_json_string(jv).into_bytes(),
                        "application/json; charset=utf-8".to_string(),
                        extra,
                    )
                } else {
                    // { "status": 200, "body": <Dict|List> } → el body estructurado
                    // se serializa a JSON, igual que un payload suelto o {"json": …}.
                    // Sin esto el cliente recibía el Display de Orion ({a: 1}), que
                    // no es JSON válido. Un content_type explícito sigue mandando.
                    let explicit_ct = m.get("content_type").map(|v| v.to_string());
                    let (body, ct) = match m.get("body") {
                        Some(v @ (Value::Dict(_) | Value::List(_))) if explicit_ct.is_none() => (
                            value_json_string(v).into_bytes(),
                            "application/json; charset=utf-8".to_string(),
                        ),
                        other => (
                            other.map(|v| v.to_string()).unwrap_or_default().into_bytes(),
                            explicit_ct.unwrap_or_else(|| "text/plain; charset=utf-8".to_string()),
                        ),
                    };
                    (status_override.unwrap_or(200), body, ct, extra)
                }
            }
            // Una lista también es un payload de datos → JSON
            Value::List(_) => (
                200,
                value_json_string(&result).into_bytes(),
                "application/json; charset=utf-8".to_string(),
                Vec::new(),
            ),
            Value::Str(s) => (200, s.into_bytes(), "text/plain; charset=utf-8".to_string(), Vec::new()),
            Value::Null   => (204, Vec::new(), "text/plain".to_string(), Vec::new()),
            other         => (200, other.to_string().into_bytes(), "text/plain; charset=utf-8".to_string(), Vec::new()),
        };

        let mut response = Response::from_data(resp_body).with_status_code(status_code);
        if let Ok(h) = Header::from_str(&format!("Content-Type: {}", content_type)) {
            response = response.with_header(h);
        }
        // Sesión: si el handler guardó datos con session.set(req["sid"], …) y el
        // sid no venía ya en la cookie, emitir el Set-Cookie automáticamente.
        // HttpOnly + SameSite=Lax por defecto (no accesible desde JS, anti-CSRF).
        if incoming_sid.as_deref() != Some(sid.as_str())
            && crate::modules::session_mod::exists(&sid)
        {
            let cookie = format!("orion_sid={}; Path=/; HttpOnly; SameSite=Lax", sid);
            if let Ok(h) = Header::from_str(&format!("Set-Cookie: {}", cookie)) {
                response = response.with_header(h);
            }
        }
        // Seguridad por defecto: nosniff salvo que el dev lo sobreescriba
        let has_nosniff = extra_headers.iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("X-Content-Type-Options"));
        if !has_nosniff {
            if let Ok(h) = Header::from_str("X-Content-Type-Options: nosniff") {
                response = response.with_header(h);
            }
        }
        for (k, v) in &extra_headers {
            if let Ok(h) = Header::from_str(&format!("{}: {}", k, v)) {
                response = response.with_header(h);
            }
        }

        log_req(status_code as u16, "");
        let _ = request.respond(response);
    }

    pub(crate) fn call_builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Option<Value>, String> {
        match name {
            // slice(obj, start, end) — soporta lista[a:b] y string[a:b].
            // start/end pueden ser Null (extremo abierto); índices negativos cuentan desde el final.
            "slice" => {
                let mut it = args.into_iter();
                let obj     = it.next().ok_or("slice() requiere un objeto")?;
                let start_v = it.next().unwrap_or(Value::Null);
                let end_v   = it.next().unwrap_or(Value::Null);
                let resolve = |v: Value, len: i64, default: i64| -> i64 {
                    match v {
                        Value::Int(n) => (if n < 0 { len + n } else { n }).clamp(0, len),
                        _ => default,
                    }
                };
                match obj {
                    Value::List(items) => {
                        let items = items.borrow();
                        let len = items.len() as i64;
                        let s = resolve(start_v, len, 0);
                        let e = resolve(end_v, len, len);
                        let out = if s < e { items[s as usize..e as usize].to_vec() } else { vec![] };
                        Ok(Some(Value::list(out)))
                    }
                    Value::Str(st) => {
                        let chars: Vec<char> = st.chars().collect();
                        let len = chars.len() as i64;
                        let s = resolve(start_v, len, 0);
                        let e = resolve(end_v, len, len);
                        let out: String = if s < e { chars[s as usize..e as usize].iter().collect() } else { String::new() };
                        Ok(Some(Value::Str(out)))
                    }
                    _ => Err("slice(): solo aplica a listas o strings".to_string()),
                }
            }
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
                    Value::List(v) => Ok(Some(Value::Int(v.borrow().len() as i64))),
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
                let text = args.iter().map(|a| format!("{}", a)).collect::<Vec<_>>().join(" ");
                write_utf8_line(&text);
                Ok(None)
            }
            //    Listas                                                        
            "push" | "append" => {
                let mut it = args.into_iter();
                let list = it.next().ok_or("push() requiere al menos 2 argumentos")?;
                let val  = it.next().ok_or("push() requiere al menos 2 argumentos")?;
                match list {
                    Value::List(v) => {
                        // Un contenedor dentro de la lista puede cerrar un ciclo.
                        if crate::gc::is_container(&val) {
                            self.gc.register_list(&v);
                        }
                        v.borrow_mut().push(val);
                        Ok(Some(Value::List(v)))
                    }
                    _ => Err("push(): el primer argumento debe ser una lista".to_string()),
                }
            }
            "pop" => {
                let list = args.into_iter().next().ok_or("pop() requiere un argumento")?;
                match list {
                    Value::List(v) => {
                        let item = v.borrow_mut().pop().unwrap_or(Value::Null);
                        // devuelve [item, lista_mutada] para permitir acceso a ambos;
                        // la lista interior es el MISMO backing (ya mutado in-place)
                        Ok(Some(Value::list(vec![item, Value::List(v)])))
                    }
                    _ => Err("pop(): requiere una lista".to_string()),
                }
            }
            "first" => {
                let list = args.into_iter().next().ok_or("first() requiere un argumento")?;
                match list {
                    Value::List(v) => Ok(Some(v.borrow().first().cloned().unwrap_or(Value::Null))),
                    _ => Err("first(): requiere una lista".to_string()),
                }
            }
            "last" => {
                let list = args.into_iter().next().ok_or("last() requiere un argumento")?;
                match list {
                    Value::List(v) => Ok(Some(v.borrow().last().cloned().unwrap_or(Value::Null))),
                    _ => Err("last(): requiere una lista".to_string()),
                }
            }
            "reverse" => {
                let list = args.into_iter().next().ok_or("reverse() requiere un argumento")?;
                match list {
                    Value::List(v) => { v.borrow_mut().reverse(); Ok(Some(Value::List(v))) }
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
                Ok(Some(Value::list(v)))
            }
            "contains" => {
                let mut it = args.into_iter();
                let container = it.next().ok_or("contains() requiere 2 argumentos")?;
                let item      = it.next().ok_or("contains() requiere 2 argumentos")?;
                match container {
                    Value::List(v) => Ok(Some(Value::Bool(v.borrow().contains(&item)))),
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
            //    Dicts                                                        
            "keys" => {
                let val = args.into_iter().next().ok_or("keys() requiere un argumento")?;
                match val {
                    Value::Dict(m) => Ok(Some(Value::list(m.keys().map(|k| Value::Str(k.clone())).collect()))),
                    _ => Err("keys(): requiere un dict".to_string()),
                }
            }
            "values" => {
                let val = args.into_iter().next().ok_or("values() requiere un argumento")?;
                match val {
                    Value::Dict(m) => Ok(Some(Value::list(m.into_values().collect()))),
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
            //    Strings                                                      
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
                        Ok(Some(Value::list(parts)))
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
                        let parts: Vec<String> = v.borrow().iter().map(|x| x.to_string()).collect();
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
            //    Matemáticas                                                  
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
                        Value::List(v) => v.borrow().0.clone(),
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
                        Value::List(v) => v.borrow().0.clone(),
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
            "round" => {
                let mut it = args.into_iter();
                let val = it.next().ok_or("round() requiere al menos un argumento")?;
                let f = match val {
                    Value::Float(f) => f,
                    Value::Int(n)   => n as f64,
                    _ => return Err("round(): requiere un número".to_string()),
                };
                match it.next() {
                    Some(d) => {
                        let digits = match d { Value::Int(n) => n, Value::Float(f) => f as i64, _ => 0 };
                        let factor = 10_f64.powi(digits as i32);
                        Ok(Some(Value::Float((f * factor).round() / factor)))
                    }
                    None => Ok(Some(Value::Int(f.round() as i64))),
                }
            }
            "pow" => {
                let mut it = args.into_iter();
                let base = it.next().ok_or("pow() requiere 2 argumentos")?;
                let exp  = it.next().ok_or("pow() requiere 2 argumentos")?;
                let b = match base { Value::Float(f) => f, Value::Int(n) => n as f64, _ => return Err("pow(): requiere números".into()) };
                let e = match exp  { Value::Float(f) => f, Value::Int(n) => n as f64, _ => return Err("pow(): requiere números".into()) };
                Ok(Some(Value::Float(b.powf(e))))
            }
            "sum" => {
                let items = match args.into_iter().next().unwrap_or_else(|| Value::list(vec![])) {
                    Value::List(v) => v.borrow().0.clone(),
                    other => vec![other],
                };
                let mut total = 0.0_f64;
                let mut all_int = true;
                for item in &items {
                    match item {
                        Value::Int(n)   => total += *n as f64,
                        Value::Float(f) => { total += f; all_int = false; }
                        _ => return Err("sum(): la lista debe contener números".to_string()),
                    }
                }
                if all_int { Ok(Some(Value::Int(total as i64))) }
                else       { Ok(Some(Value::Float(total))) }
            }
            "bool" => {
                let val = args.into_iter().next().ok_or("bool() requiere un argumento")?;
                Ok(Some(Value::Bool(val.is_truthy())))
            }
            "sort" => {
                let val = args.into_iter().next().ok_or("sort() requiere una lista")?;
                match val {
                    Value::List(v) => {
                        v.borrow_mut().sort_by(|a, b| {
                            let fa = match a { Value::Int(n) => *n as f64, Value::Float(f) => *f, _ => 0.0 };
                            let fb = match b { Value::Int(n) => *n as f64, Value::Float(f) => *f, _ => 0.0 };
                            fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        Ok(Some(Value::List(v)))
                    }
                    _ => Err("sort(): requiere una lista".to_string()),
                }
            }
            //    I/O                                                          
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
            //    Tests                                                        
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
    //     C FFI                                                                

    fn call_extern_fn(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        use libffi::middle::{Arg, Cif, CodePtr, Type};
        use std::ffi::{CStr, CString};

        let def = self.extern_fns.get(name).cloned()
            .ok_or_else(|| format!("FFI: función extern '{}' no registrada", name))?;
        if def.lib.is_empty() {
            return Err(format!("FFI: '{}' no especifica librería (falta `from \"lib\"`)", name));
        }
        if args.len() != def.params.len() {
            return Err(format!("FFI: '{}' espera {} arg(s), recibió {}", name, def.params.len(), args.len()));
        }

        // Cargar librería dinámicamente (con caché)
        if !self.extern_libs.contains_key(&def.lib) {
            let path = ffi_resolve_lib(&def.lib);
            let lib = unsafe { libloading::Library::new(&path) }
                .map_err(|e| format!("FFI: no se pudo cargar '{}': {}", path, e))?;
            self.extern_libs.insert(def.lib.clone(), lib);
        }

        // Obtener puntero de función (antes de liberar el borrow)
        let fn_ptr_raw: *const () = {
            let lib = self.extern_libs.get(&def.lib).unwrap();
            unsafe {
                let sym: libloading::Symbol<unsafe extern "C" fn()> = lib.get(name.as_bytes())
                    .map_err(|e| format!("FFI: símbolo '{}' no encontrado en '{}': {}", name, def.lib, e))?;
                *sym as *const ()
            }
        };

        //    Almacenamiento tipado (deben vivir hasta que termine la llamada)    
        let mut s_i32:  Vec<i32>        = Vec::new();
        let mut s_i64:  Vec<i64>        = Vec::new();
        let mut s_u32:  Vec<u32>        = Vec::new();
        let mut s_u64:  Vec<u64>        = Vec::new();
        let mut s_f32:  Vec<f32>        = Vec::new();
        let mut s_f64:  Vec<f64>        = Vec::new();
        let mut s_ptr:  Vec<*const ()>  = Vec::new();
        let mut _cstrs: Vec<CString>    = Vec::new();

        enum AK { I32(usize), I64(usize), U32(usize), U64(usize), F32(usize), F64(usize), Ptr(usize) }
        let mut arg_kinds: Vec<AK>  = Vec::new();
        let mut ffi_types: Vec<Type> = Vec::new();

        //    Convertir cada argumento                                           
        for (pt, arg) in def.params.iter().zip(args.iter()) {
            match pt.as_str() {
                "int" | "i32" => {
                    let v = match arg {
                        Value::Int(n)  => *n as i32,
                        Value::Bool(b) => *b as i32,
                        _ => return Err(format!("FFI: param 'int', recibió {}", arg.type_name())),
                    };
                    s_i32.push(v); arg_kinds.push(AK::I32(s_i32.len()-1)); ffi_types.push(Type::i32());
                }
                "i64" | "long" | "int64" => {
                    let v = match arg {
                        Value::Int(n)  => *n,
                        Value::Bool(b) => *b as i64,
                        _ => return Err(format!("FFI: param 'i64', recibió {}", arg.type_name())),
                    };
                    s_i64.push(v); arg_kinds.push(AK::I64(s_i64.len()-1)); ffi_types.push(Type::i64());
                }
                "uint" | "u32" => {
                    let v = match arg {
                        Value::Int(n) => *n as u32,
                        _ => return Err(format!("FFI: param 'u32', recibió {}", arg.type_name())),
                    };
                    s_u32.push(v); arg_kinds.push(AK::U32(s_u32.len()-1)); ffi_types.push(Type::u32());
                }
                "u64" | "size_t" | "usize" => {
                    let v = match arg {
                        Value::Int(n) => *n as u64,
                        Value::Ptr(p) => *p,
                        _ => return Err(format!("FFI: param 'u64', recibió {}", arg.type_name())),
                    };
                    s_u64.push(v); arg_kinds.push(AK::U64(s_u64.len()-1)); ffi_types.push(Type::u64());
                }
                "float" | "f32" => {
                    let v = match arg {
                        Value::Float(f) => *f as f32,
                        Value::Int(n)   => *n as f32,
                        _ => return Err(format!("FFI: param 'float', recibió {}", arg.type_name())),
                    };
                    s_f32.push(v); arg_kinds.push(AK::F32(s_f32.len()-1)); ffi_types.push(Type::f32());
                }
                "double" | "f64" => {
                    let v = match arg {
                        Value::Float(f) => *f,
                        Value::Int(n)   => *n as f64,
                        _ => return Err(format!("FFI: param 'double', recibió {}", arg.type_name())),
                    };
                    s_f64.push(v); arg_kinds.push(AK::F64(s_f64.len()-1)); ffi_types.push(Type::f64());
                }
                "bool" => {
                    let v = match arg {
                        Value::Bool(b) => *b as i32,
                        Value::Int(n)  => (*n != 0) as i32,
                        _ => return Err(format!("FFI: param 'bool', recibió {}", arg.type_name())),
                    };
                    s_i32.push(v); arg_kinds.push(AK::I32(s_i32.len()-1)); ffi_types.push(Type::i32());
                }
                "ptr" | "pointer" | "void*" => {
                    let v: *const () = match arg {
                        Value::Ptr(p) => *p as *const (),
                        Value::Int(n) => *n as *const (),
                        Value::Null   => std::ptr::null(),
                        _ => return Err(format!("FFI: param 'ptr', recibió {}", arg.type_name())),
                    };
                    s_ptr.push(v); arg_kinds.push(AK::Ptr(s_ptr.len()-1)); ffi_types.push(Type::pointer());
                }
                "string" | "str" | "cstr" | "char*" => {
                    let v: *const () = match arg {
                        Value::Str(s) => {
                            let cs = CString::new(s.as_bytes())
                                .map_err(|_| format!("FFI: string para '{}' contiene byte nulo", name))?;
                            let p = cs.as_ptr() as *const ();
                            _cstrs.push(cs);
                            p
                        }
                        Value::Null => std::ptr::null(),
                        _ => return Err(format!("FFI: param 'string', recibió {}", arg.type_name())),
                    };
                    s_ptr.push(v); arg_kinds.push(AK::Ptr(s_ptr.len()-1)); ffi_types.push(Type::pointer());
                }
                t => return Err(format!(
                    "FFI: tipo '{}' no soportado (usa: int, i64, uint, u64, float, double, ptr, string, bool)", t
                )),
            }
        }

        //    Tipo de retorno                                                    
        let ffi_ret = match def.ret_type.as_str() {
            "void" | ""              => Type::void(),
            "int"  | "i32" | "bool" => Type::i32(),
            "i64"  | "long"          => Type::i64(),
            "uint" | "u32"           => Type::u32(),
            "u64"  | "size_t" | "usize" => Type::u64(),
            "float"  | "f32"         => Type::f32(),
            "double" | "f64"         => Type::f64(),
            _                        => Type::pointer(),
        };

        let cif  = Cif::new(ffi_types.into_iter(), ffi_ret);
        let code = CodePtr(fn_ptr_raw as *mut _);

        //    Construir lista de Arg (después de todos los push — sin más realloc)  
        let ffi_args: Vec<Arg> = arg_kinds.iter().map(|ak| match ak {
            AK::I32(i) => Arg::new(&s_i32[*i]),
            AK::I64(i) => Arg::new(&s_i64[*i]),
            AK::U32(i) => Arg::new(&s_u32[*i]),
            AK::U64(i) => Arg::new(&s_u64[*i]),
            AK::F32(i) => Arg::new(&s_f32[*i]),
            AK::F64(i) => Arg::new(&s_f64[*i]),
            AK::Ptr(i) => Arg::new(&s_ptr[*i]),
        }).collect();

        //    Llamar y convertir resultado                                      
        let result = match def.ret_type.as_str() {
            "void" | "" => {
                unsafe { cif.call::<()>(code, &ffi_args) };
                Value::Null
            }
            "int" | "i32" => {
                let r: i32 = unsafe { cif.call(code, &ffi_args) };
                Value::Int(r as i64)
            }
            "i64" | "long" => {
                let r: i64 = unsafe { cif.call(code, &ffi_args) };
                Value::Int(r)
            }
            "uint" | "u32" => {
                let r: u32 = unsafe { cif.call(code, &ffi_args) };
                Value::Int(r as i64)
            }
            "u64" | "size_t" | "usize" => {
                let r: u64 = unsafe { cif.call(code, &ffi_args) };
                Value::Int(r as i64)
            }
            "float" | "f32" => {
                let r: f32 = unsafe { cif.call(code, &ffi_args) };
                Value::Float(r as f64)
            }
            "double" | "f64" => {
                let r: f64 = unsafe { cif.call(code, &ffi_args) };
                Value::Float(r)
            }
            "bool" => {
                let r: i32 = unsafe { cif.call(code, &ffi_args) };
                Value::Bool(r != 0)
            }
            "string" | "cstr" | "char*" => {
                let r: *const std::os::raw::c_char = unsafe { cif.call(code, &ffi_args) };
                if r.is_null() { Value::Null } else {
                    unsafe { Value::Str(CStr::from_ptr(r).to_string_lossy().into_owned()) }
                }
            }
            _ => {
                let r: *mut () = unsafe { cif.call(code, &ffi_args) };
                if r.is_null() { Value::Null } else { Value::Ptr(r as u64) }
            }
        };

        Ok(result)
    }

    //     Interfaz pública del debugger                                        

    /// Ejecuta exactamente un paso del loop principal.
    /// Retorna `Ok(true)` cuando el programa terminó.
    pub fn step_once(&mut self) -> Result<bool, String> {
        self.step()
    }

    /// Línea de código fuente que se está ejecutando actualmente.
    pub fn current_line(&self) -> u32 {
        self.current_line
    }

    /// Número de frames en el call stack (0 = programa terminado).
    pub fn call_depth(&self) -> usize {
        self.call_stack.len()
    }

    /// Frames del call stack para el debugger: `(nombre_fn, línea)` del más reciente al más antiguo.
    pub fn debug_frames(&self) -> Vec<(String, u32)> {
        self.call_stack.iter().rev()
            .map(|f| (f.name.clone(), f.current_line()))
            .collect()
    }

    /// Variables del frame más reciente (scope local actual).
    pub fn debug_frame_vars(&self) -> Vec<(String, Value)> {
        self.call_stack.last()
            .map(|f| f.vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    /// Variables de todos los frames, del más reciente al más antiguo.
    pub fn debug_all_scopes(&self) -> Vec<Vec<(String, Value)>> {
        self.call_stack.iter().rev()
            .map(|f| f.vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .collect()
    }

    /// Copia del value stack actual (índice 0 = bottom).
    pub fn debug_value_stack(&self) -> Vec<Value> {
        self.value_stack.clone()
    }

    /// Busca una variable en la cadena de scopes (local → global).
    pub fn debug_lookup_var(&self, name: &str) -> Option<Value> {
        for frame in self.call_stack.iter().rev() {
            if let Some(v) = frame.vars.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

} // fin impl VM

// ---------------------------------------------------------------------------
// C FFI — utilidades
// ---------------------------------------------------------------------------

/// Resuelve el nombre de librería al path correcto para la plataforma.
fn ffi_resolve_lib(name: &str) -> String {
    if name.contains('.') { return name.to_string(); }
    #[cfg(target_os = "windows")]  return format!("{}.dll", name);
    #[cfg(target_os = "linux")]    return format!("lib{}.so", name);
    #[cfg(target_os = "macos")]    return format!("lib{}.dylib", name);
    #[allow(unreachable_code)]
    name.to_string()
}

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
            Value::list(arr.into_iter().map(json_to_value).collect())
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

/// ¿El Dict devuelto por un handler describe una respuesta HTTP (status,
/// body, headers…) o es un payload de datos que debe salir como JSON?
/// Las claves ambiguas solo cuentan si traen el tipo correcto: un dict de
/// datos con "status": "ok" sigue siendo datos.
fn is_response_spec(m: &IndexMap<String, Value>) -> bool {
    m.contains_key("body") || m.contains_key("json") || m.contains_key("file")
        || m.contains_key("redirect") || m.contains_key("content_type")
        || matches!(m.get("status"),  Some(Value::Int(_)))
        || matches!(m.get("headers"), Some(Value::Dict(_)))
        || matches!(m.get("cookies"), Some(Value::Dict(_)))
}

/// Serializa un Value de Orion a JSON (mismo camino que json.forge).
fn value_json_string(v: &Value) -> String {
    let json = crate::modules::json_mod::eval_to_json(value_to_eval(v.clone()));
    serde_json::to_string(&json).unwrap_or_else(|_| "null".to_string())
}

//     multipart/form-data: parser a nivel de bytes (subida de archivos)

/// Extrae el boundary del header Content-Type.
fn multipart_boundary(content_type: &str) -> Option<String> {
    content_type.split(';')
        .map(|s| s.trim())
        .find_map(|p| p.strip_prefix("boundary="))
        .map(|b| b.trim_matches('"').to_string())
}

/// Parsea un cuerpo multipart en (campos_de_texto, archivos). Cada archivo se
/// escribe a un temporal único y se describe con {field, filename, content_type,
/// size, tmp_path}; el handler decide dónde guardarlo (fs.move/fs.copy).
fn parse_multipart(body: &[u8], boundary: &str) -> (IndexMap<String, Value>, Vec<Value>) {
    let mut form = IndexMap::new();
    let mut files = Vec::new();

    let delim = format!("--{}", boundary).into_bytes();
    // Cada parte va entre delimitadores; el bloque de headers termina en \r\n\r\n.
    for part in split_on(body, &delim) {
        // Quitar el CRLF inicial y descartar el cierre "--" y partes vacías.
        let part = part.strip_prefix(b"\r\n").unwrap_or(part);
        if part.is_empty() || part.starts_with(b"--") { continue; }
        let sep = match find_sub(part, b"\r\n\r\n") { Some(i) => i, None => continue };
        let (head, mut content) = (&part[..sep], &part[sep + 4..]);
        // El contenido termina con un CRLF antes del siguiente delimitador.
        if content.ends_with(b"\r\n") { content = &content[..content.len() - 2]; }

        let headers = String::from_utf8_lossy(head);
        let mut name = String::new();
        let mut filename: Option<String> = None;
        let mut ctype = String::from("application/octet-stream");
        for line in headers.split("\r\n") {
            let low = line.to_ascii_lowercase();
            if low.starts_with("content-disposition:") {
                name     = header_param(line, "name").unwrap_or_default();
                filename = header_param(line, "filename");
            } else if low.starts_with("content-type:") {
                ctype = line[13..].trim().to_string();
            }
        }

        match filename {
            // Archivo: volcar a un temporal y describirlo.
            Some(fname) if !fname.is_empty() => {
                let tmp = std::env::temp_dir()
                    .join(format!("orion_upload_{}", next_upload_id()));
                if std::fs::write(&tmp, content).is_ok() {
                    let mut d = IndexMap::new();
                    d.insert("field".into(),        Value::Str(name));
                    d.insert("filename".into(),     Value::Str(fname));
                    d.insert("content_type".into(), Value::Str(ctype));
                    d.insert("size".into(),         Value::Int(content.len() as i64));
                    d.insert("tmp_path".into(),     Value::Str(tmp.to_string_lossy().into_owned()));
                    files.push(Value::Dict(d));
                }
            }
            // Campo de texto normal.
            _ => {
                form.insert(name, Value::Str(String::from_utf8_lossy(content).into_owned()));
            }
        }
    }
    (form, files)
}

fn next_upload_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64).unwrap_or(0);
    t ^ (n << 20) ^ (std::process::id() as u64)
}

/// Valor de un parámetro `clave="valor"` (o sin comillas) dentro de una línea.
fn header_param(line: &str, key: &str) -> Option<String> {
    let needle = format!("{}=", key);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    if let Some(stripped) = rest.strip_prefix('"') {
        stripped.find('"').map(|end| stripped[..end].to_string())
    } else {
        Some(rest.split(';').next().unwrap_or(rest).trim().to_string())
    }
}

/// Divide `data` por cada aparición de `sep` (sin incluir el separador).
fn split_on<'a>(data: &'a [u8], sep: &[u8]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i + sep.len() <= data.len() {
        if &data[i..i + sep.len()] == sep {
            out.push(&data[start..i]);
            i += sep.len();
            start = i;
        } else {
            i += 1;
        }
    }
    out.push(&data[start..]);
    out
}

/// Índice de la primera aparición de `needle` en `hay`.
fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() { return None; }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

//     Archivos estáticos: MIME por extensión y resolución segura

fn mime_for(path: &std::path::Path) -> String {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css"          => "text/css; charset=utf-8",
        "js" | "mjs"   => "application/javascript; charset=utf-8",
        "json"         => "application/json; charset=utf-8",
        "svg"          => "image/svg+xml",
        "png"          => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif"          => "image/gif",
        "webp"         => "image/webp",
        "ico"          => "image/x-icon",
        "woff"         => "font/woff",
        "woff2"        => "font/woff2",
        "ttf"          => "font/ttf",
        "otf"          => "font/otf",
        "pdf"          => "application/pdf",
        "txt" | "md"   => "text/plain; charset=utf-8",
        "xml"          => "application/xml",
        "csv"          => "text/csv; charset=utf-8",
        "wasm"         => "application/wasm",
        "mp4"          => "video/mp4",
        "mp3"          => "audio/mpeg",
        "zip"          => "application/zip",
        _              => "application/octet-stream",
    }.to_string()
}

/// Resuelve un archivo bajo la carpeta estática de forma SEGURA: canonicaliza
/// ambos paths y exige que el resultado siga dentro de la carpeta (un `../`
/// codificado no puede escapar). Directorio → intenta index.html.
fn resolve_static(dir: &str, rest: &str) -> Option<(Vec<u8>, String)> {
    let root = std::path::Path::new(dir).canonicalize().ok()?;
    let mut target = root.join(rest);
    if target.is_dir() { target = target.join("index.html"); }
    let canon = target.canonicalize().ok()?;
    if !canon.starts_with(&root) { return None; }
    let bytes = std::fs::read(&canon).ok()?;
    Some((bytes, mime_for(&canon)))
}

//     Percent-decoding para paths y query strings HTTP

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decodifica percent-encoding byte a byte (seguro ante UTF-8 multibyte).
/// Con `plus_as_space` un '+' también cuenta como espacio (convención de query).
fn url_decode(s: &str, plus_as_space: bool) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let (Some(a), Some(b)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                    out.push(a * 16 + b);
                    i += 3;
                    continue;
                }
                out.push(b'%');
                i += 1;
            }
            b'+' if plus_as_space => { out.push(b' '); i += 1; }
            b => { out.push(b); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

//     Bridge Value ↔ EvalValue (para módulos stdlib en el bytecode VM)

pub fn value_to_eval(v: Value) -> crate::eval_value::EvalValue {
    use crate::eval_value::EvalValue as E;
    match v {
        Value::Int(n)    => E::Int(n),
        Value::Float(f)  => E::Float(f),
        Value::Str(s)    => E::Str(s),
        Value::Bool(b)   => E::Bool(b),
        Value::Null      => E::Null,
        Value::Module(m) => E::Module(m),
        Value::List(items) => E::List(items.borrow().iter().cloned().map(value_to_eval).collect()),
        Value::Dict(map)   => {
            // pre-reservar evita re-hashes en tablas grandes; IndexMap→IndexMap
            // preserva el ORDEN de inserción de las claves end-to-end
            let mut m = indexmap::IndexMap::with_capacity(map.len());
            for (k, v) in map { m.insert(k, value_to_eval(v)); }
            E::Dict(m)
        }
        _ => E::Null,
    }
}

pub fn eval_to_value(e: crate::eval_value::EvalValue) -> Value {
    use crate::eval_value::EvalValue as E;
    match e {
        E::Int(n)    => Value::Int(n),
        E::Float(f)  => Value::Float(f),
        E::Str(s)    => Value::Str(s),
        E::Bool(b)   => Value::Bool(b),
        E::Null      => Value::Null,
        E::Module(m) => Value::Module(m),
        E::List(items) => Value::list(items.into_iter().map(eval_to_value).collect()),
        E::Dict(map)   => {
            let mut m = indexmap::IndexMap::with_capacity(map.len());
            for (k, v) in map { m.insert(k, eval_to_value(v)); }
            Value::Dict(m)
        }
        _ => Value::Null,
    }
}

//     Tests unitarios de la VM                                                 

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::Instruction;
    use indexmap::IndexMap;

    /// Crea una VM mínima con solo las instrucciones dadas.
    fn make_vm(instructions: Vec<Instruction>) -> VM {
        let n = instructions.len();
        VM::new(
            instructions,
            vec![1u32; n],
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        )
    }

    /// Ejecuta las instrucciones y devuelve el valor en el tope del stack.
    fn run_top(instructions: Vec<Instruction>) -> Result<Value, String> {
        let mut vm = make_vm(instructions);
        vm.run_raw()?;
        vm.value_stack.pop().ok_or_else(|| "Stack vacío al terminar".to_string())
    }

    //    Literales                                                             

    #[test]
    fn test_load_int() {
        assert_eq!(run_top(vec![Instruction::LoadInt(42), Instruction::Halt]).unwrap(), Value::Int(42));
    }

    #[test]
    fn test_load_float() {
        assert_eq!(run_top(vec![Instruction::LoadFloat(3.14), Instruction::Halt]).unwrap(), Value::Float(3.14));
    }

    #[test]
    fn test_load_str() {
        assert_eq!(run_top(vec![Instruction::LoadStr("orion".into()), Instruction::Halt]).unwrap(), Value::Str("orion".into()));
    }

    #[test]
    fn test_load_bool_true() {
        assert_eq!(run_top(vec![Instruction::LoadBool(true), Instruction::Halt]).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_load_null() {
        assert_eq!(run_top(vec![Instruction::LoadNull, Instruction::Halt]).unwrap(), Value::Null);
    }

    //    Aritmética                                                            

    #[test]
    fn test_add_int() {
        let r = run_top(vec![Instruction::LoadInt(3), Instruction::LoadInt(4), Instruction::Add, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Int(7));
    }

    #[test]
    fn test_sub_int() {
        let r = run_top(vec![Instruction::LoadInt(10), Instruction::LoadInt(3), Instruction::Sub, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Int(7));
    }

    #[test]
    fn test_mul_int() {
        let r = run_top(vec![Instruction::LoadInt(6), Instruction::LoadInt(7), Instruction::Mul, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Int(42));
    }

    #[test]
    fn test_div_exact() {
        let r = run_top(vec![Instruction::LoadInt(10), Instruction::LoadInt(4), Instruction::Div, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Float(2.5));
    }

    #[test]
    fn test_div_by_zero_int() {
        let r = run_top(vec![Instruction::LoadInt(5), Instruction::LoadInt(0), Instruction::Div, Instruction::Halt]);
        assert!(r.is_err());
    }

    #[test]
    fn test_mod_op() {
        let r = run_top(vec![Instruction::LoadInt(10), Instruction::LoadInt(3), Instruction::Mod, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Int(1));
    }

    #[test]
    fn test_pow_op() {
        let r = run_top(vec![Instruction::LoadInt(2), Instruction::LoadInt(10), Instruction::Pow, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Int(1024));
    }

    #[test]
    fn test_neg_int() {
        let r = run_top(vec![Instruction::LoadInt(5), Instruction::Neg, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Int(-5));
    }

    #[test]
    fn test_float_add() {
        let r = run_top(vec![Instruction::LoadFloat(1.5), Instruction::LoadFloat(2.5), Instruction::Add, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Float(4.0));
    }

    #[test]
    fn test_string_concat() {
        let r = run_top(vec![
            Instruction::LoadStr("hola".into()),
            Instruction::LoadStr(" mundo".into()),
            Instruction::Add,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Str("hola mundo".into()));
    }

    //    Variables                                                             

    #[test]
    fn test_store_load_var() {
        let r = run_top(vec![
            Instruction::LoadInt(99),
            Instruction::StoreVar("x".into()),
            Instruction::LoadVar("x".into()),
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Int(99));
    }

    //    Comparación                                                           

    #[test]
    fn test_eq_true() {
        let r = run_top(vec![Instruction::LoadInt(5), Instruction::LoadInt(5), Instruction::Eq, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Bool(true));
    }

    #[test]
    fn test_eq_false() {
        let r = run_top(vec![Instruction::LoadInt(5), Instruction::LoadInt(6), Instruction::Eq, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Bool(false));
    }

    #[test]
    fn test_not_eq() {
        let r = run_top(vec![Instruction::LoadInt(3), Instruction::LoadInt(5), Instruction::NotEq, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Bool(true));
    }

    #[test]
    fn test_lt_true() {
        let r = run_top(vec![Instruction::LoadInt(3), Instruction::LoadInt(5), Instruction::Lt, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Bool(true));
    }

    #[test]
    fn test_gt_false() {
        let r = run_top(vec![Instruction::LoadInt(3), Instruction::LoadInt(5), Instruction::Gt, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Bool(false));
    }

    #[test]
    fn test_not_op() {
        let r = run_top(vec![Instruction::LoadBool(true), Instruction::Not, Instruction::Halt]).unwrap();
        assert_eq!(r, Value::Bool(false));
    }

    //    Control de flujo                                                      

    #[test]
    fn test_jump_unconditional() {
        // Salta sobre LoadInt(999) que nunca debe ejecutarse
        let r = run_top(vec![
            Instruction::Jump(2),         // 0 → salta a 2
            Instruction::LoadInt(999),    // 1 (ignorado)
            Instruction::LoadInt(42),     // 2
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Int(42));
    }

    #[test]
    fn test_jump_if_false_taken() {
        let r = run_top(vec![
            Instruction::LoadBool(false),
            Instruction::JumpIfFalse(3), // condición falsa → salta a 3
            Instruction::LoadInt(999),   // ignorado
            Instruction::LoadInt(1),     // 3
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Int(1));
    }

    #[test]
    fn test_jump_if_false_not_taken() {
        let r = run_top(vec![
            Instruction::LoadBool(true),
            Instruction::JumpIfFalse(3), // condición verdadera → no salta
            Instruction::LoadInt(42),    // 2 (se ejecuta)
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Int(42));
    }

    //    Stack                                                                 

    #[test]
    fn test_dup() {
        let mut vm = make_vm(vec![Instruction::LoadInt(7), Instruction::Dup, Instruction::Halt]);
        vm.run_raw().unwrap();
        assert_eq!(vm.value_stack.len(), 2);
        assert_eq!(vm.value_stack[0], Value::Int(7));
        assert_eq!(vm.value_stack[1], Value::Int(7));
    }

    #[test]
    fn test_pop() {
        let mut vm = make_vm(vec![
            Instruction::LoadInt(1),
            Instruction::LoadInt(2),
            Instruction::Pop,
            Instruction::Halt,
        ]);
        vm.run_raw().unwrap();
        assert_eq!(vm.value_stack.len(), 1);
        assert_eq!(vm.value_stack[0], Value::Int(1));
    }

    //    Colecciones                                                           

    #[test]
    fn test_make_list() {
        let r = run_top(vec![
            Instruction::LoadInt(1),
            Instruction::LoadInt(2),
            Instruction::LoadInt(3),
            Instruction::MakeList(3),
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]));
    }

    #[test]
    fn test_get_index() {
        let r = run_top(vec![
            Instruction::LoadInt(10),
            Instruction::LoadInt(20),
            Instruction::LoadInt(30),
            Instruction::MakeList(3),
            Instruction::LoadInt(1), // índice 1 → 20
            Instruction::GetIndex,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Int(20));
    }

    #[test]
    fn test_get_index_out_of_bounds() {
        let r = run_top(vec![
            Instruction::LoadInt(1),
            Instruction::MakeList(1),
            Instruction::LoadInt(99),
            Instruction::GetIndex,
            Instruction::Halt,
        ]);
        assert!(r.is_err());
    }

    //    Manejo de errores                                                     

    #[test]
    fn test_attempt_catch_error() {
        // attempt { raise "boom" } handle e { "capturado" }
        // 0: BeginAttempt(3) — si error, salta a 3
        // 1: LoadStr("boom")
        // 2: Raise            — error → salta a 3, push "boom" en stack
        // 3: StoreVar("e")    — handler: guarda el error
        // 4: LoadStr("capturado")
        // 5: Halt
        let r = run_top(vec![
            Instruction::BeginAttempt(3),
            Instruction::LoadStr("boom".into()),
            Instruction::Raise,
            Instruction::StoreVar("e".into()),
            Instruction::LoadStr("capturado".into()),
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Str("capturado".into()));
    }

    #[test]
    fn test_attempt_no_error() {
        // attempt { 42 } handle e { "error" }
        // 0: BeginAttempt(3)
        // 1: LoadInt(42)
        // 2: EndAttempt(4)  — sin error, salta a 4
        // 3: StoreVar("e")  — handler (no se ejecuta)
        // 4: Halt
        let r = run_top(vec![
            Instruction::BeginAttempt(3),
            Instruction::LoadInt(42),
            Instruction::EndAttempt(4),
            Instruction::StoreVar("e".into()),
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Int(42));
    }

    #[test]
    fn test_raise_without_handler_propagates() {
        let r = run_top(vec![
            Instruction::LoadStr("error sin handler".into()),
            Instruction::Raise,
            Instruction::Halt,
        ]);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("error sin handler"));
    }

    //    Lógica booleana (Sprint 1 — P0)

    #[test]
    fn test_and_true_false() {
        // true and false → false
        let r = run_top(vec![
            Instruction::LoadBool(true),
            Instruction::LoadBool(false),
            Instruction::And,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Bool(false));
    }

    #[test]
    fn test_and_true_true() {
        let r = run_top(vec![
            Instruction::LoadBool(true),
            Instruction::LoadBool(true),
            Instruction::And,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Bool(true));
    }

    #[test]
    fn test_or_false_true() {
        let r = run_top(vec![
            Instruction::LoadBool(false),
            Instruction::LoadBool(true),
            Instruction::Or,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Bool(true));
    }

    #[test]
    fn test_or_false_false() {
        let r = run_top(vec![
            Instruction::LoadBool(false),
            Instruction::LoadBool(false),
            Instruction::Or,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Bool(false));
    }

    #[test]
    fn test_and_truthiness_nonbool() {
        // is_truthy: 1 and 0 → false (0 es falsy)
        let r = run_top(vec![
            Instruction::LoadInt(1),
            Instruction::LoadInt(0),
            Instruction::And,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Bool(false));
    }

    //    Comparaciones restantes (Sprint 1 — P0)

    #[test]
    fn test_lt_eq_equal() {
        // 5 <= 5 → true
        let r = run_top(vec![
            Instruction::LoadInt(5),
            Instruction::LoadInt(5),
            Instruction::LtEq,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Bool(true));
    }

    #[test]
    fn test_lt_eq_false() {
        // 7 <= 3 → false
        let r = run_top(vec![
            Instruction::LoadInt(7),
            Instruction::LoadInt(3),
            Instruction::LtEq,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Bool(false));
    }

    #[test]
    fn test_gt_eq_equal() {
        // 5 >= 5 → true
        let r = run_top(vec![
            Instruction::LoadInt(5),
            Instruction::LoadInt(5),
            Instruction::GtEq,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Bool(true));
    }

    #[test]
    fn test_gt_eq_false() {
        // 3 >= 7 → false
        let r = run_top(vec![
            Instruction::LoadInt(3),
            Instruction::LoadInt(7),
            Instruction::GtEq,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Bool(false));
    }

    //    JumpIfTrue (Sprint 1 — P0)

    #[test]
    fn test_jump_if_true_taken() {
        // condición true → salta sobre LoadInt(1), aterriza en LoadInt(99)
        let r = run_top(vec![
            Instruction::LoadBool(true),    // 0
            Instruction::JumpIfTrue(4),     // 1
            Instruction::LoadInt(1),        // 2 (saltado)
            Instruction::Halt,              // 3
            Instruction::LoadInt(99),       // 4
            Instruction::Halt,              // 5
        ]).unwrap();
        assert_eq!(r, Value::Int(99));
    }

    #[test]
    fn test_jump_if_true_not_taken() {
        // condición false → no salta, ejecuta LoadInt(7)
        let r = run_top(vec![
            Instruction::LoadBool(false),   // 0
            Instruction::JumpIfTrue(4),     // 1
            Instruction::LoadInt(7),        // 2
            Instruction::Halt,              // 3
            Instruction::LoadInt(99),       // 4
            Instruction::Halt,              // 5
        ]).unwrap();
        assert_eq!(r, Value::Int(7));
    }

    //    Diccionarios (Sprint 1 — P0)

    #[test]
    fn test_make_dict_and_get() {
        // {"a": 10, "b": 20}["a"] → 10
        let r = run_top(vec![
            Instruction::LoadStr("a".into()),
            Instruction::LoadInt(10),
            Instruction::LoadStr("b".into()),
            Instruction::LoadInt(20),
            Instruction::MakeDict(2),
            Instruction::LoadStr("a".into()),
            Instruction::GetIndex,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Int(10));
    }

    #[test]
    fn test_dict_missing_key_errors() {
        // acceso a clave inexistente → error
        let r = run_top(vec![
            Instruction::LoadStr("a".into()),
            Instruction::LoadInt(10),
            Instruction::MakeDict(1),
            Instruction::LoadStr("no_existe".into()),
            Instruction::GetIndex,
            Instruction::Halt,
        ]);
        assert!(r.is_err());
    }

    #[test]
    fn test_set_index_dict() {
        // d = {"a": 10}; d["a"] = 99; d["a"] → 99
        let r = run_top(vec![
            Instruction::LoadStr("a".into()),
            Instruction::LoadInt(10),
            Instruction::MakeDict(1),
            Instruction::LoadStr("a".into()),
            Instruction::LoadInt(99),
            Instruction::SetIndex,
            Instruction::LoadStr("a".into()),
            Instruction::GetIndex,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Int(99));
    }

    #[test]
    fn test_set_index_dict_new_key() {
        // d = {"a": 10}; d["b"] = 20; d["b"] → 20 (inserción de clave nueva)
        let r = run_top(vec![
            Instruction::LoadStr("a".into()),
            Instruction::LoadInt(10),
            Instruction::MakeDict(1),
            Instruction::LoadStr("b".into()),
            Instruction::LoadInt(20),
            Instruction::SetIndex,
            Instruction::LoadStr("b".into()),
            Instruction::GetIndex,
            Instruction::Halt,
        ]).unwrap();
        assert_eq!(r, Value::Int(20));
    }
}
