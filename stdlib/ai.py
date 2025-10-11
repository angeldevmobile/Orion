"""
Orion AI Helpers
────────────────────────────────────────────
Módulo de inteligencia artificial ligero para Orion.
Diseñado para ser moderno, rápido y fácil de usar.

Principios:
- Usa numpy si está disponible, pero funciona sin él.
- Brinda primitivas ML-lite (regresión, similitud, clustering, embeddings, métricas).
- Tiene un modo “think” inteligente que adapta la operación según el tipo de dato.
- Pensado para desarrolladores que quieren resultados rápidos sin escribir mucho.
"""

from collections import Counter
import math
import random
from typing import List, Tuple, Optional, Callable
from functools import lru_cache

# ------------------------------------------
# Backend opcional (aceleración con numpy)
# ------------------------------------------
try:
    import numpy as _np  # type: ignore
except Exception:
    _np = None


# ------------------------------------------
# Utilidades matemáticas básicas
# ------------------------------------------
def _as_numpy(arr):
    if _np:
        return _np.asarray(arr, dtype=float)
    return None


def euclidean_distance(a: List[float], b: List[float]) -> float:
    """Distancia euclidiana entre dos vectores."""
    if _np:
        return float(_np.linalg.norm(_np.asarray(a) - _np.asarray(b)))
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))


def cosine_similarity(a: List[float], b: List[float]) -> float:
    """Similitud coseno entre dos vectores (en [-1,1])."""
    if _np:
        a_n = _np.asarray(a, dtype=float)
        b_n = _np.asarray(b, dtype=float)
        na = _np.linalg.norm(a_n)
        nb = _np.linalg.norm(b_n)
        if na == 0 or nb == 0:
            return 0.0
        return float((_np.dot(a_n, b_n) / (na * nb)))
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(y * y for y in b))
    if na == 0 or nb == 0:
        return 0.0
    return dot / (na * nb)


def normalize(vec: List[float]) -> List[float]:
    """Normaliza un vector a norma 1."""
    if _np:
        arr = _np.asarray(vec, dtype=float)
        norm = _np.linalg.norm(arr)
        if norm == 0:
            return arr.tolist()
        return (arr / norm).tolist()
    s = math.sqrt(sum(x * x for x in vec))
    if s == 0:
        return list(vec)
    return [x / s for x in vec]


def top_k_frequent(items: List, k: int = 5) -> List:
    """Retorna los k elementos más frecuentes de una lista."""
    c = Counter(items)
    return [item for item, _ in c.most_common(k)]


# ------------------------------------------
# Regresión lineal (OLS y SGD)
# ------------------------------------------
def linear_regression_fit(X: List[List[float]], y: List[float]) -> Tuple[List[float], float]:
    """Ajuste OLS sencillo: retorna (weights, bias)."""
    if not X:
        return [], 0.0
    if _np:
        Xmat = _np.asarray(X, dtype=float)
        yvec = _np.asarray(y, dtype=float)
        ones = _np.ones((Xmat.shape[0], 1))
        A = _np.hstack([Xmat, ones])
        w, *_ = _np.linalg.lstsq(A, yvec, rcond=None)
        return w[:-1].tolist(), float(w[-1])
    # fallback sin numpy
    n = len(X)
    m = len(X[0])
    XtX = [[0.0] * (m + 1) for _ in range(m + 1)]
    Xty = [0.0] * (m + 1)
    for i in range(n):
        xi = list(X[i]) + [1.0]
        yi = y[i]
        for a in range(m + 1):
            Xty[a] += xi[a] * yi
            for b in range(m + 1):
                XtX[a][b] += xi[a] * xi[b]
    M = [row[:] + [Xty[i]] for i, row in enumerate(XtX)]
    size = m + 1
    for i in range(size):
        pivot = M[i][i]
        if abs(pivot) < 1e-12:
            for r in range(i + 1, size):
                if abs(M[r][i]) > 1e-12:
                    M[i], M[r] = M[r], M[i]
                    pivot = M[i][i]
                    break
        if abs(pivot) < 1e-12:
            continue
        for j in range(i, size + 1):
            M[i][j] /= pivot
        for k in range(size):
            if k == i:
                continue
            factor = M[k][i]
            for j in range(i, size + 1):
                M[k][j] -= factor * M[i][j]
    w = [M[i][-1] for i in range(size)]
    return w[:-1], float(w[-1])


def linear_regression_predict(X: List[List[float]], weights: List[float], bias: float) -> List[float]:
    """Predice valores con pesos y bias dados."""
    if _np:
        return (_np.dot(_np.asarray(X, dtype=float), _np.asarray(weights, dtype=float)) + bias).tolist()
    return [sum(w * x for w, x in zip(weights, row)) + bias for row in X]


