use std::fmt;
use std::collections::HashMap;

/// Tipos de valor nativos de Orion
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    List(Vec<Value>),
    Dict(HashMap<String, Value>),
    Null,
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
        }
    }
}

impl Value {
    /// Inferencia de tipo — devuelve el nombre del tipo como string
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_)   => "int",
            Value::Float(_) => "float",
            Value::Str(_)   => "string",
            Value::Bool(_)  => "bool",
            Value::List(_)  => "list",
            Value::Dict(_)  => "dict",
            Value::Null     => "null",
        }
    }

    /// Convierte a bool para condicionales
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b)   => *b,
            Value::Int(n)    => *n != 0,
            Value::Float(n)  => *n != 0.0,
            Value::Str(s)    => !s.is_empty(),
            Value::List(v)   => !v.is_empty(),
            Value::Dict(m)   => !m.is_empty(),
            Value::Null      => false,
        }
    }

    /// Suma: int+int, float+float, string+string
    pub fn add(&self, other: &Value) -> Result<Value, String> {
        match (self, other) {
            (Value::Int(a), Value::Int(b))     => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Int(a), Value::Float(b))   => Ok(Value::Float(*a as f64 + b)),
            (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a + *b as f64)),
            (Value::Str(a), Value::Str(b))     => Ok(Value::Str(format!("{}{}", a, b))),
            (Value::Str(a), b)                 => Ok(Value::Str(format!("{}{}", a, b))),
            (a, Value::Str(b))                 => Ok(Value::Str(format!("{}{}", a, b))),
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
        match (self, other) {
            (Value::Int(a), Value::Int(b))     => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b))     => a == b,
            (Value::Bool(a), Value::Bool(b))   => a == b,
            (Value::Null, Value::Null)         => true,
            _ => false,
        }
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
