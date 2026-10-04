# Changelog - Orion Language

Los cambios notables del lenguaje, la stdlib y las herramientas. Fechas en
formato AAAA-MM-DD.

## Sin publicar

### Cambiado
- **En el JIT, un bucle escrito en el programa principal va tan rápido como
  dentro de una función.** Cada asignación en `main` copiaba además el valor
  a la tabla de globales del runtime (una llamada y un mutex); ahora solo se
  copian las variables que alguna función, act o tarea lee.
  `bench/jit/bucle_main.orx` (10 millones de sumas en main): 2,10 s → 0,10 s.
- **El JIT ya no reserva memoria por cada número: hasta 10× más rápido y
  50× menos RAM.** Los valores usan NaN-boxing: enteros de 48 bits,
  decimales, `null` y booleanos van dentro del propio valor (los enteros más
  grandes siguen en el heap, el rango de `i64` no cambia). Antes cada
  operación reservaba memoria que nunca se liberaba. Mejor de 3, misma
  sesión, `bench/jit/run_jit.ps1`: bucle de enteros 2,06 s · 1241 MB →
  0,21 s · 23 MB; de decimales 1,81 s · 1088 MB → 0,20 s · 23 MB; `fib(30)`
  0,58 s · 384 MB → 0,10 s · 23 MB; lista de un millón 0,58 s · 234 MB →
  0,19 s · 32 MB. También afecta a lo compilado con `--build`.
- **La aritmética y las comparaciones del JIT van en línea.** Si los dos
  operandos son enteros de 48 bits o decimales, `+ - * / %` y `< <= > >= ==
  !=` se compilan a instrucciones de la CPU, y la condición de un salto se
  decide sin llamar al runtime; con otros tipos, o si el resultado no cabe,
  se llama al runtime como antes. Sobre el cambio anterior (mejor de 3, misma
  sesión, tiempo de pared con el arranque incluido): bucle de enteros 280 →
  129 ms, de decimales 222 → 123 ms, `fib(30)` 126 → 69 ms. Desde el inicio
  de la sesión, el bucle de enteros pasó de 2,06 s y 1241 MB a 0,13 s y
  12 MB.
- **El intérprete es hasta 2× más rápido en llamadas a función.** Llamar a una
  función ya no copia su cuerpo (ahora es `Arc<[Instruction]>`) y cada
  instrucción se ejecuta por referencia en vez de clonarse. Medido con
  `bench/jit/run_jit.ps1` (mejor de 3, misma sesión): `fib(30)` 5,83 s →
  2,79 s; lista de un millón 2,42 s → 1,67 s y 131 → 81 MB; bucles de enteros
  y decimales entre 16 % y 24 % más rápidos. El formato `.orbc` no cambia.
- **El intérprete ejecuta las instrucciones simples por lotes.** Cargas,
  variables, aritmética, comparaciones y saltos van por un despacho pequeño
  (`exec_hot`), y la cancelación, el GC y el fin de frame se revisan una vez
  cada hasta 1024 de ellas en vez de en cada una. Sobre el cambio anterior
  (mejor de 3, misma sesión): bucle de enteros 8,68 s → 4,53 s, de decimales
  8,06 s → 4,28 s, `fib(30)` 3,49 s → 2,39 s, ternario 11,6 s → 6,6 s. El
  depurador sigue avanzando de una instrucción en una.

### Corregido
- **El error de una tarea `spawn` se escribe en stderr** (`error in spawned
  task 'f': ...`), en el intérprete y en el JIT. Nadie puede hacer `await`
  de `spawn f()` y su error se perdía; con canales, el programa se colgaba
  sin decir por qué. Una tarea guardada (`t = f()`) sigue entregando su
  error en el `await`, y una tarea cancelada no avisa.
- **Una función `async` ve las variables globales en el intérprete.** La
  tarea corre en otra VM que no las recibía: `async fn f() { return x }`
  fallaba con "Variable 'x' is not defined" (el JIT sí las veía). Ahora
  recibe una copia, al lanzarla, de las globales que leen ella, las
  funciones a las que llama y los acts.
- **Un ternario ya no apaga el JIT.** Un valor que cruzaba de un bloque a
  otro (el ternario, o una suma con un operando ya calculado y el otro
  condicional) mandaba **todo el programa** al intérprete, y `--build` lo
  entregaba en modo bundle. Ahora esos valores viajan en variables de
  Cranelift por posición de la pila. `bench/jit/ternario.orx`: 5,3 s → 0,16 s.
- El test `smoke_net_post_con_409_devuelve_status_y_cuerpo` fallaba al azar:
  su servidor falso respondía sin leer el cuerpo de la petición y Windows
  cortaba la conexión.
- **En el JIT (y en lo compilado con `--build`) los errores llegan al
  `attempt`**, como en el intérprete. Antes el runtime terminaba el proceso
  con cualquier error (división por cero, desbordamiento, índice fuera de
  rango, tipos incompatibles, método inexistente...) y un `error "x"` dentro
  de una función no llegaba al `attempt` de quien la llamaba. Ahora el error
  queda pendiente, el código nativo salta al `handle` activo o sale de la
  función hasta encontrarlo, y en una tarea `async` llega al `await`. Los
  mensajes son los del intérprete. `1 / 0.0` también es "División por cero"
  en el JIT (antes daba infinito).
- **El JIT ya no se queda colgado al llenar una lista con `push`.** Los
  builtins `len`, `push`, `pop`, `first` y `last` copiaban la lista entera a
  la VM en cada llamada: llenar un millón de elementos no terminaba (más de
  15 min y 8,7 GB). Ahora van directos y tardan 0,54 s, frente a 3,0 s en el
  intérprete (`bench/jit/lista.orx`).
- **En el JIT, `push` devuelve la misma lista**, como en el intérprete. Antes
  devolvía una copia y lo añadido después no llegaba a la original.

### Seguridad
- Dependencias actualizadas por avisos de `cargo audit`: `rustls` 0.23.45,
  `rustls-webpki` 0.103.15, `lettre` 0.11.23, `crossbeam-epoch` 0.9.21 y
  `webbrowser` 1.2.4. Solo cambia `Cargo.lock`.
- El README y `crypto2` avisan de que `rsa_decrypt` y `rsa_sign` no son de
  tiempo constante (RUSTSEC-2023-0071, sin versión corregida).

