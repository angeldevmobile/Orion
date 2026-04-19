"""
Orion Bytecode Compiler
AST (Python) → instrucciones JSON → archivo .orbc
"""
import json
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from core.lexer import lex
from core.parser import parse, parse_expression


class FunctionCompiler:
    """Compilador dedicado para el cuerpo de una función."""
    def __init__(self):
        self.instructions = []
        self.line_table = []
        self.current_line = 0

    def emit(self, instr):
        self.instructions.append(instr)
        self.line_table.append(self.current_line)
        return len(self.instructions) - 1

    def patch(self, idx, new_instr):
        self.instructions[idx] = new_instr

    def current_addr(self):
        return len(self.instructions)

    def compile(self, body):
        for node in body:
            compile_node_into(self, node)
        return self.instructions


class Compiler:
    def __init__(self):
        self.instructions = []
        self.line_table = []
        self.current_line = 0
        self.functions = {}  # nombre -> {params, body}

    def emit(self, instr):
        self.instructions.append(instr)
        self.line_table.append(self.current_line)
        return len(self.instructions) - 1

    def patch(self, idx, new_instr):
        """Parchea una instrucción de salto con la dirección correcta."""
        self.instructions[idx] = new_instr

    def current_addr(self):
        return len(self.instructions)

    def compile_program(self, ast):
        # Primero compilar funciones de usuario en tabla separada
        for node in ast:
            if isinstance(node, tuple) and node[0] == "FN":
                fn_name, params, body = node[1], node[2], node[3]
                fc = FunctionCompiler()
                for stmt in body:
                    compile_node_into(fc, stmt)
                fc.emit("LoadNull")
                fc.emit("Return")
                self.functions[fn_name] = {
                    "params": params,
                    "body": fc.instructions,
                    "lines": fc.line_table,
                }
        # Luego compilar el código principal (ignorando FN)
        for node in ast:
            if not (isinstance(node, tuple) and node[0] == "FN"):
                self.compile_node(node)
        self.emit({"Halt": None})
        return self.instructions

    def compile_node(self, node):
        compile_node_into(self, node)


# Función libre — compila un nodo en cualquier contexto (main o función)
def compile_node_into(ctx, node):
    if not isinstance(node, tuple):
        return
    # Extraer número de línea si está presente como último elemento entero
    if len(node) >= 2 and isinstance(node[-1], int) and not isinstance(node[-1], bool):
        ctx.current_line = node[-1]
        node = node[:-1]
    tag = node[0]

    if tag == "ASSIGN":
        _, name, expr = node
        compile_expr_into(ctx, expr)
        ctx.emit({"StoreVar": name})

    elif tag == "CONST":
        _, name, expr = node
        compile_expr_into(ctx, expr)
        ctx.emit({"StoreConst": name})

    elif tag == "CALL" and node[1] == "show":
        _, _, args, _ = node
        for arg in args:
            compile_expr_into(ctx, arg)
        ctx.emit("Show")

    elif tag == "CALL":
        _, name, args, _ = node
        fn_name = name[1] if isinstance(name, tuple) else name
        for arg in args:
            compile_expr_into(ctx, arg)
        ctx.emit({"Call": [fn_name, len(args)]})

    elif tag == "IF":
        _, cond, body_true, body_false = node
        compile_expr_into(ctx, cond)
        jump_false = ctx.emit({"JumpIfFalse": 0})
        for stmt in body_true:
            compile_node_into(ctx, stmt)
        if body_false:
            jump_end = ctx.emit({"Jump": 0})
            ctx.patch(jump_false, {"JumpIfFalse": ctx.current_addr()})
            for stmt in body_false:
                compile_node_into(ctx, stmt)
            ctx.patch(jump_end, {"Jump": ctx.current_addr()})
        else:
            ctx.patch(jump_false, {"JumpIfFalse": ctx.current_addr()})

    elif tag == "IF_ELSIF":
        _, cond, body_true, elsif_parts, body_false = node
        compile_expr_into(ctx, cond)
        jump_false = ctx.emit({"JumpIfFalse": 0})
        for stmt in body_true:
            compile_node_into(ctx, stmt)
        jumps_end = [ctx.emit({"Jump": 0})]
        ctx.patch(jump_false, {"JumpIfFalse": ctx.current_addr()})
        for (elsif_cond, elsif_body) in elsif_parts:
            compile_expr_into(ctx, elsif_cond)
            jf = ctx.emit({"JumpIfFalse": 0})
            for stmt in elsif_body:
                compile_node_into(ctx, stmt)
            jumps_end.append(ctx.emit({"Jump": 0}))
            ctx.patch(jf, {"JumpIfFalse": ctx.current_addr()})
        for stmt in body_false:
            compile_node_into(ctx, stmt)
        end_addr = ctx.current_addr()
        for j in jumps_end:
            ctx.patch(j, {"Jump": end_addr})

    elif tag == "WHILE":
        _, cond, body = node
        loop_start = ctx.current_addr()
        compile_expr_into(ctx, cond)
        jump_end = ctx.emit({"JumpIfFalse": 0})
        for stmt in body:
            compile_node_into(ctx, stmt)
        ctx.emit({"Jump": loop_start})
        ctx.patch(jump_end, {"JumpIfFalse": ctx.current_addr()})

    elif tag == "FOR_RANGE":
        _, var, start, end, body = node
        compile_expr_into(ctx, start)
        ctx.emit({"StoreVar": var})
        loop_start = ctx.current_addr()
        ctx.emit({"LoadVar": var})
        compile_expr_into(ctx, end)
        ctx.emit("LtEq")
        jump_end = ctx.emit({"JumpIfFalse": 0})
        for stmt in body:
            compile_node_into(ctx, stmt)
        ctx.emit({"LoadVar": var})
        ctx.emit({"LoadInt": 1})
        ctx.emit("Add")
        ctx.emit({"StoreVar": var})
        ctx.emit({"Jump": loop_start})
        ctx.patch(jump_end, {"JumpIfFalse": ctx.current_addr()})

    elif tag == "EXPR":
        compile_expr_into(ctx, node[1])
        ctx.emit("Pop")

    elif tag == "FN":
        # Las funciones se compilan por separado en compile_program
        pass

    elif tag == "RETURN":
        _, expr = node
        if expr is not None:
            compile_expr_into(ctx, expr)
        ctx.emit("Return")


