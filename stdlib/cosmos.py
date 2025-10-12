# stdlib/cosmos.py
"""
Orion Cosmos — simulación universal extendida.
Filosofía: "El universo en una línea de código."

Características:
- Cuerpos dinámicos con masa, energía y momentum.
- Simulación de gravedad y órbitas en tiempo real.
- Soporte para “universos persistentes” y seeds deterministas.
- Operaciones declarativas tipo lenguaje:
    cosmos create 10 stars
    cosmos orbit sun earth for 100 steps
    cosmos run universe speed=2
"""

import math
import random
import time
from typing import List, Optional, Dict

# ------------------------------------------------------------
# Núcleo de cuerpos celestes
# ------------------------------------------------------------

class Body:
    """Representa un cuerpo celeste: estrella, planeta, satélite, etc."""
    def __init__(self, name="unknown", mass=1.0, pos=(0, 0, 0), vel=(0, 0, 0)):
        self.name = name
        self.mass = float(mass)
        self.pos = list(map(float, pos))
        self.vel = list(map(float, vel))
        self.energy = 0.0

    def move(self, dt=1.0):
        """Actualiza la posición según su velocidad."""
        self.pos = [p + v * dt for p, v in zip(self.pos, self.vel)]

    def distance_to(self, other: "Body") -> float:
        return math.sqrt(sum((a - b) ** 2 for a, b in zip(self.pos, other.pos)))

    def kinetic_energy(self) -> float:
        return 0.5 * self.mass * sum(v * v for v in self.vel)

    def __repr__(self):
        return f"<{self.name}: pos={tuple(round(x,2) for x in self.pos)}, vel={tuple(round(v,2) for v in self.vel)}>"

# ------------------------------------------------------------
# Física cósmica
# ------------------------------------------------------------

def gravity(b1: Body, b2: Body, G=6.674e-11):
    """Calcula la fuerza gravitacional entre dos cuerpos."""
    dist = b1.distance_to(b2)
    if dist == 0:
        return [0, 0, 0]
    F = G * b1.mass * b2.mass / (dist ** 2)
    direction = [(b2.pos[i] - b1.pos[i]) / dist for i in range(3)]
    return [F * d for d in direction]


def apply_gravity(bodies: List[Body], G=6.674e-11, dt=1.0):
    """Aplica fuerzas gravitacionales entre todos los cuerpos."""
    forces = {b: [0, 0, 0] for b in bodies}
    for i, b1 in enumerate(bodies):
        for j, b2 in enumerate(bodies):
            if i >= j:
                continue
            F = gravity(b1, b2, G)
            for k in range(3):
                forces[b1][k] += F[k]
                forces[b2][k] -= F[k]
    for b in bodies:
        acc = [f / b.mass for f in forces[b]]
        b.vel = [v + a * dt for v, a in zip(b.vel, acc)]
        b.move(dt)
    return bodies


def total_energy(bodies: List[Body], G=6.674e-11):
    """Calcula la energía total del sistema."""
    kinetic = sum(b.kinetic_energy() for b in bodies)
    potential = 0.0
    for i, b1 in enumerate(bodies):
        for j, b2 in enumerate(bodies):
            if i < j:
                r = b1.distance_to(b2)
                potential -= G * b1.mass * b2.mass / r if r != 0 else 0
    return {"kinetic": kinetic, "potential": potential, "total": kinetic + potential}

# ------------------------------------------------------------
# Universo
# ------------------------------------------------------------

class Universe:
    """Representa un universo Orion persistente."""
    def __init__(self, n=5, seed: Optional[int] = None):
        self.seed = seed or int(time.time())
        random.seed(self.seed)
        self.bodies = [random_star() for _ in range(n)]
        self.time = 0.0

    def step(self, dt=1.0):
        """Avanza un paso temporal."""
        apply_gravity(self.bodies, dt=dt)
        self.time += dt
        return self

    def summary(self):
        e = total_energy(self.bodies)
        return {
            "time": round(self.time, 2),
            "bodies": len(self.bodies),
            "energy": e
        }

    def __repr__(self):
        return f"<Universe t={round(self.time,2)} bodies={len(self.bodies)}>"

# ------------------------------------------------------------
# Generadores
# ------------------------------------------------------------

def random_star(name=None):
    """Genera una estrella aleatoria."""
    return Body(
        name or f"Star_{random.randint(1000,9999)}",
        mass=random.uniform(1e20, 1e30),
        pos=[random.uniform(-1e5, 1e5) for _ in range(3)],
        vel=[random.uniform(-10, 10) for _ in range(3)]
    )


def stardust(n=100):
    """Genera una nube de coordenadas aleatorias (polvo estelar)."""
    return [[random.uniform(-1, 1) for _ in range(3)] for _ in range(n)]

# ------------------------------------------------------------
# Interfaz de lenguaje
# ------------------------------------------------------------

_active_universes: Dict[str, Universe] = {}

def cosmos(command="universe", *args, **kwargs):
    """
    Interfaz unificada tipo lenguaje.
    Ejemplos:
        cosmos("create", 10)
        cosmos("orbit", sun, planet)
        cosmos("run", "default", steps=100)
    """
    cmd = command.lower()

    if cmd in ("universe", "create"):
        n = args[0] if args else 5
        name = kwargs.get("name", "default")
        uni = Universe(n)
        _active_universes[name] = uni
        return uni

    if cmd == "run":
        name = args[0] if args else "default"
        steps = kwargs.get("steps", 10)
        dt = kwargs.get("dt", 1.0)
        uni = _active_universes.get(name)
        if not uni:
            return f"Universe '{name}' not found."
        for _ in range(steps):
            uni.step(dt)
        return uni.summary()

    if cmd == "dust":
        n = args[0] if args else 100
        return stardust(n)

    if cmd == "energy":
        name = args[0] if args else "default"
        uni = _active_universes.get(name)
        return total_energy(uni.bodies) if uni else None

    return None

# ------------------------------------------------------------
# Alias y exportación
# ------------------------------------------------------------

ALIASES = {
    "gravity": gravity,
    "energy": total_energy,
    "dust": stardust,
    "create": cosmos,
    "run": cosmos,
    "Body": Body,
    "Universe": Universe,
}

def orion_export():
    exports = {"cosmos": cosmos}
    exports.update(ALIASES)
    return exports

__all__ = list(orion_export().keys())