### Añadido
- `bench/jit/`: intérprete contra JIT en micro-benchmarks, con tiempo, pico
  de RAM y comprobación de que la salida coincide (`run_jit.ps1`).

## v0.1.9 - 2026-10-03

### Cambiado
- **`secret`, reescrito para que un secreto no salga nunca en claro.** Todo
  valor leído con `secret.get` o `secret.require` se sustituye por `***` en
  `show`, `log`, los errores, el log de `serve` (también en la URL) y sus
  respuestas 500, y `env.all()` lo devuelve oculto. Se lee del entorno, de
  `NOMBRE_FILE` (secretos de Docker y Kubernetes) o de un `.env`.
  `secret.require(lista, { min_length })` comprueba todos y lista juntos los
  que faltan o son cortos. Con `ORION_ENV=production`, `secret.load()` rechaza
  el `.env` y `secret.get(nombre, defecto)` rechaza el valor por defecto.
  Nuevas: `secret.redact(texto)` y `secret.production()`. El `.env` admite
  `export`, comillas con escapes, valores entre comillas dobles de varias
  líneas (claves PEM) y comentarios al final de línea; un valor vacío cuenta
  como no definido. Módulo, opciones y mensajes en inglés.

### Corregido
- **Un error al inicializar un módulo detiene el programa.** Antes se
  ignoraba: el módulo quedaba sin sus variables y el fallo aparecía después
  como un "Attribute not found" sin relación. Ahora dice `loading module
  'backend/config.orx:11' failed: …` con el mensaje completo.
- **`secret.mask` ya no rompe el programa con "ñ" o "é"**: cortaba el texto
  por bytes.
- **`secret.all()` devolvía todos los secretos en claro**; ahora, enmascarados.

## v0.1.8 - 2026-10-03

### Añadido
- **`csv.headers(ruta, delimitador?)` lee solo la cabecera de un archivo.**
  Antes solo aceptaba filas ya cargadas, así que saber qué columnas trae un
  CSV de cientos de MB obligaba a leerlo entero. Respeta comillas y el BOM de
  UTF-8. Con una lista de filas funciona como siempre.
- **`mail.send(opciones)`: adjuntos, varios destinatarios y cualquier SMTP.**
  Con un dict: `servidor`, `puerto`, `seguridad` (`tls`, `starttls` o
  `ninguna`), `usuario`/`clave` opcionales, `de`, `para` (texto o lista), `cc`,
  `bcc`, `responder_a`, `asunto`, `texto` y `html` (los dos van como
  alternativas), `adjuntos` (rutas o `{ruta | base64, nombre, tipo}`, con el
  tipo deducido de la extensión) y `timeout` (30 s). Antes solo había TLS
  implícito, con credenciales obligatorias, un destinatario y sin adjuntos: no
  servía para mandar una factura ni para probar contra un buzón local. Cada
  opción vale también en inglés. La forma de 7 argumentos sigue igual.

### Cambiado
- **`net` tiene timeout y trata igual todos los códigos HTTP.** `get`, `post`,
  `put` y `delete` aceptan `opts` al final (`{ timeout: segundos }`, 30 por
  defecto); antes no había tope y un servidor que no contestaba colgaba el
  script o un hilo de `serve`. Un 4xx/5xx en `post`, `put` o `delete` vuelve
  como `{status, body, ok: no, headers}`, igual que en `get`, en vez de lanzar
  un error que perdía el código: un 409 y un 401 ya se pueden distinguir.

### Corregido
- **Un módulo puede usar sus propias funciones como valor.** `router.get(r,
  "/x", handler)` o `lista.map(doble)` dentro de un módulo daban "Variable no
  definida": la referencia se buscaba sin el prefijo del módulo. Ahora un
  módulo puede montar sus propias rutas sin conocer su nombre interno.
- **Un error dentro de un módulo importado señala el módulo.** Antes decía
  `main.orx:57` y enseñaba la línea 57 del programa, que no tenía nada que ver;
  ahora dice `backend/bd.orx:57` con su línea, también en módulos anidados, y la
  pila nombra el archivo de cada llamada (`at bd__crear_esquema
  (backend/bd.orx:57)`). En `serve`, el 500 de un handler lleva el archivo y la
  línea en el log, pero no en la respuesta: el cliente no ve cómo está
  organizado el servidor.

## v0.1.7 - 2026-10-02

### Añadido
- **`process.version()`, `process.uptime()` y `process.memory()`.** La versión
  de Orion que ejecuta el script, los segundos desde que arrancó el proceso y
  su memoria (`{rss, peak}` en bytes; `null` fuera de Linux y Windows). Sirven
  para que un servicio informe de sí mismo sin herramientas externas.

## v0.1.6 - 2026-09-30

### Añadido
- **`pdf.build(ruta, bloques, opts?)`: documentos libres a base de bloques.**
  `title`, `heading`, `text`, `table`, `fields`, `image`, `line`, `space` y
  `page_break`, con su nombre en español como alias. Un texto suelto en la
  lista es un párrafo. `pdf.report`, `pdf.create` y `pdf.template` pasan a ser
  atajos de lo mismo, y todos aceptan un `opts` final.
- **La maquetación de PDF se configura entera desde el código.** Página:
  `size` (A3, A4, A5, letter, legal o `[ancho, alto]`), `orientation`,
  `margin`, `header`, `footer`, `page_numbers`, `page_format`
  (`"{page}/{pages}"`), y metadatos `title`, `author` y `subject`. Tabla:
  `columns` por nombre o por posición (`width`, `align`, `format`, `bold`,
  `color`, `title`, `hidden`), `headers`, `font_size`, `header_bg`,
  `header_color`, `zebra`, `borders`, `total`, y los separadores `decimal` y
  `thousands`. Formatos numéricos por columna: `integer`, `decimal:3`,
  `money` (o `money:$`) y `percent:1`. Los colores admiten `#rrggbb`, `#rgb`,
  nombres o un gris de 0 a 1.
- **Imágenes en PDF.** Bloque `image` (PNG, JPEG, GIF, BMP) con `width`,
  `height` y `align`; la transparencia se funde sobre blanco. Se construye el
  objeto de imagen a partir de los píxeles para no arrastrar una segunda
  versión del crate `image` (la de `printpdf` es la 0.24 y Orion usa la 0.25).
