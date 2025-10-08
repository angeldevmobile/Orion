"""
Loader de módulos del sistema Orion.
Permite: use fs, use json, use net, use log, etc.
"""

import importlib
import inspect

_loaded_modules = {}

def load_module(env: dict, name: str):
    """
    Carga dinámicamente un módulo Orion del directorio modules/
    y registra sus funciones en el entorno global.
    """
    if name in _loaded_modules:
        return _loaded_modules[name]

    try:
        mod = importlib.import_module(f"modules.{name}")
    except ImportError:
        raise RuntimeError(f"Módulo '{name}' no encontrado en Orion")

    exports = {}

    # Registrar funciones públicas
    for key, val in inspect.getmembers(mod, inspect.isfunction):
        if not key.startswith("_"):
            exports[key] = val
            env[key] = val  #  Disponible globalmente (p. ej. progress, trace_start)

    env[name] = mod
    _loaded_modules[name] = mod

    print(f"[Orion] Módulo '{name}' cargado con {len(exports)} funciones exportadas.")
    return exports
