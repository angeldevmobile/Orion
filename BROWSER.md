# Orion `browser` — referencia

Automatización web sobre CDP (Chrome DevTools Protocol). Sin driver externo, sin
`chromedriver`, sin dependencias nuevas: el módulo usa el `tungstenite`
síncrono y el `serde_json` que Orion ya lleva dentro.

```orion
use "browser" as web

with b = web.open() {
    p = web.page(b)
    web.goto(p, "https://example.com")
    show(web.title(p))
}
```

`with` desugara a `web.free(b)` incluso si el cuerpo lanza un error, y `free`
cierra en cascada las pestañas del navegador. No quedan procesos huérfanos.

> **Estado**: transporte, arranque, navegación, interacción, modales, ventanas,
> extracción y archivos verificados de punta a punta (41 tests e2e en
> [`orion-vm/tests/browser_e2e.rs`](orion-vm/tests/browser_e2e.rs), contra
> servidor local). **Cero constantes fijadas**: todo lo que decide el
> comportamiento se puede cambiar desde `open()` — ver 1.2. Pendiente:
> cookies/sesión y benchmark completo contra Python.

## 1. Arranque

### 1.1 `web.open(opts?)` → navegador

Localiza el navegador en cascada, **sin nada fijado en el código**:

1. `opts.chrome` — ruta explícita
2. `ORION_CHROME` — variable de entorno
3. Detección automática: Chrome, Chromium, Brave o Edge

En Windows importa que acepte Edge: viene instalado de fábrica, así que no hay
nada que descargar.

**Orion no descarga ningún navegador.** Usa el que ya tienes. Si no hay ninguno
basado en Chromium, `open()` falla diciendo cuáles sirven y cómo indicar la ruta.

Lo que sí desaparece por completo es `chromedriver`: CDP habla directamente con
el navegador, así que no hay un segundo binario cuya versión haya que mantener
sincronizada. Que Chrome se actualice solo deja de ser un problema.

El endpoint se descubre por dos vías, porque no todos los navegadores dan las
dos: Chrome lo anuncia por su salida de error y además escribe
`DevToolsActivePort` en el perfil; **Edge solo escribe el archivo**.

```orion
b = web.open({
    chrome:   "C:/ruta/chrome.exe",  -- opcional
    headless: yes,                   -- por defecto yes
    images:   no,                    -- por defecto NO se descargan
    gpu:      no,
    width:    1280,
    height:   800,
    timeout:  30000,                 -- ms, arranque del navegador
    user_data: "C:/perfil",          -- perfil propio (persiste sesiones)
    args:     ["--proxy-server=x:1"],   -- banderas extra, van al final
    sin:      ["--disable-extensions"]  -- banderas por defecto a quitar
})
```

`args` **añade** y `sin` **quita**. Hacen falta las dos: en Chrome una bandera
posterior no siempre revierte a la anterior, así que sin `sin` un sitio que
necesitara extensiones no tenía forma de deshacer `--disable-extensions`.
Se quita por nombre, sin repetir el valor: `sin: ["--blink-settings"]`.

**Las imágenes vienen desactivadas por defecto.** Son el grueso del consumo de
memoria y de red de una página, y casi ningún scraper las necesita. Se
reactivan con `images: yes`, obligatorio para capturas fieles.

Sin `user_data` se crea un perfil temporal que se borra al cerrar. Con
`user_data` el perfil es tuyo y no se toca: es la forma de conservar sesiones
entre ejecuciones.

### 1.2 Afinado

Nada del motor está fijado en el código. Los parámetros se agrupan en dos
niveles según lo que sean:

**Política** — decisiones sobre *tu* problema, en la raíz de las opciones:

| Opción | Default | Qué controla |
|---|---|---|
| `wait` | 10000 | espera de acciones y lecturas, en ms |
| `retry` | 50 | cada cuánto se reintenta dentro de la página |
| `cdp_margin` | 5000 | margen del plazo de transporte sobre el de espera |
| `drag_steps` | 10 | pasos intermedios de un arrastre |
| `force_layers` | 12 | capas superpuestas que `force` atraviesa |
| `iframe_depth` | 8 | profundidad de iframes anidados que se recorre |
| `hit_inset` | 24 | margen en píxeles al probar puntos de un elemento |
| `nav_settle` | 5000 | cuánto se tolera que la página esté cambiando de documento |