- **Editar PDF existentes:** `pdf.merge`, `pdf.delete_pages`, `pdf.reorder`,
  `pdf.rotate`, `pdf.stamp` (texto con posición, tamaño, color, giro y
  opacidad, y `{page}`/`{pages}` para numerar), `pdf.stamp_image` (logo, firma
  o sello) y `pdf.set_info`. Las páginas se eligen con un número, una lista,
  un texto `"1-3,7"` o negativos desde el final (`-1` es la última).
  `pdf.info` devuelve también `pages` y el resto de metadatos, y lee bien los
  títulos con tildes, que se guardan en UTF-16.
- **`docs/paquetes.md`**: dónde viven los paquetes y los esquemas de
  `registry.json`, `installed.json`, `orion.json` y `orion.lock`, que antes
  estaban en comentarios de `pkg.rs` y `paths.rs`.

### Cambiado
- **Los comentarios del código fuente pasan a 2-3 líneas como máximo.** Había
  224 bloques de más de tres líneas en `orion-vm/src`, que contaban la
  historia de cada decisión entre línea y línea de código. Esa historia va al
  CHANGELOG y a los mensajes de commit. Quedan unas 1.300 líneas menos.
  Los comentarios que generan la ayuda del editor (`nombre(args) →
  descripción`, que lee `scripts/gen_builtins.js`) conservan esa línea de
  contrato.

### Corregido
- **El binario de Linux no arrancaba en Debian 12 ni en Ubuntu 22.04.** Se
  compilaba en `ubuntu-latest`, que hoy es 24.04 con glibc 2.39, y un binario
  enlazado contra glibc exige en destino una versión igual o más nueva:
  `orion: /lib/x86_64-linux-gnu/libc.so.6: version 'GLIBC_2.39' not found`.
  Quedaba fuera Debian estable, que es la base de media imagen de contenedor
  que se usa para desplegar. Ahora se compila en `ubuntu-22.04` (glibc 2.35),
  que funciona tanto en las distribuciones viejas como en las nuevas.

  Encontrado desplegando la demo `comercio` en contenedores, no en una
  revisión: el CI compila y prueba en el mismo Ubuntu donde publica, así que
  nunca se vio.
- **`--watch` ahora vigila también los archivos importados.** Sondeaba el
  `mtime` de un solo archivo, así que en un proyecto repartido en varios
  `.orx` tocar un módulo no recargaba nada: se guardaba, no pasaba nada, y uno
  acababa dudando de si el watch funcionaba. Los `use` se resuelven con el
  lexer (un `use` dentro de un comentario o de una cadena no cuenta), se
  siguen de forma recursiva, y la lista se recalcula en cada vuelta para que un
  `use` nuevo empiece a vigilarse sin reiniciar. Al arrancar dice cuántos
  archivos vigila: `Watching backend/main.orx + 4 imported`.

- **`--watch` reacciona al doble de rápido y gasta la mitad de nada.** El
  sondeo pasa a ser adaptativo: cada 120 ms mientras se está editando y cada
  800 ms tras minuto y medio sin cambios. La latencia al guardar baja de hasta
  480 ms a unos 220.

  Y la lista de archivos vigilados se calcula una vez, no en cada vuelta:
  rehacerla exige leer y lexar cada archivo, y medido costaba un **6,9% de un
  núcleo** de forma continua. Sondear solo los `stat` cuesta **0,31%**. La
  lista se rehace al detectar un cambio, que es cuando un `use` nuevo puede
  haber aparecido.

  Se valoró usar eventos del sistema de archivos (`notify`) y se descartó: el
  sondeo ya no se nota en CPU, la ganancia sería de latencia, y a cambio entra
  una dependencia nueva con su árbol, hace falta *debounce* porque un guardado
  son varios eventos, y en unidades de red o algunos montajes de contenedor
  los eventos no llegan (habría que mantener el sondeo igualmente como
  respaldo).

- **Los mensajes de `--watch` estaban en español** mientras el resto del CLI
  está en inglés: `change detected`, `Watching`, `server runs as a child
  process`.
- **`and` y `or` no cortocircuitaban.** Se evaluaban siempre los dos lados, así
  que `has_key(d, "k") and d["k"] > 0` reventaba con `Key 'k' not found`
  justo en el caso que la izquierda existía para evitar, y lo mismo con
  `x != null and x.algo`. En la demo `comercio` el login respondía 500 cuando
  el email no existía, que además delataba qué emails estaban registrados.

  Se compilan a tres instrucciones nuevas: `<a> JumpIfFalseOrPop fin <b>
  ToBool fin:` (`JumpIfTrueOrPop` para `or`), con el valor en la pila, como en
  CPython o Lua. Una primera versión pasaba el resultado por una variable
  oculta para no dejar valores vivos entre bloques del JIT, y costaba un
  **57 % más** en el intérprete. Con las instrucciones dedicadas, el mismo
  programa de 3 millones de iteraciones va un 7 % más rápido que antes del
  arreglo en el intérprete y un 5 % en el JIT: pesa más no evaluar la derecha
  que el salto extra. El JIT lleva el valor en una variable de Cranelift por
  punto de llegada, así que las funciones con `and`/`or` siguen compilando a
  nativo. Las instrucciones van al final del enum para no cambiar los índices
  de los `.orbc` ya compilados.
- **`attempt` no capturaba nada dentro de los handlers de `serve`.** El bucle
  que ejecuta los handlers propagaba el error sin mirar los `attempt` abiertos,
  así que un `json.parse` fallido dentro de un `attempt` salía como 500. Solo
  funcionaba si el `attempt` estaba en un módulo importado, porque ese camino
  pasaba por otro bucle.
- **Un `return` dentro de `attempt` dejaba su manejador vivo.** Un error
  posterior, ya fuera del `attempt`, saltaba a aquel `handle`.
- **Un middleware que fallaba respondía HTTP 200** con "error interno" como
  cuerpo. Ahora es un 500, igual que un handler.
- **Un módulo importado solo por otro módulo no llegaba a la VM.** Sus
  funciones se quedaban en la sub-VM donde se cargó el primer módulo, y
  llamarlas desde él fallaba con `Function 'cola__estadisticas' not found`
  (también bajo `serve`). En la demo `comercio` había que repetir el `use` en
  `main.orx` para que funcionara.
