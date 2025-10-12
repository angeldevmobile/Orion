# stdlib/quantum.py
"""
Orion Quantum+ — simulador cuántico extendido para Orion.
- Sin dependencias obligatorias (opcional numpy para acelerar).
- Qubits como vectores complejos normalizados.
- Puertas básicas (H, X, Y, Z, S, T, CNOT), tensores, medidas con shots.
- Control gate general para n-qubits.
- Construcción de circuitos: expandir puertas a posiciones concretas.
- Modelos de ruido por qubit (amplitude damping por-qubit, depolarizing por-qubit).
- Función 'quantum' como entrada unificada y orion_export compatible.
"""

import math
import random
import cmath
from typing import List, Tuple, Callable, Dict, Any, Optional

# optional acceleration
try:
    import numpy as _np  # type: ignore
except Exception:
    _np = None

Complex = complex
ComplexVec = List[Complex]
Matrix = List[List[Complex]]

# -------------------------
# Helpers
# -------------------------
def _to_np(v):
    """Convierte listas (1D o 2D) a ndarray complejo si NumPy está disponible."""
    if not _np:
        return None
    if isinstance(v, list) and all(isinstance(x, list) for x in v):
        # matriz 2D
        return _np.array(v, dtype=complex)
    return _np.asarray(v, dtype=complex)
def _normalize(state: ComplexVec) -> ComplexVec:
    arr = _to_np(state)
    if arr is not None:
        norm = _np.linalg.norm(arr)
        if norm == 0:
            return [1+0j] + [0j]*(len(state)-1)
        return (arr / norm).tolist()
    norm = math.sqrt(sum(abs(x)**2 for x in state))
    if norm == 0:
        return [1+0j] + [0j]*(len(state)-1)
    return [x / norm for x in state]

def _is_power_of_two(n: int) -> bool:
    return n > 0 and (n & (n - 1)) == 0

def _tensor(a: ComplexVec, b: ComplexVec) -> ComplexVec:
    a_np, b_np = _to_np(a), _to_np(b)
    if a_np is not None and b_np is not None:
        return (_np.kron(a_np, b_np)).tolist()
    return [x * y for x in a for y in b]

def _apply_matrix(state: ComplexVec, mat: Matrix) -> ComplexVec:
    s_np, m_np = _to_np(state), _to_np(mat)
    if s_np is not None and m_np is not None:
        return (_np.dot(m_np, s_np)).tolist()
    n = len(mat)
    return [sum(mat[i][j]*state[j] for j in range(n)) for i in range(n)]

# -------------------------
# Basic states & gates
# -------------------------
def qubit(alpha: complex = 1+0j, beta: complex = 0+0j) -> ComplexVec:
    """Construye un qubit (alpha|0> + beta|1>) normalizado."""
    return _normalize([alpha, beta])

def zero() -> ComplexVec:
    return qubit(1+0j, 0+0j)

def one() -> ComplexVec:
    return qubit(0+0j, 1+0j)

def rand_qubit() -> ComplexVec:
    a = random.random()
    theta = math.acos(1 - 2 * a)
    phi = random.random() * 2 * math.pi
    alpha = math.cos(theta / 2)
    beta = cmath.exp(1j * phi) * math.sin(theta / 2)
    return qubit(alpha, beta)

# Single-qubit gates (2x2)
I: Matrix = [[1+0j, 0+0j], [0+0j, 1+0j]]
X: Matrix = [[0+0j, 1+0j], [1+0j, 0+0j]]
Y: Matrix = [[0+0j, -1j], [1j, 0+0j]]
Z: Matrix = [[1+0j, 0+0j], [0+0j, -1+0j]]
H: Matrix = [[1/math.sqrt(2)+0j, 1/math.sqrt(2)+0j],
            [1/math.sqrt(2)+0j, -1/math.sqrt(2)+0j]]