**Mecanismo** — uso de recursos, bajo `tuning` para no ensuciar la API diaria:

| Opción | Default | Qué controla |
|---|---|---|
| `max_events` | 512 | eventos CDP retenidos (más historial es más RAM) |
| `idle_poll` | 5 | techo del sondeo en reposo (subirlo baja la CPU) |
| `close_timeout` | 2000 | plazo de las operaciones de cierre |
| `send_timeout` | 5000 | plazo para que un envío progrese |
| `cleanup_tries` | 12 | intentos de borrar el perfil temporal |
| `stale_profile_mins` | 60 | edad a partir de la cual se barre un perfil abandonado |

```orion
b = web.open({
    wait: 4000,
    drag_steps: 25,
    tuning: { max_events: 64, idle_poll: 20 }
})
```

El `wait` tiene **tres niveles**, del más concreto al más general: lo que diga
la llamada, lo que se fijó al abrir, y el default.

```orion
web.text(p, "#tarde", 6000)   -- manda sobre el wait del navegador
```

### 1.3 `web.page(navegador)` → pestaña

### 1.4 `web.free(handle)` / `web.close(handle)`

Vale para navegador o pestaña; es el nombre que invoca `with`.

### 1.5 `web.pages(navegador)` → lista de handles

### 1.6 `web.info()` → diccionario de diagnóstico

Qué navegador se usaría, de dónde sale y cuántos hay abiertos. Sin esto, un
"no me funciona" es indepurable.

```orion
show(web.info())
-- {found: yes, path: C:\...\chrome.exe, env: , open_browsers: 0, in_use: [], open_pages: 0}
```

## 2. Navegación

| Función | Devuelve |
|---|---|
| `web.goto(pestaña, url)` | la URL final |
| `web.title(pestaña)` | título |
| `web.url(pestaña)` | URL actual |
| `web.content(pestaña)` | HTML completo |

`goto` espera a que la página cargue. Si no carga porque un `alert` la dejó
congelada, lo dice con el texto del diálogo en vez de un timeout genérico.

## 3. Selectores

**Uno solo**, deducido del propio texto. No hay `find_by_xpath` y `find_by_css`.

| Forma | Significa |
|---|---|
| `.card > button` | CSS |
| `//li[@data-n='2']` | XPath (empieza por `//` o `(//`) |
| `text=Comprar` | por texto visible |

La variante por texto existe porque la mayoría de los XPath que escribe la gente
son para buscar por contenido, y salen frágiles e ilegibles.

**Los selectores atraviesan los iframes accesibles**, que es donde viven casi
todos los modales de consentimiento de cookies. Los iframes de otro origen se
saltan sin romper la búsqueda.

## 4. Interacción

| Función | Nota |
|---|---|
| `web.click(pestaña, sel, opts?)` | |
| `web.dblclick(pestaña, sel, opts?)` | |
| `web.rightclick(pestaña, sel, opts?)` | |
| `web.hover(pestaña, sel, ms?)` | |
| `web.drag(pestaña, origen, destino, ms?)` | con pasos intermedios, para `dragover` |
| `web.scroll(pestaña, dx, dy)` | rueda del ratón |
| `web.type(pestaña, sel, texto, opts?)` | limpia el campo salvo `{ clear: no }` |
| `web.press(pestaña, tecla)` | ver 4.3 |
| `web.select(pestaña, sel, opción, ms?)` | `<select>` nativo, ver 4.4 |

El tercer argumento admite un número (milisegundos de espera) o un diccionario:

```orion
web.click(p, "#ir", 3000)                    -- espera hasta 3 s
web.click(p, "#ir", { wait: 3000, force: yes })
```

### 4.1 Espera implícita, siempre

Ninguna acción exige acordarse de poner un `wait`. `click` y compañía esperan a
que el elemento sea **accionable**: existe, ocupa espacio, no está oculto por
estilo, y nadie lo tapa. Un scraper que depende de que el programador recuerde
esperar es un scraper que falla de forma intermitente.

El bucle de reintento vive **dentro de la página**, así que todo esto sigue
siendo una única llamada CDP.

### 4.2 Elementos tapados

Hay tres situaciones y cada una tiene su respuesta:

