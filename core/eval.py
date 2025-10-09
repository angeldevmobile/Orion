import importlib.util
import sys
import os
import types

sys.path.append(os.path.join(os.path.dirname(__file__), ".."))
from core.control import eval_match
from core.functions import register_function, get_function, register_native_function
from core.types import (
    OrionString,
    OrionNumber,
    OrionBool,
    OrionDate,
    OrionList,
    null_safe,
)
from core.errors import (
    OrionRuntimeError,
    OrionTypeError,
    OrionNameError,
    OrionFunctionError,
)

from modules import json as orion_json
from lib import collections
from modules import code, show
from lib import io
from lib import math as orion_math
from lib import strings

NATIVE_FUNCTIONS = {
    "trace_start": code.trace_start,
    "trace_end": code.trace_end,
    "progress": code.progress,
    "divider": code.divider,
    "frame": code.frame,
    "show": show.show,
}
def lookup_variable(name, variables):
    """Busca una variable en el scope actual."""
    if name in variables:
        return variables[name]
    # Usa el error personalizado de Orion
    raise OrionNameError(name)

def _register_builtin_functions(functions):
    import types
    from lib import io 
    
    builtin_modules = [collections, io, orion_math, strings]
    for mod in builtin_modules:
        for k in dir(mod):
            if not k.startswith("_"):
                v = getattr(mod, k)
                if isinstance(v, types.FunctionType):
                    register_native_function(functions, k, v)
    # Registrar funciones nativas de Python necesarias
    register_native_function(functions, "len", len)

def eval_call_args(args, variables, functions):
    pos_args = []
    kw_args = {}
    for arg in args:
        if isinstance(arg, tuple) and arg[0] == "NAMED_ARG":
            name = arg[1]
            value = eval_expr(arg[2], variables, functions)
            kw_args[name] = to_native(value)
        else:
            pos_args.append(to_native(eval_expr(arg, variables, functions)))
    return pos_args, kw_args

