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

> **Estado**: transporte, arranque, navegación, interacción, formularios,
> tablas, modales, ventanas, extracción (con descubrimiento de esquema),
> archivos, sesión, estabilidad, captura de red y recorrido paralelo (`crawl`)
> verificados de punta a punta (72 tests e2e en
> [`orion-vm/tests/browser_e2e.rs`](orion-vm/tests/browser_e2e.rs), contra
> servidor local). **Cero constantes fijadas**: todo lo que decide el
> comportamiento se puede cambiar desde `open()` — ver 1.2. Medido contra
> Selenium y Playwright en 16.3, con la metodología en
> [`bench/web/README.md`](bench/web/README.md).

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
    sin:      ["--disable-extensions"], -- banderas por defecto a quitar
    allow:    ["*.empresa.com"]       -- lista blanca de dominios, ver 9.2
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
| `web.reload(pestaña, opts?)` | recarga, ver 10.1 |
| `web.back(pestaña)` / `web.forward(pestaña)` | historial, ver 10.1 |

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
| `web.fill(pestaña, campos, opts?)` | un formulario entero en una llamada, ver 4.5 |
| `web.check(pestaña, sel)` / `web.uncheck(...)` | casillas, ver 4.6 |

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

### 4.5 `web.fill(pestaña, campos, opts?)` → cuántos rellenó

Un formulario entero en **una sola llamada**, y el tipo de cada control lo
decide la página:

```orion
web.fill(p, {
    "#nombre":  "Ana Torres",
    "#notas":   "texto largo",
    "#pais":    "España",     -- <select>: vale el texto visible, el value o el índice
    "#acepto":  yes,          -- casilla
    "#plan_b":  yes,          -- radio
    "#bio":     "..."         -- contenteditable
})
```

Obligar a elegir la función según de qué está hecho el campo —`type` para el
texto, `select` para el desplegable, `check` para la casilla— significa mirar el
HTML antes de poder escribir una línea.

**El orden se respeta**, y hace falta: un desplegable de provincia que solo se
llena al elegir el país tiene que ir después del país.

**Por qué es rápido.** `type` manda dos eventos CDP por carácter. Medido contra
un sitio real, 51 caracteres tecla a tecla cuestan **221 ms** y la misma
asignación en una llamada cuesta **1 ms**.

**Por qué `type` sigue existiendo.** Las teclas de verdad hacen falta cuando el
sitio reacciona a ellas: autocompletados, máscaras de teléfono, buscadores que
filtran mientras escribes. Para esos, `{ keys: yes }` pasa `fill` al modo lento
y fiel, campo a campo.

```orion
web.fill(p, { "#buscador": "madr" }, { keys: yes })   -- dispara el autocompletado
```

#### La trampa del `value`

Asignar `el.value = x` y lanzar un evento **no llega a la aplicación** si el
sitio usa React. React instala un rastreador sobre el descriptor `value` del
elemento y, cuando llega el evento, compara con lo último que él anotó: si
coincide, da el cambio por visto y no avisa a nadie.

El resultado es el peor fallo posible: **el campo se ve relleno en pantalla y el
formulario se envía vacío.** Comprobado sobre el mismo mecanismo que usa React:

| Cómo se rellena | ¿Se entera la aplicación? |
|---|---|
| `el.value = x` + evento | **No** |
| setter nativo del prototipo + evento | Sí |
| teclas reales | Sí |

`fill` escribe por el setter nativo del prototipo, que el rastreador no
intercepta. Y usa el prototipo correcto: el de `HTMLInputElement` no sirve para
un `<textarea>` y la asignación se perdería sin decir nada.

Además hace `blur` al terminar cada campo, porque muchos formularios validan al
perder el foco y si no el campo queda relleno pero marcado en rojo, con el botón
de enviar deshabilitado.

#### Lo que no encuentra, lo dice

Un campo que no se rellena casi nunca es un dato que faltaba: es el selector
equivocado, o el formulario que cambió. Callarlo deja el envío incompleto y el
fallo aparece en el servidor de otro.

