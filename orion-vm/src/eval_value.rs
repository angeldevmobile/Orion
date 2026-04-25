use std::collections::HashMap;
use std::fmt;
use serde_json::Value as Json;

/// Tipo de valor del evaluador de árbol (independiente del bytecode VM).
#[derive(Debug, Clone)]
pub enum EvalValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    List(Vec<EvalValue>),
    Dict(HashMap<String, EvalValue>),
    Function {
        name:    String,
        params:  Vec<String>,
        body:    Vec<Json>,
        closure: HashMap<String, EvalValue>,
    },
    /// Definición de shape (tipo OOP de Orion). Se crea con SHAPE_DEF.
    Shape {
        name:      String,
        fields:    Vec<(String, EvalValue)>,                     // nombre → valor por defecto
        on_create: Option<(Vec<String>, Vec<Json>)>,             // (params, body)
        acts:      HashMap<String, (Vec<String>, Vec<Json>)>,    // nombre → (params, body)
    },
    /// Instancia creada al llamar un Shape como constructor.
    Instance {
        shape_name: String,
        fields:     HashMap<String, EvalValue>,
        acts:       HashMap<String, (Vec<String>, Vec<Json>)>,
    },
    Null,
}

impl fmt::Display for EvalValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalValue::Int(n)   => write!(f, "{}", n),
            EvalValue::Float(n) => {
                if n.fract() == 0.0 { write!(f, "{}", *n as i64) }
                else { write!(f, "{}", n) }
            }
            EvalValue::Str(s)   => write!(f, "{}", s),
            EvalValue::Bool(b)  => write!(f, "{}", if *b { "yes" } else { "no" }),
            EvalValue::Null     => write!(f, "null"),
            EvalValue::List(v)  => {
                write!(f, "[")?;
                for (i, item) in v.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    match item {
                        EvalValue::Str(s) => write!(f, "\"{}\"", s)?,
                        other             => write!(f, "{}", other)?,
                    }
                }
                write!(f, "]")
            }
            EvalValue::Dict(m)  => {
                write!(f, "{{")?;
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "\"{}\": {}", k, v)?;
                }
                write!(f, "}}")
            }
            EvalValue::Function { name, .. } => write!(f, "<fn {}>", name),
            EvalValue::Shape    { name, .. } => write!(f, "<shape {}>", name),
            EvalValue::Instance { shape_name, fields, .. } => {
                write!(f, "{} {{", shape_name)?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

impl EvalValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            EvalValue::Int(_)       => "number",
            EvalValue::Float(_)     => "number",
            EvalValue::Str(_)       => "string",
            EvalValue::Bool(_)      => "bool",
            EvalValue::List(_)      => "list",
            EvalValue::Dict(_)      => "dict",
            EvalValue::Function{..} => "function",
            EvalValue::Shape{..}    => "shape",
            EvalValue::Instance{..} => "instance",
            EvalValue::Null         => "null",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            EvalValue::Bool(b)      => *b,
            EvalValue::Null         => false,
            EvalValue::Int(n)       => *n != 0,
            EvalValue::Float(f)     => *f != 0.0,
            EvalValue::Str(s)       => !s.is_empty(),
            EvalValue::List(v)      => !v.is_empty(),
            EvalValue::Dict(m)      => !m.is_empty(),
            EvalValue::Function{..} => true,
            EvalValue::Shape{..}    => true,
            EvalValue::Instance{..} => true,
        }
    }

    pub fn to_i64(&self) -> Result<i64, String> {
        match self {
            EvalValue::Int(n)   => Ok(*n),
            EvalValue::Float(f) => Ok(*f as i64),
            EvalValue::Str(s)   => s.trim().parse::<i64>()
                .map_err(|_| format!("No se puede convertir '{}' a int", s)),
            EvalValue::Bool(b)  => Ok(if *b { 1 } else { 0 }),
            other => Err(format!("No se puede convertir {} a int", other.type_name())),
        }
    }

    pub fn to_f64(&self) -> Result<f64, String> {
        match self {
            EvalValue::Float(f) => Ok(*f),
            EvalValue::Int(n)   => Ok(*n as f64),
            EvalValue::Str(s)   => s.trim().parse::<f64>()
                .map_err(|_| format!("No se puede convertir '{}' a float", s)),
            other => Err(format!("No se puede convertir {} a float", other.type_name())),
        }
    }
}
