"""Genera la página de prueba: un listado de productos, determinista.

Se sirve como archivo local (`file://`) y no por HTTP a propósito: lo que se
mide es el coste de hablar con el navegador, no la red ni un servidor de
pruebas. Todas las herramientas cargan exactamente el mismo archivo.
"""
import random
import sys
from pathlib import Path

FILAS = int(sys.argv[1]) if len(sys.argv) > 1 else 500

random.seed(42)  # mismo listado en cada ejecución y en cada herramienta

partes = ["""<!doctype html>
<html><head><meta charset="utf-8"><title>Catalogo</title></head><body>
<div id="lista">"""]

for i in range(FILAS):
    precio = round(random.uniform(5, 2000), 2)
    partes.append(
        f'<div class="card" data-sku="SKU-{i:05d}">'
        f'<span class="title">Producto {i}</span>'
        f'<span class="price">{precio:,.2f} EUR</span>'
        f'<a href="/p/{i}">ver</a>'
        f'</div>'
    )

partes.append("</div></body></html>")

destino = Path(__file__).with_name("catalogo.html")
destino.write_text("".join(partes), encoding="utf-8")
print(f"{destino.name}: {FILAS} filas, {destino.stat().st_size / 1024:.0f} KB")
