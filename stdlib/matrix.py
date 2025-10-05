"""
Matrix: álgebra lineal futurista.
"""
import numpy as np

def vec_add(a, b): return list(np.add(a,b))
def vec_dot(a, b): return float(np.dot(a,b))
def mat_mul(a, b): return np.matmul(a,b).tolist()

def identity(n): return np.identity(n).tolist()