```
browser.fill: 1 campo(s) no existen en la página:
    #telefono_viejo
    #pais  ->  no hay opción "Marte"
  Opciones: España, Portugal
  Revisa esos selectores, o usa { strict: no } si de verdad pueden faltar.
```

Se espera a que estén **todos** antes de tocar ninguno: parar a medio rellenar
deja el formulario en un estado que nadie escribió.

### 4.6 `web.check(pestaña, sel)` / `web.uncheck(pestaña, sel)`

Marca o desmarca con un **clic real**, y solo si hace falta:

```orion
web.check(p, "#acepto")
web.check(p, "#acepto")   -- ya estaba: no hace nada
```

La idempotencia no es un detalle. Si se limitara a pulsar, un reintento inocente
—o un bucle que revisa la casilla— la dejaría en el contrario de lo que se pedía.

Un `<input type="radio">` no se puede desmarcar pulsándolo, y `uncheck` lo dice
en vez de fallar en silencio: hay que marcar otro del grupo.

## 5. Lectura del DOM

| Función | ¿Espera? |
|---|---|
| `web.text(pestaña, sel, ms?)` | sí |
| `web.texts(pestaña, sel, ms?)` | sí |
| `web.html(pestaña, sel, ms?)` | sí |
| `web.attr(pestaña, sel, atributo, ms?)` | sí |
| `web.value(pestaña, sel)` | sí — lo que el campo contiene AHORA, ver 5.1 |
| `web.table(pestaña, sel, opts?)` | sí — una `<table>` entera, ver 5.2 |
| `web.watch` + `web.capture` | el JSON que la página pide a su API, ver 12 |
| `web.discover(pestaña, opts?)` | deduce el esquema de extracción solo, ver 7.5 |
| `web.crawl(navegador, opts)` | recorre urls en paralelo, vuelca y reanuda, ver 7.6 |
| `web.exists(pestaña, sel)` | **no** |
| `web.count(pestaña, sel)` | **no** |
| `web.visible(pestaña, sel)` | **no** |
| `web.wait(pestaña, sel, ms?)` | espera explícita |
| `web.wait(pestaña, { idle: ms })` | espera a que la red se calme, ver 10.2 |

La regla: **lo que devuelve contenido espera; lo que informa del estado no.**

Devolver `null` porque el contenido aún no había llegado convierte un problema
de tiempo en un dato perdido en silencio — el fallo que hace que un scraper
funcione en el portátil y no en el servidor. Al revés, hacer esperar a `exists`
convertiría un "no está" legítimo en diez segundos de bloqueo.

### 5.1 `web.value(pestaña, sel)` → lo que el campo contiene ahora

```orion
web.fill(p, { "#nombre": "Ana" })
show(web.value(p, "#nombre"))          -- Ana
show(web.attr(p, "#nombre", "value"))  -- null
```

Las dos líneas de arriba no se contradicen, y confundirlas es un clásico:
`attr` lee el **atributo del HTML** —el que venía escrito en la página— y ese no
cambia cuando alguien escribe en el campo. Un `<input>` sin `value=` en el HTML
devuelve `null` por ahí aunque tenga texto dentro, que es justo el momento en el
que uno cree que su `fill` no funcionó.

`value` devuelve además lo que corresponde a cada control: el valor de la opción
elegida en un `<select>`, `yes`/`no` en una casilla, y el texto en un
`contenteditable`.

### 5.2 `web.table(pestaña, sel, opts?)` → lista de registros

```orion
filas = web.table(p, "table.wikitable")
show(len(filas))       -- 222
show(filas[1])
-- {Country/Territory: United States, IMF (2026)[1]: 32,383,920, ...}
```

Una tabla entera en una llamada, con la cabecera deducida y las columnas ya
nombradas. Se puede encadenar directamente con el motor de datos.

**Las reglas de aquí salen de mirar tablas reales, no de imaginarlas.** De 13
tablas en tres páginas de Wikipedia:

| | Cuántas |
|---|---|
| Sin `<thead>` | **13 de 13** |
| Con `<th>` dentro del cuerpo (encabezados de fila) | 10 |
| Con `colspan` o `rowspan` | 4 |
| Con otra tabla dentro | 1 |

Un lector que dé por hecho el `<thead>` —que es como sale la primera versión—
funciona perfecto en el sitio de demostración y falla en el 100% de las tablas
de verdad. De ahí las cuatro decisiones:

1. **La cabecera se busca en cascada**: `<thead>`, o la primera fila si
   **todas** sus celdas son `<th>`, o nombres generados `col_1`, `col_2`…
2. **Exigir que sean *todas* `<th>`** es lo que evita confundir una fila de
   datos que empieza con un encabezado de fila con la cabecera de la tabla.
   Es el caso de 10 de las 13.
3. **`colspan` y `rowspan` se expanden.** Sin eso, las columnas se desalinean a
   partir de la primera celda combinada y todo lo que sigue queda corrido un
   puesto, con pinta de dato bueno.
4. **Las filas de una tabla anidada pertenecen a la de dentro**, no a esta.

Con cabeceras a varios pisos manda la de abajo, que es la que nombra columnas.

**Los nombres de columna se limpian** porque son claves: se colapsan los
espacios (una cabecera con un `<br>` daría una clave con un salto de línea, y esa
no hay quien la escriba), los vacíos pasan a `col_N` y los repetidos se numeran
(`n`, `n_2`). Los **valores no se tocan**: ahí un salto de línea puede ser parte
del dato.

`{ header: no }` no interpreta ninguna fila como cabecera y devuelve todo como
datos con nombres generados. Hace falta para las tablas que se usan como
maquetación, donde la primera fila ya es un dato.

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
| `.tag\|list` | **todas** las coincidencias, no la primera |
| `.p\|list:num` | todas, convertidas a número |
| `a@href\|list` | todos los enlaces de la fila |

Conversiones: `num`, `int`, `bool`, `html`, `text`, `trim`, `list`,
`list:<conversión>`.

Tres detalles que evitan errores silenciosos:

**`list` recoge todas.** Sin él, un campo con varios valores —las etiquetas de un
producto, las imágenes de una galería— devolvía la primera coincidencia y las
demás se perdían sin decir nada. Una lista vacía en **todas** las filas cuenta
como selector muerto igual que un `null`, así que el aviso de 7.3 sigue
funcionando ahí, que es donde más falta hace.

Dentro de una lista se conserva el `null` que venga de la conversión (`"Agotado"`
con `list:num`), porque ahí sí había algo y hace falta verlo para entender por
qué no salió el número. Lo que se salta son los elementos sin nada dentro.

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

### 7.5 `web.discover(pestaña, opts?)` → esquema propuesto

El problema de un scraper no es leer datos, es **averiguar qué selector usar**.
Uno abre las herramientas del navegador, baja por el árbol, prueba una clase, ve
que también casa con el menú, prueba otra… y veinte minutos después tiene un
esquema que se rompe en la página siguiente.

`discover` mira la página y lo propone:

```orion
e = web.discover(p)
show(e["row"])       -- ".quote"     (el selector de la fila que se repite)
show(e["fields"])    -- {text: ".text", author: ".author", url: "a@href"}
show(e["sample"])    -- las primeras filas ya extraídas con esa propuesta

-- y se usa tal cual:
filas = web.extract(p, e["row"], { frase: ".text", autor: ".author" })
```

Devuelve `{ row, count, fields, sample, fragil }`. La **muestra** es lo que lo
hace fiable: no te pide que confíes en la propuesta, te enseña qué extraería.

Cómo lo deduce, para que no sea magia:

- **La fila** es el grupo de elementos hermanos que más se repite con la misma
  estructura interna, puntuado por cantidad **y riqueza** —texto y número de
  campos—. Así no confunde un listado de productos con el menú de navegación,
  que también se repite pero está vacío.