S: Matrix = [[1+0j, 0+0j], [0+0j, 1j]]
T: Matrix = [[1+0j, 0+0j], [0+0j, cmath.exp(1j * math.pi / 4)]]

# Two-qubit common gate
CNOT: Matrix = [
    [1+0j,0+0j,0+0j,0+0j],
    [0+0j,1+0j,0+0j,0+0j],
    [0+0j,0+0j,0+0j,1+0j],
    [0+0j,0+0j,1+0j,0+0j]
]

# -------------------------
# Multi-qubit utilities
# -------------------------
def tensor(*states: ComplexVec) -> ComplexVec:
    """Tensor product of many states."""
    if not states:
        return []
    res = states[0]
    for s in states[1:]:
        res = _tensor(res, s)
    return _normalize(res)

def gates_tensor(*gates: Matrix) -> Matrix:
    """Kronecker product of gates (matrices)."""
    if not gates:
        return [[1+0j]]
    if _np:
        res = _np.asarray(gates[0], dtype=complex)
        for g in gates[1:]:
            res = _np.kron(res, _np.asarray(g, dtype=complex))
        return res.tolist()
    # naive kron
    res = gates[0]
    for g in gates[1:]:
        rrows = len(res)
        rcols = len(res[0])
        grows = len(g)
        gcols = len(g[0])
        new = [[0+0j for _ in range(rcols * gcols)] for _ in range(rrows * grows)]
        for i in range(rrows):
            for j in range(rcols):
                for k in range(grows):
                    for l in range(gcols):
                        new[i * grows + k][j * gcols + l] = res[i][j] * g[k][l]
        res = new
    return res

def apply_gate(state: ComplexVec, gate: Matrix) -> ComplexVec:
    """Aplica un gate (matriz) sobre el estado (vector)."""
    return _normalize(_apply_matrix(state, gate))

# -------------------------
# Build full-matrix for single-qubit gate at target index
# (convention: qubit index 0 = most-significant bit / leftmost)
# -------------------------
def expand_single_qubit_gate(gate: Matrix, target_index: int, n_qubits: int) -> Matrix:
    """Construye la matriz completa para aplicar `gate` en `target_index` (0..n-1)."""
    gates = []
    for i in range(n_qubits):
        if i == target_index:
            gates.append(gate)
        else:
            gates.append(I)
    return gates_tensor(*gates)

# -------------------------
# Controlled gate general
# -------------------------
def control_gate(base_gate: Matrix, control_index: int, target_index: int, n_qubits: int) -> Matrix:
    """
    Crea una puerta controlada general (aplica base_gate al target cuando control==1).
    Construcción naive por columnas: para cada basis state |j>:
      - si control bit en j == '0' => M[j,j] = 1 (identidad en esa columna)
      - si control bit == '1' => aplica base_gate sobre el target subspace:
          para t_out in {0,1}:
            i = j with target bit replaced by t_out
            M[i, j] = base_gate[t_out][t_in]
    (M dimension = 2^n x 2^n)
    """
    size = 2 ** n_qubits
    # initialize zero matrix
    M = [[0+0j for _ in range(size)] for _ in range(size)]
    for j in range(size):
        bits = format(j, "b").zfill(n_qubits)
        control_bit = bits[control_index]
        if control_bit == "0":
            M[j][j] = 1+0j
        else:
            # apply base_gate on target bit
            t_in = int(bits[target_index])
            for t_out in (0, 1):
                # build i index: same bits but target bit = t_out
                new_bits = list(bits)
                new_bits[target_index] = str(t_out)
                i = int("".join(new_bits), 2)
                M[i][j] = base_gate[t_out][t_in]
    return M

# -------------------------
# Circuit helpers
# -------------------------
def apply_single_qubit(state: ComplexVec, gate: Matrix, target: int, n_qubits: int) -> ComplexVec:
    """Expande gate y lo aplica al estado."""
    mat = expand_single_qubit_gate(gate, target, n_qubits)
    return apply_gate(state, mat)

