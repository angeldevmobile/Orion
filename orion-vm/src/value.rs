use std::fmt;
use std::rc::Rc;
use indexmap::IndexMap;
use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::{AtomicBool, Ordering};

/// Estado compartido de una tarea async lanzada con `spawn` o al invocar una
/// `async fn`. Reemplaza el antiguo `Arc<Mutex<Option<...>>>` con busy-wait:
///   - `result`/`done`  → parking real vía Condvar (await no gira en un sleep-loop).
///   - `cancel`         → cancelación cooperativa; la sub-VM la consulta en su
///                        bucle de instrucciones y aborta limpiamente si se activa.
#[derive(Debug)]
pub struct TaskHandle {
    result: Mutex<Option<Result<SendValue, String>>>,
    done:   Condvar,
    cancel: AtomicBool,
}

impl TaskHandle {
    pub fn new() -> Arc<Self> {
        Arc::new(TaskHandle {
            result: Mutex::new(None),
            done:   Condvar::new(),
            cancel: AtomicBool::new(false),
        })
    }

    /// Llamado por el worker cuando la tarea termina: guarda el resultado y
    /// despierta a todos los `await` que estén bloqueados en el Condvar.
    pub fn complete(&self, r: Result<SendValue, String>) {
        let mut guard = self.result.lock().unwrap();
        if guard.is_none() {
            *guard = Some(r);
        }
        self.done.notify_all();
    }

    /// Bloquea (aparcando el hilo) hasta que la tarea termine y devuelve su
    /// resultado. Sin espera activa: el SO nos despierta al `notify_all`.
    pub fn wait(&self) -> Result<SendValue, String> {
        let mut guard = self.result.lock().unwrap();
        while guard.is_none() {
            guard = self.done.wait(guard).unwrap();
        }
        guard.clone().unwrap()
    }

    /// ¿Ya terminó la tarea? (no bloquea)
    pub fn is_done(&self) -> bool {
        self.result.lock().unwrap().is_some()
    }