- La repetición se detecta por **estructura, no por clases**: los sitios
  modernos generan clases como `x1i10hfl` que no significan nada, así que se
  mira el tag y los tags de los hijos.
- **El selector de fila** es la clase común a todas las filas que además
  selecciona exactamente esas. Si ninguna clase sirve, se cae a un selector
  estructural (`article > h3 > a`) y `fragil` viene en `yes` para avisarlo.
- **Los campos** solo se conservan si aparecen en la mayoría de las filas: uno
  que esté en una sola no es un campo, es una casualidad.

No adivina la intención —no sabe que eso es un "precio", así que un campo sin
clase legible se llama `campo_1`—. No sustituye a `extract`: te deja a un paso
de él en vez de a veinte minutos. Nadie lo tiene de serie; en Python te pones a
leer el HTML a mano.

Medido en tres sitios distintos sin decirle nada de ninguno: en Hacker News saca
la URL del artículo y su título; en una tienda de libros, el enlace, la miniatura
y el precio; en un listado de citas, el texto y el autor.

### 7.6 `web.crawl(navegador, opts)` → resumen

`extract_to` recorre una lista de URLs con **una sola pestaña, en serie**. Sirve,
pero deja la máquina a un octavo de gas: mientras una página carga —que es
esperar a la red, no calcular— el resto del navegador está parado.

`web.crawl` abre **N pestañas y las conduce en paralelo desde N hilos de
sistema**:

```orion
r = web.crawl(b, {
    urls:    paginas,                    -- la lista de páginas
    row:     ".card",
    schema:  { nombre: ".title", precio: ".price|num" },
    out:     "catalogo.csv",
    workers: 8,                          -- 8 pestañas a la vez
    resume:  yes                         -- retoma si se cortó
})
show(r)
-- {rows: 4000, ok: 40, failed: 0, skipped: 0, workers: 8, empty: [], files: [catalogo.csv], errors: []}
```

Se le pasa el **navegador**, no una pestaña: las abre él. Toma el `row` y el
`schema` de `extract`, y escribe a disco con el mismo volcador en streaming de
`extract_to` (RAM plana, `.csv` o `.odf`).

**El paralelismo es real, y es el músculo que Orion tiene y un scraper de Python
no**: hilos de sistema de verdad sobre el mismo socket CDP, que el transporte
multiplexa. No es `asyncio` cooperativo. Medido contra un servidor local de 12
páginas lentas: `extract_to` en serie **7,9 s**, `crawl` con 8 workers **1,8 s**
— las mismas 120 filas. El factor depende de cuántas páginas y de la red; la
forma es la que cambia.

**Reanuda.** Un recorrido de diez mil páginas que se corta en la siete mil no
puede empezar de cero. Cada URL terminada se anota en `<salida>.progress`, y al
volver a arrancar con `resume: yes` las hechas se saltan (`skipped` las cuenta).
Se anota **después** de escribir sus filas: si el proceso muere entre medias, esa
página se repite al reanudar en vez de perderse. La reanudación es para `.csv`
—que admite añadir al final—; el `.odf` obliga a empezar de cero y se avisa.

Como `extract`, un campo que no trae valor en **ninguna** página se delata en vez
de dejar una columna vacía que parece buena; con `{ strict: no }` se acepta.

En Python esto es **Scrapy**: un framework entero, otro fichero de settings, otra
mentalidad. Aquí es una llamada, apoyada en piezas que ya existían.

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

## 9. Sesión y seguridad

### 9.1 `web.save_state(pestaña, ruta)` / `web.load_state(pestaña, ruta)`

Lo más caro de una automatización que corre a diario no es navegar: es **volver
a iniciar sesión en cada ejecución**. Es lento, y sobre todo es frágil — cada
login es un formulario que puede cambiar, un captcha que puede aparecer y un
doble factor que puede saltar. Un proceso que se loguea cien veces al día
también es un proceso que parece un ataque.

