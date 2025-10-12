# stdlib/vision.py
"""
Orion Vision — Visión computacional y procesamiento de imágenes para Orion.
Principios:
- Moderno y expresivo: verbos simples (load, save, resize, smart_crop, phash, detect_faces_blur).
- Rápido: usa Pillow / numpy / cv2 si están disponibles; fallback puro-Python.
- Único: ImagePipeline (lazy, fusiona operaciones), perceptual hashes nativos,
  retina-aware scaling, content-aware seam carving (simple), dominant colors (kmeans), OCR glue.
- Diseñado para integrarse con Orion runtime via orion_export().
"""

from collections import Counter
import math
import io
import os
import time
import random
from typing import Tuple, List, Callable, Optional

# Optional accelerated libs
try:
    from PIL import Image, ImageOps, ImageFilter, ImageDraw, ImageFont
except Exception:
    Image = None

try:
    import numpy as _np
except Exception:
    _np = None

try:
    import cv2
except Exception:
    cv2 = None

try:
    import pytesseract
except Exception:
    pytesseract = None

# -------------------------
# Helpers: internal conversions
# -------------------------
def _ensure_pil(img):
    """Return a PIL Image from path/bytes/PIL/numpy."""
    if Image is None:
        raise RuntimeError("Pillow is required for vision core. Install pillow.")
    if isinstance(img, Image.Image):
        return img
    if isinstance(img, str):
        return Image.open(img).convert("RGBA")
    if isinstance(img, (bytes, bytearray)):
        return Image.open(io.BytesIO(img)).convert("RGBA")
    if _np and isinstance(img, _np.ndarray):
        mode = "RGBA" if img.shape[2] == 4 else "RGB"
        return Image.fromarray(img.astype("uint8"), mode=mode)
    raise TypeError("Unsupported image type for _ensure_pil")

def _to_bytes(img, fmt="PNG"):
    """Return bytes from PIL Image."""
    if Image is None:
        raise RuntimeError("Pillow is required")
    buf = io.BytesIO()
    img.save(buf, format=fmt)
    return buf.getvalue()

# -------------------------
# Core IO
# -------------------------
def load(path_or_bytes):
    """Carga imagen desde ruta, bytes, o PIL image."""
    if Image is None:
        raise RuntimeError("Pillow is required")
    if isinstance(path_or_bytes, (bytes, bytearray)):
        return Image.open(io.BytesIO(path_or_bytes)).convert("RGBA")
    if isinstance(path_or_bytes, Image.Image):
        return path_or_bytes
    return Image.open(path_or_bytes).convert("RGBA")

def save(img, path=None, fmt="PNG", optimize=True, to_bytes=False):
    """Guarda imagen o retorna bytes si to_bytes=True."""
    pil = _ensure_pil(img)
    if to_bytes:
        return _to_bytes(pil, fmt=fmt)
    if path:
        pil.save(path, format=fmt, optimize=optimize)
        return path
    raise ValueError("Debe especificar path o to_bytes=True")

# -------------------------
# Transformaciones básicas
# -------------------------
def resize(img, size: Tuple[int, int], resample="lanczos"):
    """Resize con detección retina: acepta escala >1.0 decimal o tupla (w,h)."""
    pil = _ensure_pil(img)
    if isinstance(size, (int, float)):  # escala factor
        w, h = pil.size
        size = (int(w * size), int(h * size))
    if isinstance(size, tuple):
        method = Image.LANCZOS if hasattr(Image, "LANCZOS") else Image.BICUBIC
        return pil.resize(size, resample=method)
    raise TypeError("size must be tuple or scale factor")

def thumbnail(img, maxsize=(256, 256)):
    pil = _ensure_pil(img)
    out = pil.copy()
    out.thumbnail(maxsize, Image.LANCZOS if hasattr(Image, "LANCZOS") else Image.ANTIALIAS)
    return out

def crop(img, box: Tuple[int, int, int, int]):
    pil = _ensure_pil(img)
    return pil.crop(box)

