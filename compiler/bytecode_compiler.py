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

# Compilador raíz activo (para que compile_node_into dentro de funciones
# pueda registrar funciones de módulos importados)
_current_compiler = None


def _compile_use_module(module_path, alias, selective):
    """Carga un .orx, compila sus funciones y las inserta en _current_compiler.functions."""
    global _current_compiler
    if _current_compiler is None:
        return

    # Quitar comillas si las tiene
    module_path = module_path.strip('"\'')

    # Resolver ruta del módulo
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    candidates = [
        module_path,
        module_path + ".orx",
        os.path.join(base_dir, module_path),
        os.path.join(base_dir, module_path + ".orx"),
        os.path.join(base_dir, "packages", os.path.basename(module_path) + ".orx"),
        os.path.join(base_dir, "lib", os.path.basename(module_path) + ".orx"),
        os.path.join(base_dir, "stdlib", os.path.basename(module_path) + ".orx"),
    ]
    found = None
    for p in candidates:
        if os.path.exists(p):
            found = p
            break
    if not found:
        return  # silencioso — el intérprete lo maneja en runtime

    with open(found, "r", encoding="utf-8") as f:
        source = f.read()

    mod_tokens = lex(source)
    mod_ast = parse(mod_tokens)

    for n in mod_ast:
        if not isinstance(n, tuple):
            continue

        # Constantes de módulo: ASSIGN de nivel top con valores literales
        if n[0] in ("ASSIGN", "CONST"):
            var_name = n[1]
            val_expr = n[2] if len(n) > 2 else None
            # Solo literales simples
            if isinstance(val_expr, (int, float, str, bool)):
                qualified = f"{alias}.{var_name}" if alias else var_name
                if not hasattr(_current_compiler, "module_consts"):
                    _current_compiler.module_consts = {}
                _current_compiler.module_consts[qualified] = val_expr
            continue

        if n[0] != "FN":
            continue
        fn_name, params, body = n[1], n[2], n[3]

        # Filtrar si hay imports selectivos
        if selective and fn_name not in selective:
            continue

        fc = FunctionCompiler()
        fc._root_compiler = _current_compiler
        for stmt in body:
            compile_node_into(fc, stmt)
        fc.emit("LoadNull")
        fc.emit("Return")

        # Nombre calificado: alias.fn_name  o solo fn_name si no hay alias
        qualified = f"{alias}.{fn_name}" if alias else fn_name
        _current_compiler.functions[qualified] = {
            "params": params,
            "body":   fc.instructions,
            "lines":  fc.line_table,
        }

class FunctionCompiler:
    """Compilador dedicado para el cuerpo de una función o act."""
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
        self.functions = {}  # nombre -> {params, body, lines}
        self.shapes    = {}  # nombre -> {fields, on_create, acts, using}

    def emit(self, instr):
        self.instructions.append(instr)
        self.line_table.append(self.current_line)
        return len(self.instructions) - 1

    def patch(self, idx, new_instr):
        self.instructions[idx] = new_instr

    def current_addr(self):
        return len(self.instructions)

    def compile_program(self, ast):
        global _current_compiler
        _current_compiler = self  # permite que _compile_use_module acceda a self.functions

        # Primer pase: compilar funciones, shapes y módulos USE
        for node in ast:
            if not isinstance(node, tuple):
                continue
            if node[0] == "FN":
                fn_name, params, body = node[1], node[2], node[3]
                fc = FunctionCompiler()
                fc._root_compiler = self
                for stmt in body:
                    compile_node_into(fc, stmt)
                fc.emit("LoadNull")
                fc.emit("Return")
                self.functions[fn_name] = {
                    "params": params,
                    "body": fc.instructions,
                    "lines": fc.line_table,
                }
            elif node[0] == "SHAPE_DEF":
                _compile_shape_def(self, node)
            elif node[0] == "USE":
                # Inlining en compile-time: ningún cambio en la VM necesario
                # node puede ser (USE, path, alias, selective) o (USE, path, alias, selective, line)
                module_path = node[1]
                alias       = node[2]
                selective   = node[3]
                _compile_use_module(module_path, alias, selective)

        # Segundo pase: compilar código principal (ignorando FN, SHAPE_DEF y USE)
        for node in ast:
            if isinstance(node, tuple) and node[0] in ("FN", "SHAPE_DEF", "USE"):
                continue
            self.compile_node(node)
        self.emit({"Halt": None})
        return self.instructions

    def compile_node(self, node):
        compile_node_into(self, node)