```orion
-- una vez
web.save_state(p, "sesion.json")

-- todos los días
web.goto(p, "https://portal.empresa.com")
web.load_state(p, "sesion.json")
web.reload(p)                        -- ya dentro
```

`save_state` devuelve qué guardó y `load_state` qué aplicó:

```
{path: sesion.json, cookies: 5, local: 3, session: 0, origin: https://portal.empresa.com}
{cookies: 5, local: 3, session: 0, skipped: []}
```

**Hay que estar en el origen antes de restaurar.** Las cookies van al navegador
entero, pero el almacenamiento local solo se puede escribir estando en su
dominio — el navegador no deja tocar el de otro. Los orígenes que no coinciden
salen en `skipped` en vez de perderse en silencio, porque una sesión restaurada
a medias no da ningún error y es indepurable.

`user_data` en `open()` resuelve algo parecido guardando el perfil entero, pero
es una carpeta de cientos de megas atada a una máquina. Esto es un JSON que se
puede mover, versionar aparte o guardar en un gestor de secretos.

> **Este archivo es una credencial.** Dentro van las cookies de sesión: quien lo
> tenga entra como tú, sin contraseña y sin segundo factor. No va al
> repositorio. Vale exactamente lo mismo que la contraseña, con el agravante de
> que no caduca cuando la cambias.

### 9.2 `open({ allow: [...] })` — lista blanca de dominios

```orion
b = web.open({ allow: ["*.empresa.com", "cdn.proveedor.net"] })
```

Un proceso automático lleva encima la sesión de la empresa. Si la página que
visita está comprometida —o si un anuncio inyectado en ella redirige— el bot se
va a otro sitio **con esa sesión puesta**. La lista blanca acota a dónde puede
ir: lo que no esté, no se carga.

`*.empresa.com` cubre los subdominios y el dominio pelado; sin comodín es solo
ese host exacto. El puerto no cuenta, y lo que va antes de una arroba tampoco:
`http://empresa.com@malo.net/` es una petición a **malo.net**, y ese truco de
suplantación no pasa la lista.

`web.blocked(navegador)` devuelve lo que se ha cortado, que es lo primero que
hace falta cuando un sitio deja de funcionar con la lista puesta.

La interceptación solo se activa si hay lista: con ella, el navegador para en
cada petición y espera respuesta, y eso no se paga si no se pide.

### 9.3 Credenciales fuera de los registros

```orion
web.fill(p, { "#usuario": u, "#clave": c }, { secret: ["#clave"] })
```

Un error de `fill` puede repetir el valor que no se admitió, y ese error acaba
en un log o en una consola compartida. Los campos marcados no cuentan el suyo.
`{ secret: yes }` tapa todos los de la llamada.

## 10. Estabilidad

### 10.1 `web.reload(pestaña, opts?)` / `web.back` / `web.forward`

Devuelven la URL en la que quedan. `{ cache: no }` en `reload` fuerza traerlo
todo del servidor.

Ninguna espera el evento de carga del navegador, y es deliberado: **al volver
atrás, Chrome suele restaurar la página desde su caché de retroceso sin
recargarla, y entonces no hay evento de carga**. Esperarlo dejaba cada `back`
clavado el plazo entero —treinta segundos— para acabar continuando igual. Se
mira la página, que es quien sabe dónde está.

Un `back` sin historial lo dice en vez de no hacer nada. Ojo con una cosa que
confunde: toda pestaña empieza en `about:blank`, así que después de una sola
navegación **sí** queda una página a la que volver.

### 10.2 `web.wait(pestaña, { idle: ms })`

Espera a que la red se calme, para lo que ningún selector resuelve: no sabes
**qué** va a aparecer, solo que la página sigue trayendo cosas. Es el caso del
panel que se monta con tres llamadas encadenadas, o del listado que se recarga
al filtrar.

```orion
web.click(p, "#filtrar")
web.wait(p, { idle: 500 })      -- medio segundo sin peticiones
```

