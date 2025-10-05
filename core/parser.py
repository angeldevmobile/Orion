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

    # --- Booleanos ---
    elif kind in ("TRUE", "FALSE"):
        return (kind == "TRUE"), i + 1

    # --- Identificadores, llamadas, atributos, índices ---
    elif kind == "IDENT":
        name = ("IDENT", value)  # ✅ AST correcto para variables
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
                # Llamada de método: obj.metodo(...)
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
    return parse_or(tokens, i)


def parse_block(tokens, i):
    stmts = []
    if i >= len(tokens) or tokens[i][0] != "LBRACE":
        raise OrionSyntaxError("Se esperaba '{'")

    i += 1

    while i < len(tokens) and tokens[i][0] != "RBRACE":
        kind = tokens[i][0]

        if kind == "PRINT":
            if i+1 >= len(tokens) or tokens[i+1][0] != "LPAREN":
                raise OrionSyntaxError("Se esperaba '(' después de show")

            args, i = parse_call_args(tokens, i+1)
            stmts.append(("CALL", "show", args))

        elif kind == "TYPE":
            type_name = tokens[i][1]
            if i+1 < len(tokens) and tokens[i+1][0] == "IDENT":
                var_name = tokens[i+1][1]
                if i+2 < len(tokens) and tokens[i+2][0] == "ASSIGN":
                    expr_value, i = parse_expression(tokens, i+3)
                    stmts.append(("DECLARE", type_name, var_name, expr_value))
                else:
                    stmts.append(("DECLARE", type_name, var_name, None))
                    i += 2
            else:
                i += 1

        elif kind == "IDENT":
            var_name = tokens[i][1]
            # Asignación
            if i+1 < len(tokens) and tokens[i+1][0] == "ASSIGN":
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
            condition, i = parse_expression(tokens, i+1)
            body_true, i = parse_block(tokens, i)
            body_false = []
            if i < len(tokens) and tokens[i][0] == "ELSE":
                body_false, i = parse_block(tokens, i+1)
            stmts.append(("IF", condition, body_true, body_false))

        elif kind == "FOR":
            # for i in 1..n { ... }
            if tokens[i+1][0] != "IDENT":
                raise OrionSyntaxError("Se esperaba un identificador después de 'for'")
            var_name = tokens[i+1][1]

            if tokens[i+2][0] != "IN":
                raise OrionSyntaxError("Se esperaba 'in' en el bucle for")

            # expresión inicial
            start, j = parse_expression(tokens, i+3)

            if j >= len(tokens) or tokens[j][0] not in ("RANGE", "RANGE_EX"):
                raise OrionSyntaxError("Se esperaba '..' o '..<' en el bucle for")
            range_type = tokens[j][0]
            j += 1

            # expresión final
            end, j = parse_expression(tokens, j)

            # cuerpo del for
            body, j = parse_block(tokens, j)

            # En parse_block:
            stmts.append(("FOR", var_name, start, end, body, range_type))
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

    return stmts, i + 1


def parse(tokens):
    ast = []
    i = 0
    while i < len(tokens):
        kind = tokens[i][0]

        if kind == "FN":
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

        elif kind == "TYPE":
            type_name = tokens[i][1]
            if i+1 < len(tokens) and tokens[i+1][0] == "IDENT":
                var_name = tokens[i+1][1]
                if i+2 < len(tokens) and tokens[i+2][0] == "ASSIGN":
                    expr_value, i = parse_expression(tokens, i+3)
                    ast.append(("DECLARE", type_name, var_name, expr_value))
                else:
                    ast.append(("DECLARE", type_name, var_name, None))
                    i += 2
            else:
                i += 1

        elif kind == "IDENT":
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
            if i+1 >= len(tokens) or tokens[i+1][0] != "LPAREN":
                raise OrionSyntaxError("Se esperaba '(' después de show")

            args, i = parse_call_args(tokens, i+1)
            ast.append(("CALL", "show", args))

        elif kind == "IF":
            condition, i = parse_expression(tokens, i+1)
            body_true, i = parse_block(tokens, i)
            body_false = []
            if i < len(tokens) and tokens[i][0] == "ELSE":
                body_false, i = parse_block(tokens, i+1)
            ast.append(("IF", condition, body_true, body_false))

        elif kind == "FOR":
            # for i in 1..n { ... }
            if tokens[i+1][0] != "IDENT":
                raise OrionSyntaxError("Se esperaba un identificador después de 'for'")
            var_name = tokens[i+1][1]

            if tokens[i+2][0] != "IN":
                raise OrionSyntaxError("Se esperaba 'in' en el bucle for")

            # expresión inicial
            start, j = parse_expression(tokens, i+3)

            if j >= len(tokens) or tokens[j][0] not in ("RANGE", "RANGE_EX"):
                raise OrionSyntaxError("Se esperaba '..' o '..<' en el bucle for")
            range_type = tokens[j][0]
            j += 1

            # expresión final
            end, j = parse_expression(tokens, j)

            # cuerpo del for
            body, j = parse_block(tokens, j)

            # En parse:
            ast.append(("FOR", var_name, start, end, body, range_type))
            i = j

        else:
            i += 1

    return ast