| Situación | Comportamiento |
|---|---|
| Tapado temporal (spinner, banner que se va) | espera y clica |
| Tapado parcial (cabecera fija sobre media mitad) | clica por la zona libre |
| Tapado permanente (banner de cookies) | falla nombrando al culpable |

El tapado parcial se resuelve probando **nueve puntos** dentro del rectángulo
del elemento (centro, cuatro desplazamientos, cuatro esquinas) en vez de solo el
centro, que es lo que hacen las demás herramientas. Es lo que haría una persona:
pinchar donde se ve.

Cuando de verdad no se puede, el error identifica lo que estorba:

```
browser.click '#total': lo tapa <div.cookie-banner> (tras esperar 1200 ms)
  Si el elemento que estorba no se va a quitar, usa: { force: yes }
```

Con `{ force: yes }` se atraviesa. **No se clica a ciegas en las coordenadas**
—eso es como Selenium acaba pulsando el banner en lugar del botón—: se vuelve
transparente al puntero lo que estorba, el clic sigue siendo un evento real del
navegador, y después se restaura todo (incluso si el clic falla).

No es el comportamiento por defecto a propósito: atravesar un modal suele
significar saltarse algo que el sitio te está pidiendo, y eso produce sesiones
raras que fallan tres pasos después.

### 4.3 Teclas

`enter`, `tab`, `escape`, `backspace`, `delete`, `space`, `up`, `down`, `left`,
`right`, `home`, `end`, `pageup`, `pagedown`.

`web.type` manda **tecla a tecla**, no asigna `value` desde JavaScript: React,
Vue y compañía solo se enteran del cambio si llegan los eventos de teclado, y un
`value` puesto a mano queda ignorado al enviar el formulario.

### 4.4 `<select>` nativo

Un `<select>` abre un desplegable **del sistema operativo**, fuera del DOM:
ningún clic puede navegarlo. `web.select` asigna la opción y emite `input` y
`change` como haría el navegador.

Acepta el `value`, **el texto visible** o el índice, porque quien escribe el
scraper ve el texto en pantalla:

```orion
web.select(p, "#pais", "México")   -- por texto
web.select(p, "#pais", "mx")       -- por value
web.select(p, "#pais", "1")        -- por índice
```

Si la opción no existe, el error lista las que hay.

## 5. Lectura del DOM

| Función | ¿Espera? |
|---|---|
| `web.text(pestaña, sel, ms?)` | sí |
| `web.texts(pestaña, sel, ms?)` | sí |
| `web.html(pestaña, sel, ms?)` | sí |
| `web.attr(pestaña, sel, atributo, ms?)` | sí |
| `web.exists(pestaña, sel)` | **no** |
| `web.count(pestaña, sel)` | **no** |
| `web.visible(pestaña, sel)` | **no** |
| `web.wait(pestaña, sel, ms?)` | espera explícita |

La regla: **lo que devuelve contenido espera; lo que informa del estado no.**

Devolver `null` porque el contenido aún no había llegado convierte un problema
de tiempo en un dato perdido en silencio — el fallo que hace que un scraper
funcione en el portátil y no en el servidor. Al revés, hacer esperar a `exists`
convertiría un "no está" legítimo en diez segundos de bloqueo.

## 6. Modales y ventanas

### 6.1 `web.dialogs(pestaña, política)`

Para `alert`, `confirm` y `prompt`:

```orion
web.dialogs(p, "accept")          -- acepta
web.dialogs(p, "dismiss")         -- rechaza
web.dialogs(p, "answer:Orion")    -- responde un prompt
web.dialogs(p, "off")             -- deja de atenderlos
```

Se declara **una vez** y vale para la sesión. Playwright obliga a registrar el
handler *antes* de cada acción que pudiera abrir uno, y eso falla cuando el
diálogo lo lanza un temporizador de la página: no hay ninguna llamada tuya a la
que engancharlo.

Un diálogo sin atender **congela la página** sin dar ningún error, que es el
peor fallo posible. Por eso lo atiende el propio hilo lector de CDP.

### 6.2 `web.click_opens(pestaña, sel, opts?)` → pestaña nueva

Un clic que abre una pestaña te devuelve su handle, ya cargada:

```orion
factura = web.click_opens(p, "#ver-factura")
show(web.title(factura))
```

Playwright necesita envolver el clic en `expect_popup`; Selenium te hace listar
handles de ventana y adivinar cuál es la nueva.

### 6.3 Modales HTML

