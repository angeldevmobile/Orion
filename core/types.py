# core/types.py

import re

class OrionString:
    """Encapsula un string de Orion y permite interpolación."""
    INTERP_RE = re.compile(r"\$\{([a-zA-Z_][a-zA-Z0-9_]*)\}")

    def __init__(self, value: str):
        self.value = value

    def __str__(self):
        return self.value

    def interpolate(self, env: dict):
        """Reemplaza ${var} por el valor en env (si existe)."""
        def repl(m):
            name = m.group(1)
            if name in env:
                val = env[name]
                # si es objeto OrionString
                if isinstance(val, OrionString):
                    return str(val)
                return str(val)
            return ""  # missing -> empty string
        return OrionString(self.INTERP_RE.sub(repl, self.value))

def null_safe(obj, attr):
    """Operador null-safe: si obj es None devuelve None, si no getattr."""
    if obj is None:
        return None
    return getattr(obj, attr, None)
