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
| `datetime` | barrido e2e (2026-07-18, `tests/test_utilidades.orx`): parse multi-formato día-primero (misma convención que table), add_days/hours/**minutes/months/years** (fin de mes ajustado: 31 ene + 1 mes = 28/29 feb), diff_days/**hours/minutes**/seconds, parts, weekday **bilingüe** ("es" → "sábado"), start/end_of_month (bisiestos ok), from_date con validación, **to_timestamp/from_timestamp** (puente a unix, roundtrip exacto), is_past/is_future |
| `regex` | is_match, replace |
| `crypto` | hash, sha256 (64 hex) |
| `crypto2` | AES encrypt→decrypt roundtrip |
| `matrix` | add, mul, transpose |
| `validate` | email, length |
| `random` | int (en rango), choice, shuffle |
| `zip` | gzip→gunzip roundtrip |
| `vector` | store en memoria: new/add/search |
| `auth` | argon2 hash+verify, JWT token |
| `cola` | barrido e2e (2026-07-18, `tests/test_infra.orx`): FIFO exacto (push/pop/peek sin consumir), pop de vacía → null, valores complejos con roundtrip que conserva orden de claves, crear/lista/vaciar, eliminar devuelve si existía, size de inexistente → 0 |
| `stat` | correlation |
| `proto` | encode→decode roundtrip |
| `formato` | barrido e2e (2026-07-18, `tests/test_utilidades.orx`): tabla ASCII (listas y dicts), centrar/separador/truncar con elipsis, **numero** (miles/decimales configurables, estilo español opcional), **moneda** (símbolo pegado o código con espacio), **porcentaje**, **bytes** humanizado (base 1024), **duracion** ("1d 2h 3m 4s", sub-segundo en ms) |
| `template` | render con variables (`{{var}}`) |
| `csv` | write→read roundtrip |
| `fs` | mkdir/write/read/size/append/ls/delete |
| `db` | SQLite: create/insert/query |
| `process` | execute (code/out) |
| `env` | set/get/has |
| `state` | barrido e2e (2026-07-18, `tests/test_infra.orx`): set (devuelve valor)/get con default/has, incr/decr atómicos que conservan Int y ERROR claro sobre valor no numérico (antes lo pisaba), **setnx** atómico (candado simple entre requests), delete devuelve si existía, keys en orden de inserción, all/len/clear, persist siembra desde archivo + cada escritura queda en disco (verificado leyendo el JSON crudo) |
| `cache` | barrido e2e (2026-07-18, `tests/test_infra.orx`): set/get con default, **TTL opcional en segundos** (expiración perezosa sin hilos: get/has/size/keys solo ven entradas vivas; `ttl(clave)` → segundos restantes o null), del/has honestos (dicen si existía), roundtrip de estructuras anidadas conservando orden de claves |
| `config` | get sobre dict |
| `grafo` | create/node/edge |
| `quantum` | zero, bell |
| `db` | **SQLite + Postgres, ambos con pool + validados e2e** (2026-07-21). El primer argumento decide el motor: una URL `postgres://user:pass@host:puerto/base` va al backend Postgres; cualquier otra cosa es una ruta de archivo SQLite. **Misma API en los dos** (query/uno/ejecutar/insertar/transaccion/tablas/cerrar) — el código del dev no cambia entre motores. **SQLite** (rusqlite bundled): conexión persistente por ruta (antes reabría en cada query), WAL + synchronous=NORMAL + foreign_keys=ON + busy_timeout(5s), `:memory:` **persiste** entre llamadas, `insertar` = last_insert_rowid; bench 5000 inserts ≈390ms Windows. **Postgres** (crate `postgres` sync, NoTls): pool de una conexión por URL, placeholders `?`→`$1..$n` automáticos, `insertar` espera `... RETURNING id`, transacciones reales, mapeo de tipos PG→Orion (TEXT→Str, NUMERIC→Float vía rust_decimal, BOOLEAN→Bool, INT/SERIAL→Int, JSONB→Dict anidado, TIMESTAMP/DATE→string ISO, UUID→Str, BYTEA→base64). Adaptador de parámetros `Param(ToSql)` que codifica cada valor según el tipo que la columna espera (un Float Orion entra bien en NUMERIC sin castear). **TLS por `sslmode` en la URL** (2026-07-21): `disable`/ausente→NoTls (dev local), `require`/`prefer`→cifrado sin verificar cert (self-signed ok), `verify-ca`/`verify-full`→cifrado + verificado; native-tls usa SChannel en Windows (sin OpenSSL). Validado 17/17 checks e2e contra Postgres 17 real (base orion-nexus) + SQL multilínea e2e. **MySQL/MariaDB** (crate `mysql`, URL `mysql://`): misma API, pool interno del crate, `?` nativo (sin traducir), `insertar`=last_insert_id, transacciones, mapeo mysql::Value→Orion; implementado y compila, dispatch verificado, **pendiente validar e2e contra un servidor MySQL real**. **Carga masiva `db.copiar(id, tabla, [columnas], [[fila],…]) → Int`** (2026-07-21): portable entre motores — Postgres usa **COPY FROM STDIN** (formato texto TSV con escapes libpq), SQLite una transacción + sentencia preparada reusada, MySQL INSERT multi-fila por lotes de 500. Medido: **1.000.000 filas por COPY en 2,09 s (~478.000 filas/s)** contra Postgres real; correctitud 7/7 (tabs/bools/numeric/nulls). **Carga en STREAMING con RAM constante `db.copiar_archivo(id, tabla, [columnas], ruta_csv, opts?)`** (2026-07-21): lee el CSV en trozos de 64KB y lo empuja sin materializar — Postgres = COPY FROM STDIN WITH (FORMAT csv) (el server parsea), SQLite/MySQL = crate `csv` en streaming + tx/lotes. opts: `{header, delim}`. **1M filas desde CSV de 32MB en 1,2 s con pico de RAM de solo 13 MB** vs 1.902 MB si se materializa la lista en memoria (**146× menos RAM** — el enfoque HPC real, no "otro Python"). Respeta comas dentro de comillas. **Pool de N conexiones** en Postgres (`db.pool(url, n)`, default 8): checkout/checkin con Condvar, descarta conexiones muertas (is_closed); medido **3,9× de concurrencia** (8 queries de 0,2s: pool 8 = 0,46 s vs pool 1 = 1,79 s). OJO gotcha del lenguaje para construir listas grandes: `lista = lista + [x]` es O(n²) (copia); usar `lista.push(x)` (O(1), muta in-place) |
| `session` | **NUEVO + e2e** (2026-07-20): sesiones server-side, store global compartido entre workers de serve. set/get(default)/all/has/delete/destroy/count/sweep(max_edad). Integrado en serve: `req["sid"]` (reutiliza cookie `orion_sid` o genera sid de 128 bits); el **Set-Cookie se emite solo si el handler guardó datos** (no intrusivo), con `HttpOnly; SameSite=Lax` por defecto. Verificado: la sesión persiste entre requests con la cookie, sin cookie → defaults |
| `router` | **e2e con servidor HTTP real** (2026-07-20, `orion-vm/tests/serve_e2e.rs` + `tests/server_e2e.orx`, 31 bloques): serve despacha por el router activo (attach ya es real, antes era superficie fantasma), rutas por método, `:params` y `*wildcard` verificados con requests reales, middlewares en orden con corte (403/429), fallback al handler global. **Nuevo stack "backend moderno"**: `router.static` (archivos con MIME auto, index.html, anti path-traversal) · `router.guard(id, "/prefijo", secret)` (JWT Bearer automático: sin token → 401 solo, con token → claims en `req["user"]` sin tocar el handler). Respuestas ergonómicas en serve: un Dict de datos o una List salen como **JSON automático** (estilo FastAPI, sin `json.forge`); `{"json": v}`, `{"redirect": "/x"}` (302+Location), `{"file": ruta}` (binario byte a byte), `{"cookies": {...}}` (Set-Cookie por entrada). `req` ahora trae `cookies` (Cookie parseado) y `json` (body pre-parseado) además de headers/query/params/ip |
| `sse` | **e2e con servidor real**: named/json_event llegan al cliente con content-type text/event-stream y headers custom; formato spec verificado sobre HTTP |
| `stream` | range, from |
| `middleware` | **e2e con servidor real**: rate_limit por IP (3 pasan → 429, poda automática del mapa), cors llega al navegador vía response headers, auth_bearer acepta "Bearer <token>" completo y valida JWT end-to-end (bug arreglado: jsonwebtoken exigía claim exp y rechazaba tokens sin expiración) |
| `pdf` | **lectura + OCR + escritura + conversión** (2026-07-22). Escritura (ya existía): crear/plantilla/reporte/marca/paginar (lopdf+printpdf). **Nuevo — lectura**: `pdf.leer(ruta)` extrae texto embebido (crate `pdf-extract`); `pdf.ocr(ruta)` hace OCR de PDFs escaneados extrayendo las imágenes JPEG/DCTDecode embebidas y pasándolas por `vision.ocr` (motor ocrs local); `pdf.texto(ruta)` inteligente = texto embebido, y si no hay (escaneado) cae a OCR solo; `pdf.desde_imagen(img, salida)` convierte una imagen a PDF (embebe como XObject DCTDecode). Validado 9/9 e2e: crear→leer, imagen→PDF→ocr lee texto+números conocidos, pdf.texto elige bien. pdf.ocr cubre TODO tipo de PDF (2026-07-22): primero extrae imágenes JPEG embebidas (escáneres); si no hay, **rasteriza cada página con pdfium** (dll INCRUSTADO en el binario, ~7MB, se extrae a temporal en el 1er uso — self-contained, sin instalar nada) y hace OCR del render. Validado con PDF vectorial de texto puro (leyó ORION/2026/8842). Demo: `demo/ocr/demo_ocr.orx`. **Multiplataforma** (2026-07-22): binario pdfium incrustado por plataforma (Windows/Linux x64, macOS arm64/x64) vía cfg; la lógica de rasterizado es compartida. Windows validado (compila+corre); Linux/macOS cableados con la misma lógica pero pendientes de compilar en esos SO (no cross-compilables desde Windows). Precisión limitada por ocrs |
| `watch` | stat de archivo |
| `cosmos` | star, universe (simulación) |
| `table` | barrido e2e completo (2026-07-16, `tests/test_table.orx`, 34 tests / ~150 checks exactos): from/rows/size/count/column/headers, keep/drop/rename/cast (bool inteligente + **date** a ISO con formato chrono opcional), where con motor de expresiones real (parser con precedencia: && \|\| !, paréntesis, aritmética, columna vs columna, contains/starts_with/ends_with sobre cualquier tipo, null, notación científica, escapes, `` `columnas con espacios` ``), add con funciones (upper/lower/trim/len/abs/round/floor/ceil/sqrt/min/max/pow + fechas: date/year/month/day/date_diff/date_add/today), sort/top/bottom/dedupe/sample, group (multi-columna, claves con tipo original)/agg, stats y anomalies con percentiles interpolados (paridad frame), join (multi-clave, inner+left, colisiones → sufijo `_2`)/concat, forecast/correlate/rank/moving_avg/normalize, profile/schema (detectan bool), clean_headers (+"snake")/clean, save→load CSV/JSON/XLSX + load_sheet, **orden de columnas ORIGINAL preservado end-to-end** (dicts = IndexMap en todo el pipeline, 2026-07-18) + **save/peek con lista de columnas = el dev decide orden y selección**, delimitadores auto (`,` `;` tab) leyendo solo 1ª línea, stream con límite y delimitador auto, errores limpios vía attempt. Perf: where 200k filas ≈ 45ms (parse-una-vez); pipeline completo de 200k ≈ 6s (el puente VM↔módulo bajó ~33% con IndexMap); para volumen grande está `frame` (columnar, por handle) |
| `serie` | new (serie temporal) |
| `frame` | barrido e2e completo (2026-07-15, `tests/test_frame.orx`, 16 tests / ~80 checks exactos): from_list/schema/size/col/row/to_list, keep/drop/rename, where_ por tipo, head/tail/sort, sum/mean/min/max/std, stats con percentiles interpolados, group (sum/avg/count/min/max), add_col, save→open CSV, save_odf→load_odf + autodetección, from_txt con separador, scan_stats, each_chunk, to_excel/txt_to_odf/txt_to_excel, free/frames (gestión del store) |
| `timewarp` | barrido e2e (2026-07-18, `tests/test_utilidades.orx`): timestamp/ms/ns coherentes, add/sub/diff/since/until exactos, clock+elapsed (cronómetro real), wait acepta "50ms"/"1s"/número, format coherente con datetime.to_timestamp; **measure_time RETIRADO** (devolvía siempre ~0 ms — era fake; el error guía a clock/elapsed) |
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
| `vision` | procesamiento de imágenes (resize/crop/gray/rotate/base64/thumbnail) + **OCR real** (2026-07-22): `vision.ocr(ruta, opts?)` → texto. Motor por defecto **`ocrs`** (redes ONNX en Rust puro vía rten, corriendo LOCAL — sin Tesseract, sin API, sin internet); modelos de detección+reconocimiento INCRUSTADOS en el binario (include_bytes, ~12MB) → OCR out-of-the-box. Motor opcional Tesseract si el developer lo instala: `vision.ocr(r, {engine:"tesseract", lang:"spa"})` llama al binario del sistema por proceso (sin dependencia de compilación). Validado 9/9 asserts con texto+números conocidos en ambos motores + error limpio. Modelos en `orion-vm/models/*.rten`. **Preprocesado OCR**: `vision.ocr(r, {preprocess:yes})` binariza (gris+Otsu) antes de leer, reduce ruido. **Ops estilo OpenCV** (crate `imageproc`, Rust puro): `threshold`/`umbral` (Otsu), `edges`/`bordes` (Canny), `contrast`, `sharpen`, `invert` — además de las de imagen que ya había. Validado 8/8 (cada op genera salida + OCR con preprocesado lee todo) |
| `insight` | archivo de imagen + (posible) modelo |
| `search` | archivos de datos a indexar |
| `excel_f` | (puro, cableado; falta test de salida) |

