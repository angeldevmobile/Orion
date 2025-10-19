"""
Orion Cognitive Engine
────────────────────────────────────────────
Sistema cognitivo híbrido para Orion.
Fusiona heurística, vectorización y aprendizaje ligero.

Principios:
- Usa numpy si está disponible, pero no depende de él.
- Embeddings cuánticos deterministas.
- Red neural-lite integrada (sin frameworks).
- Modo “think” adaptativo con razonamiento contextual.
- Memoria cognitiva para inferir patrones entre llamadas.
"""

import math, random, time
from collections import deque, Counter
from typing import List, Tuple, Optional, Any

# ------------------------------------------
# Backend opcional (aceleración con numpy)
# ------------------------------------------
try:
    import numpy as _np  # type: ignore
except Exception:
    _np = None


# ------------------------------------------
# Núcleo de memoria y contexto Orion
# ------------------------------------------
_MEMORY = deque(maxlen=32)  # Últimas 32 operaciones

def recall() -> List[dict]:
    """Retorna el contexto cognitivo actual."""
    return list(_MEMORY)

def _remember(action: str, data: Any):
    _MEMORY.append({"t": round(time.time(), 2), "action": action, "data": data})


# ------------------------------------------
# Embeddings cuánticos Orion
# ------------------------------------------
def quantum_embed(text, dim=128):  # Cambiar None por 128
    """
    Genera un embedding cuántico simulado para texto.
    Convierte texto en un vector de alta dimensión usando técnicas cuánticas simuladas.
    """
    # Asegurar que dim es un entero válido
    if dim is None or not isinstance(dim, int):
        dim = 128
    
    vec = [0.0] * dim
    for i, ch in enumerate(text):
        h = math.sin((ord(ch) * 0.137 + i) * 3.1415)
        for j in range(dim):
            vec[j] += math.sin(h * (j + 1) * 0.0317)
    norm = math.sqrt(sum(v*v for v in vec)) or 1
    res = [v / norm for v in vec]
    _remember("quantum_embed", {"len": len(text), "dim": dim})
    return res


# ------------------------------------------
# Utilidades matemáticas básicas
# ------------------------------------------
def euclidean_distance(a: List[float], b: List[float]) -> float:
    if _np:
        return float(_np.linalg.norm(_np.asarray(a) - _np.asarray(b)))
    return math.sqrt(sum((x - y)**2 for x, y in zip(a, b)))


def cosine_similarity(a: List[float], b: List[float]) -> float:
    if _np:
        a_n, b_n = _np.asarray(a), _np.asarray(b)
        na, nb = _np.linalg.norm(a_n), _np.linalg.norm(b_n)
        if na == 0 or nb == 0:
            return 0.0
        return float(_np.dot(a_n, b_n) / (na * nb))
    dot = sum(x*y for x, y in zip(a, b))
    na = math.sqrt(sum(x*x for x in a))
    nb = math.sqrt(sum(y*y for y in b))
    return 0.0 if na == 0 or nb == 0 else dot / (na * nb)


def normalize(vec: List[float]) -> List[float]:
    s = math.sqrt(sum(x*x for x in vec)) or 1
    return [x/s for x in vec]


def top_k_frequent(items: List, k: int = 5) -> List:
    c = Counter(items)
    return [item for item, _ in c.most_common(k)]


# ------------------------------------------
# Neural-lite (red neuronal simple)
# ------------------------------------------
def neural_lite_fit(X: List[List[float]], y: List[float],
                    hidden: int = 8, lr: float = 0.01, epochs: int = 200):
    """Entrena una red neuronal ligera de 1 capa oculta."""
    if not X:
        return None
    n, m = len(X), len(X[0])
    W1 = [[random.uniform(-0.1, 0.1) for _ in range(m)] for _ in range(hidden)]
    b1 = [0.0] * hidden
    W2 = [random.uniform(-0.1, 0.1) for _ in range(hidden)]
    b2 = 0.0

    def relu(x): return max(0.0, x)
    def d_relu(x): return 1.0 if x > 0 else 0.0

    for _ in range(epochs):
        for xi, yi in zip(X, y):
            h = [relu(sum(w*x for w, x in zip(wr, xi)) + b) for wr, b in zip(W1, b1)]
            y_pred = sum(w*h_i for w, h_i in zip(W2, h)) + b2
            err = y_pred - yi

            grad_W2 = [err * h_i for h_i in h]
            grad_b2 = err
            grad_W1, grad_b1 = [], []
            for j in range(hidden):
                dh = err * W2[j] * d_relu(h[j])
                grad_W1.append([dh * x for x in xi])
                grad_b1.append(dh)

            for j in range(hidden):
                for k in range(m):
                    W1[j][k] -= lr * grad_W1[j][k]
                b1[j] -= lr * grad_b1[j]
                W2[j] -= lr * grad_W2[j]
            b2 -= lr * grad_b2

    _remember("neural_fit", {"samples": n, "hidden": hidden})
    return {"W1": W1, "b1": b1, "W2": W2, "b2": b2}


