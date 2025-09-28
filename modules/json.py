"""
Módulo JSON futurista en Orion.
Soporta lectura, escritura, merge y formateo estilizado.
"""

import json

def parse(text):
    """Convierte string JSON a objeto Orion."""
    return json.loads(text)

def stringify(obj, pretty=False):
    """Convierte objeto Orion a string JSON."""
    return json.dumps(obj, indent=2 if pretty else None)

def load(path):
    """Carga JSON desde archivo."""
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)

def save(path, obj, pretty=True):
    """Guarda objeto como JSON en archivo."""
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=2 if pretty else None)

# --- Futurista ---
def merge(*objs):
    """Combina múltiples objetos JSON en uno solo."""
    result = {}
    for obj in objs:
        result.update(obj)
    return result

def query(obj, path):
    """
    Consulta tipo 'user.profile.name'
    """
    parts = path.split(".")
    cur = obj
    for p in parts:
        if isinstance(cur, dict) and p in cur:
            cur = cur[p]
        else:
            return None
    return cur
