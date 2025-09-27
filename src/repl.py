from core.lexer import lex
from core.parser import parse
from core.eval import evaluate

def start_repl():
    print("Orion REPL | escribe 'exit' para salir")
    variables = {}

    while True:
        code = input(">>> ")
        if code.strip() == "exit":
            break

        tokens = lex(code)
        ast = parse(tokens)
        evaluate(ast, variables)

if __name__ == "__main__":
    start_repl()
