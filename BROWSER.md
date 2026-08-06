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

> **Estado**: transporte, arranque, navegación, interacción, modales y ventanas
> verificados de punta a punta (24 tests e2e en
> [`orion-vm/tests/browser_e2e.rs`](orion-vm/tests/browser_e2e.rs), contra
> servidor local). **Cero constantes fijadas**: todo lo que decide el
> comportamiento se puede cambiar desde `open()` — ver 1.2. Pendiente:
> extracción declarativa (`extract`), streaming a `.odf`, cookies/sesión y
> benchmark contra Python.

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

## 7. Captura

`web.screenshot(pestaña, ruta)` → escribe un PNG y devuelve la ruta.

Requiere `images: yes` en `open` si quieres que salgan las imágenes.

## 8. JavaScript

`web.eval(pestaña, js)` evalúa y devuelve el valor ya convertido a Orion.

```orion
n = web.eval(p, "document.querySelectorAll('.card').length")
```

Una excepción del JavaScript se convierte en error de Orion, no en un `null`
silencioso.

## 9. Memoria

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

## 10. Arquitectura

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

## 11. Despliegue

### 11.1 Qué entregas

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

### 11.2 Qué necesita la máquina del usuario

**Un navegador basado en Chromium, y nada más.** En Windows ya está: Edge viene
con el sistema. Si su instalación está en una ruta poco habitual, se resuelve
sin recompilar con la variable `ORION_CHROME` o pasando `chrome:` en `open()`.

### 11.3 Comparado con Python

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

### 11.4 Redes corporativas

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

### 11.5 Lo que conviene saber

**Tamaño.** El ejecutable ronda los 58 MB. Es el binario completo de Orion:
lleva GUI, TUI, tres motores de base de datos, OCR con sus modelos… todo, se use
o no. Hoy no hay forma de adelgazarlo.

**Runtime de C.** El binario enlaza el CRT de MSVC de forma dinámica, así que
depende de `vcruntime140.dll`, presente en cualquier Windows moderno. No está
probado compilar con el CRT estático.

**Pruébalo en una máquina limpia** antes de entregarlo. Aislar el `PATH` descarta
lo importante, pero un Windows recién instalado sin herramientas de desarrollo
es la comprobación definitiva y cuesta cinco minutos.

## 12. Diagnóstico

| Síntoma | Qué mirar |
|---|---|
| "no se encontró ningún navegador" | `web.info()`, o define `ORION_CHROME` |
| "lo tapa `<...>`" | cierra ese elemento primero, o `{ force: yes }` |
| "no apareció en N ms" | ¿el selector es correcto? ¿está en un iframe de otro origen? |
| la página se queda congelada | `web.dialogs(p, "accept")` antes de la acción |
| `text` devuelve vacío | ¿estás usando `count`/`exists`, que no esperan? |
