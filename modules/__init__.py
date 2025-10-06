"""
Loader de módulos del sistema Orion.
Permite: use fs, use json, use net, use log
"""

import importlib

_loaded_modules = {}

def load_module(env: dict, name: str):
    """
    Carga dinámicamente un módulo Orion del directorio modules/
    y lo registra en el entorno.
    """
    if name in _loaded_modules:
        return _loaded_modules[name]

    try:
        mod = importlib.import_module(f"modules.{name}")
    except ImportError:
        raise RuntimeError(f"Módulo '{name}' no encontrado en Orion")

    exports = {}
    for key, val in mod.__dict__.items():
        if not key.startswith("_") and callable(val):
            exports[key] = val

    env[name] = exports
    _loaded_modules[name] = exports

    # --- 🚀 registrar funciones globales para "log"
    if name == "log":
        env.update({
            "trace_start": mod.trace_start,
            "trace_end": mod.trace_end,
            "divider": mod.divider,
            "progress": mod.progress,
            "info": mod.info,
            "ok": mod.ok,
            "warn": mod.warn,
            "error": mod.error,
            "debug": mod.debug,
        })

    return exports
