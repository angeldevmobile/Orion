"""
Criptografía moderna en Orion.
Hashes, cifrado simple y firmas futuristas.
"""
import hashlib
import base64

def sha256(data): return hashlib.sha256(data.encode()).hexdigest()
def md5(data): return hashlib.md5(data.encode()).hexdigest()
def b64encode(data): return base64.b64encode(data.encode()).decode()
def b64decode(data): return base64.b64decode(data.encode()).decode(errors="ignore")

# Futurista
def hash_orbit(data, rounds=3):
    """Aplica hash varias veces, como órbitas."""
    h = data.encode()
    for _ in range(rounds):
        h = hashlib.sha256(h).digest()
    return h.hex()