- **Los errores de Postgres llegaban como "db error" a secas.** El crate
  `postgres` solo imprime eso con `{}`; el mensaje del servidor va dentro. Un
  CHECK violado, una clave duplicada o una fila mala en un COPY eran
  indistinguibles. Ahora llegan con mensaje, detalle, pista, contexto (en un
  COPY, la línea del archivo) y SQLSTATE: `… (SQLSTATE 23505)`. Y
  `db.insert` ya no añade "is 'RETURNING id' missing?" a cualquier fallo.
  El checkout de `comercio` parecía funcionar porque `db.transaction` metía el
  SQL en el mensaje, y el SQL contenía las palabras que se buscaban.
- **`excel.write_styled` y `csv.write` ordenaban las columnas
  alfabéticamente**, y `csv.write` solo miraba las claves de la primera fila.
  Un informe "Fecha, Pedidos, Importe" salía "Fecha, Importe, Pedidos" sin
  forma de pedir otro orden. Ahora usan el orden en que se escribieron las
  claves, en todas las filas, como ya hacía `excel.write`.
- **`pdf.report` cortaba en silencio**: pintaba como mucho 4 columnas de 38
  mm, y al llegar al pie de la primera página dejaba de pintar filas. Un texto
  más ancho que su columna se montaba encima de la siguiente. Ahora el texto
  se mide con las métricas de Helvetica, pagina repitiendo la cabecera y
  recorta con "…" solo las columnas de texto: una cifra nunca se recorta.
  Las columnas numéricas se alinean a la derecha.
- **`pdf.create` escribía una sola línea en tamaño Carta** y sin codificar las
  tildes. Ahora es A4, parte el texto al ancho y pagina.
- **`pdf.watermark` borraba las fuentes de la página.** Si el diccionario de
  fuentes era una referencia (lo normal en los PDF que genera el propio
  Orion), lo sustituía por uno con solo la fuente de la marca: la marca salía
  y el texto del documento dejaba de verse. Además era opaca, tapaba lo que
  hubiera debajo, se centraba para Carta aunque la página fuese A4, y no
  codificaba las tildes ni el "€".
- **La documentación de `compile_entry` estaba pegada a `compile_repl`**, así
  que el editor mostraba la descripción de una en la otra.
- **`pdf` alineaba las fechas como cifras.** El detector de columnas numéricas
  aceptaba guiones en cualquier posición, y "2026-09-27" salía alineada a la
  derecha. Ahora el signo solo vale al principio.

### Tests
- **25 tests nuevos**, de 799 a **824**:
  - `logic_and_errors` (11): cortocircuito, precedencia, recursión, `and`/`or`
    anidados comparando intérprete y JIT, `return` dentro de `attempt` y
    módulos importados por otro módulo.
  - `serve_handlers` (1, con un servidor real): `attempt` en un handler del
    script principal, módulo anidado en un handler, middleware que falla y
    cortocircuito dentro de `serve`.
  - `documents` (12): paginación, columnas configurables, separadores propios,
    `pdf.build` con todos los bloques, edición de páginas, estampados, que la
    marca de agua conserve las fuentes (comprobado con lopdf), metadatos con
    tildes, y el orden de columnas en Excel y CSV.
  - `postgres_errors` (1): mensaje y SQLSTATE en CHECK, duplicado, `db.insert`
    y COPY. Se salta si no hay una base en `ORION_TEST_PG`.

## v0.1.5 - 2026-09-27

Una tanda de arreglos del módulo `browser`, todos encontrados reproduciendo el
fallo en vivo y todos con su test de regresión. Dos de ellos hacían que un
recorrido nocturno perdiera datos o dejara basura en la máquina, así que esta
versión importa para quien ya use `crawl`.

La suite pasa de 94 a **98 tests e2e** y de 76 a **81 unitarios**.

### Corregido
- **`crawl` perdía filas al reanudar.** El progreso marcaba una URL como
  terminada mientras sus filas seguían en el búfer de 8 KB del escritor CSV. Si
  el proceso moría ahí (un `kill`, el OOM killer, un cron con timeout), al
  reanudar se saltaban esas páginas y las filas no se recuperaban nunca: el
  recorrido terminaba diciendo `errors: []`. Ahora el CSV se vuelca a disco
  **antes** de anotar el progreso, así que lo peor que puede pasar es repetir
  una página. Reproducido matando el proceso a mitad: el progreso tenía 3 URLs
  y el CSV 0 bytes.

- **El navegador quedaba huérfano si mataban el proceso.** `with` lo cierra
  aunque el cuerpo falle, pero no puede hacer nada ante una muerte súbita, y
  cada pasada dejaba otro Chrome de cientos de MB. Ahora lo ata el sistema
  operativo: **job object** en Windows y `PR_SET_PDEATHSIG` en Linux, sin
  dependencias nuevas. En macOS no hay equivalente y se documenta.

- **`discover` proponía selectores inválidos con Tailwind.** Las clases con dos
  puntos (`md:flex`, `hover:shadow-lg`) volvían sin escapar, así que `.md:flex`
  no casaba con nada. Y no se veía: la muestra se calcula con los nodos ya
  encontrados, de modo que salía perfecta y `extract` devolvía una lista vacía
  en silencio.

- **`web.wait` no veía lo que aparecía dentro de un iframe o de una shadow
  root.** El `MutationObserver` solo vigila su propio documento. El elemento
  aparecía, `web.text` lo encontraba, y `wait` seguía dormido hasta agotar el
  plazo - justo en los dos sitios donde más se espera: un modal de cookies en
  iframe y un componente web. El observador sigue (es lo que da respuesta
  inmediata) y se le suma un sondeo, que llega donde él no entra.

- **Fuga de memoria en el transporte CDP.** Las respuestas que nadie esperaba
  (una por cada `Fetch.continueRequest`, es decir una por PETICIÓN de la página
  cuando hay `allow` o `route`) se quedaban para siempre en el mapa de
  respuestas. Ahora solo se guarda lo que alguien espera y el resto se descarta.

- **Riesgo de comandos CDP duplicados.** En tungstenite un `send` que devuelve
  `WouldBlock` ya ha encolado el mensaje; reintentar con `send` lo encolaba dos
  veces, y el navegador habría ejecutado el comando dos veces (dos clics, dos
  navegaciones). A partir del primer `WouldBlock` solo se vacía el búfer.

