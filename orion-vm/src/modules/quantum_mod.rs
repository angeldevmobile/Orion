/// Orion Quantum — simulador cuántico en Rust puro.
/// Qubits representados como Vec de pares (re, im) = amplitudes complejas.
/// EvalValue: un estado N-qubit es List([List([re, im]), ...]) con 2^N elementos.
use crate::eval_value::EvalValue;
use indexmap::IndexMap as HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

// Número complejo (re, im)
type C = (f64, f64);

// Estado cuántico = vector de amplitudes complejas
type State = Vec<C>;

// Matriz cuántica = Vec<Vec<C>>
type Gate = Vec<Vec<C>>;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // qubit(alpha_re?, alpha_im?, beta_re?, beta_im?) → estado |qubit> (sin args: |0>)
        "qubit" | "zero" => {
            if args.is_empty() {
                return Ok(state_to_eval(&vec![(1.0, 0.0), (0.0, 0.0)]));
            }
            if args.len() < 4 {
                return Err("quantum.qubit: () para |0> o (alpha_re, alpha_im, beta_re, beta_im)".into());
            }
            let raw = vec![
                (to_f64v(&args[0])?, to_f64v(&args[1])?),
                (to_f64v(&args[2])?, to_f64v(&args[3])?),
            ];
            if raw.iter().map(|&a| c_abs2(a)).sum::<f64>() < 1e-15 {
                return Err("quantum.qubit: all amplitudes are zero (invalid state)".into());
            }
            Ok(state_to_eval(&normalize(raw)))
        }
        "one" => {
            let state = normalize(vec![(0.0, 0.0), (1.0, 0.0)]);
            Ok(state_to_eval(&state))
        }
        // rand() → qubit aleatorio
        "rand" => {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let theta: f64 = rng.gen::<f64>() * std::f64::consts::PI;
            let phi: f64   = rng.gen::<f64>() * 2.0 * std::f64::consts::PI;
            let state = normalize(vec![
                ((theta / 2.0).cos(), 0.0),
                ((theta / 2.0).sin() * phi.cos(), (theta / 2.0).sin() * phi.sin()),
            ]);
            Ok(state_to_eval(&state))
        }
        // bell() → par de Bell (|00> + |11>) / sqrt(2)
        "bell" => {
            let inv_sqrt2 = 1.0 / 2.0f64.sqrt();
            let state = vec![
                (inv_sqrt2, 0.0),
                (0.0, 0.0),
                (0.0, 0.0),
                (inv_sqrt2, 0.0),
            ];
            Ok(state_to_eval(&state))
        }
        // tensor(a, b) → producto tensorial de dos estados
        "tensor" | "entangle" => {
            if args.len() < 2 { return Err("quantum.tensor requires (a, b)".into()); }
            let a = eval_to_state(&args[0])?;
            let b = eval_to_state(&args[1])?;
            let result = tensor_product(&a, &b);
            Ok(state_to_eval(&normalize(result)))
        }
        // apply(state, gate) → aplica puerta al estado
        "apply" => {
            if args.len() < 2 { return Err("quantum.apply requires (state, gate)".into()); }
            let state = eval_to_state(&args[0])?;
            let gate  = eval_to_gate(&args[1])?;
            let result = apply_gate(&state, &gate)?;
            Ok(state_to_eval(&normalize(result)))
        }
        // measure(state, shots?) → dict con conteos {"0": n, "1": m, ...}
        "measure" => {
            if args.is_empty() { return Err("quantum.measure requires (state, shots?)".into()); }
            let state = eval_to_state(&args[0])?;
            let shots = if args.len() > 1 { to_i64(&args[1])? as usize } else { 1024 };
            let counts = measure(&state, shots);
            let m: HashMap<String, EvalValue> = counts.into_iter()
                .map(|(k, v)| (k, EvalValue::Int(v as i64)))
                .collect();
            Ok(EvalValue::Dict(m))
        }
        // measure_probs(state) → dict con probabilidades
        "measure_probs" | "probabilities" => {
            if args.is_empty() { return Err("quantum.measure_probs requires (state)".into()); }
            let state = eval_to_state(&args[0])?;
            let n_qubits = (state.len() as f64).log2() as usize;
            let m: HashMap<String, EvalValue> = state.iter().enumerate()
                .map(|(i, amp)| {
                    let prob = amp.0 * amp.0 + amp.1 * amp.1;
                    let key = format!("{:0>width$b}", i, width = n_qubits);
                    (key, EvalValue::Float((prob * 1e10).round() / 1e10))
                })
                .collect();
            Ok(EvalValue::Dict(m))
        }
        // fidelity(s1, s2) → f64 ∈ [0, 1]
        "fidelity" => {
            if args.len() < 2 { return Err("quantum.fidelity requires (s1, s2)".into()); }
            let s1 = eval_to_state(&args[0])?;
            let s2 = eval_to_state(&args[1])?;
            let inner = s1.iter().zip(&s2).map(|(a, b)| c_mul(c_conj(*a), *b)).fold((0.0, 0.0), c_add);
            let fid   = inner.0 * inner.0 + inner.1 * inner.1;
            Ok(EvalValue::Float((fid * 1e10).round() / 1e10))
        }
        // bloch(qubit_state) → [x, y, z]
        "bloch" => {
            if args.is_empty() { return Err("quantum.bloch requires (state)".into()); }
            let s  = eval_to_state(&args[0])?;
            if s.len() != 2 { return Err("quantum.bloch solo aplica a un qubit (2 amplitudes)".into()); }
            let a = s[0];
            let b = s[1];
            let rho01 = c_mul(a, c_conj(b));
            let x = 2.0 * rho01.0;
            let y = 2.0 * rho01.1;
            let z = a.0 * a.0 + a.1 * a.1 - (b.0 * b.0 + b.1 * b.1);
            Ok(EvalValue::List(vec![
                EvalValue::Float((x * 1e10).round() / 1e10),
                EvalValue::Float((y * 1e10).round() / 1e10),
                EvalValue::Float((z * 1e10).round() / 1e10),
            ]))
        }
        // state_from_bits("01") → estado |01>
        "state_from_bits" => {
            if args.is_empty() { return Err("quantum.state_from_bits requires (bitstring)".into()); }
            let bits = to_str(&args[0]);
            let n    = bits.len();
            let size = 1 << n;
            let idx  = usize::from_str_radix(&bits, 2).map_err(|_| "quantum.state_from_bits: invalid bits")?;
            let mut state = vec![(0.0f64, 0.0f64); size];
            state[idx] = (1.0, 0.0);
            Ok(state_to_eval(&state))
        }
        // Puertas estándar como funciones
        "gate_H"    => Ok(gate_to_eval(&hadamard())),
        "gate_X"    => Ok(gate_to_eval(&pauli_x())),
        "gate_Y"    => Ok(gate_to_eval(&pauli_y())),
        "gate_Z"    => Ok(gate_to_eval(&pauli_z())),
        "gate_S"    => Ok(gate_to_eval(&phase_s())),
        "gate_CNOT" => Ok(gate_to_eval(&cnot())),
        // amplitudes(state) → lista de [re, im, prob]
        "amplitudes" => {
            if args.is_empty() { return Err("quantum.amplitudes requires (state)".into()); }
            let state = eval_to_state(&args[0])?;
            let result: Vec<EvalValue> = state.iter().map(|(re, im)| {
                let prob = re * re + im * im;
                EvalValue::List(vec![
                    EvalValue::Float(*re),
                    EvalValue::Float(*im),
                    EvalValue::Float((prob * 1e10).round() / 1e10),
                ])
            }).collect();
            Ok(EvalValue::List(result))
        }

        // ── Circuitos (registro de n qubits, puertas por qubit, O(2^n)) ──────

        // circuit(n) → id; registro de n qubits inicializado en |0...0> (máx 24)
        "circuit" | "circuito" => {
            let n = to_i64(args.first().ok_or("quantum.circuit requires (n_qubits)")?)? as usize;
            if n == 0 || n > MAX_QUBITS {
                return Err(format!("quantum.circuit: n must be between 1 and {} qubits", MAX_QUBITS));
            }
            let mut state = vec![(0.0, 0.0); 1 << n];
            state[0] = (1.0, 0.0);
            let id = NEXT_CIRCUIT.fetch_add(1, Ordering::SeqCst);
            with_circuits(|cs| cs.insert(id, Circuit { n, state }));
            Ok(EvalValue::Int(id as i64))
        }
        // h(id, q) → Hadamard sobre el qubit q
        "h" => circuit_gate("h", args, false),
        // x(id, q) → NOT cuántico sobre el qubit q
        "x" => circuit_gate("x", args, false),
        // y(id, q) → Pauli-Y sobre el qubit q
        "y" => circuit_gate("y", args, false),
        // z(id, q) → Pauli-Z sobre el qubit q
        "z" => circuit_gate("z", args, false),
        // sgate(id, q) → puerta de fase S (π/2)
        "sgate" => circuit_gate("s", args, false),
        // tgate(id, q) → puerta de fase T (π/4)
        "tgate" => circuit_gate("t", args, false),
        // rx(id, q, theta) → rotación paramétrica en X (radianes)
        "rx" => circuit_gate("rx", args, true),
        // ry(id, q, theta) → rotación paramétrica en Y (radianes)
        "ry" => circuit_gate("ry", args, true),
        // rz(id, q, theta) → rotación paramétrica en Z (radianes)
        "rz" => circuit_gate("rz", args, true),
        // phase(id, q, theta) → fase relativa e^(i·theta) sobre |1>
        "phase" => circuit_gate("phase", args, true),
        // cnot(id, control, target) → X sobre target si control es 1
        "cnot" | "cx" => circuit_cgate("cnot", args, false),
        // cz(id, control, target) → Z controlada
        "cz" => circuit_cgate("cz", args, false),
        // cphase(id, control, target, theta) → fase controlada (para QFT)
        "cphase" => circuit_cgate("cphase", args, true),
        // swap(id, a, b) → intercambia dos qubits (3 CNOTs)
        "swap" => {
            let id = circuit_id(&args)?;
            with_circuits(|cs| {
                let circ = cs.get_mut(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
                let a = qubit_arg(&args, 1, circ.n, "swap")?;
                let b = qubit_arg(&args, 2, circ.n, "swap")?;
                if a == b { return Err("quantum.swap: the qubits must be different".into()); }
                let x = named_gate("x", 0.0).unwrap();
                apply_1q(circ, b, &x, &[a]);
                apply_1q(circ, a, &x, &[b]);
                apply_1q(circ, b, &x, &[a]);
                Ok(EvalValue::Int(id as i64))
            })
        }
        // ccx(id, c1, c2, target) → Toffoli: X si ambos controles son 1
        "ccx" | "toffoli" => {
            let id = circuit_id(&args)?;
            with_circuits(|cs| {
                let circ = cs.get_mut(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
                let c1 = qubit_arg(&args, 1, circ.n, "ccx")?;
                let c2 = qubit_arg(&args, 2, circ.n, "ccx")?;
                let t  = qubit_arg(&args, 3, circ.n, "ccx")?;
                if c1 == c2 || c1 == t || c2 == t { return Err("quantum.ccx: the three qubits must be different".into()); }
                let x = named_gate("x", 0.0).unwrap();
                apply_1q(circ, t, &x, &[c1, c2]);
                Ok(EvalValue::Int(id as i64))
            })
        }
        // ugate(id, q, matriz2x2) → puerta DEFINIDA POR EL USUARIO (se valida unitariedad)
        "ugate" => {
            if args.len() < 3 { return Err("quantum.ugate requires (id, qubit, matriz 2x2)".into()); }
            let id = circuit_id(&args)?;
            let g  = eval_to_gate2(&args[2])?;
            with_circuits(|cs| {
                let circ = cs.get_mut(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
                let q = qubit_arg(&args, 1, circ.n, "ugate")?;
                apply_1q(circ, q, &g, &[]);
                Ok(EvalValue::Int(id as i64))
            })
        }
        // cugate(id, control, target, matriz2x2) → puerta custom controlada
        "cugate" => {
            if args.len() < 4 { return Err("quantum.cugate requires (id, control, target, matriz 2x2)".into()); }
            let id = circuit_id(&args)?;
            let g  = eval_to_gate2(&args[3])?;
            with_circuits(|cs| {
                let circ = cs.get_mut(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
                let ctrl = qubit_arg(&args, 1, circ.n, "cugate")?;
                let tgt  = qubit_arg(&args, 2, circ.n, "cugate")?;
                if ctrl == tgt { return Err("quantum.cugate: control and target must be different".into()); }
                apply_1q(circ, tgt, &g, &[ctrl]);
                Ok(EvalValue::Int(id as i64))
            })
        }
        // state(id) → amplitudes del circuito (mismo formato que zero()/bell())
        "state" => {
            let id = circuit_id(&args)?;
            with_circuits(|cs| {
                let circ = cs.get(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
                Ok(state_to_eval(&circ.state))
            })
        }
        // probs(id) → dict {"010": prob, ...} solo con probabilidades > 1e-12
        "probs" => {
            let id = circuit_id(&args)?;
            with_circuits(|cs| {
                let circ = cs.get(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
                let m: HashMap<String, EvalValue> = circ.state.iter().enumerate()
                    .filter_map(|(i, &amp)| {
                        let p = c_abs2(amp);
                        if p < 1e-12 { return None; }
                        let key = format!("{:0>width$b}", i, width = circ.n);
                        Some((key, EvalValue::Float((p * 1e10).round() / 1e10)))
                    })
                    .collect();
                Ok(EvalValue::Dict(m))
            })
        }
        // sample(id, shots?) → dict de conteos {"010": n, ...} (regla de Born, no colapsa)
        "sample" => {
            let id = circuit_id(&args)?;
            let shots = match args.get(1) { Some(v) => to_i64(v)? as usize, None => 1024 };
            with_circuits(|cs| {
                let circ = cs.get(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
                let counts = measure(&circ.state, shots);
                let m: HashMap<String, EvalValue> = counts.into_iter()
                    .map(|(k, v)| (k, EvalValue::Int(v as i64)))
                    .collect();
                Ok(EvalValue::Dict(m))
            })
        }
        // collapse(id, q) → mide el qubit q: devuelve 0 o 1 y COLAPSA el estado
        "collapse" => {
            let id = circuit_id(&args)?;
            with_circuits(|cs| {
                let circ = cs.get_mut(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
                let q = qubit_arg(&args, 1, circ.n, "collapse")?;
                let bit = 1usize << (circ.n - 1 - q);
                let p1: f64 = circ.state.iter().enumerate()
                    .filter(|(i, _)| i & bit != 0)
                    .map(|(_, &a)| c_abs2(a)).sum();
                use rand::Rng;
                let outcome = if rand::thread_rng().gen::<f64>() < p1 { 1usize } else { 0 };
                for (i, a) in circ.state.iter_mut().enumerate() {
                    let has_bit = (i & bit != 0) as usize;
                    if has_bit != outcome { *a = (0.0, 0.0); }
                }
                circ.state = normalize(std::mem::take(&mut circ.state));
                Ok(EvalValue::Int(outcome as i64))
            })
        }
        // reset(id) → devuelve el circuito a |0...0>
        "reset" => {
            let id = circuit_id(&args)?;
            with_circuits(|cs| {
                let circ = cs.get_mut(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
                circ.state.iter_mut().for_each(|a| *a = (0.0, 0.0));
                circ.state[0] = (1.0, 0.0);
                Ok(EvalValue::Int(id as i64))
            })
        }
        // nqubits(id) → número de qubits del circuito
        "nqubits" => {
            let id = circuit_id(&args)?;
            with_circuits(|cs| {
                let circ = cs.get(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
                Ok(EvalValue::Int(circ.n as i64))
            })
        }
        // free(id) → libera el circuito; yes si existía
        "free" => {
            let id = circuit_id(&args)?;
            Ok(EvalValue::Bool(with_circuits(|cs| cs.shift_remove(&id).is_some())))
        }

        f => Err(format!("quantum.{}() does not exist", f)),
    }
}

//     Circuitos: registro de n qubits con puertas dirigidas a qubits concretos
//
//     Las puertas de 1 qubit se aplican en O(2^n) recorriendo pares de índices
//     que difieren solo en el bit del qubit objetivo — nunca se construye la
//     matriz 2^n × 2^n. Es el mismo esquema de los simuladores de verdad y
//     permite ~24 qubits en un portátil. Convención: qubit 0 = bit más
//     significativo (coincide con state_from_bits y las claves "010...").

struct Circuit {
    n:     usize,
    state: State,
}

const MAX_QUBITS: usize = 24; // 2^24 amplitudes × 16 bytes = 256 MB

static CIRCUITS: Mutex<Option<HashMap<u64, Circuit>>> = Mutex::new(None);
static NEXT_CIRCUIT: AtomicU64 = AtomicU64::new(1);

fn with_circuits<F, T>(f: F) -> T
where
    F: FnOnce(&mut HashMap<u64, Circuit>) -> T,
{
    let mut guard = CIRCUITS.lock().unwrap();
    if guard.is_none() { *guard = Some(HashMap::new()); }
    f(guard.as_mut().unwrap())
}

fn circuit_id(args: &[EvalValue]) -> Result<u64, String> {
    match args.first() {
        Some(EvalValue::Int(n)) if *n > 0 => Ok(*n as u64),
        _ => Err("quantum: expected a circuit id (int)".into()),
    }
}

fn qubit_arg(args: &[EvalValue], pos: usize, n: usize, fname: &str) -> Result<usize, String> {
    match args.get(pos) {
        Some(EvalValue::Int(q)) if *q >= 0 && (*q as usize) < n => Ok(*q as usize),
        Some(EvalValue::Int(q)) => Err(format!("quantum.{}: qubit {} out of range (the circuit has {})", fname, q, n)),
        _ => Err(format!("quantum.{}: expected a qubit index (int)", fname)),
    }
}

fn theta_arg(args: &[EvalValue], pos: usize, fname: &str) -> Result<f64, String> {
    match args.get(pos) {
        Some(EvalValue::Float(f)) => Ok(*f),
        Some(EvalValue::Int(n))   => Ok(*n as f64),
        _ => Err(format!("quantum.{}: expected a theta angle (number, radians)", fname)),
    }
}

// Aplica una puerta 2×2 al qubit `q`, opcionalmente condicionada a que TODOS
// los bits de `controls` estén en 1. O(2^n), y en paralelo (rayon) a partir
// de 2^16 amplitudes: cada k del subespacio comprimido mapea a un par (i, j)
// disjunto, así que las escrituras nunca chocan.
const PAR_THRESHOLD: usize = 1 << 16;

fn apply_1q(circ: &mut Circuit, q: usize, g: &[[C; 2]; 2], controls: &[usize]) {
    let bit = 1usize << (circ.n - 1 - q);
    let cmask: usize = controls.iter().map(|&c| 1usize << (circ.n - 1 - c)).sum();
    let size = circ.state.len();
    let low = bit - 1; // bits por debajo del qubit objetivo
    let g = *g;
    let pair_op = move |a: C, b: C| -> (C, C) {
        (c_add(c_mul(g[0][0], a), c_mul(g[0][1], b)),
         c_add(c_mul(g[1][0], a), c_mul(g[1][1], b)))
    };
    if size >= PAR_THRESHOLD {
        use rayon::prelude::*;
        struct Ptr(*mut C);
        unsafe impl Send for Ptr {}
        unsafe impl Sync for Ptr {}
        let ptr = Ptr(circ.state.as_mut_ptr());
        let p = &ptr;
        (0..size >> 1).into_par_iter().for_each(|k| {
            // inserta un 0 en la posición del bit objetivo → i con bit=0
            let i = ((k & !low) << 1) | (k & low);
            if (i & cmask) != cmask { return; }
            let j = i | bit;
            // SAFETY: (i, j) es único por k; ningún otro k toca estos índices
            unsafe {
                let (na, nb) = pair_op(*p.0.add(i), *p.0.add(j));
                *p.0.add(i) = na;
                *p.0.add(j) = nb;
            }
        });
    } else {
        for k in 0..size >> 1 {
            let i = ((k & !low) << 1) | (k & low);
            if (i & cmask) != cmask { continue; }
            let j = i | bit;
            let (na, nb) = pair_op(circ.state[i], circ.state[j]);
            circ.state[i] = na;
            circ.state[j] = nb;
        }
    }
}

fn gate2(m: [[C; 2]; 2]) -> [[C; 2]; 2] { m }

fn named_gate(name: &str, theta: f64) -> Option<[[C; 2]; 2]> {
    let s = 1.0 / 2.0f64.sqrt();
    let (ht2c, ht2s) = ((theta / 2.0).cos(), (theta / 2.0).sin());
    Some(match name {
        "h"     => gate2([[(s,0.0),(s,0.0)], [(s,0.0),(-s,0.0)]]),
        "x"     => gate2([[(0.0,0.0),(1.0,0.0)], [(1.0,0.0),(0.0,0.0)]]),
        "y"     => gate2([[(0.0,0.0),(0.0,-1.0)], [(0.0,1.0),(0.0,0.0)]]),
        "z"     => gate2([[(1.0,0.0),(0.0,0.0)], [(0.0,0.0),(-1.0,0.0)]]),
        "s"     => gate2([[(1.0,0.0),(0.0,0.0)], [(0.0,0.0),(0.0,1.0)]]),
        "t"     => gate2([[(1.0,0.0),(0.0,0.0)], [(0.0,0.0),((std::f64::consts::FRAC_PI_4).cos(),(std::f64::consts::FRAC_PI_4).sin())]]),
        "rx"    => gate2([[(ht2c,0.0),(0.0,-ht2s)], [(0.0,-ht2s),(ht2c,0.0)]]),
        "ry"    => gate2([[(ht2c,0.0),(-ht2s,0.0)], [(ht2s,0.0),(ht2c,0.0)]]),
        "rz"    => gate2([[(ht2c,-ht2s),(0.0,0.0)], [(0.0,0.0),(ht2c,ht2s)]]),
        "phase" => gate2([[(1.0,0.0),(0.0,0.0)], [(0.0,0.0),(theta.cos(),theta.sin())]]),
        _ => return None,
    })
}

// Convierte EvalValue [[..],[..]] en puerta 2×2 (para ugate/cugate)
fn eval_to_gate2(v: &EvalValue) -> Result<[[C; 2]; 2], String> {
    let g = eval_to_gate(v)?;
    if g.len() != 2 || g[0].len() != 2 || g[1].len() != 2 {
        return Err("quantum.ugate: the gate must be 2×2 ([[a,b],[c,d]] with [re,im] or numbers)".into());
    }
    // Unitariedad: G·G† = I (si no, el estado deja de ser físico)
    let (a, b, c, d) = (g[0][0], g[0][1], g[1][0], g[1][1]);
    let row1 = c_abs2(a) + c_abs2(b);
    let row2 = c_abs2(c) + c_abs2(d);
    let cross = c_add(c_mul(a, c_conj(c)), c_mul(b, c_conj(d)));
    if (row1 - 1.0).abs() > 1e-9 || (row2 - 1.0).abs() > 1e-9 || c_abs2(cross) > 1e-18 {
        return Err("quantum.ugate: the matrix is not unitary (G·G† ≠ I)".into());
    }
    Ok([[g[0][0], g[0][1]], [g[1][0], g[1][1]]])
}

// Ejecuta una puerta con nombre sobre un circuito: (id, q) o (id, q, theta)
fn circuit_gate(name: &str, args: Vec<EvalValue>, parametric: bool) -> Result<EvalValue, String> {
    let id = circuit_id(&args)?;
    with_circuits(|cs| {
        let circ = cs.get_mut(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
        let q = qubit_arg(&args, 1, circ.n, name)?;
        let theta = if parametric { theta_arg(&args, 2, name)? } else { 0.0 };
        let g = named_gate(name, theta).ok_or(format!("quantum.{}: puerta desconocida", name))?;
        apply_1q(circ, q, &g, &[]);
        Ok(EvalValue::Int(id as i64))
    })
}

// Puerta con nombre controlada: (id, control, target) o (id, control, target, theta)
fn circuit_cgate(name: &str, args: Vec<EvalValue>, parametric: bool) -> Result<EvalValue, String> {
    let id = circuit_id(&args)?;
    with_circuits(|cs| {
        let circ = cs.get_mut(&id).ok_or(format!("quantum: circuit {} does not exist", id))?;
        let ctrl = qubit_arg(&args, 1, circ.n, name)?;
        let tgt  = qubit_arg(&args, 2, circ.n, name)?;
        if ctrl == tgt { return Err(format!("quantum.{}: control and target must be different", name)); }
        let theta = if parametric { theta_arg(&args, 3, name)? } else { 0.0 };
        let base = match name { "cnot" => "x", "cz" => "z", "cphase" => "phase", other => other };
        let g = named_gate(base, theta).ok_or(format!("quantum.{}: puerta desconocida", name))?;
        apply_1q(circ, tgt, &g, &[ctrl]);
        Ok(EvalValue::Int(id as i64))
    })
}

//     Puertas estándar

fn hadamard() -> Gate {
    let s = 1.0 / 2.0f64.sqrt();
    vec![vec![(s,0.0),(s,0.0)], vec![(s,0.0),(-s,0.0)]]
}
fn pauli_x() -> Gate { vec![vec![(0.0,0.0),(1.0,0.0)], vec![(1.0,0.0),(0.0,0.0)]] }
fn pauli_y() -> Gate { vec![vec![(0.0,0.0),(0.0,-1.0)], vec![(0.0,1.0),(0.0,0.0)]] }
fn pauli_z() -> Gate { vec![vec![(1.0,0.0),(0.0,0.0)], vec![(0.0,0.0),(-1.0,0.0)]] }
fn phase_s() -> Gate { vec![vec![(1.0,0.0),(0.0,0.0)], vec![(0.0,0.0),(0.0,1.0)]] }
fn cnot()    -> Gate {
    vec![
        vec![(1.0,0.0),(0.0,0.0),(0.0,0.0),(0.0,0.0)],
        vec![(0.0,0.0),(1.0,0.0),(0.0,0.0),(0.0,0.0)],
        vec![(0.0,0.0),(0.0,0.0),(0.0,0.0),(1.0,0.0)],
        vec![(0.0,0.0),(0.0,0.0),(1.0,0.0),(0.0,0.0)],
    ]
}

//     Operaciones matemáticas                                                   

fn c_add(a: C, b: C) -> C { (a.0 + b.0, a.1 + b.1) }
fn c_mul(a: C, b: C) -> C { (a.0*b.0 - a.1*b.1, a.0*b.1 + a.1*b.0) }
fn c_conj(a: C) -> C { (a.0, -a.1) }
fn c_abs2(a: C) -> f64 { a.0*a.0 + a.1*a.1 }

fn normalize(mut state: State) -> State {
    let norm = state.iter().map(|&a| c_abs2(a)).sum::<f64>().sqrt();
    if norm < 1e-15 { return state; }
    for a in &mut state { *a = (a.0 / norm, a.1 / norm); }
    state
}

fn tensor_product(a: &State, b: &State) -> State {
    a.iter().flat_map(|&ai| b.iter().map(move |&bi| c_mul(ai, bi))).collect()
}

fn apply_gate(state: &State, gate: &Gate) -> Result<State, String> {
    let n = gate.len();
    if n != state.len() {
        return Err(format!("quantum.apply: gate {}x{} no coincide con estado de {} amplitudes", n, n, state.len()));
    }
    let result: State = (0..n).map(|i| {
        gate[i].iter().zip(state).map(|(&g, &s)| c_mul(g, s)).fold((0.0, 0.0), c_add)
    }).collect();
    Ok(result)
}

fn measure(state: &State, shots: usize) -> HashMap<String, usize> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let probs: Vec<f64> = state.iter().map(|&a| c_abs2(a)).collect();
    let n_qubits = (state.len() as f64).log2() as usize;
    let mut cumulative = Vec::with_capacity(probs.len());
    let mut acc = 0.0;
    for p in &probs { acc += p; cumulative.push(acc); }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for _ in 0..shots {
        let r: f64 = rng.gen();
        let idx = cumulative.iter().position(|&c| r <= c).unwrap_or(state.len() - 1);
        let key = format!("{:0>width$b}", idx, width = n_qubits);
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

//     Conversiones EvalValue ↔ State                                           

fn state_to_eval(state: &State) -> EvalValue {
    EvalValue::List(state.iter().map(|(re, im)| {
        EvalValue::List(vec![EvalValue::Float(*re), EvalValue::Float(*im)])
    }).collect())
}

fn eval_to_state(v: &EvalValue) -> Result<State, String> {
    match v {
        EvalValue::List(amps) => {
            let mut state = Vec::new();
            for amp in amps {
                match amp {
                    EvalValue::List(pair) if pair.len() >= 2 => {
                        state.push((to_f64v(&pair[0])?, to_f64v(&pair[1])?));
                    }
                    EvalValue::Float(f) => state.push((*f, 0.0)),
                    EvalValue::Int(n)   => state.push((*n as f64, 0.0)),
                    _ => return Err("quantum: an amplitude must be [re, im]".into()),
                }
            }
            if state.is_empty() { return Err("quantum: empty state".into()); }
            Ok(state)
        }
        _ => Err(format!("quantum: expected a list of amplitudes, got {}", v.type_name())),
    }
}

fn eval_to_gate(v: &EvalValue) -> Result<Gate, String> {
    match v {
        EvalValue::List(rows) => {
            rows.iter().map(|row| {
                match row {
                    EvalValue::List(cols) => {
                        cols.iter().map(|col| {
                            match col {
                                EvalValue::List(pair) if pair.len() >= 2 => {
                                    Ok((to_f64v(&pair[0])?, to_f64v(&pair[1])?))
                                }
                                EvalValue::Float(f) => Ok((*f, 0.0)),
                                EvalValue::Int(n)   => Ok((*n as f64, 0.0)),
                                _ => Err("quantum: a gate element must be [re, im]".into()),
                            }
                        }).collect()
                    }
                    _ => Err("quantum: the gate must be a list of lists".into()),
                }
            }).collect()
        }
        _ => Err("quantum: gate must be a list of lists".into()),
    }
}

fn gate_to_eval(gate: &Gate) -> EvalValue {
    EvalValue::List(gate.iter().map(|row| {
        EvalValue::List(row.iter().map(|(re, im)| {
            EvalValue::List(vec![EvalValue::Float(*re), EvalValue::Float(*im)])
        }).collect())
    }).collect())
}

fn to_f64v(v: &EvalValue) -> Result<f64, String> {
    match v {
        EvalValue::Float(f) => Ok(*f),
        EvalValue::Int(n)   => Ok(*n as f64),
        other => Err(format!("quantum: expected an f64, got {}", other.type_name())),
    }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("quantum: expected an integer, got {}", other.type_name())),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
