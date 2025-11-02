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
        ("RETURN",   r"\breturn\b"),
        ("FN",       r"\bfn\b"),
        ("CLASS",    r"\bclass\b"),
        ("FOR",      r"\bfor\b"),
        ("IN",       r"\bin\b"),
        ("IF",       r"\bif\b"),
        ("ELSE",     r"\belse\b"),
        ("ELSIF",    r"\belsif\b"),
        ("WHILE",    r"\bwhile\b"),
        ("MATCH",    r"\bmatch\b"),
        ("USE",      r"\buse\b"),
        ("ATTEMPT",  r"\battempt\b"),
        ("HANDLE",   r"\bhandle\b"),
        ("YES",      r"\byes\b"),
        ("NO",       r"\bno\b"),
        ("TYPE",     r"\b(int|float|bool|string)\b"),

        # --- Programación Orientada a Objetos ---
        ("ON_CREATE", r"\bon_create\b"),
        ("ON_EVENT",  r"\bon_event\b"),
        ("ON_ERROR",  r"\bon_error\b"),
        ("ME",        r"\bme\b"),  # reemplazo de 'self'

        # --- Concurrencia / procesos asíncronos ---
        ("SPAWN",     r"\bspawn\b"),
        ("ASYNC",     r"\basync\b"),
        ("AWAIT",     r"\bawait\b"),
        ("CHANNEL",   r"\bchannel\b"),

        # --- Inteligencia Artificial / simbiótico ---
        ("THINK",     r"\bthink\b"),
        ("LEARN",     r"\blearn\b"),
        ("SENSE",     r"\bsense\b"),
        ("ADAPT",     r"\badapt\b"),
        ("EMBED",     r"\bembed\b"),

        # --- Comunicación / sistemas ---
        ("SYNC",      r"\bsync\b"),
        ("SEND",      r"\bsend\b"),
        ("RECEIVE",   r"\breceive\b"),
        ("PIPE",      r"\bpipe\b"),
        ("TASK",      r"\btask\b"),

        # --- Operadores y símbolos ---
        ("OP_ASSIGN",  r"(\+=|-=|\*=|/=)"),
        ("RANGE_EX",   r"\.\.<"),
        ("RANGE",      r"\.\."),
        ("NULL_SAFE",  r"\?\."),
        ("PIPE_OP",    r"\|>"),           # operador pipeline
        ("DOUBLE_COLON", r"::"),          # namespaces / estáticos
        ("ELLIPSIS",   r"\.\.\."),        # spread operator
        ("QUESTION",   r"\?"),
        ("EXP",        r"\*\*"),
        ("MOD",        r"%"),
        ("AND",        r"&&"),
        ("OR",         r"\|\|"),
        ("NOT",        r"!"),
        ("LPAREN",     r"\("),
        ("RPAREN",     r"\)"),
        ("LBRACE",     r"\{"),
        ("RBRACE",     r"\}"),
        ("COLON",      r":"),
        ("COMMA",      r","),
        ("THIN_ARROW", r"->"),
        ("ARROW",      r"=>"),
        ("COMPARE",    r"(==|!=|<=|>=|<|>)"),
        ("ASSIGN",     r"="),
        ("OP",         r"[+\-*/]"),
        ("DOT",        r"\."),
        ("LBRACKET",   r"\["),
        ("RBRACKET",   r"\]"),

        # --- Literales modernos ---
        ("NULL",       r"\bnull\b"),
        ("AUTO",       r"\bauto\b"),
        ("ANY",        r"\bany\b"),

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
            "IDENT", "OP", "PRINT", "CLASS", "FOR", "IF", "ELSE", "ELSIF", "WHILE", "ASSIGN",
            "RANGE", "RANGE_EX", "COMPARE", "LBRACE", "RBRACE", "LPAREN", "RPAREN",
            "FN", "RETURN", "TYPE", "AND", "OR", "NOT", "MATCH", "USE", "ARROW", "THIN_ARROW",
            "IN", "NULL_SAFE", "COLON", "COMMA", "DOT", "LBRACKET", "RBRACKET", "ATTEMPT",
            "HANDLE", "OP_ASSIGN", "PIPE_OP", "DOUBLE_COLON", "ELLIPSIS", "EXP", "MOD",
            "QUESTION", "SPAWN", "ASYNC", "AWAIT", "CHANNEL", "THINK", "LEARN", "SENSE",
            "ADAPT", "EMBED", "SYNC", "SEND", "RECEIVE", "PIPE", "TASK", "ON_CREATE",
            "ON_EVENT", "ON_ERROR", "ME", "NULL", "AUTO", "ANY"
        ):
            tokens.append((kind, value))

        elif kind in ("SKIP", "NEWLINE", "COMMENT"):
            continue

        elif kind == "MISMATCH":
            raise OrionSyntaxError(f"Token inesperado: {value}")

    return tokens
