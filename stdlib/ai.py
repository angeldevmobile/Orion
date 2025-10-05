"""
Módulo AI en Orion.
Mini inteligencia artificial para prototipos rápidos.
"""
from collections import Counter
import random

def predict_next(seq):
    """Predice el próximo elemento basado en frecuencia."""
    if not seq: return None
    return Counter(seq).most_common(1)[0][0]

def sentiment(text):
    """Análisis de sentimiento ultra simple."""
    pos = ["good","yes","happy","love"]
    neg = ["bad","no","sad","hate"]
    score = sum(1 for w in text.split() if w in pos) - sum(1 for w in text.split() if w in neg)
    return "positive" if score>0 else "negative" if score<0 else "neutral"

def ai_choice(options):
    """Elige 'inteligentemente' un elemento (random con sesgo)."""
    return random.choice(options) if options else None
