# core/typechecker.py
"""
Orion Static Type Checker — Fase 5B

Recorre el AST producido por el parser y valida:
  - TYPED_ASSIGN: el tipo inferido del valor coincide con la anotación
  - Llamadas a funciones: los tipos de los argumentos coinciden con param_types
  - RETURN dentro de funciones tipadas: el tipo retornado coincide con return_type

No bloquea la ejecución por sí solo — lanza OrionTypeError en el primer error
o acumula una lista de TypeIssue para el modo `--types` de `orion check`.
"""

from __future__ import annotations
from dataclasses import dataclass, field
from typing import Optional


# ───────────────────────── Tipos de resultado ─────────────────────────────────

@dataclass
class TypeIssue:
    message: str
    kind: str = "error"   # "error" | "warning"
    line: int = 0

    def __str__(self):
        tag = "✖ error" if self.kind == "error" else "⚠ warning"
        prefix = f"Línea {self.line}: " if self.line > 0 else ""
        return f"  [{tag}] {prefix}{self.message}"


# ───────────────────────── Mapa de tipos Orion ────────────────────────────────

# Alias canónicos: cualquier nombre → nombre normalizado
_ALIASES: dict[str, str] = {
    "str":     "string",
    "integer": "int",
    "boolean": "bool",
    "bool":    "bool",
    "num":     "number",
    "number":  "number",
    "float":   "float",
    "int":     "int",
    "string":  "string",
    "list":    "list",
    "dict":    "dict",
    "any":     "any",
    "void":    "void",
}

# Los tipos que son sub-tipo de "number"
_NUMBER_SUBTYPES = {"int", "float", "number"}


def _node_line(node) -> int:
    """Extrae el número de línea del último elemento de un nodo AST, si existe."""
    if isinstance(node, tuple) and node and isinstance(node[-1], int):
        return node[-1]
    return 0


def _normalize(t: Optional[str]) -> Optional[str]:
    if t is None:
        return None
    return _ALIASES.get(t, t)


def _types_compatible(declared: str, actual: str) -> bool:
    """True si `actual` satisface la restricción `declared`."""
    d = _normalize(declared)
    a = _normalize(actual)
    if d in (None, "any", "void"):
        return True
    if a in (None, "any"):
        return True          # valor desconocido → no reportar
    if d == a:
        return True
    if d == "number" and a in _NUMBER_SUBTYPES:
        return True
    if d == "float" and a == "int":
        return True          # int es asignable a float
    return False


# ───────────────────────── Inferencia de tipos sobre el AST ───────────────────

def _infer_type(expr, scope: Optional[dict] = None) -> Optional[str]:
    """
    Inferencia de tipo a partir de un nodo AST.
    Retorna el nombre de tipo Orion o None si es desconocido.
    scope: diccionario nombre→tipo acumulado por el TypeChecker.
    """
    if expr is None:
        return None

    # Literales primitivos Python (como aparecen en el AST de Orion)
    if isinstance(expr, bool):
        return "bool"
    if isinstance(expr, int):
        return "int"
    if isinstance(expr, float):
        return "float"
    if isinstance(expr, str):
        # Cadenas con comillas son literales string; sin comillas son identificadores
        if expr.startswith('"') or expr.startswith("'"):
            return "string"
        return None  # podría ser un identificador resuelto a string

    if not isinstance(expr, tuple):
        return None

    tag = expr[0]

    if tag == "NUMBER":
        val = expr[1]
        return "float" if isinstance(val, float) else "int"

    if tag == "STRING":
        return "string"

    if tag in ("BOOL", "YES", "NO"):
        return "bool"

    if tag == "NULL":
        return "any"

    if tag == "LIST":
        return "list"

    if tag == "DICT":
        return "dict"

    if tag == "IDENT":
        if scope:
            return scope.get(expr[1])   # tipo conocido del scope
        return None

    if tag == "BINARY_OP":
        _, op, left, right = expr[0], expr[1], expr[2], expr[3]
        lt = _infer_type(left, scope)
        rt = _infer_type(right, scope)
        if op in ("+", "-", "*", "/", "%", "**"):
            if lt == "float" or rt == "float":
                return "float"
            if lt == "int" and rt == "int":
                return "int"
            if lt == "string" and op == "+":
                return "string"
            return None
        if op in ("<", ">", "<=", ">=", "==", "!=", "and", "or", "not"):
            return "bool"
        return None

    # Alias legacy
    if tag == "BINOP":
        lt = _infer_type(expr[2], scope)
        rt = _infer_type(expr[3], scope)
        op = expr[1]
        if op in ("+", "-", "*", "/", "%", "**"):
            if lt == "float" or rt == "float":
                return "float"
            if lt == "int" and rt == "int":
                return "int"
            return None
        if op in ("<", ">", "<=", ">=", "==", "!=", "and", "or"):
            return "bool"
        return None

    if tag in ("UNOP", "UNARY_OP"):
        op = expr[1]
        if op == "not":
            return "bool"
        return _infer_type(expr[2], scope)

    if tag == "INDEX":
        return None

    if tag == "LIST":
        return "list"

    if tag == "DICT":
        return "dict"

    if tag == "LAMBDA":
        return "fn"

    if tag == "CALL":
        return None   # sin análisis interprocedural

    return None


