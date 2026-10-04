# Backlog

Cosas encontradas y no arregladas todavía, con el motivo por el que importan.
Lo que se arregla sale de aquí y entra en [`CHANGELOG.md`](CHANGELOG.md).

## Dependencias con avisos de seguridad (`cargo audit`, 2026-10-03)

Quedan 7 avisos tras actualizar las compatibles (ver CHANGELOG). Ninguno
alcanzable hoy, salvo `rsa`:

- **`rsa` 0.9** (media, Marvin): canal lateral de tiempo al descifrar o
  firmar. Sin versión corregida; avisado en el README y en `crypto2_mod.rs`.
  Afecta a `crypto2.rsa_decrypt` / `rsa_sign` en un servidor expuesto.
- **`lopdf` 0.31** (alta): desbordamiento de pila al **leer** un PDF muy
  anidado. Llega por `printpdf` 0.6, que solo lo usa para escribir; Orion lee
  con el `lopdf` 0.42 directo. Para quitarlo hay que pasar a `printpdf` 0.12,
  cuya API es otra: reescribir `pdf_layout.rs` (936 líneas). Encaja con la
  entrada "PDF: la fuente la tiene que elegir quien escribe el documento".
- **`quick-xml` 0.30 y 0.39**: solo en las dependencias de GUI de Linux
  (wayland, zbus) y en tiempo de compilación.
- **`rkyv` 0.7**: dependencia opcional de `rust_decimal` que no se compila.

Repasar `cargo audit` antes de cada release.

## JIT: los objetos del heap no se liberan

Los números ya no van al heap y la aritmética va en línea (ver CHANGELOG):
`bucle_int` pasó de 2,06 s y 1241 MB a 0,13 s y 12 MB. Queda:

- **Los strings, listas, dicts e instancias nunca se liberan.** Un bucle que
  construye strings o listas temporales sigue perdiendo memoria.

Plan (cada paso se mide con `bench/jit/run_jit.ps1` contra el binario
anterior en `orion-vm/target/base/`):

1. ~~Valores inmediatos~~ (hecho, 2026-10-03).
2. ~~Ruta rápida en línea~~ (hecho, 2026-10-04).
3. ~~Valores vivos entre bloques~~ (hecho, 2026-10-04; el ternario ya
   compila).
4. **Liberar objetos del heap** (strings, listas): conteo de referencias o un
   arena por llamada.

Afecta a `jit/compiler.rs`, `jit/runtime.rs`, `jit/runtime_oop.rs`,
`jit/bridge.rs` y al backend AOT (`--build`), que comparte el generador.

## JIT: los builtins menos usados copian la lista y crean una VM

`rt_call_builtin` (`jit/bridge.rs`) convierte todos los argumentos a `Value`,
crea un `VM::new` por llamada y, si el builtin muta la lista, la vuelve a
convertir entera reservando valores nuevos. `len`, `push`, `pop`, `first` y
`last` ya van directos (`builtin_directo`); el resto (`sort`, `reverse`,
`contains`, `slice`, `join`, `sum`...) sigue costando O(n) por llamada y
dejando memoria sin liberar. Camino: llevarlos a `builtin_directo` según
aparezcan en bucles calientes, y no crear una VM por llamada.

## Un canal no se entera de que su productor murió

El error de una tarea `spawn` ya se escribe en stderr (ver CHANGELOG), pero
si el productor falla antes de `chan.cerrar`, el consumidor sigue esperando
en `chan.recibir` para siempre: el programa queda colgado, ahora al menos
con el error a la vista. Camino: que un canal cuyo productor muere se cierre
con ese error y `chan.recibir` lo lance.

## `append(lista, x)` no se puede escribir

`append` es palabra reservada (`append "ruta" with texto`), así que el
builtin `append(xs, v)` da error de sintaxis aunque la VM y el JIT lo
implementan. Solo funciona `xs.append(v)`. O se quita el builtin, o el parser
acepta `append(` como llamada.

## El intérprete busca cada variable por nombre

