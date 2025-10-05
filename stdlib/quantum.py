"""
Quantum: generador de aleatoriedad cuántica.
"""
import random, os

def qrand():
    """Número cuántico (0-1)."""
    return random.random() * os.urandom(1)[0] / 255

def qbit():
    """Devuelve '0' o '1' aleatorio como bit cuántico."""
    return random.choice([0,1])
