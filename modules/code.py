"""
Orion CODE Engine
────────────────────────────────────────────
Structured. Expressive. Living.

Logs are not mere prints — they are pulses of the Orion Core.
Adaptive visuals, contextual awareness, and a console that breathes.

Modes:
    - compact → fast output, minimal format
    - extended → full aesthetic + timestamps
    - cosmic → adds sentiment & AI-driven summaries

Levels:
    DEBUG, INFO, WARN, ERROR, OK, PROC, TRACE

Usage:
    code.info("Boot sequence initialized", module="core")
    code.progress("orion-core", "Initializing modules", 65)
"""

import datetime, shutil, os, re, math, random

# ============================================================
# ─── Color & Style Engine ───────────────────────────────────
# ============================================================
RESET = "\033[0m"
BOLD = "\033[1m"
DIM = "\033[2m"
BLINK = "\033[5m"

FG = {
    "gray": "\033[90m",
    "cyan": "\033[96m",
    "yellow": "\033[93m",
    "red": "\033[91m",
    "green": "\033[92m",
    "white": "\033[97m",
    "magenta": "\033[95m",
}

BG = {
    "cyan": "\033[46m",
    "green": "\033[42m",
    "yellow": "\033[43m",
    "red": "\033[41m",
    "gray": "\033[100m",
    "purple": "\033[45m",
}

# ============================================================
# ─── Config ─────────────────────────────────────────────────
# ============================================================
MODE = os.getenv("ORION_LOG_MODE", "cosmic")     # compact | extended | cosmic
LOG_FILE = os.getenv("ORION_LOG_FILE", "orion.log")
WIDTH_DEFAULT = 120

# ============================================================
# ─── Internals ──────────────────────────────────────────────
# ============================================================
def _width():
    try:
        return shutil.get_terminal_size((WIDTH_DEFAULT, 30)).columns
    except Exception:
        return WIDTH_DEFAULT

def _timestamp():
    return datetime.datetime.now().strftime("[%Y-%m-%d %H:%M:%S]")

def _strip_ansi(s):
    return re.sub(r"\x1b\[[0-9;]*m", "", s)

def _write_file(line):
    try:
        with open(LOG_FILE, "a", encoding="utf-8") as f:
            f.write(_strip_ansi(line) + "\n")
    except Exception:
        pass

# ============================================================
# ─── Emotion & Sentiment Engine ─────────────────────────────
# ============================================================
def _emotion(message):
    """Clasifica emoción según el tono del mensaje."""
    msg = message.lower()
    if any(x in msg for x in ["fail", "error", "fatal", "denied"]): return "red"
    if any(x in msg for x in ["warn", "delay", "retry"]): return "yellow"
    if any(x in msg for x in ["ok", "done", "success", "ready"]): return "green"
    if any(x in msg for x in ["init", "boot", "load", "start"]): return "cyan"
    return "white"

def _summary(message):
    """Modo cosmic: crea una descripción corta tipo IA del log."""
    verbs = ["syncing", "processing", "resolving", "connecting", "evolving"]
    tone = random.choice(["stabilized", "detected", "activated", "balanced"])
    return f"{random.choice(verbs).capitalize()} — {tone}"

# ============================================================
# ─── Core Line Builder ──────────────────────────────────────
# ============================================================
def _line(level, module, message, color=None, bg=None):
    ts = _timestamp()
    width = _width()

    if not color:
        color = FG[_emotion(message)]

    lvl = f"{(bg or color)}{BOLD}[{level:<5}]{RESET}"
    mod = f"{FG['magenta']}{BOLD}[{module.upper()}]{RESET}"

    cosmic_hint = f" {DIM}({ _summary(message) }){RESET}" if MODE == "cosmic" else ""
    msg = f"{color}{message}{RESET}{cosmic_hint}"

    if MODE == "compact":
        line = f"{lvl} {mod} {msg}"
    else:
        line = f"{ts} {lvl} {mod} → {msg}"

    _write_file(line)
    return line[:width]

# ============================================================
# ─── Public Log Levels ──────────────────────────────────────
# ============================================================
def info(msg, module="system"):  print(_line("INFO", module, msg, FG["cyan"], BG["cyan"]))
def ok(msg, module="system"):    print(_line("OK", module, msg, FG["green"], BG["green"]))
def warn(msg, module="system"):  print(_line("WARN", module, msg, FG["yellow"], BG["yellow"]))
def error(msg, module="system"): print(_line("ERROR", module, msg, FG["red"], BG["red"]))
def debug(msg, module="system"): print(_line("DEBUG", module, msg, FG["gray"], BG["gray"]))
def trace(msg, module="system"): print(_line("TRACE", module, msg, FG["magenta"], BG["purple"]))

# ============================================================
# ─── Visual Utilities ───────────────────────────────────────
# ============================================================
def divider(title=""):
    width = _width()
    pad = f"{FG['gray']}{'─' * (width - len(title) - 3)}{RESET}"
    line = f"{FG['cyan']}{BOLD}─ {title} {pad}{RESET}"
    _write_file(line)
    print(line)

def progress(module, step, percent):
    bar_len = 40
    filled = int(bar_len * percent / 100)
    bar = f"{FG['green']}{'█' * filled}{FG['gray']}{'░' * (bar_len - filled)}{RESET}"
    line = _line("PROC", module, f"{step} {bar} {percent:>3.0f}%", FG["cyan"])
    print(line, end="\r" if percent < 100 else "\n")

def pulse(level="INFO", text="...", frequency=0.08):
    """Efecto visual de 'latido' en la consola, tipo vivo."""
    import time, sys
    for i in range(3):
        sys.stdout.write(f"{FG['magenta']}{BOLD}[{level}] {text}{RESET}\r")
        sys.stdout.flush()
        time.sleep(frequency)
        sys.stdout.write(f"{DIM}{FG['magenta']}[{level}] {text}{RESET}\r")
        sys.stdout.flush()
        time.sleep(frequency)
    print()

def trace_start(title="TRACE START"):
    divider(f"{FG['cyan']}{BOLD}── {title} ──{RESET}")

def trace_end(title="TRACE END"):
    divider(f"{FG['green']}{BOLD}── {title} ──{RESET}")

def frame(title: str, style="magenta"):
    """Muestra un marco decorativo alrededor de un título."""
    width = _width()
    border = "═" * (width - len(title) - 4)
    color = FG.get(style, FG["magenta"])
    print(f"{color}{BOLD}╔═ {title} {border}{RESET}")
    print(f"{color}{BOLD}╚{border}{RESET}")

