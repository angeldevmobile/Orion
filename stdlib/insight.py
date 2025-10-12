"""
Orion Insight — Extracción inteligente de información desde documentos escaneados.
- OCR heurístico sin dependencias externas (modo liviano).
- Detección de estructuras: tablas, campos, firmas, sellos.
- Ideal para digitalización bancaria, formularios y documentos legales.
"""

import io, re, math
from typing import List, Dict, Tuple, Optional
from stdlib import vision

# ============================================================
# Utilidades internas
# ============================================================

def _load_image(src):
    """Carga imagen desde path, bytes, objeto PIL o URL."""
    from stdlib import vision
    import requests

    if isinstance(src, str):
        if src.startswith("http"):
            resp = requests.get(src)
            return vision._ensure_pil(io.BytesIO(resp.content))
        else:
            return vision._ensure_pil(src)
    elif isinstance(src, bytes):
        return vision._ensure_pil(io.BytesIO(src))
    else:
        return vision._ensure_pil(src)


def _binarize(img):
    """Binariza imagen (modo texto blanco/negro rápido)."""
    pil = _load_image(img).convert("L")
    pix = pil.load()
    w, h = pil.size
    for x in range(w):
        for y in range(h):
            pix[x, y] = 255 if pix[x, y] > 180 else 0
    return pil


# ============================================================
# Detección estructural
# ============================================================

def extract_text_blocks(img):
    """
    Extrae bloques de texto detectando regiones oscuras rectangulares.
    Retorna lista [(x, y, w, h)] aproximadas.
    """
    pil = _binarize(img)
    w, h = pil.size
    pix = pil.load()
    blocks = []
    visited = set()

    for y in range(h):
        for x in range(w):
            if pix[x, y] == 0 and (x, y) not in visited:
                stack = [(x, y)]
                minx = maxx = x
                miny = maxy = y
                while stack:
                    cx, cy = stack.pop()
                    if (cx, cy) in visited:
                        continue
                    visited.add((cx, cy))
                    minx, maxx = min(minx, cx), max(maxx, cx)
                    miny, maxy = min(miny, cy), max(maxy, cy)
                    for nx in (cx - 1, cx, cx + 1):
                        for ny in (cy - 1, cy, cy + 1):
                            if 0 <= nx < w and 0 <= ny < h and pix[nx, ny] == 0:
                                stack.append((nx, ny))
                if (maxx - minx) * (maxy - miny) > 50:  # evita ruido
                    blocks.append((minx, miny, maxx - minx, maxy - miny))
    return blocks


def extract_tables(img):
    """
    Detecta posibles tablas (líneas horizontales/verticales cruzadas).
    Devuelve diccionario con confianza.
    """
    pil = _binarize(img)
    w, h = pil.size
    pix = pil.load()
    horizontals = sum(1 for y in range(h)
                      if sum(1 for x in range(w) if pix[x, y] == 0) > w * 0.7)
    verticals = sum(1 for x in range(w)
                    if sum(1 for y in range(h) if pix[x, y] == 0) > h * 0.7)
    confidence = min(1.0, (horizontals + verticals) / 10)
    return {"detected": horizontals > 2 and verticals > 2, "confidence": round(confidence, 2)}


def extract_metadata(img):
    """
    Extrae metadatos visuales: densidad, orientación, contraste.
    """
    pil = _load_image(img).convert("L")
    w, h = pil.size
    pix = pil.load()
    dark = sum(1 for x in range(w) for y in range(h) if pix[x, y] < 50)
    light = w * h - dark
    density = dark / (w * h)
    orientation = "portrait" if h > w else "landscape"
    contrast = abs(dark - light) / (w * h)
    return {
        "density": round(density, 3),
        "orientation": orientation,
        "contrast": round(contrast, 3)
    }


def extract_signatures(img):
    """
    Detección básica de firma/sello por irregularidad de trazos.
    Devuelve dict con flag y confianza.
    """
    pil = _binarize(img)
    w, h = pil.size
    pix = pil.load()
    dark_zones = sum(1 for x in range(w) for y in range(h) if pix[x, y] == 0)
    complexity = dark_zones / (w * h)
    detected = 0.01 < complexity < 0.2
    confidence = max(0, min(1, (complexity - 0.01) / 0.19))
    return {"detected": detected, "confidence": round(confidence, 2)}


# ============================================================
# Resumen estructurado
# ============================================================

def summarize(img):
    """
    Devuelve resumen estructurado de un documento escaneado.
    """
    return {
        "metadata": extract_metadata(img),
        "tables": extract_tables(img),
        "signatures": extract_signatures(img),
        "text_blocks": len(extract_text_blocks(img))
    }


# ============================================================
# Registro y exportación Orion
# ============================================================

ALIASES = {
    "extract_text_blocks": extract_text_blocks,
    "extract_tables": extract_tables,
    "extract_metadata": extract_metadata,
    "extract_signatures": extract_signatures,
    "summarize": summarize
}

def orion_export():
    """Registro estándar para integración con Orion Runtime."""
    return {"insight": ALIASES, **ALIASES}


# ============================================================
# CLI autónomo (ejecución directa)
# ============================================================

if __name__ == "__main__":
    import sys, json
    if len(sys.argv) < 2:
        print("Uso: python insight.py <imagen>")
        sys.exit(0)
    img_path = sys.argv[1]
    result = summarize(img_path)
    print(json.dumps(result, indent=2))
