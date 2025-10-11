# stdlib/matrix.py
"""
Orion Matrix — Futuristic math & tensor engine
──────────────────────────────────────────────
Inspirado en el diseño de lenguajes cuánticos y ML,
pero pensado para la simplicidad absoluta.

Características:
- Autoadaptativo (detecta escalar/matriz/tensor)
- Operaciones matemáticas vectorizadas (sin numpy)
- Rotaciones 2D/3D, inversa, determinante, identidad
- Matrices “inteligentes”: pueden inferir tipo y simplificar
- “Smart Operators” que aprenden del tamaño de la matriz
"""

import math
import random
from typing import List, Union, Tuple

Number = Union[int, float]
Matrix = List[List[Number]]


# ------------------------------------------------------------
# 🔹 Utilidades internas
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
# 🔸 Operaciones básicas
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
    """Multiplicación de matrices, con soporte escalar y broadcasting"""
    if isinstance(A, (int, float)) or isinstance(B, (int, float)):
        if isinstance(A, (int, float)):
            return [[A * x for x in row] for row in B]
        else:
            return [[B * x for x in row] for row in A]

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
    """Transpuesta"""
    r, c = _shape(A)
    return [[A[j][i] for j in range(r)] for i in range(c)]


# ------------------------------------------------------------
# 🔹 Funciones avanzadas
# ------------------------------------------------------------

def det(A):
    """Determinante (recursivo, O(n!))"""
    n, m = _shape(A)
    if n != m:
        raise ValueError("Matrix must be square")
    if n == 1:
        return A[0][0]
    if n == 2:
        return A[0][0]*A[1][1] - A[0][1]*A[1][0]
    total = 0
    for c in range(n):
        minor = [row[:c] + row[c+1:] for row in A[1:]]
        total += ((-1)**c) * A[0][c] * det(minor)
    return total

def inverse(A):
    """Inversa (por Gauss-Jordan simplificado)"""
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
    """Traza"""
    n, m = _shape(A)
    return sum(A[i][i] for i in range(min(n, m)))

def rot2D(angle_deg):
    """Matriz de rotación 2D"""
    a = math.radians(angle_deg)
    return [
        [math.cos(a), -math.sin(a)],
        [math.sin(a),  math.cos(a)]
    ]

def rot3D(x_deg, y_deg, z_deg):
    """Matriz de rotación 3D (XYZ)"""
    x = math.radians(x_deg)
    y = math.radians(y_deg)
    z = math.radians(z_deg)
    Rx = [
        [1, 0, 0],
        [0, math.cos(x), -math.sin(x)],
        [0, math.sin(x),  math.cos(x)],
    ]
    Ry = [
        [math.cos(y), 0, math.sin(y)],
        [0, 1, 0],
        [-math.sin(y), 0, math.cos(y)],
    ]
    Rz = [
        [math.cos(z), -math.sin(z), 0],
        [math.sin(z),  math.cos(z), 0],
        [0, 0, 1],
    ]
    return mul(mul(Rz, Ry), Rx)


# ------------------------------------------------------------
# 🌌 Funcionalidades únicas de Orion
# ------------------------------------------------------------

def morph(A, fn):
    """Aplica una función dinámica elemento a elemento."""
    return [[fn(x) for x in row] for row in A]

def amplify(A, factor=2):
    """Duplica la energía matemática de una matriz (escala adaptativa)."""
    f = abs(factor)
    return [[(x * f if x >= 0 else -abs(x) * f) for x in row] for row in A]

def collapse(A):
    """Convierte una matriz en un escalar de energía (inspirado en física cuántica)."""
    flat = sum(sum(row) for row in A)
    return math.tanh(flat)

def neuralify(A, activation="relu"):
    """Aplica activaciones neuronales sin frameworks."""
    act = activation.lower()
    if act == "relu":
        return morph(A, lambda x: max(0, x))
    if act == "sigmoid":
        return morph(A, lambda x: 1 / (1 + math.exp(-x)))
    if act == "tanh":
        return morph(A, math.tanh)
    raise ValueError("Unknown activation")


# ------------------------------------------------------------
# ⚡ Exportación a Orion
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
}

def orion_export():
    exports = {"matrix": ALIASES}
    exports.update(ALIASES)
    return exports
