from core.errors import OrionSyntaxError


def parse_call_args(tokens, i):
    """Parse function call arguments, supporting both positional and named arguments."""
    # Check if we have a left parenthesis - if not, this isn't a function call
    if i >= len(tokens) or tokens[i][0] != "LPAREN":
        raise OrionSyntaxError("Se esperaba '(' en llamada de función")
    
    i += 1  # Skip '('
    args = []
    
    if i >= len(tokens):
        raise OrionSyntaxError("Fin inesperado de entrada en argumentos de función")
    
    while i < len(tokens) and tokens[i][0] != "RPAREN":
        # Check if this is a named argument (name=value)
        if (i + 2 < len(tokens) and 
            tokens[i][0] == "IDENT" and 
            tokens[i + 1][0] == "ASSIGN"):
            
            # Named argument
            name = tokens[i][1]
            i += 2  # Skip name and =
            value, i = parse_expression(tokens, i)
            args.append(("NAMED_ARG", name, value))
        else:
            # Positional argument
            arg, i = parse_expression(tokens, i)
            args.append(arg)
        
        # Handle comma
        if i < len(tokens) and tokens[i][0] == "COMMA":
            i += 1
        elif i < len(tokens) and tokens[i][0] != "RPAREN":
            raise OrionSyntaxError(f"Se esperaba ',' o ')' en argumentos de función")
    
    if i >= len(tokens) or tokens[i][0] != "RPAREN":
        raise OrionSyntaxError("Se esperaba ')' después de argumentos de función")
    
    i += 1  # Skip ')'
    return args, i


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
    elif kind == "IDENT":
        expr = ("IDENT", value)
        i += 1
        
        # Handle method calls, attribute access, indexing, and function calls
        while i < len(tokens):
            if tokens[i][0] == "DOT":
                i += 1
                if i >= len(tokens) or tokens[i][0] != "IDENT":
                    raise OrionSyntaxError("Se esperaba un identificador después de '.'")
                attr_name = tokens[i][1]
                i += 1
                
                # Check if it's a method call
                if i < len(tokens) and tokens[i][0] == "LPAREN":
                    i += 1
                    args, i = parse_call_args(tokens, i-1)  # Back up to include LPAREN
                    expr = ("CALL_METHOD", attr_name, expr, args)
                else:
                    expr = ("ATTR_ACCESS", expr, attr_name)
            
            elif tokens[i][0] == "LBRACKET":
                i += 1
                
                # Handle slicing notation
                if i < len(tokens) and tokens[i][0] == "COLON":
                    # Case: [:end] - slice from beginning
                    i += 1  # skip ':'
                    if i < len(tokens) and tokens[i][0] != "RBRACKET":
                        end_index, i = parse_expression(tokens, i)
                        slice_expr = ("SLICE", None, end_index, None)  # [start:end:step]
                    else:
                        slice_expr = ("SLICE", None, None, None)  # [:]
                elif i < len(tokens) and tokens[i][0] != "RBRACKET":
                    # Parse first expression (could be index or start of slice)
                    first_expr, i = parse_expression(tokens, i)
                    
                    if i < len(tokens) and tokens[i][0] == "COLON":
                        # This is a slice: [start:end] or [start:]
                        i += 1  # skip ':'
                        if i < len(tokens) and tokens[i][0] != "RBRACKET":
                            end_expr, i = parse_expression(tokens, i)
                            slice_expr = ("SLICE", first_expr, end_expr, None)
                        else:
                            slice_expr = ("SLICE", first_expr, None, None)  # [start:]
                    else:
                        # This is a simple index
                        slice_expr = first_expr
                else:
                    # Empty brackets []
                    slice_expr = ("SLICE", None, None, None)
                
                if i >= len(tokens) or tokens[i][0] != "RBRACKET":
                    raise OrionSyntaxError("Se esperaba ']'")
                i += 1
                
                if isinstance(slice_expr, tuple) and slice_expr[0] == "SLICE":
                    expr = ("SLICE_ACCESS", expr, slice_expr)
                else:
                    expr = ("INDEX", expr, slice_expr)
            
            elif tokens[i][0] == "LPAREN":
                # Function call - use the existing function call parsing
                args, i = parse_call_args(tokens, i)
                expr = ("CALL", value, args)
            
            elif tokens[i][0] == "SAFE_ACCESS":
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
        # List literal
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
        # Dictionary literal
        i += 1
        items = []
        while i < len(tokens) and tokens[i][0] != "RBRACE":
            # Parse key
            if tokens[i][0] not in ["STRING", "IDENT"]:
                raise OrionSyntaxError("Se esperaba clave de diccionario")
            key = tokens[i][1]
            if tokens[i][0] == "STRING":
                key = key.strip('"')
            i += 1
            
            # Expect colon
            if i >= len(tokens) or tokens[i][0] != "COLON":
                raise OrionSyntaxError("Se esperaba ':' después de clave")
            i += 1
            
            # Parse value
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
    return expr, j


