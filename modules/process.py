"""
Orion PROCESS Engine
────────────────────────────────────────────
Executor of commands. Engine of motion.

Más que ejecutar comandos, interpreta intención.
Cada proceso se representa con energía visual.

Modes:
    - execute()     → comando directo con captura
    - stream()      → flujo continuo en tiempo real
    - background()  → proceso paralelo sintético
    - timed()       → medición temporal cuántica
"""

import subprocess
import shutil
import time
import modules.code as code

def execute(command, capture=True):
    """Ejecuta un comando y devuelve salida estructurada."""
    code.frame(f"EXECUTE → {command}", style="cyan")

    try:
        result = subprocess.run(command, shell=True, capture_output=capture, text=True)
        if result.returncode == 0:
            code.ok("Execution completed successfully.", module="process")
        else:
            code.error(f"Command failed with code {result.returncode}.", module="process")
    except Exception as e:
        code.error(f"Execution error: {e}", module="process")
        return {"code": -1, "out": "", "err": str(e)}

    return {
        "code": result.returncode,
        "out": result.stdout.strip(),
        "err": result.stderr.strip(),
    }


def stream(command):
    """Ejecuta un comando y muestra salida en tiempo real."""
    code.frame(f"STREAM → {command}", style="yellow")
    proc = subprocess.Popen(command, shell=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    for line in proc.stdout:
        code.debug(line.strip(), module="process")
    proc.wait()

    if proc.returncode == 0:
        code.ok("Stream ended successfully.", module="process")
    else:
        code.warn(f"Stream ended with code {proc.returncode}.", module="process")

    return {"done": True, "code": proc.returncode}


def background(command):
    """Ejecuta un comando en segundo plano."""
    code.frame(f"BACKGROUND → {command}", style="magenta")
    proc = subprocess.Popen(command, shell=True)
    code.ok(f"Process started (PID={proc.pid}).", module="process")
    return {"pid": proc.pid}


def check_dependency(cmd):
    """Verifica si un comando existe en el sistema."""
    exists = shutil.which(cmd) is not None
    if exists:
        code.ok(f"Dependency '{cmd}' found.", module="process")
    else:
        code.error(f"Dependency '{cmd}' missing.", module="process")
    return exists


def execute_timed(command):
    """Ejecuta un comando midiendo el tiempo total."""
    start = time.time()
    result = execute(command)
    elapsed = round(time.time() - start, 2)
    code.debug(f"Elapsed: {elapsed}s", module="process")
    result["elapsed"] = elapsed
    return result
