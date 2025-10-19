import re
from core.errors import OrionSyntaxError
from core.types import OrionBool


def lex(code):
    tokens = []

    token_specification = [
        # --- Comentarios ---
        ("INVALID_COMMENT", r"(//.*|---.*)"),
        ("COMMENT",  r"--[^-].*"),

        # --- Números y cadenas ---
        ("NUMBER",   r"\d+(\.\d+)?"),
        ("STRING",   r'"[^"]*"'),

        # --- Palabras clave ---
        ("PRINT",    r"\bshow\b"),
        # ("CODE",     r"\bcode\b"),
        ("RETURN",   r"\breturn\b"),
        ("FN",       r"\bfn\b"),
        ("MATCH",    r"\bmatch\b"),
        ("USE",      r"\buse\b"),
        ("FOR",      r"\bfor\b"),
        ("IN",       r"\bin\b"),
        ("IF",       r"\bif\b"),
        ("ELSE",     r"\belse\b"),
        ("TRUE",     r"\btrue\b"),
        ("FALSE",    r"\bfalse\b"),
        ("YES",      r"\byes\b"),
        ("NO",       r"\bno\b"),
        ("TYPE",     r"\b(int|float|bool|string)\b"),

        # --- Identificadores (debe ir después de las palabras clave) ---
        ("IDENT",    r"[a-zA-Z_][a-zA-Z0-9_]*"),

        # --- Operadores y símbolos ---
        ("RANGE_EX", r"\.\.<"),
        ("RANGE",    r"\.\."),
        ("NULL_SAFE", r"\?\."),
        ("AND",      r"&&"),
        ("OR",       r"\|\|"),
        ("NOT",      r"!"),
        ("LPAREN",   r"\("),
        ("RPAREN",   r"\)"),
        ("LBRACE",   r"\{"),
        ("RBRACE",   r"\}"),
        ("COLON",    r":"),
        ("COMMA",    r","),
        ("THIN_ARROW", r"->"),
        ("ARROW",     r"=>"),
        ("COMPARE",  r"(==|!=|<=|>=|<|>)"), 
        ("ASSIGN",   r"="),
        ("OP",       r"[+\-*/]"),
        ("DOT",      r"\."),
        ("LBRACKET", r"\["),
        ("RBRACKET", r"\]"),

        # --- Espacios y errores ---
        ("NEWLINE",  r"\n"),
        ("SKIP",     r"[ \t]+"),
        ("MISMATCH", r"."),
    ]

    tok_regex = "|".join("(?P<%s>%s)" % pair for pair in token_specification)

    for mo in re.finditer(tok_regex, code):
        kind = mo.lastgroup
        value = mo.group()

        if kind == "INVALID_COMMENT":
            raise OrionSyntaxError(f"Comentario inválido: {value}")

        elif kind == "NUMBER":
            tokens.append((kind, float(value)) if '.' in value else (kind, int(value)))

        elif kind == "STRING":
            tokens.append((kind, value))

        elif kind in ("YES", "TRUE"):
            tokens.append(("BOOL", OrionBool(True)))
        elif kind in ("NO", "FALSE"):
            tokens.append(("BOOL", OrionBool(False)))

        elif kind in (
            "IDENT", "OP", "PRINT", "CODE", "FOR", "IF", "ELSE", "ASSIGN",
            "RANGE", "RANGE_EX", "COMPARE", "LBRACE", "RBRACE",
            "LPAREN", "RPAREN", "FN", "RETURN", "TYPE",
            "AND", "OR", "NOT", "MATCH", "USE",
            "ARROW", "THIN_ARROW", "IN", "NULL_SAFE",
            "COLON", "COMMA", "DOT",
            "LBRACKET", "RBRACKET"
        ):
            tokens.append((kind, value))

        elif kind in ("SKIP", "NEWLINE", "COMMENT"):
            continue

        elif kind == "MISMATCH":
            raise OrionSyntaxError(f"Token inesperado: {value}")

    return tokens
