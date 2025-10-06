# modules/show.py
import log

def show(*args, level="ok", module="orion"):
    """
    Función universal de Orion para mostrar mensajes usando log.py
    """
    mensaje = " ".join(str(a) for a in args)

    if level == "info":
        log.info(mensaje, module=module)
    elif level == "warn":
        log.warn(mensaje, module=module)
    elif level == "error":
        log.error(mensaje, module=module)
    elif level == "debug":
        log.debug(mensaje, module=module)
    elif level == "proc":
        log.progress(module, mensaje, 100)  # Para progreso completo
    else:
        log.ok(mensaje, module=module)
