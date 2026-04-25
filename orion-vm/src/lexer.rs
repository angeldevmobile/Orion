use crate::token::{LexError, Token, TokenKind};

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source).tokenize()
}

//   Lexer                                    

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Lexer { src: source.as_bytes(), pos: 0, line: 1, col: 1 }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.src.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let ch = self.src.get(self.pos).copied()?;
        self.pos += 1;
        if ch == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn skip_while(&mut self, pred: impl Fn(u8) -> bool) {
        while self.peek().map_or(false, |c| pred(c)) {
            self.advance();
        }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token()?;
            let eof = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if eof { break; }
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Token, LexError> {
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n') => { self.advance(); }
                _ => break,
            }
        }

        let line = self.line;
        let col = self.col;

        macro_rules! tok {
            ($kind:expr) => { Ok(Token { kind: $kind, line, col }) };
        }
        macro_rules! one {
            ($kind:expr) => {{ self.advance(); tok!($kind) }};
        }
        macro_rules! two {
            ($kind:expr) => {{ self.advance(); self.advance(); tok!($kind) }};
        }
        macro_rules! three {
            ($kind:expr) => {{ self.advance(); self.advance(); self.advance(); tok!($kind) }};
        }

        let Some(ch) = self.peek() else {
            return tok!(TokenKind::Eof);
        };

        match ch {
            //   Comments                          
            b'-' if self.peek_at(1) == Some(b'-') => {
                if self.peek_at(2) == Some(b'-') {
                    return Err(LexError {
                        message: "Comentario inválido '---'. Usa '--' para comentarios".into(),
                        line, col,
                    });
                }
                self.skip_while(|c| c != b'\n');
                self.next_token()
            }
            b'/' if self.peek_at(1) == Some(b'/') => Err(LexError {
                message: "Comentario inválido '//'. Usa '--' para comentarios".into(),
                line, col,
            }),

            //   Numbers                           
            b'0' if self.peek_at(1) == Some(b'x') => tok!(self.lex_hex(line, col)?),
            b'0' if self.peek_at(1) == Some(b'b') => tok!(self.lex_binary(line, col)?),
            b'0'..=b'9' => tok!(self.lex_number()),

            //   Strings                           
            b'r' if self.peek_at(1) == Some(b'"') => tok!(self.lex_raw_string(line, col)?),
            b'"' if self.peek_at(1) == Some(b'"') && self.peek_at(2) == Some(b'"') => {
                tok!(self.lex_multiline_string(line, col)?)
            }
            b'"' => tok!(self.lex_string(line, col)?),
            b'\'' => tok!(self.lex_char(line, col)?),

            //   Compound assignment (longest first)             
            b'*' if self.peek_at(1) == Some(b'*') && self.peek_at(2) == Some(b'=') => three!(TokenKind::StarStarEq),
            b'+' if self.peek_at(1) == Some(b'=') => two!(TokenKind::PlusEq),
            b'-' if self.peek_at(1) == Some(b'=') => two!(TokenKind::MinusEq),
            b'*' if self.peek_at(1) == Some(b'=') => two!(TokenKind::StarEq),
            b'/' if self.peek_at(1) == Some(b'=') => two!(TokenKind::SlashEq),
            b'%' if self.peek_at(1) == Some(b'=') => two!(TokenKind::PercentEq),

            //   Range / spread                       
            b'.' if self.peek_at(1) == Some(b'.') && self.peek_at(2) == Some(b'<') => three!(TokenKind::DotDotLt),
            b'.' if self.peek_at(1) == Some(b'.') && self.peek_at(2) == Some(b'.') => three!(TokenKind::DotDotDot),
            b'.' if self.peek_at(1) == Some(b'.') => two!(TokenKind::DotDot),

            //   Multi-char operators                    
            b'?' if self.peek_at(1) == Some(b'.') => two!(TokenKind::NullSafe),
            b'|' if self.peek_at(1) == Some(b'>') => two!(TokenKind::PipeOp),
            b':' if self.peek_at(1) == Some(b':') => two!(TokenKind::DoubleColon),
            b'-' if self.peek_at(1) == Some(b'>') => two!(TokenKind::ThinArrow),
            b'=' if self.peek_at(1) == Some(b'>') => two!(TokenKind::Arrow),
            b'*' if self.peek_at(1) == Some(b'*') => two!(TokenKind::StarStar),
            b'=' if self.peek_at(1) == Some(b'=') => two!(TokenKind::Eq),
            b'!' if self.peek_at(1) == Some(b'=') => two!(TokenKind::NotEq),
            b'<' if self.peek_at(1) == Some(b'=') => two!(TokenKind::LtEq),
            b'>' if self.peek_at(1) == Some(b'=') => two!(TokenKind::GtEq),
            b'&' if self.peek_at(1) == Some(b'&') => two!(TokenKind::And),
            b'|' if self.peek_at(1) == Some(b'|') => two!(TokenKind::Or),

            //   Single-char operators                    
            b'+' => one!(TokenKind::Plus),
            b'-' => one!(TokenKind::Minus),
            b'*' => one!(TokenKind::Star),
            b'/' => one!(TokenKind::Slash),
            b'%' => one!(TokenKind::Percent),
            b'!' => one!(TokenKind::Not),
            b'<' => one!(TokenKind::Lt),
            b'>' => one!(TokenKind::Gt),
            b'=' => one!(TokenKind::Assign),
            b'(' => one!(TokenKind::LParen),
            b')' => one!(TokenKind::RParen),
            b'{' => one!(TokenKind::LBrace),
            b'}' => one!(TokenKind::RBrace),
            b'[' => one!(TokenKind::LBracket),
            b']' => one!(TokenKind::RBracket),
            b':' => one!(TokenKind::Colon),
            b',' => one!(TokenKind::Comma),
            b';' => one!(TokenKind::Semicolon),
            b'.' => one!(TokenKind::Dot),
            b'?' => one!(TokenKind::Question),
            b'&' => one!(TokenKind::Ampersand),
            b'@' => one!(TokenKind::At),

            //   Identifiers and keywords                  
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => tok!(self.lex_ident_or_keyword()),

            _ => {
                let bad = self.advance().unwrap() as char;
                Err(LexError { message: format!("Token inesperado: '{bad}'"), line, col })
            }
        }
    }

    //   Number helpers                             

    fn lex_hex(&mut self, line: u32, col: u32) -> Result<TokenKind, LexError> {
        self.advance(); // '0'
        self.advance(); // 'x'
        let start = self.pos;
        self.skip_while(|c| c.is_ascii_hexdigit());
        let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        if s.is_empty() {
            return Err(LexError { message: "Número hex vacío después de '0x'".into(), line, col });
        }
        i64::from_str_radix(s, 16)
            .map(TokenKind::Int)
            .map_err(|_| LexError { message: format!("Número hex inválido: 0x{s}"), line, col })
    }

    fn lex_binary(&mut self, line: u32, col: u32) -> Result<TokenKind, LexError> {
        self.advance(); // '0'
        self.advance(); // 'b'
        let start = self.pos;
        self.skip_while(|c| c == b'0' || c == b'1');
        let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        if s.is_empty() {
            return Err(LexError { message: "Número binario vacío después de '0b'".into(), line, col });
        }
        i64::from_str_radix(s, 2)
            .map(TokenKind::Int)
            .map_err(|_| LexError { message: format!("Número binario inválido: 0b{s}"), line, col })
    }

    fn lex_number(&mut self) -> TokenKind {
        let start = self.pos;
        self.skip_while(|c| c.is_ascii_digit());

        let has_dot = self.peek() == Some(b'.')
            && self.peek_at(1).map_or(false, |c| c.is_ascii_digit());
        if has_dot {
            self.advance();
            self.skip_while(|c| c.is_ascii_digit());
        }

        let has_sci = matches!(self.peek(), Some(b'e') | Some(b'E'));
        if has_sci {
            self.advance();
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.advance();
            }
            self.skip_while(|c| c.is_ascii_digit());
        }

        let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        if has_dot || has_sci {
            TokenKind::Float(s.parse().unwrap())
        } else {
            TokenKind::Int(s.parse().unwrap())
        }
    }

    //   String helpers                             

    fn lex_raw_string(&mut self, line: u32, col: u32) -> Result<TokenKind, LexError> {
        self.advance(); // 'r'
        self.advance(); // '"'
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b'"' { break; }
            self.advance();
        }
        let content = std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string();
        if self.advance() != Some(b'"') {
            return Err(LexError { message: "String raw sin cerrar".into(), line, col });
        }
        Ok(TokenKind::Str(content))
    }

    fn lex_multiline_string(&mut self, line: u32, col: u32) -> Result<TokenKind, LexError> {
        self.advance(); self.advance(); self.advance(); // '"""'
        let start = self.pos;
        loop {
            if self.peek().is_none() {
                return Err(LexError { message: "String multi-línea sin cerrar".into(), line, col });
            }
            if self.peek() == Some(b'"')
                && self.peek_at(1) == Some(b'"')
                && self.peek_at(2) == Some(b'"')
            {
                break;
            }
            self.advance();
        }
        let content = std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string();
        self.advance(); self.advance(); self.advance(); // closing '"""'
        Ok(TokenKind::Str(content))
    }

    fn lex_string(&mut self, line: u32, col: u32) -> Result<TokenKind, LexError> {
        self.advance(); // '"'
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b'"' { break; }
            self.advance();
        }
        let content = std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string();
        if self.advance() != Some(b'"') {
            return Err(LexError { message: "String sin cerrar".into(), line, col });
        }
        Ok(TokenKind::Str(content))
    }

    fn lex_char(&mut self, line: u32, col: u32) -> Result<TokenKind, LexError> {
        self.advance(); // '\''
        let ch = self.advance()
            .ok_or_else(|| LexError { message: "Literal de carácter vacío".into(), line, col })?
            as char;
        if self.advance() != Some(b'\'') {
            return Err(LexError { message: "Literal de carácter sin cerrar".into(), line, col });
        }
        Ok(TokenKind::Str(ch.to_string()))
    }

    //   Identifier / keyword                          

    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        let start = self.pos;
        self.skip_while(|c| c.is_ascii_alphanumeric() || c == b'_');
        let word = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        keyword_or_ident(word)
    }
}

