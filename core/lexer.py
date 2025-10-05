import re
from core.errors import OrionSyntaxError
from core.types import OrionBool

def lex(code):
    tokens = []
    token_specification = [
        ("INVALID_COMMENT", r"(//.*|---.*)"),
        ("COMMENT",  r"--[^-].*"),
        ("NUMBER",   r"\d+(\.\d+)?"),
        ("PRINT",    r"show"),         
        ("RETURN",   r"return"),
        ("FN",       r"fn"),
        ("MATCH",    r"match"),
        ("TRUE",     r"\btrue\b"),
        ("FALSE",    r"\bfalse\b"),
        ("YES",      r"\byes\b"),        
        ("NO",       r"\bno\b"),        
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
        ("COLON",    r":"),
        ("COMMA",    r","),
        ("THIN_ARROW", r"->"),
        ("ARROW",    r"=>"),
        ("COMPARE",  r"(==|!=|<=|>=|<|>)"),
        ("ASSIGN",   r"="),
        ("OP",       r"[+\-*/]"),
        ("STRING",   r'"[^"]*"'),
        ("IDENT",    r"[a-zA-Z_][a-zA-Z0-9_]*"),  
        ("NEWLINE",  r"\n"),
        ("SKIP",     r"[ \t]+"),
        ("DOT",      r"\."),                  
        ("LBRACKET", r"\["),
        ("RBRACKET", r"\]"),
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
        elif kind == "YES":
            tokens.append(("BOOL", OrionBool(True)))
        elif kind == "NO":
            tokens.append(("BOOL", OrionBool(False)))
        elif kind == "TRUE":
            tokens.append(("BOOL", OrionBool(True)))
        elif kind == "FALSE":
            tokens.append(("BOOL", OrionBool(False)))
        elif kind in (
            "IDENT", "OP", "PRINT", "FOR", "IF", "ELSE", "ASSIGN",
            "RANGE", "RANGE_EX", "COMPARE", "LBRACE", "RBRACE",
            "LPAREN", "RPAREN", "FN", "RETURN", "TYPE",
            "AND", "OR", "NOT", "MATCH", 
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
