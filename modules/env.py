"""
Orion ENV Engine
────────────────────────────────────────────
Atmospheric layer — environment management.
Connects Orion Core to its surrounding variables.

Functions:
    pull("API_KEY")
    push("MODE", "production")
    reveal()
    load()
"""

import os
import modules.code as code

ENV_FILE = ".env"

# ===== Core Functions =====
def load(path=ENV_FILE):
    """Loads variables from a .env file if present."""
    if not os.path.exists(path):
        code.warn(f"No {path} file found.", module="env")
        return False

    code.divider(f"Loading Environment ({path})")
    count = 0
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if "=" in line:
                key, val = map(str.strip, line.split("=", 1))
                os.environ[key] = val
                code.debug(f"{key} = {val}", module="env")
                count += 1
    code.ok(f"{count} environment variables loaded.", module="env")
    return True


def pull(key, default=None):
    """Gets an environment variable."""
    val = os.getenv(key, default)
    if val is not None:
        code.info(f"{key} = {val}", module="env")
    else:
        code.warn(f"{key} not found. Default = {default}", module="env")
    return val


def push(key, value):
    """Sets an environment variable."""
    os.environ[key] = str(value)
    code.ok(f"Set: {key} = {value}", module="env")
    return {"key": key, "value": value}


def reveal():
    """Lists all environment variables."""
    code.frame("ENVIRONMENT SNAPSHOT")
    for k, v in sorted(os.environ.items()):
        code.debug(f"{k}={v}", module="env")
