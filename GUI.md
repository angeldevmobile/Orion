# Orion GUI — referencia

GUI de escritorio nativo sobre `egui`. Modo inmediato: el script se **re-ejecuta
en cada evento** y el estado vive en `gui.set/gui.val`. Cero hardcodeo: el
developer decide tema, colores, bordes, fuentes y layout.

```orion
use "gui" as gui
gui.panel("Mi App", 720, 560)
gui.heading("Hola Orion")
gui.run()                       -- abre la ventana (bloqueante)
```

## Tema — `gui.theme({...})`

Sobrescribe lo que quieras; lo que no fijes cae al default.

```orion
gui.theme({
    "accent":   "#f97316",      -- o nombre: "blue", "success"…
    "bg":       "#1b1410",
    "surface":  "#2a2018",
    "text":     "#f5f5fa",
    "rounding": 16,             -- radio de esquinas
    "heading":  30,             -- tamaño de títulos
    "body":     15,             -- tamaño de cuerpo
    "spacing":  8,
    "light":    no              -- yes = tema claro
})
```

Los nombres semánticos `accent`/`surface`/`bg`/`text` **siguen tu tema**.

## Colores

- **Nombres semánticos:** `accent`, `success`, `warning`, `error`, `info`
- **Básicos:** `white`, `black`, `red`, `green`, `blue`, `yellow`, `orange`, `purple`, `pink`, `gray`
- **Hex:** `"#22c55e"`

## Estilo por widget

Cualquier widget acepta un color posicional **o** un dict de estilo completo:

```orion
gui.heading("Título", "accent")                 -- color de texto
gui.heading("Título", { "color": "accent", "size": 28 })
gui.press("Guardar", "#22c55e", "white")        -- bg, texto
gui.zone({ "border": "accent", "border_w": 2, "rounding": 18, "pad": 22 })
  -- … hijos …
gui.end()
```

Claves de estilo: `bg`/`fill`, `fg`/`color`/`text`, `border`, `border_w`,
`rounding`/`radius`, `size`/`font_size`, `pad`/`padding`.

## Layout (contenedores — se cierran con `gui.end()`)

| Función | Qué hace |
|---|---|
| `gui.panel(titulo, w, h)` | configura la ventana |
| `gui.card({width?, fill?})` | tarjeta |
| `gui.row()` | fila (columnas de igual ancho) |
| `gui.col()` | columna |
| `gui.grid(n)` | rejilla de n columnas |
| `gui.sidebar(ancho?)` | barra lateral |
| `gui.zone({estilo})` | contenedor con estilo libre |
| `gui.end()` | cierra el último contenedor |

## Widgets

**Texto:** `gui.heading(t, estilo?)` · `gui.text(t, estilo?)` · `gui.caption(t, estilo?)`

**Acciones:** `gui.press(label, bg?, fg?)` · `gui.ghost(label, color?)` · `gui.tap(label)`
— al pulsar disparan `label` como evento → `if gui.pressed("label") { … }`

Para disparar un evento **distinto del texto visible** (clave en listas dinámicas:
ícono fijo, evento por índice), pásalo en el dict de estilo:

```orion
gui.press("✓", { "bg": "success", "event": "toggle:" + str(i) })
gui.ghost("×", { "color": "error",  "event": "del:" + str(i) })
-- luego: if gui.pressed("toggle:3") { … }   ó parsea gui.ev()
```

**Inputs:** `gui.field(placeholder, estilo?)` · `gui.toggle(label)` ·
`gui.pick(id, [opciones], estilo?)` · `gui.slide(id, min, max, step)`

Dale al campo un **id estable** para leerlo fiable (sin él el id es posicional y
se rompe si cambia el layout):

```orion
gui.field("¿Qué hacer?", { "id": "nueva" })
txt = gui.value("nueva")        -- lee lo escrito
gui.setval("nueva", "")         -- fija/limpia el campo (p.ej. tras agregar)
```

**Display:** `gui.badge(t, estilo?)` · `gui.banner(titulo, subtitulo?, estilo?)` ·
`gui.avatar(t, tam?, estilo?)` · `gui.divider()` · `gui.spacer(px?)`

**Datos:** `gui.table([dicts], {height?, cols?})` ·
`gui.chart([dicts], tipo, {x, y, color, height…})` — tipo: `bar|line|area|scatter|pie|hist`

**Nuevos:**
| Widget | Uso |
|---|---|
| `gui.progress(v, color?)` | barra de progreso (`v` 0..1 o 0..100) |
| `gui.tabs([labels], activa?)` | pestañas; al click dispara el label |
| `gui.image(ruta, ancho?, alto?)` | png/jpg/bmp/gif (mantiene aspecto) |
| `gui.modal(titulo) … gui.end()` | diálogo centrado |

## Estado reactivo

```orion
n = gui.val("contador", 0)          -- lee (con default)
if gui.pressed("+1") { gui.set("contador", n + 1) }   -- escribe
```

## Animaciones

`gui.fade(id, mostrar) … gui.end()` · `gui.slide_in(id) … gui.end()`

### Reloj — `gui.tick(ms)`

Dispara el evento `"tick"` cada `ms` milisegundos y re-ejecuta el script,
igual que un clic. Con eso cualquier cosa se anima en Orion puro: el script
avanza su estado un paso por tick y redibuja.

```orion
if gui.pressed("tick") { gui.set("x", gui.val("x", 0) + 2) }
if gui.val("animando", no) { gui.tick(30) }   -- pedirlo en CADA re-ejecución
```

- Los clics del usuario tienen prioridad: la animación nunca se traga un botón.
- No es pegajoso: si el script deja de llamar `gui.tick`, el reloj se apaga.
- Pídelo solo mientras haya algo que animar (no gastar CPU en reposo).

### Lienzo — `gui.canvas(ancho, alto) … gui.end()`

Dibujo 2D libre con coordenadas locales al lienzo (`(0,0)` = esquina superior
izquierda). Las formas van dentro del bloque; los colores aceptan nombres del
tema (`accent`, `success`…), básicos (`gray`, `red`…) o hex (`#ff7a00`).

| Forma | Descripción |
|---|---|
| `gui.circle(x, y, r, color?, fill?)` | círculo (`fill: no` = solo contorno) |
| `gui.line(x1, y1, x2, y2, color?, grosor?)` | segmento |
| `gui.rect(x, y, w, h, color?, fill?)` | rectángulo |
| `gui.arrow(x1, y1, x2, y2, color?, grosor?)` | flecha con punta en `(x2, y2)` |
| `gui.text_at(x, y, texto, tamaño?, color?)` | texto centrado en `(x, y)` |

```orion
gui.canvas(240, 240)
  gui.circle(120, 120, 90, "gray", no)      -- órbita
  gui.arrow(120, 120, x, y, "accent", 3)    -- vector que se mueve por tick
gui.end()
```

---

Ejemplos completos: [`demo/demo_design.orx`](demo/demo_design.orx) (dashboard),
[`demo/demo_theme.orx`](demo/demo_theme.orx) (tema custom),
[`demo/demo_widgets.orx`](demo/demo_widgets.orx) (widgets nuevos),
[`demo/demo_calc.orx`](demo/demo_calc.orx) (calculadora reactiva),
[`demo/demo_tasks.orx`](demo/demo_tasks.orx) (gestor de tareas: GUI + módulo
`state` + persistencia a disco — las tareas sobreviven al reinicio),
[`demo/demo_bloch_anim.orx`](demo/demo_bloch_anim.orx) (esfera de Bloch
animada: `tick` + `canvas` + física cuántica real paso a paso).
