import sys
import os
import time

project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), '..'))
sys.path.insert(0, project_root)

from core.eval import evaluate
from core.lexer import lex
from core.parser import parse
from core.builtins import load_builtins

def run_orion(filename, verbose=False):
    with open(filename, "r", encoding="utf-8") as f:
        code = f.read()

    t_total = time.perf_counter()

    try:
        t0 = time.perf_counter()
        tokens = lex(code)
        t_lex = time.perf_counter() - t0
    except Exception as e:
        print(f"[ORION ERROR] Lexer: {e}")
        return

    try:
        t0 = time.perf_counter()
        ast = parse(tokens)
        t_parse = time.perf_counter() - t0
    except Exception as e:
        print(f"[ORION ERROR] Parser: {e}")
        return

    if verbose:
        print("=== TOKENS ===")
        print(tokens)
        print("=== AST ===")
        print(ast)

    print("=== EJECUCIÓN ===")
    variables = {}
    functions = {}
    load_builtins(functions)

    try:
        t0 = time.perf_counter()
        evaluate(ast, variables, functions)
        t_eval = time.perf_counter() - t0
    except Exception as e:
        print(f"[ORION ERROR] Runtime: {e}")
        return

    t_total = time.perf_counter() - t_total
    print()
    print("─" * 50)
    print(f"  Lexer   : {t_lex * 1000:.3f} ms")
    print(f"  Parser  : {t_parse * 1000:.3f} ms")
    print(f"  Eval    : {t_eval * 1000:.3f} ms")
    print(f"  Total   : {t_total * 1000:.3f} ms")
    print("─" * 50)

if __name__ == "__main__":
    args = sys.argv[1:]
    if not args:
        print("Uso: python main.py archivo.orx [--verbose]")
        sys.exit(1)
    verbose = "--verbose" in args
    filename = next(a for a in args if not a.startswith("--"))
    run_orion(filename, verbose=verbose)