# ---------------------------------------------------------------------------
# Helpers de compilación de shapes
# ---------------------------------------------------------------------------

def _compile_body(body):
    """Compila una lista de statements en un FunctionCompiler. Retorna fc."""
    fc = FunctionCompiler()
    for stmt in body:
        compile_node_into(fc, stmt)
    fc.emit("LoadNull")
    fc.emit("Return")
    return fc


def _compile_shape_def(compiler, node):
    """Compila un SHAPE_DEF y lo almacena en compiler.shapes."""
    # Strip trailing line number if present
    if len(node) >= 2 and isinstance(node[-1], int) and not isinstance(node[-1], bool):
        node = node[:-1]
    _, shape_name, field_defs, on_create_def, acts_list, using_list = node

    # --- Fields: compilar cada valor por defecto como mini-bytecode ---
    fields = []
    for field_item in field_defs:
        if len(field_item) == 3:
            fname, ftype, fdefault = field_item
        else:
            fname, fdefault = field_item[0], field_item[1]
            ftype = None
        fc = FunctionCompiler()
        compile_expr_into(fc, fdefault)
        fc.emit("Return")
        fields.append({
            "name":    fname,
            "type":    ftype,
            "default": fc.instructions,
        })

    # --- on_create ---
    on_create = None
    if on_create_def:
        params = on_create_def[0]
        body   = on_create_def[1]
        fc = _compile_body(body)
        on_create = {"params": params, "body": fc.instructions, "lines": fc.line_table}

    # --- Acts ---
    acts = {}
    for act_item in acts_list:
        act_name   = act_item[0]
        act_params = act_item[1]
        act_body   = act_item[2]
        fc = _compile_body(act_body)
        acts[act_name] = {"params": act_params, "body": fc.instructions, "lines": fc.line_table}

    compiler.shapes[shape_name] = {
        "fields":    fields,
        "on_create": on_create,
        "acts":      acts,
        "using":     using_list,
    }


