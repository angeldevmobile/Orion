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
use indexmap::IndexMap;
use crate::value::{Value, InstanceData, ListData};

type EnvData = IndexMap<String, Value>;

/// `true` si el valor puede formar parte de un ciclo de referencias.
/// Un ciclo solo puede cerrarse almacenando un contenedor dentro de otro;
/// los escalares nunca participan.
pub fn is_container(v: &Value) -> bool {
    matches!(v, Value::List(_) | Value::Dict(_) | Value::Instance(_) | Value::Closure { .. })
}

pub struct Gc {
    /// Registro débil de todos los `Rc<RefCell<InstanceData>>` vivos.
    heap: Vec<Weak<RefCell<InstanceData>>>,
    /// Listas candidatas a ciclo. NO se registra cada lista al crearse (sería
    /// costo en el camino caliente): un ciclo solo puede cerrarse cuando una
    /// mutación (push/append/set-index) guarda un contenedor dentro de una
    /// lista, así que se registra SOLO la lista receptora en ese momento.
    /// Todo ciclo queda con al menos un miembro registrado, y vaciar uno
    /// rompe el ciclo entero (el resto cae por conteo de Rc).
    lists: Vec<Weak<RefCell<ListData>>>,
    /// Dedup de `lists` por puntero. Se reconstruye en cada collect; si el
    /// allocator recicla una dirección entre collects, a lo sumo se retrasa
    /// el registro de esa lista hasta después del siguiente collect (fuga
    /// acotada, nunca corrupción).
    lists_seen: HashSet<*const RefCell<ListData>>,
    /// Entornos de closures (registrados al crear la closure; un env puede
    /// ciclarse consigo mismo vía el write-back de una closure recursiva).
    envs: Vec<Weak<RefCell<EnvData>>>,
    envs_seen: HashSet<*const RefCell<EnvData>>,
    /// Asignaciones registradas desde la última colección.
    alloc_since_collect: usize,
    /// Colectar cuando `alloc_since_collect` supera este umbral.
    threshold: usize,
}

impl Gc {
    pub fn new() -> Self {
        Gc {
            heap: Vec::new(),
            lists: Vec::new(),
            lists_seen: HashSet::new(),
            envs: Vec::new(),
            envs_seen: HashSet::new(),
            alloc_since_collect: 0,
            threshold: 512,
        }
    }

    /// Registra una nueva instancia recién creada.
    pub fn register(&mut self, rc: &Rc<RefCell<InstanceData>>) {
        self.heap.push(Rc::downgrade(rc));
        self.alloc_since_collect += 1;
    }

    /// Registra una lista que acaba de recibir un contenedor (ver `lists`).
    pub fn register_list(&mut self, rc: &Rc<RefCell<ListData>>) {
        if self.lists_seen.insert(Rc::as_ptr(rc)) {
            self.lists.push(Rc::downgrade(rc));
            self.alloc_since_collect += 1;
        }
    }

    /// Registra el entorno capturado de una closure recién creada.
    pub fn register_env(&mut self, rc: &Rc<RefCell<EnvData>>) {
        if self.envs_seen.insert(Rc::as_ptr(rc)) {
            self.envs.push(Rc::downgrade(rc));
            self.alloc_since_collect += 1;
        }
    }

    pub fn should_collect(&self) -> bool {
        self.alloc_since_collect >= self.threshold
    }