No necesitan nada especial: son HTML. Además el fondo bloqueante se comporta
bien — con el modal abierto, un clic fuera falla nombrando al culpable en vez de
colarse por debajo.

## 7. Extracción

### 7.1 `web.extract(pestaña, selector_de_fila, esquema, opts?)` → lista

El esquema es un diccionario de campo a especificación, y **todo él se compila a
una sola llamada** que corre dentro de la página.

```orion
esquema = {
    id:     "@data-id",
    nombre: ".title",
    precio: ".price|num",
    stock:  "[data-qty]@data-qty|int",
    url:    "a@href",
    hay:    ".disp|bool"
}
items = web.extract(p, ".card", esquema)
```

```
{id: 1, nombre: Laptop Pro, precio: 1299,  stock: 7,  url: /p/1, hay: yes}
{id: 2, nombre: Mouse,      precio: 24.99, stock: 0,  url: /p/2, hay: no}
```

Ahí está la diferencia de fondo con Selenium: allí **cada lectura de un atributo
es una petición HTTP al driver**, así que 500 productos por 3 campos son unas
1.500 idas y vueltas más las 500 de localizar las filas. Esto es una. Y como se
usa `returnByValue`, lo que cruza el socket son los datos pedidos, no el HTML.

`extract` espera a que haya filas antes de rendirse: el listado suele llegar
después de la acción que lo pidió, y devolver una lista vacía convertiría un
problema de tiempo en un resultado vacío silencioso.

### 7.2 Gramática de una especificación

Las tres partes son opcionales: `<selector> @<atributo> |<conversión>`

| Ejemplo | Significa |
|---|---|
| `.price` | texto del elemento |
| `a@href` | atributo de un descendiente |
| `@data-id` | atributo de la propia fila |
| `.price\|num` | texto convertido a número |
| `//td[2]\|num` | XPath **relativo a la fila** |
| `\|num` | el texto de la fila entera, como número |

Conversiones: `num`, `int`, `bool`, `html`, `text`, `trim`.

Dos detalles que evitan errores silenciosos:

**Los XPath se relativizan.** `//td[1]` es absoluto y buscaría desde la raíz del
documento, devolviendo **la misma fila repetida** con datos que parecen buenos.
Como una especificación de campo describe por definición algo dentro de la fila,
se convierte en relativo.

**Los números entienden los dos formatos.** `1.299,00 €` y `$1,234.56` conviven
en la misma página. Manda el separador que aparece más a la derecha; con una
coma sola, es decimal si separa uno o dos dígitos finales y miles si no. Un
valor no numérico como `Agotado` da `null`, no un número inventado.

### 7.3 Selectores muertos

Un campo vacío en **todas** las filas casi nunca es un dato ausente: es un
selector equivocado, o el sitio que cambió de estructura. Callarlo devuelve una
lista que parece buena y revienta cien líneas después — el fallo clásico de
BeautifulSoup.

```
browser.extract: 2 campo(s) no encontraron nada en ninguna de las 3 filas:
    precio  ←  .precio-viejo
    sku  ←  @data-sku
  Revisa esos selectores, o usa { strict: no } si de verdad pueden faltar.
```

Con `{ strict: no }` se acepta y esos campos vienen como `null`.

### 7.4 `web.extract_to(pestaña, urls, selector, esquema, salida, opts?)` → resumen

Recorre varias URLs y **vuelca a disco según extrae**.

```orion
r = web.extract_to(p, urls, ".card", esquema, "productos.csv")
show(r)
-- {rows: 8000, urls: 40, ok: 40, failed: 0, empty: [], files: [productos.csv], errors: []}
```

Dos decisiones deliberadas: se **reutiliza una sola pestaña** para todas las URLs
(abrir una por página multiplica la memoria del navegador) y **no se acumula el
listado** antes de guardar, que es lo que hace que un scraper de Python se coma
la RAM en cuanto el volumen crece.

Medido con 200 filas por página:

| Páginas | Filas | Pico de RAM de Orion |
|---|---|---|
| 5 | 1.000 | 18,4 MB |
| 40 | 8.000 | 18,9 MB |

Ocho veces más datos, medio megabyte más. La medida es del proceso de Orion; la
memoria del navegador va aparte y es la grande, por eso las imágenes vienen
desactivadas.

