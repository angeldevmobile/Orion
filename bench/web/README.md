# Benchmark de automatización web

Orion contra Selenium y Playwright en la tarea que define un scraper: **cargar
un listado y extraer varios campos de cada fila**.

```powershell
powershell -ExecutionPolicy Bypass -File bench\web\run_web.ps1
```

Requisitos: `python` con `selenium` y `playwright` (`pip install selenium
playwright` — **no** hace falta `playwright install`, se usa el Chrome que ya
tienes), Chrome instalado, y el binario release de Orion.

## La tarea

500 tarjetas × 4 campos = **2.000 lecturas**: dos textos, un atributo de la
propia fila y un atributo de un descendiente. Uno de los textos es un precio con
separadores de miles, así que hay que convertirlo a número — en las variantes de
Python esa conversión se escribe a mano, porque forma parte del trabajo que se
está comparando.

La página se carga como archivo local (`file://`). No hay red ni servidor: lo
que se mide es el coste de hablar con el navegador.

## Las reglas

Un benchmark de esto es fácil de amañar sin querer, así que:

1. **El mismo binario de Chrome** para las tres. Playwright se descargaría su
   propio Chromium; aquí se le pasa `executable_path` al instalado. Si cada una
   moviera un navegador distinto, el número mediría eso.
2. **Las tres en headless y sin imágenes**, con las mismas banderas.
3. **Dos formas por herramienta.** `idiomatico` es como sale en la documentación
   de cada una: localizar los elementos y pedir su texto uno a uno. `js` es la
   salida que conoce quien ya se ha peleado con el problema: mandar una
   evaluación que lo resuelva dentro de la página.

   Comparar solo contra la forma lenta sería un espantapájaros. Lo que se mide
   es **qué cuesta hacerlo bien en cada herramienta**, y cuál te da el camino
   bueno sin pedirlo.
4. **Todas imprimen una huella** (SHA-256 de lo extraído, en céntimos enteros
   para que no dependa de cómo formatea cada lenguaje). Si no coinciden, el
   script se detiene: no tiene sentido publicar tiempos de tareas distintas.
5. Dos pasadas, se reporta la **segunda**, con todo en caliente.

## Qué mide cada columna

- **extracción**: solo la lectura de datos, con la página ya cargada. Es la
  comparación limpia entre formas de hablar con el navegador.
- **proceso entero**: arrancar el navegador, extraer una vez y cerrar. Aquí se
  ve lo que cuesta levantar y tirar cada pila.
- **RAM proceso**: pico del proceso que escribes tú (`python` u `orion.exe`).
- **RAM pila**: ese proceso **más los auxiliares que arranca**.

Esa última columna existe porque la anterior, sola, cuenta mal. Cada
herramienta necesita un acompañante distinto y no es el navegador:

| | proceso auxiliar propio |
|---|---|
| Orion | **ninguno** — habla CDP desde su propio proceso |
| Selenium | `chromedriver.exe`, un segundo binario cuya versión tiene que corresponderse con la de Chrome |
| Playwright | un `node.exe`, porque su driver está escrito en JavaScript |

**El navegador sí se excluye de la cuenta**: es el mismo binario y el mismo
trabajo en los tres casos, así que sumarlo solo añadiría ruido igual para todos.

Los auxiliares se identifican por PID contra una foto tomada justo antes de
arrancar: en una máquina de desarrollo hay procesos `node` de otras cosas —el
editor, sin ir más lejos— y contarlos falsearía el resultado.

## Resultados (2026-08-08)

Intel i7-1165G7, 24 GB RAM, Windows 11, Chrome 151, Python 3.13, Selenium 4.30,
Playwright 1.62, Orion release. **Las cinco variantes mueven el mismo Chrome** y
devuelven la misma huella `a4f7969f5377`. Mejor de cinco pasadas completas.

| variante | extracción | proceso entero | RAM proceso | RAM pila | auxiliar |
|---|---:|---:|---:|---:|---|
| Selenium, idiomático | 14.132 ms | 24.953 ms | 39,0 MB | 62,3 MB | chromedriver |
| Selenium, con JS a mano | 7,7 ms | 8.088 ms | 38,6 MB | 59,5 MB | chromedriver |
| Playwright, idiomático | 9.234 ms | 12.175 ms | 38,3 MB | 317,3 MB | node |
| Playwright, con JS a mano | 31,0 ms | 1.430 ms | 34,1 MB | 156,5 MB | node |
| **Orion `extract`** | **8 ms** | **745 ms** | **16,2 MB** | **16,2 MB** | **ninguno** |

### Lo que dicen estos números

**Orion no ejecuta JavaScript más rápido que nadie.** Su extracción (8 ms) está
en el mismo orden que la de Selenium mandando JS a mano (7,7 ms); esa diferencia
cabe dentro del ruido y de la resolución de milisegundo del reloj de Orion.
Quien esperase un titular de "10× más rápido" en esa fila no lo va a encontrar,
y decirlo sería mentir.

**El resultado de verdad está en la primera fila contra la última: 14 segundos
contra 8 milisegundos.** Esa primera fila es cómo enseñan a hacerlo Selenium y
Playwright en su documentación — localizar los elementos y pedirles el texto uno
a uno. Con 500 filas × 4 campos son 2.000 viajes de ida y vuelta, y el precio no
se ve en un ejemplo de diez filas: se ve el día que el catálogo crece.

Lo que aporta `extract` no es velocidad bruta, es que **el camino rápido es el
único que hay**. En las otras dos hay que saber que el problema existe, y
entonces escribir JavaScript a mano dentro de Python — que es exactamente el
trabajo que uno esperaba no tener que hacer.

**Arrancar y cerrar sí es una diferencia de fondo: 745 ms contra 8,1 segundos de
Selenium.** Y no es la extracción, que son 8 ms. Desglosado:

| Selenium, de dónde salen sus 8 segundos | |
|---|---|
| arrancar Python e importar la librería | ~0,4 s |
| resolver y arrancar `chromedriver` + Chrome | ~1,4 s |
| `driver.quit()` | ~2,1 s |
| **después de la última línea del script** | **~4,2 s** |

Esa última fila se midió comparando el reloj interno del programa (4,08 s) con
el tiempo de pared del proceso (8,23 s): el proceso de Python no termina de
salir hasta que su árbol de `chromedriver` se va del todo. Para una tarea suelta
da igual; para un trabajo que se lanza cada cinco minutos, son ocho segundos por
vuelta que no hacen nada.

**En memoria la diferencia es de otro orden: 16 MB contra 60 y contra 157.** Un
proceso auxiliar no es gratis, y el de Playwright es un runtime de JavaScript
entero. Su versión idiomática llega a **317 MB** porque además retiene un handle
por cada elemento consultado: 2.000 handles vivos a la vez.

Esto último importa más de lo que parece cuando el trabajo corre en un servidor
con varias tareas a la vez: no es lo mismo reservar 16 MB por proceso que 157.

### Lo que este benchmark NO mide

- **Una sola página local.** No hay red, ni latencia, ni un sitio que tarde en
  responder. En un scraper real eso suele dominar el tiempo total, y ahí las
  tres herramientas esperan lo mismo.
- **Solo lectura.** No mide clics, formularios ni esperas de accionabilidad.
- **Playwright pierde una ventaja aquí**: se le pasa el Chrome instalado para
  que la comparación sea justa, y así no se ve lo que aporta traerse su propio
  navegador con versión fijada.
- **El navegador no se cuenta**, ni en tiempo de arranque ni en memoria. Es la
  parte más pesada de todas y es idéntica para las tres.
- Una sola máquina y un solo sistema operativo.