# ───────────────────────── Type checker principal ─────────────────────────────

class TypeChecker:
    def __init__(self, strict: bool = False):
        """
        strict=False → acumula errores y los devuelve
        strict=True  → lanza OrionTypeError al primer error
        """
        self.strict = strict
        self.issues: list[TypeIssue] = []
        # Tabla de firmas de funciones: name → {param_types, return_type}
        self._fn_sigs: dict[str, dict] = {}
        # Stack de return_types para funciones anidadas
        self._return_stack: list[Optional[str]] = []
        # Scope de variables: nombre → tipo inferido
        self._scope: dict[str, str] = {}

    # ── API pública ────────────────────────────────────────────────────────────

    def check(self, ast: list) -> list[TypeIssue]:
        """Analiza un programa completo; retorna la lista de issues."""
        self.issues = []
        self._fn_sigs = {}
        self._return_stack = []
        self._scope = {}

        # Primer pase: recolectar firmas de todas las funciones globales
        self._collect_fn_signatures(ast)
        # Segundo pase: verificar el programa
        self._check_stmts(ast, scope_return_type=None)
        return self.issues

    # ── Recolección de firmas ──────────────────────────────────────────────────

    def _collect_fn_signatures(self, ast: list):
        for node in ast:
            if not isinstance(node, tuple):
                continue
            tag = node[0]
            if tag in ("FN", "ASYNC_FN"):
                fn_name = node[1]
                # node = ("FN", name, params, body, return_type, param_types [, line])
                return_type = node[4] if len(node) > 4 and not isinstance(node[4], int) else None
                param_types = node[5] if len(node) > 5 and isinstance(node[5], dict) else {}
                self._fn_sigs[fn_name] = {
                    "return_type": return_type,
                    "param_types": param_types or {},
                }

    # ── Verificación de statements ─────────────────────────────────────────────

    def _check_stmts(self, stmts: list, scope_return_type: Optional[str]):
        for node in stmts:
            self._check_stmt(node, scope_return_type)

    def _check_stmt(self, node, scope_return_type: Optional[str]):
        if not isinstance(node, tuple):
            return
        tag = node[0]

        # ── TYPED_ASSIGN: nombre: tipo = expr [, line] ──
        if tag == "TYPED_ASSIGN":
            # node = ("TYPED_ASSIGN", name, declared_type, expr [, line])
            name, declared, expr = node[1], node[2], node[3]
            line = _node_line(node)
            actual = _infer_type(expr, self._scope)
            if actual is not None and not _types_compatible(declared, actual):
                self._report(
                    f"'{name}: {declared}' — se asignó valor de tipo '{actual}'",
                    line
                )
            # Registrar el tipo declarado en el scope
            self._scope[name] = _normalize(declared)

        # ── ASSIGN ──
        elif tag == "ASSIGN":
            expr = node[2] if len(node) > 2 else None
            # Registrar tipo inferido en el scope para usos futuros
            inferred = _infer_type(expr, self._scope)
            if inferred and len(node) > 1:
                self._scope[node[1]] = inferred
            self._check_expr(expr, _node_line(node))

        # ── FN / ASYNC_FN definition ──
        elif tag in ("FN", "ASYNC_FN"):
            # node = ("FN", name, params, body, return_type, param_types [, line])
            fn_name     = node[1]
            body        = node[3]
            return_type = node[4] if len(node) > 4 else None
            param_types = node[5] if len(node) > 5 and isinstance(node[5], dict) else {}
            self._fn_sigs[fn_name] = {
                "return_type": return_type,
                "param_types": param_types or {},
            }
            # Guardar scope exterior y crear uno local con los parámetros tipados
            outer_scope = dict(self._scope)
            for pname, ptype in (param_types or {}).items():
                self._scope[pname] = _normalize(ptype)
            self._check_stmts(body, scope_return_type=return_type)
            # Restaurar scope exterior al salir de la función
            self._scope = outer_scope

        # ── RETURN ──
        elif tag == "RETURN":
            # node = ("RETURN", expr [, line])
            ret_expr = node[1] if len(node) > 1 else None
            line = _node_line(node)
            if scope_return_type and scope_return_type not in ("void", "any"):
                actual = _infer_type(ret_expr, self._scope)
                if actual is not None and not _types_compatible(scope_return_type, actual):
                    self._report(
                        f"RETURN: se esperaba '{scope_return_type}', "
                        f"pero la expresión es de tipo '{actual}'",
                        line
                    )

        # ── IF ──
        elif tag == "IF":
            # node: ('IF', cond, then_block [, elif_clauses [, else_block]] [, line])
            self._check_expr(node[1])
            if len(node) > 2 and isinstance(node[2], list):
                self._check_stmts(node[2], scope_return_type)
            if len(node) > 3 and isinstance(node[3], list):
                for elif_clause in node[3]:
                    if isinstance(elif_clause, (list, tuple)) and len(elif_clause) >= 2:
                        self._check_expr(elif_clause[0])
                        self._check_stmts(elif_clause[1], scope_return_type)
            if len(node) > 4 and isinstance(node[4], list):
                self._check_stmts(node[4], scope_return_type)

        # ── WHILE / FOR ──
        elif tag in ("WHILE", "FOR"):
            self._check_expr(node[1])
            # body is at different positions; find the first list child after index 1
            for child in node[2:]:
                if isinstance(child, list):
                    self._check_stmts(child, scope_return_type)
                    break

        # ── ATTEMPT ──
        elif tag == "ATTEMPT":
            if len(node) > 1 and isinstance(node[1], list):
                self._check_stmts(node[1], scope_return_type)
            if len(node) > 2 and isinstance(node[2], list):
                self._check_stmts(node[2], scope_return_type)

        # ── EXPR wrapper (parser envuelve expresiones sueltas como statement) ──
        elif tag == "EXPR":
            inner = node[1] if len(node) > 1 else None
            self._check_expr(inner, _node_line(node))

        # ── CALL statement (expr suelto) ──
        elif tag == "CALL":
            self._check_expr(node, _node_line(node))

        # ── Otros nodos: verificar recursivamente ──
        else:
            for child in node[1:]:
                if isinstance(child, tuple):
                    self._check_expr(child)
                elif isinstance(child, list):
                    self._check_stmts(child, scope_return_type)

    # ── Verificación de expresiones ───────────────────────────────────────────

    def _check_expr(self, expr, hint_line: int = 0):
        if not isinstance(expr, (tuple, list)):
            return
        if isinstance(expr, list):
            for item in expr:
                self._check_expr(item, hint_line)
            return

        tag = expr[0]

        if tag == "CALL":
            # node = ("CALL", fn_expr, args, kwargs [, line])
            fn_expr   = expr[1]
            args      = expr[2] if len(expr) > 2 else []
            call_line = _node_line(expr) or hint_line

            # Resolver el nombre de la función para buscar la firma
            fn_name = None
            if isinstance(fn_expr, tuple) and fn_expr[0] == "IDENT":
                fn_name = fn_expr[1]
            elif isinstance(fn_expr, str):
                fn_name = fn_expr

            if fn_name:
                sig = self._fn_sigs.get(fn_name)
                if sig:
                    param_types  = sig.get("param_types", {})
                    params_order = list(param_types.keys())
                    for idx, arg in enumerate(args):
                        if isinstance(arg, tuple) and arg[0] == "NAMED_ARG":
                            pname    = arg[1]
                            declared = param_types.get(pname)
                            arg_expr = arg[2] if len(arg) > 2 else None
                        else:
                            pname    = params_order[idx] if idx < len(params_order) else None
                            declared = param_types.get(pname) if pname else None
                            arg_expr = arg
                        if declared:
                            actual = _infer_type(arg_expr, self._scope)
                            if actual is not None and not _types_compatible(declared, actual):
                                self._report(
                                    f"Llamada a '{fn_name}': argumento #{idx+1} "
                                    f"('{pname}: {declared}') "
                                    f"— se esperaba '{declared}', se recibió '{actual}'",
                                    call_line
                                )

            # Verificar argumentos recursivamente
            for arg in (args if isinstance(args, list) else []):
                self._check_expr(arg)

        elif tag in ("BINARY_OP", "BINOP"):
            self._check_expr(expr[2])
            self._check_expr(expr[3])

        elif tag in ("UNOP", "UNARY_OP"):
            self._check_expr(expr[2])

        elif tag == "INDEX":
            self._check_expr(expr[1])
            self._check_expr(expr[2])

        elif tag == "LIST":
            for item in expr[1] if len(expr) > 1 else []:
                self._check_expr(item)

        elif tag == "DICT":
            for pair in expr[1] if len(expr) > 1 else []:
                if isinstance(pair, (list, tuple)) and len(pair) >= 2:
                    self._check_expr(pair[1])

        elif tag == "LAMBDA":
            body = expr[2] if len(expr) > 2 else []
            if isinstance(body, list):
                self._check_stmts(body, scope_return_type=None)
            else:
                self._check_expr(body)

    # ── Reporte de errores ─────────────────────────────────────────────────────

    def _report(self, message: str, line: int = 0):
        from core.errors import OrionTypeError
        issue = TypeIssue(message=message, kind="error", line=line)
        if self.strict:
            raise OrionTypeError(f"Línea {line}: {message}" if line > 0 else message)
        self.issues.append(issue)


# ───────────────────────── Función de conveniencia ────────────────────────────

def type_check(ast: list, strict: bool = False) -> list[TypeIssue]:
    """
    Ejecuta el type checker sobre el AST.

    strict=False → retorna lista de TypeIssue (sin lanzar excepciones)
    strict=True  → lanza OrionTypeError al primer problema encontrado
    """
    checker = TypeChecker(strict=strict)
    return checker.check(ast)