- **`force` dejaba la página tocada con shadow DOM.** El clic forzado vuelve
  transparente al puntero lo que estorba y luego lo restaura, pero la
  restauración no entraba en las shadow roots: la capa de un banner hecho como
  componente web (Usercentrics, OneTrust) se quedaba con `pointer-events: none`
  para siempre, y con una marca `data-orion-pe` que delata al scraper.

- **Dos tests e2e desfasados.** Desde que `__nombre` prefiere el `id` a la
  clase, el error dice `<div#velo>`; los tests seguían esperando el nombre de
  la clase. El aviso de "94 e2e verificados" de BROWSER.md no era cierto
  mientras tanto.

### Cambiado
- **La versión del paquete vuelve a cuadrar con el tag.** `v0.1.4` se publicó
  conteniendo `0.1.3`, así que el binario descargado decía una versión que no
  era. El workflow de release ahora **comprueba que el tag y `Cargo.toml`
  coinciden** y falla si no, para que no vuelva a pasar.

## 2026-08-23

### Añadido
- **`browser` entra en el shadow DOM.** Los selectores atraviesan las shadow
  roots abiertas a cualquier profundidad, igual que ya atravesaban los iframes.
  Un componente web guarda su contenido en una shadow root y el
  `querySelector` del documento no entra: el selector correcto "no existe" y no
  hay pista de por qué. Media web moderna es exactamente eso.

  Entra la búsqueda **y el clic**: el hit-test baja por las shadow roots,
  porque `elementFromPoint` devuelve el host y `host.contains(boton)` es false
  (`contains` no cruza la frontera), así que sin esto todo componente parecería
  tapado por sí mismo y `click` fallaría con un motivo imposible de entender.

  Las roots cerradas (`mode: 'closed'`) no son accesibles ni para el navegador:
  `exists` dice `no`, que es la respuesta honesta. El caso normal no paga el
  recorrido (3 ms en una página de 500 filas), y se apaga con
  `open({ shadow: no })`.

- **`browser.route` - intercepción de peticiones.** `watch`/`capture` miraban
  la red; ahora se puede decidir:

  ```orion
  web.route(p, "*/api/stock*", { mock: { status: 500, json: { "error": "caido" } } })
  web.route(p, "*.png",        { block: yes })
  web.route(p, "*/api/*",      { headers: { Authorization: "Bearer " + token } })
  web.route(p, "*/lento*",     { fail: "timedout" })
  ```

  Con esto se puede probar el camino de error sin tocar el servidor, trabajar
  con el backend a medias, quitarse de encima lo que no se mira, y autenticarse
  donde no hay formulario. `{ times: n }` dispara solo las n primeras veces, que
  es la única forma de comprobar que un reintento reintenta.

  Las reglas se prueban en orden y manda la primera que casa, como en un
  cortafuegos. `unroute` las quita y `routes` dice cuántas veces ha disparado
  cada una. La lista blanca de `open({ allow })` se comprueba **antes**: un
  `mock` no puede reabrir un dominio cerrado a propósito.

- **`browser.emulate` - dispositivo, idioma, zona horaria y ubicación.**
  Presets (`iphone`, `ipad`, `android`, `laptop`, `desktop`) que son un punto de
  partida, no una lista cerrada: cualquier campo se sobrescribe en la misma
  llamada. Sin esto no se pueden automatizar los sitios que sirven otro HTML al
  móvil, ni reproducir un fallo que depende de la zona horaria, ni evitar que el
  `Accept-Language` del contenedor de CI cambie los textos.

  Poner `geo` concede el permiso de geolocalización solo: sin ello la página
  recibe `PERMISSION_DENIED` y la posición emulada no llega a usarse nunca.

- **`browser.cookies` / `set_cookie` / `clear_cookies`**, para cuando
  `save_state`/`load_state` (la sesión entera) es demasiado.

- **`fn main()` se llama sola.** Un programa cuyo código entero vivía dentro de
  `main` terminaba con éxito, sin salida y sin aviso: el peor fallo posible,
  porque no se parece a un fallo. Ahora la llamada se añade si el programa
  define `main` y **no la nombra en ninguna parte** (ni a nivel superior, ni
  desde otra función, ni pasándola como valor), así que los programas que ya
  escribían `main()` a mano siguen ejecutándose una sola vez.

  Los módulos cargados con `use` no pasan por ahí: su `main` no debe correr al
  importarlos. El REPL tampoco. Un `main` con parámetros obligatorios no se
  puede llamar sin argumentos, y en vez de callarse lo dice.

### Cambiado
- **Los mensajes de error están en inglés**, como el resto del lenguaje. Eran
  ~920 cadenas repartidas por el núcleo (VM, value, named args, pkg, JIT/AOT),
  el módulo `browser` entero (incluido el JavaScript que se inyecta en la
  página) y la librería estándar. La traza de pila (`at f (line 3)`) y el
  prefijo que el renderizador de errores parsea cambiaron con ellas.

  Quedan en español los nombres de los alias obsoletos (`db.insertar`,
  `cache.guardar`), que son nombres y no texto, y el catálogo de documentación
  que alimenta el hover de la extensión.

## 2026-08-20

### Añadido
- **`SPEC.md` - especificación del lenguaje, y es ejecutable**: 11 secciones
  derivadas del compilador (lexer, parser, typechecker, VM), no de la memoria.
  Cubre estructura léxica, comentarios, identificadores, keywords, literales,
  precedencia completa de 14 niveles, el desugar de `|>`, resolución de
  nombres, valores función, y semántica de evaluación.

  Lo que la hace distinta de un documento: `tests/spec_examples.orx` +
  `tests/spec_conformance.rs` ejecutan **45 afirmaciones** con valor exacto, y
  el paso está en CI. Si el compilador cambia, el fallo no dice "algo se rompió",
  dice `power_asocia_derecha: SPEC dice '512', el compilador da '64'`.

  Escribirla desmintió tres cosas que se daban por ciertas: `**` asocia a la
  derecha, `type()` de una función nombrada devuelve `string` (una función
  nombrada **es** el string de su nombre, y `greet == "greet"` es `yes`), y la
  notación exponencial sí existe. También dejó fijado que `null` y `undefined`
  son el mismo valor, que `/` es división real, que el overflow es error, y que
  `for .. in` no itera dicts.

