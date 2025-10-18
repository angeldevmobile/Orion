"""
Orion SHOW Engine
────────────────────────────────────────────
Visualizador universal del lenguaje.

Fusión de log, consola y narrativa sintética.
Cada línea tiene intención, energía y contexto.

Capacidades:
- Autoformato cósmico según tipo de mensaje
- Detección de bloques largos
- Animación de pulso o traza temporal
- Interfaz con IO futurista o CODE visual
"""

import modules.code as code
import lib.io as io
import re
import math
import time

def show(*args, level="ok", module="orion", env=None, pulse=False, trace=False, delay=0):
    """
    Orion Universal Show — salida inteligente y expresiva.

    Args:
        *args: texto(s) o valores a mostrar
        level: nivel de log (ok, info, warn, error, debug, proc, trace)
        module: origen lógico del mensaje
        env: entorno opcional (pasado a io.show futurista)
        pulse: si True, muestra un efecto de latido vivo
        trace: si True, inicia una traza visual extendida
        delay: si > 0, imprime con retardo (efecto sintético)
    """

    mensaje = " ".join(str(a) for a in args)
    multiline = "\n" in mensaje or len(mensaje) > 140

    # Efecto de retardo (simula pensamiento o transmisión)
    if delay > 0:
        for ch in mensaje:
            print(ch, end="", flush=True)
            time.sleep(delay)
        print()
        return

    # Modo multilinea visual
    if multiline:
        code.divider(f"{module.upper()} MULTILINE")
        for line in mensaje.splitlines():
            code.debug(line, module=module)
        code.divider(f"{module.upper()} END")
        return

    # Efecto de pulso
    if pulse:
        code.pulse(level.upper(), mensaje)
        return

    # Modo traza sintética
    if trace:
        code.trace_start(mensaje)
        return

    # Niveles visuales
    match level.lower():
        case "info":
            code.info(mensaje, module=module)
        case "warn":
            code.warn(mensaje, module=module)
        case "error":
            code.error(mensaje, module=module)
        case "debug":
            code.debug(mensaje, module=module)
        case "proc":
            code.progress(module, mensaje, 100)
        case "trace":
            code.trace(mensaje, module=module)
        case "frame":
            code.frame(mensaje, style="magenta")
        case _:
            io.show(*args, env=env)  # fallback futurista
