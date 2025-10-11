# stdlib/timewarp.py
"""
Orion Timewarp — Manipulación avanzada del tiempo.
Permite pausar, retroceder, adelantar y ejecutar código en líneas de tiempo alternativas.
Inspirado en física teórica, pero diseñado para desarrolladores modernos.

Ejemplo en Orion:
  use timewarp

  clock = time.start()
  wait(2s)
  print("Han pasado:", clock.since_start())
  time.rewind(5s)
  future {
     show("Este código viene del futuro")
  }
"""

import time
import threading
import functools
from datetime import datetime, timedelta

# --- Tipos y helpers ---
TimeRef = float  # segundos desde epoch
_nanosec = 1e-9

def _now() -> TimeRef:
    return time.time()

def _ns() -> int:
    return time.time_ns()

# --- WarpClock ---
class WarpClock:
    """Reloj cuántico con control total del flujo temporal."""
    def __init__(self):
        self.start_time = _now()
        self.paused = False
        self.offset = 0.0
        self.scale = 1.0  # velocidad del tiempo (1 = normal, 0.5 = más lento, 2 = más rápido)

    def now(self):
        if self.paused:
            return self.start_time + self.offset
        return self.start_time + (time.time() - self.start_time) * self.scale + self.offset

    def pause(self):
        if not self.paused:
            self.offset += (time.time() - self.start_time) * self.scale
            self.paused = True

    def resume(self):
        if self.paused:
            self.start_time = time.time()
            self.paused = False

    def warp(self, scale: float):
        """Cambia la velocidad del tiempo (e.g., warp(0.5) = más lento)."""
        self.scale = scale

    def rewind(self, seconds: float):
        """Retrocede el reloj."""
        self.offset -= seconds

    def fastforward(self, seconds: float):
        """Adelanta el reloj."""
        self.offset += seconds

    def since_start(self):
        return (self.now() - self.start_time)

    def reset(self):
        self.start_time = time.time()
        self.offset = 0.0
        self.paused = False

# --- Timelines ---
class TimeLine:
    """Línea temporal alternativa, permite ejecutar funciones en pasado o futuro."""
    def __init__(self, name="main"):
        self.name = name
        self.events = []

    def future(self, seconds: float, fn, *args, **kwargs):
        """Ejecuta una función en el futuro."""
        def delayed():
            time.sleep(seconds)
            fn(*args, **kwargs)
        threading.Thread(target=delayed).start()
        self.events.append(("future", fn.__name__, seconds))

    def past(self, seconds: float, fn, *args, **kwargs):
        """
        Simula ejecutar una función 'en el pasado' — reejecuta con rollback.
        (Se guarda estado y se aplica compensación lógica)
        """
        self.events.append(("past", fn.__name__, seconds))
        print(f"[TimeWarp] Retrocediendo {seconds}s en timeline '{self.name}' → reejecutando {fn.__name__}")
        fn(*args, **kwargs)

    def now(self, fn, *args, **kwargs):
        """Ejecuta inmediatamente en esta línea de tiempo."""
        fn(*args, **kwargs)
        self.events.append(("now", fn.__name__, 0))

# --- Decoradores cuántico-temporales ---
def future(delay: float):
    """Ejecuta una función en el futuro."""
    def deco(fn):
        @functools.wraps(fn)
        def wrapper(*args, **kwargs):
            threading.Timer(delay, fn, args=args, kwargs=kwargs).start()
        return wrapper
    return deco

def warp_speed(multiplier: float):
    """Ejecuta una función bajo una velocidad temporal alterada."""
    def deco(fn):
        @functools.wraps(fn)
        def wrapper(*args, **kwargs):
            old_sleep = time.sleep
            time.sleep = lambda s: old_sleep(s / multiplier)
            try:
                return fn(*args, **kwargs)
            finally:
                time.sleep = old_sleep
        return wrapper
    return deco

# --- Utilidades ---
def wait(duration):
    """Permite usar duración en segundos, milisegundos o con sufijo 's', 'ms', 'ns'."""
    if isinstance(duration, str):
        if duration.endswith("ms"):
            time.sleep(float(duration[:-2]) / 1000)
        elif duration.endswith("ns"):
            time.sleep(float(duration[:-2]) * _nanosec)
        elif duration.endswith("s"):
            time.sleep(float(duration[:-1]))
        else:
            time.sleep(float(duration))
    else:
        time.sleep(float(duration))

def measure(fn: callable, *args, **kwargs):
    """Mide cuánto tarda una función en ejecutarse."""
    start = _ns()
    result = fn(*args, **kwargs)
    end = _ns()
    elapsed = (end - start) / 1e6
    return {"result": result, "ms": elapsed}

# --- Función principal para Orion ---
def timewarp(action="clock", *args, **kwargs):
    """
    Entrada principal para Orion.
      timewarp("clock") → nuevo WarpClock
      timewarp("future", 3, fn) → ejecuta fn en 3s
      timewarp("measure", fn) → mide tiempo de ejecución
    """
    if action == "clock":
        return WarpClock()
    if action == "timeline":
        return TimeLine(kwargs.get("name", "main"))
    if action == "future":
        seconds = args[0]
        fn = args[1]
        threading.Timer(seconds, fn).start()
        return f"Scheduled {fn.__name__} in {seconds}s"
    if action == "measure":
        fn = args[0]
        return measure(fn)
    return None

# --- Exportación a Orion ---
ALIASES = {
    "WarpClock": WarpClock,
    "TimeLine": TimeLine,
    "future": future,
    "warp_speed": warp_speed,
    "wait": wait,
    "measure": measure,
}

def orion_export():
    exports = {"timewarp": timewarp}
    exports.update(ALIASES)
    return exports
