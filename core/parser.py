from core.errors import OrionSyntaxError


def parse_call_args(tokens, i):
    """
    Parser extendido de Orion para llamadas de función futuristas.
    Soporta:
      - Argumentos posicionales: f(x, y, z)
      - Argumentos nombrados: f(lr="auto", epochs=100)
      - Anidación de llamadas: f(auto(1, precision="high"))
      - Funciones lambda con argumentos nombrados: map(x => func(x, arg=val))
      - Validación robusta de comas y cierre de paréntesis
    """
    # Verificar apertura de paréntesis
    if i >= len(tokens) or tokens[i][0] != "LPAREN":
        raise OrionSyntaxError("Se esperaba '(' en llamada de función")
    
    i += 1  # saltar '('
    args = []
    kwargs = {}

    # Detección temprana de cierre vacío: f()
    if i < len(tokens) and tokens[i][0] == "RPAREN":
        return args, kwargs, i + 1

    while i < len(tokens):
        # Verificar si es argumento nombrado en el nivel actual
        if (i + 1 < len(tokens)
            and tokens[i][0] == "IDENT"
            and tokens[i + 1][0] == "ASSIGN"):
            
            # Contar profundidad de paréntesis y buscar ARROW
            # para determinar si estamos en el nivel correcto
            paren_depth = 0
            has_arrow_at_this_level = False
            temp_i = i - 1
            
            # Buscar hacia atrás desde la posición actual
            while temp_i >= 0:
                if tokens[temp_i][0] == "RPAREN":
                    paren_depth += 1
                elif tokens[temp_i][0] == "LPAREN":
                    if paren_depth == 0:
                        # Llegamos al inicio de esta llamada
                        break
                    paren_depth -= 1
                elif tokens[temp_i][0] == "ARROW" and paren_depth == 0:
                    # Hay un ARROW en este mismo nivel
                    has_arrow_at_this_level = True
                    break
                temp_i -= 1
            
            # Si no hay ARROW en este nivel, es un argumento nombrado de esta función
            if not has_arrow_at_this_level:
                name = tokens[i][1]
                i += 2  # saltar IDENT y '='
                value, i = parse_expression(tokens, i)
                kwargs[name] = value
            else:
                # Hay ARROW en este nivel, tratar como parte de expresión lambda
                value, i = parse_expression(tokens, i)
                args.append(value)
        else:
            # --- MEJORA: Detectar lambda como argumento ---
            # Si el argumento inicia con LPAREN y luego ARROW, es lambda
            if (i + 2 < len(tokens)
                and tokens[i][0] == "LPAREN"
                and tokens[i+2][0] == "ARROW"):
                value, i = parse_lambda(tokens, i)
                args.append(value)
            else:
                # --- MEJORA ROBUSTA: Detectar lambda con múltiples parámetros ---
                if i < len(tokens) and tokens[i][0] == "LPAREN":
                    # Buscar el cierre de paréntesis y el ARROW
                    temp = i + 1
                    paren_count = 1
                    while temp < len(tokens) and paren_count > 0:
                        if tokens[temp][0] == "LPAREN":
                            paren_count += 1
                        elif tokens[temp][0] == "RPAREN":
                            paren_count -= 1
                        temp += 1
                    # Si después del cierre hay ARROW, es lambda
                    if temp < len(tokens) and tokens[temp][0] == "ARROW":
                        value, i = parse_lambda(tokens, i)
                        args.append(value)
                    else:
                        value, i = parse_expression(tokens, i)
                        args.append(value)
                else:
                    value, i = parse_expression(tokens, i)
                    args.append(value)

        # Si hay coma, continuar con el siguiente argumento
        if i < len(tokens) and tokens[i][0] == "COMMA":
            i += 1
            continue
        
        # Si hay cierre de paréntesis, terminamos
        if i < len(tokens) and tokens[i][0] == "RPAREN":
            return args, kwargs, i + 1

        # Si no hay coma ni cierre → error
        current_token = tokens[i] if i < len(tokens) else ("EOF", "")
        raise OrionSyntaxError(f"Se esperaba ',' o ')' después de argumento en llamada de función, pero se encontró '{current_token}'")
    
    # Si termina sin cierre correcto
    raise OrionSyntaxError("Se esperaba ')' al final de la llamada de función")