    /// Marca la tarea para cancelación cooperativa.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// ¿Se pidió cancelar esta tarea? Consultado por la sub-VM en su bucle.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// Datos internos de una instancia de shape
#[derive(Debug, Clone)]
pub struct InstanceData {
    pub shape_name: String,
    pub fields: IndexMap<String, Value>,
}

thread_local! {
    /// Cola de valores pendientes de destruir (ver `Drop for InstanceData`
    /// y `Drop for ListData`).
    static DROP_QUEUE: RefCell<Vec<Value>> = RefCell::new(Vec::new());
    /// `true` mientras un drop de nivel superior está drenando la cola.
    static DROP_DRAINING: Cell<bool> = Cell::new(false);
}

/// Drena la cola de drops si nadie la está drenando ya. Los drops anidados
/// que se disparan dentro del bucle solo encolan y retornan (guardado por
/// `DROP_DRAINING`), así la profundidad de llamadas queda acotada sea cual
/// sea la profundidad de la estructura que se destruye.
fn drain_drop_queue() {
    DROP_DRAINING.with(|draining| {
        if draining.get() {
            return; // ya hay un drenador más arriba en la pila
        }
        draining.set(true);
        loop {
            // Sacar con el borrow cerrado ANTES de soltar el valor: ese
            // drop puede re-entrar aquí y volver a encolar.
            let next = DROP_QUEUE.with(|q| q.borrow_mut().pop());
            match next {
                Some(v) => drop(v),
                None => break,
            }
        }
        draining.set(false);
    });
}

/// Drop iterativo: sin esto, soltar una cadena de instancias enlazadas
/// (nodo.next → nodo.next → …) destruye recursivamente y desborda el call
/// stack nativo con ~100k nodos. En vez de descender, cada instancia mueve
/// sus fields a la cola thread-local y deja que el drenador del nivel
/// superior los suelte en un bucle.
impl Drop for InstanceData {
    fn drop(&mut self) {
        if self.fields.is_empty() {
            return;
        }
        DROP_QUEUE.with(|q| {
            let mut q = q.borrow_mut();
            for (_, v) in self.fields.drain(..) {
                q.push(v);
            }
        });
        drain_drop_queue();
    }
}

/// Backing de una lista por referencia. Newtype sobre `Vec<Value>` con
/// `Deref`/`DerefMut` para que el resto del código use la API de Vec como
/// siempre; existe SOLO para poder colgarle el mismo Drop iterativo que a
/// `InstanceData` — sin él, soltar listas anidadas a mucha profundidad
/// ([[[[…]]]]) recorría la cadena Rc→RefCell→Vec→Value con el drop glue
/// recursivo de Rust y desbordaba el stack.
#[derive(Debug, Clone, PartialEq)]
pub struct ListData(pub Vec<Value>);

impl std::ops::Deref for ListData {
    type Target = Vec<Value>;
    fn deref(&self) -> &Vec<Value> { &self.0 }
}

impl std::ops::DerefMut for ListData {
    fn deref_mut(&mut self) -> &mut Vec<Value> { &mut self.0 }
}

impl Drop for ListData {
    fn drop(&mut self) {
        if self.0.is_empty() {
            return;
        }
        DROP_QUEUE.with(|q| q.borrow_mut().append(&mut self.0));
        drain_drop_queue();
    }
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
    Ptr(u64),
}

/// Convierte un SendValue de vuelta a Value
pub fn from_send(sv: SendValue) -> Value {
    match sv {
        SendValue::Null        => Value::Null,
        SendValue::Bool(b)     => Value::Bool(b),
        SendValue::Int(n)      => Value::Int(n),
        SendValue::Float(f)    => Value::Float(f),
        SendValue::Str(s)      => Value::Str(s),
        SendValue::List(items) => Value::list(items.into_iter().map(from_send).collect()),
        SendValue::Dict(map)   => Value::Dict(map.into_iter().map(|(k, v)| (k, from_send(v))).collect()),
        SendValue::Ptr(p)      => Value::Ptr(p),
    }
}

/// Tipos de valor nativos de Orion
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    /// Lista por referencia: el contenido es compartido (Rc<RefCell>) para que
    /// `xs.push(x)` y demás mutaciones se vean en todos los alias (`ys = xs`).
    /// La igualdad (`==`) sigue siendo estructural; `Clone` comparte el backing.
    List(Rc<RefCell<ListData>>),
    Dict(IndexMap<String, Value>),
    Instance(Rc<RefCell<InstanceData>>),
    /// Closure: función + variables capturadas del scope donde fue creada.
    /// El entorno es compartido (Rc<RefCell>) para que las mutaciones de las
    /// variables capturadas persistan entre invocaciones del mismo closure.
    Closure { fn_name: String, env: Rc<RefCell<IndexMap<String, Value>>> },
    /// Handle a una tarea asíncrona en curso (parking + cancelación).
    Task(Arc<TaskHandle>),
    /// Puntero opaco de C FFI (dirección de memoria como u64)
    Ptr(u64),
    /// Módulo stdlib nativo cargado con `use`
    Module(String),
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
            (Value::List(a), Value::List(b))     => *a.borrow() == *b.borrow(),
            (Value::Dict(a), Value::Dict(b))     => a == b,
            (Value::Ptr(a), Value::Ptr(b))       => a == b,
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
            Value::Ptr(p)    => write!(f, "<ptr 0x{:x}>", p),
            Value::List(items) => {
                let parts: Vec<String> = items.borrow().iter().map(|v| v.to_string()).collect();
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
            Value::Module(m) => write!(f, "<module {}>", m),
        }
    }
}

