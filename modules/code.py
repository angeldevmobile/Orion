"""
Orion LOG Engine
────────────────────────────────────────────
Structured. Expressive. Cosmic.

Logs are not prints — they are pulses of the Orion Core.
Wide visuals, adaptive color, and a living console aesthetic.

Levels:
    DEBUG, INFO, WARN, ERROR, OK, PROC

Usage:
    log.info("System online", module="core")
"""

import datetime
import shutil
import re
import os

# ===== Color System =====
RESET = "\033[0m"
BOLD = "\033[1m"
DIM = "\033[2m"

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
}

# ===== Settings =====
MODE = os.getenv("ORION_LOG_MODE", "extended")   # "extended" or "compact"
LOG_FILE = os.getenv("ORION_LOG_FILE", "orion.log")
WIDTH_DEFAULT = 120

# ===== Core Utils =====
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

# ===== Core Line Formatter =====
def _line(level, module, message, color, bg=None):
    ts = _timestamp()
    # Nivel con fondo y negrita
    lvl = f"{(bg or color)}{BOLD}[{level:<5}]{RESET}"
    # Módulo en magenta y negrita
    mod = f"{FG['magenta']}{BOLD}[{module.upper()}]{RESET}"
    # Mensaje en color
    msg = f"{color}{message}{RESET}"

    line = f"{ts} {lvl} {mod} → {msg}" if MODE == "extended" else f"{lvl} {mod} {msg}"
    width = _width()
    _write_file(line)
    return line[:width]

# ===== Public Log Levels =====
def info(message, module="system"):  print(_line("INFO", module, message, FG["cyan"], BG["cyan"]))
def ok(message, module="system"):    print(_line("OK", module, message, FG["green"], BG["green"]))
def warn(message, module="system"):  print(_line("WARN", module, message, FG["yellow"], BG["yellow"]))
def error(message, module="system"): print(_line("ERROR", module, message, FG["red"], BG["red"]))
def debug(message, module="system"): print(_line("DEBUG", module, message, FG["gray"], BG["gray"]))

# ===== Visual Enhancements =====
def divider(title=""):
    width = _width()
    pad = f"{FG['cyan']}{'─' * (width - len(title) - 2)}{RESET}"
    line = f"{FG['yellow']}{BOLD}{title} {pad}{RESET}"
    _write_file(line)
    print(line)

def frame(title):
    width = _width()
    pad = f"{FG['magenta']}{'═' * (width - len(title) - 2)}{RESET}"
    print(f"{FG['magenta']}{BOLD}╔ {title} {pad}{RESET}")
    print(f"{FG['magenta']}{BOLD}╚{FG['magenta']}{'═' * (width - 1)}{RESET}")

def progress(module, step, percent):
    bar_len = 40
    filled = int(bar_len * percent / 100)
    bar = f"{FG['green']}{'█' * filled}{FG['gray']}{'░' * (bar_len - filled)}{RESET}"
    line = _line("PROC", module, f"{step} {bar} {percent}%", FG["cyan"])
    print(line, end="\r" if percent < 100 else "\n")

def trace_start(title="TRACE START"):
    divider(f"{FG['cyan']}{BOLD}── {title} ──{RESET}")

def trace_end(title="TRACE END"):
    divider(f"{FG['green']}{BOLD}── {title} ──{RESET}")
