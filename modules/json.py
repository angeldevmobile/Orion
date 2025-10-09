"""
Orion JSON Module
────────────────────────────────────────────
Futuristic, expressive and human-readable.
Absorbs, fuses, purifies, and emits JSON with clarity.

Core principles:
- Simple verbs with clear intent
- Human-like semantics: absorb, emit, fuse, trace
- Designed for elegance, not verbosity
"""

import json
from copy import deepcopy
from core.types import OrionList, OrionDict, OrionBool


# =========================================================
# 0. INTERNAL CONVERSION HELPERS
# =========================================================

def _to_native(obj):
    """Convierte objetos Orion a tipos nativos de Python para serialización."""
    from core.types import OrionList, OrionDict, OrionBool, OrionString, OrionNumber

    if isinstance(obj, OrionList):
        return [_to_native(x) for x in obj.items]

    if isinstance(obj, OrionDict):
        clean_dict = {}
        for k, v in obj.value.items():
            key = k.value if hasattr(k, "value") else str(k)
            print("DEBUG KEY:", repr(key))  # Depuración
            key = key.replace('"', '')  # elimina todas las comillas dobles
            clean_dict[key] = _to_native(v)
        return clean_dict

    if isinstance(obj, OrionBool):
        return bool(obj.value)

    if isinstance(obj, OrionString):
        return str(obj.value)

    if isinstance(obj, OrionNumber):
        return obj.value

    # Recursividad para listas y dicts nativos
    if isinstance(obj, list):
        return [_to_native(x) for x in obj]
    if isinstance(obj, dict):
        return {str(k): _to_native(v) for k, v in obj.items()}

    return obj


def _to_orion(obj):
    """Converts native Python structures back into Orion objects."""
    if isinstance(obj, list):
        return OrionList([_to_orion(x) for x in obj])
    elif isinstance(obj, dict):
        return OrionDict({k: _to_orion(v) for k, v in obj.items()})
    elif isinstance(obj, bool):
        return OrionBool(obj)
    else:
        return obj


# =========================================================
# 1. CORE — The Essence of Orion JSON
# =========================================================

def absorb(path):
    """Absorbs a JSON file and returns an Orion object."""
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)
        return _to_orion(data)


def emit(path, obj, beauty=True):
    """Emits an Orion object to a JSON file."""
    native = _to_native(obj)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(native, f, indent=2 if beauty else None, ensure_ascii=False)


def parse(raw):
    """Transmutes a raw JSON string into a live Orion object."""
    data = json.loads(raw)
    return _to_orion(data)


def forge(obj, beauty=False):
    """Forges a JSON string from an Orion object."""
    native_obj = _to_native(obj)
    return json.dumps(native_obj, indent=2 if beauty else None, ensure_ascii=False)


# =========================================================
# 2. STRUCTURAL INTELLIGENCE
# =========================================================

def fuse(*objs):
    """Fuses multiple JSON objects into one unified entity."""
    result = {}
    for o in objs:
        result.update(_to_native(o))
    return _to_orion(result)


def trace(obj, path):
    """Traces a value within a JSON using a route like 'user.profile.name'."""
    parts = path.split(".")
    cur = _to_native(obj)
    for p in parts:
        if isinstance(cur, dict) and p in cur:
            cur = cur[p]
        else:
            return None
    return cur


def haspath(obj, path):
    """Checks if a route exists within a JSON structure."""
    return trace(obj, path) is not None


def shiftmap(a, b):
    """Reveals mutations between two JSON structures (like a diff)."""
    a_native = _to_native(a)
    b_native = _to_native(b)
    diffs = {"added": {}, "removed": {}, "altered": {}}
    keys = set(a_native.keys()) | set(b_native.keys())
    for k in keys:
        if k not in a_native:
            diffs["added"][k] = b_native[k]
        elif k not in b_native:
            diffs["removed"][k] = a_native[k]
        elif a_native[k] != b_native[k]:
            diffs["altered"][k] = {"from": a_native[k], "to": b_native[k]}
    return diffs


# =========================================================
# 3. DYNAMIC MANIPULATION
# =========================================================

def filter_by(items, condition):
    """Filters a list of JSON objects using a lambda condition."""
    items = _to_native(items)
    if not isinstance(items, list):
        return OrionList([])
    return OrionList([_to_orion(x) for x in items if condition(x)])


def extract(obj, fields):
    """Extracts selected keys from a JSON object or list."""
    obj = _to_native(obj)
    if isinstance(obj, list):
        return OrionList([{k: v for k, v in o.items() if k in fields} for o in obj])
    elif isinstance(obj, dict):
        return OrionDict({k: v for k, v in obj.items() if k in fields})
    return obj


def replicate(obj):
    """Replicates a JSON object deeply."""
    return deepcopy(obj)


def purify(obj):
    """Purifies a JSON object, removing null, empty, or void values."""
    obj = _to_native(obj)
    if isinstance(obj, dict):
        return OrionDict({k: purify(v) for k, v in obj.items() if v not in (None, "", [], {})})
    elif isinstance(obj, list):
        return OrionList([purify(v) for v in obj if v not in (None, "", [], {})])
    return obj


# =========================================================
# 4. ADVANCED — Modern productivity
# =========================================================

def merge_deep(a, b):
    """Deep merge of two JSON objects (recursive)."""
    a = _to_native(a)
    b = _to_native(b)
    res = deepcopy(a)
    for k, v in b.items():
        if k in res and isinstance(res[k], dict) and isinstance(v, dict):
            res[k] = merge_deep(res[k], v)
        else:
            res[k] = deepcopy(v)
    return _to_orion(res)


def sort_keys(obj, deep=True):
    """Sorts JSON object keys for determinism."""
    obj = _to_native(obj)
    if isinstance(obj, dict):
        return OrionDict({k: sort_keys(obj[k], deep) if deep else obj[k] for k in sorted(obj)})
    elif isinstance(obj, list):
        return OrionList([sort_keys(x, deep) for x in obj] if deep else obj)
    return obj


def patch(obj, changes):
    """
    Applies a shallow patch (mini JSON patch).
    Example: patch(user, {"age": 31})
    """
    obj = _to_native(obj)
    res = deepcopy(obj)
    res.update(changes)
    return _to_orion(res)


# =========================================================
# 5. FUTURIST — Orion-only powers
# =========================================================

def validate(obj, schema):
    """Minimalistic schema validator."""
    obj = _to_native(obj)
    for k, t in schema.items():
        if k not in obj or not isinstance(obj[k], t):
            return False
    return True


def stream_absorb(path):
    """Reads a large JSON file progressively (streaming)."""
    decoder = json.JSONDecoder()
    with open(path, "r", encoding="utf-8") as f:
        buf = ""
        for chunk in f:
            buf += chunk
            try:
                while buf:
                    obj, idx = decoder.raw_decode(buf)
                    yield _to_orion(obj)
                    buf = buf[idx:].lstrip()
            except json.JSONDecodeError:
                continue


def encrypt(obj, key):
    """Encrypts a JSON object with a simple XOR cipher (demo purpose)."""
    raw = forge(obj)
    return "".join(chr(ord(c) ^ key) for c in raw)


def decrypt(raw, key):
    """Decrypts a JSON object previously encrypted with encrypt()."""
    plain = "".join(chr(ord(c) ^ key) for c in raw)
    return parse(plain)

def stream_emit(path, obj, on_chunk=None):
    native = _to_native(obj)
    raw = json.dumps(native, indent=2)
    for i in range(0, len(raw), 512):
        chunk = raw[i:i+512]
        on_chunk(chunk)
