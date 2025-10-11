from core.errors import OrionSyntaxError


def parse_call_args(tokens, i):
    # tokens[i] debe ser LPAREN
    if tokens[i][0] != "LPAREN":
        raise OrionSyntaxError("Se esperaba '(' en llamada")
    i += 1
    args = []
    first = True
    while i < len(tokens) and tokens[i][0] != "RPAREN":
        if not first:
            if tokens[i][0] == "COMMA":
                i += 1
            else:
                raise OrionSyntaxError("Se esperaba ',' entre argumentos")
        # Soporte para argumentos nombrados: nombre=valor
        if (i+1 < len(tokens)) and tokens[i][0] == "IDENT" and tokens[i+1][0] == "ASSIGN":
            arg_name = tokens[i][1]
            i += 2
            arg_value, i = parse_expression(tokens, i)
            args.append(("NAMED_ARG", arg_name, arg_value))
        else:
            arg, i = parse_expression(tokens, i)
            args.append(arg)
        first = False
    if i >= len(tokens) or tokens[i][0] != "RPAREN":
        raise OrionSyntaxError("Falta ')' en llamada")
    return args, i + 1

def parse_primary(tokens, i):
    while i < len(tokens) and tokens[i][0] in ("OP", "COMMA"):
        i += 1
    if i >= len(tokens):
        raise OrionSyntaxError("Expresión inesperadamente vacía.")

    kind, value = tokens[i]

    # --- If como expresión (if cond { expr1 } else { expr2 }) ---
    if kind == "IF" or (kind == "IDENT" and value == "if"):
        condition, i = parse_expression(tokens, i + 1)

        # bloque o expresión verdadera
        if i < len(tokens) and tokens[i][0] == "LBRACE":
            then_branch, i = parse_block(tokens, i)
        else:
            then_branch, i = parse_expression(tokens, i)

        # bloque o expresión falsa (opcional)
        else_branch = None
        if i < len(tokens) and (tokens[i][0] == "ELSE" or (tokens[i][0] == "IDENT" and tokens[i][1] == "else")):
            i += 1
            if i < len(tokens) and tokens[i][0] == "LBRACE":
                else_branch, i = parse_block(tokens, i)
            else:
                else_branch, i = parse_expression(tokens, i)

        return ("IF_EXPR", condition, then_branch, else_branch), i

    # --- Listas literales ---
    if kind == "LBRACKET":
        items = []
        i += 1
        while i < len(tokens) and tokens[i][0] != "RBRACKET":
            item, i = parse_expression(tokens, i)
            items.append(item)
            if i < len(tokens) and tokens[i][0] == "COMMA":
                i += 1
        if i >= len(tokens) or tokens[i][0] != "RBRACKET":
            raise OrionSyntaxError("Falta ']' en lista")
        return ("LIST", items), i + 1

    # --- Diccionarios literales ---
    if kind == "LBRACE":
        items = []
        i += 1
        while i < len(tokens) and tokens[i][0] != "RBRACE":
            if tokens[i][0] not in ("STRING", "IDENT"):
                raise OrionSyntaxError("Se esperaba una clave STRING o IDENT en el diccionario")
            key = tokens[i][1]
            # Limpia las comillas si es STRING
            if tokens[i][0] == "STRING":
                key = key[1:-1]  # elimina la primera y última comilla
            i += 1
            if i >= len(tokens) or tokens[i][0] != "COLON":
                raise OrionSyntaxError("Se esperaba ':' en el diccionario")
            i += 1
            val, i = parse_expression(tokens, i)
            items.append((key, val))
            if i < len(tokens) and tokens[i][0] == "COMMA":
                i += 1
        if i >= len(tokens) or tokens[i][0] != "RBRACE":
            raise OrionSyntaxError("Falta '}' en el diccionario")
        return ("DICT", items), i + 1

    # --- Expresiones entre paréntesis ---
    if kind == "LPAREN":
        expr, i = parse_expression(tokens, i + 1)
        if i >= len(tokens) or tokens[i][0] != "RPAREN":
            raise OrionSyntaxError("Falta ')'")
        return expr, i + 1

    # --- Números ---
    elif kind == "NUMBER":
        return (float(value) if "." in str(value) else int(value)), i + 1

    # --- Cadenas ---
    elif kind == "STRING":
        return value, i + 1

    # --- Booleanos (yes, no) ---
    elif kind == "BOOL":
        return value, i + 1

    # --- Tipos (int, str, bool, etc.) ---
    elif kind == "TYPE":
        return ("TYPE", value), i + 1

    # --- Identificadores, llamadas, atributos, índices ---
    elif kind == "IDENT":
        name = ("IDENT", value)
        i += 1

        # Null-safe: user?.email
        if i < len(tokens) and tokens[i][0] == "NULL_SAFE":
            attr = tokens[i + 1][1]
            return ("NULL_SAFE", name, attr), i + 2

        # Llamada de función normal
        if i < len(tokens) and tokens[i][0] == "LPAREN":
            args, i = parse_call_args(tokens, i)
            name = ("CALL", value, args)

        # Acceso a propiedades, métodos o índices
        while i < len(tokens):
            if tokens[i][0] == "DOT":
                i += 1
                attr_name = tokens[i][1]
                i += 1
                # Llamada de método
                if i < len(tokens) and tokens[i][0] == "LPAREN":
                    args, i = parse_call_args(tokens, i)
                    name = ("CALL_METHOD", attr_name, name, args)
                else:
                    name = ("ATTR_ACCESS", name, attr_name)
            elif tokens[i][0] == "LBRACKET":
                i += 1
                index_expr, i = parse_expression(tokens, i)
                if i >= len(tokens) or tokens[i][0] != "RBRACKET":
                    raise OrionSyntaxError("Falta ']' en acceso por índice")
                i += 1
                name = ("INDEX", name, index_expr)
            else:
                break

        return name, i

    # --- Operador unario ---
    elif kind == "NOT":
        expr, j = parse_primary(tokens, i + 1)
        return ("UNARY_OP", "!", expr), j

    else:
        raise OrionSyntaxError(f"Token inesperado: {kind}")

