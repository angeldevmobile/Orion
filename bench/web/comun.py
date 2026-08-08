"""Lo compartido por las variantes de Python: mismo navegador, misma tarea.

La comparación solo vale si las tres herramientas mueven **el mismo binario de
Chrome**, en headless y sin imágenes. Si una descargara su propio Chromium o
dejara las imágenes activadas, el número mediría eso y no la automatización.
"""
import hashlib
import os
import sys
from pathlib import Path

# El mismo ejecutable que resuelve Orion (`web.info()["path"]`).
CHROME = os.environ.get(
    "ORION_CHROME", r"C:\Program Files\Google\Chrome\Application\chrome.exe"
)

PAGINA = (Path(__file__).with_name("catalogo.html")).as_uri()

# Las mismas banderas que pone Orion por defecto.
ARGS = [
    "--headless=new",
    "--blink-settings=imagesEnabled=false",
    "--disable-gpu",
    "--no-first-run",
    "--no-default-browser-check",
]


def resumen(filas):
    """Huella de lo extraído, para probar que todos sacaron LO MISMO.

    Sin esto, una variante podría ser rapidísima por leer de menos y el
    benchmark lo premiaría.

    En céntimos enteros y no en decimales con formato: "1234.50" y "1,234.50"
    son el mismo número y huellas distintas, y se acabaría comparando cómo
    imprime cada lenguaje.
    """
    total = 0
    h = hashlib.sha256()
    for f in filas:
        c = int(round((f["precio"] or 0) * 100))
        total += c
        h.update(f"{f['nombre']}|{c}|{f['sku']}|{f['url']}".encode())
    return len(filas), total, h.hexdigest()[:12]


def repite(extraer, presupuesto_s=15.0, minimo=3, maximo=7):
    # Con BENCH_UNA se hace una sola extracción. Lo usa la pasada que mide el
    # proceso entero: si ahí dentro hubiera siete extracciones, el "tiempo de
    # pared" ya no sería el coste de la tarea sino el de repetirla.
    if os.environ.get("BENCH_UNA"):
        minimo, maximo, presupuesto_s = 1, 1, 0.0
    return _repite(extraer, presupuesto_s, minimo, maximo)


def _repite(extraer, presupuesto_s, minimo, maximo):
    """Repite la extracción y se queda con el MEJOR tiempo.

    Hace falta porque la medición suelta no era publicable: la misma extracción
    con Selenium daba entre 20 y 430 ms según la vuelta. El ruido —el navegador
    calentando, el recolector, otro proceso— solo puede **sumar** tiempo, nunca
    restarlo, así que el mínimo es el que más se acerca al coste real y el que
    no depende de lo ocupada que estuviera la máquina.

    Se repite hasta gastar el presupuesto o llegar al máximo, con un mínimo de
    tres: así una variante que tarda trece segundos por vuelta no convierte el
    benchmark en algo que nadie va a esperar.
    """
    import time as _t

    tiempos = []
    filas = []
    arranque = _t.perf_counter()
    while len(tiempos) < maximo:
        t0 = _t.perf_counter()
        filas = extraer()
        tiempos.append((_t.perf_counter() - t0) * 1000)
        if len(tiempos) >= minimo and (_t.perf_counter() - arranque) > presupuesto_s:
            break
    return filas, min(tiempos), len(tiempos)


def informa(etiqueta, filas, ms_extraccion, vueltas=1):
    n, total, huella = resumen(filas)
    print(f"{etiqueta}: filas={n} suma_cent={total} huella={huella} "
          f"extraccion_ms={ms_extraccion:.1f} vueltas={vueltas}")
    if n == 0:
        sys.exit("no se extrajo nada")


def a_numero(texto):
    """`1.234,56 EUR` y `1,234.56` conviven en las páginas reales.

    Orion hace esta conversión dentro de `extract`; aquí hay que escribirla,
    que es parte del trabajo que se está comparando.
    """
    t = "".join(c for c in texto if c.isdigit() or c in ",.-")
    if not t:
        return None
    coma, punto = t.rfind(","), t.rfind(".")
    if coma > -1 and punto > -1:
        t = t.replace(".", "").replace(",", ".") if coma > punto else t.replace(",", "")
    elif coma > -1:
        t = t.replace(",", ".") if len(t) - coma - 1 <= 2 else t.replace(",", "")
    try:
        return float(t)
    except ValueError:
        return None


# El mismo JavaScript para las variantes "en una llamada" de Selenium y
# Playwright: lo que hace `browser.extract` por dentro.
JS_UNA_LLAMADA = """
return Array.from(document.querySelectorAll('.card')).map(c => {
  const t = c.querySelector('.title');
  const p = c.querySelector('.price');
  const a = c.querySelector('a');
  return {
    nombre: t ? t.textContent.trim() : null,
    precio: p ? p.textContent.trim() : null,
    sku: c.getAttribute('data-sku'),
    url: a ? a.getAttribute('href') : null
  };
});
"""
