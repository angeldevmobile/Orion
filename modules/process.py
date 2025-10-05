"""
Orion PROCESS Engine
────────────────────────────────────────────
Executor of commands. Engine of movement.

Functions:
    execute("ls -la")
    stream("ping google.com")
    background("python server.py")
    check_dependency("git")
    execute_timed("npm install")
"""

import subprocess
import shutil
import time
import log

def execute(command, capture=True):
    """Runs a shell command and returns structured output."""
    log.divider(f"EXECUTE {command}")
    result = subprocess.run(
        command,
        shell=True,
        capture_output=capture,
        text=True
    )

    if result.returncode == 0:
        log.ok("Execution completed successfully.", module="process")
    else:
        log.error(f"Command failed with code {result.returncode}.", module="process")

    return {
        "code": result.returncode,
        "out": result.stdout.strip(),
        "err": result.stderr.strip()
    }


def stream(command):
    """Streams command output line by line."""
    log.divider(f"STREAM {command}")
    proc = subprocess.Popen(
        command,
        shell=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True
    )
    for line in proc.stdout:
        log.debug(line.strip(), module="process")
    proc.wait()
    if proc.returncode == 0:
        log.ok("Stream ended successfully.", module="process")
    else:
        log.warn(f"Stream ended with code {proc.returncode}.", module="process")
    return {"done": True, "code": proc.returncode}


def background(command):
    """Runs a command in the background."""
    log.divider(f"BACKGROUND {command}")
    proc = subprocess.Popen(command, shell=True)
    log.ok(f"Process started (PID={proc.pid}).", module="process")
    return {"pid": proc.pid}


def check_dependency(cmd):
    """Checks if a system command exists."""
    exists = shutil.which(cmd) is not None
    if exists:
        log.ok(f"Dependency '{cmd}' found.", module="process")
    else:
        log.error(f"Dependency '{cmd}' missing.", module="process")
    return exists


def execute_safe(command):
    """Executes a command safely, catching all exceptions."""
    try:
        return execute(command)
    except Exception as e:
        log.error(f"Exception while executing '{command}': {e}", module="process")
        return {"code": -1, "out": "", "err": str(e)}


def execute_timed(command):
    """Runs a command and reports execution time."""
    start = time.time()
    result = execute(command)
    elapsed = round(time.time() - start, 2)
    log.debug(f"Elapsed: {elapsed}s", module="process")
    result["elapsed"] = elapsed
    return result
