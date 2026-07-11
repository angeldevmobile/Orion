"""Genera el dataset del benchmark: 500.000 filas x 4 columnas (CSV ~14MB).

Determinista (seed fija) para que los resultados sean comparables entre
máquinas y entre corridas.
"""
import csv, random, os

random.seed(42)
regiones = ["Norte", "Sur", "Este", "Oeste", "Centro"]
productos = ["Laptop", "Mouse", "Teclado", "Monitor", "Webcam", "Dock"]

out = os.path.join(os.path.dirname(__file__), "data.csv")
with open(out, "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["id", "region", "producto", "venta"])
    for i in range(500_000):
        w.writerow([i, random.choice(regiones), random.choice(productos),
                    round(random.uniform(10.0, 5000.0), 2)])
print("ok", out)
