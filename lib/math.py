"""
Matemáticas cósmicas para Orion.
Incluye aritmética, trigonometría, álgebra y utilidades futuristas.
"""

import math
import random

# --- Constantes universales ---
PI   = math.pi
E    = math.e
TAU  = math.tau
PHI  = (1 + 5**0.5) / 2  # número áureo
INF  = float("inf")
NAN  = float("nan")

# --- Aritmética ---
def add(a, b): return a + b
def sub(a, b): return a - b
def mul(a, b): return a * b
def div(a, b): return a / b if b != 0 else INF
def mod(a, b): return a % b
def pow(a, b): return a ** b
def sqrt(x): return math.sqrt(x)

# --- Trigonometría ---
def sin(x): return math.sin(x)
def cos(x): return math.cos(x)
def tan(x): return math.tan(x)

def asin(x): return math.asin(x)
def acos(x): return math.acos(x)
def atan(x): return math.atan(x)
def atan2(y, x): return math.atan2(y, x)

# --- Hiperbólicas ---
def sinh(x): return math.sinh(x)
def cosh(x): return math.cosh(x)
def tanh(x): return math.tanh(x)

# --- Log/exp ---
def log(x, base=math.e): return math.log(x, base)
def exp(x): return math.exp(x)
def log10(x): return math.log10(x)
def log2(x): return math.log2(x)

# --- Factoriales y combinatoria ---
def factorial(n): return math.factorial(n)
def comb(n, k): return math.comb(n, k)
def perm(n, k): return math.perm(n, k)

# --- Distancias y geometría ---
def dist(a, b): return math.dist(a, b)
def hypot(*coords): return math.hypot(*coords)
def degrees(rad): return math.degrees(rad)
def radians(deg): return math.radians(deg)

# --- Random cósmico ---
def rand(): return random.random()
def randint(a, b): return random.randint(a, b)
def randrange(a, b, step=1): return random.randrange(a, b, step)
def choice(seq): return random.choice(seq)
def shuffle(seq): random.shuffle(seq); return seq
def sample(seq, k): return random.sample(seq, k)

# --- Futurista: utilidades ---
def clamp(x, lo, hi):
    """Limita x dentro de [lo, hi]."""
    return max(lo, min(x, hi))

def lerp(a, b, t):
    """Interpolación lineal (mezcla cósmica)."""
    return a + (b - a) * t

def norm(x, lo, hi):
    """Normaliza x dentro del rango [0,1]."""
    return (x - lo) / (hi - lo) if hi != lo else 0

def map_range(x, in_min, in_max, out_min, out_max):
    """Mapea un valor de un rango a otro (warp)."""
    return out_min + (float(x - in_min) / (in_max - in_min)) * (out_max - out_min)

# --- Futurista: potencia avanzada ---
def futuristic_power(base, exp):
    """
    Potencia futurista: combina pow con warp cósmico.
    Si exp es par => pow normal,
    Si exp es impar => pow y se suma PHI.
    """
    result = base ** exp
    if exp % 2 == 1:
        result += PHI
    return result