La alternativa que usa todo el mundo es dormir dos segundos, y tiene los dos
defectos a la vez: si la red va lenta se lee a medias, y si va rápida se tiran
dos segundos en cada vuelta.

Las peticiones en vuelo se cuentan **dentro de la página**, envolviendo `fetch`
y `XMLHttpRequest`. Así es una sola llamada y no depende de que el historial de
eventos —que está acotado— haya conservado los que hacían falta.

El límite honesto: hay páginas que sondean el servidor para siempre y nunca se
quedan quietas. En esas, el error lo dice y hay que esperar por un selector.

## 11. Capturas de pantalla

`web.screenshot(pestaña, ruta)` → escribe un PNG y devuelve la ruta.

Requiere `images: yes` en `open` si quieres que salgan las imágenes.

## 12. Captura de red

Casi todo sitio moderno pinta sus listados con JavaScript a partir de un JSON
que él mismo se descarga. Un scraper clásico espera a que ese JSON se convierta
en HTML y luego **deshace el trabajo**: busca `div`s, quita etiquetas,
reconstruye números que ya venían siendo números. Y se rompe el día que alguien
renombra una clase de CSS.

`watch` + `capture` leen la fuente.

```orion
web.watch(p, "/api/productos")     -- 1. armar la escucha
web.click(p, "#cargar")            -- 2. provocar la petición
r = web.capture(p)                 -- 3. recoger, ya parseado
```

Son dos llamadas y no una porque hay que armar **antes**: si la escucha se
encendiera al recoger, la petición ya habría pasado y no quedaría nada que leer.

### 12.1 Qué se gana

En el ejemplo de los tests, la página pinta el nombre de cada producto. Su API
devuelve esto:

```
{id: 1, nombre: Teclado, precio: 49.9, stock: 12, margen: 0.31, proveedor: ACME}
```

`stock`, `margen` y `proveedor` **no llegan al HTML**. No hay selector que los
saque, porque no están. Y los que sí están llegan ya tipados: `49.9` es un
número, no `"49,90 EUR"` que haya que convertir.

| | del HTML | de la API |
|---|---|---|
| Campos | los que el diseño enseñe | todos |
| Tipos | texto, a convertir | ya tipados |
| Se rompe cuando… | cambia una clase de CSS | cambia el contrato de la API |

Lo segundo pasa mucho menos: una clase de CSS la toca cualquier rediseño, y el
contrato de una API lo defiende el propio equipo del sitio.

### 12.2 `web.watch(pestaña, patrón)`

Sin `*`, el patrón es "contiene" — que es lo que casi siempre se quiere y lo que
uno escribe primero:

```orion
web.watch(p, "/api/")
```

Con `*`, es un comodín que cubre cualquier trozo, para cuando hace falta afinar:

```orion
web.watch(p, "*/v2/pedidos?*")
web.watch(p, "*.json")
```

No son expresiones regulares a propósito: una URL lleva `?`, `.` y `+`, que en
una regex significan otra cosa, y el patrón obvio daría resultados
sorprendentes. Aquí esos signos son literales.

El dominio de red del navegador solo se enciende al llamar a `watch`: con él
puesto se emiten varios eventos por petición, y una página con cien recursos
serían cientos de mensajes que nadie va a consumir.

### 12.3 `web.capture(pestaña, opts?)` → lista

Cada elemento es `{url, status, json}`. Si el cuerpo no era JSON, `json` viene
nulo y el texto crudo va en `text`.

```orion
r = web.capture(p)
for resp in r {
    show(resp["url"] + " -> " + str(len(resp["json"]["items"])))
}
```

**Espera a que llegue algo que case** en vez de mirar una vez y volver vacía. La
petición sale después de la acción que la provoca, y una lista vacía convertiría
un problema de tiempo en "este sitio no usa API" — una conclusión falsa y difícil
de deshacer. Si de verdad no casa nada, devuelve vacío al agotar el plazo.

