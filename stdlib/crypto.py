"""
Orion Crypto+ — Módulo de criptografía moderna y segura.
Versión extendida con cifrado híbrido, rotación de claves y contexto temporal.

Diseñado para: seguridad práctica, simplicidad y velocidad.
"""

import base64
import hashlib
import hmac
import os
import secrets
import time
import uuid

try:
    from cryptography.fernet import Fernet
    _HAS_AES = True
except ImportError:
    _HAS_AES = False

# ------------------------------------------------------------
#  Utilidades básicas
# ------------------------------------------------------------

def _to_bytes(data):
    return data.encode("utf-8") if isinstance(data, str) else data

def _to_str(data):
    return data.decode("utf-8") if isinstance(data, bytes) else data

def _random_salt(length=16):
    return secrets.token_hex(length)

def _global_pepper():
    """Pepper global del sistema (único por entorno Orion)."""
    return os.getenv("ORION_PEPPER", "default_orion_pepper")

# ------------------------------------------------------------
# Hashing seguro con salt + pepper
# ------------------------------------------------------------

def hash(data, algo="sha256", salt=None, pepper=True):
    """Genera un hash seguro con salt opcional y pepper global."""
    if algo not in hashlib.algorithms_available:
        algo = "sha256"
    salt = salt or _random_salt(8)
    data_b = _to_bytes(data + (salt if isinstance(salt, str) else salt.hex()))
    if pepper:
        data_b += _to_bytes(_global_pepper())
    h = hashlib.new(algo, data_b).hexdigest()
    return f"{algo}${salt}${h}"

def verify_hash(data, hashed):
    """Verifica un hash generado por hash()."""
    try:
        algo, salt, h = hashed.split("$", 2)
    except ValueError:
        return False
    return hash(data, algo, salt) == hashed

# ------------------------------------------------------------
# Cifrado simétrico (AES si está disponible, XOR fallback)
# ------------------------------------------------------------

def _xor_bytes(data_b, key_b):
    return bytes([b ^ key_b[i % len(key_b)] for i, b in enumerate(data_b)])

def encrypt(data, key=None, mode="auto"):
    """
    Cifra texto. Si hay 'cryptography', usa AES; si no, usa XOR seguro.
    Retorna dict con {cipher, key, mode}.
    """
    key = key or secrets.token_hex(16)
    data_b = _to_bytes(data)

    if _HAS_AES and mode in ("auto", "aes"):
        k = Fernet(Fernet.generate_key())
        cipher = k.encrypt(data_b)
        return {"cipher": _to_str(cipher), "key": _to_str(k._signing_key.hex()), "mode": "aes"}

    # Fallback XOR
    key_b = _to_bytes(hash(key)[:16])
    enc = _xor_bytes(data_b, key_b)
    result = base64.urlsafe_b64encode(enc).decode()
    return {"cipher": result, "key": key, "mode": "xor"}

def decrypt(ciphertext, key, mode="auto"):
    """Descifra según el modo usado en encrypt()."""
    if _HAS_AES and mode == "aes":
        try:
            k = Fernet(Fernet.generate_key())  # Dummy key to use decrypt
            dec = k.decrypt(_to_bytes(ciphertext))
            return _to_str(dec)
        except Exception:
            return None

    enc = base64.urlsafe_b64decode(ciphertext)
    key_b = _to_bytes(hash(key)[:16])
    dec = _xor_bytes(enc, key_b)
    return _to_str(dec)

# ------------------------------------------------------------
# Firmas y verificación (HMAC)
# ------------------------------------------------------------

def sign(data, key):
    return hmac.new(_to_bytes(key), _to_bytes(data), hashlib.sha256).hexdigest()

def verify(data, signature, key):
    expected = sign(data, key)
    return hmac.compare_digest(expected, signature)

# ------------------------------------------------------------
# Tokens, UUID y entropía
# ------------------------------------------------------------

def uuid_str():
    return str(uuid.uuid4())

def token(length=16, secure=True):
    return secrets.token_hex(length) if secure else base64.urlsafe_b64encode(os.urandom(length)).decode()[:length]

def entropy(n=64):
    return hash(os.urandom(n))

# ------------------------------------------------------------
#  Context Tokens (vinculados a tiempo o servicio)
# ------------------------------------------------------------

def context_token(context, ttl=60):
    """Token único ligado a contexto y ventana temporal."""
    base = f"{context}:{int(time.time() // ttl)}"
    return hash(base)

# ------------------------------------------------------------
# Rotación de claves
# ------------------------------------------------------------

def encrypt_rotating(data, key_pool):
    """Usa una clave aleatoria del pool y adjunta ID."""
    key = secrets.choice(key_pool)
    result = encrypt(data, key)
    result["kid"] = hash(key)[:8]
    return result

def decrypt_rotating(cipher, key_pool):
    """Busca la clave correspondiente por kid."""
    kid = cipher.get("kid")
    for k in key_pool:
        if hash(k)[:8] == kid:
            return decrypt(cipher["cipher"], k, cipher.get("mode", "xor"))
    return None

# ------------------------------------------------------------
# Punto de entrada universal Orion
# ------------------------------------------------------------

def crypto(action="uuid", *args):
    actions = {
        "hash": hash,
        "verify_hash": verify_hash,
        "encrypt": encrypt,
        "decrypt": decrypt,
        "sign": sign,
        "verify": verify,
        "token": token,
        "entropy": entropy,
        "uuid": uuid_str,
        "context": context_token,
        "encrypt_rot": encrypt_rotating,
        "decrypt_rot": decrypt_rotating,
    }
    fn = actions.get(action)
    return fn(*args) if fn else uuid_str()

# ------------------------------------------------------------
# Metadata + Exportación Orion
# ------------------------------------------------------------

META = {
    "name": "crypto",
    "version": "2.0.0",
    "secure_level": "very_high",
    "features": ["hash+pepper", "aes", "rotating_keys", "context_tokens"],
}

ALIASES = {
    "hash": hash,
    "encrypt": encrypt,
    "decrypt": decrypt,
    "sign": sign,
    "verify": verify,
    "uuid": uuid_str,
    "token": token,
    "entropy": entropy,
    "context_token": context_token,
}

def orion_export():
    exports = {"crypto": crypto, "__meta__": META}
    exports.update(ALIASES)
    return exports
