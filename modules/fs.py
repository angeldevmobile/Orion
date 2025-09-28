"""
Módulo de sistema de archivos para Orion.
Moderno y futurista: inspirado en Node.js pero más simple.
"""

import os
from pathlib import Path
import shutil

# --- Paths ---
def cwd():
    """Directorio actual."""
    return str(Path.cwd())

def ls(path="."):
    """Lista archivos en un directorio."""
    return [str(p) for p in Path(path).iterdir()]

def exists(path):
    """¿Existe el archivo/directorio?"""
    return Path(path).exists()

def is_file(path):
    return Path(path).is_file()

def is_dir(path):
    return Path(path).is_dir()

# --- Archivos ---
def read(path):
    with open(path, "r", encoding="utf-8") as f:
        return f.read()

def write(path, content):
    with open(path, "w", encoding="utf-8") as f:
        f.write(content)

def append(path, content):
    with open(path, "a", encoding="utf-8") as f:
        f.write(content)

def delete(path):
    Path(path).unlink(missing_ok=True)

# --- Directorios ---
def mkdir(path, exist_ok=True):
    Path(path).mkdir(parents=True, exist_ok=exist_ok)

def rmdir(path):
    shutil.rmtree(path, ignore_errors=True)

# --- Futurista ---
def space(path="."):
    """Devuelve espacio total, usado y libre en bytes."""
    st = os.statvfs(path)
    total = st.f_frsize * st.f_blocks
    free  = st.f_frsize * st.f_bfree
    used  = total - free
    return {"total": total, "used": used, "free": free}
