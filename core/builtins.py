"""
Registro automático de funciones nativas para Orion.
Carga todas las funciones de core/math.py en el entorno.
"""

from core.functions import register_native_function
from lib.io import show
from lib import math as orion_math

def load_builtins(env):
    """
    Registra todas las funciones de core/math.py como nativas en Orion.
    """
    for name in dir(orion_math):
        if not name.startswith("_"):  # ignorar privados
            attr = getattr(orion_math, name)
            if callable(attr):  # solo registrar funciones
                register_native_function(env, name, attr)

    # también podemos registrar utilidades como print/show
    register_native_function(env, "show", show)
