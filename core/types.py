# core/types.py

import re
import random
from datetime import datetime


# ===============================
# OrionString
# ===============================
class OrionString:
    """String de Orion con interpolación dinámica futurista."""
    INTERP_RE = re.compile(r"\$\{([a-zA-Z_][a-zA-Z0-9_]*)\}")

    def __init__(self, value: str):
        self.value = str(value)

    def __str__(self):
        return self.value

    def __add__(self, other):
        return OrionString(self.value + str(other))

    def interpolate(self, env: dict):
        """Reemplaza ${var} por su valor en el entorno."""
        def repl(m):
            name = m.group(1)
            if name in env:
                val = env[name]
                return str(val)
            return ""  # si no existe, se reemplaza por vacío
        return OrionString(self.INTERP_RE.sub(repl, self.value))

    # --- Métodos adicionales ---
    def futuristic_upper(self):
        return OrionString(self.value.upper())

    def reverse(self):
        return OrionString(self.value[::-1])

    def wave(self):
        res = ''.join(
            c.upper() if i % 2 == 0 else c.lower()
            for i, c in enumerate(self.value)
        )
        return OrionString(res)

    def glitch(self):
        return OrionString(self.value + " ???")


# ===============================
# OrionNumber
# ===============================
class OrionNumber:
    """Número con operaciones extendidas."""
    def __init__(self, value):
        self.value = int(value) if isinstance(value, bool) else value

    def __str__(self):
        return str(self.value)

    def __repr__(self):
        return f"OrionNumber({self.value})"

    # ========= Operadores aritméticos =========
    def __add__(self, other):
        return OrionNumber(self.value + self._unwrap(other))

    def __sub__(self, other):
        return OrionNumber(self.value - self._unwrap(other))

    def __mul__(self, other):
        return OrionNumber(self.value * self._unwrap(other))

    def __truediv__(self, other):
        return OrionNumber(self.value / self._unwrap(other))

    def __floordiv__(self, other):
        return OrionNumber(self.value // self._unwrap(other))

    def __mod__(self, other):
        return OrionNumber(self.value % self._unwrap(other))

    def __pow__(self, other):
        return OrionNumber(self.value ** self._unwrap(other))

    # ========= Comparaciones =========
    def __eq__(self, other):
        return OrionBool(self.value == self._unwrap(other))

    def __ne__(self, other):
        return OrionBool(self.value != self._unwrap(other))

    def __lt__(self, other):
        return OrionBool(self.value < self._unwrap(other))

    def __le__(self, other):
        return OrionBool(self.value <= self._unwrap(other))

    def __gt__(self, other):
        return OrionBool(self.value > self._unwrap(other))

    def __ge__(self, other):
        return OrionBool(self.value >= self._unwrap(other))

    # ========= Helpers =========
    def _unwrap(self, other):
        if isinstance(other, OrionNumber):
            return other.value
        return other

    def unwrap(self):
        """Devuelve el valor Python puro (int/float)."""
        return self.value

    # ========= Métodos especiales Orion =========
    def add(self, other):
        return OrionNumber(self.value + self._unwrap(other))

    def futuristic_power(self, exp):
        """Potencia elevada a otro nivel"""
        return OrionNumber(self.value ** self._unwrap(exp))

    def is_prime(self):
        n = self.value
        if n < 2:
            return OrionBool(False)
        for i in range(2, int(n**0.5) + 1):
            if n % i == 0:
                return OrionBool(False)
        return OrionBool(True)

    def factorial(self):
        n = self.value
        res = 1
        for i in range(1, n+1):
            res *= i
        return OrionNumber(res)

    def to_binary(self):
        return OrionString(bin(self.value)[2:])


# ===============================
# OrionBool
# ===============================
class OrionBool:
    """Booleano futurista con extras."""
    def __init__(self, value: bool):
        self.value = bool(value)

    def __str__(self):
        return "yes" if self.value else "no"

    def toggle(self):
        return OrionBool(not self.value)

    def to_icon(self):
        return "[yes]" if self.value else "[no]"

    def as_number(self):
        return OrionNumber(1 if self.value else 0)

# ===============================
# OrionDate
# ===============================
class OrionDate:
    """Tipo de fecha nativo de Orion."""
    def __init__(self, year, month, day):
        self.date = datetime(year, month, day)

    def __str__(self):
        return self.date.strftime("%Y-%m-%d")

    def futuristic_format(self):
        return self.date.strftime("%d-%m-%Y")

    def to_future(self, years):
        return OrionDate(self.date.year + years, self.date.month, self.date.day)

    def to_past(self, years):
        return OrionDate(self.date.year - years, self.date.month, self.date.day)

    def day_name(self):
        return OrionString(self.date.strftime("%A"))


# ===============================
# OrionList
# ===============================
class OrionList:
    """Lista nativa de Orion con superpoderes."""
    def __init__(self, items):
        # Aseguramos que sea una lista Python interna
        self.items = list(items)

    def __str__(self):
        return "[" + ", ".join(str(i) for i in self.items) + "]"

    def __repr__(self):
        return f"OrionList({self.items})"

    # ===============================
    #   INDEXACIÓN Y OPERACIONES
    # ===============================
    def __getitem__(self, index):
        """Permite acceder con lista[i]"""
        try:
            return self.items[index]
        except IndexError:
            raise IndexError(f"Índice fuera de rango en OrionList ({index})")

    def __setitem__(self, index, value):
        """Permite asignar con lista[i] = valor"""
        if 0 <= index < len(self.items):
            self.items[index] = value
        else:
            raise IndexError(f"No se puede asignar al índice {index} (fuera de rango)")

    def __len__(self):
        """Permite usar len(lista)"""
        return len(self.items)

    def append(self, value):
        """Agrega un elemento al final"""
        self.items.append(value)
        return self

    def extend(self, other):
        """Concatena otra lista Orion o Python"""
        if isinstance(other, OrionList):
            self.items.extend(other.items)
        else:
            self.items.extend(list(other))
        return self

    def pop(self, index=-1):
        """Elimina y devuelve el elemento en posición index"""
        try:
            return self.items.pop(index)
        except IndexError:
            raise IndexError(f"No se puede hacer pop en índice {index} (fuera de rango)")

    # ===============================
    #   FUNCIONES FUNCIONALES
    # ===============================
    def map(self, fn):
        return OrionList([fn(x) for x in self.items])

    def filter(self, fn):
        return OrionList([x for x in self.items if fn(x)])

    def reduce(self, fn, init=None):
        acc = init if init is not None else self.items[0]
        start = 0 if init is not None else 1
        for i in self.items[start:]:
            acc = fn(acc, i)
        return acc

    # ===============================
    #   MÉTODOS ADICIONALES ORION
    # ===============================
    def unique(self):
        """Elimina duplicados conservando orden"""
        return OrionList(list(dict.fromkeys(self.items)))

    def shuffle(self):
        """Devuelve una nueva lista mezclada aleatoriamente"""
        arr = self.items[:]
        random.shuffle(arr)
        return OrionList(arr)

    def chunk(self, n):
        """Divide la lista en sublistas de tamaño n"""
        return OrionList([self.items[i:i+n] for i in range(0, len(self.items), n)])

    def first(self):
        """Devuelve el primer elemento"""
        return self.items[0] if self.items else None

    def last(self):
        """Devuelve el último elemento"""
        return self.items[-1] if self.items else None

# ===============================
# Operador Null-safe
# ===============================
def null_safe(obj, attr):
    """Operador null-safe: si obj es None devuelve None, si no getattr."""
    if obj is None:
        return None
    return getattr(obj, attr, None)
