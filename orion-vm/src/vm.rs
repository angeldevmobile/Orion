use std::collections::{HashMap, HashSet};
use crate::instruction::Instruction;
use crate::value::Value;
use crate::bytecode::FunctionDef;

struct CallFrame {
    instructions: Vec<Instruction>,
    lines: Vec<u32>,
    ip: usize,
    vars: HashMap<String, Value>,
    consts: HashSet<String>,
}

impl CallFrame {
    fn new(instructions: Vec<Instruction>, lines: Vec<u32>) -> Self {
        CallFrame { instructions, lines, ip: 0, vars: HashMap::new(), consts: HashSet::new() }
    }

    fn with_args(instructions: Vec<Instruction>, lines: Vec<u32>, params: &[String], args: Vec<Value>) -> Self {
        let mut frame = Self::new(instructions, lines);
        for (param, val) in params.iter().zip(args.into_iter()) {
            frame.vars.insert(param.clone(), val);
        }
        frame
    }
}

pub struct VM {
    value_stack: Vec<Value>,
    call_stack: Vec<CallFrame>,
    functions: HashMap<String, FunctionDef>,
    current_line: u32,
}

impl VM {
    pub fn new(main: Vec<Instruction>, main_lines: Vec<u32>, functions: HashMap<String, FunctionDef>) -> Self {
        VM {
            value_stack: Vec::new(),
            call_stack: vec![CallFrame::new(main, main_lines)],
            functions,
            current_line: 0,
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
            let instr = {
                let frame = match self.call_stack.last_mut() {
                    Some(f) => f,
                    None => break,
                };
                if frame.ip >= frame.instructions.len() {
                    self.call_stack.pop();
                    continue;
                }
                let line = frame.lines.get(frame.ip).copied().unwrap_or(0);
                let instr = frame.instructions[frame.ip].clone();
                frame.ip += 1;
                if line > 0 { self.current_line = line; }
                instr
            };

            match instr {
                //    Constantes                               
                Instruction::LoadInt(n)   => self.value_stack.push(Value::Int(n)),
                Instruction::LoadFloat(f) => self.value_stack.push(Value::Float(f)),
                Instruction::LoadStr(s)   => self.value_stack.push(Value::Str(s)),
                Instruction::LoadBool(b)  => self.value_stack.push(Value::Bool(b)),
                Instruction::LoadNull     => self.value_stack.push(Value::Null),

                //    Variables                                
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

                //    Aritmética                               
                Instruction::Add => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(a.add(&b)?);
                }
                Instruction::Sub => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(a.sub(&b)?);
                }
                Instruction::Mul => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(a.mul(&b)?);
                }
                Instruction::Div => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(a.div(&b)?);
                }
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

                //    Comparación                              
                Instruction::Eq => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(Value::Bool(a.compare_eq(&b)));
                }
                Instruction::NotEq => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(Value::Bool(!a.compare_eq(&b)));
                }
                Instruction::Lt => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(Value::Bool(a.compare_lt(&b)?));
                }
                Instruction::LtEq => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(Value::Bool(a.compare_lt(&b)? || a.compare_eq(&b)));
                }
                Instruction::Gt => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(Value::Bool(!a.compare_lt(&b)? && !a.compare_eq(&b)));
                }
                Instruction::GtEq => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(Value::Bool(!a.compare_lt(&b)?));
                }

                //    Lógica                                   
                Instruction::And => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(Value::Bool(a.is_truthy() && b.is_truthy()));
                }
                Instruction::Or => {
                    let b = self.pop()?; let a = self.pop()?;
                    self.value_stack.push(Value::Bool(a.is_truthy() || b.is_truthy()));
                }
                Instruction::Not => {
                    let a = self.pop()?;
                    self.value_stack.push(Value::Bool(!a.is_truthy()));
                }

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

                //    Funciones                                
                Instruction::Call(name, argc) => {
                    let mut args: Vec<Value> = (0..argc)
                        .map(|_| self.pop())
                        .collect::<Result<Vec<_>, _>>()?;
                    args.reverse();

                    if let Some(func) = self.functions.get(&name).cloned() {
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
                    self.call_stack.pop();
                    if self.call_stack.is_empty() {
                        break;
                    }
                }
                Instruction::Halt => break,

                //    Colecciones                              
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
                            let item = items.get(i as usize).cloned()
                                .ok_or_else(|| format!("Índice {} fuera de rango", i))?;
                            self.value_stack.push(item);
                        }
                        (Value::Dict(map), Value::Str(key)) => {
                            let val = map.get(&key).cloned()
                                .ok_or_else(|| format!("Clave '{}' no encontrada", key))?;
                            self.value_stack.push(val);
                        }
                        _ => return Err("GetIndex: tipo no soportado".to_string()),
                    }
                }

                //    I/O nativo                               
                Instruction::Show => {
                    let val = self.pop()?;
                    println!("{}", val);
                }

                //    Stack                                     
                Instruction::Pop => { self.pop()?; }
                Instruction::Dup => {
                    let top = self.value_stack.last().cloned().ok_or("Stack vacío en Dup")?;
                    self.value_stack.push(top);
                }

                _ => return Err(format!("Instrucción no implementada: {:?}", instr)),
            }
        }
        Ok(())
    }

    fn pop(&mut self) -> Result<Value, String> {
        self.value_stack.pop().ok_or_else(|| "Stack vacío".to_string())
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
                Ok(Some(Value::Str(val.type_name().to_string())))
            }
            "show" => {
                for arg in args { print!("{} ", arg); }
                println!();
                Ok(None)
            }
            other => Err(format!("Función '{}' no definida", other)),
        }
    }
}