def sgd_linear_regression_fit(X: List[List[float]], y: List[float],
                              lr: float = 0.01, epochs: int = 100,
                              batch_size: int = 32, seed: Optional[int] = None) -> Tuple[List[float], float]:
    """Regresión lineal con SGD (mini-batch)."""
    if seed is not None:
        random.seed(seed)
    n = len(X)
    if n == 0:
        return [], 0.0
    m = len(X[0])
    weights = [random.uniform(-0.1, 0.1) for _ in range(m)]
    bias = 0.0
    for _e in range(epochs):
        idxs = list(range(n))
        random.shuffle(idxs)
        for start in range(0, n, batch_size):
            batch_idxs = idxs[start:start + batch_size]
            grad_w = [0.0] * m
            grad_b = 0.0
            for i in batch_idxs:
                xi = X[i]
                yi = y[i]
                pred = sum(w * x for w, x in zip(weights, xi)) + bias
                err = pred - yi
                for j in range(m):
                    grad_w[j] += err * xi[j]
                grad_b += err
            bs = len(batch_idxs) or 1
            for j in range(m):
                weights[j] -= lr * (grad_w[j] / bs)
            bias -= lr * (grad_b / bs)
    return weights, bias


# ------------------------------------------
# Clustering (K-means)
# ------------------------------------------
def kmeans(points: List[List[float]], k: int = 2, iterations: int = 20, seed: Optional[int] = None):
    """K-means simple: retorna (centers, labels)."""
    if not points:
        return [], []
    if seed is not None:
        random.seed(seed)
    centers = [list(p) for p in random.sample(points, min(k, len(points)))]
    labels = [0] * len(points)
    for _ in range(iterations):
        changed = False
        for i, p in enumerate(points):
            j = min(range(len(centers)), key=lambda c: euclidean_distance(p, centers[c]))
            if labels[i] != j:
                labels[i] = j
                changed = True
        for j in range(len(centers)):
            cluster = [p for i, p in enumerate(points) if labels[i] == j]
            if cluster:
                centers[j] = [sum(col) / len(cluster) for col in zip(*cluster)]
        if not changed:
            break
    return centers, labels


# ------------------------------------------
# Métricas
# ------------------------------------------
def mean_squared_error(y_true: List[float], y_pred: List[float]) -> float:
    n = len(y_true)
    return 0.0 if n == 0 else sum((a - b) ** 2 for a, b in zip(y_true, y_pred)) / n


def accuracy(y_true: List, y_pred: List) -> float:
    return 0.0 if not y_true else sum(1 for a, b in zip(y_true, y_pred) if a == b) / len(y_true)


# ------------------------------------------
# Embeddings
# ------------------------------------------
def embed_text_tokens(tokens: List[str], dim: int = 64, seed: Optional[int] = None) -> List[float]:
    """Genera embeddings deterministas simples (sin modelos externos)."""
    if seed is not None:
        random.seed(seed)
    vec = [0.0] * dim
    for t in tokens:
        h = sum(ord(c) for c in t)
        for i in range(dim):
            vec[i] += math.sin((h + i) * 0.0137)
    return normalize(vec)


# ------------------------------------------
# THINK — modo automático Orion
# ------------------------------------------
def think(data, mode: str = "auto"):
    """Interfaz principal de IA en Orion."""
    if isinstance(data, str):
        return embed_text_tokens(data.split())
    elif isinstance(data, list) and all(isinstance(x, (int, float)) for x in data):
        X = [[i] for i in range(len(data))]
        w, b = linear_regression_fit(X, data)
        return {"weights": w, "bias": b}
    elif isinstance(data, list) and all(isinstance(x, list) for x in data):
        centers, labels = kmeans(data)
        return {"centers": centers, "labels": labels}
    else:
        return None


# ------------------------------------------
# Alias Orion (comandos cortos)
# ------------------------------------------
ALIASES = {
    "fit": linear_regression_fit,
    "predict": linear_regression_predict,
    "sim": cosine_similarity,
    "dist": euclidean_distance,
    "cluster": kmeans,
    "embed": embed_text_tokens,
}


# ------------------------------------------
# Exportador Orion Runtime
# ------------------------------------------
def orion_export():
    exports = {
        "think": think,
        "normalize": normalize,
        "accuracy": accuracy,
        "mse": mean_squared_error,
    }
    exports.update(ALIASES)
    return exports


__all__ = list(orion_export().keys())