def eval_expr(expr, variables, functions):
    # 1. Caso nulo
    if expr is None:
        return None

    # 2. Caso: referencia a variable ('IDENT', 'nombre_variable')
    if isinstance(expr, tuple) and len(expr) == 2 and expr[0] == "IDENT":
        _, name = expr
        if name in variables:
            val = variables[name]
            if hasattr(val, "value"):
                val = val.value

            # Conversión automática de strings numéricos
            if isinstance(val, str):
                if val.isdigit():
                    return int(val)
                try:
                    return float(val)
                except ValueError:
                    pass
            return val
        else:
            raise OrionRuntimeError(f"Variable '{name}' no definida")

    # 3. Caso: la expresión es una cadena y coincide con una variable existente
    if isinstance(expr, str) and expr in variables:
        val = variables[expr]
        if hasattr(val, "value"):
            return val.value
        return val

    # 4. Procesamiento de expresiones tipo tupla (AST)
    if isinstance(expr, tuple):
        tag = expr[0]

        # --- INDEX ---
        if tag == "INDEX":
            _, list_expr, index_expr = expr
            list_val = eval_expr(list_expr, variables, functions)
            index_val = eval_expr(index_expr, variables, functions)

            # Si es OrionList (o similar con .items)
            if isinstance(list_val, OrionList) or (hasattr(list_val, "items") and isinstance(getattr(list_val, "items"), (list, tuple))):
                container = getattr(list_val, "items", list_val)
                try:
                    return container[int(index_val)]
                except (ValueError, IndexError):
                    raise OrionRuntimeError(f"Índice fuera de rango o inválido: {index_val}")

            # Si es un wrapper que guarda value con lista interna (p.ej. OrionNumber/OrionString/otros)
            if hasattr(list_val, "value") and isinstance(getattr(list_val, "value"), (list, tuple, str)):
                inner = getattr(list_val, "value")
                try:
                    return inner[int(index_val)]
                except (ValueError, IndexError, TypeError):
                    raise OrionRuntimeError(f"Índice fuera de rango o inválido: {index_val}")

            # Si es lista o tupla nativa
            if isinstance(list_val, (list, tuple)):
                try:
                    return list_val[int(index_val)]
                except (ValueError, IndexError):
                    raise OrionRuntimeError(f"Índice fuera de rango o inválido: {index_val}")

            # Si es diccionario
            if isinstance(list_val, dict):
                return list_val.get(index_val, None)

            # No indexable
            raise OrionRuntimeError(f"No se puede indexar el tipo {type(list_val).__name__}")

        # --- LIST ---
        elif tag == "LIST":
            _, elements = expr
            return [eval_expr(e, variables, functions) for e in elements]
        
        # --- DICT ---
        elif tag == "DICT":
            _, items = expr
            return {k: eval_expr(v, variables, functions) for k, v in items}

        # --- IDENT ---
        elif tag == "IDENT":
            _, name = expr
            if name in variables:
                val = variables[name]
                if hasattr(val, "value"):
                    return val.value
                return val
            else:
                raise OrionRuntimeError(f"Variable '{name}' no definida")

        # --- BINARY_OP ---
        elif tag == "BINARY_OP":
            _, op, left, right = expr
            left_val = eval_expr(left, variables, functions)
            right_val = eval_expr(right, variables, functions)

            if hasattr(left_val, "value"):
                left_val = left_val.value
            if hasattr(right_val, "value"):
                right_val = right_val.value

            # Intentar convertir strings numéricos a enteros o flotantes
            def try_cast_numeric(v):
                if isinstance(v, str):
                    try:
                        if "." in v:
                            return float(v)
                        return int(v)
                    except ValueError:
                        return v
                return v

            left_val = try_cast_numeric(left_val)
            right_val = try_cast_numeric(right_val)

            # --- OPERADORES BINARIOS ---
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
            elif op in [">", "<", ">=", "<=", "==", "!="]:
                # Intentar normalizar tipos antes de comparar
                left_val = try_cast_numeric(left_val)
                right_val = try_cast_numeric(right_val)

                # Comparaciones numéricas
                if isinstance(left_val, (int, float)) and isinstance(right_val, (int, float)):
                    if op == ">": return left_val > right_val
                    if op == "<": return left_val < right_val
                    if op == ">=": return left_val >= right_val
                    if op == "<=": return left_val <= right_val
                    if op == "==": return left_val == right_val
                    if op == "!=": return left_val != right_val

                # Comparaciones de texto
                if isinstance(left_val, str) and isinstance(right_val, str):
                    if op == ">": return left_val > right_val
                    if op == "<": return left_val < right_val
                    if op == ">=": return left_val >= right_val
                    if op == "<=": return left_val <= right_val
                    if op == "==": return left_val == right_val
                    if op == "!=": return left_val != right_val

                # Si uno es string numérico y otro número
                if isinstance(left_val, str) and left_val.replace('.', '', 1).isdigit():
                    left_val = float(left_val) if '.' in left_val else int(left_val)
                    return eval_expr(("BINARY_OP", op, left_val, right_val), variables, functions)
                if isinstance(right_val, str) and right_val.replace('.', '', 1).isdigit():
                    right_val = float(right_val) if '.' in right_val else int(right_val)
                    return eval_expr(("BINARY_OP", op, left_val, right_val), variables, functions)

                raise OrionRuntimeError(
                    f"No se puede comparar {type(left_val).__name__} con {type(right_val).__name__}"
                )

            elif op == "&&":
                return bool(left_val) and bool(right_val)
            elif op == "||":
                return bool(left_val) or bool(right_val)
            else:
                raise OrionRuntimeError(f"Operador binario desconocido: {op}")

        # --- UNARY_OP ---
        elif tag == "UNARY_OP":
            _, op, operand = expr
            if op == "!":
                return not eval_expr(operand, variables, functions)
            else:
                raise OrionRuntimeError(f"Operador unario desconocido: {op}")

        # --- CALL ---
        elif tag == "CALL":
            _, fn_name, args = expr

            fn_def = get_function(functions, fn_name)
            if fn_def is None and fn_name in NATIVE_FUNCTIONS:
                fn_def = {
                    "type": "NATIVE_FN",
                    "impl": NATIVE_FUNCTIONS[fn_name]
                }
            if not fn_def:
                raise OrionFunctionError(f"Función no definida: {fn_name}")

            pos_args, kw_args = eval_call_args(args, variables, functions)

            # Función nativa
            if fn_def["type"] == "NATIVE_FN":
                # Procesar argumentos especialmente para show
                if fn_name == "show":
                    processed_args = []
                    for i, arg in enumerate(args):
                        if isinstance(arg, str) and arg.startswith('"') and arg.endswith('"'):
                            raw = arg[1:-1]  # Quitar comillas
                            orion_str = OrionString(raw)
                            interpolated = orion_str.interpolate(variables)
                            processed_args.append(str(interpolated))
                        else:
                            processed_args.append(pos_args[i])
                    return fn_def["impl"](*processed_args, **kw_args)
                else:
                    pos_args = [str(a) if isinstance(a, OrionString) else a for a in pos_args]
                    return fn_def["impl"](*pos_args, **kw_args)

            # Función definida por usuario
            params = fn_def.get("params", [])
            body = fn_def.get("body", [])
            if len(pos_args) != len(params):
                raise OrionFunctionError(
                    f"Argumentos esperados: {len(params)}, recibidos: {len(pos_args)}"
                )
            local_vars = variables.copy()
            for p, a in zip(params, pos_args):
                local_vars[p] = a
            local_vars.update(kw_args)
            if callable(body):
                arg_vals = [local_vars[p] for p in params]
                return body(*arg_vals)
            else:
                return evaluate(body, local_vars, functions, inside_fn=True)

        # --- CALL_METHOD ---
        elif tag == "CALL_METHOD":
            _, method_name, obj_expr, args = expr
            obj_val = eval_expr(obj_expr, variables, functions)
            pos_args, kw_args = eval_call_args(args, variables, functions)
            if hasattr(orion_math, method_name):
                fn = getattr(orion_math, method_name)
                return fn(obj_val, *pos_args, **kw_args)
            method = getattr(obj_val, method_name, None)
            if callable(method):
                return method(*pos_args, **kw_args)
            raise OrionFunctionError(
                f"Método '{method_name}' no definido en lib.math ni en el objeto."
            )

        # --- ATTR_ACCESS ---
        elif tag == "ATTR_ACCESS":
            _, obj_expr, attr_name = expr
            obj_val = eval_expr(obj_expr, variables, functions)
            # Si es una instancia de clase, accede al atributo directamente
            if hasattr(obj_val, attr_name):
                val = getattr(obj_val, attr_name)
                # Si el valor es una propiedad o método, evalúalo si es necesario
                if callable(val):
                    return val()
                return val
            # Si es un diccionario
            elif isinstance(obj_val, dict) and attr_name in obj_val:
                return obj_val[attr_name]
            else:
                raise OrionRuntimeError(f"Atributo '{attr_name}' no encontrado en objeto.")

        # --- NULL_SAFE ---
        elif tag == "NULL_SAFE":
            # NULL_SAFE: ('NULL_SAFE', object_expr, attr_name)
            _, object_expr, attr_name = expr
            obj = eval_expr(object_expr, variables, functions)
            
            # Si el objeto es null/None, devolver null
            if obj is None:
                return None
            
            # Si el objeto tiene el atributo, devolverlo
            if hasattr(obj, attr_name):
                return getattr(obj, attr_name)
            elif isinstance(obj, dict) and attr_name in obj:
                return obj[attr_name]
            else:
                return None

    # 5. Tipos básicos
    if isinstance(expr, bool):
        return expr

    if isinstance(expr, (int, float)):
        return expr

    if isinstance(expr, str):
        if expr.startswith('"') and expr.endswith('"'):
            content = expr[1:-1]  # Quitar comillas
            # Interpolación de variables ${variable}
            import re
            def replace_var(match):
                var_name = match.group(1)
                if var_name in variables:
                    val = variables[var_name]
                    if hasattr(val, 'value'):
                        return str(val.value)
                    return str(val)
                return match.group(0)  # Si no existe la variable, dejar como está
            
            interpolated = re.sub(r'\$\{(\w+)\}', replace_var, content)
            return interpolated
        if expr in ("true", "yes"):
            return True
        if expr in ("false", "no"):
            return False
        if expr.isdigit():
            return int(expr)
        if expr.replace('.', '', 1).isdigit():
            return float(expr)
        if expr.startswith('"') and expr.endswith('"'):
            raw = expr.strip('"')
            # Crear OrionString, interpolar y retornar el valor interpolado
            orion_str = OrionString(raw)
            interpolated = orion_str.interpolate(variables)
            return str(interpolated)  # Convertir a string nativo
        if expr in variables:
            return lookup_variable(expr, variables)
        return expr

    # 6. Si no coincide con nada
    return expr

