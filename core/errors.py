"""
Sistema de errores para Orion Language.
Define excepciones personalizadas y un mecanismo uniforme de reporte.
"""

class OrionError(Exception):
    """Base de todos los errores de Orion."""
    def __init__(self, message, line=None, column=None):
        self.message = message
        self.line = line
        self.column = column
        super().__init__(self._format())

    def _format(self):
        pos = f"(line {self.line}, col {self.column}) " if self.line is not None else ""
        return f"[OrionError] {pos}{self.message}"

    def __str__(self):
        return self._format()


# --- Tipos de errores específicos ---
class OrionSyntaxError(OrionError):
    """Errores en parsing/sintaxis."""
    def __init__(self, message, line=None, column=None):
        super().__init__(f"Syntax disruption: {message}", line, column)


class OrionRuntimeError(OrionError):
    """Errores durante ejecución."""
    def __init__(self, message, line=None, column=None):
        super().__init__(f"Runtime fault: {message}", line, column)


class OrionTypeError(OrionError):
    """Errores de tipos."""
    def __init__(self, message, line=None, column=None):
        super().__init__(f"Type mismatch: {message}", line, column)


class OrionNameError(OrionError):
    """Errores de nombres no definidos."""
    def __init__(self, name, line=None, column=None):
        super().__init__(f"Unknown identifier '{name}'", line, column)


class OrionFunctionError(OrionError):
    """Errores relacionados con funciones."""
    def __init__(self, message, line=None, column=None):
        super().__init__(f"Function failure: {message}", line, column)


# --- Utilidad global ---
def raise_orion_error(error_type, message, line=None, column=None):
    """
    Lanza un error de Orion según el tipo pedido.
    Uso:
        raise_orion_error("syntax", "Unexpected token '}'", 3, 15)
    """
    mapping = {
        "syntax": OrionSyntaxError,
        "runtime": OrionRuntimeError,
        "type": OrionTypeError,
        "name": OrionNameError,
        "function": OrionFunctionError,
    }
    error_cls = mapping.get(error_type, OrionError)
    raise error_cls(message, line, column)
