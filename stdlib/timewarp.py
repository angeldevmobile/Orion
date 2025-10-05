"""
Timewarp: manipulación temporal futurista.
"""
import time

def sleep(ms): time.sleep(ms/1000)
def now(): return time.time()

def warp(seconds):
    """Salta en el tiempo (simulado)."""
    return f"Warped {seconds} seconds ahead!"

def chrono():
    """Devuelve timestamp futurista."""
    return f"⏱ {int(time.time()*1000)}ms"
