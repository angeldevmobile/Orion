"""Regenera las imágenes de tests/fixtures/insight/.

Son deterministas: ejecutar este script produce exactamente los mismos ficheros
que hay versionados. Solo hace falta si se quiere cambiar o ampliar el juego de
pruebas de `insight`.

    python tools/gen_test_images.py

Requiere Pillow.
"""
import math
import os
from PIL import Image, ImageDraw

OUT = os.path.join("tests", "fixtures", "insight")


def tabla(fondo, tinta, nombre):
    """Rejilla de 4x4 celdas. Con fondo/tinta grises reproduce el caso que un
    umbral fijo pierde por completo."""
    im = Image.new("L", (400, 300), fondo)
    dr = ImageDraw.Draw(im)
    for y in [40, 90, 140, 190, 240]:
        dr.line([(20, y), (380, y)], fill=tinta, width=2)
    for x in [20, 110, 200, 290, 380]:
        dr.line([(x, 40), (x, 240)], fill=tinta, width=2)
    im.save(os.path.join(OUT, nombre))


def texto():
    """Bloques densos sin rejilla: no es tabla ni firma."""
    im = Image.new("L", (400, 300), 255)
    dr = ImageDraw.Draw(im)
    for row in range(12):
        y = 20 + row * 22
        x = 20
        while x < 370:
            wlen = 12 + (row * 7 + x) % 30
            dr.rectangle([x, y, min(x + wlen, 375), y + 9], fill=0)
            x += wlen + 8
    im.save(os.path.join(OUT, "texto.png"))


def firma():
    """Texto arriba y un trazo curvo continuo abajo."""
    im = Image.new("L", (400, 300), 255)
    dr = ImageDraw.Draw(im)
    for row in range(4):
        y = 20 + row * 22
        dr.rectangle([20, y, 300, y + 9], fill=0)
    pts = []
    for i in range(300):
        t = i / 300 * 4 * math.pi
        pts.append((60 + i, 240 + int(28 * math.sin(t) * math.cos(t * 0.7))))
    dr.line(pts, fill=0, width=2)
    im.save(os.path.join(OUT, "firma.png"))


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    tabla(255, 0, "tabla.png")
    tabla(235, 120, "tabla_gris.png")
    texto()
    firma()
    Image.new("L", (400, 300), 255).save(os.path.join(OUT, "blanco.png"))
    print("generadas en", OUT)
