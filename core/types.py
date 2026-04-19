# core/types.py

import re
import random
from datetime import datetime


# ===============================
# OrionDate
# ===============================
class OrionDate:
    """Tipo de fecha nativo de Orion."""
    def __init__(self, year, month, day):
        self.date = datetime(year, month, day)

    def __str__(self):
        return self.date.strftime("%Y-%m-%d")

    def __repr__(self):
        return f"OrionDate({self.date.year}, {self.date.month}, {self.date.day})"

    # ========= Operadores de comparación =========
    def __eq__(self, other):
        if isinstance(other, OrionDate):
            return OrionBool(self.date == other.date)
        elif isinstance(other, datetime):
            return OrionBool(self.date.date() == other.date())
        return OrionBool(False)

    def __ne__(self, other):
        return OrionBool(not self.__eq__(other).value)

    def __lt__(self, other):
        if isinstance(other, OrionDate):
            return OrionBool(self.date < other.date)
        elif isinstance(other, datetime):
            return OrionBool(self.date < other)
        return OrionBool(False)

    def __le__(self, other):
        if isinstance(other, OrionDate):
            return OrionBool(self.date <= other.date)
        elif isinstance(other, datetime):
            return OrionBool(self.date <= other)
        return OrionBool(False)

    def __gt__(self, other):
        if isinstance(other, OrionDate):
            return OrionBool(self.date > other.date)
        elif isinstance(other, datetime):
            return OrionBool(self.date > other)
        return OrionBool(False)

    def __ge__(self, other):
        if isinstance(other, OrionDate):
            return OrionBool(self.date >= other.date)
        elif isinstance(other, datetime):
            return OrionBool(self.date >= other)
        return OrionBool(False)

    # ========= Métodos adicionales =========
    def futuristic_format(self):
        return OrionString(self.date.strftime("%d-%m-%Y"))

    def to_future(self, years):
        return OrionDate(self.date.year + years, self.date.month, self.date.day)

    def to_past(self, years):
        return OrionDate(self.date.year - years, self.date.month, self.date.day)

    def day_name(self):
        return OrionString(self.date.strftime("%A"))

    def month_name(self):
        return OrionString(self.date.strftime("%B"))

    def year(self):
        return OrionNumber(self.date.year)

    def month(self):
        return OrionNumber(self.date.month)

    def day(self):
        return OrionNumber(self.date.day)

    def weekday(self):
        """Devuelve el día de la semana (0=lunes, 6=domingo)"""
        return OrionNumber(self.date.weekday())

    def add_days(self, days):
        """Agrega días a la fecha"""
        from datetime import timedelta
        new_date = self.date + timedelta(days=days)
        return OrionDate(new_date.year, new_date.month, new_date.day)

    def subtract_days(self, days):
        """Resta días a la fecha"""
        from datetime import timedelta
        new_date = self.date - timedelta(days=days)
        return OrionDate(new_date.year, new_date.month, new_date.day)

    def days_until(self, other_date):
        """Calcula días hasta otra fecha"""
        if isinstance(other_date, OrionDate):
            delta = other_date.date - self.date
            return OrionNumber(delta.days)
        return OrionNumber(0)

    def is_weekend(self):
        """Verifica si es fin de semana"""
        return OrionBool(self.date.weekday() >= 5)

    def is_today(self):
        """Verifica si es hoy"""
        from datetime import date
        return OrionBool(self.date.date() == date.today())
    
# ===============================
# OrionBool
# ===============================
class OrionBool:
    """Booleano futurista con extras."""
    def __init__(self, value: bool):
        self.value = bool(value)

    def __str__(self):
        return "yes" if self.value else "no"

    def __repr__(self):
        return f"OrionBool({self.value})"

    # ========= Operadores de comparación =========
    def __eq__(self, other):
        if isinstance(other, OrionBool):
            return OrionBool(self.value == other.value)
        elif isinstance(other, bool):
            return OrionBool(self.value == other)
        elif isinstance(other, str):
            # Permitir comparación con strings para compatibilidad con type()
            return OrionBool(str(self) == other)
        return OrionBool(False)

    def __ne__(self, other):
        return OrionBool(not self.__eq__(other).value)

    def __bool__(self):
        """Permite usar el objeto en contextos booleanos (if, while, etc.)"""
        return self.value

    # ========= Métodos adicionales =========
    def toggle(self):
        return OrionBool(not self.value)

    def to_icon(self):
        return "[yes]" if self.value else "[no]"

    def as_number(self):
        return OrionNumber(1 if self.value else 0)


