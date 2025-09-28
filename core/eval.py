from core.control import eval_match
from core.functions import register_function, get_function, register_native_function
from core.types import OrionString, null_safe
from core.errors import (
    OrionRuntimeError,
    OrionTypeError,
    OrionNameError,
    OrionFunctionError,
)

from lib import collections
from lib import io
from lib import math as orion_math
from lib import strings

def lookup_variable(name, variables):
    """Busca una variable en el scope actual."""
    if name in variables:
        return variables[name]
    # Usa el error personalizado de Orion
    raise OrionNameError(name)

def _register_builtin_functions(functions):
    import types
    builtin_modules = [collections, io, orion_math, strings]
    for mod in builtin_modules:
        for k in dir(mod):
            if not k.startswith("_"):
                v = getattr(mod, k)
                if isinstance(v, types.FunctionType):
                    register_native_function(functions, k, v)

def eval_expr(expr, variables, functions):
    if isinstance(expr, tuple):
        tag = expr[0]

        if tag == "BINARY_OP":
            _, op, left, right = expr
            left_val = eval_expr(left, variables, functions)
            right_val = eval_expr(right, variables, functions)

            if op == "+":
                if isinstance(left_val, str) or isinstance(right_val, str):
                    return str(left_val) + str(right_val)
                return left_val + right_val
            elif op == "-":
                return left_val - right_val
            elif op == "*":
                return left_val * right_val
            elif op == "/":
                return left_val / right_val
            elif op == ">":
                return left_val > right_val
            elif op == "<":
                return left_val < right_val
            elif op == "==":
                return left_val == right_val
            elif op == "!=":
                return left_val != right_val
            elif op == "<=":
                return left_val <= right_val
            elif op == ">=":
                return left_val >= right_val
            elif op == "&&":
                return bool(left_val) and bool(right_val)
            elif op == "||":
                return bool(left_val) or bool(right_val)
            else:
                # Error personalizado para operador desconocido
                raise OrionRuntimeError(f"Operador binario desconocido: {op}")

        elif tag == "UNARY_OP":
            _, op, operand = expr
            if op == "!":
                return not eval_expr(operand, variables, functions)
            else:
                raise OrionRuntimeError(f"Operador unario desconocido: {op}")

        elif tag == "CALL":
            _, fn_name, args = expr
            fn_def = get_function(functions, fn_name)
            if not fn_def:
                raise OrionFunctionError(f"Función no definida: {fn_name}")

            _, params, body = fn_def
            if len(args) != len(params):
                raise OrionFunctionError(f"Argumentos esperados: {len(params)}, recibidos: {len(args)}")

            local_vars = variables.copy()
            for p, a in zip(params, args):
                local_vars[p] = eval_expr(a, local_vars, functions)

            if callable(body):
                arg_vals = [local_vars[p] for p in params]
                return body(*arg_vals)
            else:
                return evaluate(body, local_vars, functions, inside_fn=True)

        elif tag == "RETURN":
            _, value = expr
            return eval_expr(value, variables, functions) if value is not None else None

        elif tag == "MATCH":
            _, expr_val, cases = expr
            val = eval_expr(expr_val, variables, functions)
            result = eval_match(val, cases, evaluate, variables)
            return result

        elif tag == "NULL_SAFE":
            _, obj, attr = expr
            obj_val = eval_expr(obj, variables, functions)
            return null_safe(obj_val, attr)

        elif tag == "ATTR_ACCESS":
            _, obj_expr, attr_name = expr
            obj_val = eval_expr(obj_expr, variables, functions)
            # Acceso a atributo simple
            if hasattr(obj_val, attr_name):
                return getattr(obj_val, attr_name)
            elif isinstance(obj_val, dict) and attr_name in obj_val:
                return obj_val[attr_name]
            else:
                raise OrionRuntimeError(f"Atributo '{attr_name}' no encontrado en objeto.")

        elif tag == "CALL_METHOD":
            _, method_name, obj_expr, args = expr
            obj_val = eval_expr(obj_expr, variables, functions)
            arg_vals = [eval_expr(a, variables, functions) for a in args]
            # Si el método existe en lib.math, lo llama como math.method(obj, *args)
            if hasattr(orion_math, method_name):
                fn = getattr(orion_math, method_name)
                return fn(obj_val, *arg_vals)
            # Si el método existe en el objeto, lo llama como obj.method(*args)
            method = getattr(obj_val, method_name, None)
            if callable(method):
                return method(*arg_vals)
            raise OrionFunctionError(f"Método '{method_name}' no definido en lib.math ni en el objeto.")

    if isinstance(expr, bool):
        return expr

    if isinstance(expr, (int, float)):
        return expr

    if isinstance(expr, str):
        if expr == "true":
            return True
        if expr == "false":
            return False
        if expr.isdigit():
            return int(expr)
        if expr.startswith('"') and expr.endswith('"'):
            return OrionString(expr.strip('"'))
        if expr in variables:
            return lookup_variable(expr, variables)
        return expr

    return expr

