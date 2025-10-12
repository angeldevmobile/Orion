"""
Orion Matrix+ — Smart math & tensor engine
──────────────────────────────────────────
Extiende Orion Matrix con matrices inteligentes,
operaciones composables, autodetección de forma
y soporte para álgebra cuántica básica.
"""

import math
import random
from typing import List, Union, Tuple, Callable

Number = Union[int, float]
Matrix = List[List[Number]]

# ------------------------------------------------------------
# SmartMatrix: clase adaptativa
# ------------------------------------------------------------

class SmartMatrix:
    """Contenedor inteligente que se adapta según contexto."""
    def __init__(self, data):
        self.data = self._normalize(data)
        self.shape = _shape(self.data)

    def _normalize(self, data):
        if isinstance(data, SmartMatrix):
            return data.data
        if isinstance(data, (int, float)):
            return [[data]]
        if isinstance(data, list) and all(isinstance(x, (int, float)) for x in data):
            return [data]  # vector fila
        return data

    def __matmul__(self, other):  # A @ B
        return SmartMatrix(mul(self.data, SmartMatrix(other).data))

    def __add__(self, other):
        return SmartMatrix(add(self.data, SmartMatrix(other).data))

    def __sub__(self, other):
        return SmartMatrix(sub(self.data, SmartMatrix(other).data))

    def __mul__(self, factor):
        if isinstance(factor, (int, float)):
            return SmartMatrix([[x * factor for x in row] for row in self.data])
        return SmartMatrix(mul(self.data, SmartMatrix(factor).data))

    def T(self):
        return SmartMatrix(transpose(self.data))

    def det(self):
        return det(self.data)

    def inv(self):
        return SmartMatrix(inverse(self.data))

    def apply(self, fn: Callable[[float], float]):
        return SmartMatrix(morph(self.data, fn))

    def energy(self):
        return collapse(self.data)

    def __repr__(self):
        return f"<SmartMatrix shape={self.shape} data={self.data}>"

# ------------------------------------------------------------
# Utilidades internas
# ------------------------------------------------------------

def _is_matrix(x):
    return isinstance(x, list) and x and isinstance(x[0], list)

def _shape(m: Matrix) -> Tuple[int, int]:
    if not _is_matrix(m):
        return (1, 1)
    return (len(m), len(m[0]))

def _zeros(rows, cols):
    return [[0.0 for _ in range(cols)] for _ in range(rows)]

def _identity(n):
    I = _zeros(n, n)
    for i in range(n):
        I[i][i] = 1.0
    return I

# ------------------------------------------------------------
# Operaciones básicas
# ------------------------------------------------------------

def add(A, B):
    """Suma matrices o escalares"""
    if not _is_matrix(A):
        A = [[A]]
    if not _is_matrix(B):
        B = [[B]]
    r, c = _shape(A)
    return [[A[i][j] + B[i][j] for j in range(c)] for i in range(r)]

def sub(A, B):
    if not _is_matrix(A):
        A = [[A]]
    if not _is_matrix(B):
        B = [[B]]
    r, c = _shape(A)
    return [[A[i][j] - B[i][j] for j in range(c)] for i in range(r)]

def mul(A, B):
    """Multiplicación con soporte escalar y matrices."""
    if isinstance(A, (int, float)) or isinstance(B, (int, float)):
        scalar = A if isinstance(A, (int, float)) else B
        mat = B if isinstance(A, (int, float)) else A
        return [[scalar * x for x in row] for row in mat]

    r1, c1 = _shape(A)
    r2, c2 = _shape(B)
    if c1 != r2:
        raise ValueError("Matrix dimensions mismatch")
    result = _zeros(r1, c2)
    for i in range(r1):
        for j in range(c2):
            result[i][j] = sum(A[i][k] * B[k][j] for k in range(c1))
    return result

def transpose(A):
    r, c = _shape(A)
    return [[A[j][i] for j in range(r)] for i in range(c)]

# ------------------------------------------------------------
# Funciones avanzadas
# ------------------------------------------------------------

