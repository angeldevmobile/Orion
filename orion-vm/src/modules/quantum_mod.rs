/// Orion Quantum — simulador cuántico en Rust puro.
/// Qubits representados como Vec de pares (re, im) = amplitudes complejas.
/// EvalValue: un estado N-qubit es List([List([re, im]), ...]) con 2^N elementos.
use crate::eval_value::EvalValue;
use std::collections::HashMap;

// Número complejo (re, im)
type C = (f64, f64);

// Estado cuántico = vector de amplitudes complejas
type State = Vec<C>;

// Matriz cuántica = Vec<Vec<C>>
type Gate = Vec<Vec<C>>;

pub fn call(function: &str, args: Vec<EvalValue>) -> Result<EvalValue, String> {
    match function {
        // qubit(alpha_re?, alpha_im?, beta_re?, beta_im?) → estado |qubit>
        "qubit" | "zero" => {
            let state = normalize(vec![(1.0, 0.0), (0.0, 0.0)]);
            Ok(state_to_eval(&state))
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
            if args.len() < 2 { return Err("quantum.tensor requiere (a, b)".into()); }
            let a = eval_to_state(&args[0])?;
            let b = eval_to_state(&args[1])?;
            let result = tensor_product(&a, &b);
            Ok(state_to_eval(&normalize(result)))
        }
        // apply(state, gate) → aplica puerta al estado
        "apply" => {
            if args.len() < 2 { return Err("quantum.apply requiere (state, gate)".into()); }
            let state = eval_to_state(&args[0])?;
            let gate  = eval_to_gate(&args[1])?;
            let result = apply_gate(&state, &gate)?;
            Ok(state_to_eval(&normalize(result)))
        }
        // measure(state, shots?) → dict con conteos {"0": n, "1": m, ...}
        "measure" => {
            if args.is_empty() { return Err("quantum.measure requiere (state, shots?)".into()); }
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
            if args.is_empty() { return Err("quantum.measure_probs requiere (state)".into()); }
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
            if args.len() < 2 { return Err("quantum.fidelity requiere (s1, s2)".into()); }
            let s1 = eval_to_state(&args[0])?;
            let s2 = eval_to_state(&args[1])?;
            let inner = s1.iter().zip(&s2).map(|(a, b)| c_mul(c_conj(*a), *b)).fold((0.0, 0.0), c_add);
            let fid   = inner.0 * inner.0 + inner.1 * inner.1;
            Ok(EvalValue::Float((fid * 1e10).round() / 1e10))
        }
        // bloch(qubit_state) → [x, y, z]
        "bloch" => {
            if args.is_empty() { return Err("quantum.bloch requiere (state)".into()); }
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
            if args.is_empty() { return Err("quantum.state_from_bits requiere (bitstring)".into()); }
            let bits = to_str(&args[0]);
            let n    = bits.len();
            let size = 1 << n;
            let idx  = usize::from_str_radix(&bits, 2).map_err(|_| "quantum.state_from_bits: bits inválidos")?;
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
            if args.is_empty() { return Err("quantum.amplitudes requiere (state)".into()); }
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

        f => Err(format!("quantum.{}() no existe", f)),
    }
}

// ─── Puertas estándar ─────────────────────────────────────────────────────────

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

// ─── Operaciones matemáticas ──────────────────────────────────────────────────

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

// ─── Conversiones EvalValue ↔ State ──────────────────────────────────────────

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
                    _ => return Err("quantum: amplitud debe ser [re, im]".into()),
                }
            }
            if state.is_empty() { return Err("quantum: estado vacío".into()); }
            Ok(state)
        }
        _ => Err(format!("quantum: se esperaba lista de amplitudes, recibió {}", v.type_name())),
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
                                _ => Err("quantum: elemento de gate debe ser [re, im]".into()),
                            }
                        }).collect()
                    }
                    _ => Err("quantum: gate debe ser lista de listas".into()),
                }
            }).collect()
        }
        _ => Err("quantum: gate debe ser una lista de listas".into()),
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
        other => Err(format!("quantum: esperaba f64, recibió {}", other.type_name())),
    }
}

fn to_i64(v: &EvalValue) -> Result<i64, String> {
    match v {
        EvalValue::Int(n)   => Ok(*n),
        EvalValue::Float(f) => Ok(*f as i64),
        other => Err(format!("quantum: esperaba entero, recibió {}", other.type_name())),
    }
}

fn to_str(v: &EvalValue) -> String {
    match v { EvalValue::Str(s) => s.clone(), other => format!("{}", other) }
}
