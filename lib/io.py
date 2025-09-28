"""
Entrada/Salida futurista en Orion.
La consola y los archivos como streams de datos.
"""

import sys
import json

def show(*args, sep=" ", end="\n"):
    """Imprime en consola estilo Orion."""
    sys.stdout.write(sep.join(str(a) for a in args) + end)

def ask(prompt="> "):
    """Lee input del usuario."""
    return input(prompt)

# --- Archivos ---
def read_file(path, mode="r"):
    with open(path, mode, encoding="utf-8") as f:
        return f.read()

def write_file(path, content, mode="w"):
    with open(path, mode, encoding="utf-8") as f:
        f.write(content)

def append_file(path, content):
    write_file(path, content, mode="a")

# --- JSON helpers ---
def read_json(path):
    return json.loads(read_file(path))

def write_json(path, data):
    write_file(path, json.dumps(data, indent=2))

# --- Futurista: stream reader ---
def stream_lines(path):
    """Generador que lee archivo línea por línea (lazy)."""
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            yield line.strip()