def _split_interpolation(s):
    """Divide una string en partes literales y expresiones ${...}."""
    parts = []
    i = 0
    current_text = ""
    while i < len(s):
        if s[i] == '$' and i + 1 < len(s) and s[i + 1] == '{':
            if current_text:
                parts.append(('text', current_text))
            current_text = ""
            i += 2
            depth = 1
            expr_start = i
            while i < len(s) and depth > 0:
                if s[i] == '{':
                    depth += 1
                elif s[i] == '}':
                    depth -= 1
                i += 1
            parts.append(('expr', s[expr_start:i - 1]))
        else:
            current_text += s[i]
            i += 1
    if current_text:
        parts.append(('text', current_text))
    return parts


def _compile_interpolated_str(ctx, raw):
    """Compila una string interpolada como secuencia de Add."""
    parts = _split_interpolation(raw)
    if not parts:
        ctx.emit({"LoadStr": ""})
        return
    # Emitir primera parte
    kind, content = parts[0]
    if kind == 'text':
        ctx.emit({"LoadStr": content})
    else:
        tokens = lex(content)
        expr, _ = parse_expression(tokens, 0)
        compile_expr_into(ctx, expr)
    # Emitir el resto con Add después de cada una
    for kind, content in parts[1:]:
        if kind == 'text':
            ctx.emit({"LoadStr": content})
        else:
            tokens = lex(content)
            expr, _ = parse_expression(tokens, 0)
            compile_expr_into(ctx, expr)
        ctx.emit("Add")


# Función libre para compilar expresiones en cualquier contexto
def compile_expr_into(ctx, expr):
    if expr is None:
        ctx.emit("LoadNull")
        return
    if isinstance(expr, bool):
        ctx.emit({"LoadBool": expr})
        return
    if isinstance(expr, int):
        ctx.emit({"LoadInt": expr})
        return
    if isinstance(expr, float):
        ctx.emit({"LoadFloat": expr})
        return
    if isinstance(expr, str):
        inner = expr[1:-1] if (expr.startswith('"') and expr.endswith('"')) else expr
        if '${' in inner:
            _compile_interpolated_str(ctx, inner)
        else:
            ctx.emit({"LoadStr": inner})
        return
    if not isinstance(expr, tuple):
        return

    tag = expr[0]

    if tag == "IDENT":
        ctx.emit({"LoadVar": expr[1]})

    elif tag == "BINARY_OP":
        _, op, left, right = expr
        compile_expr_into(ctx, left)
        compile_expr_into(ctx, right)
        op_map = {
            "+": "Add", "-": "Sub", "*": "Mul", "/": "Div",
            "%": "Mod", "**": "Pow",
            "==": "Eq", "!=": "NotEq",
            "<": "Lt", "<=": "LtEq", ">": "Gt", ">=": "GtEq",
            "&&": "And", "||": "Or",
        }
        ctx.emit(op_map.get(op, f"Unknown_{op}"))

    elif tag == "UNARY_OP":
        _, op, operand = expr
        compile_expr_into(ctx, operand)
        if op == "!":
            ctx.emit("Not")
        elif op == "-":
            ctx.emit("Neg")

    elif tag == "CALL":
        _, name, args, _ = expr
        fn_name = name[1] if isinstance(name, tuple) else name
        if fn_name == "show":
            for arg in args:
                compile_expr_into(ctx, arg)
            ctx.emit("Show")
        else:
            for arg in args:
                compile_expr_into(ctx, arg)
            ctx.emit({"Call": [fn_name, len(args)]})

    elif tag == "LIST":
        _, elements = expr
        for el in elements:
            compile_expr_into(ctx, el)
        ctx.emit({"MakeList": len(elements)})

    elif tag == "DICT":
        _, items = expr
        for key, val in items:
            ctx.emit({"LoadStr": key})
            compile_expr_into(ctx, val)
        ctx.emit({"MakeDict": len(items)})

    elif tag == "INDEX":
        _, obj, idx = expr
        compile_expr_into(ctx, obj)
        compile_expr_into(ctx, idx)
        ctx.emit("GetIndex")


def compile_file(source_path: str, output_path: str = None):
    with open(source_path, "r", encoding="utf-8") as f:
        source = f.read()

    tokens = lex(source)
    ast = parse(tokens)

    compiler = Compiler()
    compiler.compile_program(ast)

    if output_path is None:
        output_path = source_path.replace(".orx", ".orbc")

    bytecode = {
        "main": compiler.instructions,
        "lines": compiler.line_table,
        "functions": compiler.functions,
    }

    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(bytecode, f, indent=2)

    total = len(compiler.instructions) + sum(len(f["body"]) for f in compiler.functions.values())
    print(f"Compilado: {source_path} -> {output_path}")
    print(f"Main: {len(compiler.instructions)} instrucciones | Funciones: {len(compiler.functions)} ({total} total)")
    return output_path


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Uso: python bytecode_compiler.py archivo.orx [salida.orbc]")
        sys.exit(1)
    src = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else None
    compile_file(src, out)
