"""
Módulo string de Orion.
Manipulación de texto moderna, expresiva y veloz.
"""

import re
import base64

# --- Básico ---
def length(s): return len(s)
def upper(s): return s.upper()
def lower(s): return s.lower()
def title(s): return s.title()
def reverse(s): return s[::-1]
def strip(s): return s.strip()

# --- División y unión ---
def split(s, sep=None): return s.split(sep)
def join(lst, sep=" "): return sep.join(lst)

# --- Reemplazos y búsquedas ---
def replace(s, old, new): return s.replace(old, new)
def contains(s, sub): return sub in s
def starts_with(s, sub): return s.startswith(sub)
def ends_with(s, sub): return s.endswith(sub)

# --- Regex (expresiones regulares) ---
def match(pattern, s): return re.match(pattern, s) is not None
def find(pattern, s): return re.findall(pattern, s)
def replace_regex(pattern, repl, s): return re.sub(pattern, repl, s)

# --- Futuristas Orion ---
def pad(s, width, char=" "): return s.ljust(width, char)
def center(s, width, char=" "): return s.center(width, char)

def orbit(s, times=2):
    """Hace orbitar los caracteres (rotación circular)."""
    if not s: return s
    times %= len(s)
    return s[times:] + s[:times]

def mirror(s):
    """Refleja el texto hacia ambos lados."""
    return s + s[::-1]

def glitch(s):
    """Desordena aleatoriamente los caracteres."""
    import random
    arr = list(s)
    random.shuffle(arr)
    return ''.join(arr)

def glow(s):
    """Devuelve texto 'brillante' (estilo futurista visual)."""
    return f"✨{s.upper()}✨"

# --- Codificación útil ---
def encode_base64(s): return base64.b64encode(s.encode()).decode()
def decode_base64(s): return base64.b64decode(s).decode()
