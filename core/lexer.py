import re
from core.errors import OrionSyntaxError

def lex(code):
    tokens = []
    token_specification = [
        ("INVALID_COMMENT", r"(//.*|---.*)"),  # marca como error // y ---
        ("COMMENT",  r"--[^-].*"),             # solo acepta doble guion, no triple
        ("NUMBER",   r"\d+(\.\d+)?"),
        ("PRINT",    r"show"),         
        ("RETURN",   r"return"),
        ("FN",       r"fn"),
        ("MATCH",    r"match"),
        ("TRUE",     r"true"),
        ("FALSE",    r"false"),
        ("TYPE",     r"(int|float|bool|string)"),
        ("FOR",      r"for"),
        ("IN",       r"in"),
        ("IF",       r"if"),
        ("ELSE",     r"else"),
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
        ("COLON",    r":"),                   # para match-case
        ("COMMA",    r","),                   # agrega este para la coma
        ("ARROW",    r"=>"),
        ("COMPARE",  r"(==|!=|<=|>=|<|>)"),
        ("ASSIGN",   r"="),
        ("OP",       r"[+\-*/]"),
        ("STRING",   r'"[^"]*"'),
        ("IDENT",    r"[a-zA-Z_][a-zA-Z0-9_]*"),  
        ("NEWLINE",  r"\n"),
        ("SKIP",     r"[ \t]+"),
        ("DOT",      r"\."),                  
        ("MISMATCH", r"."),                   # cualquier otro char
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
            tokens.append((kind, value.strip('"')))
        elif kind in (
            "IDENT", "OP", "PRINT", "FOR", "IF", "ELSE", "ASSIGN",
            "RANGE", "RANGE_EX", "COMPARE", "LBRACE", "RBRACE",
            "LPAREN", "RPAREN", "FN", "RETURN", "TYPE",
            "TRUE", "FALSE", "AND", "OR", "NOT", "MATCH", "ARROW", "IN",
            "NULL_SAFE", "COLON", "COMMA", "DOT"  
        ):
            tokens.append((kind, value))
        elif kind in ("SKIP", "NEWLINE", "COMMENT"):
            continue
        elif kind == "MISMATCH":
            raise OrionSyntaxError(f"Token inesperado: {value}")

    return tokens