Se recogen **todas** las respuestas que casen, no la primera: un panel suele
pedir tres o cuatro cosas a la vez, y quedarse con una daría un resultado
incompleto con pinta de completo.

**El cuerpo puede haber desaparecido.** No viaja en el evento: se pide aparte, y
el navegador lo guarda en un búfer que acaba reciclando. Si pasa, ese elemento
trae `error` explicándolo en vez de tirar la captura entera. Se sube con:

```orion
b = web.open({ tuning: { body_buffer: 52428800 } })
```

### 12.4 Comparado

Playwright tiene `page.on("response")`: un callback donde hay que filtrar a
mano, pedir el cuerpo con otro `await` y acordarse de que puede no estar.
Selenium no tiene nada equivalente sin poner un proxy delante.

## 13. JavaScript

`web.eval(pestaña, js)` evalúa y devuelve el valor ya convertido a Orion.

```orion
n = web.eval(p, "document.querySelectorAll('.card').length")
```

Una excepción del JavaScript se convierte en error de Orion, no en un `null`
silencioso.

## 14. Memoria

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

## 15. Arquitectura

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

## 16. Despliegue

### 16.1 Qué entregas

```powershell
orion --build app.orx -o app.exe
```

**Un solo archivo.** `--build` no empaqueta el intérprete al lado: compila tu
programa a nativo con Cranelift y lo enlaza contra el runtime de Orion como
librería estática. El resultado no es un lanzador que busca `orion.exe`, es un
ejecutable de verdad con el runtime dentro.

Tu usuario recibe `app.exe` y no necesita saber que Orion existe.

#### Qué se probó exactamente

Un programa que usa `upload`, `fill`, `table`, `extract`, `save_state`, `pdf` y
`reload` —es decir, el módulo entero, no un "hola mundo"— compilado a **nativo
AOT** (61 MB) y ejecutado en una carpeta que contenía **solo `app.exe`**, sin
ningún `orion.exe` cerca y con el `PATH` reducido a `system32`. Los diez
resultados correctos y código de salida 0.

Conviene decir cómo llegó a estar bien, porque hasta el 8 de agosto de 2026 esto
**no funcionaba** y la documentación decía que sí. Dos defectos, los dos
exclusivos del ejecutable compilado (`orion run` nunca estuvo afectado):

1. **Una función no veía las variables globales.** El compilador daba a cada
   función solo variables locales, así que un global leído dentro de ella
   llegaba como `null`. Y como `use "browser"` define un global, cualquier
   llamada al módulo dentro de una función moría con un error sobre
   `CallMethod` que no apuntaba a la causa. Peor aún: un cálculo con una
   constante global daba **otro resultado** sin avisar.
2. **Llamar `main` a tu función** hacía chocar su símbolo con el `main` de C del
   ejecutable, y la compilación se pasaba a bytecode embebido. Seguía
   funcionando, pero ninguna aplicación real —que se escriben así— llegaba a
   compilarse nativa.

Los dos tienen ahora tests de regresión en
[`orion-vm/tests/aot_native.rs`](orion-vm/tests/aot_native.rs), que es lo que
faltaba: la batería anterior solo probaba programas autocontenidos —aritmética,
recursión, shapes, cadenas— y por eso nadie se enteró.

**La lección para leer esta página**: si aquí pone "verificado", debería decir
también *con qué programa*. Un "hola mundo" compilado no prueba que tu
aplicación compile.

### 16.2 Qué necesita la máquina del usuario

**Un navegador basado en Chromium, y nada más.** En Windows ya está: Edge viene
con el sistema. Si su instalación está en una ruta poco habitual, se resuelve
sin recompilar con la variable `ORION_CHROME` o pasando `chrome:` en `open()`.

### 16.3 Comparado con Python

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

#### Medido

500 tarjetas × 4 campos, las tres herramientas moviendo **el mismo Chrome**, en
headless, contra el mismo archivo local, y comprobando que las tres devuelven la
misma huella de datos. Reproducible con `bench\web\run_web.ps1`; metodología y
avisos en [`bench/web/README.md`](bench/web/README.md).

