"""Línea base Python (solo stdlib, sin pandas): el equivalente honesto de
frame.open + sum + mean — carga el CSV completo a columnas tipadas y agrega
sobre 'venta'. Misma cantidad de trabajo que hace Orion."""
import csv, time, os

path = os.path.join(os.path.dirname(__file__), "data.csv")
t0 = time.perf_counter()

ids, regs, prods, ventas = [], [], [], []
with open(path, newline="") as f:
    r = csv.reader(f)
    next(r)
    for row in r:
        ids.append(int(row[0]))
        regs.append(row[1])
        prods.append(row[2])
        ventas.append(float(row[3]))

total = sum(ventas)
media = total / len(ventas)
t1 = time.perf_counter()
print(f"filas={len(ventas)} suma={total:.2f} media={media:.4f}")
print(f"tiempo_ms={(t1 - t0) * 1000:.1f}")