//   Keyword table                                

fn keyword_or_ident(word: &str) -> TokenKind {
    match word {
        "null"      => TokenKind::Null,
        "undefined" => TokenKind::Undefined,
        "yes"       => TokenKind::Bool(true),
        "no"        => TokenKind::Bool(false),

        "int"    => TokenKind::TypeInt,
        "float"  => TokenKind::TypeFloat,
        "bool"   => TokenKind::TypeBool,
        "string" => TokenKind::TypeString,
        "list"   => TokenKind::TypeList,
        "dict"   => TokenKind::TypeDict,
        "any"    => TokenKind::TypeAny,
        "auto"   => TokenKind::TypeAuto,

        "show"     => TokenKind::Show,
        "return"   => TokenKind::Return,
        "break"    => TokenKind::Break,
        "continue" => TokenKind::Continue,
        "fn"       => TokenKind::Fn,
        "class"    => TokenKind::Class,
        "const"    => TokenKind::Const,
        "for"      => TokenKind::For,
        "in"       => TokenKind::In,
        "if"       => TokenKind::If,
        "else"     => TokenKind::Else,
        "while"    => TokenKind::While,
        "match"    => TokenKind::Match,
        "use"      => TokenKind::Use,
        "attempt"  => TokenKind::Attempt,
        "handle"   => TokenKind::Handle,
        "error"    => TokenKind::ErrorKw,
        "as"       => TokenKind::As,
        "take"     => TokenKind::Take,

        "shape"     => TokenKind::Shape,
        "act"       => TokenKind::Act,
        "using"     => TokenKind::Using,
        "is"        => TokenKind::Is,
        "on_create" => TokenKind::OnCreate,
        "on_event"  => TokenKind::OnEvent,
        "on_error"  => TokenKind::OnError,
        "me"        => TokenKind::Me,
        "super"     => TokenKind::Super,

        "spawn"    => TokenKind::Spawn,
        "async"    => TokenKind::Async,
        "await"    => TokenKind::Await,
        "channel"  => TokenKind::Channel,
        "parallel" => TokenKind::Parallel,
        "lock"     => TokenKind::Lock,

        "ask"    => TokenKind::Ask,
        "read"   => TokenKind::Read,
        "write"  => TokenKind::Write,
        "env"    => TokenKind::Env,
        "append" => TokenKind::Append,

        "serve"   => TokenKind::Serve,
        "route"   => TokenKind::Route,
        "with"    => TokenKind::With,
        "choices" => TokenKind::Choices,

        "think"   => TokenKind::Think,
        "learn"   => TokenKind::Learn,
        "sense"   => TokenKind::Sense,
        "adapt"   => TokenKind::Adapt,
        "embed"   => TokenKind::Embed,
        "predict" => TokenKind::Predict,
        "train"   => TokenKind::Train,

        "sync"    => TokenKind::Sync,
        "send"    => TokenKind::Send,
        "receive" => TokenKind::Receive,
        "pipe"    => TokenKind::Pipe,
        "task"    => TokenKind::Task,
        "stream"  => TokenKind::Stream,

        _ => TokenKind::Ident(word.to_string()),
    }
}