## 🟡 Interfaz — verificado por construcción

| Módulo | Estado |
|---|---|
| `gui` | ✅ Árbol de widgets verificado headless (`smoke_gui_headless_widgets`) + render confirmado con capturas. Tema configurable, estilo por widget, 20+ widgets, charts. Ver [GUI.md](GUI.md). El render abre ventana, así que no se automatiza el pixel-perfect. |
| `tui` | 🚧 UI de terminal (requiere TTY/interacción) — sin tests aún |

## 🌐 Automatización web

| Módulo | Estado |
|---|---|
| `browser` | ✅ Automatización web sobre CDP, sin driver externo ni dependencias nuevas. 61 tests e2e en [`browser_e2e.rs`](orion-vm/tests/browser_e2e.rs) contra servidor local, más 54 unitarios. Verificado: arranque y detección de Chrome/Chromium/Brave/Edge, navegación, clic con espera de accionabilidad (tapado temporal, parcial y permanente con `force`), teclado real (eventos, no asignación de `value`), `<select>` nativo, diálogos `alert`/`confirm`/`prompt` por política, iframes accesibles, pestañas nuevas, capturas PNG, selectores CSS/XPath/`text=` unificados, y **extracción declarativa** (`extract`/`extract_to`): el esquema entero se compila a UNA sola evaluación en la página (Selenium hace una petición HTTP por atributo leído), con volcado en streaming a `.csv` o `.odf` que lee `frame` directamente. Pico de RAM medido: 18,4 MB con 1.000 filas y 18,9 MB con 8.000. **Sin constantes fijadas**: los 13 parámetros que deciden el comportamiento (espera global, reintento, pasos de arrastre, capas de `force`, profundidad de iframes, buffer de eventos, sondeo…) se configuran en `open()`, y las banderas de arranque se pueden quitar además de añadir. Ver [BROWSER.md](BROWSER.md). **Formularios y tablas**: `fill` rellena un formulario entero en una llamada (medido: 221 ms tecla a tecla frente a 1 ms) detectando si cada control es texto, desplegable o casilla, y escribiendo por el setter nativo del prototipo — lo unico que React escucha; `{ keys: yes }` vuelve a las teclas reales cuando el sitio las necesita. `check`/`uncheck` son idempotentes. `table` lee una `<table>` entera con la cabecera deducida en cascada, `colspan`/`rowspan` expandidos y las tablas anidadas excluidas — reglas sacadas de medir tablas reales, donde 13 de 13 no tenian `<thead>`. `value` lee lo que un campo contiene ahora, que no es lo mismo que `attr("value")`. **Archivos**: `upload` adjunta tanto sobre el `<input type=file>` como sobre el botón que lo abre (el caso que Selenium no cubre, con el input oculto tras el diseño del sitio); `download` espera al evento de finalización y devuelve la ruta real, sin diálogo "Guardar como" ni archivos a medias; `pdf` imprime sin diálogo de impresión. Las tres interceptan la ventana nativa **antes de que exista**, así que no dependen del idioma del sistema ni de que haya escritorio. **Sesión y seguridad**: `save_state`/`load_state` guardan y restauran cookies y almacenamiento en un JSON — te logueas UNA vez y las siguientes ejecuciones entran solas, sin repetir un formulario que puede cambiar ni un doble factor que puede saltar (ese archivo vale como credencial); `open({allow: [...]})` acota a que dominios puede ir el proceso, porque un bot lleva encima la sesion de la empresa; `{secret: [...]}` mantiene una contrasena fuera de los errores y los logs. **Estabilidad**: `reload`/`back`/`forward` sin depender del evento de carga (al volver atras Chrome restaura de su cache y ese evento NO llega), y `wait{idle}` espera a que la red se calme contando peticiones dentro de la pagina, en vez del `sleep(2)` que va corto cuando la red va lenta y sobra cuando va rapida. **Medido** (2026-08-08, `bench/web/`, mismo Chrome para las tres y misma huella de datos): extraer 500 filas x 4 campos cuesta **8 ms** con `extract` frente a 14.132 ms haciendolo como ensena la documentacion de Selenium y 9.234 ms como la de Playwright — unas 1.700 veces, porque ahi el camino rapido es el unico que hay. Contra sus caminos optimizados (JavaScript a mano) Orion NO es mas rapido: 8 ms frente a 7,7 ms de Selenium, dentro del ruido. Donde si hay diferencia de fondo es en el proceso entero (**745 ms** frente a 1.430 ms de Playwright y 8.088 ms de Selenium, de los que ~4,2 s se van despues de la ultima linea del script) y en memoria de la pila (**16,2 MB** frente a 59,5 MB de Selenium y 156,5 MB de Playwright): Orion no arranca ningun proceso auxiliar, Selenium necesita `chromedriver.exe` y Playwright un `node.exe`. Requiere un navegador basado en Chromium instalado; los tests se saltan si no lo hay. **Despliegue verificado**: `orion --build` produce un `.exe` autónomo (~58 MB) que corre sin `orion.exe` ni `chromedriver.exe` — comprobado con `PATH` reducido a `C:\Windows\system32`. El módulo **no hace ninguna llamada de red propia**, lo que lo hace usable en redes corporativas donde `webdriver-manager` de Python no puede descargar el driver. Ver [BROWSER.md §11](BROWSER.md). |

---

## Recomendación para el anuncio del MVP

- **Promete los ~43 módulos de la columna ✅** con ejemplos — ahí está el valor real
  y verificado: datos (csv/excel/table/frame/serie/stat/matrix), web/backend
  (state/router/middleware/sse/auth/db), utilidades (crypto/zip/regex/validate/
  template/pdf), y automatización (fs/process/env/watch/cola).
- Para los **⚠️ externos**, di "integra con S3/SSH/Docker/SMTP/LLM…" en vez de
  "probado en producción" — son adaptadores cableados, no garantías end-to-end.
- No anuncies `gui`/`tui` como listos todavía.
