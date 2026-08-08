"""Playwright, en sus dos formas.

`idiomatico` usa localizadores y pide el texto de cada elemento, que es lo que
enseña su documentación. Playwright no habla por HTTP con un driver sino por
CDP, así que cada lectura cuesta bastante menos que en Selenium — pero sigue
siendo un mensaje por lectura.

`js` manda una sola evaluación a la página, que es el equivalente exacto de lo
que hace `browser.extract` de Orion.

Se usa el navegador YA instalado (`executable_path`) en vez del Chromium que
Playwright se descarga: si cada uno moviera un binario distinto, el número
mediría la diferencia entre navegadores.
"""
import sys

from playwright.sync_api import sync_playwright

from comun import ARGS, CHROME, JS_UNA_LLAMADA, PAGINA, a_numero, informa, repite

modo = sys.argv[1] if len(sys.argv) > 1 else "idiomatico"

# Playwright pone el headless por su cuenta; pasarlo dos veces le molesta.
extra = [a for a in ARGS if not a.startswith("--headless")]

with sync_playwright() as pw:
    navegador = pw.chromium.launch(executable_path=CHROME, headless=True, args=extra)
    pagina = navegador.new_page()
    pagina.goto(PAGINA)

    def idiomatico():
        filas = []
        for c in pagina.query_selector_all(".card"):
            filas.append({
                "nombre": c.query_selector(".title").inner_text().strip(),
                "precio": a_numero(c.query_selector(".price").inner_text()),
                "sku": c.get_attribute("data-sku"),
                "url": c.query_selector("a").get_attribute("href"),
            })
        return filas

    def con_js():
        return [{
            "nombre": f["nombre"],
            "precio": a_numero(f["precio"] or ""),
            "sku": f["sku"],
            "url": f["url"],
        } for f in pagina.evaluate("() => { " + JS_UNA_LLAMADA + " }")]

    filas, ms, vueltas = repite(idiomatico if modo == "idiomatico" else con_js)

    informa(f"playwright_{modo}", filas, ms, vueltas)
    navegador.close()