### Añadido
- **`orion check` avisa de los alias españoles obsoletos**, con el nombre inglés
  que los sustituye:

  ```
  !  [deprecated] line 1 - use "formato" uses a deprecated Spanish module name;
                           write use "format" instead - see SPEC.md section 11.
  !  [deprecated] line 6 - db.insertar() is a deprecated Spanish alias of
                           db.insert(). It still works, but it is scheduled for
                           removal - see SPEC.md section 11.
  ```

  Sin esto, la retirada anunciada en SPEC.md §11 era una trampa: el usuario se
  enteraría el día que su programa dejara de compilar. Los avisos van con `kind`
  propio (`deprecation`, no `warning`) y se enseñan **siempre**, con o sin
  `--types`; esconder una deprecación tras un flag es no avisar. `orion run` no
  los muestra, para no dar la lata en cada ejecución.

  La tabla vive en `src/deprecated.rs`, **144 entradas curadas a mano**. No se
  deriva del registro a propósito: el registro marca todos los alias, pero
  `log.warn` comparte brazo con `log.info` sin estar obsoleto, y
  `state.increment` es alias inglés de `state.incr`. Avisar de esos sería
  decirle al usuario que su código está obsoleto cuando no lo está. `df` y
  `embeddings` tampoco están: son abreviaturas inglesas deliberadas.

  `tests/deprecated_sync.rs` comprueba que cada entrada siga existiendo en el
  registro, que su destino inglés exista (un aviso que manda a un nombre
  inexistente es peor que no avisar) y que la tabla esté ordenada, porque la
  búsqueda es por bisección.

### Cambiado
- **Toda la salida del CLI está en inglés**: ayuda, banner, `doctor`, `check`,
  `fmt`, `test`, `watch`, `bench`, `build`, `docs`, `new` y el debugger
  interactivo. Antes el comando que enseñaba las deprecaciones en inglés las
  rodeaba de español (`Verificando:`, `sin errores`), que era la peor mezcla
  posible.

  Fuera de esta tanda a propósito: `builtins.rs` y `builtins_gen.rs`, que no son
  salida del CLI sino **documentación de la stdlib** (352 entradas generadas
  desde los comentarios de contrato de los módulos). Se traducen junto con los
  671 mensajes de `modules/`, que es el mismo eje.

- **Los mensajes de error del núcleo están en inglés**: lexer, parser, codegen,
  typechecker, VM y las cinco etiquetas de `error.rs` (`lexical error`,
  `syntax error`, `compile error`, `type error`, `runtime error`). Es lo primero
  que ve alguien que escribe mal una línea, y hasta ahora le contestaba en
  español aunque el lenguaje se anunciara en inglés.

  ```
  antes:  error léxico   → Comentario inválido '//'. Usa '--' para comentarios
  ahora:  lexical error  → Invalid comment '//'. Use '--' for comments
  ```

  Ojo con dos que no son lo que parecen y se tradujeron a mano: `Módulo por
  cero` y `Módulo solo soporta enteros` hablan del operador `%`, no de un módulo
  de la stdlib. Quedan **671 mensajes en `modules/`** sin traducir, que es la
  siguiente tanda.

### Arreglado
- **Una lambda dentro de una interpolación `${...}` no compilaba**:
  `show "${apply(fn(x) { return x + 1 }, 10)}"` moría con
  `Función '__lambda_2__' no definida`. El compilador junta los cuerpos de
  lambda generados en un vector `extra_fns` que sube hasta quien registra las
  funciones; `compile_sub_expr`, que compila el trozo de dentro de `${}`,
  se creaba uno **local** y lo descartaba al salir. La llamada quedaba emitida
  y su destino no existía nunca.

  El fallo solo aparecía dentro de un string, así que
  `xs.map(fn(x) { return x * 2 })` funcionaba y
  `"${xs.map(fn(x) { return x * 2 })}"` no. Ahora `extra_fns` se enhebra por
  `compile_interpolated` y `compile_sub_expr`. Cubierto por
  `regression::lambda_dentro_de_interpolacion_se_registra`, que verifica también
  dos lambdas en la misma interpolación, una anidada dentro de otra, la forma
  de flecha y un método con lambda inline.


### Cambiado
- **La API pública de Orion es inglesa**: el inglés pasa a ser la forma canónica
  de la stdlib, y los nombres españoles pasan a **alias obsoletos**. En esta
  versión no se ha renombrado ni eliminado nada: `db.insertar`, `cache.guardar`
  o `validate.requerido` siguen funcionando y lo harán durante toda la 0.1.x.
  Pero quedan fuera de la superficie estable y **está previsto retirarlos en una
  versión futura**; en código nuevo va el inglés. Lo que cambia ya es cuál
  documenta el registro, y con él el hover, el autocompletado,
  `orion --builtins-json` y la referencia del sitio.

  Se aplicó reordenando los nombres dentro de cada brazo del `match` (en Rust el
  orden es indiferente, pero `scripts/gen_builtins.js` toma el primero como
  principal) y volteando el comentario de contrato, que es de donde sale la
  firma documentada. Alcance: **115 brazos** en 20 módulos y **101 comentarios**.

- **Cuatro módulos tenían nombre español sin alternativa** y ahora responden
  también en inglés: `task`/`tarea`, `queue`/`cola`, `format`/`formato`,
  `graph`/`grafo`. Requiere mantener sincronizados cuatro sitios: el dispatch y
  `is_known_module()` en `modules/mod.rs`, `canonical_module()` en el
  typechecker y `canonical()` en `tests/builtins_registry_sync.rs`.

- **Coherencia dentro del propio inglés**: `stream.where` (antes `where_`),
  `stream.zip_lists` (antes `zip_`), `frame.where` (nuevo, `where_` no tenía
  alternativa), `proto.encode_base64`/`decode_base64` (antes solo la forma
  abreviada `_b64`) y `matrix.rot_2d` (antes solo `rot2D`, camelCase en una API
  snake_case). Todas las formas anteriores siguen vivas como alias.

  Se dejan a propósito dos cosas que parecen incoherencias y no lo son:
  `excel_f.if_` lleva guion bajo porque `if` es keyword y `excel_f.if(...)` no
  parsearía; y `quantum.gate_H`/`gate_CNOT` van en mayúscula porque es la
  notación de la física, y además son funciones distintas de `h`/`cnot` (unas
  devuelven la matriz de la puerta, las otras la aplican).

