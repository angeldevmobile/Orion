import re
from core.errors import OrionSyntaxError
from core.types import OrionBool

class Token:
    """Token con información de posición para mejores errores"""
    def __init__(self, kind, value, line=1, column=1):
        self.kind = kind
        self.value = value
        self.line = line
        self.column = column
    
    def __repr__(self):
        return f"Token({self.kind}, {self.value!r}, line={self.line}, col={self.column})"
    
    def __iter__(self):
        """Para compatibilidad con código existente (kind, value)"""
        return iter((self.kind, self.value))

# === COMPILAR REGEX UNA SOLA VEZ (PERFORMANCE) ===
_TOKEN_SPECIFICATION = [
    # --- Comentarios ---
    ("INVALID_COMMENT", r"(//.*|---.*)"),
    ("COMMENT",  r"--[^-].*"),

    # --- Números (MEJORADO: hex, bin, científico) ---
    ("NUMBER_HEX", r"0x[0-9a-fA-F]+"),           # 0xFF
    ("NUMBER_BIN", r"0b[01]+"),                  # 0b1010
    ("NUMBER_SCI", r"\d+(\.\d+)?[eE][+-]?\d+"),  # 1.5e10
    ("NUMBER",     r"\d+(\.\d+)?"),              # 123.45

    # --- Strings (MEJORADO: raw strings, multi-line) ---
    ("STRING_RAW",   r'r"[^"]*"'),               # r"raw\nstring"
    ("STRING_MULTI", r'"""[\s\S]*?"""'),         # """multi-line"""
    ("STRING",       r'"[^"]*"'),                # "normal"
    ("CHAR",         r"'[^']'"),                 # 'a'

    # --- Palabras clave ---
    ("PRINT",    r"\bshow\b"),
    ("RETURN",   r"\breturn\b"),
    ("BREAK",    r"\bbreak\b"),
    ("CONTINUE", r"\bcontinue\b"),
    ("FN",       r"\bfn\b"),
    ("CLASS",    r"\bclass\b"),
    ("CONST",    r"\bconst\b"),
    ("FOR",      r"\bfor\b"),
    ("IN",       r"\bin\b"),
    ("IF",       r"\bif\b"),
    ("ELSE",     r"\belse\b"),
    ("WHILE",    r"\bwhile\b"),
    ("MATCH",    r"\bmatch\b"),
    ("USE",      r"\buse\b"),
    ("ATTEMPT",  r"\battempt\b"),
    ("HANDLE",   r"\bhandle\b"),
    ("YES",      r"\byes\b"),
    ("NO",       r"\bno\b"),
    ("TYPE",     r"\b(int|float|bool|string|list|dict|any|auto)\b"),

    # --- Programación Orientada a Objetos ---
    ("ON_CREATE", r"\bon_create\b"),
    ("ON_EVENT",  r"\bon_event\b"),
    ("ON_ERROR",  r"\bon_error\b"),
    ("ME",        r"\bme\b"),
    ("SUPER",     r"\bsuper\b"),     

    # --- Concurrencia / procesos asíncronos ---
    ("SPAWN",     r"\bspawn\b"),
    ("ASYNC",     r"\basync\b"),
    ("AWAIT",     r"\bawait\b"),
    ("CHANNEL",   r"\bchannel\b"),
    ("PARALLEL",  r"\bparallel\b"),   
    ("LOCK",      r"\block\b"),       

    # --- Inteligencia Artificial / simbiótico ---
    ("THINK",     r"\bthink\b"),
    ("LEARN",     r"\blearn\b"),
    ("SENSE",     r"\bsense\b"),
    ("ADAPT",     r"\badapt\b"),
    ("EMBED",     r"\bembed\b"),
    ("PREDICT",   r"\bpredict\b"),     
    ("TRAIN",     r"\btrain\b"),      

    # --- Comunicación / sistemas ---
    ("SYNC",      r"\bsync\b"),
    ("SEND",      r"\bsend\b"),
    ("RECEIVE",   r"\breceive\b"),
    ("PIPE",      r"\bpipe\b"),
    ("TASK",      r"\btask\b"),
    ("STREAM",    r"\bstream\b"),    

    # --- Operadores y símbolos ---
    ("OP_ASSIGN",    r"(\+=|-=|\*=|/=|%=|\*\*=)"),
    ("RANGE_EX",     r"\.\.<"),
    ("RANGE",        r"\.\."),
    ("NULL_SAFE",    r"\?\."),
    ("PIPE_OP",      r"\|>"),
    ("DOUBLE_COLON", r"::"),
    ("ELLIPSIS",     r"\.\.\."),
    ("QUESTION",     r"\?"),
    ("EXP",          r"\*\*"),
    ("MOD",          r"%"),
    ("AND",          r"&&"),
    ("OR",           r"\|\|"),
    ("NOT",          r"!"),
    ("LPAREN",       r"\("),
    ("RPAREN",       r"\)"),
    ("LBRACE",       r"\{"),
    ("RBRACE",       r"\}"),
    ("COLON",        r":"),
    ("COMMA",        r","),
    ("SEMICOLON",    r";"),          
    ("THIN_ARROW",   r"->"),
    ("ARROW",        r"=>"),
    ("COMPARE",      r"(==|!=|<=|>=|<|>)"),
    ("ASSIGN",       r"="),
    ("OP",           r"[+\-*/]"),
    ("DOT",          r"\."),
    ("LBRACKET",     r"\["),
    ("RBRACKET",     r"\]"),
    ("AMPERSAND",    r"&"),        
    ("AT",           r"@"),            

    # --- Literales modernos ---
    ("NULL",       r"\bnull\b"),
    ("UNDEFINED",  r"\bundefined\b"),
    ("AUTO",       r"\bauto\b"),
    ("ANY",        r"\bany\b"),
    
    ("IDENT", r"[A-Za-z_][A-Za-z0-9_]*"),

    # --- Espacios y errores ---
    ("NEWLINE",  r"\n"),
    ("SKIP",     r"[ \t]+"),
    ("MISMATCH", r"."),
]

