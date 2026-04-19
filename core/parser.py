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

    kind, value = tokens[i][0], tokens[i][1]

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
        depth = 1
        start_i = i

        # Contar niveles de anidación hasta cerrar todos los '('
        while i < len(tokens) and depth > 0:
            tkind, _ = tokens[i]
            if tkind == "LPAREN":
                depth += 1
            elif tkind == "RPAREN":
                depth -= 1
            i += 1

        if depth != 0:
            raise OrionSyntaxError("Se esperaba ')'")

        # Extraer los tokens internos entre los paréntesis
        inner_tokens = tokens[start_i:i - 1]

        # Evaluar la expresión interna recursivamente
        expr, _ = parse_expression(inner_tokens, 0)

        return expr, i

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
        # Si lo que sigue NO es STRING o IDENT seguido de COLON, NO es un diccionario
        lookahead = tokens[i+1:i+3] if i+1 < len(tokens) else []
        is_dict = False
        
        if len(lookahead) >= 2:
            if lookahead[0][0] in ("STRING", "IDENT") and lookahead[1][0] == "COLON":
                is_dict = True
        elif len(lookahead) == 1 and lookahead[0][0] == "RBRACE":
            # Diccionario vacío {}
            is_dict = True
            
        if not is_dict:
            context_valid = False
            temp_i = max(0, i - 5)  # buscar en los últimos 5 tokens
            while temp_i < i:
                if (tokens[temp_i][0] == "IDENT" and 
                    tokens[temp_i][1] in ("attempt", "handle", "if", "while", "for", "fn")):
                    context_valid = True
                    break
                temp_i += 1
            
            if not context_valid:
                raise OrionSyntaxError("Token '{' encontrado en contexto de expresión")
        
        # Si sí es un diccionario, sigue como antes
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
        print("DEBUG: tokens[i-2:i+3]", tokens[max(0, i-2):i+3])
        raise OrionSyntaxError(f"Token inesperado en expresión primaria: '{kind}' ('{value}')")

def parse_unary(tokens, i):
    if i < len(tokens) and tokens[i][0] == "NOT":
        expr, j = parse_unary(tokens, i+1)
        return ("UNARY_OP", "!", expr), j
    
    # Agregar soporte para operadores unarios + y -
    elif i < len(tokens) and tokens[i][0] == "OP" and tokens[i][1] in ("+", "-"):
        op = tokens[i][1]
        expr, j = parse_unary(tokens, i+1)
        return ("UNARY_OP", op, expr), j
    
    # Verificar que hay tokens disponibles
    if i >= len(tokens):
        raise OrionSyntaxError("Fin inesperado de entrada en expresión unaria")
    
    # parse_primary ya maneja todos los errores internamente
    expr, j = parse_primary(tokens, i)
    return expr, j

def parse_expression_until(tokens, i, stop_tokens):
    """
    Parsea una expresión y se detiene si el siguiente token es de parada (por ejemplo, 'LBRACE').
    Devuelve (expr, next_i) donde next_i es el índice del token de parada.
    """
    # Si el siguiente token es de parada, no hay expresión (error de sintaxis)
    if i < len(tokens) and tokens[i][0] in stop_tokens:
        raise OrionSyntaxError(f"Se esperaba una expresión antes de '{tokens[i][1]}'")
    expr, next_i = parse_expression(tokens, i)
    return expr, next_i

def parse_term(tokens, i):
    left, i = parse_power(tokens, i)
    while i < len(tokens) and tokens[i][0] == "OP" and tokens[i][1] in ("*", "/", "%"):
        op = tokens[i][1]
        right, i = parse_power(tokens, i+1)
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
    """Parsea una declaración individual e inyecta el número de línea como último elemento."""
    line = tokens[i][2] if len(tokens[i]) > 2 else 0
    node, j = _parse_statement(tokens, i)
    if isinstance(node, tuple):
        node = node + (line,)
    return node, j