def parse_statement(tokens, i):
    """Parsea una declaración individual con mejor manejo de errores."""
    if i >= len(tokens):
        raise OrionSyntaxError("Se esperaba una declaración")
        
    kind = tokens[i][0]
    value = tokens[i][1] if len(tokens[i]) > 1 else None

    # --- USE statement ---
    if kind == "USE":
        if i+1 >= len(tokens) or tokens[i+1][0] not in ("STRING", "IDENT"):
            raise OrionSyntaxError("Se esperaba una cadena o identificador después de 'use'")
        module_path = tokens[i+1][1]
        return ("USE", module_path), i + 2

    # --- PRINT/SHOW statement ---
    elif kind == "PRINT":
        if i+1 < len(tokens) and tokens[i+1][0] == "LPAREN":
            # Sintaxis tradicional: show(expr)
            args, i = parse_call_args(tokens, i+1)
            return ("CALL", "show", args), i
        else:
            # Sintaxis sin paréntesis: show expr
            expr, i = parse_expression(tokens, i+1)
            return ("CALL", "show", [expr]), i

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
        if i+1 >= len(tokens) or tokens[i+1][0] != "IDENT":
            raise OrionSyntaxError("Se esperaba un identificador después de 'for'")
        var_name = tokens[i+1][1]

        if i+2 >= len(tokens) or tokens[i+2][0] != "IN":
            raise OrionSyntaxError("Se esperaba 'in' en el bucle for")

        expr, j = parse_expression(tokens, i+3)

        if j < len(tokens) and tokens[j][0] in ("RANGE", "RANGE_EX"):
            range_type = tokens[j][0]
            j += 1
            end, j = parse_expression(tokens, j)
            body, j = parse_block(tokens, j)
            return ("FOR_RANGE", var_name, expr, end, body, range_type), j
        else:
            body, j = parse_block(tokens, j)
            return ("FOR_IN", var_name, expr, body), j

    # --- FN statement ---
    elif kind == "FN" or (kind == "IDENT" and value == "fn"):
        if i+1 >= len(tokens) or tokens[i+1][0] != "IDENT":
            raise OrionSyntaxError("Se esperaba un nombre de función después de 'fn'")
        fn_name = tokens[i+1][1]

        if i+2 >= len(tokens) or tokens[i+2][0] != "LPAREN":
            raise OrionSyntaxError("Se esperaba '(' después del nombre de función")
        params, j = parse_fn_params(tokens, i+2)

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

    # --- IDENT - Asignación o expresión ---
    elif kind == "IDENT":
        # ✅ MEJORADO: Mejor manejo de asignaciones complejas
        saved_i = i
        try:
            # Parsear la expresión del lado izquierdo
            left_expr, temp_i = parse_primary(tokens, i)
            
            # Verificar si hay asignación
            if temp_i < len(tokens) and tokens[temp_i][0] == "ASSIGN":
                right_expr, i = parse_expression(tokens, temp_i + 1)
                
                # Determinar el tipo de asignación
                if left_expr[0] == "INDEX":
                    return ("INDEX_ASSIGN", left_expr[1], left_expr[2], right_expr), i
                elif left_expr[0] == "ATTR_ACCESS":
                    return ("ATTR_ASSIGN", left_expr[1], left_expr[2], right_expr), i
                elif left_expr[0] == "IDENT":
                    return ("ASSIGN", left_expr[1], right_expr), i
                else:
                    return ("COMPLEX_ASSIGN", left_expr, right_expr), i
            else:
                # No es asignación, verificar asignación múltiple
                i = saved_i
                if i+1 < len(tokens) and tokens[i+1][0] == "COMMA":
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
                        raise OrionSyntaxError("Se esperaba '=' en asignación múltiple")
                
                # Es una expresión (probablemente una llamada de función)
                expr, i = parse_primary(tokens, saved_i)
                return expr, i
                
        except OrionSyntaxError:
            # Fallback: tratarlo como expresión
            expr, i = parse_expression(tokens, saved_i)
            return expr, i

    else:
        # Tratar como expresión
        expr, i = parse_expression(tokens, i)
        return expr, i


def parse_block(tokens, i):
    """Parsea un bloque de código con mejor manejo de declaraciones."""
    stmts = []
    if i >= len(tokens) or tokens[i][0] != "LBRACE":
        raise OrionSyntaxError("Se esperaba '{'")

    i += 1

    while i < len(tokens) and tokens[i][0] != "RBRACE":
        # ✅ MEJORADO: Usar parse_statement para mejor consistencia
        stmt, i = parse_statement(tokens, i)
        stmts.append(stmt)

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

