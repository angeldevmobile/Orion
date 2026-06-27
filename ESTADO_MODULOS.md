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
| `table` | from (lista de dicts) |
| `serie` | new (serie temporal) |
| `frame` | from_list |
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

## 🚧 No probado — interfaz interactiva

| Módulo | Motivo |
|---|---|
| `gui` | UI de escritorio (requiere ventana/interacción) |
| `tui` | UI de terminal (requiere TTY/interacción) |

---

## Recomendación para el anuncio del MVP

- **Promete los ~43 módulos de la columna ✅** con ejemplos — ahí está el valor real
  y verificado: datos (csv/excel/table/frame/serie/stat/matrix), web/backend
  (state/router/middleware/sse/auth/db), utilidades (crypto/zip/regex/validate/
  template/pdf), y automatización (fs/process/env/watch/cola).
- Para los **⚠️ externos**, di "integra con S3/SSH/Docker/SMTP/LLM…" en vez de
  "probado en producción" — son adaptadores cableados, no garantías end-to-end.
- No anuncies `gui`/`tui` como listos todavía.
