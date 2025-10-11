# stdlib/crypto.py
"""
Orion Crypto — Módulo de criptografía moderna y simple.
Diseñado para velocidad, simplicidad y seguridad práctica.
"""

import base64
import hashlib
import hmac
import os
import time
import secrets
import uuid

# ------------------------------------------------------------
# 🔒 Utilidades básicas
# ------------------------------------------------------------

def _to_bytes(data):
    if isinstance(data, str):
        return data.encode("utf-8")
    return data

def _to_str(data):
    if isinstance(data, bytes):
        return data.decode("utf-8")
    return data

# ------------------------------------------------------------
# 🔐 Hashing moderno
# ------------------------------------------------------------

def hash(data, algo="sha256", salt=None):
    """Genera un hash seguro, opcionalmente con salt."""
    data_b = _to_bytes(data)
    if salt:
        data_b += _to_bytes(salt)
    if algo not in hashlib.algorithms_available:
        algo = "sha256"
    return hashlib.new(algo, data_b).hexdigest()

# ------------------------------------------------------------
# 🔑 Cifrado simétrico simple (AES-like XOR)
# ------------------------------------------------------------

def _xor_bytes(data_b, key_b):
    """XOR reversible para cifrado rápido (no AES real, pero ligero)."""
    return bytes([b ^ key_b[i % len(key_b)] for i, b in enumerate(data_b)])

def encrypt(data, key=None):
    """Cifra texto con XOR + Base64 usando clave aleatoria si no se pasa una."""
    key = key or secrets.token_hex(8)
    data_b = _to_bytes(data)
    key_b = _to_bytes(hash(key)[:16])
    enc = _xor_bytes(data_b, key_b)
    result = base64.urlsafe_b64encode(enc).decode()
    return {"cipher": result, "key": key}

def decrypt(ciphertext, key):
    """Descifra texto cifrado previamente con encrypt()."""
    enc = base64.urlsafe_b64decode(ciphertext)
    key_b = _to_bytes(hash(key)[:16])
    dec = _xor_bytes(enc, key_b)
    return _to_str(dec)

# ------------------------------------------------------------
# 🧾 Firmas digitales (HMAC)
# ------------------------------------------------------------

def sign(data, key):
    """Genera una firma HMAC-SHA256."""
    return hmac.new(_to_bytes(key), _to_bytes(data), hashlib.sha256).hexdigest()

def verify(data, signature, key):
    """Verifica una firma HMAC-SHA256."""
    expected = sign(data, key)
    return hmac.compare_digest(expected, signature)

# ------------------------------------------------------------
# 🪄 Identificadores únicos y tokens
# ------------------------------------------------------------

def uuid_str():
    """Genera un UUID4 moderno."""
    return str(uuid.uuid4())

def token(length=16, secure=True):
    """Genera un token corto o seguro."""
    if secure:
        return secrets.token_hex(length)
    else:
        return base64.urlsafe_b64encode(os.urandom(length)).decode()[:length]

def entropy(n=64):
    """Devuelve una cadena aleatoria de alta entropía."""
    return hash(os.urandom(n))

# ------------------------------------------------------------
# 🧠 Función de alto nivel Orion
# ------------------------------------------------------------

def crypto(action="uuid", *args):
    """
    Punto de entrada universal de Crypto.
    Ejemplo:
        crypto("hash", "hello")
        crypto("encrypt", "mensaje", "clave")
        crypto("token", 8)
    """
    if action == "hash":
        return hash(*args)
    if action == "encrypt":
        return encrypt(*args)
    if action == "decrypt":
        return decrypt(*args)
    if action == "sign":
        return sign(*args)
    if action == "verify":
        return verify(*args)
    if action == "token":
        return token(*args)
    if action == "entropy":
        return entropy(*args)
    return uuid_str()

# ------------------------------------------------------------
# ⚡ Alias y exportación para Orion Runtime
# ------------------------------------------------------------

ALIASES = {
    "hash": hash,
    "encrypt": encrypt,
    "decrypt": decrypt,
    "sign": sign,
    "verify": verify,
    "uuid": uuid_str,
    "token": token,
    "entropy": entropy,
}

def orion_export():
    exports = {"crypto": crypto}
    exports.update(ALIASES)
    return exports
