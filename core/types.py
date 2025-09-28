# core/types.py

import re
from datetime import datetime

class OrionString:
    """String de Orion con interpolación dinámica futurista."""
    INTERP_RE = re.compile(r"\$\{([a-zA-Z_][a-zA-Z0-9_]*)\}")

    def __init__(self, value: str):
        self.value = value

    def __str__(self):
        return self.value

    def interpolate(self, env: dict):
        """Reemplaza ${var} por su valor en el entorno."""
        def repl(m):
            name = m.group(1)
            if name in env:
                val = env[name]
                if isinstance(val, OrionString):
                    return str(val)
                return str(val)
            return ""  # si no existe, se reemplaza por vacío
        return OrionString(self.INTERP_RE.sub(repl, self.value))

    def futuristic_upper(self):
        """Convierte el string en MAYÚSCULAS futuristas (ejemplo de extensión)."""
        return OrionString(self.value.upper() + " ⚡️")


class OrionNumber:
    """Número con operaciones extendidas."""
    def __init__(self, value):
        self.value = value

    def __str__(self):
        return str(self.value)

    def add(self, other):
        return OrionNumber(self.value + other.value)

    def futuristic_power(self, exp):
        """Potencia elevada a otro nivel 🚀"""
        return OrionNumber(self.value ** exp.value)


class OrionBool:
    """Booleano futurista con extras."""
    def __init__(self, value: bool):
        self.value = bool(value)

    def __str__(self):
        return "yes" if self.value else "no"  # futurista: no usar True/False clásicos

    def toggle(self):
        """Invierte el valor (yes -> no, no -> yes)."""
        return OrionBool(not self.value)


class OrionDate:
    """Tipo de fecha nativo de Orion."""
    def __init__(self, year, month, day):
        self.date = datetime(year, month, day)

    def __str__(self):
        return self.date.strftime("%Y-%m-%d")

    def futuristic_format(self):
        """Formato futurista bonito."""
        return self.date.strftime(" %d-%m-%Y")


# 🔹 Operador null-safe universal
def null_safe(obj, attr):
    """Operador null-safe: si obj es None devuelve None, si no getattr."""
    if obj is None:
        return None
    return getattr(obj, attr, None)