def parse_unary(tokens, i):
    if i < len(tokens) and tokens[i][0] == "NOT":
        expr, j = parse_unary(tokens, i+1)
        return ("UNARY_OP", "!", expr), j
    return parse_primary(tokens, i)


def parse_term(tokens, i):
    left, i = parse_unary(tokens, i)
    while i < len(tokens) and tokens[i][0] == "OP" and tokens[i][1] in ("*", "/", "%"):
        op = tokens[i][1]
        right, i = parse_unary(tokens, i+1)
        left = ("BINARY_OP", op, left, right)
    return left, i


def parse_arith(tokens, i):
    left, i = parse_term(tokens, i)
    while i < len(tokens) and tokens[i][0] == "OP" and tokens[i][1] in ("+", "-"):
        op = tokens[i][1]
        right, i = parse_term(tokens, i+1)
        left = ("BINARY_OP", op, left, right)
    return left, i


def parse_compare(tokens, i):
    left, i = parse_arith(tokens, i)
    while i < len(tokens) and tokens[i][0] == "COMPARE":
        op = tokens[i][1]
        right, i = parse_arith(tokens, i+1)
        left = ("BINARY_OP", op, left, right)
    return left, i


def parse_and(tokens, i):
    left, i = parse_compare(tokens, i)
    while i < len(tokens) and tokens[i][0] == "AND":
        op = tokens[i][1]
        right, i = parse_compare(tokens, i+1)
        left = ("BINARY_OP", op, left, right)
    return left, i


def parse_or(tokens, i):
    left, i = parse_and(tokens, i)
    while i < len(tokens) and tokens[i][0] == "OR":
        op = tokens[i][1]
        right, i = parse_and(tokens, i+1)
        left = ("BINARY_OP", op, left, right)
    return left, i


def parse_expression(tokens, i):
    expr, j = parse_or(tokens, i)
    if j < len(tokens) and tokens[j][0] == "LBRACE":
        return expr, j

    return expr, j


