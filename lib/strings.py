"""
Utilidades de strings en Orion.
Expresivas, rápidas y con funciones futuristas.
"""

import re

def length(s): return len(s)
def upper(s): return s.upper()
def lower(s): return s.lower()
def title(s): return s.title()
def reverse(s): return s[::-1]

def split(s, sep=None): return s.split(sep)
def join(lst, sep=" "): return sep.join(lst)

def replace(s, old, new): return s.replace(old, new)
def strip(s): return s.strip()
def contains(s, sub): return sub in s

# --- Regex power ---
def match(pattern, s): return re.match(pattern, s) is not None
def find(pattern, s): return re.findall(pattern, s)
def replace_regex(pattern, repl, s): return re.sub(pattern, repl, s)

# --- Futurista ---
def pad(s, width, char=" "):
    """Pad con caracter hasta ancho."""
    return s.ljust(width, char)

def center(s, width, char=" "):
    """Centrar texto."""
    return s.center(width, char)

def orbit(s, times=2):
    """Hace orbitar un string (rotación de chars)."""
    if not s: return s
    times %= len(s)
    return s[times:] + s[:times]