def parse_primary(tokens, i):
    if i >= len(tokens):
        raise OrionSyntaxError("Fin inesperado de entrada")

    kind, value = tokens[i]

    if kind == "NUMBER":
        return value, i + 1
    elif kind == "STRING":
        return value, i + 1
    elif kind == "BOOL":
        return value.value, i + 1
    elif kind == "TYPE":
        return ("TYPE", value), i + 1

    elif kind == "IDENT":
        expr = ("IDENT", value)
        i += 1
        while i < len(tokens):
            token_type = tokens[i][0]
            if token_type == "DOT":
                i += 1
                if i >= len(tokens) or tokens[i][0] != "IDENT":
                    raise OrionSyntaxError("Se esperaba un identificador después de '.'")
                attr_name = tokens[i][1]
                i += 1
                if i < len(tokens) and tokens[i][0] == "LPAREN":
                    args, kwargs, i = parse_call_args(tokens, i)
                    expr = ("CALL_METHOD", attr_name, expr, args, kwargs)
                else:
                    expr = ("ATTR_ACCESS", expr, attr_name)
            elif token_type == "LBRACKET":
                i += 1
                if i < len(tokens) and tokens[i][0] == "COLON":
                    i += 1
                    if i < len(tokens) and tokens[i][0] != "RBRACKET":
                        end_index, i = parse_expression(tokens, i)
                        slice_expr = ("SLICE", None, end_index, None)
                    else:
                        slice_expr = ("SLICE", None, None, None)
                elif i < len(tokens) and tokens[i][0] != "RBRACKET":
                    first_expr, i = parse_expression(tokens, i)
                    if i < len(tokens) and tokens[i][0] == "COLON":
                        i += 1
                        if i < len(tokens) and tokens[i][0] != "RBRACKET":
                            end_expr, i = parse_expression(tokens, i)
                            slice_expr = ("SLICE", first_expr, end_expr, None)
                        else:
                            slice_expr = ("SLICE", first_expr, None, None)
                    else:
                        slice_expr = first_expr
                else:
                    slice_expr = ("SLICE", None, None, None)
                if i >= len(tokens) or tokens[i][0] != "RBRACKET":
                    raise OrionSyntaxError("Se esperaba ']'")
                i += 1
                if isinstance(slice_expr, tuple) and slice_expr[0] == "SLICE":
                    expr = ("SLICE_ACCESS", expr, slice_expr)
                else:
                    expr = ("INDEX", expr, slice_expr)
            elif token_type == "LPAREN":
                args, kwargs, i = parse_call_args(tokens, i)
                expr = ("CALL", expr, args, kwargs)
            elif token_type == "SAFE_ACCESS" or token_type == "NULL_SAFE":
                i += 1
                if i >= len(tokens) or tokens[i][0] != "IDENT":
                    raise OrionSyntaxError("Se esperaba un identificador después de '?.'")
                attr_name = tokens[i][1]
                i += 1
                expr = ("NULL_SAFE", expr, attr_name)
            else:
                break
        return expr, i

    elif kind == "LPAREN":
        i += 1
        expr, i = parse_expression(tokens, i)
        if i >= len(tokens) or tokens[i][0] != "RPAREN":
            raise OrionSyntaxError("Se esperaba ')'")
        return expr, i + 1

    elif kind == "LBRACKET":
        i += 1
        elements = []
        while i < len(tokens) and tokens[i][0] != "RBRACKET":
            elem, i = parse_expression(tokens, i)
            elements.append(elem)
            if i < len(tokens) and tokens[i][0] == "COMMA":
                i += 1
            elif i < len(tokens) and tokens[i][0] != "RBRACKET":
                raise OrionSyntaxError("Se esperaba ',' o ']' en lista")
        if i >= len(tokens):
            raise OrionSyntaxError("Se esperaba ']'")
        i += 1
        return ("LIST", elements), i

    elif kind == "LBRACE":
        i += 1
        items = []
        while i < len(tokens) and tokens[i][0] != "RBRACE":
            if tokens[i][0] not in ["STRING", "IDENT"]:
                raise OrionSyntaxError("Se esperaba clave de diccionario")
            key = tokens[i][1]
            if tokens[i][0] == "STRING":
                key = key.strip('"')
            i += 1
            if i >= len(tokens) or tokens[i][0] != "COLON":
                raise OrionSyntaxError("Se esperaba ':' después de clave")
            i += 1
            value, i = parse_expression(tokens, i)
            items.append((key, value))
            if i < len(tokens) and tokens[i][0] == "COMMA":
                i += 1
            elif i < len(tokens) and tokens[i][0] != "RBRACE":
                raise OrionSyntaxError("Se esperaba ',' o '}' en diccionario")
        if i >= len(tokens):
            raise OrionSyntaxError("Se esperaba '}'")
        i += 1
        return ("DICT", items), i

    # --- CORRECCIÓN: Manejar tokens no válidos para expresiones primarias ---
    else:
        raise OrionSyntaxError(f"Token inesperado en expresión primaria: '{kind}' ('{value}')")