def neural_lite_predict(X: List[List[float]], model):
    """Predice con una red neural-lite entrenada."""
    def relu(x): return max(0.0, x)
    W1, b1, W2, b2 = model["W1"], model["b1"], model["W2"], model["b2"]
    preds = []
    for xi in X:
        h = [relu(sum(w*x for w, x in zip(wr, xi)) + b) for wr, b in zip(W1, b1)]
        preds.append(sum(w*h_i for w, h_i in zip(W2, h)) + b2)
    return preds


# ------------------------------------------
# Clustering (K-means Orion)
# ------------------------------------------
def kmeans(points: List[List[float]], k: int = 2, iterations: int = 20, seed: Optional[int] = None, **kwargs):
    if not points:
        return [], []
    # Soporte para k="auto" o k como string
    if isinstance(k, str):
        if k == "auto":
            k = min(2, len(points))  # O elige otro valor automático
        else:
            try:
                k = int(k)
            except Exception:
                k = 2
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
                centers[j] = [sum(col)/len(cluster) for col in zip(*cluster)]
        if not changed:
            break
    _remember("cluster", {"k": k, "iters": iterations})
    return centers, labels


# ------------------------------------------
# Métricas Orion
# ------------------------------------------
def mean_squared_error(y_true: List[float], y_pred: List[float]) -> float:
    n = len(y_true)
    return 0.0 if n == 0 else sum((a - b)**2 for a, b in zip(y_true, y_pred)) / n


def accuracy(y_true: List, y_pred: List) -> float:
    return 0.0 if not y_true else sum(1 for a, b in zip(y_true, y_pred) if a == b) / len(y_true)


# ------------------------------------------
# THINK — modo de intuición Orion
# ------------------------------------------
def think(data, mode: str = "auto"):
    """Modo cognitivo Orion: genera resumen semántico o analiza patrones."""
    _remember("think", {"input_type": str(type(data))})

    # --- Caso 1: texto o lista de títulos ---
    if isinstance(data, list):
        data = " ".join([str(x) for x in data])
    if isinstance(data, str):
        emb = quantum_embed(data)
        words = [w for w in data.split() if len(w) > 3]
        if not words:
            return {"type": "text", "summary": "(sin contenido)", "embedding": emb}

        from collections import Counter
        freq = Counter(words)
        key_terms = [w for w, _ in freq.most_common(5)]
        summary = f"Resumen cognitivo: este grupo trata sobre {', '.join(key_terms)}."
        return {"type": "text", "summary": summary, "embedding": emb}

    # --- Caso 2: lista numérica ---
    elif isinstance(data, list) and all(isinstance(x, (int, float)) for x in data):
        trend = "creciente" if data[-1] > sum(data)/len(data) else "decreciente"
        return {"type": "series", "trend": trend, "avg": sum(data)/len(data)}

    # --- Caso 3: matriz ---
    elif isinstance(data, list) and all(isinstance(x, list) for x in data):
        centers, labels = kmeans(data)
        return {"type": "matrix", "clusters": len(centers), "labels": labels}

    return {"type": "unknown", "summary": "(no interpretable)"}

# ------------------------------------------
# Alias Orion (comandos cortos)
# ------------------------------------------
ALIASES = {
    "fit": neural_lite_fit,
    "predict": neural_lite_predict,
    "sim": cosine_similarity,
    "dist": euclidean_distance,
    "cluster": kmeans,
    "embed": quantum_embed,
    "recall": recall
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
