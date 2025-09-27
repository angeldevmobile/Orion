# core/control.py

"""
Funciones de control: match evaluation y helpers.
"""

def eval_match(expr_value, cases, evaluate_func, variables):
    """
    expr_value: valor evaluado de la expresión de match
    cases: lista de (pattern, body) donde pattern puede ser a literal or "else"
    evaluate_func: función para evaluar bodies (por ejemplo evaluate(body, vars, inside_fn=True))
    variables: entorno actual (se pasa a evaluate_func)
    Retorna el resultado de ejecutar el body correspondiente o None.
    """
    for pattern, body in cases:
        if pattern == "else":
            # guardar para ejecutar si no hubo match
            else_body = body
            continue
        # patrón literal comparado por igualdad
        # pattern puede ser una expresion AST: evaluarla primero contra variables
        pat_val = pattern
        # Si pattern es un AST node tuyo, lo esperable es que lo pases ya evaluado desde parser
        if pat_val == expr_value:
            return evaluate_func(body, variables, inside_fn=True)
    # si llegamos y hay else
    if 'else_body' in locals():
        return evaluate_func(else_body, variables, inside_fn=True)
    return None
