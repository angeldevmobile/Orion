# core/functions.py

"""
Utilities para manejar funciones en Orion Language.
Soporta:
- Funciones definidas por el usuario
- Sobrecarga de funciones
- Closures
- Funciones anónimas
- Funciones nativas (wrappers Python)
"""

from copy import deepcopy


# --- Registro de funciones ---
def register_function(env: dict, name: str, params: list, body: list,
                      closure=None, is_async=False,
                      param_types: dict | None = None,
                      return_type: str | None = None):
    """
    Registra una función en el entorno.
    closure:     entorno capturado (para closures).
    is_async:    marca si es async.
    param_types: dict nombre→tipo de cada parámetro anotado.
    return_type: tipo declarado de retorno, o None.
    """
    if name not in env:
        env[name] = []

    env[name].append({
        "type": "FN_DEF",
        "params": params,
        "body": body,
        "closure": deepcopy(closure) if closure else {},
        "async": is_async,
        "param_types": param_types or {},
        "return_type": return_type,
    })


def is_function(env: dict, name: str):
    """Verifica si existe alguna función definida en Orion."""
    return name in env and any(fn["type"] == "FN_DEF" for fn in env[name])


def get_function(env: dict, name: str, arg_count=None):
    """
    Obtiene una función según su nombre.
    Si hay sobrecarga, selecciona por número de argumentos.
    Incluye funciones nativas.
    """
    if name not in env:
        return None

    # Buscar funciones definidas por el usuario
    candidates = [fn for fn in env[name] if fn["type"] == "FN_DEF"]

    # Si no hay, buscar funciones nativas
    if not candidates:
        candidates = [fn for fn in env[name] if fn["type"] == "NATIVE_FN"]

    if not candidates:
        return None

    if arg_count is None:
        return candidates[0]  # primera definición

    # buscar por número de argumentos (solo aplica a FN_DEF)
    for fn in candidates:
        if fn["type"] == "FN_DEF" and len(fn["params"]) == arg_count:
            return fn

    return candidates[0]  # si es nativa, no valida número de argumentos


# --- Funciones anónimas ---
def create_anonymous_function(params: list, body: list, closure=None):
    """
    Crea una función anónima (sin nombre).
    Retorna un diccionario tipo función que puede guardarse en variables.
    """
    return {
        "type": "ANON_FN",
        "params": params,
        "body": body,
        "closure": deepcopy(closure) if closure else {},
        "async": False
    }


# --- Funciones nativas ---
def register_native_function(env: dict, name: str, py_func):
    """
    Registra una función nativa (Python) en Orion.
    Ejemplo:
        register_native_function(env, "now", lambda: 2025)
    """
    if name not in env:
        env[name] = []

    env[name].append({
        "type": "NATIVE_FN",
        "impl": py_func
    })


def is_native_function(fn_def):
    return fn_def and fn_def["type"] == "NATIVE_FN"


def call_native_function(fn_def, args):
    """Ejecuta una función nativa."""
    return fn_def["impl"](*args)