def apply_controlled_gate(state: ComplexVec, base_gate: Matrix, control: int, target: int, n_qubits: int) -> ComplexVec:
    mat = control_gate(base_gate, control, target, n_qubits)
    return apply_gate(state, mat)

def state_from_bits(bitstring: str) -> ComplexVec:
    """Genera el estado computacional |bitstring> (e.g. '01' => |01>)."""
    n = len(bitstring)
    size = 2 ** n
    vec = [0+0j] * size
    idx = int(bitstring, 2) if bitstring else 0
    vec[idx] = 1+0j
    return vec

def apply_circuit(state: ComplexVec, n_qubits: int, ops: List[Tuple[str, Any]]) -> ComplexVec:
    """
    Ejecuta una lista de operaciones sobre el estado.
    ops: lista de (op, params)
     - op == "single": params = (gate_matrix, target_index)
     - op == "controlled": params = (base_gate, control_index, target_index)
     - op == "matrix": params = (full_matrix,)
    Retorna el estado final (normalizado).
    """
    s = state
    for op, params in ops:
        if op == "single":
            gate, target = params
            s = apply_single_qubit(s, gate, target, n_qubits)
        elif op == "controlled":
            base, control, target = params
            s = apply_controlled_gate(s, base, control, target, n_qubits)
        elif op == "matrix":
            mat, = params
            s = apply_gate(s, mat)
        else:
            raise ValueError(f"Unknown op {op}")
    return _normalize(s)

# -------------------------
# Entanglement & operations
# -------------------------
def bell_pair() -> ComplexVec:
    """Genera un par de Bell: (|00> + |11>)/sqrt(2)"""
    s = _normalize([1+0j, 0+0j, 0+0j, 1+0j])
    return s

def entangle(a: ComplexVec, b: ComplexVec) -> ComplexVec:
    """Entrelaza dos qubits (tensor) — no hace CNOT automático, solo tensor y normaliza."""
    return _normalize(_tensor(a, b))

# -------------------------
# Noise models (per-qubit)
# -------------------------
def amplitude_damping_per_qubit(state: ComplexVec, gammas: List[float]) -> ComplexVec:
    """
    Aplica amplitude damping por-qubit aproximado.
    gammas: lista de gamma por cada qubit (len = n_qubits)
    Aproximación: cada amplitud se atenúa por prod((1-gamma)^{bit_value})
    """
    n = len(state)
    if not _is_power_of_two(n):
        raise ValueError("State vector length must be power of two")
    n_qubits = int(math.log2(n))
    new = [0+0j] * n
    for idx, amp in enumerate(state):
        bits = format(idx, "b").zfill(n_qubits)
        factor = 1.0
        for q, ch in enumerate(bits):
            if ch == "1":
                factor *= (1 - gammas[q])
        new[idx] = amp * factor
    return _normalize(new)

def depolarizing_per_qubit(state: ComplexVec, ps: List[float]) -> ComplexVec:
    """
    Depolarizing per-qubit approximation mixing with maximally mixed (naive).
    ps: prob per qubit; overall mixed_prob computed as average for simplicity.
    """
    n = len(state)
    if not _is_power_of_two(n):
        raise ValueError("State vector length must be power of two")
    mixed_prob = sum(ps) / len(ps) if ps else 0.0
    pure = [a * (1 - mixed_prob) for a in state]
    mix = [cmath.sqrt(mixed_prob / n)] * n
    return _normalize([pure[i] + mix[i] for i in range(n)])