def parse_unary(tokens, i):
    if i < len(tokens) and tokens[i][0] == "NOT":
        expr, j = parse_unary(tokens, i+1)
        return ("UNARY_OP", "!", expr), j
    
    # Verificar que hay tokens disponibles
    if i >= len(tokens):
        raise OrionSyntaxError("Fin inesperado de entrada en expresión unaria")
    
    # parse_primary ya maneja todos los errores internamente
    expr, j = parse_primary(tokens, i)
    return expr, j

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
    # Verificar si es una lambda
    if i < len(tokens):
        # Caso 1: IDENT => expr
        if (i + 1 < len(tokens) and 
            tokens[i][0] == "IDENT" and 
            tokens[i + 1][0] == "ARROW"):
            return parse_lambda(tokens, i)
        
        # Caso 2: (param1, param2) => expr
        elif (tokens[i][0] == "LPAREN"):
            # Mirar hacia adelante para ver si es lambda
            temp_i = i + 1
            paren_count = 1
            might_be_lambda = False
            
            while temp_i < len(tokens) and paren_count > 0:
                if tokens[temp_i][0] == "LPAREN":
                    paren_count += 1
                elif tokens[temp_i][0] == "RPAREN":
                    paren_count -= 1
                elif tokens[temp_i][0] == "ARROW" and paren_count == 0:
                    might_be_lambda = True
                    break
                temp_i += 1
            
            # Verificar si inmediatamente después del ')' hay '=>'
            if (paren_count == 0 and temp_i + 1 < len(tokens) and 
                tokens[temp_i + 1][0] == "ARROW"):
                return parse_lambda(tokens, i)
    
    # No es lambda, usar parsing normal
    expr, j = parse_or(tokens, i)
    return expr, j
