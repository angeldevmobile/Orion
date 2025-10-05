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


# =========================================================
# 1. CORE — The Essence of Orion JSON
# =========================================================

def absorb(path):
    """Absorbs a JSON file and returns an Orion object."""
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def emit(path, obj, beauty=True):
    """Emits an Orion object to a JSON file."""
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=2 if beauty else None, ensure_ascii=False)


def parse(raw):
    """Transmutes a raw JSON string into a live Orion object."""
    return json.loads(raw)


def forge(obj, beauty=False):
    """Forges a JSON string from an Orion object."""
    return json.dumps(obj, indent=2 if beauty else None, ensure_ascii=False)


# =========================================================
# 2. STRUCTURAL INTELLIGENCE
# =========================================================

def fuse(*objs):
    """Fuses multiple JSON objects into one unified entity."""
    result = {}
    for o in objs:
        result.update(o)
    return result


def trace(obj, path):
    """Traces a value within a JSON using a route like 'user.profile.name'."""
    parts = path.split(".")
    cur = obj
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
    diffs = {"added": {}, "removed": {}, "altered": {}}
    keys = set(a.keys()) | set(b.keys())
    for k in keys:
        if k not in a:
            diffs["added"][k] = b[k]
        elif k not in b:
            diffs["removed"][k] = a[k]
        elif a[k] != b[k]:
            diffs["altered"][k] = {"from": a[k], "to": b[k]}
    return diffs


# =========================================================
# 3. DYNAMIC MANIPULATION
# =========================================================

def filter_by(items, condition):
    """Filters a list of JSON objects using a lambda condition."""
    if not isinstance(items, list):
        return []
    return [x for x in items if condition(x)]


def extract(obj, fields):
    """Extracts selected keys from a JSON object or list."""
    if isinstance(obj, list):
        return [{k: v for k, v in o.items() if k in fields} for o in obj]
    elif isinstance(obj, dict):
        return {k: v for k, v in obj.items() if k in fields}
    return obj


def replicate(obj):
    """Replicates a JSON object deeply."""
    return deepcopy(obj)


def purify(obj):
    """Purifies a JSON object, removing null, empty, or void values."""
    if isinstance(obj, dict):
        return {k: purify(v) for k, v in obj.items() if v not in (None, "", [], {})}
    elif isinstance(obj, list):
        return [purify(v) for v in obj if v not in (None, "", [], {})]
    return obj


# =========================================================
# 4. ADVANCED — Modern productivity
# =========================================================

def merge_deep(a, b):
    """Deep merge of two JSON objects (recursive)."""
    res = deepcopy(a)
    for k, v in b.items():
        if k in res and isinstance(res[k], dict) and isinstance(v, dict):
            res[k] = merge_deep(res[k], v)
        else:
            res[k] = deepcopy(v)
    return res


def sort_keys(obj, deep=True):
    """Sorts JSON object keys for determinism."""
    if isinstance(obj, dict):
        return {k: sort_keys(obj[k], deep) if deep else obj[k] for k in sorted(obj)}
    elif isinstance(obj, list):
        return [sort_keys(x, deep) for x in obj] if deep else obj
    return obj


def patch(obj, changes):
    """
    Applies a shallow patch (mini JSON patch).
    Example: patch(user, {"age": 31})
    """
    res = deepcopy(obj)
    res.update(changes)
    return res


# =========================================================
# 5. FUTURIST — Orion-only powers
# =========================================================

def validate(obj, schema):
    """
    Minimalistic schema validator.
    Schema example: {"name": str, "age": int}
    """
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
                    yield obj
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
