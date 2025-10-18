# stdlib/timewarp.py
"""
Orion Timewarp — Manipulación avanzada del tiempo.
Extiende el flujo temporal del lenguaje Orion: pausas, rebobinado, viajes y futuros.
"""

import time
import threading
import functools
from datetime import datetime
from typing import Callable

_nanosec = 1e-9
TimeRef = float

def _now() -> TimeRef: return time.time()
def _ns() -> int: return time.time_ns()

# --- WarpClock ------------------------------------------------------------
class WarpClock:
    """Reloj cuántico con control total del flujo temporal."""
    def __init__(self):
        self.start_time = _now()
        self.paused = False
        self.offset = 0.0
        self.scale = 1.0

    def now(self): return self.start_time + (0 if self.paused else (time.time() - self.start_time) * self.scale) + self.offset
    def since_start(self): return self.now() - self.start_time
    def pause(self): 
        if not self.paused: self.offset += (time.time() - self.start_time) * self.scale; self.paused = True
    def resume(self): 
        if self.paused: self.start_time = time.time(); self.paused = False
    def warp(self, scale: float): self.scale = scale
    def speed(self, factor: float): self.scale = factor; return self
    def rewind(self, seconds: float): self.offset -= seconds
    def fastforward(self, seconds: float): self.offset += seconds
    def travel(self, seconds: float): self.offset += seconds  # negativo = pasado
    def reset(self): self.start_time = time.time(); self.offset = 0; self.paused = False
    def timeline(self, name="linked"): return TimeLine(name)

# --- TimeLine -------------------------------------------------------------
class TimeLine:
    """Línea temporal alternativa: ejecuta funciones en pasado, presente o futuro."""
    def __init__(self, name="main"):
        self.name = name
        self.events = []

    def _run_delayed(self, seconds, fn, *a, **kw):
        def delayed():
            if seconds > 0:
                time.sleep(seconds)
            fn(*a, **kw)
        threading.Thread(target=delayed).start()

    def future(self, seconds: float, fn, *a, **kw):
        self._run_delayed(seconds, fn, *a, **kw)
        self.events.append(("future", fn.__name__, seconds))

    def past(self, seconds: float, fn, *a, **kw):
        print(f"[TimeWarp] Reescribiendo pasado {seconds}s → {fn.__name__}")
        fn(*a, **kw)
        self.events.append(("past", fn.__name__, seconds))

    def now(self, fn, *a, **kw):
        fn(*a, **kw)
        self.events.append(("now", fn.__name__, 0))

# --- Decoradores y helpers ------------------------------------------------
def future(delay: float):
    """Ejecuta una función en el futuro."""
    def deco(fn):
        @functools.wraps(fn)
        def wrapper(*a, **kw): threading.Timer(delay, fn, args=a, kwargs=kw).start()
        return wrapper
    return deco

def warp_speed(multiplier: float):
    """Ejecuta una función bajo velocidad temporal alterada."""
    def deco(fn):
        @functools.wraps(fn)
        def wrapper(*a, **kw):
            old_sleep = time.sleep
            time.sleep = lambda s: old_sleep(s / multiplier)
            try: return fn(*a, **kw)
            finally: time.sleep = old_sleep
        return wrapper
    return deco

def wait(duration):
    """Soporta 's', 'ms', 'ns' o número directo."""
    if isinstance(duration, str):
        if duration.endswith("ms"): time.sleep(float(duration[:-2]) / 1000)
        elif duration.endswith("ns"): time.sleep(float(duration[:-2]) * _nanosec)
        elif duration.endswith("s"): time.sleep(float(duration[:-1]))
        else: time.sleep(float(duration))
    else: time.sleep(float(duration))

def measureMtime(fn: Callable, *a, **kw):
    start = _ns()
    result = fn(*a, **kw)
    return {"result": result, "ms": (_ns() - start) / 1e6}

# --- Orion integration ----------------------------------------------------
def run_future_block(delay: float, block: Callable):
    """Permite 'future { ... }' desde Orion."""
    threading.Timer(delay, block).start()
    return f"Future block scheduled in {delay}s"

def run_past_block(delta: float, block: Callable):
    """Permite 'past { ... }' semánticamente."""
    print(f"[TimeWarp] Retrocediendo {delta}s (bloque Orion)")
    block()
    return f"Past block executed with offset {delta}s"

def timewarp(action="clock", *a, **kw):
    """
    Entrada principal para Orion.
      timewarp("clock")
      timewarp("future", 3, fn)
      timewarp("measureMtime", fn)
      timewarp("block_future", delay, fn)
    """
    if action == "clock": return WarpClock()
    if action == "timeline": return TimeLine(kw.get("name", "main"))
    if action == "future": return threading.Timer(a[0], a[1]).start()
    if action == "measureMtime": return measureMtime(a[0])
    if action == "block_future": return run_future_block(a[0], a[1])
    if action == "block_past": return run_past_block(a[0], a[1])
    return None

ALIASES = {
    "WarpClock": WarpClock,
    "TimeLine": TimeLine,
    "future": future,
    "warp_speed": warp_speed,
    "wait": wait,
    "measureMtime": measureMtime,
    "block_future": run_future_block,
    "block_past": run_past_block,
}

def orion_export():
    exports = {"timewarp": timewarp}
    exports.update(ALIASES)
    return exports