impl Value {
    /// Construye una lista nueva a partir de un Vec, envolviéndola en el backing
    /// compartido. Usar SIEMPRE esto en vez de `Value::List(vec)` directo.
    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(ListData(items))))
    }

    pub fn type_name(&self) -> String {
        match self {
            Value::Int(_)      => "int".to_string(),
            Value::Float(_)    => "float".to_string(),
            Value::Str(_)      => "string".to_string(),
            Value::Bool(_)     => "bool".to_string(),
            Value::List(_)     => "list".to_string(),
            Value::Dict(_)     => "dict".to_string(),
            Value::Ptr(_)      => "ptr".to_string(),
            Value::Null                => "null".to_string(),
            Value::Instance(i)         => i.borrow().shape_name.clone(),
            Value::Closure { .. }      => "fn".to_string(),
            Value::Task(_)             => "task".to_string(),
            Value::Module(m)           => format!("module<{}>", m),
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b)   => *b,
            Value::Int(n)    => *n != 0,
            Value::Float(n)  => *n != 0.0,
            Value::Str(s)    => !s.is_empty(),
            Value::List(v)   => !v.borrow().is_empty(),
            Value::Dict(m)   => !m.is_empty(),
            Value::Ptr(p)    => *p != 0,
            Value::Instance(_)    => true,
            Value::Closure { .. } => true,
            Value::Task(_)        => true,
            Value::Module(_)      => true,
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
                let sv: Result<Vec<_>, _> = items.borrow().iter().map(|v| v.to_send()).collect();
                Ok(SendValue::List(sv?))
            }
            Value::Dict(map) => {
                let mut m = IndexMap::new();
                for (k, v) in map {
                    m.insert(k.clone(), v.to_send()?);
                }
                Ok(SendValue::Dict(m))
            }
            Value::Ptr(p) => Ok(SendValue::Ptr(*p)),
            Value::Closure { fn_name, .. } =>
                Ok(SendValue::Str(fn_name.clone())),
            Value::Module(m) =>
                Ok(SendValue::Str(format!("<module {}>", m))),
            Value::Instance(_) =>
                Err("No se puede pasar una instancia a una función async".to_string()),
            Value::Task(_) =>
                Err("No se puede pasar una tarea como argumento async".to_string()),
        }
    }

    pub fn add(&self, other: &Value) -> Result<Value, String> {
        match (self, other) {
            (Value::Int(a), Value::Int(b))     => a.checked_add(*b).map(Value::Int)
                .ok_or_else(|| "Desbordamiento aritmético en suma de enteros".to_string()),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Int(a), Value::Float(b))   => Ok(Value::Float(*a as f64 + b)),
            (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a + *b as f64)),
            (Value::Str(a), Value::Str(b))         => Ok(Value::Str(format!("{}{}", a, b))),
            (Value::Str(a), b)                     => Ok(Value::Str(format!("{}{}", a, b))),
            (a, Value::Str(b))                     => Ok(Value::Str(format!("{}{}", a, b))),
            (Value::List(a), Value::List(b))        => {
                // Concatenación: produce una lista NUEVA e independiente (no muta
                // ninguno de los operandos). Snapshot de ambos backings.
                let mut result = a.borrow().0.clone();
                result.extend(b.borrow().iter().cloned());
                Ok(Value::list(result))
            }
            _ => Err(format!("No se puede sumar {} + {}", self.type_name(), other.type_name())),
        }
    }

    pub fn sub(&self, other: &Value) -> Result<Value, String> {
        match (self, other) {
            (Value::Int(a), Value::Int(b))     => a.checked_sub(*b).map(Value::Int)
                .ok_or_else(|| "Desbordamiento aritmético en resta de enteros".to_string()),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (Value::Int(a), Value::Float(b))   => Ok(Value::Float(*a as f64 - b)),
            (Value::Float(a), Value::Int(b))   => Ok(Value::Float(a - *b as f64)),
            _ => Err(format!("No se puede restar {} - {}", self.type_name(), other.type_name())),
        }
    }

    pub fn mul(&self, other: &Value) -> Result<Value, String> {
        match (self, other) {
            (Value::Int(a), Value::Int(b))     => a.checked_mul(*b).map(Value::Int)
                .ok_or_else(|| "Desbordamiento aritmético en multiplicación de enteros".to_string()),
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
            // Igualdad numérica con promoción int↔float, consistente con
            // `compare_lt` y con el runtime JIT (`rt_eq`): el operador `==` es
            // numérico, no estructural. (El `PartialEq` de abajo sigue siendo
            // estructural para igualdad de listas/dicts/instancias.)
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            _ => self == other,
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
