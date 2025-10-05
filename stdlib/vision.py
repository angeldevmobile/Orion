"""
Vision: procesamiento de imágenes en Orion.
"""
from PIL import Image

def open_image(path): return Image.open(path)
def size(path): 
    img = Image.open(path)
    return img.size
def to_gray(path, out):
    img = Image.open(path).convert("L")
    img.save(out)
    return out