### Roto
- **`type(t)` sobre una tarea devuelve `"task"`, antes `"tarea"`**. Es el único
  cambio de esta tanda que no se puede aliasar, porque un string devuelto no
  tiene alias. Era el único tipo de runtime con nombre español entre diez
  ingleses (`int`, `float`, `string`, `bool`, `list`, `dict`, `ptr`, `null`,
  `fn`, `module`). Un programa que compare `type(t) == "tarea"` debe
  actualizarse.

### Tests
- Suite completa en verde tras el cambio: **421 tests, 0 fallos** (unit,
  regression, differential VM-JIT, typecheck, modules_smoke, concurrency,
  packages_resolution, readme_examples_parse y builtins_registry_sync).
- `tests/test_infra.orx` y `tests/test_utilidades.orx` se dejan **en español a
  propósito**: son la cobertura de regresión que demuestra que los alias siguen
  vivos.

## 2026-08-08

### Arreglado
- **`orion build` - las funciones no veían las variables globales (P0)**: el
  compilador nativo daba a cada función únicamente variables locales de
  Cranelift, así que un nombre definido fuera de ella llegaba como `null`. Solo
  afectaba al **ejecutable compilado**; `orion run` nunca estuvo mal, ni
  siquiera con miles de llamadas calientes, porque ahí manda la VM.

  Lo peligroso era la forma del fallo. A veces se caía y a veces no:

  ```orion
  IVA = 0.21
  fn con_iva(base) { return base * (1 + IVA) }
  show con_iva(100)         -- orion run: 121   |   .exe: otro resultado
  ```

  Y como `use "modulo"` define un global, **cualquier llamada a un módulo dentro
  de una función** moría con `[JIT] CallMethod: tipo no soportado (tag=0)`, un
  mensaje que no apuntaba a la causa: el receptor era el `null` del global que
  no se encontró. En la práctica ningún programa real compilaba, porque todos
  envuelven su lógica en funciones.

  Ahora el runtime del JIT tiene una tabla de globales: el nivel superior
  publica al asignar (y al hacer `use`), y una función lee de ahí los nombres
  que no son suyos. La regla de qué es local se conserva igual que en la VM
  (parámetros, lo que la función asigna, y los campos del shape en el cuerpo de
  un `act`), así que asignar dentro sigue creando una variable propia sin tocar
  el global. La tabla es de proceso, no por hilo, para que una tarea lanzada con
  `spawn` vea lo mismo que el resto.

- **`orion build` - un programa con `fn main()` no compilaba nativo**: el objeto
  generado comparte espacio de nombres con el `main` de C que arranca el
  ejecutable, así que el símbolo se declaraba dos veces con firmas distintas
  (`i64` contra `i32`), la compilación nativa se abortaba y caía al modo
  bytecode embebido. El binario funcionaba, pero **ninguna aplicación real
  llegaba a compilarse nativa**, porque `fn main()` es la forma natural de
  escribirlas. Los símbolos de usuario ahora se prefijan en AOT; el nombre de
  Orion se conserva para el registro en tiempo de ejecución.

- **`orion build` - un diccionario salía con las claves invertidas**: los pares
  de un literal salen de la pila al revés que en el código, y la VM los voltea
  para conservar el orden de escritura. El JIT decía replicar al intérprete y se
  saltaba justo ese paso, así que `{zeta: 1, alfa: 2}` se convertía en
  `{alfa: 2, zeta: 1}` **solo en el ejecutable compilado**. De ese orden dependen
  cosas que se ven: el JSON generado, las columnas de un CSV, lo que imprime un
  `show` - y el esquema de `browser.extract`, que es un literal y hacía salir los
  registros con los campos al revés. Dos tests nuevos en `differential.rs`.

### Tests
- `aot_native.rs`: seis casos nuevos que cubren el hueco por el que se colaron
  los dos defectos anteriores - un global leído dentro de una función (número,
  cadena y namespace de módulo), que una asignación local no pise el global, que
  el valor visto sea el del momento de la llamada, y que `fn main()` compile
  nativo. La batería anterior solo probaba programas autocontenidos
  (aritmética, recursión, shapes, cadenas), y por eso nadie se enteró.


## 2026-07-15

### Añadido
- **Lenguaje - `with` (recursos con ámbito)**: nueva sintaxis
  `with h = modulo.abrir(...) { ... }` que garantiza `modulo.free(h)` al salir
  del bloque, **también si el cuerpo lanza un error** (se libera y el error se
  re-lanza, capturable por un `attempt` exterior). Funciona con cualquier
  módulo que tenga `free` (frame, serie, quantum…) y con handles string o int
  (conoce el módulo estáticamente, no adivina por el handle). Reglas del
  parser: el inicializador debe ser `modulo.fn(...)`, y `return`/`break`/
  `continue` que escaparían del bloque sin liberar se rechazan en compilación
  con un mensaje claro (los loops internos del cuerpo sí pueden usar break).
  Implementado por desugar a `attempt/handle` en codegen - el JIT hereda la
  semántica sin cambios porque compila desde el mismo bytecode. Soporte
  completo en typechecker (sin falsos positivos), `orion fmt` y resaltado de
  la extensión VSCode.
- **frame - gestión del store**: `frame.free(handle)` (libera un frame; las
  transformaciones crean frames nuevos que antes vivían para siempre - la misma
  fuga que ya se arregló en `serie`) y `frame.frames()` (frames vivos en
  memoria). Imprescindibles en procesos largos (`serve`).
- **Tests**: `tests/test_frame.orx` - barrido funcional e2e del motor de datos
  columnar con valores exactos (17 tests / ~85 checks): inferencia de tipos,
  keep/drop/rename, where_ por tipo, head/tail/sort, estadísticas (std
  poblacional, percentiles interpolados), group, add_col, roundtrips CSV y
  .odf, autodetección de formato, from_txt, scan_stats, each_chunk, salidas
  Excel/odf streaming, free/frames. +1 test de regresión en modules_smoke.