def parse_block(tokens, i):
    stmts = []
    if i >= len(tokens) or tokens[i][0] != "LBRACE":
        raise OrionSyntaxError("Se esperaba '{'")

    i += 1

    while i < len(tokens) and tokens[i][0] != "RBRACE":
        kind = tokens[i][0]
        value = tokens[i][1] if len(tokens[i]) > 1 else None

        # Soporte para 'use'
        if kind == "USE":
            if i+1 >= len(tokens) or tokens[i+1][0] not in ("STRING", "IDENT"):
                raise OrionSyntaxError("Se esperaba una cadena o identificador después de 'use'")
            module_path = tokens[i+1][1]
            stmts.append(("USE", module_path))
            i += 2
            continue

        # Agregar soporte para PRINT dentro de bloques
        elif kind == "PRINT":
            # Permitir show con o sin paréntesis
            if i+1 < len(tokens) and tokens[i+1][0] == "LPAREN":
                # Sintaxis tradicional: show(expr)
                args, i = parse_call_args(tokens, i+1)
                stmts.append(("CALL", "show", args))
            else:
                # Sintaxis sin paréntesis: show expr
                expr, i = parse_expression(tokens, i+1)
                stmts.append(("CALL", "show", [expr]))
            continue

        # Soporte para FN como palabra clave o como IDENT "fn"
        elif kind == "FN" or (kind == "IDENT" and value == "fn"):
            if i+1 >= len(tokens) or tokens[i+1][0] != "IDENT":
                raise OrionSyntaxError("Se esperaba un nombre de función después de 'fn'")
            fn_name = tokens[i+1][1]

            # Parámetros
            if i+2 >= len(tokens) or tokens[i+2][0] != "LPAREN":
                raise OrionSyntaxError("Se esperaba '(' después del nombre de función")
            params, j = parse_fn_params(tokens, i+2)

            # Cuerpo
            if j >= len(tokens) or tokens[j][0] != "LBRACE":
                raise OrionSyntaxError("Se esperaba '{' al inicio del cuerpo de la función")
            body, j = parse_block(tokens, j)

            stmts.append(("FN", fn_name, params, body))
            i = j
            continue

        elif kind == "IDENT":
            # Asignación
            if i+1 < len(tokens) and tokens[i+1][0] == "ASSIGN":
                var_name = tokens[i][1]
                expr_value, i = parse_expression(tokens, i+2)
                stmts.append(("ASSIGN", var_name, expr_value))
            # Llamada a función
            elif i+1 < len(tokens) and tokens[i+1][0] == "LPAREN":
                call_expr, i = parse_primary(tokens, i)
                stmts.append(call_expr)
            # Null-safe
            elif i+1 < len(tokens) and tokens[i+1][0] == "NULL_SAFE":
                attr = tokens[i+2][1]
                stmts.append(("NULL_SAFE", var_name, attr))
                i += 3
            else:
                i += 1

        elif kind == "RETURN":
            # return puede no llevar expresión
            if i+1 < len(tokens) and tokens[i+1][0] not in ("RBRACE", "ELSE"):
                expr_value, i = parse_expression(tokens, i+1)
                stmts.append(("RETURN", expr_value))
            else:
                stmts.append(("RETURN", None))
                i += 1

        elif kind == "IF":
            # Parsear la condición
            condition, i = parse_expression(tokens, i + 1)

            # Esperar el bloque { ... } después del if
            if i >= len(tokens) or tokens[i][0] != "LBRACE":
                raise OrionSyntaxError("Se esperaba '{' después de la condición del if")
            body_true, i = parse_block(tokens, i)

            #Verificar si existe un else
            body_false = []
            if i < len(tokens) and tokens[i][0] == "ELSE":
                i += 1  # muy importante, avanza después de 'else'
                if i < len(tokens) and tokens[i][0] == "LBRACE":
                    body_false, i = parse_block(tokens, i)
                else:
                    raise OrionSyntaxError("Se esperaba '{' después de 'else'")

            # Agregar el nodo al AST
            stmts.append(("IF", condition, body_true, body_false))

        elif kind == "FOR":
            # for i in 1..n { ... } o for h in host { ... }
            if tokens[i+1][0] != "IDENT":
                raise OrionSyntaxError("Se esperaba un identificador después de 'for'")
            var_name = tokens[i+1][1]

            if tokens[i+2][0] != "IN":
                raise OrionSyntaxError("Se esperaba 'in' en el bucle for")

            expr, j = parse_expression(tokens, i+3)

            # Si es rango, espera RANGE/RANGE_EX
            if j < len(tokens) and tokens[j][0] in ("RANGE", "RANGE_EX"):
                range_type = tokens[j][0]
                j += 1
                end, j = parse_expression(tokens, j)
                body, j = parse_block(tokens, j)
                stmts.append(("FOR_RANGE", var_name, expr, end, body, range_type))
                i = j
            else:
                # Si no hay RANGE, es colección
                body, j = parse_block(tokens, j)
                stmts.append(("FOR_IN", var_name, expr, body))
                i = j
        elif kind == "FN":
            if i+1 >= len(tokens) or tokens[i+1][0] != "IDENT":
                raise OrionSyntaxError("Se esperaba un nombre de función después de 'fn'")
            fn_name = tokens[i+1][1]

            # Parámetros
            if i+2 >= len(tokens) or tokens[i+2][0] != "LPAREN":
                raise OrionSyntaxError("Se esperaba '(' después del nombre de función")
            params, j = parse_fn_params(tokens, i+2)

            # Cuerpo
            if j >= len(tokens) or tokens[j][0] != "LBRACE":
                raise OrionSyntaxError("Se esperaba '{' al inicio del cuerpo de la función")
            body, j = parse_block(tokens, j)

            stmts.append(("FN", fn_name, params, body))
            i = j


        # MATCH
        elif kind == "MATCH":
            expr, j = parse_expression(tokens, i+1)
            if j >= len(tokens) or tokens[j][0] != "LBRACE":
                raise OrionSyntaxError("Se esperaba '{' después de match")
            j += 1
            cases = []
            while j < len(tokens) and tokens[j][0] != "RBRACE":
                case_kind = tokens[j][0]
                # else case
                if case_kind == "ELSE":
                    if j+1 >= len(tokens) or tokens[j+1][0] != "COLON":
                        raise OrionSyntaxError("Se esperaba ':' después de else en match")
                    if j+2 >= len(tokens) or tokens[j+2][0] != "LBRACE":
                        raise OrionSyntaxError("Se esperaba '{' en else de match")
                    body, j2 = parse_block(tokens, j+2)
                    cases.append(("else", body))
                    j = j2
                else:
                    # pattern: { ... }
                    pattern = tokens[j][1]
                    if j+1 >= len(tokens) or tokens[j+1][0] != "COLON":
                        raise OrionSyntaxError("Se esperaba ':' después del patrón en match")
                    if j+2 >= len(tokens) or tokens[j+2][0] != "LBRACE":
                        raise OrionSyntaxError("Se esperaba '{' en el patrón de match")
                    body, j2 = parse_block(tokens, j+2)
                    cases.append((pattern, body))
                    j = j2
            if j >= len(tokens) or tokens[j][0] != "RBRACE":
                raise OrionSyntaxError("Se esperaba '}' al final de match")
            stmts.append(("MATCH", expr, cases))
            i = j + 1

        else:
            i += 1

    if i >= len(tokens):
        raise OrionSyntaxError("Se esperaba '}' pero se alcanzó el final del archivo.")
    print("parse_block stmts:", stmts)
    return stmts, i + 1