def parse_statement(tokens, i):
    """Parsea una declaración individual con mejor manejo de errores."""
    if i >= len(tokens):
        raise OrionSyntaxError("Se esperaba una declaración")
        
    kind = tokens[i][0]
    value = tokens[i][1] if len(tokens[i]) > 1 else None
    
    # --- VAR statement ---
    if kind == "IDENT" and value == "var":
        if i+1 >= len(tokens) or tokens[i+1][0] != "IDENT":
            raise OrionSyntaxError("Se esperaba un nombre de variable después de 'var'")
        var_name = tokens[i+1][1]
        if i+2 >= len(tokens) or tokens[i+2][0] != "ASSIGN":
            raise OrionSyntaxError("Se esperaba '=' después del nombre de variable")
        expr_value, j = parse_expression(tokens, i+3)
        return ("VAR", var_name, expr_value), j

    # --- USE statement ---
    elif kind == "USE":
        if i+1 >= len(tokens) or tokens[i+1][0] not in ("STRING", "IDENT"):
            raise OrionSyntaxError("Se esperaba una cadena o identificador después de 'use'")
        module_path = tokens[i+1][1]
        return ("USE", module_path), i + 2

    # --- PRINT/SHOW statement ---
    elif kind == "PRINT":
        if i+1 < len(tokens) and tokens[i+1][0] == "LPAREN":
            # Sintaxis tradicional: show(expr)
            args, kwargs, i = parse_call_args(tokens, i+1)
            return ("CALL", "show", args, kwargs), i
        else:
            # Sintaxis sin paréntesis: show expr, arg2, ..., kwarg1=val1, ...
            args = []
            kwargs = {}
            i += 1
            # Primer argumento obligatorio
            expr, i = parse_expression(tokens, i)
            args.append(expr)
            # Procesar argumentos adicionales
            while i < len(tokens):
                if tokens[i][0] == "COMMA":
                    i += 1
                    # Argumento nombrado: IDENT ASSIGN expr
                    if (i + 1 < len(tokens)
                        and tokens[i][0] == "IDENT"
                        and tokens[i+1][0] == "ASSIGN"):
                        name = tokens[i][1]
                        i += 2
                        value, i = parse_expression(tokens, i)
                        kwargs[name] = value
                    else:
                        # Argumento posicional extra
                        value, i = parse_expression(tokens, i)
                        args.append(value)
                else:
                    break
            return ("CALL", "show", args, kwargs), i

    # --- RETURN statement ---
    elif kind == "RETURN":
        if i+1 < len(tokens) and tokens[i+1][0] not in ("RBRACE", "ELSE"):
            expr_value, i = parse_expression(tokens, i+1)
            return ("RETURN", expr_value), i
        else:
            return ("RETURN", None), i + 1

    # --- IF statement ---
    elif kind == "IF":
        condition, i = parse_expression(tokens, i + 1)
        if i >= len(tokens) or tokens[i][0] != "LBRACE":
            raise OrionSyntaxError("Se esperaba '{' después de la condición del if")
        body_true, i = parse_block(tokens, i)

        body_false = []
        if i < len(tokens) and tokens[i][0] == "ELSE":
            i += 1
            if i < len(tokens) and tokens[i][0] == "LBRACE":
                body_false, i = parse_block(tokens, i)
            else:
                raise OrionSyntaxError("Se esperaba '{' después de 'else'")

        return ("IF", condition, body_true, body_false), i

        # --- FOR statement ---
    elif kind == "FOR":
        i += 1

        # Permitir opcionalmente paréntesis después de for
        has_paren = False
        if i < len(tokens) and tokens[i][0] == "LPAREN":
            has_paren = True
            i += 1

        # Variable del bucle
        if i >= len(tokens) or tokens[i][0] != "IDENT":
            raise OrionSyntaxError("Se esperaba un identificador después de for")
        var_name = tokens[i][1]
        i += 1

        # Palabra clave 'in'
        if i >= len(tokens) or tokens[i][0] != "IN":
            raise OrionSyntaxError("Se esperaba 'in' en el bucle for")
        i += 1

        # Soporte para rango estilo 1..n
        start_expr, i = parse_expression(tokens, i)
        if i < len(tokens) and tokens[i][0] == "RANGE":
            i += 1
            end_expr, i = parse_expression(tokens, i)
            expr = ("RANGE_LITERAL", start_expr, end_expr)
        else:
            # Cualquier otra expresión iterable (lista, llamada, etc.)
            expr = start_expr

        # Cerrar paréntesis si los hubo
        if has_paren:
            if i >= len(tokens) or tokens[i][0] != "RPAREN":
                raise OrionSyntaxError("Se esperaba ')' al final del encabezado del for")
            i += 1

        # Saltar newlines o puntos y coma antes del cuerpo
        while i < len(tokens) and tokens[i][0] in ("NEWLINE", "SEMICOLON"):
            i += 1

        # Bloque del cuerpo
        if i >= len(tokens) or tokens[i][0] != "LBRACE":
            raise OrionSyntaxError("Se esperaba '{' al inicio del cuerpo del for'")
        body, j = parse_block(tokens, i)

        # Distinguir tipo de for
        if expr[0] == "RANGE_LITERAL":
            return ("FOR_RANGE", var_name, expr[1], expr[2], body), j
        else:
            return ("FOR_IN", var_name, expr, body), j

    # --- FN statement ---
    elif kind == "FN" or (kind == "IDENT" and value == "fn"):
        if i+1 >= len(tokens) or tokens[i+1][0] != "IDENT":
            raise OrionSyntaxError("Se esperaba un nombre de función después de 'fn'")
        fn_name = tokens[i+1][1]

        if i+2 >= len(tokens) or tokens[i+2][0] != "LPAREN":
            raise OrionSyntaxError("Se esperaba '(' después del nombre de función")
        params, j = parse_fn_params(tokens, i+2)

        # Saltar cualquier NEWLINE o SEMICOLON antes de la llave
        while j < len(tokens) and tokens[j][0] in ("NEWLINE", "SEMICOLON"):
            j += 1

        if j >= len(tokens) or tokens[j][0] != "LBRACE":
            raise OrionSyntaxError("Se esperaba '{' al inicio del cuerpo de la función")
        body, j = parse_block(tokens, j)

        return ("FN", fn_name, params, body), j

    # --- MATCH statement ---
    elif kind == "MATCH":
        expr, j = parse_expression(tokens, i+1)
        if j >= len(tokens) or tokens[j][0] != "LBRACE":
            raise OrionSyntaxError("Se esperaba '{' después de match")
        j += 1
        cases = []
        while j < len(tokens) and tokens[j][0] != "RBRACE":
            case_kind = tokens[j][0]
            if case_kind == "ELSE":
                if j+1 >= len(tokens) or tokens[j+1][0] != "COLON":
                    raise OrionSyntaxError("Se esperaba ':' después de else en match")
                if j+2 >= len(tokens) or tokens[j+2][0] != "LBRACE":
                    raise OrionSyntaxError("Se esperaba '{' en else de match")
                body, j2 = parse_block(tokens, j+2)
                cases.append(("else", body))
                j = j2
            else:
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
        return ("MATCH", expr, cases), j + 1

    # --- IDENT - Manejar asignaciones múltiples primero ---
    elif kind == "IDENT":
        # Asignación simple: IDENT ASSIGN expr
        if i+1 < len(tokens) and tokens[i+1][0] == "ASSIGN":
            var_name = tokens[i][1]
            expr_value, j = parse_expression(tokens, i+2)
            return ("ASSIGN", var_name, expr_value), j

        # Verificar si es una asignación múltiple: IDENT, IDENT, ... = expr
        if i+1 < len(tokens) and tokens[i+1][0] == "COMMA":
            # Es una asignación múltiple
            var_names = [tokens[i][1]]
            j = i + 1
            while j < len(tokens) and tokens[j][0] == "COMMA":
                j += 1
                if j < len(tokens) and tokens[j][0] == "IDENT":
                    var_names.append(tokens[j][1])
                    j += 1
                else:
                    raise OrionSyntaxError("Se esperaba un identificador después de la coma")
            
            if j < len(tokens) and tokens[j][0] == "ASSIGN":
                j += 1
                expr_value, j = parse_expression(tokens, j)
                return ("MULTI_ASSIGN", var_names, expr_value), j
            else:
                # No es asignación múltiple, manejar como expresión
                expr, i = parse_expression(tokens, i)
                return expr, i
        
        # No es asignación múltiple, verificar si es asignación simple o compleja
        else:
            # Intentar parsear la expresión completa del lado izquierdo
            saved_i = i
            try:
                left_expr, temp_i = parse_primary(tokens, i)
                
                # Buscar el operador de asignación
                if temp_i < len(tokens) and tokens[temp_i][0] == "ASSIGN":
                    right_expr, final_i = parse_expression(tokens, temp_i + 1)
                    
                    # Detectar tipo de asignación basado en la estructura del lado izquierdo
                    if isinstance(left_expr, tuple):
                        if left_expr[0] == "INDEX":
                            return ("INDEX_ASSIGN", left_expr[1], left_expr[2], right_expr), final_i
                        elif left_expr[0] == "ATTR_ACCESS":
                            return ("ATTR_ASSIGN", left_expr[1], left_expr[2], right_expr), final_i
                        elif left_expr[0] == "IDENT":
                            return ("ASSIGN", left_expr[1], right_expr), final_i
                        else:
                            # Asignación compleja (llamada, acceso seguro, etc.)
                            return ("COMPLEX_ASSIGN", left_expr, right_expr), final_i
                    else:
                        # Asignación simple de variable
                        return ("ASSIGN", left_expr, right_expr), final_i
                else:
                    # No hay asignación, es solo una expresión
                    return left_expr, temp_i
                    
            except OrionSyntaxError:
                # Si falla el parsing como expresión primaria, intentar como expresión completa
                try:
                    expr, i = parse_expression(tokens, saved_i)
                    return expr, i
                except OrionSyntaxError:
                    # Si todo falla, lanzar un error más descriptivo
                    current_token = tokens[saved_i] if saved_i < len(tokens) else ("EOF", "")
                    raise OrionSyntaxError(f"No se pudo parsear la declaración que comienza con '{current_token[1]}'")
    
    # --- DEFAULT: Manejar como expresión ---
    else:
        try:
            expr, i = parse_expression(tokens, i)
            return expr, i
        except OrionSyntaxError as e:
            current_token = tokens[i] if i < len(tokens) else ("EOF", "")
            raise OrionSyntaxError(f"Error parseando expresión que comienza con '{current_token[1]}': {str(e)}")
        