`orion archivo.orx` usa el intérprete. Un bucle de 10 millones de sumas tarda
4,5 s (`bench/jit/bucle_int.orx`, unos 45 ns por instrucción), frente a 1,7 s
del JIT; CPython hace el mismo bucle en torno a 1 s (estimación, falta
medirlo junto a los demás). Lo que queda en `vm.rs`:

- Las variables locales viven en un `IndexMap` por nombre: cada `LoadVar` y
  `StoreVar` calcula un hash del nombre, y `LoadVar` clona el `Value`.
- Cada llamada crea un `IndexMap` nuevo para las locales y copia el nombre de
  la función y de cada parámetro.
- `Value` guarda `Dict(IndexMap)` sin `Rc`, así que es grande y cada push y
  pop de la pila mueve muchos bytes.

Camino: resolver las locales a índices de slot en `codegen`
(`LoadLocal(u16)` / `StoreLocal(u16)` sobre un `Vec<Value>` del frame), con
cuidado con lo que hoy lee `vars` por nombre (closures, `act` y
`sync_to_instance`, el depurador, el REPL). Medir con `bench/jit/run_jit.ps1`
contra el binario anterior (`target/base/`).

Ya hecho (2026-10-03, ver CHANGELOG): cuerpos como `Arc<[Instruction]>`,
despacho por referencia y lotes de instrucciones calientes. Desde el inicio:
bucle de enteros 9,8 s → 4,5 s y `fib(30)` 5,8 s → 2,4 s (sesiones
distintas, orientativo).

## Los tests de `browser_e2e` fallan al azar con el equipo cargado

Con la batería completa, 1 a 3 tests distintos en cada pasada fallan por tiempo
(`Page.navigate: no response within 30000 ms`) y pasan al ejecutarlos solos.
Pasó al validar la v0.1.9: la batería tardó 341 s en vez de 160.

El 2026-10-03 pasó en las tres baterías completas seguidas (1 o 2 tests,
cada vez distintos, siempre `Page.navigate` a los 30 s): ya no es ocasional.
El 2026-10-04 llegaron a 5 en una sola batería (344 s en vez de unos 160).

Un test que falla al azar acaba ignorándose y entonces oculta un fallo real.
Camino: menos navegadores a la vez por defecto, y plazos de los tests ligados al
tiempo que tarde en arrancar el primero, no fijos.

## `db.transaction` no deja decidir dentro de la transacción

Recibe una **lista fija** de sentencias y las ejecuta todas: no hay forma de
leer un valor a mitad y ramificar en Orion. Un checkout del tipo "mira el
stock, y si no alcanza aborta" no se puede escribir tal cual.

En la demo `comercio` se resolvió apoyándose en un `CHECK (stock >= 0)` que
aborta la sentencia, lo cual además es mejor diseño. Pero no todo caso se deja
expresar en SQL puro: por ejemplo, cobrar a una pasarela externa entre dos
escrituras.

Camino razonable: una transacción interactiva con `db.begin(url)` que devuelva
un handle y permita `query`/`exec`/`commit`/`rollback` sobre esa misma
conexión, dejando la lista de sentencias como atajo para el caso sencillo.

## En macOS el navegador sobrevive a un `kill`

`browser` ata el navegador a la vida del proceso con un *job object* en
Windows y `PR_SET_PDEATHSIG` en Linux. macOS no tiene equivalente directo, así
que ahí un proceso muerto de golpe sigue dejando el navegador vivo, que es
justo lo que esta función existe para evitar.

Camino posible: un hilo que vigile al padre por `kqueue` (`NOTE_EXIT`) y mate
al navegador, o que el proceso hijo compruebe periódicamente si su padre
cambió de PID.

## PDF: la fuente la tiene que elegir quien escribe el documento

Todo lo que genera `pdf` (report, build, create, template, stamp, watermark)
usa **Helvetica, fija en el código**. Es una de las 14 fuentes base de PDF, y
por eso no hace falta incrustar nada, pero tiene dos consecuencias:

- El documento no puede llevar la tipografía de una marca ni de un cliente.
  Una factura o un informe corporativo salen siempre con la misma letra.