def _parse_statement(tokens, i):
    """Lógica interna de parsing de una declaración."""
    if i >= len(tokens):
        raise OrionSyntaxError("Se esperaba una declaración")

    kind = tokens[i][0]
    value = tokens[i][1] if len(tokens[i]) > 1 else None

    # --- CONST statement: const x = valor (inmutable, tipo inferido) ---
    if kind == "CONST":
        if i+1 >= len(tokens) or tokens[i+1][0] != "IDENT":
            raise OrionSyntaxError("Se esperaba un nombre después de 'const'")
        var_name = tokens[i+1][1]
        if i+2 >= len(tokens) or tokens[i+2][0] != "ASSIGN":
            raise OrionSyntaxError("Se esperaba '=' después del nombre en const")
        expr_value, j = parse_expression(tokens, i+3)
        return ("CONST", var_name, expr_value), j

    # --- VAR (compatibilidad legacy) → trata como asignación normal ---
    if kind == "IDENT" and value == "var":
        if i+1 >= len(tokens) or tokens[i+1][0] != "IDENT":
            raise OrionSyntaxError("Se esperaba un nombre de variable después de 'var'")
        var_name = tokens[i+1][1]
        if i+2 >= len(tokens) or tokens[i+2][0] != "ASSIGN":
            raise OrionSyntaxError("Se esperaba '=' después del nombre de variable")
        expr_value, j = parse_expression(tokens, i+3)
        return ("ASSIGN", var_name, expr_value), j
    
    # --- ATTEMPT/HANDLE statement ---
    elif kind == "ATTEMPT":  # Cambio aquí: usar ATTEMPT directamente
        i += 1  # consumir 'attempt'
        
        # Saltar newlines antes del bloque
        while i < len(tokens) and tokens[i][0] in ("NEWLINE", "SEMICOLON"):
            i += 1
            
        if i >= len(tokens) or tokens[i][0] != "LBRACE":
            raise OrionSyntaxError("Se esperaba '{' después de 'attempt'")
            
        # Parsear el bloque attempt
        body_attempt, i = parse_block(tokens, i)
        
        # Verificar si hay handle
        handler = None
        if i < len(tokens) and tokens[i][0] == "HANDLE":  # Cambio aquí también
            i += 1  # consumir 'handle'
            
            # Parsear nombre de variable de error (opcional)
            err_name = "_error"  # nombre por defecto
            if i < len(tokens) and tokens[i][0] == "IDENT":
                err_name = tokens[i][1]
                i += 1
            
            # Saltar newlines antes del bloque
            while i < len(tokens) and tokens[i][0] in ("NEWLINE", "SEMICOLON"):
                i += 1
                
            if i >= len(tokens) or tokens[i][0] != "LBRACE":
                raise OrionSyntaxError("Se esperaba '{' después de 'handle'")
                
            # Parsear el bloque handle
            body_handle, i = parse_block(tokens, i)
            handler = ("HANDLE", err_name, body_handle)
        
        return ("ATTEMPT", body_attempt, handler), i

    # --- USE statement (import eliminado, usar solo 'use') ---
    elif kind == "USE":
        if i+1 >= len(tokens) or tokens[i+1][0] not in ("STRING", "IDENT"):
            raise OrionSyntaxError("Se esperaba un módulo después de 'use'")
        module_path = tokens[i+1][1]
        return ("USE", module_path), i + 2

    # --- PRINT/SHOW statement ---
    elif kind == "PRINT":
        if i+1 < len(tokens) and tokens[i+1][0] == "LPAREN":
            args, kwargs, i = parse_call_args(tokens, i+1)
            return ("CALL", "show", args, kwargs), i
        else:
            args = []
            kwargs = {}
            i += 1
            expr, i = parse_expression(tokens, i)
            args.append(expr)
            while i < len(tokens):
                if tokens[i][0] == "COMMA":
                    i += 1
                    if (i + 1 < len(tokens)
                        and tokens[i][0] == "IDENT"
                        and tokens[i+1][0] == "ASSIGN"):
                        name = tokens[i][1]
                        i += 2
                        value, i = parse_expression(tokens, i)
                        kwargs[name] = value
                    else:
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

    # --- WHILE statement ---
    elif kind == "WHILE":
        condition, i = parse_expression_until(tokens, i + 1, {"LBRACE"})
        # Saltar NEWLINE o SEMICOLON antes de la llave
        while i < len(tokens) and tokens[i][0] in ("NEWLINE", "SEMICOLON"):
            i += 1
        if i >= len(tokens) or tokens[i][0] != "LBRACE":
            raise OrionSyntaxError("Se esperaba '{' después de la condición del while")
        body, i = parse_block(tokens, i)
        return ("WHILE", condition, body), i

    elif kind == "IF":
        condition, i = parse_expression_until(tokens, i + 1, {"LBRACE"})
        while i < len(tokens) and tokens[i][0] in ("NEWLINE", "SEMICOLON"):
            i += 1
        if i >= len(tokens) or tokens[i][0] != "LBRACE":
            print("DEBUG IF: tokens[i-3:i+3]", tokens[max(0, i-3):i+3])
            raise OrionSyntaxError("Se esperaba '{' después de la condición del if")
        body_true, i = parse_block(tokens, i)

        # Manejar múltiples "or if" (reemplaza elsif)
        elsif_parts = []
        while (i + 1 < len(tokens)
               and tokens[i][0] == "IDENT" and tokens[i][1] == "or"
               and tokens[i + 1][0] == "IF"):
            i += 2  # consumir "or" + "if"
            elsif_condition, i = parse_expression_until(tokens, i, {"LBRACE"})
            while i < len(tokens) and tokens[i][0] in ("NEWLINE", "SEMICOLON"):
                i += 1
            if i >= len(tokens) or tokens[i][0] != "LBRACE":
                raise OrionSyntaxError("Se esperaba '{' después de 'or if'")
            elsif_body, i = parse_block(tokens, i)
            elsif_parts.append((elsif_condition, elsif_body))

        # Manejar else final
        body_false = []
        if i < len(tokens) and tokens[i][0] == "ELSE":
            i += 1
            if i < len(tokens) and tokens[i][0] == "LBRACE":
                body_false, i = parse_block(tokens, i)
            else:
                raise OrionSyntaxError("Se esperaba '{' después de 'else'")

        # Retornar estructura IF extendida con elsif
        if elsif_parts:
            return ("IF_ELSIF", condition, body_true, elsif_parts, body_false), i
        else:
            return ("IF", condition, body_true, body_false), i

    # --- FOR statement ---
    elif kind == "FOR":
        i += 1

        # Permitir opcionalmente paréntesis después de for
        has_paren = False
        if i < len(tokens) and tokens[i][0] == "LPAREN":
            has_paren = True
            i += 1

        var_names = []
        if i >= len(tokens) or tokens[i][0] != "IDENT":
            raise OrionSyntaxError("Se esperaba un identificador después de for")
        var_names.append(tokens[i][1])
        i += 1
        while i < len(tokens) and tokens[i][0] == "COMMA":
            i += 1
            if i < len(tokens) and tokens[i][0] == "IDENT":
                var_names.append(tokens[i][1])
                i += 1
            else:
                raise OrionSyntaxError("Se esperaba un identificador después de la coma en el encabezado del for")

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

        # CORRECCIÓN: Manejar múltiples variables correctamente
        if isinstance(expr, tuple) and expr[0] == "RANGE_LITERAL":
            # Para rangos, usar solo la primera variable (rangos no soportan múltiples variables)
            if len(var_names) > 1:
                raise OrionSyntaxError("Los bucles de rango no soportan múltiples variables")
            return ("FOR_RANGE", var_names[0], expr[1], expr[2], body), j
        else:
            # Para iterables, pasar la lista de variables si hay más de una
            var_spec = var_names if len(var_names) > 1 else var_names[0]
            return ("FOR_IN", var_spec, expr, body), j

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

    # --- LPAREN: Detectar if implícito ---
    elif kind == "LPAREN":
        # Esto podría ser un if sin la palabra clave 'if'
        # Buscar el patrón: (expresión) { bloque }
        condition, i = parse_expression(tokens, i)
        
        if i < len(tokens) and tokens[i][0] == "LBRACE":
            # Es un if implícito: (condición) { bloque }
            body_true, i = parse_block(tokens, i)
            body_false = []
            
            # Verificar si hay else
            if i < len(tokens) and tokens[i][0] == "ELSE":
                i += 1
                if i < len(tokens) and tokens[i][0] == "LBRACE":
                    body_false, i = parse_block(tokens, i)
                else:
                    raise OrionSyntaxError("Se esperaba '{' después de 'else'")
            
            return ("IF", condition, body_true, body_false), i
        else:
            # No es un if implícito, es solo una expresión
            return condition, i

    # --- IDENT - Manejo mejorado de asignaciones ---
    elif kind == "IDENT":
        # PASO 1: Verificar asignación múltiple
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
                return ("EXPR", expr), i
        
        # PASO 2: Verificar asignación simple (IDENT = ...)
        elif i+1 < len(tokens) and tokens[i+1][0] == "ASSIGN":
            var_name = tokens[i][1]
            expr_value, j = parse_expression(tokens, i+2)
            return ("ASSIGN", var_name, expr_value), j
        
        elif i+1 < len(tokens) and tokens[i+1][0] == "OP_ASSIGN":
            var_name = tokens[i][1]
            op_assign = tokens[i+1][1]  # e.g. '+=', '-=', '*=', '/='
            op = op_assign[0]           # '+', '-', '*', '/'
            expr_value, j = parse_expression(tokens, i+2)
            # Expande: a += b  ->  a = a + b
            left = ("IDENT", var_name)
            right = expr_value
            bin_expr = ("BINARY_OP", op, left, right)
            return ("ASSIGN", var_name, bin_expr), j
        
        # PASO 3: NO es asignación - parsear como expresión completa
        else:
            # CAMBIO CRÍTICO: Siempre usar parse_expression para toda la línea
            expr, final_i = parse_expression(tokens, i)
            return ("EXPR", expr), final_i
    # --- DEFAULT: Manejar como expresión solo si no es un token de bloque ---
    else:
        if kind == "LBRACE":
            raise OrionSyntaxError("Bloque de código encontrado fuera de contexto")
        elif kind == "RBRACE":
            raise OrionSyntaxError("'}' encontrado sin '{' correspondiente")
        
        try:
            expr, next_i = parse_expression(tokens, i)
            return ("EXPR", expr), next_i 
        except OrionSyntaxError as e:
            current_token = tokens[i] if i < len(tokens) else ("EOF", "")
            raise OrionSyntaxError(f"Error parseando expresión que comienza con '{current_token[1]}': {str(e)}")