# -------------------------
# Measure & utilities
# -------------------------
def measure(state: ComplexVec, shots: int = 1024, seed: Optional[int] = None) -> Dict[str, int]:
    """
    Mide un estado multi-qubit y retorna conteos.
    seed: opcional para reproducibilidad.
    """
    if seed is not None:
        random.seed(seed)
    n = len(state)
    if not _is_power_of_two(n):
        raise ValueError("State vector length must be power of two")
    probs = [abs(a) ** 2 for a in state]
    counts: Dict[str, int] = {}
    cumulative = []
    s = 0.0
    for p in probs:
        s += p
        cumulative.append(s)
    for _ in range(shots):
        r = random.random()
        # binary search
        lo, hi = 0, n-1
        while lo < hi:
            mid = (lo + hi) // 2
            if r <= cumulative[mid]:
                hi = mid
            else:
                lo = mid + 1
        idx = lo
        key = format(idx, "b").zfill(int(math.log2(n)))
        counts[key] = counts.get(key, 0) + 1
    return counts

def measure_probabilities(state: ComplexVec) -> Dict[str, float]:
    """Retorna probabilidades teóricas por base (sin sampling)."""
    n = len(state)
    if not _is_power_of_two(n):
        raise ValueError("State vector length must be power of two")
    probs = [abs(a) ** 2 for a in state]
    n_qubits = int(math.log2(n))
    return {format(i, "b").zfill(n_qubits): p for i, p in enumerate(probs)}

def fidelity(s1: ComplexVec, s2: ComplexVec) -> float:
    a, b = _to_np(s1), _to_np(s2)
    if a is not None and b is not None:
        return float(abs(_np.vdot(a, b)) ** 2)
    inner = sum((x.conjugate() * y) for x, y in zip(s1, s2))
    return abs(inner) ** 2

def bloch_vector(single_qubit_state: ComplexVec) -> Tuple[float, float, float]:
    """Retorna (x,y,z) del vector de Bloch para un qubit."""
    a, b = single_qubit_state[0], single_qubit_state[1]
    rho00 = abs(a) ** 2
    rho11 = abs(b) ** 2
    rho01 = a * b.conjugate()
    x = 2 * rho01.real
    y = 2 * rho01.imag
    z = rho00 - rho11
    return (x, y, z)

# -------------------------
# High-level entry and aliases
# -------------------------
def quantum(action: str = "qubit", *args, **kwargs):
    """
    Entrada universal.
    Examples:
      quantum("qubit", 1, 0)
      quantum("rand")
      quantum("bell")
      quantum("measure", state, shots=1024)
      quantum("apply_circuit", state, n_qubits, ops)
    """
    if action == "qubit":
        return qubit(*args)
    if action == "rand":
        return rand_qubit()
    if action == "bell":
        return bell_pair()
    if action == "tensor":
        return tensor(*args)
    if action == "entangle":
        return entangle(*args)
    if action == "measure":
        state = args[0]
        return measure(state, kwargs.get("shots", 1024), kwargs.get("seed"))
    if action == "measure_probs":
        return measure_probabilities(args[0])
    if action == "fidelity":
        return fidelity(args[0], args[1])
    if action == "state_from_bits":
        return state_from_bits(args[0])
    if action == "apply_circuit":
        return apply_circuit(args[0], args[1], args[2])
    if action == "expand_gate":
        return expand_single_qubit_gate(*args)  # (gate, target, n_qubits)
    return None

ALIASES = {
    "qubit": qubit,
    "zero": zero,
    "one": one,
    "rand": rand_qubit,
    "bell": bell_pair,
    "tensor": tensor,
    "apply": apply_gate,
    "measure": measure,
    "measure_probs": measure_probabilities,
    "fidelity": fidelity,
    "bloch": bloch_vector,
    "amplitude_damping_per_qubit": amplitude_damping_per_qubit,
    "depolarize_per_qubit": depolarizing_per_qubit,
    "expand_gate": expand_single_qubit_gate,
    "control_gate": control_gate,
    "apply_circuit": apply_circuit,
    "state_from_bits": state_from_bits,
}

def orion_export():
    exports = {"quantum": quantum}
    exports.update(ALIASES)
    return exports