# ---------------------------------------------------------------------------
# compile_node_into — compila un nodo statement en cualquier contexto
# ---------------------------------------------------------------------------

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

    elif tag == "TYPED_ASSIGN":
        # nombre: tipo = valor — type hint es metadata, no se valida en runtime
        _, name, _type_hint, expr = node
        compile_expr_into(ctx, expr)
        ctx.emit({"StoreVar": name})

    elif tag == "CONST":
        _, name, expr = node
        compile_expr_into(ctx, expr)
        ctx.emit({"StoreConst": name})

    elif tag == "ATTR_ASSIGN":
        # obj.field = value
        _, obj_expr, attr_name, value_expr = node
        compile_expr_into(ctx, obj_expr)
        compile_expr_into(ctx, value_expr)
        ctx.emit({"SetAttr": attr_name})

    elif tag == "SHAPE_DEF":
        # Ya compilado en primer pase; emitir instrucción para registrarlo en VM
        shape_name = node[1]
        ctx.emit({"DefineShape": shape_name})

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

    elif tag == "FOR_IN":
        # for item in expr { body }  — compila como bucle con índice
        _, var_spec, iterable_expr, body = node
        # Soporte para un solo var (string) o destructuring (lista)
        var_name = var_spec if isinstance(var_spec, str) else var_spec[0]

        # Contadores únicos para variables temporales
        counter = getattr(ctx, "_for_counter", 0)
        ctx._for_counter = counter + 1
        list_var = f"__list_{counter}__"
        len_var  = f"__len_{counter}__"
        idx_var  = f"__idx_{counter}__"

        # Evaluar iterable y almacenar
        compile_expr_into(ctx, iterable_expr)
        ctx.emit({"StoreVar": list_var})
        # Obtener longitud usando builtin len()
        ctx.emit({"LoadVar": list_var})
        ctx.emit({"Call": ["len", 1]})
        ctx.emit({"StoreVar": len_var})
        # Índice = 0
        ctx.emit({"LoadInt": 0})
        ctx.emit({"StoreVar": idx_var})

        loop_start = ctx.current_addr()
        ctx.emit({"LoadVar": idx_var})
        ctx.emit({"LoadVar": len_var})
        ctx.emit("Lt")
        jump_end = ctx.emit({"JumpIfFalse": 0})

        # Obtener elemento actual
        ctx.emit({"LoadVar": list_var})
        ctx.emit({"LoadVar": idx_var})
        ctx.emit("GetIndex")
        ctx.emit({"StoreVar": var_name})

        # Cuerpo del bucle
        for stmt in body:
            compile_node_into(ctx, stmt)

        # Incrementar índice
        ctx.emit({"LoadVar": idx_var})
        ctx.emit({"LoadInt": 1})
        ctx.emit("Add")
        ctx.emit({"StoreVar": idx_var})
        ctx.emit({"Jump": loop_start})
        ctx.patch(jump_end, {"JumpIfFalse": ctx.current_addr()})

    elif tag == "ATTEMPT":
        # attempt { body } handle err { handler }
        try_body = node[1]
        handler_node = node[2] if len(node) > 2 else None

        begin_patch = ctx.emit({"BeginAttempt": 0})  # handler_addr a parchear
        for stmt in try_body:
            compile_node_into(ctx, stmt)
        end_patch = ctx.emit({"EndAttempt": 0})       # end_addr a parchear

        handler_addr = ctx.current_addr()
        ctx.patch(begin_patch, {"BeginAttempt": handler_addr})

        if handler_node:
            # handler_node = ("HANDLE", err_name, body)
            err_name = handler_node[1] if len(handler_node) > 1 else "_error"
            handler_body = handler_node[2] if len(handler_node) > 2 else []
            # El error ya está en el stack — lo almacenamos
            ctx.emit({"StoreVar": err_name})
            for stmt in handler_body:
                compile_node_into(ctx, stmt)
        else:
            ctx.emit("Pop")  # descartar error si no hay handler

        end_addr = ctx.current_addr()
        ctx.patch(end_patch, {"EndAttempt": end_addr})

    elif tag == "EXPR":
        inner = node[1]
        # Artefacto del parser: 'let x = ...' genera ('EXPR', ('IDENT','let'))
        # seguido de ('ASSIGN', ...). Ignorar referencias a keywords sueltas.
        _KEYWORDS = {"let", "const", "fn", "if", "else", "while", "for", "return",
                     "attempt", "handle", "think", "learn", "sense", "use", "shape", "act",
                     "async", "await", "spawn", "serve", "route", "with"}
        if isinstance(inner, tuple) and inner[0] == "IDENT" and inner[1] in _KEYWORDS:
            return
        compile_expr_into(ctx, inner)
        ctx.emit("Pop")

    elif tag == "SERVE_STMT":
        # serve <port> <handler>
        # Instrucción: ServeHTTP(port, fn_name)
        _, port_expr, handler_node = node
        # Extraer el nombre del handler
        if isinstance(handler_node, tuple) and handler_node[0] == "FN":
            fn_name = handler_node[1]
            # Compilar la función inline si aún no está registrada
            if fn_name not in (getattr(ctx, 'functions', None) or {}):
                compile_node_into(ctx, handler_node)
        elif isinstance(handler_node, tuple) and handler_node[0] == "IDENT":
            fn_name = handler_node[1]
        elif isinstance(handler_node, str):
            fn_name = handler_node
        else:
            fn_name = "__serve_handler__"
            compile_node_into(ctx, handler_node)

        # Evaluar el puerto y emitir la instrucción
        compile_expr_into(ctx, port_expr)
        ctx.emit({"ServeHTTP": fn_name})

    elif tag == "USE":
        # Inlining de módulos en compile-time — no emite instrucciones en el cuerpo
        _, module_path, alias, selective = node
        _compile_use_module(module_path, alias, selective)

    elif tag == "FN":
        pass  # compilado en primer pase

    elif tag == "ASYNC_FN":
        pass  # el intérprete maneja async fn; el bytecode VM no lo soporta aún

    elif tag == "AWAIT_STMT":
        # await <expr> — evaluar la expresión (si es Future, ya se resuelve en runtime)
        _, inner_expr = node
        compile_expr_into(ctx, inner_expr)
        ctx.emit("Pop")  # descarta el resultado (es un statement)

    elif tag == "SPAWN":
        # spawn <expr> — lanzar sin esperar
        _, inner_expr = node
        compile_expr_into(ctx, inner_expr)
        ctx.emit("Pop")

    elif tag == "THINK":
        # think <expr>  →  eval expr, call AI, push result, print
        _, expr = node
        compile_expr_into(ctx, expr)
        ctx.emit("AiAsk")
        ctx.emit("Show")

    elif tag == "LEARN":
        # learn <expr>  →  embed en memoria AI, push confirmación, print
        _, expr = node
        compile_expr_into(ctx, expr)
        ctx.emit("AiLearn")
        ctx.emit("Show")

    elif tag == "SENSE":
        # sense <expr>  →  recall de memoria AI, push respuesta, print
        _, expr = node
        compile_expr_into(ctx, expr)
        ctx.emit("AiSense")
        ctx.emit("Show")

    elif tag == "RETURN":
        _, expr = node
        if expr is not None:
            compile_expr_into(ctx, expr)
        else:
            ctx.emit("LoadNull")
        ctx.emit("Return")