# ===============================
# OrionNumber
# ===============================
class OrionNumber:
    """Número con operaciones extendidas."""
    def __init__(self, value):
        # Desempaqueta OrionNumber anidados (varias capas)
        while isinstance(value, OrionNumber):
            value = value.value
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

    # ========= Conversión a int/float (para indexado y aritmética Python) =========
    def __int__(self):
        return int(self.value)

    def __float__(self):
        return float(self.value)

    def __index__(self):
        """Permite usar OrionNumber como índice en listas Python."""
        return int(self.value)

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
# OrionString
# ===============================
class OrionString:
    """String de Orion con interpolación dinámica futurista."""
    # Patrón actualizado para manejar acceso a atributos como ${obj.prop}
    INTERP_RE = re.compile(r"\$\{([a-zA-Z_][a-zA-Z0-9_.]*)\}")

    def __init__(self, value: str):
        self.value = str(value)

    def __str__(self):
        return self.value

    def __repr__(self):
        return f"OrionString('{self.value}')"

    def __add__(self, other):
        return OrionString(self.value + str(other))

    # ========= Operadores de comparación =========
    def __eq__(self, other):
        if isinstance(other, OrionString):
            return OrionBool(self.value == other.value)
        elif isinstance(other, str):
            return OrionBool(self.value == other)
        return OrionBool(False)

    def __ne__(self, other):
        return OrionBool(not self.__eq__(other).value)

    def __lt__(self, other):
        other_val = other.value if isinstance(other, OrionString) else str(other)
        return OrionBool(self.value < other_val)

    def __le__(self, other):
        other_val = other.value if isinstance(other, OrionString) else str(other)
        return OrionBool(self.value <= other_val)

    def __gt__(self, other):
        other_val = other.value if isinstance(other, OrionString) else str(other)
        return OrionBool(self.value > other_val)

    def __ge__(self, other):
        other_val = other.value if isinstance(other, OrionString) else str(other)
        return OrionBool(self.value >= other_val)

    # ========= Métodos existentes =========
    def interpolate(self, env: dict):
        """Reemplaza ${var} o ${obj.prop} por su valor en el entorno."""
        def repl(m):
            expr = m.group(1)
            
            # Si contiene punto, es acceso a propiedad
            if '.' in expr:
                parts = expr.split('.')
                obj_name = parts[0]
                
                if obj_name in env:
                    obj = env[obj_name]
                    # Navegar por las propiedades
                    for prop in parts[1:]:
                        if hasattr(obj, prop):
                            obj = getattr(obj, prop)
                        elif isinstance(obj, dict) and prop in obj:
                            obj = obj[prop]
                        else:
                            return f"${{{expr}}}"  # Retorna original si no se encuentra
                    return str(obj)
                else:
                    return f"${{{expr}}}"  # Retorna original si no se encuentra
            else:
                # Variable simple
                if expr in env:
                    val = env[expr]
                    return str(val)
                return f"${{{expr}}}"  # Retorna original si no se encuentra
                
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

    # ========= Operadores de comparación =========
    def __eq__(self, other):
        if isinstance(other, OrionList):
            return OrionBool(self.items == other.items)
        elif isinstance(other, list):
            return OrionBool(self.items == other)
        elif isinstance(other, str):
            # Permitir comparación con strings para compatibilidad con type()
            return OrionBool("list" == other)
        return OrionBool(False)

    def __ne__(self, other):
        return OrionBool(not self.__eq__(other).value)

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
# OrionDict
# ===============================
class OrionDict:
    """Diccionario nativo de Orion con semántica futurista."""
    def __init__(self, value=None):
        # Acepta tanto un dict como None
        self.value = dict(value or {})

    def __str__(self):
        return "{" + ", ".join(f"{k}: {v}" for k, v in self.value.items()) + "}"

    def __repr__(self):
        return f"OrionDict({self.value})"

    # ===============================
    #   ACCESO Y MODIFICACIÓN
    # ===============================
    def __getitem__(self, key):
        try:
            return self.value[key]
        except KeyError:
            raise KeyError(f"Clave '{key}' no encontrada en OrionDict")

    def __setitem__(self, key, val):
        self.value[key] = val

    def __delitem__(self, key):
        if key in self.value:
            del self.value[key]
        else:
            raise KeyError(f"No se puede eliminar clave inexistente '{key}'")

    def __len__(self):
        return len(self.value)

    def keys(self):
        return list(self.value.keys())

    def values(self):
        return list(self.value.values())

    def items(self):
        return list(self.value.items())

    def has(self, key):
        """Verifica si la clave existe."""
        return OrionBool(key in self.value)

    def merge(self, other):
        """Fusiona con otro dict o OrionDict."""
        new = self.value.copy()
        if isinstance(other, OrionDict):
            new.update(other.value)
        else:
            new.update(other)
        return OrionDict(new)

    def clone(self):
        """Copia profunda."""
        return OrionDict(self.value.copy())

    def remove(self, key):
        """Elimina una clave si existe."""
        if key in self.value:
            del self.value[key]
        return self

    def clear(self):
        """Limpia todas las claves."""
        self.value.clear()
        return self

    def map(self, fn):
        """Aplica una función a cada (clave, valor)."""
        return OrionList([fn(k, v) for k, v in self.value.items()])


# ===============================
# Operador Null-safe
# ===============================
def null_safe(obj, attr):
    """Operador null-safe: si obj es None devuelve None, si no getattr."""
    if obj is None:
        return None
    return getattr(obj, attr, None)
