//! Mark-and-sweep GC para instancias de Orion (`Rc<RefCell<InstanceData>>`).
//!
//! Problema: `Value::Instance` usa `Rc<RefCell<InstanceData>>` con campos que pueden
//! apuntar a otras instancias → ciclos → el conteo de referencias nunca llega a 0 → memory leak.
//!
//! Solución: mantener un registro de todos los `Rc` asignados como `Weak`. Al colectar:
//!   1. Mark — recorrer desde los roots (stack + vars) y marcar instancias alcanzables.
//!   2. Sweep — las no marcadas son ciclos; vaciamos sus `fields` para romper el ciclo.
//!             El `Rc` baja a 0 y Rust los libera solos.

use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::collections::HashSet;
use crate::value::{Value, InstanceData};

pub struct Gc {
    /// Registro débil de todos los `Rc<RefCell<InstanceData>>` vivos.
    heap: Vec<Weak<RefCell<InstanceData>>>,
    /// Instancias asignadas desde la última colección.
    alloc_since_collect: usize,
    /// Colectar cuando `alloc_since_collect` supera este umbral.
    threshold: usize,
}

impl Gc {
    pub fn new() -> Self {
        Gc {
            heap: Vec::new(),
            alloc_since_collect: 0,
            threshold: 512,
        }
    }

    /// Registra una nueva instancia recién creada.
    pub fn register(&mut self, rc: &Rc<RefCell<InstanceData>>) {
        self.heap.push(Rc::downgrade(rc));
        self.alloc_since_collect += 1;
    }

    pub fn should_collect(&self) -> bool {
        self.alloc_since_collect >= self.threshold
    }

    /// Ejecuta el ciclo completo mark-and-sweep.
    /// `roots` debe contener todos los `Value` accesibles desde el programa
    /// (value_stack + vars de todos los frames + self_instance de cada frame).
    /// Devuelve el número de instancias liberadas.
    pub fn collect(&mut self, roots: &[Value]) -> usize {
        self.alloc_since_collect = 0;

        // Eliminar Weak ya caídos (liberados naturalmente sin ciclo)
        self.heap.retain(|w| w.strong_count() > 0);

        if self.heap.is_empty() {
            return 0;
        }

        //    Mark                                                             
        let mut marked: HashSet<*const RefCell<InstanceData>> = HashSet::new();
        for root in roots {
            mark_value(root, &mut marked);
        }

        //    Sweep                                                            
        // Vaciar fields de instancias no alcanzables → rompe ciclos → Rc.count cae
        for weak in &self.heap {
            if let Some(rc) = weak.upgrade() {
                let ptr = Rc::as_ptr(&rc);
                if !marked.contains(&ptr) {
                    rc.borrow_mut().fields.clear();
                }
            }
        }

        let before = self.heap.len();
        self.heap.retain(|w| w.strong_count() > 0);
        before - self.heap.len()
    }

    /// Número de instancias vivas según el heap del GC.
    pub fn heap_size(&self) -> usize {
        self.heap.iter().filter(|w| w.strong_count() > 0).count()
    }
}

