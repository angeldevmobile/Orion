""" Sistema Orion: versión, args, entorno. """
import sys, os

VERSION = "0.1.0 Orion"

def args(): return sys.argv[1:]
def exit(code=0): sys.exit(code)
def env(key, default=None): return os.getenv(key, default)
def platform(): return sys.platform