El límite honesto: la memoria queda acotada por **la página más grande**, no por
el total del recorrido, porque cada página se extrae de una vez.

**Formatos**, según la extensión:

- `.csv` — se escribe fila a fila, un solo archivo, memoria constante de verdad.
- `.odf` — el formato binario lleva el número de filas en la cabecera y no admite
  añadir al final, así que se vuelca por bloques (`chunk`, 50.000 por defecto)
  liberando cada uno. El primero conserva el nombre pedido y los siguientes se
  numeran. Lo lee `frame` directamente, con los tipos ya inferidos:

```orion
h = fr.open("productos.odf")
show(fr.schema(h))    -- {id: int, nombre: string, precio: float, stock: int}
```

**Una URL que falla no aborta el recorrido.** En una tanda de veinte, morir por
un 404 tira el trabajo de las diecinueve buenas: se anota en `errors` y se sigue.

**Una página que carga pero no da filas se reporta en `empty`.** Un 404 con
plantilla, un redirect al login o un selector que dejó de valer en esa sección
cargan bien y no producen nada. Sin esto, un recorrido pierde páginas en
silencio y nadie lo nota hasta que faltan datos en el informe.

## 8. Archivos

Las tres cosas que el navegador **delega en el sistema operativo**: elegir un
archivo para subir, guardar uno que se descarga, e imprimir. Las tres abren una
ventana nativa que está fuera del DOM, así que ningún clic ni ninguna tecla la
alcanza. Es donde se atasca la automatización web de verdad.

Aquí no se maneja ninguna de esas ventanas: **se impide que existan**. CDP
permite interceptar las tres antes de que el navegador se las pida al sistema,
así que nada de esto depende del idioma del Windows, de la resolución, ni de
que haya escritorio. Funciona igual en headless y en un servidor sin pantalla.

### 8.1 `web.upload(pestaña, selector, archivos)` → rutas absolutas

```orion
web.upload(p, "#adjunto", "contrato.pdf")            -- uno
web.upload(p, "#adjunto", ["a.pdf", "b.pdf"])        -- varios
```

El selector puede apuntar a dos cosas distintas, y las dos funcionan:

1. **El propio `<input type="file">`.** Se le asignan los archivos y ya está.
2. **Cualquier cosa que abra el selector al pulsarla** — el botón "Examinar",
   una zona de arrastrar y soltar, un `<label>`. El `<input>` real suele estar
   oculto tras el diseño del sitio y a veces ni siquiera es alcanzable con un
   selector.

El caso 2 es el que no cubre Selenium: su receta es `send_keys` sobre el input,
que exige que el input exista y sea alcanzable. Aquí se activa la interceptación,
se pulsa, y cuando el navegador anuncia que iba a abrir la ventana se le contesta
con los archivos. La ventana no llega a aparecer.

Las rutas relativas se resuelven contra el directorio del programa, no contra el
del navegador, que es otro proceso y está en otro sitio. Se devuelven las
absolutas ya resueltas porque sin verlas es imposible entender por qué el
navegador dice que un archivo que existe no existe.

**Un archivo que no existe se dice antes de tocar la página.** El navegador
acepta en silencio una ruta inventada: el formulario se envía sin adjunto y el
fallo aparece mucho después, en el servidor de otro.

```
browser.upload: el archivo 'contrato.pdf' no existe
  se buscó en: C:\trabajo\facturas\contrato.pdf
```

### 8.2 `web.download(pestaña, selector, opts?)` → dict

```orion
d = web.download(p, "#descargar", { dir: "facturas" })
show(d)
-- {path: C:\trabajo\facturas\factura-042.pdf, name: factura-042.pdf, bytes: 51234, url: https://...}
```

Descargar con un navegador automatizado tiene dos problemas, no uno:

**El diálogo "Guardar como"**, que se evita fijando el comportamiento de descarga
antes de pulsar.

**Saber cuándo ha terminado.** El navegador escribe primero un archivo temporal
`.crdownload` y lo renombra al acabar. Sin un aviso, la receta habitual es dormir
unos segundos y cruzar los dedos: si la red va lenta se lee un archivo a medias,
y si va rápida se pierde el tiempo. Aquí se espera el evento de finalización, así
que la llamada vuelve exactamente cuando el archivo está entero — y `bytes` lo
confirma.