    /// Ejecuta el ciclo completo mark-and-sweep.
    /// `roots` debe contener todos los `Value` accesibles desde el programa
    /// (value_stack + vars de todos los frames + self_instance + closure_env).
    /// IMPORTANTE: llamar solo desde un safepoint (ningún local de Rust puede
    /// retener Values fuera de los roots, o el sweep los corrompe).
    /// Devuelve el número de objetos liberados.
    pub fn collect(&mut self, roots: &[Value]) -> usize {
        self.alloc_since_collect = 0;

        // Eliminar Weak ya caídos (liberados naturalmente sin ciclo)
        self.heap.retain(|w| w.strong_count() > 0);
        self.lists.retain(|w| w.strong_count() > 0);
        self.envs.retain(|w| w.strong_count() > 0);

        if self.heap.is_empty() && self.lists.is_empty() && self.envs.is_empty() {
            self.lists_seen.clear();
            self.envs_seen.clear();
            return 0;
        }

        //    Mark
        let reach = mark_roots(roots);

        //    Sweep
        // Vaciar el contenido de lo no alcanzable → rompe el ciclo → Rc cae a 0.
        // El vaciado dispara drops en cascada; el Drop iterativo de
        // InstanceData/ListData (value.rs) acota la profundidad de llamadas.
        for weak in &self.heap {
            if let Some(rc) = weak.upgrade() {
                if !reach.instances.contains(&Rc::as_ptr(&rc)) {
                    rc.borrow_mut().fields.clear();
                }
            }
        }
        for weak in &self.lists {
            if let Some(rc) = weak.upgrade() {
                if !reach.lists.contains(&Rc::as_ptr(&rc)) {
                    rc.borrow_mut().0.clear();
                }
            }
        }
        for weak in &self.envs {
            if let Some(rc) = weak.upgrade() {
                if !reach.envs.contains(&Rc::as_ptr(&rc)) {
                    rc.borrow_mut().clear();
                }
            }
        }

        let before = self.heap.len() + self.lists.len() + self.envs.len();
        self.heap.retain(|w| w.strong_count() > 0);
        self.lists.retain(|w| w.strong_count() > 0);
        self.envs.retain(|w| w.strong_count() > 0);

        // Reconstruir los dedup con los supervivientes (los punteros de los
        // muertos pueden reciclarse y no deben bloquear registros futuros).
        self.lists_seen = self.lists.iter()
            .filter_map(|w| w.upgrade()).map(|rc| Rc::as_ptr(&rc)).collect();
        self.envs_seen = self.envs.iter()
            .filter_map(|w| w.upgrade()).map(|rc| Rc::as_ptr(&rc)).collect();

        before - (self.heap.len() + self.lists.len() + self.envs.len())
    }

    /// Número de instancias vivas según el heap del GC. Solo usado por tests.
    #[cfg(test)]
    pub fn heap_size(&self) -> usize {
        self.heap.iter().filter(|w| w.strong_count() > 0).count()
    }
}

/// Conjuntos de alcanzables por categoría, resultado del mark.
struct Reachable {
    instances: HashSet<*const RefCell<InstanceData>>,
    lists: HashSet<*const RefCell<ListData>>,
    envs: HashSet<*const RefCell<EnvData>>,
}

