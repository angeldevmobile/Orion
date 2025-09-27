# core/functions.py

"""
Utilities para manejar funciones (registro simple, closures mínimos).
"""

def register_function(env: dict, name: str, params: list, body: list):
    """Registra una función en el entorno global."""
    env[name] = ("FN_DEF", params, body)

def is_function(env: dict, name: str):
    fn = env.get(name)
    return fn is not None and fn[0] == "FN_DEF"

def get_function(env: dict, name: str):
    fn = env.get(name)
    if not fn or fn[0] != "FN_DEF":
        return None
    return fn  # ("FN_DEF", params, body)
