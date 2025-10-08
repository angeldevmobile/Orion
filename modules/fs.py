"""
Módulo de sistema de archivos futurista de Orion.
Rápido, moderno y con estilo minimalista. Inspirado en Rust + Node.js.

Características:
- Lectura y escritura síncrona/asíncrona
- Streams, backups y hashing rápido
- Manipulación de paths inteligente
- Métodos cortos, poderosos y expresivos
"""

import os
import shutil
import hashlib
import asyncio
from pathlib import Path
from datetime import datetime

# =========================================================
# PATHS Y UTILIDADES
# =========================================================

def cwd():
    """Devuelve el directorio actual."""
    return str(Path.cwd())

def ls(path="."):
    """Lista archivos y carpetas en el directorio."""
    return [str(p) for p in Path(path).iterdir()]

def walk(path="."):
    """Recorre el árbol de archivos recursivamente."""
    return [str(p) for p in Path(path).rglob("*")]

def home():
    """Devuelve el directorio del usuario."""
    return str(Path.home())

def join(*parts):
    """Une rutas de forma segura."""
    return str(Path(*parts))

def exists(path):
    """¿Existe el archivo o directorio?"""
    path = _to_str_path(path)
    return Path(path).exists()

def is_file(path):
    return Path(path).is_file()

def is_dir(path):
    return Path(path).is_dir()


# =========================================================
# ARCHIVOS
# =========================================================

def read(path, binary=False):
    """Lee el contenido de un archivo."""
    path = _to_str_path(path)
    mode = "rb" if binary else "r"
    with open(path, mode, encoding=None if binary else "utf-8") as f:
        return f.read()

def write(path, content, binary=False):
    """Escribe contenido en un archivo (sobrescribe)."""
    path = _to_str_path(path)
    mode = "wb" if binary else "w"
    with open(path, mode, encoding=None if binary else "utf-8") as f:
        f.write(content)

def append(path, content):
    """Agrega texto al final de un archivo."""
    path = _to_str_path(path)
    with open(path, "a", encoding="utf-8") as f:
        f.write(content)

def copy(src, dst, overwrite=True):
    """Copia un archivo."""
    if not overwrite and Path(dst).exists():
        return False
    shutil.copy2(src, dst)
    return True

def move(src, dst):
    """Mueve un archivo."""
    shutil.move(src, dst)

def delete(path):
    """Elimina un archivo."""
    Path(path).unlink(missing_ok=True)

def backup(path, suffix=".bak"):
    """Crea un backup rápido de un archivo."""
    if not exists(path):
        return False
    dst = f"{path}{suffix}"
    shutil.copy2(path, dst)
    return dst

def autobackup(path, target_dir=".autobackups", keep_last=5, algo="sha256"):
    """
    Crea automáticamente una copia de seguridad solo si el archivo cambió.
    Guarda hasta `keep_last` versiones y elimina las más antiguas.

    Ejemplo:
        fs.autobackup("config.json")
    """
    p = Path(path)
    if not p.exists():
        return None

    # Crear directorio de backups
    mkdir(target_dir)
    base = p.stem
    ext = p.suffix
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    dst = Path(target_dir) / f"{base}_{timestamp}{ext}"

    # --- Calcular hash actual ---
    current_hash = hash(path, algo)
    hash_file = Path(target_dir) / f"{base}.last_hash"

    # Leer hash previo (si existe)
    prev_hash = None
    if hash_file.exists():
        prev_hash = read(hash_file).strip()

    # Si el contenido no cambió → no hacer backup
    if prev_hash == current_hash:
        return None

    # Crear nueva copia y actualizar hash
    shutil.copy2(path, dst)
    write(hash_file, current_hash)

    # --- Mantener solo las últimas `keep_last` copias ---
    backups = sorted(
        [b for b in Path(target_dir).glob(f"{base}_*{ext}")],
        key=lambda x: x.stat().st_mtime,
        reverse=True
    )
    for old in backups[keep_last:]:
        old.unlink()

    return str(dst)

# =========================================================
# ASYNC (para tareas rápidas sin bloqueo)
# =========================================================

async def read_async(path):
    loop = asyncio.get_event_loop()
    return await loop.run_in_executor(None, read, path)

