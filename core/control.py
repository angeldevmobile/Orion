# core/control.py

"""
Funciones de control: match evaluation y helpers.
"""

"""
Funciones de control para Orion Language:
- switcher (tipo match futurista)
- if/else if/else en cadena
"""

def eval_match(expr_value, cases, evaluate_func, variables):
    """
    switcher: evalúa una expresión contra varios patrones.
    
    expr_value: valor ya evaluado de la expresión de match.
    cases: lista de (pattern, body, guard) donde:
        - pattern puede ser literal, variable o "default".
        - guard es una condición extra (o None).
    evaluate_func: función que evalúa cuerpos o expresiones.
    variables: entorno actual.
    """
    fallback = None

    for pattern, body, guard in cases:
        if pattern == "default":
            fallback = (body, guard)
            continue

        # Si el patrón coincide
        if expr_value == pattern:
            # Si tiene condición (guard), evaluarla
            if guard:
                guard_val = evaluate_func(guard, variables, inside_fn=True)
                if not guard_val:
                    continue
            return evaluate_func(body, variables, inside_fn=True)

    # Si hubo default, ejecutarlo
    if fallback:
        body, guard = fallback
        # incluso el default puede tener guard opcional
        if guard:
            guard_val = evaluate_func(guard, variables, inside_fn=True)
            if not guard_val:
                return None
        return evaluate_func(body, variables, inside_fn=True)

    return None


def eval_if_chain(conditions, evaluate_func, variables):
    """
    if/else if/else en Orion.
    
    conditions: lista de (cond_expr, body) donde cond_expr puede ser None (para else).
    evaluate_func: función para evaluar bodies.
    variables: entorno actual.
    """
    for cond_expr, body in conditions:
        if cond_expr is None:
            # else sin condición
            return evaluate_func(body, variables, inside_fn=True)

        cond_val = evaluate_func(cond_expr, variables, inside_fn=True)
        if cond_val:
            return evaluate_func(body, variables, inside_fn=True)

    return None