- Helvetica base solo cubre Latin-1 (WinAnsi). Chino, árabe, cirílico,
  griego o un simple "≥" no se pueden escribir: salen como "?".

Lo que hace falta: que el developer elija la fuente por opciones, por
documento y por bloque. Por ejemplo `{ "font": "fuentes/Inter.ttf" }` para
una TTF/OTF propia, que se incrusta subconjuntada (solo los glifos usados,
para que el archivo no pese la fuente entera), o el nombre de una de las 14
base (`"Times-Roman"`, `"Courier"`). Con variantes para negrita y cursiva
(`font_bold`, `font_italic`) y un valor por defecto que siga siendo Helvetica.

Hay que tocar dos cosas que hoy dependen de la fuente fija:

- **La medición del texto.** `pdf_layout.rs` mide con las tablas AFM de
  Helvetica (`HELV`, `HELV_BOLD`). Con una fuente propia hay que medir con sus
  anchos reales (tabla `hmtx` de la TTF), o las columnas, la alineación a la
  derecha y los recortes con "…" saldrán mal.
- **La codificación.** Hoy el texto va en WinAnsi. Una TTF incrustada necesita
  una fuente Type0 con codificación Identity-H y un mapa ToUnicode, para que
  se pueda escribir cualquier carácter y para que copiar el texto del PDF
  siga funcionando.

`printpdf` 0.6 ya sabe incrustar TTF (`add_external_font`); el subconjuntado
está detrás de su feature `font_subsetting`.

## PDF: lo que todavía no se puede hacer con un PDF existente

Hoy se puede unir, quitar, reordenar, rotar y extraer páginas, estampar
texto e imágenes encima, poner marca de agua, cambiar metadatos y extraer el
texto. Falta, por orden de utilidad:

1. **Formularios (AcroForm).** Leer los campos de un PDF y rellenarlos:
   solicitudes, contratos, impresos oficiales. Es lo más pedido en la
   práctica.
2. **Fuentes propias** (ver la entrada anterior).
3. **Dividir en varios archivos y recortar páginas** (`split`, CropBox).
   `paginate` solo saca un rango a un archivo.
4. **Cifrado:** proteger con contraseña y abrir un PDF protegido.
5. **Enlaces, marcadores (índice) y anotaciones.**
6. **Comprimir las imágenes** de un PDF que pesa demasiado.
7. **Firma digital** con certificado. Una imagen de firma se puede estampar
   ya, pero no tiene validez legal.
8. **Cambiar o borrar texto que ya está en el PDF.** Lo más difícil: un PDF no
   guarda párrafos, sino trozos de texto colocados en coordenadas, a menudo
   con fuentes recortadas a los glifos que usaban. Aunque se haga, será
   limitado; para corregir un documento suele ser mejor regenerarlo.

## Principio: el diseño lo decide el developer, Orion lo aplica

Todo módulo que genere algo visible (PDF, Excel, correo, gráficos,
imágenes) tiene que dejar que el developer elija **colores, fuentes, tamaños,
márgenes, bordes, espaciados y textos fijos**, y Orion tiene que aplicarlos
tal cual. Un valor por defecto está bien para que lo sencillo salga en una
línea; un valor que no se pueda cambiar, no. Si algo se ve de una forma, tiene
que haber una opción para que se vea de otra.

Hoy no se cumple del todo. Valores fijos en el código:

- **pdf** (`pdf_layout.rs`, `pdf_edit.rs`): la fuente (Helvetica, ver más
  abajo); el color y el tamaño del pie, del encabezado y del número de página
  (gris 0.45, 7.5 pt); el color y el grosor de las rayas de la tabla (gris
  0.35) y de los bordes; el tamaño y el color del subtítulo de `report` (9.5
  pt, gris 0.4); los tamaños por defecto de título (18 / 16) y encabezado
  (13), que se pueden cambiar por bloque pero no para todo el documento; el
  interlineado de `fields`; y el relleno interior de las celdas.
