import sys
import os

# La raíz del proyecto al sys.path
project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), '..'))
sys.path.insert(0, project_root)

from core.eval import evaluate
from core.lexer import lex
from core.parser import parse

def run_orion(filename):
    with open(filename, "r", encoding="utf-8") as f:
        code = f.read()

    try:
        tokens = lex(code)
    except RuntimeError as e:
        print(f"Error de sintaxis: {e}")
        return  # Detiene la ejecución si hay error

    print("=== TOKENS ===")
    print(tokens)
    ast = parse(tokens)
    print("=== AST ===")
    print(ast)
    print("=== EJECUCIÓN ===")
    evaluate(ast)

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Uso: python main.py funcion.orx")
    else:
        run_orion(sys.argv[1])
