"""
Colecciones futuristas en Orion.
Incluye listas, mapas y utilidades funcionales rápidas.
"""

from functools import reduce

# --- List utilities ---
def list_new(*args):
    """Crea una nueva lista Orion."""
    return list(args)

def list_flat(seq):
    """Aplana una lista de listas en O(n)."""
    return [x for sub in seq for x in sub]

def list_unique(seq):
    """Elimina duplicados, preservando orden."""
    seen, out = set(), []
    for x in seq:
        if x not in seen:
            seen.add(x)
            out.append(x)
    return out

def list_chunk(seq, size):
    """Divide en bloques de tamaño size."""
    return [seq[i:i+size] for i in range(0, len(seq), size)]

def list_cycle(seq, n):
    """Repite lista n veces."""
    return seq * n

def list_find(seq, fn):
    """Encuentra el primer elemento que cumpla fn."""
    return next((x for x in seq if fn(x)), None)

# --- Map utilities ---
def map_new(pairs):
    """Crea diccionario desde [(k, v), ...]."""
    return {k: v for k, v in pairs}

def map_merge(a, b):
    """Fusiona dos diccionarios (b sobrescribe a)."""
    return {**a, **b}

def map_invert(d):
    """Invierte claves y valores."""
    return {v: k for k, v in d.items()}

# --- Functional style ---
def col_map(fn, seq):
    return [fn(x) for x in seq]

def col_filter(fn, seq):
    return [x for x in seq if fn(x)]

def col_reduce(fn, seq, init=None):
    return reduce(fn, seq, init) if init is not None else reduce(fn, seq)

def col_sort(seq, key=None, desc=False):
    return sorted(seq, key=key, reverse=desc)

def col_zip(*seqs):
    return list(zip(*seqs))
  