_TOK_REGEX = re.compile(
    "|".join(f"(?P<{name}>{pattern})" for name, pattern in _TOKEN_SPECIFICATION)
)

def lex(code, track_position=False):
    """
    Tokeniza código Orion.
    
    Args:
        code: String con código fuente
        track_position: Si True, retorna objetos Token con línea/columna
                       Si False, retorna tuplas (kind, value) para compatibilidad
    
    Returns:
        Lista de tokens
    """
    tokens = []
    line = 1
    line_start = 0

    for mo in _TOK_REGEX.finditer(code):
        kind = mo.lastgroup
        value = mo.group()
        column = mo.start() - line_start + 1

        # === MANEJO DE ERRORES ===
        if kind == "INVALID_COMMENT":
            raise OrionSyntaxError(
                f"Comentario inválido en línea {line}, columna {column}: {value}\n"
                f"  Usa '--' para comentarios de una línea"
            )

        # === NÚMEROS ===
        elif kind == "NUMBER_HEX":
            num_value = int(value, 16)
            tokens.append(Token("NUMBER", num_value, line, column) if track_position
                         else ("NUMBER", num_value, line))

        elif kind == "NUMBER_BIN":
            num_value = int(value, 2)
            tokens.append(Token("NUMBER", num_value, line, column) if track_position
                         else ("NUMBER", num_value, line))
        
        elif kind == "NUMBER_SCI":
            num_value = float(value)
            tokens.append(Token("NUMBER", num_value, line, column) if track_position
                         else ("NUMBER", num_value, line))

        elif kind == "NUMBER":
            num_value = float(value) if '.' in value else int(value)
            tokens.append(Token("NUMBER", num_value, line, column) if track_position
                         else ("NUMBER", num_value, line))

        # === STRINGS ===
        elif kind == "STRING_RAW":
            str_value = value[2:-1]  # Quitar r" y "
            tokens.append(Token("STRING", f'"{str_value}"', line, column) if track_position
                         else ("STRING", f'"{str_value}"', line))

        elif kind == "STRING_MULTI":
            # Multi-line string
            str_value = value[3:-3]
            tokens.append(Token("STRING", f'"{str_value}"', line, column) if track_position
                         else ("STRING", f'"{str_value}"', line))

        elif kind == "STRING":
            tokens.append(Token("STRING", value, line, column) if track_position
                         else ("STRING", value, line))

        elif kind == "CHAR":
            # Caracteres individuales
            char_value = value[1:-1]  # Quitar comillas simples
            tokens.append(Token("CHAR", char_value, line, column) if track_position
                         else ("CHAR", char_value, line))

        # === BOOLEANOS ===
        elif kind == "YES":
            tokens.append(Token("BOOL", OrionBool(True), line, column) if track_position
                         else ("BOOL", OrionBool(True), line))

        elif kind == "NO":
            tokens.append(Token("BOOL", OrionBool(False), line, column) if track_position
                         else ("BOOL", OrionBool(False), line))

        # === KEYWORDS Y OPERADORES ===
        elif kind in (
            "IDENT", "OP", "PRINT", "CLASS", "CONST", "FOR", "IF", "ELSE", "WHILE", "ASSIGN",
            "RANGE", "RANGE_EX", "COMPARE", "LBRACE", "RBRACE", "LPAREN", "RPAREN",
            "FN", "RETURN", "BREAK", "CONTINUE", "TYPE", "AND", "OR", "NOT", "MATCH",
            "USE", "ARROW", "THIN_ARROW", "IN", "NULL_SAFE", "COLON", "COMMA",
            "SEMICOLON", "DOT", "LBRACKET", "RBRACKET", "ATTEMPT", "HANDLE", "OP_ASSIGN",
            "PIPE_OP", "DOUBLE_COLON", "ELLIPSIS", "EXP", "MOD", "QUESTION", "SPAWN",
            "ASYNC", "AWAIT", "CHANNEL", "PARALLEL", "LOCK", "THINK", "LEARN", "SENSE",
            "ADAPT", "EMBED", "PREDICT", "TRAIN", "SYNC", "SEND", "RECEIVE", "PIPE",
            "TASK", "STREAM", "ON_CREATE", "ON_EVENT", "ON_ERROR", "ME", "SUPER",
            "NULL", "UNDEFINED", "AUTO", "ANY", "AMPERSAND", "AT"
        ):
            tokens.append(Token(kind, value, line, column) if track_position
                         else (kind, value, line))

        # === CONTROL DE LÍNEAS ===
        elif kind == "NEWLINE":
            line += 1
            line_start = mo.end()
            continue

        # === IGNORAR ===
        elif kind in ("SKIP", "COMMENT"):
            continue

        # === ERROR ===
        elif kind == "MISMATCH":
            raise OrionSyntaxError(
                f"Token inesperado en línea {line}, columna {column}: '{value}'\n"
                f"  No se reconoce este símbolo"
            )

    return tokens
