# Benchmarks de Orion

Benchmarks reproducibles del motor de datos contra Python, más un estrés del
GC. Un solo comando:

```powershell
powershell -ExecutionPolicy Bypass -File bench\run_all.ps1
```

Requisitos: `python` en el PATH y el binario release
(`cargo build --release --manifest-path orion-vm/Cargo.toml`).

## La tarea

La misma para ambos lenguajes, sin trucos: **cargar 500.000 filas × 4
columnas a columnas tipadas** (int, str, str, float) **y agregar** (`sum` +
`mean` sobre la columna numérica). La línea base de Python usa solo stdlib
(`csv`) — el equivalente honesto de `frame.open`, no una comparación contra
pandas ni contra un Python artificialmente lento.

## Metodología

- Dataset determinista (seed fija) generado por `gen_data.py` (~14 MB CSV).
- Cada medición se corre dos veces y se reporta la **segunda** (en caliente,
  archivo en cache del SO): ninguno paga el I/O frío.
- `medir.ps1` reporta tiempo de pared y **pico real de RAM** del proceso
  (working set muestreado cada 10 ms).
- Los tiempos "internos" (sin arranque del proceso) los imprime cada script.

## Resultados (2026-07-11)

Intel i7-1165G7, 24 GB RAM, Windows 11, Python 3.13, Orion release.

| Pipeline                    | interno | pared  | RAM pico |
|-----------------------------|---------|--------|----------|
| Python 3.13 stdlib csv      | 591 ms  | 746 ms | 104 MB   |
| Orion `frame.open` CSV      | 481 ms  | 516 ms | 171 MB   |
| **Orion `frame.open` .odf** | **77 ms** | **101 ms** | **58 MB** |

Bonus de corrección: la suma y la media que imprimen Python y Orion
coinciden dígito a dígito — el benchmark es también un test cruzado.

Lecturas honestas:

- **Con `.odf`, Orion es ~7-8× más rápido y usa ~la mitad de RAM que
  Python** en el mismo pipeline. El binario columnar elimina el parsing de
  texto: los números se leen como bytes crudos.
- **En CSV, Orion ≈ Python** (±20% según la corrida): el cuello es el
  parsing de texto, no el lenguaje. Este resultado *valida* la decisión de
  diseño del `.odf`.
- Pendiente conocido: `frame.open` sobre CSV usa más RAM que Python (171 vs
  104 MB) porque la inferencia de tipos materializa las celdas crudas y las
  columnas tipadas a la vez. Optimizable.

## Estrés del GC (`gc_ciclos.orx`)

200.000 listas cíclicas huérfanas (`push(a, a)` + rebind). Mide que el
recolector de ciclos de listas/closures (2026-07-11) de verdad devuelve la
memoria:

| Versión                        | RAM pico |
|--------------------------------|----------|
| Sin recolección de ciclos de listas | 79 MB (fuga lineal) |
| Con recolección (actual)       | ~11 MB (línea base de la VM) |

## Archivos

- `gen_data.py` — genera `data.csv` (determinista, seed 42)
- `bench_py.py` — línea base Python stdlib
- `bench_csv.orx` / `bench_odf.orx` — pipeline Orion (CSV / binario)
- `conv_odf.orx` — conversión CSV → `.odf` (una vez)
- `gc_ciclos.orx` — estrés del GC
- `medir.ps1` — tiempo de pared + pico de RAM muestreado
- `run_all.ps1` — orquestador

Los artefactos generados (`data.csv`, `data.odf`, `out_*.txt`) están en el
`.gitignore` — solo se versionan los scripts.
