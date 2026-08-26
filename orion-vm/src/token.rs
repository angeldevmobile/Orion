use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
    Undefined,

    // Keywords – control flow
    Show,
    Return,
    Break,
    Continue,
    Fn,
    Const,
    For,
    In,
    If,
    Else,
    While,
    Match,
    Use,
    Attempt,
    Handle,
    ErrorKw,
    As,
    Take,
    Extern,

    // Keywords – type annotations
    TypeInt,
    TypeFloat,
    TypeBool,
    TypeString,
    TypeList,
    TypeDict,
    TypeAny,
    TypeAuto,

    // Keywords – OOP
    Shape,
    Act,
    Using,
    Is,
    OnCreate,
    OnError,
    Me,
    Super,

    // Keywords – concurrency
    Spawn,
    Async,
    Await,

    // Keywords – I/O
    Ask,
    Read,
    Write,
    Append,

    // Keywords – server/net
    Serve,
    With,
    Choices,

    // Keywords – AI / symbiotic
    Think,
    Learn,
    Sense,

    // Identifier
    Ident(String),

    // Docstring — líneas `/// texto` que documentan la siguiente declaración
    DocComment(String),

    // Arithmetic operators
    Plus,       // +
    Minus,      // -
    Star,       // *
    Slash,      // /
    Percent,    // %
    StarStar,   // **

    // Comparison operators
    Eq,         // ==
    NotEq,      // !=
    Lt,         // <
    LtEq,       // <=
    Gt,         // >
    GtEq,       // >=

    // Logical operators
    And,        // &&
    Or,         // ||
    Not,        // !

    // Assignment operators
    Assign,     // =
    PlusEq,     // +=
    MinusEq,    // -=
    StarEq,     // *=
    SlashEq,    // /=
    PercentEq,  // %=
    StarStarEq, // **=

    // Arrow operators
    Arrow,      // =>
    ThinArrow,  // ->

    // Range / spread
    DotDotLt,   // ..<
    DotDotDot,  // ...
    DotDot,     // ..
    Dot,        // .

    // Special operators
    NullSafe,    // ?.
    PipeOp,      // |>
    DoubleColon, // ::
    Question,    // ?
    At,          // @

    Ampersand,   // &   AND
    Pipe,        // |   OR   (a dos pasos de |> y ||, que se reconocen antes)
    Caret,       // ^   XOR
    Shl,         // <<
    Shr,         // >>

    // Delimiters
    LParen,    // (
    RParen,    // )
    LBrace,    // {
    RBrace,    // }
    LBracket,  // [
    RBracket,  // ]
    Colon,     // :
    Comma,     // ,
    Semicolon, // ;

    Eof,
}

impl TokenKind {
    /// Si el token es una palabra clave, su texto original.
    ///
    /// Sirve para permitir keywords como nombres de miembro tras un punto
    /// (`ai.ask`, `fs.read`, `net.error`…), igual que Python/JS: después de `.`
    /// no hay ambigüedad sintáctica posible. Debe cubrir TODAS las keywords del
    /// lexer — si se agrega una nueva allí, agregarla aquí también.
    pub fn keyword_text(&self) -> Option<&'static str> {
        use TokenKind::*;
        Some(match self {
            Show => "show", Return => "return", Break => "break", Continue => "continue",
            Fn => "fn", Const => "const", For => "for", In => "in", If => "if",
            Else => "else", While => "while", Match => "match", Use => "use",
            Attempt => "attempt", Handle => "handle", ErrorKw => "error", As => "as",
            Take => "take", Extern => "extern",
            TypeInt => "int", TypeFloat => "float", TypeBool => "bool",
            TypeString => "string", TypeList => "list", TypeDict => "dict",
            TypeAny => "any", TypeAuto => "auto",
            Shape => "shape", Act => "act", Using => "using", Is => "is",
            OnCreate => "on_create", OnError => "on_error", Me => "me", Super => "super",
            Spawn => "spawn", Async => "async", Await => "await",
            Ask => "ask", Read => "read", Write => "write", Append => "append",
            Serve => "serve", With => "with", Choices => "choices",
            Think => "think", Learn => "learn", Sense => "sense",
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug)]
pub struct LexError {
    pub message: String,
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SyntaxError [line {}, col {}]: {}", self.line, self.col, self.message)
    }
}
