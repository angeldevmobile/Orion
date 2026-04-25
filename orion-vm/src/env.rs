use std::collections::HashMap;
use crate::eval_value::EvalValue;

/// Entorno de variables con cadena de scopes.
/// El último elemento del Vec es el scope más interno (actual).
pub struct Env {
    scopes: Vec<HashMap<String, EvalValue>>,
}

impl Env {
    pub fn new() -> Self {
        Env { scopes: vec![HashMap::new()] }
    }

    /// Crea un entorno a partir de un snapshot (para closures de funciones).
    pub fn from_snapshot(snap: HashMap<String, EvalValue>) -> Self {
        Env { scopes: vec![snap] }
    }

    /// Abre un nuevo scope (al entrar a un bloque).
    pub fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Cierra el scope actual.
    pub fn pop(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Busca una variable en la cadena de scopes (interior → exterior).
    pub fn get(&self, name: &str) -> Option<EvalValue> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    /// Asigna una variable: actualiza en el scope donde ya existe,
    /// o crea en el scope actual si es nueva.
    pub fn set(&mut self, name: &str, value: EvalValue) {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return;
            }
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), value);
        }
    }

    /// Siempre crea/sobreescribe en el scope más interno (para parámetros).
    pub fn set_local(&mut self, name: &str, value: EvalValue) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), value);
        }
    }

    /// Captura una copia plana de todas las variables visibles (para closures).
    pub fn snapshot(&self) -> HashMap<String, EvalValue> {
        let mut snap = HashMap::new();
        for scope in &self.scopes {
            for (k, v) in scope {
                snap.insert(k.clone(), v.clone());
            }
        }
        snap
    }
}