### Arreglado
- **break/continue (P0)**: estaban rotos en TODO el lenguaje - codegen emitía
  `Jump(0)` que nunca se parcheaba, así que `break` y `continue` saltaban a la
  instrucción 0 (reinicio del programa o de la función) y el loop se volvía
  infinito. Ningún test los ejercitaba; lo destapó el barrido de `with`. Ahora
  codegen mantiene una pila de contextos de loop y parchea break → fin del
  loop y continue → re-evaluación de condición (while) o paso de incremento
  (for). `break`/`continue` fuera de un loop son error de compilación con
  mensaje claro. +9 tests de regresión y +3 diferenciales VM/JIT.
- **VM - handlers huérfanos**: un `return` dentro de `attempt` se saltaba el
  `EndAttempt` y su handler quedaba vivo en la pila de errores; un error
  posterior en el caller saltaba a una dirección de otra función (el programa
  podía "terminar" en silencio en vez de reportar). Al morir un frame se
  descartan ahora sus handlers pendientes. +2 tests de regresión.
- **frame.each_chunk**: exigía un tercer argumento `fn` que el código nunca
  llamaba (los scripts pasaban un dummy). Firma real ahora:
  `each_chunk(ruta, chunk_size = 10_000)` → lista de handles, un frame por
  bloque; los argumentos extra se ignoran por compatibilidad.

### Validado
- **GC - ciclos huérfanos**: el fix del 2026-07-11 verificado e2e con el
  binario release: 200k y 1M de ciclos de listas (`push(a,a)`), closures
  (env→lista→closure→env) e instancias (`a.next=b, b.next=a`) → RAM pico
  plana (~11 MB, igual que el control sin ciclos; antes del fix 200k ciclos
  fugaban ~79 MB).

## 2026-07-14

### Añadido
- **GUI - animación y dibujo libre**: `gui.tick(ms)` (evento periódico que
  re-ejecuta el script; los clics tienen prioridad y el reloj se apaga si el
  script deja de pedirlo) y `gui.canvas(w, h) … gui.end()` con formas
  genéricas `circle`, `line`, `rect`, `arrow`, `text_at` (colores temables).
- **Demo**: `demo/demo_bloch_anim.orx` - esfera de Bloch animada con física
  real: cada tick rota el estado cuántico 6° y redibuja desde `q.bloch()`.
- **Typeshed automática**: el generador cubre módulos-directorio (`gui`,
  `tui`) → 875 funciones en 58 módulos, y `build.rs` la regenera en cada
  build (imposible que el hover del LSP quede desfasado del código).

### Arreglado
- **CLI (P1)**: `orion run app.orx` no registraba la ruta del script, por lo
  que toda GUI lanzada con `run` quedaba estática (los eventos no
  re-ejecutaban el script). Solo la invocación directa `orion app.orx`
  activaba el modo reactivo.

## 2026-07-13

### Añadido
- **quantum - simulador de circuitos real**: `circuit(n)` hasta 24 qubits con
  puertas por qubit en O(2^n) (nunca se construye la matriz 2^n×2^n), después
  paralelizadas con rayon (GHZ-20: 11 ms). Puertas `h/x/y/z/sgate/tgate`,
  paramétricas `rx/ry/rz/phase`, multi-qubit `cnot/cz/cphase/swap/ccx`, y
  `ugate`/`cugate` para puertas definidas por el usuario (con validación de
  unitariedad). Medición: `probs`, `sample` (regla de Born), `collapse`
  (mide un qubit y colapsa), `state`, `reset`, `free`.
- **Demo**: `demo/demo_grover.orx` - búsqueda de Grover en Orion puro
  (P(101) = 0.9453125, el valor teórico exacto) y
  `demo/demo_quantum_lab.orx` - laboratorio interactivo de 1 qubit.
- **matrix - álgebra lineal numérica**: motor nalgebra a partir de 32×32
  (mul tipo BLAS, LU con pivoteo; 512×512 ≈ 10× más rápido). Funciones
  nuevas: `solve` (sistemas lineales), `eig` (valores propios),
  `svd` ({u, s, vt}), `rank`, `norm`.
- **serie**: `free(handle)` y `count()` - las transformaciones acumulaban
  handles sin forma de liberarlos en procesos largos.

### Arreglado
- **matrix**: `det` pasó de cofactores O(n!) a eliminación gaussiana con
  pivoteo O(n³) (11×11: 7 s → 1.2 ms); llamadas sin argumentos hacían panic
  de la VM entera (ahora error controlado).
- **quantum**: `qubit(a_re, a_im, b_re, b_im)` ignoraba sus argumentos y
  devolvía siempre |0⟩; ahora los honra, normaliza y rechaza el estado nulo.
- **stat**: `correlation` devolvía 0.9999999999999998 en correlación
  perfecta (ruido f64); ahora redondea a 1e-12.
- **csv**: `stats` interpola percentiles linealmente (mediana de [1,2,3,4]
  = 2.5, consistente con `serie`).
- **fs**: `rmdir` es idempotente (no-existe → `no` en vez de error; permisos
  y bloqueos sí se reportan).

### Validado
- Barrido funcional e2e con valores exactos (~240 checks, 0 fallos) de:
  `matrix`, `serie`, `zip`, `json`, `template`, `csv`, `stat`, `vector`,
  `grafo`, `quantum`. Todos hacen trabajo real (serde, minijinja, deflate,
  petgraph con A*, similitud coseno, Pearson/OLS, vector de estados
  cuántico con interferencia de fases genuina).

## 2026-07-12

### Añadido
- **crypto/crypto2**: AES-256-GCM real (antes XOR), derivación de clave con
  Argon2id + salt (formato versionado con compatibilidad hacia atrás), MD5
  real para checksums, comparación en tiempo constante.
- **Extensión VSCode**: hover estilo Pylance para módulos y `use`, typeshed
  completa autogenerada, `show` multi-argumento, formatter + `fmt --check`,
  lint de indentación engañosa.
- **orion watch**: reinicia servidores (`serve`) como proceso hijo estilo
  nodemon; GUI con hot reload in-process.

### Arreglado
- **ai**: `chat_start(system)` descartaba el system prompt; `set_model()` no
  afectaba a `think/learn/sense` (tenían HTTP propio con un modelo retirado);
  `status()` ahora devuelve dict. Defaults de modelo movidos a alias sin
  fecha (`claude-haiku-4-5`).