| variante | extracción | proceso entero | RAM de la pila | auxiliar |
|---|---:|---:|---:|---|
| Selenium, idiomático | 14.132 ms | 24.953 ms | 62,3 MB | chromedriver |
| Selenium, con JS a mano | 7,7 ms | 8.088 ms | 59,5 MB | chromedriver |
| Playwright, idiomático | 9.234 ms | 12.175 ms | 317,3 MB | node |
| Playwright, con JS a mano | 31,0 ms | 1.430 ms | 156,5 MB | node |
| **Orion `extract`** | **8 ms** | **745 ms** | **16,2 MB** | **ninguno** |

La RAM es la del proceso de automatización **más el auxiliar que arranca**, que
no es el navegador y no es el mismo en los tres: Selenium necesita
`chromedriver.exe` y Playwright un `node.exe` porque su driver está escrito en
JavaScript. Orion no necesita ninguno — habla CDP desde su propio proceso, que
es la misma razón por la que no hay un segundo binario que mantener
sincronizado con la versión de Chrome. El navegador se excluye de la cuenta: es
idéntico para las tres.

Dos lecturas honestas de esta tabla:

**Orion no ejecuta JavaScript más rápido que nadie.** Sus 8 ms están en el mismo
orden que los 7,7 ms de Selenium mandando JavaScript a mano, y esa diferencia
cabe en el ruido. El resultado no es ese.

**El resultado es la primera fila contra la última: 14 segundos contra 8
milisegundos.** Esa primera fila es cómo enseñan a hacerlo las dos
documentaciones — localizar los elementos y pedirles el texto uno a uno, que con
500 filas × 4 campos son 2.000 viajes. Lo que aporta `extract` no es velocidad
bruta: es que **el camino rápido es el único que hay**. En las otras dos hay que
saber que el problema existe y escribir JavaScript a mano dentro de Python, que
es justo el trabajo que uno esperaba no tener que hacer.

De los 8 segundos de Selenium, la extracción son 8 ms: el resto es arrancar
`chromedriver` (~1,4 s), `quit()` (~2,1 s) y **~4,2 s después de la última línea
del script**, esperando a que su árbol de procesos termine de irse. Para una
tarea suelta da igual; para un trabajo que corre cada cinco minutos, no.

En memoria la diferencia es de otro orden: **16 MB contra 60 y contra 157**. La
versión idiomática de Playwright llega a 317 MB porque retiene un handle por
cada elemento consultado, y aquí son 2.000 vivos a la vez. Eso pesa cuando el
trabajo corre en un servidor con varias tareas en paralelo.

### 16.4 Redes corporativas

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

### 16.5 Lo que conviene saber

**Tamaño.** El ejecutable ronda los 58 MB. Es el binario completo de Orion:
lleva GUI, TUI, tres motores de base de datos, OCR con sus modelos… todo, se use
o no. Hoy no hay forma de adelgazarlo.

**Runtime de C.** El binario enlaza el CRT de MSVC de forma dinámica, así que
depende de `vcruntime140.dll`, presente en cualquier Windows moderno. No está
probado compilar con el CRT estático.

**Pruébalo en una máquina limpia** antes de entregarlo. Aislar el `PATH` descarta
lo importante, pero un Windows recién instalado sin herramientas de desarrollo
es la comprobación definitiva y cuesta cinco minutos.

## 17. Diagnóstico

| Síntoma | Qué mirar |
|---|---|
| "no se encontró ningún navegador" | `web.info()`, o define `ORION_CHROME` |
| "lo tapa `<...>`" | cierra ese elemento primero, o `{ force: yes }` |
| "no apareció en N ms" | ¿el selector es correcto? ¿está en un iframe de otro origen? |
| la página se queda congelada | `web.dialogs(p, "accept")` antes de la acción |
| `text` devuelve vacío | ¿estás usando `count`/`exists`, que no esperan? |
