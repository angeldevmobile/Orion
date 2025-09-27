from core.control import eval_match
from core.functions import register_function, get_function
from core.types import OrionString, null_safe


def lookup_variable(name, variables):
    """Busca una variable en el scope actual."""
    if name in variables:
        return variables[name]
    # Si no está, lanza error (no devuelve el string)
    raise NameError(f"Variable no definida: {name}")


def eval_expr(expr, variables):
    """Evalúa una expresión del AST."""

    if isinstance(expr, tuple):
        tag = expr[0]

        # Operadores binarios
        if tag == "BINARY_OP":
            _, op, left, right = expr
            left_val = eval_expr(left, variables)
            right_val = eval_expr(right, variables)

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
                raise ValueError(f"Operador binario desconocido: {op}")

        # Operadores unarios
        elif tag == "UNARY_OP":
            _, op, operand = expr
            if op == "!":
                return not eval_expr(operand, variables)
            else:
                raise ValueError(f"Operador unario desconocido: {op}")

        # Llamadas a funciones
        elif tag == "CALL":
            _, fn_name, args = expr
            fn_def = get_function(variables, fn_name)
            if not fn_def:
                raise NameError(f"Función no definida: {fn_name}")

            _, params, body = fn_def
            if len(args) != len(params):
                raise TypeError(f"Argumentos esperados: {len(params)}, recibidos: {len(args)}")

            # Nuevo scope local: copia TODO el scope actual
            local_vars = variables.copy()
            for p, a in zip(params, args):
                local_vars[p] = eval_expr(a, local_vars)  # evalúa en el scope local actualizado

            return evaluate(body, local_vars, inside_fn=True)

        # Return explícito dentro de una función
        elif tag == "RETURN":
            _, value = expr
            return eval_expr(value, variables) if value is not None else None

        elif tag == "MATCH":
            _, expr_val, cases = expr
            val = eval_expr(expr_val, variables)
            result = eval_match(val, cases, evaluate, variables)
            return result

        elif tag == "NULL_SAFE":
            _, obj, attr = expr
            obj_val = eval_expr(obj, variables)
            return null_safe(obj_val, attr)

    # Tipos primitivos
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


def evaluate(ast, variables=None, inside_fn=False):
    """Ejecuta el AST completo."""
    if variables is None:
        variables = {}

    # 1. Registrar funciones primero
    for node in ast:
        if node[0] == "FN":
            _, fn_name, params, body = node
            register_function(variables, fn_name, params, body)

    # 2. Ejecutar el resto
    i = 0
    while i < len(ast):
        node = ast[i]
        tag = node[0]

        if tag == "FN":
            pass  # ya registrado

        elif tag == "ASSIGN":
            _, name, value = node
            val = eval_expr(value, variables)
            # Si es string plano, conviértelo a OrionString
            if isinstance(val, str) and not isinstance(val, OrionString):
                val = OrionString(val)
            variables[name] = val

        elif tag == "DECLARE":
            _, type_name, var_name, expr_value = node
            if expr_value is not None:
                value = eval_expr(expr_value, variables)
                # Si es inferido (type_name == "auto" o None)
                if type_name in ("auto", None):
                    variables[var_name] = value
                else:
                    variables[var_name] = value
            else:
                # inicialización por defecto según tipo
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
            val = eval_expr(value, variables)
            if isinstance(val, OrionString):
                val = val.interpolate(variables)
            print(val)

        elif tag == "FOR":
            _, var_name, start, end, body, range_type = node

            # Evalúa los límites usando el scope actual
            start_val = eval_expr(start, variables)
            end_val = eval_expr(end, variables)

            if not isinstance(start_val, (int, float)):
                raise TypeError(f"El rango debe ser numérico, se recibió start={start_val}")
            if not isinstance(end_val, (int, float)):
                raise TypeError(f"El rango debe ser numérico, se recibió end={end_val}")

            if range_type == "RANGE":
                rng = range(start_val, end_val + 1)
            elif range_type == "RANGE_EX":
                rng = range(start_val, end_val)
            else:
                raise ValueError(f"Tipo de rango no soportado: {range_type}")

            for j in rng:
                # asignamos directamente sobre el scope actual
                variables[var_name] = j
                result = evaluate(body, variables, inside_fn=True)
                if inside_fn and result is not None:
                    return result

        elif tag == "IF":
            _, condition, body_true, body_false = node
            if eval_expr(condition, variables):
                result = evaluate(body_true, variables, inside_fn=True)
            else:
                result = evaluate(body_false, variables, inside_fn=True)
            if inside_fn and result is not None:
                return result

        elif tag == "CALL":
            result = eval_expr(node, variables)
            if inside_fn:
                return result

        elif tag == "RETURN":
            _, value = node
            return eval_expr(value, variables) if value is not None else None

        elif tag == "MATCH":
            result = eval_expr(node, variables)
            if inside_fn and result is not None:
                return result

        else:
            raise ValueError(f"Nodo desconocido en AST: {tag}")

        i += 1

    # 3. Si estamos en nivel superior
    if not inside_fn:
        if "main" in variables:
            _, params, body = variables["main"]
            return evaluate(body, variables, inside_fn=True)
        # Si no hay main, simplemente terminamos
        return None

