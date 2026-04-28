use std::fmt;
use std::rc::Rc;
use indexmap::IndexMap;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

/// Datos internos de una instancia de shape
#[derive(Debug, Clone)]
pub struct InstanceData {
    pub shape_name: String,
    pub fields: IndexMap<String, Value>,
}

/// Versión thread-safe de Value (sin Rc) para paso entre tareas async
#[derive(Debug, Clone)]
pub enum SendValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<SendValue>),
    Dict(IndexMap<String, SendValue>),
}

/// Convierte un SendValue de vuelta a Value
pub fn from_send(sv: SendValue) -> Value {
    match sv {
        SendValue::Null        => Value::Null,
        SendValue::Bool(b)     => Value::Bool(b),
        SendValue::Int(n)      => Value::Int(n),
        SendValue::Float(f)    => Value::Float(f),
        SendValue::Str(s)      => Value::Str(s),
        SendValue::List(items) => Value::List(items.into_iter().map(from_send).collect()),
        SendValue::Dict(map)   => Value::Dict(map.into_iter().map(|(k, v)| (k, from_send(v))).collect()),
    }
}

/// Tipos de valor nativos de Orion
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    List(Vec<Value>),
    Dict(IndexMap<String, Value>),
    Instance(Rc<RefCell<InstanceData>>),
    /// Closure: función + variables capturadas del scope donde fue creada
    Closure { fn_name: String, env: IndexMap<String, Value> },
    /// Handle a una tarea asíncrona en curso
    Task(Arc<Mutex<Option<Result<SendValue, String>>>>),
    Null,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b))       => a == b,
            (Value::Float(a), Value::Float(b))   => a == b,
            (Value::Str(a), Value::Str(b))       => a == b,
            (Value::Bool(a), Value::Bool(b))     => a == b,
            (Value::Null, Value::Null)           => true,
            (Value::List(a), Value::List(b))     => a == b,
            (Value::Dict(a), Value::Dict(b))     => a == b,
            (Value::Instance(a), Value::Instance(b))  => Rc::ptr_eq(a, b),
            (Value::Closure { fn_name: a, .. }, Value::Closure { fn_name: b, .. }) => a == b,
            (Value::Task(a), Value::Task(b))            => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n)    => write!(f, "{}", n),
            Value::Float(n)  => write!(f, "{}", n),
            Value::Str(s)    => write!(f, "{}", s),
            Value::Bool(b)   => write!(f, "{}", if *b { "yes" } else { "no" }),
            Value::Null      => write!(f, "null"),
            Value::List(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", parts.join(", "))
            }
            Value::Dict(map) => {
                let parts: Vec<String> = map.iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect();
                write!(f, "{{{}}}", parts.join(", "))
            }
            Value::Instance(inst) => write!(f, "<{} instance>", inst.borrow().shape_name),
            Value::Closure { fn_name, .. } => write!(f, "<fn {}>", fn_name),
            Value::Task(_) => write!(f, "<tarea>"),
        }
    }
}