| Opción | Qué hace |
|---|---|
| `dir` | Carpeta destino. Se crea si no está. Por defecto, la del programa. |
| `name` | Renombra al terminar. Por defecto, el que proponga el servidor. |
| `overwrite` | Permite pisar un archivo existente. Por defecto **no**. |
| `wait` | Plazo, en ms, para archivos grandes. |

**Dos descargas con el mismo nombre no se pisan.** La segunda queda como
`informe (2).txt` y la ruta real viene en `path`. Sobrescribir en silencio es lo
que hace perder una tanda entera de facturas sin que nadie se entere hasta el
cierre del mes; se pide explícitamente con `{ overwrite: yes }`.

**Un elemento que no descarga lo dice**, en vez de quedarse esperando:

```
browser.download: pulsar '#ver' no inició ninguna descarga en 10000 ms.
  Comprueba que el elemento sea el que descarga, y no un enlace que abre el
  archivo en una pestaña.
```

### 8.3 `web.pdf(pestaña, ruta, opts?)` → ruta

```orion
web.pdf(p, "justificante.pdf", { margin: 0.4, landscape: no })
```

No es una captura: es el documento entero paginado y con el texto seleccionable.
Para guardar un justificante o una factura de un portal web es lo que hace falta,
y es justo lo que obliga a pelearse con el diálogo de impresión si se hace a mano.

Opciones: `landscape`, `background`, `headers`, `scale`, `width`, `height`,
`margin`, `pages`. Las medidas van en pulgadas, que es la unidad del navegador —
un A4 son 8,27 × 11,69. Lo que no se indique lo decide el navegador con el mismo
default que aplicaría el diálogo.

El fondo se imprime por defecto, al revés que en el diálogo: el navegador lo
quita para ahorrar tinta, y en un PDF que nadie va a imprimir eso solo hace que
las tablas con filas alternas salgan en blanco.

## 9. Captura

`web.screenshot(pestaña, ruta)` → escribe un PNG y devuelve la ruta.

Requiere `images: yes` en `open` si quieres que salgan las imágenes.

## 10. JavaScript

`web.eval(pestaña, js)` evalúa y devuelve el valor ya convertido a Orion.

```orion
n = web.eval(p, "document.querySelectorAll('.card').length")
```

Una excepción del JavaScript se convierte en error de Orion, no en un `null`
silencioso.

## 11. Memoria

Decisiones tomadas con el consumo como criterio, no como consecuencia:

- **El DOM nunca cruza el socket.** Toda evaluación usa `returnByValue`: vuelve
  el valor pedido, no una referencia ni el HTML. BeautifulSoup se trae la página
  entera al proceso y construye un árbol de objetos encima; aquí la memoria es
  proporcional a los datos que pediste, no al peso de la página.
- **Una llamada por consulta.** En Selenium cada lectura de un atributo es una
  petición HTTP al driver. Toda la lectura de Orion se resuelve dentro de la
  página, en una evaluación.
- **Historial de eventos acotado** a 512. Un navegador activo emite miles por
  minuto y nadie los consume; sin tope, una sesión larga se come la RAM en un
  historial inútil.
- **Imágenes desactivadas por defecto**, más las banderas que apagan
  sincronización, extensiones y red de fondo.
- **Las pestañas se cierran de verdad** al hacer `free`: es lo que libera la
  memoria del proceso de render.

## 12. Arquitectura

```
orion-vm/src/modules/browser/
├── mod.rs      API pública y registro de handles
├── cdp.rs      transporte: WebSocket, multiplexado por id, bus de eventos
├── dom.rs      selectores, espera y accionabilidad
├── input.rs    ratón y teclado por el dominio Input de CDP
└── launch.rs   localización y arranque del navegador
```

Sobre un único socket viajan mezcladas las respuestas (llevan `id`) y los
eventos (llevan `method`). Un hilo lector por conexión reparte cada respuesta a
quien la espera, que duerme en una `Condvar` — el mismo parking que usa `await`
en `task_pool`, sin introducir un segundo modelo de concurrencia.

Los eventos de ratón y teclado se despachan por el dominio `Input` de CDP, que
los inyecta en la misma capa por la que entran los del usuario. Y la posición se
vuelve a medir **inmediatamente antes** de cada despacho, no al empezar una
cadena de acciones: ahí está la diferencia práctica con `ActionChains`, que
entre localizar y clicar deja que la página mueva el elemento.