def parse_expression_for_assignment(tokens, i):
    """
    Parsea una expresión que puede ser el lado izquierdo de una asignación.
    Se detiene antes de operadores de comparación, lógicos, etc., pero permite
    acceso a índices y atributos.
    """
    return parse_primary(tokens, i)

def parse_power(tokens, i):
    left, i = parse_unary(tokens, i)
    while i + 1 < len(tokens) and tokens[i][0] == "OP" and tokens[i][1] == "*" and tokens[i+1][0] == "OP" and tokens[i+1][1] == "*":
        # Detectar '**'
        i += 2
        right, i = parse_unary(tokens, i)
        left = ("BINARY_OP", "**", left, right)
    return left, i

def parse_block(tokens, i):
    """Parsea un bloque de código con mejor manejo de declaraciones y tolerancia a cierres mal escritos."""
    stmts = []
    if i >= len(tokens) or tokens[i][0] != "LBRACE":
        raise OrionSyntaxError("Se esperaba '{'")

    i += 1

    while i < len(tokens) and tokens[i][0] not in ("RBRACE", "RBRACEE"):
        # Saltar NEWLINE y SEMICOLON dentro del bloque
        while i < len(tokens) and tokens[i][0] in ("NEWLINE", "SEMICOLON"):
            i += 1
        # Saltar tokens de cierre de bloque mal escritos
        if i < len(tokens) and tokens[i][0] in ("RBRACEE", "RBRACES", "RBRACE2"):
            i += 1
            continue
        if i >= len(tokens):
            break
        stmt, next_i = parse_statement(tokens, i)
        stmts.append(stmt)
        if next_i == i:
            raise OrionSyntaxError(f"El parser no avanzó en el índice en parse_block cerca de '{tokens[i]}'")
        i = next_i

    if i >= len(tokens):
        raise OrionSyntaxError("Se esperaba '}' pero se alcanzó el final del archivo.")
    # Saltar el token de cierre de bloque (RBRACE o variantes)
    i += 1
    return stmts, i