def parse_block(tokens, i):
    """Parsea un bloque de código con mejor manejo de declaraciones."""
    stmts = []
    if i >= len(tokens) or tokens[i][0] != "LBRACE":
        raise OrionSyntaxError("Se esperaba '{'")

    i += 1

    while i < len(tokens) and tokens[i][0] != "RBRACE":
        stmt, next_i = parse_statement(tokens, i)
        stmts.append(stmt)
        i = next_i  # Avanza correctamente el índice

    if i >= len(tokens):
        raise OrionSyntaxError("Se esperaba '}' pero se alcanzó el final del archivo.")
    
    return stmts, i + 1

def parse(tokens):
    """Función principal de parsing con mejor manejo de errores."""
    ast = []
    i = 0
    
    while i < len(tokens):
        try:
            stmt, i = parse_statement(tokens, i)
            ast.append(stmt)
        except OrionSyntaxError as e:
            # Proporcionar mejor información de error
            current_token = tokens[i] if i < len(tokens) else ("EOF", "")
            raise OrionSyntaxError(f"{str(e)} en línea cerca del token '{current_token[1]}'")

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
            # Coma opcional entre parámetros
            if i < len(tokens) and tokens[i][0] == "COMMA":
                i += 1
        else:
            raise OrionSyntaxError("Se esperaba un identificador de parámetro")
            
    if i >= len(tokens) or tokens[i][0] != "RPAREN":
        raise OrionSyntaxError("Se esperaba ')' al final de los parámetros")
    return params, i + 1