- **excel** (`excel_mod.rs`): la cabecera de `write` y `write_multi` (azul
  0x2D5F8A con texto blanco, sin opción); el ancho de columna de 18; el color
  de las filas alternas (0xF2F7FC) y de la fila de totales (0xE0E0E0); el
  tamaño del título (14 pt); la palabra **"TOTAL"**, fija y en español, en la
  fila de totales; y la línea de meta de los gráficos (rojo 0xE74C3C). Las
  paletas de gráficos (`"orion"`, `"ocean"`…) sí se pueden elegir, y también
  una lista de colores propia.
- **Textos fijos en un idioma**: "Página {page} de {pages}" en PDF (se puede
  cambiar con `page_format`) y "TOTAL" en Excel (no se puede). Todo texto que
  Orion pinte por su cuenta tiene que poder cambiarse, porque el documento
  puede ir en cualquier idioma.

Además de exponer cada valor, conviene un **tema**: un dict con los colores,
fuentes y tamaños de la marca, que se define una vez y se pasa a cualquier
documento (`{ "theme": marca }`) en vez de repetir las mismas diez opciones
en cada llamada.

## Documentos con diseño: un vocabulario de estilo común

Cada módulo que genera archivos describe el diseño a su manera. En `excel`
los colores de cabecera van en español dentro de `cabecera`
(`{ "fondo": …, "texto": … }`); en `pdf`, como `header_bg` y
`header_color`. Quien aprende uno no sabe usar el otro, y una misma tabla no
se puede mandar a Excel y a PDF con la misma configuración.

Lo que hace falta: los mismos nombres y valores en `pdf`, `excel` y lo que
venga: `font`, `size`, `bold`, `italic`, `color`, `background`, `align`,
`border`, `format` (`"money"`, `"percent:1"`…), y `columns` con la misma
forma. Una tabla con su `columns` debería ir a Excel o a PDF sin tocarla.
Los nombres en español siguen como alias, igual que en el resto del lenguaje.

La fuente elegida por el developer (entrada anterior) forma parte de esto:
vale para PDF y para Excel.

## Excel: los estilos solo llegan a una hoja

`excel.write_styled` tiene casi todo (colores, formato numérico, formato
condicional, totales, gráficos…), pero escribe **una sola hoja**.
`excel.write_multi` escribe varias, pero **sin ningún estilo**: ni formato de
moneda. Un informe real tiene varias hojas (resumen, detalle por día, por
producto…), y hoy tiene que elegir entre estructura y formato. Es lo que le
pasa al informe de ventas de la demo `comercio`.

Camino razonable: que `write_multi` acepte, por hoja, la misma configuración
que `write_styled`.

## HTML y CSS en el backend: plantillas, PDF y correo

En Java es habitual diseñar un PDF o un correo con HTML y CSS (Thymeleaf o
FreeMarker para la plantilla, OpenHTMLtoPDF o Flying Saucer para el PDF,
JavaMail para el envío). Orion tiene las tres piezas; el correo ya está
(`mail.send` con opciones, v0.1.8), las otras dos a medias:

- **Plantillas (`template`).** Usa minijinja, así que tiene la sintaxis de
  Jinja (`{{ }}`, `{% for %}`, `{% if %}`, filtros). Pero cada plantilla se
  registra con el nombre `"t"`, sin extensión, y minijinja decide el escape
  automático por la extensión: **no se escapa nada**. Un nombre de cliente
  con `<script>` acaba tal cual dentro del HTML de un correo o de un PDF.
  Además, al ser una plantilla suelta, `{% extends %}` e `{% include %}` no
  encuentran a las demás, así que no hay layout común (cabecera y pie de
  todos los correos, por ejemplo).
- **HTML a PDF.** Existe `browser.pdf`, que usa la impresión de Chrome, pero
  hace falta abrir un navegador, escribir el HTML a un archivo y navegar a
  él: no hay un `pdf.from_html(html, ruta, opts)` de un paso. Y exige Chrome
  instalado en el servidor, que en una imagen Docker mínima no está.