def parse(tokens):
    """Función principal de parsing con mejor manejo de errores y avance seguro."""
    ast = []
    i = 0
    
    function_nodes = []
    statement_nodes = []
    
    while i < len(tokens):
        try:
            stmt, next_i = parse_statement(tokens, i)
            if next_i == i:
                raise OrionSyntaxError(f"El parser no avanzó en el índice en parse() cerca de '{tokens[i]}'")
            
            if isinstance(stmt, tuple) and stmt[0] == "FN":
                function_nodes.append(stmt)
            else:
                statement_nodes.append(stmt)
            
            i = next_i
            
            if i >= len(tokens):
                break
            
            while i < len(tokens) and tokens[i][0] in ("NEWLINE", "SEMICOLON"):
                i += 1
        except OrionSyntaxError as e:
            current_token = tokens[i] if i < len(tokens) else ("EOF", "")
            print(f"[ORION PARSER WARNING] {str(e)} en línea cerca del token '{current_token[1]}'")
            sync_tokens = {"NEWLINE", "SEMICOLON", "RBRACE", "RBRACEE", "RBRACES", "RBRACE2"}
            while i < len(tokens) and tokens[i][0] not in sync_tokens:
                i += 1
            i += 1
    
    ast = function_nodes + statement_nodes
    
    print("AST generado:", ast)
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