def parse(tokens):
    ast = []
    i = 0
    while i < len(tokens):
        kind = tokens[i][0]
        value = tokens[i][1] if len(tokens[i]) > 1 else None

        # Soporte para bucles FOR en el nivel superior
        if kind == "FOR":
            if tokens[i+1][0] != "IDENT":
                raise OrionSyntaxError("Se esperaba un identificador después de 'for'")
            var_name = tokens[i+1][1]

            if tokens[i+2][0] != "IN":
                raise OrionSyntaxError("Se esperaba 'in' en el bucle for")

            expr, j = parse_expression(tokens, i+3)

            # Si es rango, espera RANGE/RANGE_EX
            if j < len(tokens) and tokens[j][0] in ("RANGE", "RANGE_EX"):
                range_type = tokens[j][0]
                j += 1
                end, j = parse_expression(tokens, j)
                body, j = parse_block(tokens, j)
                ast.append(("FOR_RANGE", var_name, expr, end, body, range_type))
                i = j
                continue
            else:
                # Si no hay RANGE, es colección
                body, j = parse_block(tokens, j)
                ast.append(("FOR_IN", var_name, expr, body))
                i = j
                continue

        # Soporte para 'use'
        if kind == "USE":
            if i+1 >= len(tokens) or tokens[i+1][0] not in ("STRING", "IDENT"):
                raise OrionSyntaxError("Se esperaba una cadena o identificador después de 'use'")
            module_path = tokens[i+1][1]
            ast.append(("USE", module_path))
            i += 2
            continue

        # Soporte para FN como palabra clave o como IDENT "fn"
        if kind == "FN" or (kind == "IDENT" and value == "fn"):
            fn_name = tokens[i+1][1]
            i += 2  # saltamos FN y nombre
            params = []

            # Si hay paréntesis de parámetros
            if i < len(tokens) and tokens[i][0] == "LPAREN":
                i += 1  # saltamos '('
                while i < len(tokens) and tokens[i][0] != "RPAREN":
                    if tokens[i][0] == "IDENT":
                        params.append(tokens[i][1])
                    i += 1
                if i >= len(tokens) or tokens[i][0] != "RPAREN":
                    raise OrionSyntaxError("Se esperaba ')' en la declaración de función.")
                i += 1 

            # Esperamos el cuerpo
            if i >= len(tokens) or tokens[i][0] != "LBRACE":
                raise OrionSyntaxError("Se esperaba '{' después de la declaración de función.")
            body, i = parse_block(tokens, i)
            ast.append(("FN", fn_name, params, body))
            continue

        if kind == "IDENT":
            # Asignación
            if i+1 < len(tokens) and tokens[i+1][0] == "ASSIGN":
                var_name = tokens[i][1]
                expr_value, i = parse_expression(tokens, i+2)
                ast.append(("ASSIGN", var_name, expr_value))
            # Llamada a función
            elif i+1 < len(tokens) and tokens[i+1][0] == "LPAREN":
                call_expr, i = parse_primary(tokens, i)
                ast.append(call_expr)
            # Null-safe
            elif i+1 < len(tokens) and tokens[i+1][0] == "NULL_SAFE":
                attr = tokens[i+2][1]
                ast.append(("NULL_SAFE", tokens[i][1], attr))
                i += 3
            else:
                i += 1

        elif kind == "PRINT":
            # Permitir show con o sin paréntesis
            if i+1 < len(tokens) and tokens[i+1][0] == "LPAREN":
                # Sintaxis tradicional: show(expr)
                args, i = parse_call_args(tokens, i+1)
                ast.append(("CALL", "show", args))
            else:
                # Sintaxis sin paréntesis: show expr
                expr, i = parse_expression(tokens, i+1)
                ast.append(("CALL", "show", [expr]))

        elif kind == "IF":
            condition, i = parse_expression(tokens, i+1)
            body_true, i = parse_block(tokens, i)
            body_false = []
            if i < len(tokens) and tokens[i][0] == "ELSE":
                body_false, i = parse_block(tokens, i+1)
            ast.append(("IF", condition, body_true, body_false))

        elif kind == "FOR":
            # for i in 1..n { ... } o for h in host { ... }
            if tokens[i+1][0] != "IDENT":
                raise OrionSyntaxError("Se esperaba un identificador después de 'for'")
            var_name = tokens[i+1][1]

            if tokens[i+2][0] != "IN":
                raise OrionSyntaxError("Se esperaba 'in' en el bucle for")

            # expresión inicial
            expr, j = parse_expression(tokens, i+3)

            # Si es rango, espera RANGE/RANGE_EX
            if j < len(tokens) and tokens[j][0] in ("RANGE", "RANGE_EX"):
                range_type = tokens[j][0]
                j += 1

                # expresión final
                end, j = parse_expression(tokens, j)

                # cuerpo del for
                body, j = parse_block(tokens, j)

                # En parse:
                ast.append(("FOR_RANGE", var_name, expr, end, body, range_type))
                i = j
            else:
                # Si no hay RANGE, es colección
                body, j = parse_block(tokens, j)
                ast.append(("FOR_IN", var_name, expr, body))
                i = j

        else:
            i += 1

    return ast

def parse_fn_params(tokens, i):
    """Parsea los parámetros de una función."""
    if tokens[i][0] != "LPAREN":
        raise OrionSyntaxError("Se esperaba '(' al inicio de los parámetros")
    i += 1
    params = []
    while i < len(tokens) and tokens[i][0] != "RPAREN":
        if tokens[i][0] == "IDENT":
            params.append(tokens[i][1])
        i += 1
    if i >= len(tokens) or tokens[i][0] != "RPAREN":
        raise OrionSyntaxError("Se esperaba ')' al final de los parámetros")
    return params, i + 1