def evaluate(ast, variables=None, functions=None, inside_fn=False):
    if variables is None:
        variables = {}
    if functions is None:
        functions = {}

    _register_builtin_functions(functions)

    for node in ast:
        if node[0] == "FN":
            _, fn_name, params, body = node
            register_function(functions, fn_name, params, body)

    i = 0
    while i < len(ast):
        node = ast[i]
        tag = node[0]

        if tag == "FN":
            pass

        elif tag == "ASSIGN":
            _, name, value = node
            val = eval_expr(value, variables, functions)
            if isinstance(val, str) and not isinstance(val, OrionString):
                val = OrionString(val)
            variables[name] = val

        elif tag == "DECLARE":
            _, type_name, var_name, expr_value = node
            if expr_value is not None:
                value = eval_expr(expr_value, variables, functions)
                if type_name in ("auto", None):
                    variables[var_name] = value
                else:
                    variables[var_name] = value
            else:
                if type_name == "int":
                    variables[var_name] = 0
                elif type_name == "float":
                    variables[var_name] = 0.0
                elif type_name == "bool":
                    variables[var_name] = False
                elif type_name == "string":
                    variables[var_name] = ""
                else:
                    variables[var_name] = None

        elif tag == "PRINT":
            _, value = node
            val = eval_expr(value, variables, functions)
            if isinstance(val, OrionString):
                val = val.interpolate(variables)
            print(val)

        elif tag == "FOR":
            _, var_name, start, end, body, range_type = node

            start_val = eval_expr(start, variables, functions)
            end_val = eval_expr(end, variables, functions)

            if not isinstance(start_val, (int, float)):
                raise OrionTypeError(f"El rango debe ser numérico, se recibió start={start_val}")
            if not isinstance(end_val, (int, float)):
                raise OrionTypeError(f"El rango debe ser numérico, se recibió end={end_val}")

            if range_type == "RANGE":
                rng = range(start_val, end_val + 1)
            elif range_type == "RANGE_EX":
                rng = range(start_val, end_val)
            else:
                raise OrionRuntimeError(f"Tipo de rango no soportado: {range_type}")

            for j in rng:
                variables[var_name] = j
                result = evaluate(body, variables, functions, inside_fn=True)
                if inside_fn and result is not None:
                    return result

        elif tag == "IF":
            _, condition, body_true, body_false = node
            if eval_expr(condition, variables, functions):
                result = evaluate(body_true, variables, functions, inside_fn=True)
            else:
                result = evaluate(body_false, variables, functions, inside_fn=True)
            if inside_fn and result is not None:
                return result

        elif tag == "CALL":
            result = eval_expr(node, variables, functions)
            if inside_fn:
                return result

        elif tag == "RETURN":
            _, value = node
            return eval_expr(value, variables, functions) if value is not None else None

        elif tag == "MATCH":
            result = eval_expr(node, variables, functions)
            if inside_fn and result is not None:
                return result

        else:
            raise OrionRuntimeError(f"Nodo desconocido en AST: {tag}")

        i += 1

    # 3. Si estamos en nivel superior
    if not inside_fn:
        if "main" in functions:
            _, params, body = functions["main"]
            return evaluate(body, variables, functions, inside_fn=True)
        # Si no hay main, simplemente terminamos
        return None
