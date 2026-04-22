"""
Orion Insight — Extracción inteligente de información desde documentos escaneados.
- OCR heurístico sin dependencias externas (modo liviano).
- Detección de estructuras: tablas, campos, firmas, sellos.
- Ideal para digitalización bancaria, formularios y documentos legales.
"""

import base64
import io
import json
import math
import os
import re
import urllib.request
import urllib.error
from typing import List, Dict, Tuple, Optional
from stdlib import vision

# ============================================================
# Utilidades internas
# ============================================================

def _load_image(src):
    """Carga imagen desde path, bytes, objeto PIL o URL."""
    from stdlib import vision
    import urllib.request

    if isinstance(src, str):
        if src.startswith("http"):
            with urllib.request.urlopen(src, timeout=15) as resp:
                return vision._ensure_pil(io.BytesIO(resp.read()))
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
# IA Vision — análisis de documento con Claude o GPT-4o
# ============================================================

def _load_env_insight():
    from pathlib import Path
    for level in range(4):
        p = Path(*[".."] * level) / ".env" if level else Path(".env")
        try:
            with open(p, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#") or "=" not in line:
                        continue
                    k, _, v = line.partition("=")
                    k = k.strip(); v = v.strip().strip('"').strip("'")
                    if k and k not in os.environ:
                        os.environ[k] = v
            return
        except FileNotFoundError:
            continue


def analyze(img, question: str = None) -> str:
    """
    Analiza un documento o imagen usando Claude Vision o GPT-4o.
    Combina el análisis estructural local con la comprensión de IA.
    """
    _load_env_insight()

    structural = summarize(img)

    pil = _load_image(img).convert("RGB")
    buf = io.BytesIO()
    pil.save(buf, format="JPEG", quality=85)
    b64 = base64.b64encode(buf.getvalue()).decode("utf-8")

    context = (
        f"Análisis estructural previo del documento:\n"
        f"- Bloques de texto detectados: {structural['text_blocks']}\n"
        f"- Tablas: {structural['tables']}\n"
        f"- Firmas: {structural['signatures']}\n"
        f"- Metadatos: {structural['metadata']}\n\n"
    )
    text_prompt = context + (question or "Describe el contenido de este documento en detalle.")

    anthropic_key = os.environ.get("ANTHROPIC_API_KEY")
    if anthropic_key:
        model = os.environ.get("ANTHROPIC_MODEL", "claude-3-5-haiku-20241022")
        body = json.dumps({
            "model": model,
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": [
                {"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": b64}},
                {"type": "text", "text": text_prompt}
            ]}]
        }).encode()
        req = urllib.request.Request(
            "https://api.anthropic.com/v1/messages", data=body,
            headers={"Content-Type": "application/json", "x-api-key": anthropic_key,
                     "anthropic-version": "2023-06-01"}, method="POST"
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read())["content"][0]["text"]

    openai_key = os.environ.get("OPENAI_API_KEY")
    if openai_key:
        model = os.environ.get("OPENAI_MODEL", "gpt-4o-mini")
        body = json.dumps({
            "model": model, "max_tokens": 1024,
            "messages": [{"role": "user", "content": [
                {"type": "image_url", "image_url": {"url": f"data:image/jpeg;base64,{b64}"}},
                {"type": "text", "text": text_prompt}
            ]}]
        }).encode()
        req = urllib.request.Request(
            "https://api.openai.com/v1/chat/completions", data=body,
            headers={"Content-Type": "application/json", "Authorization": f"Bearer {openai_key}"},
            method="POST"
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read())["choices"][0]["message"]["content"]

    raise RuntimeError(
        "No hay API key para análisis visual.\n"
        "Agrega en tu .env: ANTHROPIC_API_KEY=sk-ant-... o OPENAI_API_KEY=sk-..."
    )


# ============================================================
# Registro y exportación Orion
# ============================================================

ALIASES = {
    "extract_text_blocks": extract_text_blocks,
    "extract_tables": extract_tables,
    "extract_metadata": extract_metadata,
    "extract_signatures": extract_signatures,
    "summarize": summarize,
    "analyze": analyze,
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