## 13. Despliegue

### 13.1 Qué entregas

```powershell
orion --build app.orx -o app.exe
```

**Un solo archivo.** `--build` no empaqueta el intérprete al lado: compila tu
programa a nativo con Cranelift y lo enlaza contra el runtime de Orion como
librería estática. El resultado no es un lanzador que busca `orion.exe`, es un
ejecutable de verdad con el runtime dentro.

Verificado ejecutándolo en una carpeta que contenía **solo** `app.exe`, sin
ningún `orion.exe` cerca y con el `PATH` reducido a `C:\Windows\system32`.

Tu usuario recibe `app.exe` y no necesita saber que Orion existe.

### 13.2 Qué necesita la máquina del usuario

**Un navegador basado en Chromium, y nada más.** En Windows ya está: Edge viene
con el sistema. Si su instalación está en una ruta poco habitual, se resuelve
sin recompilar con la variable `ORION_CHROME` o pasando `chrome:` en `open()`.

### 13.3 Comparado con Python

| | Python + Selenium | Orion |
|---|---|---|
| `chromedriver.exe` | hay que entregarlo, y de la versión correcta | **no existe** |
| Runtime | Python instalado, o PyInstaller | dentro del `.exe` |
| Dependencias | selenium + webdriver-manager + transitivas | ninguna |
| Archivos a entregar | carpeta o instalador | **uno** |
| Cuando Chrome se actualiza | rebajar el driver, reempaquetar, redistribuir | **nada** |

La última fila es la que más cuesta en la práctica: en Python cada actualización
de Chrome obliga a volver a empaquetar. Aquí el ejecutable que entregaste hace
seis meses sigue funcionando.

### 13.4 Redes corporativas

Este es el escenario donde la diferencia deja de ser comodidad y pasa a ser
"puedo o no puedo".

`webdriver-manager` **descarga chromedriver** desde dominios de Google en tiempo
de ejecución. En una red corporativa eso choca con tres cosas a la vez: el
egreso suele estar bloqueado (y seguridad no whitelistea la descarga de
ejecutables), PyPI está cerrado o tras un espejo interno, y el problema se
repite en cada actualización que empuja el departamento de sistemas.

El módulo `browser` **no hace ni una llamada de red propia**. Lo único que abre
es un WebSocket a `127.0.0.1`. Comprobado con un scraper contra una intranet
local sin salida a internet en ningún momento.

Ventajas concretas:

- **Funciona sin egreso** salvo hacia el sitio que automatizas.
- **Usa el navegador que la empresa ya administra** — Edge en un Windows
  corporativo está instalado y gestionado por política, no hay que aprobar nada.
- **CI determinista**: desaparece el paso de "bajar el driver", fuente clásica
  de fallos intermitentes ajenos a tu código.
- **Una sola cosa que auditar**: un binario, en vez de un árbol de dependencias
  que se resuelve en tiempo de instalación.

El proxy corporativo se indica como a cualquier otra herramienta, y llega al
navegador:

```orion
web.open({ args: ["--proxy-server=http://proxy.empresa:8080"] })
```

### 13.5 Lo que conviene saber

**Tamaño.** El ejecutable ronda los 58 MB. Es el binario completo de Orion:
lleva GUI, TUI, tres motores de base de datos, OCR con sus modelos… todo, se use
o no. Hoy no hay forma de adelgazarlo.

**Runtime de C.** El binario enlaza el CRT de MSVC de forma dinámica, así que
depende de `vcruntime140.dll`, presente en cualquier Windows moderno. No está
probado compilar con el CRT estático.

**Pruébalo en una máquina limpia** antes de entregarlo. Aislar el `PATH` descarta
lo importante, pero un Windows recién instalado sin herramientas de desarrollo
es la comprobación definitiva y cuesta cinco minutos.

## 14. Diagnóstico

| Síntoma | Qué mirar |
|---|---|
| "no se encontró ningún navegador" | `web.info()`, o define `ORION_CHROME` |
| "lo tapa `<...>`" | cierra ese elemento primero, o `{ force: yes }` |
| "no apareció en N ms" | ¿el selector es correcto? ¿está en un iframe de otro origen? |
| la página se queda congelada | `web.dialogs(p, "accept")` antes de la acción |
| `text` devuelve vacío | ¿estás usando `count`/`exists`, que no esperan? |
