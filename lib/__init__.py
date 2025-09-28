"""
Loader de la librería estándar de Orion (stdlib).
Permite: use math, use strings, use collections, use io
"""

import importlib

# cache de módulos ya cargados
_loaded_libs = {}

def load_lib(env: dict, name: str):
    """
    Carga dinámicamente un módulo Orion desde lib/
    y lo registra en el entorno bajo su namespace.
    """
    if name in _loaded_libs:
        return _loaded_libs[name]

    try:
        mod = importlib.import_module(f"lib.{name}")
    except ImportError:
        raise RuntimeError(f" Librería estándar '{name}' no encontrada en Orion")

    exports = {}
    for key, val in mod.__dict__.items():
        if not key.startswith("_") and callable(val):
            exports[key] = val
        # También exportamos constantes (ej: PI, E, etc.)
        if not key.startswith("_") and not callable(val):
            exports[key] = val

    env[name] = exports
    _loaded_libs[name] = exports
    return exports
