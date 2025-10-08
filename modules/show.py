import modules.code as code
import lib.io as io

def show(*args, level="ok", module="orion", env=None):
    mensaje = " ".join(str(a) for a in args)

    if level == "info":
        code.info(mensaje, module=module)
    elif level == "warn":
        code.warn(mensaje, module=module)
    elif level == "error":
        code.error(mensaje, module=module)
    elif level == "debug":
        code.debug(mensaje, module=module)
    elif level == "proc":
        code.progress(module, mensaje, 100)
    else:
        io.show(*args, env=env)  # usa el show futurista de io.py