impl Value {
    pub fn type_name(&self) -> String {
        match self {
            Value::Int(_)      => "int".to_string(),
            Value::Float(_)    => "float".to_string(),
            Value::Str(_)      => "string".to_string(),
            Value::Bool(_)     => "bool".to_string(),
            Value::List(_)     => "list".to_string(),
            Value::Dict(_)     => "dict".to_string(),
            Value::Null                => "null".to_string(),
            Value::Instance(i)         => i.borrow().shape_name.clone(),
            Value::Closure { .. }      => "fn".to_string(),
            Value::Task(_)             => "tarea".to_string(),
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b)   => *b,
            Value::Int(n)    => *n != 0,
            Value::Float(n)  => *n != 0.0,
            Value::Str(s)    => !s.is_empty(),
            Value::List(v)   => !v.is_empty(),
            Value::Dict(m)   => !m.is_empty(),
            Value::Instance(_)    => true,
            Value::Closure { .. } => true,
            Value::Task(_)        => true,
            Value::Null           => false,
        }
    }

    /// Convierte este valor a SendValue para pasar a una tarea async.
    /// Falla si el valor contiene tipos no thread-safe (Instance, Task).
    pub fn to_send(&self) -> Result<SendValue, String> {
        match self {
            Value::Null        => Ok(SendValue::Null),
            Value::Bool(b)     => Ok(SendValue::Bool(*b)),
            Value::Int(n)      => Ok(SendValue::Int(*n)),
            Value::Float(f)    => Ok(SendValue::Float(*f)),
            Value::Str(s)      => Ok(SendValue::Str(s.clone())),
            Value::List(items) => {
                let sv: Result<Vec<_>, _> = items.iter().map(|v| v.to_send()).collect();
                Ok(SendValue::List(sv?))
            }
            Value::Dict(map) => {
                let mut m = IndexMap::new();
                for (k, v) in map {
                    m.insert(k.clone(), v.to_send()?);
                }
                Ok(SendValue::Dict(m))
            }
            Value::Closure { fn_name, .. } =>
                Ok(SendValue::Str(fn_name.clone())),
            Value::Instance(_) =>
                Err("No se puede pasar una instancia a una función async".to_string()),
            Value::Task(_) =>
                Err("No se puede pasar una tarea como argumento async".to_string()),
        }
    }

    pub fn add(&self, other: &Value) -> Result<Value, String> {
        match (self, other) {
            (Value::Int(a), Value::Int(b))     => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Int(a), Value::Float(b))   => Ok(Value::Float(*a as f64 + b)),
            (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a + *b as f64)),
            (Value::Str(a), Value::Str(b))         => Ok(Value::Str(format!("{}{}", a, b))),
            (Value::Str(a), b)                     => Ok(Value::Str(format!("{}{}", a, b))),
            (a, Value::Str(b))                     => Ok(Value::Str(format!("{}{}", a, b))),
            (Value::List(a), Value::List(b))        => {
                let mut result = a.clone();
                result.extend_from_slice(b);
                Ok(Value::List(result))
            }
            _ => Err(format!("No se puede sumar {} + {}", self.type_name(), other.type_name())),
        }
    }

    pub fn sub(&self, other: &Value) -> Result<Value, String> {
        match (self, other) {
            (Value::Int(a), Value::Int(b))     => Ok(Value::Int(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (Value::Int(a), Value::Float(b))   => Ok(Value::Float(*a as f64 - b)),
            (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a - *b as f64)),
            _ => Err(format!("No se puede restar {} - {}", self.type_name(), other.type_name())),
        }
    }

    pub fn mul(&self, other: &Value) -> Result<Value, String> {
        match (self, other) {
            (Value::Int(a), Value::Int(b))     => Ok(Value::Int(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (Value::Int(a), Value::Float(b))   => Ok(Value::Float(*a as f64 * b)),
            (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a * *b as f64)),
            _ => Err(format!("No se puede multiplicar {} * {}", self.type_name(), other.type_name())),
        }
    }

    pub fn div(&self, other: &Value) -> Result<Value, String> {
        match (self, other) {
            (_, Value::Int(0))   => Err("División por cero".to_string()),
            (_, Value::Float(f)) if *f == 0.0 => Err("División por cero".to_string()),
            (Value::Int(a), Value::Int(b))     => Ok(Value::Float(*a as f64 / *b as f64)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (Value::Int(a), Value::Float(b))   => Ok(Value::Float(*a as f64 / b)),
            (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a / *b as f64)),
            _ => Err(format!("No se puede dividir {} / {}", self.type_name(), other.type_name())),
        }
    }

    pub fn compare_eq(&self, other: &Value) -> bool {
        self == other
    }

    pub fn compare_lt(&self, other: &Value) -> Result<bool, String> {
        match (self, other) {
            (Value::Int(a), Value::Int(b))     => Ok(a < b),
            (Value::Float(a), Value::Float(b)) => Ok(a < b),
            (Value::Int(a), Value::Float(b))   => Ok((*a as f64) < *b),
            (Value::Float(a), Value::Int(b))   => Ok(*a < (*b as f64)),
            _ => Err(format!("No se puede comparar {} < {}", self.type_name(), other.type_name())),
        }
    }
}
