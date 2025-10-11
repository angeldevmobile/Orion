# stdlib/cosmos.py
"""
Orion Cosmos — módulo de simulación espacial y física universal.
Filosofía: "El universo en una línea de código."
"""

import math
import random
import time

# ------------------------------------------------------------
# Clases básicas
# ------------------------------------------------------------

class Body:
    """Representa un cuerpo celeste simple."""
    def __init__(self, name="unknown", mass=1.0, pos=(0, 0, 0), vel=(0, 0, 0)):
        self.name = name
        self.mass = mass
        self.pos = list(pos)
        self.vel = list(vel)

    def move(self, dt=1.0):
        """Actualiza la posición del cuerpo."""
        self.pos = [p + v * dt for p, v in zip(self.pos, self.vel)]

    def distance_to(self, other):
        """Distancia euclidiana entre dos cuerpos."""
        return math.sqrt(sum((a - b) ** 2 for a, b in zip(self.pos, other.pos)))

    def __repr__(self):
        return f"<Body {self.name}: pos={self.pos}, vel={self.vel}>"

# ------------------------------------------------------------
# Simulación básica
# ------------------------------------------------------------

def gravity(b1, b2, G=6.674e-11):
    """Calcula la fuerza gravitacional entre dos cuerpos."""
    dist = b1.distance_to(b2)
    if dist == 0:
        return [0, 0, 0]
    F = G * b1.mass * b2.mass / (dist ** 2)
    direction = [(b2.pos[i] - b1.pos[i]) / dist for i in range(3)]
    return [F * d for d in direction]

def step_system(bodies, dt=1.0):
    """Simula un paso de movimiento simple."""
    for b in bodies:
        b.move(dt)
    return bodies

def random_star(name=None):
    """Genera una estrella aleatoria."""
    return Body(
        name or f"Star_{random.randint(1000,9999)}",
        mass=random.uniform(1e20, 1e30),
        pos=[random.uniform(-1e5, 1e5) for _ in range(3)],
        vel=[random.uniform(-10, 10) for _ in range(3)]
    )

def universe(n=5):
    """Crea un universo pequeño con n estrellas."""
    return [random_star() for _ in range(n)]

# ------------------------------------------------------------
# Funciones matemáticas espaciales
# ------------------------------------------------------------

def orbit(center, satellite, G=6.674e-11, dt=1.0):
    """Simula una órbita simple de un satélite alrededor de un centro."""
    force = gravity(center, satellite, G)
    acc = [f / satellite.mass for f in force]
    satellite.vel = [v + a * dt for v, a in zip(satellite.vel, acc)]
    satellite.move(dt)
    return satellite.pos

def cosmic_distance(a, b):
    """Distancia directa entre dos puntos o cuerpos."""
    if isinstance(a, Body) and isinstance(b, Body):
        return a.distance_to(b)
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))

def stardust(n=100):
    """Genera una nube de polvo estelar (coordenadas aleatorias)."""
    return [[random.uniform(-1, 1) for _ in range(3)] for _ in range(n)]

# ------------------------------------------------------------
# Función de alto nivel Orion
# ------------------------------------------------------------

def cosmos(action="universe", *args):
    """
    Punto de entrada unificado de Cosmos.
    Ejemplo:
        cosmos("universe", 10)
        cosmos("orbit", sun, planet)
        cosmos("dust", 500)
    """
    if action == "universe":
        n = args[0] if args else 5
        return universe(n)
    if action == "dust":
        n = args[0] if args else 100
        return stardust(n)
    if action == "orbit":
        if len(args) >= 2:
            return orbit(args[0], args[1])
    return None

# ------------------------------------------------------------
# Alias cortos y exportación
# ------------------------------------------------------------

ALIASES = {
    "gravity": gravity,
    "orbit": orbit,
    "universe": universe,
    "dust": stardust,
    "dist": cosmic_distance,
    "star": random_star,
}

def orion_export():
    exports = {"cosmos": cosmos, "Body": Body}
    exports.update(ALIASES)
    return exports