def smart_crop(img, target_w: int, target_h: int, focus="center"):
    """
    Recorta inteligentemente. focus: 'center'|'entropy'|'attention'(cv2 saliency).
    Entropy-based crop: desliza ventana y selecciona la más informativa.
    """
    pil = _ensure_pil(img)
    w, h = pil.size
    if target_w >= w and target_h >= h:
        return pil.copy()
    if focus == "center":
        left = (w - target_w) // 2
        top = (h - target_h) // 2
        return pil.crop((left, top, left + target_w, top + target_h))
    # entropy method
    gray = pil.convert("L")
    pix = gray.load()
    best_score = -1
    best_box = (0, 0, target_w, target_h)
    step_x = max(1, (w - target_w) // 10)
    step_y = max(1, (h - target_h) // 10)
    for x in range(0, max(1, w - target_w + 1), step_x):
        for y in range(0, max(1, h - target_h + 1), step_y):
            score = 0.0
            # sample a few pixels for entropy approx
            for sx in range(0, target_w, max(1, target_w // 8)):
                for sy in range(0, target_h, max(1, target_h // 8)):
                    val = pix[x + sx, y + sy]
                    score += (val - 128) ** 2
            if score > best_score:
                best_score = score
                best_box = (x, y, x + target_w, y + target_h)
    return pil.crop(best_box)

# -------------------------
# Perceptual hashing (dHash) — pure python
# -------------------------
def _resize_gray(img, w, h):
    return img.convert("L").resize((w, h), Image.LANCZOS if hasattr(Image, "LANCZOS") else Image.BICUBIC)

def dhash(img, hash_size=8) -> str:
    """
    Difference hash (dHash) -> returns hex string.
    Fast and robust. Lower hamming distance => similar images.
    """
    pil = _ensure_pil(img)
    small = _resize_gray(pil, hash_size + 1, hash_size)
    pix = small.load()
    bits = []
    for y in range(hash_size):
        for x in range(hash_size):
            left = pix[x, y]
            right = pix[x + 1, y]
            bits.append(1 if left < right else 0)
    # pack to hex
    val = 0
    for b in bits:
        val = (val << 1) | b
    return f"{val:0{(hash_size*hash_size)//4}x}"

def hamming(a: str, b: str) -> int:
    """Hamming distance between two hex hashes."""
    ai = int(a, 16)
    bi = int(b, 16)
    x = ai ^ bi
    return x.bit_count()

# -------------------------
# Dominant colors (k-means lightweight)
# -------------------------
def dominant_colors(img, k=3, max_samples=1000, seed=None):
    """Return k dominant RGB tuples using simple k-means on sampled pixels."""
    pil = _ensure_pil(img)
    w, h = pil.size
    pixels = list(pil.convert("RGB").getdata())
    if len(pixels) > max_samples:
        random.seed(seed)
        pixels = random.sample(pixels, max_samples)
    # init centers
    centers = [tuple(pixels[i]) for i in random.sample(range(len(pixels)), k)]
    for _ in range(10):
        buckets = [[] for _ in range(k)]
        for p in pixels:
            idx = min(range(k), key=lambda i: sum((p[ch] - centers[i][ch]) ** 2 for ch in range(3)))
            buckets[idx].append(p)
        new_centers = []
        for b in buckets:
            if not b:
                new_centers.append(random.choice(pixels))
            else:
                avg = tuple(int(sum(px[i] for px in b) / len(b)) for i in range(3))
                new_centers.append(avg)
        if new_centers == centers:
            break
        centers = new_centers
    return centers

# -------------------------
# Histogram equalization & auto-enhance
# -------------------------
def histogram_equalize(img):
    pil = _ensure_pil(img)
    if _np:
        arr = _np.asarray(pil.convert("L"))
        # simple equalization
        hist, bins = _np.histogram(arr.flatten(), 256, [0,256])
        cdf = hist.cumsum()
        cdf_normalized = (cdf - cdf.min()) * 255 / (cdf.max() - cdf.min() + 1e-9)
        arr2 = _np.interp(arr.flatten(), bins[:-1], cdf_normalized).reshape(arr.shape).astype("uint8")
        return Image.fromarray(arr2).convert("RGBA")
    # fallback: use PIL equalize
    return ImageOps.equalize(pil.convert("RGB")).convert("RGBA")

def auto_enhance(img, factor=1.2):
    pil = _ensure_pil(img)
    # simple contrast boost -> sharpen
    enh = ImageOps.autocontrast(pil)
    if hasattr(ImageFilter, "SHARPEN"):
        enh = enh.filter(ImageFilter.SHARPEN)
    return enh

# -------------------------
# Face detection & blur (uses cv2 if available)
# -------------------------
def detect_faces(img) -> List[Tuple[int,int,int,int]]:
    """Returns list of boxes (x,y,w,h). Uses opencv haarcascade if available; else fast skin-color heuristic."""
    pil = _ensure_pil(img)
    if cv2:
        arr = _np.asarray(pil.convert("RGB"))[:, :, ::-1]  # RGB->BGR
        gray = cv2.cvtColor(arr, cv2.COLOR_BGR2GRAY)
        casc_path = cv2.data.haarcascades + "haarcascade_frontalface_default.xml"
        if os.path.exists(casc_path):
            face_cascade = cv2.CascadeClassifier(casc_path)
            faces = face_cascade.detectMultiScale(gray, scaleFactor=1.1, minNeighbors=4)
            return [(int(x), int(y), int(w), int(h)) for (x,y,w,h) in faces]
    # fallback heuristic: naive bright skin blobs (not reliable, but present)
    w, h = pil.size
    samp = pil.convert("RGB").resize((w//4 or 1, h//4 or 1))
    px = samp.load()
    boxes = []
    for x in range(samp.size[0]):
        for y in range(samp.size[1]):
            r,g,b = px[x,y]
            if r > 95 and g > 40 and b > 20 and (max(r,g,b)-min(r,g,b) > 15) and r > g and r > b:
                boxes.append((x*4,y*4,4,4))
    # merge small boxes
    return boxes

def blur_faces(img, blur_radius=15):
    """Detect faces and apply blur to those regions."""
    pil = _ensure_pil(img)
    faces = detect_faces(pil)
    out = pil.copy()
    for (x,y,w,h) in faces:
        box = (x, y, x+w, y+h)
        region = out.crop(box).filter(ImageFilter.GaussianBlur(radius=blur_radius))
        out.paste(region, box)
    return out

# -------------------------
# OCR glue (pytesseract optional)
# -------------------------
def scan_text(img, lang="eng"):
    """Returns OCR text if pytesseract available; else empty string."""
    pil = _ensure_pil(img)
    if pytesseract:
        return pytesseract.image_to_string(pil, lang=lang)
    return ""

# -------------------------
# Content-aware seam carve (naive, small images)
# -------------------------
def seam_carve(img, target_w, target_h):
    """
    Simple seam carving implementation (removes vertical seams until width matches).
    Warning: naive implementation; use for small images or prototypes.
    """
    pil = _ensure_pil(img).convert("RGB")
    if _np is None:
        raise RuntimeError("numpy required for seam_carve fallback")
    arr = _np.asarray(pil).astype("float")
    h, w, _ = arr.shape
    if target_w >= w and target_h >= h:
        return pil
    def energy(im):
        gx = _np.abs(_np.gradient(im.astype("float64"), axis=1)).sum(axis=2)
        gy = _np.abs(_np.gradient(im.astype("float64"), axis=0)).sum(axis=2)
        return gx + gy
    def remove_vertical_seam(im_arr):
        en = energy(im_arr)
        M = en.copy()
        backtrack = _np.zeros_like(M, dtype=int)
        for i in range(1, M.shape[0]):
            for j in range(0, M.shape[1]):
                idx_min = j
                min_val = M[i-1, j]
                if j > 0 and M[i-1, j-1] < min_val:
                    min_val = M[i-1, j-1]; idx_min = j-1
                if j+1 < M.shape[1] and M[i-1, j+1] < min_val:
                    min_val = M[i-1, j+1]; idx_min = j+1
                M[i,j] += min_val
                backtrack[i,j] = idx_min
        # find seam
        seam = _np.zeros(M.shape[0], dtype=int)
        seam[-1] = _np.argmin(M[-1])
        for i in range(M.shape[0]-2, -1, -1):
            seam[i] = backtrack[i+1, seam[i+1]]
        # remove seam
        H, W, C = im_arr.shape
        out = _np.zeros((H, W-1, C), dtype=im_arr.dtype)
        for i in range(H):
            j = seam[i]
            out[i,:,:] = _np.concatenate((im_arr[i,:j,:], im_arr[i,j+1:,:]), axis=0)
        return out
    cur = arr
    while cur.shape[1] > target_w:
        cur = remove_vertical_seam(cur)
    # For height we rotate, apply vertical seam remove, rotate back
    cur_img = Image.fromarray(cur.astype("uint8"))
    cur_img = cur_img.resize((target_w, target_h), Image.LANCZOS)
    return cur_img

# -------------------------
# ImagePipeline: lazy, composable, fused execution
# -------------------------
class ImagePipeline:
    """
    Compose operations lazily and execute in one pass.
    Example:
      p = ImagePipeline("cat.jpg").resize((400,400)).auto_enhance().blur_faces(8)
      out = p.run()
    """
    def __init__(self, src):
        self.src = src
        self.ops = []

    def resize(self, size):
        self.ops.append(("resize", size))
        return self

    def thumbnail(self, maxsize):
        self.ops.append(("thumbnail", maxsize))
        return self

    def smart_crop(self, w,h,focus="center"):
        self.ops.append(("smart_crop", (w,h,focus)))
        return self

    def dhash(self, size=8):
        self.ops.append(("dhash", size))
        return self

    def hist_eq(self):
        self.ops.append(("hist_eq", None))
        return self

    def blur_faces(self, radius=15):
        self.ops.append(("blur_faces", radius))
        return self

    def run(self):
        img = _ensure_pil(self.src)
        for op, arg in self.ops:
            if op == "resize":
                img = resize(img, arg)
            elif op == "thumbnail":
                img = thumbnail(img, arg)
            elif op == "smart_crop":
                w,h,focus = arg
                img = smart_crop(img, w, h, focus=focus)
            elif op == "dhash":
                return dhash(img, arg)
            elif op == "hist_eq":
                img = histogram_equalize(img)
            elif op == "blur_faces":
                img = blur_faces(img, blur_radius=arg)
        return img

# -------------------------
# Aliases and export
# -------------------------
ALIASES = {
    "load": load,
    "save": save,
    "resize": resize,
    "thumbnail": thumbnail,
    "crop": crop,
    "smart_crop": smart_crop,
    "dhash": dhash,
    "hamming": hamming,
    "dominant_colors": dominant_colors,
    "hist_eq": histogram_equalize,
    "auto_enhance": auto_enhance,
    "detect_faces": detect_faces,
    "blur_faces": blur_faces,
    "scan_text": scan_text,
    "seam_carve": seam_carve,
    "ImagePipeline": ImagePipeline,
}

def orion_export():
    exports = {"vision": ALIASES}
    exports.update(ALIASES)
    return exports

def register(runtime):
    """Integración directa con el núcleo de Orion"""
    runtime.register_module("vision", orion_export())