# ---------------------------------------------------------------------------
# String interpolation helpers
# ---------------------------------------------------------------------------

def _split_interpolation(s):
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
    parts = _split_interpolation(raw)
    if not parts:
        ctx.emit({"LoadStr": ""})
        return
    kind, content = parts[0]
    if kind == 'text':
        ctx.emit({"LoadStr": content})
    else:
        tokens = lex(content)
        expr, _ = parse_expression(tokens, 0)
        compile_expr_into(ctx, expr)
    for kind, content in parts[1:]:
        if kind == 'text':
            ctx.emit({"LoadStr": content})
        else:
            tokens = lex(content)
            expr, _ = parse_expression(tokens, 0)
            compile_expr_into(ctx, expr)
        ctx.emit("Add")


# ---------------------------------------------------------------------------
# compile_expr_into — compila una expresión en cualquier contexto
# ---------------------------------------------------------------------------

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

    elif tag == "CALL_METHOD":
        # obj.method(args) — formato: (CALL_METHOD, method_name, obj_expr, args, kwargs?)
        method_name = expr[1]
        obj_expr    = expr[2]
        args        = expr[3] if len(expr) > 3 else []

        # Si obj es un IDENT y hay una función qualified "alias.method" registrada
        # → es una llamada a módulo, no a un método de instancia OOP
        if (obj_expr[0] == "IDENT" and _current_compiler is not None):
            qualified = f"{obj_expr[1]}.{method_name}"
            if qualified in _current_compiler.functions:
                for arg in args:
                    compile_expr_into(ctx, arg)
                ctx.emit({"Call": [qualified, len(args)]})
                return

        compile_expr_into(ctx, obj_expr)
        for arg in args:
            compile_expr_into(ctx, arg)
        ctx.emit({"CallMethod": [method_name, len(args)]})

    elif tag == "ATTR_ACCESS":
        _, obj_expr, attr_name = expr
        # Si obj es un IDENT y hay una constante de módulo "alias.attr" registrada
        if (obj_expr[0] == "IDENT" and _current_compiler is not None):
            qualified = f"{obj_expr[1]}.{attr_name}"
            consts = getattr(_current_compiler, "module_consts", {})
            if qualified in consts:
                val = consts[qualified]
                if isinstance(val, bool):
                    ctx.emit({"LoadBool": val})
                elif isinstance(val, int):
                    ctx.emit({"LoadInt": val})
                elif isinstance(val, float):
                    ctx.emit({"LoadFloat": val})
                elif isinstance(val, str):
                    ctx.emit({"LoadStr": val.strip('"')})
                return
        compile_expr_into(ctx, obj_expr)
        ctx.emit({"GetAttr": attr_name})

    elif tag == "IS_CHECK":
        _, obj_expr, shape_name = expr
        compile_expr_into(ctx, obj_expr)
        ctx.emit({"IsInstance": shape_name})

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


# ---------------------------------------------------------------------------
# API pública
# ---------------------------------------------------------------------------

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
        "main":      compiler.instructions,
        "lines":     compiler.line_table,
        "functions": compiler.functions,
        "shapes":    compiler.shapes,
    }

    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(bytecode, f, indent=2)

    fn_instrs  = sum(len(f["body"]) for f in compiler.functions.values())
    act_instrs = sum(
        len(a["body"])
        for s in compiler.shapes.values()
        for a in s["acts"].values()
    )
    oc_instrs = sum(
        len(s["on_create"]["body"]) if s["on_create"] else 0
        for s in compiler.shapes.values()
    )
    total = len(compiler.instructions) + fn_instrs + act_instrs + oc_instrs

    print(f"Compilado: {source_path} -> {output_path}")
    print(
        f"Main: {len(compiler.instructions)} instrs | "
        f"Funciones: {len(compiler.functions)} | "
        f"Shapes: {len(compiler.shapes)} | "
        f"Total: {total}"
    )
    return output_path


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Uso: python bytecode_compiler.py archivo.orx [salida.orbc]")
        sys.exit(1)
    src = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else None
    compile_file(src, out)