/// Marca recursivamente todas las instancias alcanzables desde `val`.
fn mark_value(val: &Value, marked: &mut HashSet<*const RefCell<InstanceData>>) {
    match val {
        Value::Instance(rc) => {
            let ptr = Rc::as_ptr(rc);
            if marked.insert(ptr) {
                // Primera visita — descender en los fields (puede haber ciclos, insert los para)
                let fields: Vec<Value> = rc.borrow().fields.values().cloned().collect();
                for fv in &fields {
                    mark_value(fv, marked);
                }
            }
        }
        Value::List(items) => {
            for item in items {
                mark_value(item, marked);
            }
        }
        Value::Dict(map) => {
            for v in map.values() {
                mark_value(v, marked);
            }
        }
        Value::Closure { env, .. } => {
            for v in env.values() {
                mark_value(v, marked);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn make_inst(name: &str) -> Rc<RefCell<InstanceData>> {
        Rc::new(RefCell::new(InstanceData {
            shape_name: name.to_string(),
            fields: IndexMap::new(),
        }))
    }

    #[test]
    fn test_gc_empty_heap() {
        let mut gc = Gc::new();
        let freed = gc.collect(&[]);
        assert_eq!(freed, 0);
        assert_eq!(gc.heap_size(), 0);
    }

    #[test]
    fn test_gc_reachable_instance_not_freed() {
        let mut gc = Gc::new();
        let inst = make_inst("Punto");
        gc.register(&inst);

        let roots = vec![Value::Instance(Rc::clone(&inst))];
        let freed = gc.collect(&roots);

        assert_eq!(freed, 0);
        assert_eq!(gc.heap_size(), 1);
    }

    #[test]
    fn test_gc_unreachable_non_cycle_freed_naturally() {
        // Objetos sin ciclo se liberan solos cuando el Rc sale de scope.
        // El GC solo limpia el Weak muerto del heap interno.
        let mut gc = Gc::new();
        let weak = {
            let inst = make_inst("Temp");
            gc.register(&inst);
            let w = Rc::downgrade(&inst);
            w // inst se dropea aquí → sin ciclo → Rc.count cae a 0 → freed
        };

        // Ya fue liberado naturalmente antes de llamar al GC
        assert!(weak.upgrade().is_none(), "debe estar liberado por Rc normal");

        // El GC limpia el Weak muerto del heap
        gc.collect(&[]);
        assert_eq!(gc.heap_size(), 0);
    }

    #[test]
    fn test_gc_cycle_broken() {
        // a.ref = b, b.ref = a  → ciclo puro.
        // Sin el GC, el ciclo mantiene vivos a ambos aunque nada los referencia.
        // El GC debe romper el ciclo vaciando los fields → Rc.count cae a 0.
        let mut gc = Gc::new();

        // Guardamos solo referencias débiles para comprobar al final
        let (weak_a, weak_b) = {
            let a = make_inst("A");
            let b = make_inst("B");

            // Crear el ciclo
            a.borrow_mut().fields.insert("ref".to_string(), Value::Instance(Rc::clone(&b)));
            b.borrow_mut().fields.insert("ref".to_string(), Value::Instance(Rc::clone(&a)));

            gc.register(&a);
            gc.register(&b);

            (Rc::downgrade(&a), Rc::downgrade(&b))
            // a y b (los Rc locales) se dropean aquí, pero el ciclo los mantiene vivos
        };

        // Sin GC ambos siguen vivos gracias al ciclo
        assert!(weak_a.upgrade().is_some(), "ciclo debe mantener 'a' vivo");
        assert!(weak_b.upgrade().is_some(), "ciclo debe mantener 'b' vivo");

        // GC sin roots → rompe el ciclo
        let freed = gc.collect(&[]);

        assert_eq!(freed, 2, "el GC debió liberar 2 instancias del ciclo");
        assert!(weak_a.upgrade().is_none(), "'a' debe estar liberado tras el GC");
        assert!(weak_b.upgrade().is_none(), "'b' debe estar liberado tras el GC");
    }

    #[test]
    fn test_gc_partial_cycle() {
        // root → a → b → a (ciclo), pero root mantiene a (y b) vivos
        let mut gc = Gc::new();
        let a = make_inst("A");
        let b = make_inst("B");

        a.borrow_mut().fields.insert("next".to_string(), Value::Instance(Rc::clone(&b)));
        b.borrow_mut().fields.insert("back".to_string(), Value::Instance(Rc::clone(&a)));

        gc.register(&a);
        gc.register(&b);

        // root apunta a 'a' — tanto a como b son alcanzables desde la raíz
        let roots = vec![Value::Instance(Rc::clone(&a))];
        let freed = gc.collect(&roots);

        assert_eq!(freed, 0, "ninguno debe liberarse con un root activo");
        assert_eq!(gc.heap_size(), 2);
    }

    #[test]
    fn test_gc_threshold() {
        let mut gc = Gc::new();
        gc.threshold = 3;
        assert!(!gc.should_collect());

        let a = make_inst("A");
        let b = make_inst("B");
        let c = make_inst("C");
        gc.register(&a);
        gc.register(&b);
        assert!(!gc.should_collect());
        gc.register(&c);
        assert!(gc.should_collect());
    }
}
