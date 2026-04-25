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
    Class,
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
    OnEvent,
    OnError,
    Me,
    Super,

    // Keywords – concurrency
    Spawn,
    Async,
    Await,
    Channel,
    Parallel,
    Lock,

    // Keywords – I/O
    Ask,
    Read,
    Write,
    Env,
    Append,

    // Keywords – server/net
    Serve,
    Route,
    With,
    Choices,

    // Keywords – AI / symbiotic
    Think,
    Learn,
    Sense,
    Adapt,
    Embed,
    Predict,
    Train,

    // Keywords – communication
    Sync,
    Send,
    Receive,
    Pipe,
    Task,
    Stream,

    // Identifier
    Ident(String),

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
    Ampersand,   // &
    At,          // @

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
        write!(f, "SyntaxError [línea {}, col {}]: {}", self.line, self.col, self.message)
    }
}
