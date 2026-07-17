# Estado de los módulos de Orion (matriz de madurez)

> Auditoría de smoke-tests del 2026-06-26. El objetivo es **anunciar solo lo
> verificado**. Cada módulo de la columna ✅ tiene al menos un test automático en
> [`orion-vm/tests/modules_smoke.rs`](orion-vm/tests/modules_smoke.rs) que ejecuta
> una operación real y comprueba el resultado. Total: **44 tests, todos en verde**.

## ✅ Verificado — funciona offline (anunciable con confianza)

Probados con aserciones reales sobre su salida:

| Módulo | Qué se verificó |
|---|---|
| `strings` | upper/lower/length/split |
| `json` | parse + forge (roundtrip) |
| `datetime` | timestamp, now |
| `regex` | is_match, replace |
| `crypto` | hash, sha256 (64 hex) |
| `crypto2` | AES encrypt→decrypt roundtrip |
| `matrix` | add, mul, transpose |
| `validate` | email, length |
| `random` | int (en rango), choice, shuffle |
| `zip` | gzip→gunzip roundtrip |
| `vector` | store en memoria: new/add/search |
| `auth` | argon2 hash+verify, JWT token |
| `cola` | cola FIFO push/pop/size |
| `stat` | correlation |
| `proto` | encode→decode roundtrip |
| `formato` | centrar, separador |
| `template` | render con variables (`{{var}}`) |
| `csv` | write→read roundtrip |
| `fs` | mkdir/write/read/size/append/ls/delete |
| `db` | SQLite: create/insert/query |
| `process` | execute (code/out) |
| `env` | set/get/has |
| `state` | set/get/incr atómico/decr/delete |
| `cache` | set/get |
| `config` | get sobre dict |
| `grafo` | create/node/edge |
| `quantum` | zero, bell |
| `router` | new + registro de rutas |
| `sse` | event, named (formato SSE) |
| `stream` | range, from |
| `middleware` | rate_limit |
| `pdf` | create (genera archivo .pdf) |
| `watch` | stat de archivo |
| `cosmos` | star, universe (simulación) |
| `table` | barrido e2e completo (2026-07-16, `tests/test_table.orx`, 27 tests / ~110 checks exactos): from/rows/size/count/column, keep/drop/rename/cast (bool inteligente), where con motor de expresiones real (parser con precedencia: && \|\| !, paréntesis, aritmética, columna vs columna, contains/starts_with/ends_with, null), add con funciones (upper/lower/trim/len/abs/round/floor/ceil/sqrt/min/max/pow, negativos, concat), sort/top/bottom/dedupe/sample, group/agg (count no-nulos, op inválida = error), stats con percentiles interpolados (paridad frame), join inner+left/concat, forecast/correlate/anomalies/rank/moving_avg/normalize, profile (detecta bool), save→load CSV/JSON/XLSX + load_sheet, delimitadores auto (`,` `;` tab) leyendo solo 1ª línea, stream con límite y delimitador auto, errores limpios vía attempt |
| `serie` | new (serie temporal) |
| `frame` | barrido e2e completo (2026-07-15, `tests/test_frame.orx`, 16 tests / ~80 checks exactos): from_list/schema/size/col/row/to_list, keep/drop/rename, where_ por tipo, head/tail/sort, sum/mean/min/max/std, stats con percentiles interpolados, group (sum/avg/count/min/max), add_col, save→open CSV, save_odf→load_odf + autodetección, from_txt con separador, scan_stats, each_chunk, to_excel/txt_to_odf/txt_to_excel, free/frames (gestión del store) |
| `timewarp` | timestamp |
| `tarea` | now |
| `log` | info |
| `secret` | mask |
| `embed` | similarity (matemática de vectores) |
| `excel` | estadísticas sobre datos |

## ⚠️ Cableado — existe y valida, requiere servicio externo

El módulo despacha y valida argumentos correctamente (verificado en el test
`smoke_external_modules_wired`), pero su funcionamiento end-to-end depende de un
servicio/credencial externos, así que **no está probado de extremo a extremo**.
Anunciar como "soporta X" con esa salvedad:

| Módulo | Necesita |
|---|---|
| `net` | red / servidor HTTP remoto |
| `mail` | servidor SMTP + credenciales |
| `s3` | bucket AWS + credenciales |
| `ssh` | servidor SSH |
| `docker` | daemon de Docker |
| `ws` | servidor WebSocket |
| `llm` | API key de un proveedor LLM |
| `ai` | API key / modelo |
| `vision` | archivo de imagen real |
| `insight` | archivo de imagen + (posible) modelo |
| `search` | archivos de datos a indexar |
| `excel_f` | (puro, cableado; falta test de salida) |

## 🟡 Interfaz — verificado por construcción

| Módulo | Estado |
|---|---|
| `gui` | ✅ Árbol de widgets verificado headless (`smoke_gui_headless_widgets`) + render confirmado con capturas. Tema configurable, estilo por widget, 20+ widgets, charts. Ver [GUI.md](GUI.md). El render abre ventana, así que no se automatiza el pixel-perfect. |
| `tui` | 🚧 UI de terminal (requiere TTY/interacción) — sin tests aún |

---

## Recomendación para el anuncio del MVP

- **Promete los ~43 módulos de la columna ✅** con ejemplos — ahí está el valor real
  y verificado: datos (csv/excel/table/frame/serie/stat/matrix), web/backend
  (state/router/middleware/sse/auth/db), utilidades (crypto/zip/regex/validate/
  template/pdf), y automatización (fs/process/env/watch/cola).
- Para los **⚠️ externos**, di "integra con S3/SSH/Docker/SMTP/LLM…" en vez de
  "probado en producción" — son adaptadores cableados, no garantías end-to-end.
- No anuncies `gui`/`tui` como listos todavía.