//   Tests                                    

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind;

    fn kinds(src: &str) -> Vec<TokenKind> {
        lex(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn test_integers() {
        assert_eq!(kinds("42"), vec![TokenKind::Int(42), TokenKind::Eof]);
        assert_eq!(kinds("0xFF"), vec![TokenKind::Int(255), TokenKind::Eof]);
        assert_eq!(kinds("0b1010"), vec![TokenKind::Int(10), TokenKind::Eof]);
    }

    #[test]
    fn test_floats() {
        assert_eq!(kinds("3.14"), vec![TokenKind::Float(3.14), TokenKind::Eof]);
        assert_eq!(kinds("1.5e10"), vec![TokenKind::Float(1.5e10), TokenKind::Eof]);
    }

    #[test]
    fn test_strings() {
        assert_eq!(kinds(r#""hello""#), vec![TokenKind::Str("hello".into()), TokenKind::Eof]);
        assert_eq!(kinds(r#"r"raw""#),  vec![TokenKind::Str("raw".into()),   TokenKind::Eof]);
    }

    #[test]
    fn test_booleans() {
        assert_eq!(kinds("yes"), vec![TokenKind::Bool(true),  TokenKind::Eof]);
        assert_eq!(kinds("no"),  vec![TokenKind::Bool(false), TokenKind::Eof]);
    }

    #[test]
    fn test_operators() {
        assert_eq!(kinds("**="), vec![TokenKind::StarStarEq, TokenKind::Eof]);
        assert_eq!(kinds(".."),  vec![TokenKind::DotDot,     TokenKind::Eof]);
        assert_eq!(kinds("..<"), vec![TokenKind::DotDotLt,   TokenKind::Eof]);
        assert_eq!(kinds("..."), vec![TokenKind::DotDotDot,  TokenKind::Eof]);
        assert_eq!(kinds("|>"),  vec![TokenKind::PipeOp,     TokenKind::Eof]);
    }

    #[test]
    fn test_keywords() {
        assert_eq!(kinds("shape"),     vec![TokenKind::Shape,    TokenKind::Eof]);
        assert_eq!(kinds("think"),     vec![TokenKind::Think,    TokenKind::Eof]);
        assert_eq!(kinds("on_create"), vec![TokenKind::OnCreate, TokenKind::Eof]);
    }

    #[test]
    fn test_comment_skip() {
        assert_eq!(kinds("-- comentario\n42"), vec![TokenKind::Int(42), TokenKind::Eof]);
    }

    #[test]
    fn test_invalid_comment() {
        assert!(lex("// bad").is_err());
        assert!(lex("--- bad").is_err());
    }

    #[test]
    fn test_simple_expression() {
        assert_eq!(kinds("x = 1 + 2"), vec![
            TokenKind::Ident("x".into()),
            TokenKind::Assign,
            TokenKind::Int(1),
            TokenKind::Plus,
            TokenKind::Int(2),
            TokenKind::Eof,
        ]);
    }
}
