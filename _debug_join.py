import traceback, sys
sys.path.insert(0, ".")
from core.eval import evaluate, NATIVE_FUNCTIONS
from core.lexer import lex as tokenize
from core.parser import parse

code = '''partes = split("uno,dos,tres", ",")
show partes
result = join("-", partes)
show result
'''
tokens = tokenize(code)
ast = parse(tokens)
print("AST:", ast)
vars_ = {}
fns_ = {}
try:
    evaluate(ast, vars_, fns_)
except Exception as e:
    traceback.print_exc()