def parse_lambda(tokens, i):
    """Parsea expresiones lambda: param => expr o (param1, param2) => expr"""
    params = []
    
    # Caso 1: Un solo parámetro sin paréntesis
    if tokens[i][0] == "IDENT":
        params.append(tokens[i][1])
        i += 1
    # Caso 2: Múltiples parámetros con paréntesis
    elif tokens[i][0] == "LPAREN":
        i += 1
        while i < len(tokens) and tokens[i][0] != "RPAREN":
            if tokens[i][0] == "IDENT":
                params.append(tokens[i][1])
                i += 1
                if i < len(tokens) and tokens[i][0] == "COMMA":
                    i += 1
            else:
                raise OrionSyntaxError("Se esperaba parámetro en lambda")
        if i >= len(tokens) or tokens[i][0] != "RPAREN":
            raise OrionSyntaxError("Se esperaba ')' en parámetros de lambda")
        i += 1
    else:
        raise OrionSyntaxError("Se esperaba parámetro o '(' en lambda")
    
    # Verificar ARROW
    if i >= len(tokens) or tokens[i][0] != "ARROW":
        raise OrionSyntaxError("Se esperaba '=>' en lambda")
    i += 1
    
    # Parsear cuerpo de la lambda
    body, i = parse_or(tokens, i)  # Usar parse_or para evitar recursión infinita
    
    return ("LAMBDA", params, body), i

