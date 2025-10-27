from core.lexer import lex
from core.parser import parse
from core.eval import evaluate

def start_repl():
    print("Orion REPL | escribe 'exit' para salir")
    variables = {}
    functions = {}

    while True:
        code = input(">>> ")
        if code.strip() == "exit":
            break

        try:
            tokens = lex(code)
            ast = parse(tokens)
            result = evaluate(ast, variables, functions)
            
            # Si hay un resultado y no es None, mostrarlo
            if result is not None:
                print(result)
                
        except Exception as e:
            print(f"Error: {e}")

if __name__ == "__main__":
    start_repl()
