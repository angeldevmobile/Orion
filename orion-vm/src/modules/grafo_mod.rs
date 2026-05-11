use crate::eval_value::EvalValue;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};
use petgraph::Graph;
use petgraph::graph::NodeIndex;
use petgraph::algo::astar;

type OrionGraph = Graph<String, f64>;

struct GraphStore {
    graphs:     HashMap<u64, OrionGraph>,
    node_index: HashMap<u64, HashMap<String, NodeIndex>>,
}

static STORE: OnceLock<Mutex<GraphStore>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn store() -> &'static Mutex<GraphStore> {
    STORE.get_or_init(|| Mutex::new(GraphStore {
        graphs:     HashMap::new(),
        node_index: HashMap::new(),
    }))
}

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // crear() → id Int
        "crear" | "create" => {
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            let mut st = store().lock().unwrap();
            st.graphs.insert(id, OrionGraph::new());
            st.node_index.insert(id, HashMap::new());
            Ok(EvalValue::Int(id as i64))
        }
        // nodo(id, nombre) → Bool  — agrega nodo si no existe
        "nodo" | "node" => {
            if args.len() < 2 { return Err("grafo.nodo requiere (id, nombre)".into()); }
            let gid  = to_u64(&args[0])?;
            let name = to_str(&args[1]);
            let mut st = store().lock().unwrap();
            if !st.graphs.contains_key(&gid) {
                return Err(format!("grafo {}: no existe", gid));
            }
            if !st.node_index[&gid].contains_key(&name) {
                let idx = st.graphs.get_mut(&gid).unwrap().add_node(name.clone());
                st.node_index.get_mut(&gid).unwrap().insert(name, idx);
            }
            Ok(EvalValue::Bool(true))
        }
        // arista(id, desde, hasta, peso?) → Bool
        "arista" | "edge" => {
            if args.len() < 3 { return Err("grafo.arista requiere (id, desde, hasta, peso?)".into()); }
            let gid   = to_u64(&args[0])?;
            let desde = to_str(&args[1]);
            let hasta = to_str(&args[2]);
            let peso  = args.get(3).and_then(|v| to_f64(v).ok()).unwrap_or(1.0);
            let mut st = store().lock().unwrap();
            if !st.graphs.contains_key(&gid) {
                return Err(format!("grafo {}: no existe", gid));
            }
            // Agregar nodos si no existen (borrows secuenciales, no simultáneos)
            if !st.node_index[&gid].contains_key(&desde) {
                let idx = st.graphs.get_mut(&gid).unwrap().add_node(desde.clone());
                st.node_index.get_mut(&gid).unwrap().insert(desde.clone(), idx);
            }
            if !st.node_index[&gid].contains_key(&hasta) {
                let idx = st.graphs.get_mut(&gid).unwrap().add_node(hasta.clone());
                st.node_index.get_mut(&gid).unwrap().insert(hasta.clone(), idx);
            }
            let a = st.node_index[&gid][&desde];
            let b = st.node_index[&gid][&hasta];
            st.graphs.get_mut(&gid).unwrap().add_edge(a, b, peso);
            Ok(EvalValue::Bool(true))
        }
        // camino(id, desde, hasta) → List<Str> de nodos o Null si no existe ruta
        "camino" | "path" => {
            if args.len() < 3 { return Err("grafo.camino requiere (id, desde, hasta)".into()); }
            let gid   = to_u64(&args[0])?;
            let desde = to_str(&args[1]);
            let hasta = to_str(&args[2]);
            let st = store().lock().unwrap();
            let g  = st.graphs.get(&gid).ok_or_else(|| format!("grafo {}: no existe", gid))?;
            let nm = st.node_index.get(&gid).unwrap();
            let src = *nm.get(&desde).ok_or_else(|| format!("grafo: nodo '{}' no existe", desde))?;
            let dst = *nm.get(&hasta).ok_or_else(|| format!("grafo: nodo '{}' no existe", hasta))?;
            match astar(g, src, |n| n == dst, |e| *e.weight(), |_| 0.0) {
                Some((_, path)) => Ok(EvalValue::List(
                    path.iter().map(|idx| EvalValue::Str(g[*idx].clone())).collect()
                )),
                None => Ok(EvalValue::Null),
            }
        }
        // vecinos(id, nodo) → List<Str>
        "vecinos" | "neighbors" => {
            if args.len() < 2 { return Err("grafo.vecinos requiere (id, nodo)".into()); }
            let gid  = to_u64(&args[0])?;
            let name = to_str(&args[1]);
            let st = store().lock().unwrap();
            let g  = st.graphs.get(&gid).ok_or_else(|| format!("grafo {}: no existe", gid))?;
            let nm = st.node_index.get(&gid).unwrap();
            let idx = *nm.get(&name).ok_or_else(|| format!("grafo: nodo '{}' no existe", name))?;
            let vecinos: Vec<EvalValue> = g.neighbors(idx)
                .map(|n| EvalValue::Str(g[n].clone()))
                .collect();
            Ok(EvalValue::List(vecinos))
        }
        // nodos(id) → List<Str> de todos los nodos
        "nodos" | "nodes" => {
            let gid = to_u64(args.first().ok_or("grafo.nodos requiere (id)")?)?;
            let st  = store().lock().unwrap();
            let g   = st.graphs.get(&gid).ok_or_else(|| format!("grafo {}: no existe", gid))?;
            Ok(EvalValue::List(g.node_weights().map(|n| EvalValue::Str(n.clone())).collect()))
        }
        // eliminar(id) → Bool
        "eliminar" | "delete" => {
            let gid = to_u64(args.first().ok_or("grafo.eliminar requiere (id)")?)?;
            let mut st = store().lock().unwrap();
            st.graphs.remove(&gid);
            st.node_index.remove(&gid);
            Ok(EvalValue::Bool(true))
        }
        f => Err(format!("grafo.{}() no existe", f)),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}

fn to_u64(v: &EvalValue) -> Result<u64, String> {
    match v {
        EvalValue::Int(n) if *n > 0 => Ok(*n as u64),
        other => Err(format!("grafo: id debe ser positivo, recibió {}", other.type_name())),
    }
}

fn to_f64(v: &EvalValue) -> Result<f64, String> {
    match v {
        EvalValue::Float(f) => Ok(*f),
        EvalValue::Int(n)   => Ok(*n as f64),
        other => Err(format!("grafo: peso debe ser número, recibió {}", other.type_name())),
    }
}