async def write_async(path, content):
    loop = asyncio.get_event_loop()
    await loop.run_in_executor(None, write, path, content)


# =========================================================
# DIRECTORIOS
# =========================================================

def mkdir(path, exist_ok=True):
    """Crea un directorio (y subdirectorios si es necesario)."""
    Path(path).mkdir(parents=True, exist_ok=exist_ok)

def clear_dir(path):
    """Elimina todo dentro de un directorio, sin borrar el directorio."""
    p = Path(path)
    for child in p.iterdir():
        if child.is_file():
            child.unlink()
        else:
            shutil.rmtree(child)

def rmdir(path):
    """Elimina un directorio completo."""
    shutil.rmtree(path, ignore_errors=True)


# =========================================================
# METADATOS
# =========================================================

def info(path):
    """Devuelve información detallada del archivo."""
    p = Path(path)
    stat = p.stat()
    return {
        "name": p.name,
        "path": str(p.resolve()),
        "size": stat.st_size,
        "modified": datetime.fromtimestamp(stat.st_mtime).isoformat(),
        "created": datetime.fromtimestamp(stat.st_ctime).isoformat(),
        "is_file": p.is_file(),
        "is_dir": p.is_dir(),
    }

def hash(path, algo="sha256"):
    """Calcula el hash del archivo (por defecto SHA-256)."""
    h = hashlib.new(algo)
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


# =========================================================
# FUNCIONES FUTURISTAS
# =========================================================

def space(path="."):
    """Devuelve el espacio total, usado y libre en bytes."""
    st = os.statvfs(path)
    total = st.f_frsize * st.f_blocks
    free  = st.f_frsize * st.f_bavail
    used  = total - free
    return {"total": total, "used": used, "free": free}

def clone_dir(src, dst, include_hidden=False):
    """Copia una carpeta completa (tipo duplicar proyecto)."""
    src_p, dst_p = Path(src), Path(dst)
    if not src_p.exists():
        raise FileNotFoundError(f"Origen no encontrado: {src}")
    if dst_p.exists():
        shutil.rmtree(dst_p)
    shutil.copytree(src_p, dst_p, ignore=None if include_hidden else shutil.ignore_patterns(".*"))
    return str(dst_p)

def snapshot(path, target_dir=".snapshots"):
    """Guarda un snapshot temporal del archivo o carpeta."""
    mkdir(target_dir)
    base = Path(path).name
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    dst = Path(target_dir) / f"{base}_{timestamp}"
    if Path(path).is_file():
        shutil.copy2(path, dst)
    else:
        shutil.copytree(path, dst)
    return str(dst)

def versioned_snapshot(path, target_dir=".vshots", keep_last=10):
    """
    Snapshot tipo git-lite: guarda historial versionado.
    """
    mkdir(target_dir)
    base = Path(path).name
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    dst = Path(target_dir) / f"{base}_{timestamp}"

    if Path(path).is_file():
        shutil.copy2(path, dst)
    else:
        shutil.copytree(path, dst)

    # Mantener últimos N
    versions = sorted(Path(target_dir).glob(f"{base}_*"),
                      key=lambda x: x.stat().st_mtime,
                      reverse=True)
    for old in versions[keep_last:]:
        rmdir(old)
    return str(dst)

# --- Streams
def stream_read(path, chunk_size=8192):
    """Lee un archivo como stream (generador)."""
    with open(path, "rb") as f:
        while chunk := f.read(chunk_size):
            yield chunk

def stream_hash(path, algo="sha256", chunk_size=8192):
    """Calcula hash progresivo mientras lee un archivo."""
    h = hashlib.new(algo)
    for chunk in stream_read(path, chunk_size):
        h.update(chunk)
        yield chunk, h.hexdigest()

# --- Atomicidad
def safe_write(path, content, binary=False):
    """Escribe de forma atómica evitando corrupción."""
    tmp = f"{path}.tmp"
    write(tmp, content, binary=binary)
    os.replace(tmp, path)

# --- Declarativos
def ensure(path, default=""):
    """Crea archivo con contenido por defecto si no existe."""
    if not exists(path):
        write(path, default)
    return path

def _to_str_path(p):
    # Convierte OrionString a str nativo si es necesario
    if hasattr(p, "value"):
        return str(p.value)
    return str(p)