- **Correo (`mail`).** Resuelto en `mail.send(opciones)`: varios
  destinatarios, CC y CCO, adjuntos, texto y HTML alternativos y cualquier
  SMTP. Falta solo **imágenes incrustadas** (`cid:`, el logo dentro del
  cuerpo), que hoy obligan a enlazar la imagen desde fuera.

Lo que hace falta, por orden:

1. `template` con escape automático en HTML y una carpeta de plantillas
   (para `extends` / `include`).
2. Imágenes incrustadas en `mail.send` (`inline: [{ruta, cid}]`).
3. `pdf.from_html(html, ruta, opts)`: de HTML a PDF en un paso. Primero sobre
   Chrome, que ya está. Un motor de HTML/CSS propio en Rust, sin navegador,
   es mucho más trabajo, y solo compensa si pesa no poder instalar Chrome en
   el servidor.

## Los errores salen con colores ANSI aunque no haya terminal

El intérprete pinta sus errores con secuencias de color (`[31;1m…`)
siempre, también cuando stderr va a un archivo, a una tubería o a otro
programa. No mira si la salida es una terminal ni respeta `NO_COLOR`.

Quien lee la salida desde otro programa recibe texto con basura: el
playground de la documentación (`Orion-Documentation/playground-api`) tiene
que avisar en su README de que `stderr` "llega con secuencias de color ANSI"
y cada cliente web tiene que limpiarlas. En los logs de un contenedor pasa lo
mismo.

Lo que hace falta: color solo si stderr es una terminal
(`std::io::IsTerminal`, que `banner.rs` ya usa para stdout), y nunca si
`NO_COLOR` está definida (la convención de no-color.org). Opcionalmente,
`ORION_COLOR=always` para forzarlo.

## `serve` lee el cuerpo entero de la petición a memoria

Antes de llamar al handler, `serve` hace `read_to_end` del cuerpo y además
una copia como texto (`from_utf8_lossy`), y si es multipart otra más al
volcar cada archivo a un temporal. Una subida de 80 MB cuesta así varias
veces su tamaño en RAM antes de que el código del developer la vea.

`db.copy_file` carga CSV enormes en streaming con RAM constante, pero si el
CSV llega por HTTP esa ventaja se pierde en la puerta. En la demo `comercio`
la importación por el panel se limita a 50 MB por esto, y lo grande va por la
línea de órdenes.

Camino razonable: para multipart, escribir cada parte al temporal según va
llegando, y no construir `body` como texto cuando el cuerpo es binario.

## Dos módulos con el mismo nombre de archivo se pisan

Las funciones de un módulo se registran como `<nombre de archivo>__<fn>`.
`use "a/util"` y `use "b/util"` producen los mismos nombres (`util__x`), y el
segundo pisa al primero sin avisar. Pasa igual con los módulos que importa
otro módulo. El prefijo debería salir de la ruta completa, no solo del
nombre del archivo.

## `db.transaction` mete el SQL entero en el mensaje de error

Cuando una sentencia falla, el error incluye el texto completo del SQL. Con
sentencias largas el mensaje se hace ilegible, y en un log puede acabar
información que no debería. El mensaje de Postgres ya dice qué falló; del SQL
bastaría con la posición en la lista (`sentencia 3 de 5`).

## Tres formas documentadas de ejecutar un archivo

`orion archivo.orx`, `orion run archivo.orx` y `orion --run archivo.orx`
funcionan las tres, y cada documento usa una distinta: el README de
`examples/orion-tasks-api` usa la primera, `tests/browser_e2e.rs` la segunda y
la ayuda del CLI la tercera.

No es un fallo, pero quien llega nuevo no sabe cuál es la buena. Conviene
elegir una como canónica en el README y la ayuda, y dejar las otras como
aliases sin documentar.

## Los ejemplos están en español y el lenguaje en inglés

El README dice que Orion se escribe en inglés y que los nombres en español son
aliases obsoletos, pero `examples/orion-tasks-api` está escrito y comentado en
español. Para un repositorio público conviene decidir una cosa u otra.
