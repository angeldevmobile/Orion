"""Selenium, en sus dos formas.

`idiomatico` es como se escribe normalmente y como sale en su documentación:
localizar los elementos y pedirle a cada uno su texto o su atributo. Cada una de
esas peticiones es un viaje HTTP al driver, así que 500 filas por 4 campos son
2.000 viajes más los de localizar.

`js` es la salida que conoce quien ya se ha peleado con esto: mandar un
JavaScript que lo resuelva dentro de la página y volver con los datos hechos.
Está aquí porque comparar solo contra la forma lenta sería un espantapájaros:
lo que se quiere medir es qué cuesta hacerlo bien en cada herramienta.
"""
import sys

from selenium import webdriver
from selenium.webdriver.chrome.options import Options
from selenium.webdriver.common.by import By

from comun import ARGS, CHROME, JS_UNA_LLAMADA, PAGINA, a_numero, informa, repite

modo = sys.argv[1] if len(sys.argv) > 1 else "idiomatico"

opciones = Options()
opciones.binary_location = CHROME
for a in ARGS:
    opciones.add_argument(a)

driver = webdriver.Chrome(options=opciones)
try:
    driver.get(PAGINA)

    def idiomatico():
        filas = []
        for c in driver.find_elements(By.CSS_SELECTOR, ".card"):
            filas.append({
                "nombre": c.find_element(By.CSS_SELECTOR, ".title").text.strip(),
                "precio": a_numero(c.find_element(By.CSS_SELECTOR, ".price").text),
                "sku": c.get_attribute("data-sku"),
                "url": c.find_element(By.CSS_SELECTOR, "a").get_attribute("href"),
            })
        return filas

    def con_js():
        return [{
            "nombre": f["nombre"],
            "precio": a_numero(f["precio"] or ""),
            "sku": f["sku"],
            "url": f["url"],
        } for f in driver.execute_script(JS_UNA_LLAMADA)]

    filas, ms, vueltas = repite(idiomatico if modo == "idiomatico" else con_js)

    # `get_attribute("href")` devuelve la URL absoluta y el resto relativa; se
    # normaliza para que la huella sea comparable entre herramientas.
    for f in filas:
        if f["url"]:
            f["url"] = "/p/" + f["url"].rstrip("/").rsplit("/", 1)[-1]

    informa(f"selenium_{modo}", filas, ms, vueltas)
finally:
    driver.quit()