/// Marca todas las instancias alcanzables desde los roots.
///
/// ITERATIVO a propósito (worklist en el heap, no recursión): el call stack
/// nativo es de ~1MB y una cadena de 100k instancias enlazadas lo desbordaba.
/// Con la worklist el límite es la RAM.
///
/// Los sets de visitados cubren TODO lo que comparte backing por `Rc`
/// (instancias, listas, envs de closures), no solo instancias: desde las
/// listas por referencia, `push(a, a)` crea una lista que se contiene a sí
/// misma y sin su set el mark no terminaba nunca. Los dicts son por valor
/// (no pueden contenerse a sí mismos), así que no necesitan set.
/// Los tres sets son a la vez protección contra ciclos Y el resultado del
/// mark: el sweep barre lo registrado que no aparezca en ellos.
fn mark_roots(roots: &[Value]) -> Reachable {
    let mut reach = Reachable {
        instances: HashSet::new(),
        lists: HashSet::new(),
        envs: HashSet::new(),
    };

    let mut work: Vec<Value> = roots.to_vec();
    while let Some(val) = work.pop() {
        match val {
            Value::Instance(rc) => {
                if reach.instances.insert(Rc::as_ptr(&rc)) {
                    work.extend(rc.borrow().fields.values().cloned());
                }
            }
            Value::List(items) => {
                if reach.lists.insert(Rc::as_ptr(&items)) {
                    work.extend(items.borrow().iter().cloned());
                }
            }
            Value::Dict(map) => {
                work.extend(map.into_values());
            }
            Value::Closure { env, .. } => {
                if reach.envs.insert(Rc::as_ptr(&env)) {
                    work.extend(env.borrow().values().cloned());
                }
            }
            _ => {}
        }
    }
    reach
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

        // Limpieza: romper el ciclo antes de salir (LSan exige exit limpio)
        drop(roots);
        drop(a);
        drop(b);
        assert_eq!(gc.collect(&[]), 2, "sin roots el ciclo debe liberarse");
    }

    #[test]
    fn test_gc_deep_chain_no_stack_overflow() {
        // Cadena de 200k instancias enlazadas (lista enlazada profunda).
        // Con el mark recursivo esto desbordaba el call stack nativo;
        // con la worklist iterativa debe marcar todo sin inmutarse.
        let mut gc = Gc::new();
        const N: usize = 200_000;

        let mut head = make_inst("Node");
        gc.register(&head);
        for _ in 1..N {
            let node = make_inst("Node");
            node.borrow_mut().fields.insert("next".to_string(), Value::Instance(Rc::clone(&head)));
            gc.register(&node);
            head = node;
        }

        let roots = vec![Value::Instance(Rc::clone(&head))];
        let freed = gc.collect(&roots);
        assert_eq!(freed, 0, "toda la cadena es alcanzable desde head");
        assert_eq!(gc.heap_size(), N);
    }

    #[test]
    fn test_gc_self_referential_list_terminates() {
        // push(a, a) es legal desde las listas por referencia: la lista se
        // contiene a sí misma. Sin set de visitados para listas el mark no
        // terminaba nunca (recursión infinita → stack overflow).
        let mut gc = Gc::new();
        let inst = make_inst("Dentro");
        gc.register(&inst);

        let lst = Value::list(vec![Value::Int(1), Value::Instance(Rc::clone(&inst))]);
        if let Value::List(items) = &lst {
            let alias = Value::List(Rc::clone(items));
            items.borrow_mut().push(alias); // la lista se contiene a sí misma
            gc.register_list(items);
        }

        let freed = gc.collect(&[lst.clone()]);
        assert_eq!(freed, 0, "la instancia dentro de la lista cíclica es alcanzable");
        assert_eq!(gc.heap_size(), 1);

        // Limpieza: sin roots, el auto-ciclo debe liberarse (exit limpio)
        drop(lst);
        drop(inst);
        gc.collect(&[]);
    }

    #[test]
    fn test_gc_cyclic_closure_env_terminates() {
        // Env de closure que se referencia a sí mismo (una closure recursiva
        // capturada en su propio scope). Mismo peligro que la lista cíclica.
        let mut gc = Gc::new();
        let inst = make_inst("Capturada");
        gc.register(&inst);

        let env = Rc::new(RefCell::new(IndexMap::new()));
        env.borrow_mut().insert("yo".to_string(), Value::Closure {
            fn_name: "rec".to_string(),
            env: Rc::clone(&env),
        });
        env.borrow_mut().insert("dato".to_string(), Value::Instance(Rc::clone(&inst)));
        gc.register_env(&env);

        let root = Value::Closure { fn_name: "rec".to_string(), env };
        let freed = gc.collect(&[root.clone()]);
        assert_eq!(freed, 0, "la instancia capturada por el env cíclico es alcanzable");
        assert_eq!(gc.heap_size(), 1);

        // Limpieza: sin roots, el env cíclico debe liberarse (exit limpio)
        drop(root);
        drop(inst);
        gc.collect(&[]);
    }

    #[test]
    fn test_deep_nested_pure_lists_mark_and_drop() {
        // Torre de listas puras [[[[…]]]] de 200k niveles, sin instancias.
        // Cubre los DOS recorridos profundos: el mark del GC (worklist
        // iterativa) y el Drop de ListData (cola de drops) — antes el drop
        // glue recursivo de Rust desbordaba el stack al soltar la torre.
        let mut gc = Gc::new();
        let inst = make_inst("Fondo");
        gc.register(&inst);

        let mut v = Value::list(vec![Value::Instance(Rc::clone(&inst))]);
        for _ in 0..200_000 {
            v = Value::list(vec![v]);
        }

        let freed = gc.collect(&[v.clone()]);
        assert_eq!(freed, 0, "la instancia del fondo de la torre es alcanzable");

        drop(v); // desmonta 200k niveles — no debe desbordar el stack
    }

    #[test]
    fn test_gc_list_cycle_reclaimed() {
        // a → b (literal, sin registro) y b… no: a recibe a b por push →
        // solo 'a' queda registrada. Vaciar UN miembro debe bastar para que
        // el ciclo entero caiga por conteo de Rc.
        let mut gc = Gc::new();
        let (wa, wb) = {
            let a = Value::list(vec![Value::Int(1)]);
            let b = Value::list(vec![a.clone()]); // b → a (literal)
            let (Value::List(ra), Value::List(rb)) = (&a, &b) else { unreachable!() };
            ra.borrow_mut().0.push(b.clone()); // a → b: cierra el ciclo
            gc.register_list(ra); // lo que haría push() en la VM
            (Rc::downgrade(ra), Rc::downgrade(rb))
        };
        // Sin GC el ciclo mantiene vivas ambas listas
        assert!(wa.upgrade().is_some() && wb.upgrade().is_some());

        let freed = gc.collect(&[]);
        assert!(freed >= 1, "debió liberar la lista registrada del ciclo");
        assert!(wa.upgrade().is_none(), "'a' debe morir al romperse el ciclo");
        assert!(wb.upgrade().is_none(), "'b' cae por Rc al vaciarse 'a'");
    }

    #[test]
    fn test_gc_self_list_reclaimed() {
        // push(a, a): la lista se contiene a sí misma. Antes del registro de
        // listas esto fugaba para siempre (el Rc nunca llegaba a 0).
        let mut gc = Gc::new();
        let weak = {
            let a = Value::list(vec![Value::Int(1)]);
            let Value::List(ra) = &a else { unreachable!() };
            let alias = Value::List(Rc::clone(ra));
            ra.borrow_mut().0.push(alias);
            gc.register_list(ra);
            Rc::downgrade(ra)
        };
        assert!(weak.upgrade().is_some(), "el auto-ciclo la mantiene viva");
        gc.collect(&[]);
        assert!(weak.upgrade().is_none(), "push(a,a) ya no debe fugar");
    }

    #[test]
    fn test_gc_reachable_list_cycle_untouched() {
        // El mismo ciclo, pero alcanzable desde un root: NO debe vaciarse.
        let mut gc = Gc::new();
        let a = Value::list(vec![Value::Int(1)]);
        let Value::List(ra) = &a else { unreachable!() };
        let alias = Value::List(Rc::clone(ra));
        ra.borrow_mut().0.push(alias);
        gc.register_list(ra);

        let freed = gc.collect(&[a.clone()]);
        assert_eq!(freed, 0);
        assert_eq!(ra.borrow().len(), 2, "contenido intacto: [1, a]");

        // Limpieza: sin roots, el auto-ciclo debe liberarse (exit limpio)
        let weak = Rc::downgrade(ra);
        drop(a);
        gc.collect(&[]);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn test_gc_env_cycle_reclaimed() {
        // Env que contiene una closure que captura el mismo env (closure
        // recursiva). Sin registro de envs, fuga permanente.
        let mut gc = Gc::new();
        let weak = {
            let env = Rc::new(RefCell::new(IndexMap::new()));
            env.borrow_mut().insert("yo".to_string(), Value::Closure {
                fn_name: "rec".to_string(),
                env: Rc::clone(&env),
            });
            gc.register_env(&env);
            Rc::downgrade(&env)
        };
        assert!(weak.upgrade().is_some(), "el auto-ciclo lo mantiene vivo");
        gc.collect(&[]);
        assert!(weak.upgrade().is_none(), "el env cíclico debe liberarse");
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