def evaluate(ast, variables=None, functions=None, inside_fn=False):
    if variables is None:
        variables = {}
    if functions is None:
        functions = {}

    # === Inicializar valores nativos de Orion ===
    if "null" not in variables:
        variables["null"] = None
    if "yes" not in variables:
        variables["yes"] = OrionBool(True)
    if "no" not in variables:
        variables["no"] = OrionBool(False)

    _register_builtin_functions(functions)
    functions["_variables"] = variables

    # Registrar funciones FN antes de ejecutar el resto
    for node in ast:
        if node[0] == "FN":
            _, fn_name, params, body = node
            register_function(functions, fn_name, params, body)

    i = 0
    while i < len(ast):
        node = ast[i]
        tag = node[0]

                # --- Soporte para USE con o sin comillas ---
        if tag == "USE":
            _, module_path = node
            if module_path.startswith('"') and module_path.endswith('"'):
                base_name = module_path[1:-1]
            else:
                base_name = module_path

            print(f"[DEBUG USE] Cargando módulo: {base_name}")

            # --- Orion stdlib ---
            if base_name == "json":
                variables["json"] = orion_json
                print(f"[DEBUG] Módulo Orion stdlib '{base_name}' importado")
                i += 1
                continue

            #  Rutas posibles
            orion_file = os.path.join(os.getcwd(), base_name + ".orion")
            py_file = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "modules", base_name + ".py"))

            #  Si existe un módulo Orion local
            if os.path.exists(orion_file):
                from core.lexer import lex
                from core.parser import parse
                with open(orion_file, "r", encoding="utf-8") as f:
                    code = f.read()
                imported_tokens = lex(code)
                imported_ast = parse(imported_tokens)
                evaluate(imported_ast, variables, functions)
                print(f"[DEBUG] Módulo Orion '{base_name}' ejecutado")

            # Si existe un módulo Python en /modules/
            elif os.path.exists(py_file):
                import sys
                sys.path.append(os.path.join(os.path.dirname(__file__), ".."))
                from modules import load_module
                mod_exports = load_module(variables, base_name)
                print(f"[DEBUG] Módulo Python '{base_name}' cargado con {len(mod_exports)} funciones")

                for fname, fref in mod_exports.items():
                    if callable(fref):
                        register_native_function(functions, fname, fref)

            else:
                raise OrionRuntimeError(f"No se encontró el módulo: {orion_file} ni {py_file}")

            i += 1
            continue

        elif tag == "FN":
            # Ya registradas antes del bucle
            i += 1
            continue

        elif tag == "ASSIGN":
            _, name, value = node
            val = eval_expr(value, variables, functions)
            # Envolver valores en tipos Orion si corresponde
            if isinstance(val, str) and not isinstance(val, OrionString):
                val = OrionString(val)
            elif isinstance(val, bool) and not isinstance(val, OrionBool):
                val = OrionBool(val)
            elif isinstance(val, int) and not isinstance(val, OrionNumber):
                val = OrionNumber(val)
            elif isinstance(val, float) and not isinstance(val, OrionNumber):
                val = OrionNumber(val)
            elif isinstance(val, list) and not isinstance(val, OrionList):
                val = OrionList(val)
            variables[name] = val

        elif tag == "DECLARE":
            _, type_name, var_name, expr_value = node
            if expr_value is not None:
                value = eval_expr(expr_value, variables, functions)
                # Envolver según el tipo declarado
                if type_name == "string" and not isinstance(value, OrionString):
                    value = OrionString(value)
                elif type_name == "int" and not isinstance(value, OrionNumber):
                    value = OrionNumber(value)
                elif type_name == "float" and not isinstance(value, OrionNumber):
                    value = OrionNumber(value)
                elif type_name == "bool" and not isinstance(value, OrionBool):
                    value = OrionBool(value)
                elif type_name == "list" and not isinstance(value, OrionList):
                    value = OrionList(value)
                elif type_name == "date" and not isinstance(value, OrionDate):
                    # Espera una tupla (año, mes, día) o un string "YYYY-MM-DD"
                    if isinstance(value, (tuple, list)) and len(value) == 3:
                        value = OrionDate(*value)
                    elif isinstance(value, str):
                        y, m, d = map(int, value.split("-"))
                        value = OrionDate(y, m, d)
                variables[var_name] = value
            else:
                if type_name == "int":
                    variables[var_name] = OrionNumber(0)
                elif type_name == "float":
                    variables[var_name] = OrionNumber(0.0)
                elif type_name == "bool":
                    variables[var_name] = OrionBool(False)
                elif type_name == "string":
                    variables[var_name] = OrionString("")
                elif type_name == "list":
                    variables[var_name] = OrionList([])
                elif type_name == "date":
                    variables[var_name] = OrionDate(1970, 1, 1)
                else:
                    variables[var_name] = None

        elif tag == "PRINT":
            _, value = node
            val = eval_expr(value, variables, functions)
            show.show(val)

        elif tag == "FOR":
            _, var_name, start, end, body, range_type = node

            start_val = eval_expr(start, variables, functions)
            end_val = eval_expr(end, variables, functions)

            if not isinstance(start_val, (int, float)):
                raise OrionTypeError(f"El rango debe ser numérico, se recibió start={start_val}")
            if not isinstance(end_val, (int, float)):
                raise OrionTypeError(f"El rango debe ser numérico, se recibió end={end_val}")

            if range_type == "RANGE":
                rng = range(int(start_val), int(end_val) + 1)
            elif range_type == "RANGE_EX":
                rng = range(int(start_val), int(end_val))
            else:
                raise OrionRuntimeError(f"Tipo de rango no soportado: {range_type}")

            # Guardar valor previo (por si la variable ya existía antes)
            prev_value = variables.get(var_name)

            for j in rng:
                # Registrar variable del bucle en el mismo scope
                variables[var_name] = j

                # Ejecutar cuerpo del bucle
                result = evaluate(body, variables, functions, inside_fn=True)

                # Si el cuerpo devuelve algo (por ejemplo, un return), propágalo
                if inside_fn and result is not None:
                    return result

            # Limpieza del scope (restaurar o eliminar variable)
            if prev_value is not None:
                variables[var_name] = prev_value
            elif var_name in variables:
                del variables[var_name]

        elif tag == "FOR_IN":
            # FOR_IN: ('FOR_IN', var_name, collection_expr, body)
            var_name, collection_expr, body = node[1], node[2], node[3]
            collection = eval_expr(collection_expr, variables, functions)
            # Convierte OrionList a lista nativa si es necesario
            if hasattr(collection, "items"):
                collection = collection.items
            elif hasattr(collection, "value") and isinstance(collection.value, list):
                collection = collection.value
            prev_value = variables.get(var_name)
            for item in collection:
                variables[var_name] = item
                result = evaluate(body, variables, functions, inside_fn=True)
                if inside_fn and result is not None:
                    return result
            # Limpieza del scope
            if prev_value is not None:
                variables[var_name] = prev_value
            elif var_name in variables:
                del variables[var_name]
        
        elif tag == "FOR_RANGE":
            # FOR_RANGE: ('FOR_RANGE', var_name, start, end, body, range_type)
            _, var_name, start, end, body, range_type = node

            start_val = eval_expr(start, variables, functions)
            end_val = eval_expr(end, variables, functions)

            if not isinstance(start_val, (int, float)):
                raise OrionTypeError(f"El rango debe ser numérico, se recibió start={start_val}")
            if not isinstance(end_val, (int, float)):
                raise OrionTypeError(f"El rango debe ser numérico, se recibió end={end_val}")

            if range_type == "RANGE":
                rng = range(int(start_val), int(end_val) + 1)
            elif range_type == "RANGE_EX":
                rng = range(int(start_val), int(end_val))
            else:
                raise OrionRuntimeError(f"Tipo de rango no soportado: {range_type}")

            # Guardar valor previo (por si la variable ya existía antes)
            prev_value = variables.get(var_name)

            for j in rng:
                # Registrar variable del bucle en el mismo scope
                variables[var_name] = j

                # Ejecutar cuerpo del bucle
                result = evaluate(body, variables, functions, inside_fn=True)

                # Si el cuerpo devuelve algo (por ejemplo, un return), propágalo
                if inside_fn and result is not None:
                    return result

            # Limpieza del scope (restaurar o eliminar variable)
            if prev_value is not None:
                variables[var_name] = prev_value
            elif var_name in variables:
                del variables[var_name]

        elif tag == "IF":
            _, condition, body_true, body_false = node
            if eval_expr(condition, variables, functions):
                result = evaluate(body_true, variables, functions, inside_fn=True)
            else:
                result = evaluate(body_false, variables, functions, inside_fn=True)
            if inside_fn and result is not None:
                return result

        elif tag == "CALL":
            eval_expr(node, variables, functions)

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
            main_def = functions["main"][0]  # Toma la primera definición
            params = main_def.get("params", [])
            body = main_def.get("body", [])
            return evaluate(body, variables, functions, inside_fn=True)
        return None
    
def to_native(val):
    from core.types import OrionList, OrionString, OrionNumber, OrionBool, OrionDate, OrionDict
    if isinstance(val, OrionList):
        return [to_native(v) for v in val.items]
    if isinstance(val, OrionDict):
        return {k: to_native(v) for k, v in val.value.items()}
    if isinstance(val, OrionString):
        return str(val.value)
    if isinstance(val, OrionNumber):
        return val.value
    if isinstance(val, OrionBool):
        return bool(val.value)
    if isinstance(val, OrionDate):
        return str(val)
    if isinstance(val, list):
        return [to_native(v) for v in val]
    if isinstance(val, dict):
        return {k: to_native(v) for k, v in val.items()}
    return val