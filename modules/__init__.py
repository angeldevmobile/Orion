"""
Loader de módulos del sistema Orion.
Permite: use fs, use json, use net
"""

import importlib

# cache para evitar recargar el mismo módulo varias veces
_loaded_modules = {}

def load_module(env: dict, name: str):
    """
    Carga dinámicamente un módulo Orion del directorio modules/
    y lo registra en el entorno.
    """
    if name in _loaded_modules:
        # ya estaba cargado
        return _loaded_modules[name]

    try:
        mod = importlib.import_module(f"modules.{name}")
    except ImportError:
        raise RuntimeError(f"Módulo '{name}' no encontrado en Orion")

    # Exponemos solo funciones (no privadas)
    exports = {}
    for key, val in mod.__dict__.items():
        if not key.startswith("_") and callable(val):
            exports[key] = val

    # Guardamos en el entorno bajo el namespace del módulo
    env[name] = exports
    _loaded_modules[name] = exports
    return exports