def det(A):
    n, m = _shape(A)
    if n != m:
        raise ValueError("Matrix must be square")
    if n == 1: return A[0][0]
    if n == 2: return A[0][0]*A[1][1] - A[0][1]*A[1][0]
    total = 0
    for c in range(n):
        minor = [row[:c] + row[c+1:] for row in A[1:]]
        total += ((-1)**c) * A[0][c] * det(minor)
    return total

def inverse(A):
    n, m = _shape(A)
    if n != m:
        raise ValueError("Matrix must be square")
    I = _identity(n)
    M = [A[i] + I[i] for i in range(n)]
    for i in range(n):
        pivot = M[i][i]
        if pivot == 0:
            for j in range(i+1, n):
                if M[j][i] != 0:
                    M[i], M[j] = M[j], M[i]
                    pivot = M[i][i]
                    break
        if pivot == 0:
            raise ValueError("Singular matrix")
        M[i] = [x / pivot for x in M[i]]
        for j in range(n):
            if j == i:
                continue
            factor = M[j][i]
            M[j] = [M[j][k] - factor*M[i][k] for k in range(len(M[i]))]
    return [row[n:] for row in M]

def trace(A):
    n, m = _shape(A)
    return sum(A[i][i] for i in range(min(n, m)))

def rot2D(angle_deg):
    a = math.radians(angle_deg)
    return [
        [math.cos(a), -math.sin(a)],
        [math.sin(a),  math.cos(a)]
    ]

def rot3D(x_deg, y_deg, z_deg):
    x, y, z = map(math.radians, (x_deg, y_deg, z_deg))
    Rx = [[1,0,0],[0,math.cos(x),-math.sin(x)],[0,math.sin(x),math.cos(x)]]
    Ry = [[math.cos(y),0,math.sin(y)],[0,1,0],[-math.sin(y),0,math.cos(y)]]
    Rz = [[math.cos(z),-math.sin(z),0],[math.sin(z),math.cos(z),0],[0,0,1]]
    return mul(mul(Rz, Ry), Rx)

# ------------------------------------------------------------
# Extensiones Orion
# ------------------------------------------------------------

def morph(A, fn):
    return [[fn(x) for x in row] for row in A]

def amplify(A, factor=2):
    f = abs(factor)
    return [[x * f for x in row] for row in A]

def collapse(A):
    flat = sum(sum(row) for row in A)
    return math.tanh(flat)

def neuralify(A, activation="relu"):
    act = activation.lower()
    if act == "relu":
        return morph(A, lambda x: max(0, x))
    if act == "sigmoid":
        return morph(A, lambda x: 1 / (1 + math.exp(-x)))
    if act == "tanh":
        return morph(A, math.tanh)
    raise ValueError("Unknown activation")

# ------------------------------------------------------------
# Punto de entrada Orion
# ------------------------------------------------------------

def matrix(action="identity", *args):
    if action == "add": return add(*args)
    if action == "sub": return sub(*args)
    if action == "mul": return mul(*args)
    if action == "transpose": return transpose(*args)
    if action == "det": return det(*args)
    if action == "inverse": return inverse(*args)
    if action == "trace": return trace(*args)
    if action == "rot2D": return rot2D(*args)
    if action == "rot3D": return rot3D(*args)
    if action == "neuralify": return neuralify(*args)
    if action == "collapse": return collapse(*args)
    return _identity(args[0] if args else 2)

# ------------------------------------------------------------
# Exportación Orion Runtime
# ------------------------------------------------------------

ALIASES = {
    "add": add,
    "sub": sub,
    "mul": mul,
    "transpose": transpose,
    "det": det,
    "inverse": inverse,
    "trace": trace,
    "rot2D": rot2D,
    "rot3D": rot3D,
    "morph": morph,
    "amplify": amplify,
    "collapse": collapse,
    "neuralify": neuralify,
    "SmartMatrix": SmartMatrix,
}

def orion_export():
    exports = {"matrix": matrix}
    exports.update(ALIASES)
    return exports
