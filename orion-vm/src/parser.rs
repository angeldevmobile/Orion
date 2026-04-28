#![allow(dead_code)]
use crate::token::{Token, TokenKind};
use crate::ast::{ActDef, Expr, FieldDef, Handler, MatchArm, Param, Stmt};

//   Error de parsing                              

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub line: u32,
    pub col: u32,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SyntaxError [línea {}, col {}]: {}", self.line, self.col, self.message)
    }
}

//   Punto de entrada                              

pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>, ParseError> {
    let mut p = Parser::new(tokens);
    p.parse_program()
}

//   Parser                                   

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    //   Utilidades básicas                           

    fn peek(&self) -> &TokenKind {
        self.tokens.get(self.pos).map(|t| &t.kind).unwrap_or(&TokenKind::Eof)
    }

    fn peek_at(&self, offset: usize) -> &TokenKind {
        self.tokens.get(self.pos + offset).map(|t| &t.kind).unwrap_or(&TokenKind::Eof)
    }

    fn current_line(&self) -> u32 {
        self.tokens.get(self.pos).map(|t| t.line).unwrap_or(0)
    }

    fn advance(&mut self) -> &TokenKind {
        let kind = &self.tokens[self.pos].kind;
        self.pos += 1;
        kind
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), TokenKind::Semicolon) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<(), ParseError> {
        if self.peek() == expected {
            self.pos += 1;
            Ok(())
        } else {
            let line = self.current_line();
            let col = self.tokens.get(self.pos).map(|t| t.col).unwrap_or(0);
            Err(ParseError {
                message: format!("Se esperaba {:?}, pero se encontró {:?}", expected, self.peek()),
                line, col,
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        if let TokenKind::Ident(name) = self.peek().clone() {
            self.pos += 1;
            Ok(name)
        } else {
            let line = self.current_line();
            let col = self.tokens.get(self.pos).map(|t| t.col).unwrap_or(0);
            Err(ParseError {
                message: format!("Se esperaba un identificador, pero se encontró {:?}", self.peek()),
                line, col,
            })
        }
    }

    /// Como expect_ident pero también acepta keywords como nombres de atributo/método.
    /// Usado después de `.` para permitir `obj.append(...)`, `obj.len`, etc.
    fn expect_attr_name(&mut self) -> Result<String, ParseError> {
        use crate::token::TokenKind::*;
        let name = match self.peek().clone() {
            Ident(n) => n,
            // Keywords que pueden usarse como nombres de método
            Append => "append".to_string(),
            _ => {
                let line = self.current_line();
                let col = self.tokens.get(self.pos).map(|t| t.col).unwrap_or(0);
                return Err(ParseError {
                    message: format!("Se esperaba un identificador, pero se encontró {:?}", self.peek()),
                    line, col,
                });
            }
        };
        self.pos += 1;
        Ok(name)
    }

    fn err(&self, msg: impl Into<String>) -> ParseError {
        let tok = self.tokens.get(self.pos);
        ParseError {
            message: msg.into(),
            line: tok.map(|t| t.line).unwrap_or(0),
            col:  tok.map(|t| t.col).unwrap_or(0),
        }
    }

    //   Programa                                

    fn parse_program(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::Eof) { break; }
            stmts.push(self.parse_statement()?);
        }
        Ok(stmts)
    }

    //   Bloque `{ ... }`                            

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) { break; }
            stmts.push(self.parse_statement()?);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(stmts)
    }

    //   Parámetros de función                         ─

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            let name = self.expect_ident()?;
            let mut type_hint = None;
            let mut default = None;

            // tipo opcional: name: type
            if matches!(self.peek(), TokenKind::Colon) {
                self.pos += 1;
                type_hint = Some(self.parse_type_name()?);
            }
            // valor por defecto: name = expr
            if matches!(self.peek(), TokenKind::Assign) {
                self.pos += 1;
                default = Some(self.parse_expression()?);
            }
            params.push(Param { name, type_hint, default });
            if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
        }
        self.expect(&TokenKind::RParen)?;
        Ok(params)
    }

    fn parse_type_name(&mut self) -> Result<String, ParseError> {
        let base = match self.peek().clone() {
            TokenKind::TypeInt    => { self.pos += 1; "int".to_string() }
            TokenKind::TypeFloat  => { self.pos += 1; "float".to_string() }
            TokenKind::TypeBool   => { self.pos += 1; "bool".to_string() }
            TokenKind::TypeString => { self.pos += 1; "string".to_string() }
            TokenKind::TypeList   => { self.pos += 1; "List".to_string() }
            TokenKind::TypeDict   => { self.pos += 1; "Dict".to_string() }
            TokenKind::TypeAny    => { self.pos += 1; "any".to_string() }
            TokenKind::TypeAuto   => { self.pos += 1; "auto".to_string() }
            TokenKind::Ident(n)   => { self.pos += 1; n }
            _ => return Err(self.err("Se esperaba un tipo")),
        };
        // Tipo genérico aplicado: List[T], Map[K, V], Stack[int], etc.
        if matches!(self.peek(), TokenKind::LBracket) {
            self.pos += 1; // [
            let mut args = Vec::new();
            while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                args.push(self.parse_type_name()?);
                if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
            }
            self.expect(&TokenKind::RBracket)?;
            return Ok(format!("{}[{}]", base, args.join(", ")));
        }
        Ok(base)
    }

    /// Parsea parámetros de tipo: [T], [T, U], [K, V] — retorna vec vacío si no hay `[`
    fn parse_type_params(&mut self) -> Result<Vec<String>, ParseError> {
        if !matches!(self.peek(), TokenKind::LBracket) {
            return Ok(vec![]);
        }
        self.pos += 1; // [
        let mut params = Vec::new();
        while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
            params.push(self.expect_ident()?);
            if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
        }
        self.expect(&TokenKind::RBracket)?;
        Ok(params)
    }

    /// Comprueba si el token actual puede comenzar un nombre de tipo (sin consumir).
    fn is_type_token(&self) -> bool {
        matches!(self.peek(),
            TokenKind::TypeInt | TokenKind::TypeFloat | TokenKind::TypeBool |
            TokenKind::TypeString | TokenKind::TypeList | TokenKind::TypeDict |
            TokenKind::TypeAny | TokenKind::TypeAuto | TokenKind::Ident(_)
        )
    }

    //   Argumentos de llamada  f(a, b, kw=val)                 

    fn parse_call_args(&mut self) -> Result<(Vec<Expr>, Vec<(String, Expr)>), ParseError> {
        self.expect(&TokenKind::LParen)?;
        let mut args = Vec::new();
        let mut kwargs = Vec::new();

        while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
            // kwarg: ident =
            if let TokenKind::Ident(name) = self.peek().clone() {
                if matches!(self.peek_at(1), TokenKind::Assign) {
                    let n = name.clone();
                    self.pos += 2; // skip name and '='
                    let val = self.parse_expression()?;
                    kwargs.push((n, val));
                    if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
                    continue;
                }
            }
            args.push(self.parse_expression()?);
            if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
        }
        self.expect(&TokenKind::RParen)?;
        Ok((args, kwargs))
    }

    // ══════════════════════════════════════════════════════════════════════════
    // EXPRESIONES  (precedencia ascendente: or < and < compare < add < mul < pow < unary < primary)
    // ══════════════════════════════════════════════════════════════════════════

    fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        // fn(params) { } — función anónima como expresión
        if matches!(self.peek(), TokenKind::Fn) && matches!(self.peek_at(1), TokenKind::LParen) {
            return self.parse_anon_fn();
        }

        // lambda: ident => expr  |  (p1, p2) => expr
        if self.is_lambda_ahead() {
            return self.parse_lambda();
        }

        let mut expr = self.parse_or()?;

        // is-check: expr is ShapeName
        if matches!(self.peek(), TokenKind::Is) {
            self.pos += 1;
            let shape = self.expect_ident()?;
            expr = Expr::IsCheck { expr: Box::new(expr), shape };
        }

        Ok(expr)
    }

    fn is_lambda_ahead(&self) -> bool {
        // ident =>
        if matches!(self.peek(), TokenKind::Ident(_)) && matches!(self.peek_at(1), TokenKind::Arrow) {
            return true;
        }
        // (params) =>  — buscar ')' seguido de '=>'
        if matches!(self.peek(), TokenKind::LParen) {
            let mut depth = 0usize;
            let mut i = self.pos;
            loop {
                match self.tokens.get(i).map(|t| &t.kind).unwrap_or(&TokenKind::Eof) {
                    TokenKind::LParen => { depth += 1; i += 1; }
                    TokenKind::RParen => {
                        depth -= 1;
                        i += 1;
                        if depth == 0 {
                            return matches!(self.tokens.get(i).map(|t| &t.kind).unwrap_or(&TokenKind::Eof), TokenKind::Arrow);
                        }
                    }
                    TokenKind::Eof => return false,
                    _ => { i += 1; }
                }
            }
        }
        false
    }

    fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        let params = if matches!(self.peek(), TokenKind::LParen) {
            self.expect(&TokenKind::LParen)?;
            let mut names = Vec::new();
            while !matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
                names.push(self.expect_ident()?);
                if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
            }
            self.expect(&TokenKind::RParen)?;
            names
        } else {
            vec![self.expect_ident()?]
        };
        self.expect(&TokenKind::Arrow)?; // =>

        // cuerpo: bloque { } o expresión simple
        let body = if matches!(self.peek(), TokenKind::LBrace) {
            self.parse_block()?
        } else {
            let expr = self.parse_expression()?;
            vec![Stmt::Expr { expr, line: self.current_line() }]
        };
        Ok(Expr::Lambda { params, body })
    }

    fn parse_anon_fn(&mut self) -> Result<Expr, ParseError> {
        self.pos += 1; // 'fn'
        let _type_params = self.parse_type_params()?; // fn[T](...) anónima genérica
        let params = self.parse_params()?;
        if matches!(self.peek(), TokenKind::ThinArrow) {
            self.pos += 1;
            self.parse_type_name().ok();
        }
        let body = self.parse_block()?;
        Ok(Expr::Lambda { params: params.into_iter().map(|p| p.name).collect(), body })
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), TokenKind::Or) {
            self.pos += 1;
            let right = self.parse_and()?;
            left = Expr::BinaryOp { op: "||".into(), left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_compare()?;
        while matches!(self.peek(), TokenKind::And) {
            self.pos += 1;
            let right = self.parse_compare()?;
            left = Expr::BinaryOp { op: "&&".into(), left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_compare(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_arith()?;
        loop {
            let op = match self.peek() {
                TokenKind::Eq    => "==",
                TokenKind::NotEq => "!=",
                TokenKind::Lt    => "<",
                TokenKind::LtEq  => "<=",
                TokenKind::Gt    => ">",
                TokenKind::GtEq  => ">=",
                _ => break,
            }.to_string();
            self.pos += 1;
            let right = self.parse_arith()?;
            left = Expr::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_arith(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_term()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus  => "+",
                TokenKind::Minus => "-",
                _ => break,
            }.to_string();
            self.pos += 1;
            let right = self.parse_term()?;
            left = Expr::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_power()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star    => "*",
                TokenKind::Slash   => "/",
                TokenKind::Percent => "%",
                _ => break,
            }.to_string();
            self.pos += 1;
            let right = self.parse_power()?;
            left = Expr::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Expr, ParseError> {
        let base = self.parse_unary()?;
        if matches!(self.peek(), TokenKind::StarStar) {
            self.pos += 1;
            let exp = self.parse_power()?; // right-associative
            return Ok(Expr::BinaryOp { op: "**".into(), left: Box::new(base), right: Box::new(exp) });
        }
        Ok(base)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if matches!(self.peek(), TokenKind::Not) {
            self.pos += 1;
            let expr = self.parse_unary()?;
            return Ok(Expr::UnaryOp { op: "!".into(), expr: Box::new(expr) });
        }
        if matches!(self.peek(), TokenKind::Minus) {
            self.pos += 1;
            let expr = self.parse_unary()?;
            return Ok(Expr::UnaryOp { op: "-".into(), expr: Box::new(expr) });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                // método: expr.method(args) o acceso: expr.field
                TokenKind::Dot => {
                    self.pos += 1;
                    let attr = self.expect_attr_name()?;
                    if matches!(self.peek(), TokenKind::LParen) {
                        let (args, kwargs) = self.parse_call_args()?;
                        expr = Expr::CallMethod { method: attr, receiver: Box::new(expr), args, kwargs };
                    } else {
                        expr = Expr::AttrAccess { object: Box::new(expr), attr };
                    }
                }
                // null-safe: expr?.field
                TokenKind::NullSafe => {
                    self.pos += 1;
                    let attr = self.expect_ident()?;
                    expr = Expr::NullSafe { object: Box::new(expr), attr };
                }
                // índice: expr[i]  o slice: expr[a:b]
                TokenKind::LBracket => {
                    self.pos += 1;
                    if matches!(self.peek(), TokenKind::Colon) {
                        self.pos += 1;
                        let end = if !matches!(self.peek(), TokenKind::RBracket) {
                            Some(Box::new(self.parse_expression()?))
                        } else { None };
                        self.expect(&TokenKind::RBracket)?;
                        expr = Expr::SliceAccess { object: Box::new(expr), start: None, end };
                    } else {
                        let first = self.parse_expression()?;
                        if matches!(self.peek(), TokenKind::Colon) {
                            self.pos += 1;
                            let end = if !matches!(self.peek(), TokenKind::RBracket) {
                                Some(Box::new(self.parse_expression()?))
                            } else { None };
                            self.expect(&TokenKind::RBracket)?;
                            expr = Expr::SliceAccess { object: Box::new(expr), start: Some(Box::new(first)), end };
                        } else {
                            self.expect(&TokenKind::RBracket)?;
                            expr = Expr::Index { object: Box::new(expr), index: Box::new(first) };
                        }
                    }
                }
                // llamada: expr(args)
                TokenKind::LParen => {
                    let (args, kwargs) = self.parse_call_args()?;
                    expr = Expr::Call { callee: Box::new(expr), args, kwargs };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            TokenKind::Int(n)   => { self.pos += 1; Ok(Expr::Int(n)) }
            TokenKind::Float(f) => { self.pos += 1; Ok(Expr::Float(f)) }
            TokenKind::Str(s)   => { self.pos += 1; Ok(Expr::Str(s)) }
            TokenKind::Bool(b)  => { self.pos += 1; Ok(Expr::Bool(b)) }
            TokenKind::Null      => { self.pos += 1; Ok(Expr::Null) }
            TokenKind::Undefined => { self.pos += 1; Ok(Expr::Undefined) }

            TokenKind::Ident(name) => {
                self.pos += 1;
                Ok(Expr::Ident(name))
            }

            // Tipos usados como función: int(x), float(x), etc.
            TokenKind::TypeInt | TokenKind::TypeFloat | TokenKind::TypeBool
            | TokenKind::TypeString | TokenKind::TypeList | TokenKind::TypeDict
            | TokenKind::TypeAny | TokenKind::TypeAuto => {
                let name = self.parse_type_name()?;
                Ok(Expr::Ident(name))
            }

            // Paréntesis agrupados
            TokenKind::LParen => {
                self.pos += 1;
                let expr = self.parse_expression()?;
                self.expect(&TokenKind::RParen)?;
                Ok(expr)
            }

            // Lista: [a, b, c]
            TokenKind::LBracket => {
                self.pos += 1;
                let mut elems = Vec::new();
                while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                    elems.push(self.parse_expression()?);
                    if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
                }
                self.expect(&TokenKind::RBracket)?;
                Ok(Expr::List(elems))
            }

            // Diccionario: { "key": val, ... }
            TokenKind::LBrace => {
                self.pos += 1;
                let mut items = Vec::new();
                while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                    let key = match self.peek().clone() {
                        TokenKind::Str(s)   => { self.pos += 1; s }
                        TokenKind::Ident(n) => { self.pos += 1; n }
                        _ => return Err(self.err("Se esperaba clave de diccionario")),
                    };
                    self.expect(&TokenKind::Colon)?;
                    let val = self.parse_expression()?;
                    items.push((key, val));
                    if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
                }
                self.expect(&TokenKind::RBrace)?;
                Ok(Expr::Dict(items))
            }

            // me — referencia a la instancia actual dentro de act/on_create
            TokenKind::Me => {
                self.pos += 1;
                Ok(Expr::Ident("me".to_string()))
            }

            // await como expresión: result = await future
            TokenKind::Await => {
                self.pos += 1;
                let inner = self.parse_primary()?;
                Ok(Expr::Await(Box::new(inner)))
            }

            kind => Err(self.err(format!("Token inesperado en expresión: {:?}", kind))),
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // DECLARACIONES
    // ══════════════════════════════════════════════════════════════════════════

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        let line = self.current_line();
        self.skip_newlines();
        match self.peek().clone() {

            //   const x = expr                        ─
            TokenKind::Const => {
                self.pos += 1;
                let name = self.expect_ident()?;
                self.expect(&TokenKind::Assign)?;
                let value = self.parse_expression()?;
                Ok(Stmt::Const { name, value, line })
            }

            //   show expr                           
            TokenKind::Show => {
                self.pos += 1;
                let value = self.parse_expression()?;
                Ok(Stmt::Show { value, line })
            }

            //   return [expr]                         
            TokenKind::Return => {
                self.pos += 1;
                let value = if !matches!(self.peek(), TokenKind::RBrace | TokenKind::Semicolon | TokenKind::Eof) {
                    Some(self.parse_expression()?)
                } else { None };
                Ok(Stmt::Return { value, line })
            }

            //   break                             
            TokenKind::Break    => { self.pos += 1; Ok(Stmt::Break { line }) }

            //   continue                           ─
            TokenKind::Continue => { self.pos += 1; Ok(Stmt::Continue { line }) }

            //   fn name[T, U](params) -> ret { body }
            TokenKind::Fn => {
                self.pos += 1;
                let name = self.expect_ident()?;
                let type_params = self.parse_type_params()?;
                let params = self.parse_params()?;
                let ret_type = if matches!(self.peek(), TokenKind::ThinArrow) {
                    self.pos += 1;
                    self.parse_type_name().ok()
                } else { None };
                let body = self.parse_block()?;
                Ok(Stmt::Fn { name, type_params, params, body, ret_type, line })
            }

            //   async fn name[T](params) -> ret { body }
            TokenKind::Async => {
                self.pos += 1; // async
                self.expect(&TokenKind::Fn)?;
                let name = self.expect_ident()?;
                let type_params = self.parse_type_params()?;
                let params = self.parse_params()?;
                let ret_type = if matches!(self.peek(), TokenKind::ThinArrow) {
                    self.pos += 1;
                    self.parse_type_name().ok()
                } else { None };
                let body = self.parse_block()?;
                Ok(Stmt::AsyncFn { name, type_params, params, body, ret_type, line })
            }

            //   if cond { } [else { }]                    ─
            TokenKind::If => {
                self.pos += 1;
                let cond = self.parse_expression()?;
                let then_body = self.parse_block()?;
                self.skip_newlines();
                let else_body = if matches!(self.peek(), TokenKind::Else) {
                    self.pos += 1;
                    if matches!(self.peek(), TokenKind::If) {
                        vec![self.parse_statement()?]
                    } else {
                        self.parse_block()?
                    }
                } else { Vec::new() };
                Ok(Stmt::If { cond, then_body, else_body, line })
            }

            //   while cond { }                        ─
            TokenKind::While => {
                self.pos += 1;
                let cond = self.parse_expression()?;
                let body = self.parse_block()?;
                Ok(Stmt::While { cond, body, line })
            }

            //   for var in expr { }                      
            TokenKind::For => {
                self.pos += 1;
                let var = self.expect_ident()?;
                self.expect(&TokenKind::In)?;
                let iter = self.parse_expression()?;
                let body = self.parse_block()?;
                Ok(Stmt::For { var, iter, body, line })
            }

            //   match expr { pattern { } ... }                ─
            TokenKind::Match => {
                self.pos += 1;
                let expr = self.parse_expression()?;
                self.expect(&TokenKind::LBrace)?;
                let mut arms = Vec::new();
                loop {
                    self.skip_newlines();
                    if matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) { break; }
                    let pattern = self.parse_expression()?;
                    let body = self.parse_block()?;
                    arms.push(MatchArm { pattern, body });
                }
                self.expect(&TokenKind::RBrace)?;
                Ok(Stmt::Match { expr, arms, line })
            }

            //   use "path" [as alias] [take [fn1, fn2]]            
            TokenKind::Use => {
                self.pos += 1;
                let path = match self.peek().clone() {
                    TokenKind::Str(s)   => { self.pos += 1; s }
                    TokenKind::Ident(n) => { self.pos += 1; n }
                    _ => return Err(self.err("Se esperaba una ruta de módulo después de 'use'")),
                };
                let alias = if matches!(self.peek(), TokenKind::As) {
                    self.pos += 1;
                    Some(self.expect_ident()?)
                } else { None };
                let selective = if matches!(self.peek(), TokenKind::Take) {
                    self.pos += 1;
                    self.expect(&TokenKind::LBracket)?;
                    let mut names = Vec::new();
                    while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                        names.push(self.expect_ident()?);
                        if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
                    }
                    self.expect(&TokenKind::RBracket)?;
                    Some(names)
                } else { None };
                Ok(Stmt::Use { path, alias, selective, line })
            }

            //   attempt { } handle err { }                  ─
            TokenKind::Attempt => {
                self.pos += 1;
                let body = self.parse_block()?;
                self.skip_newlines();
                let handler = if matches!(self.peek(), TokenKind::Handle) {
                    self.pos += 1;
                    let err_name = if let TokenKind::Ident(_) = self.peek() {
                        self.expect_ident()?
                    } else { "_error".into() };
                    let hbody = self.parse_block()?;
                    Some(Handler { err_name, body: hbody })
                } else { None };
                Ok(Stmt::Attempt { body, handler, line })
            }

            //   error expr                          ─
            TokenKind::ErrorKw => {
                self.pos += 1;
                let msg = self.parse_expression()?;
                Ok(Stmt::ErrorStmt { msg, line })
            }

            //   think expr                          ─
            TokenKind::Think => {
                self.pos += 1;
                let prompt = self.parse_expression()?;
                Ok(Stmt::Think { prompt, line })
            }

            //   learn expr                          ─
            TokenKind::Learn => {
                self.pos += 1;
                let text = self.parse_expression()?;
                Ok(Stmt::Learn { text, line })
            }

            //   sense expr                          ─
            TokenKind::Sense => {
                self.pos += 1;
                let query = self.parse_expression()?;
                Ok(Stmt::Sense { query, line })
            }

            //   spawn expr                          ─
            TokenKind::Spawn => {
                self.pos += 1;
                let call = self.parse_expression()?;
                Ok(Stmt::Spawn { call, line })
            }

            //   ask "msg" [as type] [choices expr] -> var           
            TokenKind::Ask => {
                self.pos += 1;
                let prompt = self.parse_expression()?;
                let mut cast = None;
                let mut choices = None;
                while matches!(self.peek(), TokenKind::As | TokenKind::Choices) {
                    if matches!(self.peek(), TokenKind::As) {
                        self.pos += 1;
                        cast = Some(self.parse_type_name()?);
                    } else {
                        self.pos += 1;
                        choices = Some(self.parse_expression()?);
                    }
                }
                self.expect(&TokenKind::ThinArrow)?;
                let var = self.expect_ident()?;
                Ok(Stmt::Ask { prompt, var, cast, choices, line })
            }

            //   read "path" [as type] -> var                 ─
            TokenKind::Read => {
                self.pos += 1;
                let path = self.parse_expression()?;
                if matches!(self.peek(), TokenKind::As) {
                    self.pos += 1;
                    self.parse_type_name().ok(); // consumir tipo (ignorado por ahora)
                }
                self.expect(&TokenKind::ThinArrow)?;
                let var = self.expect_ident()?;
                Ok(Stmt::Read { path, var, line })
            }

            //   write "path" with|append expr                 
            TokenKind::Write => {
                self.pos += 1;
                let path = self.parse_expression()?;
                let content = if matches!(self.peek(), TokenKind::Append) {
                    self.pos += 1;
                    self.parse_expression()?
                } else {
                    self.expect(&TokenKind::With)?;
                    self.parse_expression()?
                };
                Ok(Stmt::Write { path, content, line })
            }

            //   append "path" with expr                    ─
            TokenKind::Append => {
                self.pos += 1;
                let path = self.parse_expression()?;
                self.expect(&TokenKind::With)?;
                let content = self.parse_expression()?;
                Ok(Stmt::Append { path, content, line })
            }

            //   serve port handler                       
            TokenKind::Serve => {
                self.pos += 1;
                let port = self.parse_expression()?;
                // routes: bloque de route statements
                let routes = if matches!(self.peek(), TokenKind::LBrace) {
                    self.parse_block()?
                } else {
                    let fn_expr = self.parse_expression()?;
                    vec![Stmt::Expr { expr: fn_expr, line }]
                };
                Ok(Stmt::Serve { port, routes, line })
            }

            //   shape Name[T, U] { fields, on_create, acts }
            TokenKind::Shape => {
                self.pos += 1;
                let name = self.expect_ident()?;
                let type_params = self.parse_type_params()?;
                // using: shape Name using [Other1, Other2]
                let mut using = Vec::new();
                if matches!(self.peek(), TokenKind::Using) {
                    self.pos += 1;
                    self.expect(&TokenKind::LBracket)?;
                    while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                        using.push(self.expect_ident()?);
                        if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
                    }
                    self.expect(&TokenKind::RBracket)?;
                }
                self.expect(&TokenKind::LBrace)?;
                let mut fields = Vec::new();
                let mut on_create = None;
                let mut acts = Vec::new();
                loop {
                    self.skip_newlines();
                    match self.peek().clone() {
                        TokenKind::RBrace | TokenKind::Eof => break,
                        TokenKind::Using => {
                            // using ParentName  (dentro del bloque del shape)
                            self.pos += 1;
                            // puede ser: using Parent  o  using [Parent1, Parent2]
                            if matches!(self.peek(), TokenKind::LBracket) {
                                self.pos += 1;
                                while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                                    using.push(self.expect_ident()?);
                                    if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
                                }
                                self.expect(&TokenKind::RBracket)?;
                            } else {
                                using.push(self.expect_ident()?);
                            }
                        }
                        TokenKind::OnCreate => {
                            self.pos += 1;
                            let params = self.parse_params()?;
                            let body = self.parse_block()?;
                            on_create = Some((params, body));
                        }
                        TokenKind::Act => {
                            self.pos += 1;
                            let act_name = self.expect_ident()?;
                            let params = self.parse_params()?;
                            if matches!(self.peek(), TokenKind::ThinArrow) {
                                self.pos += 1;
                                self.parse_type_name().ok();
                            }
                            let body = self.parse_block()?;
                            acts.push(ActDef { name: act_name, params, body });
                        }
                        TokenKind::Ident(_) => {
                            // campo: name [: type] = default  |  name: default_expr
                            let fname = self.expect_ident()?;
                            let mut type_hint = None;
                            let mut default = None;
                            if matches!(self.peek(), TokenKind::Colon) {
                                self.pos += 1;
                                // Si el siguiente token es un tipo Y el de después es '=' o newline/'}',
                                // interpretarlo como type_hint. De lo contrario, es un valor default.
                                let next_is_type = self.is_type_token();
                                let after_is_assign_or_end = matches!(
                                    self.tokens.get(self.pos + 1).map(|t| &t.kind).unwrap_or(&TokenKind::Eof),
                                    TokenKind::Assign | TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof | TokenKind::Comma
                                );
                                if next_is_type && after_is_assign_or_end {
                                    type_hint = Some(self.parse_type_name()?);
                                    if matches!(self.peek(), TokenKind::Assign) {
                                        self.pos += 1;
                                        default = Some(self.parse_expression()?);
                                    }
                                } else {
                                    // 'campo: valor_default' sin tipo explícito
                                    default = Some(self.parse_expression()?);
                                }
                            } else if matches!(self.peek(), TokenKind::Assign) {
                                self.pos += 1;
                                default = Some(self.parse_expression()?);
                            }
                            fields.push(FieldDef { name: fname, type_hint, default });
                        }
                        _ => { self.pos += 1; } // saltar tokens inesperados
                    }
                }
                self.expect(&TokenKind::RBrace)?;
                Ok(Stmt::Shape { name, type_params, fields, on_create, acts, using, line })
            }

            //   Asignación, asignación compuesta o expresión          
            _ => {
                let expr = self.parse_expression()?;

                // nombre: tipo [= expr]  — variable con anotación de tipo
                if let Expr::Ident(ref vname) = expr {
                    if matches!(self.peek(), TokenKind::Colon) {
                        let after_colon_is_type = {
                            let kind = self.tokens.get(self.pos + 1).map(|t| &t.kind).unwrap_or(&TokenKind::Eof);
                            matches!(kind,
                                TokenKind::TypeInt | TokenKind::TypeFloat | TokenKind::TypeBool |
                                TokenKind::TypeString | TokenKind::TypeList | TokenKind::TypeDict |
                                TokenKind::TypeAny | TokenKind::TypeAuto | TokenKind::Ident(_)
                            )
                        };
                        if after_colon_is_type {
                            let vname = vname.clone();
                            self.pos += 1; // consume ':'
                            let type_hint = self.parse_type_name()?;
                            let value = if matches!(self.peek(), TokenKind::Assign) {
                                self.pos += 1;
                                self.parse_expression()?
                            } else {
                                Expr::Null
                            };
                            return Ok(Stmt::TypedAssign { name: vname, type_hint, value, line });
                        }
                    }
                }

                // x = expr
                if matches!(self.peek(), TokenKind::Assign) {
                    self.pos += 1;
                    let value = self.parse_expression()?;
                    // Si el lado izquierdo es un simple Ident
                    if let Expr::Ident(name) = expr {
                        return Ok(Stmt::Assign { name, value, line });
                    }
                    // expr[i] = value
                    if let Expr::Index { object, index } = expr {
                        return Ok(Stmt::AssignIndex { object: *object, index: *index, value, line });
                    }
                    // expr.attr = value
                    if let Expr::AttrAccess { object, attr } = expr {
                        return Ok(Stmt::AssignAttr { object: *object, attr, value, line });
                    }
                    return Err(self.err("Objetivo de asignación inválido"));
                }

                // x += expr  |  x -= expr  |  etc.
                let aug_op = match self.peek() {
                    TokenKind::PlusEq      => Some("+"),
                    TokenKind::MinusEq     => Some("-"),
                    TokenKind::StarEq      => Some("*"),
                    TokenKind::SlashEq     => Some("/"),
                    TokenKind::PercentEq   => Some("%"),
                    TokenKind::StarStarEq  => Some("**"),
                    _ => None,
                };
                if let Some(op) = aug_op {
                    let op = op.to_string();
                    self.pos += 1;
                    let value = self.parse_expression()?;
                    if let Expr::Ident(name) = expr {
                        return Ok(Stmt::AugAssign { name, op, value, line });
                    }
                    return Err(self.err("Se esperaba un identificador en asignación compuesta"));
                }

                // await var = await future
                if let Expr::Await(inner) = expr {
                    return Ok(Stmt::Await { expr: *inner, var: None, line });
                }

                Ok(Stmt::Expr { expr, line })
            }
        }
    }
}

//   Tests                                   ─

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn parse_src(src: &str) -> Vec<Stmt> {
        let tokens = lex(src).expect("lex failed");
        parse(tokens).expect("parse failed")
    }

    #[test]
    fn test_assign() {
        let stmts = parse_src("x = 42");
        assert!(matches!(&stmts[0], Stmt::Assign { name, .. } if name == "x"));
    }

    #[test]
    fn test_show() {
        let stmts = parse_src(r#"show "hola""#);
        assert!(matches!(&stmts[0], Stmt::Show { .. }));
    }

    #[test]
    fn test_if_else() {
        let stmts = parse_src("if x > 0 { show x } else { show 0 }");
        assert!(matches!(&stmts[0], Stmt::If { .. }));
    }

    #[test]
    fn test_fn() {
        let stmts = parse_src("fn suma(a, b) { return a + b }");
        assert!(matches!(&stmts[0], Stmt::Fn { name, .. } if name == "suma"));
    }

    #[test]
    fn test_for_in() {
        let stmts = parse_src("for i in lista { show i }");
        assert!(matches!(&stmts[0], Stmt::For { var, .. } if var == "i"));
    }

    #[test]
    fn test_while() {
        let stmts = parse_src("while x < 10 { x = x + 1 }");
        assert!(matches!(&stmts[0], Stmt::While { .. }));
    }

    #[test]
    fn test_think() {
        let stmts = parse_src(r#"think "cuanto es 2+2""#);
        assert!(matches!(&stmts[0], Stmt::Think { .. }));
    }

    #[test]
    fn test_use() {
        let stmts = parse_src(r#"use "math""#);
        assert!(matches!(&stmts[0], Stmt::Use { path, .. } if path == "math"));
    }

    #[test]
    fn test_attempt_handle() {
        let stmts = parse_src("attempt { x = 1 } handle err { show err }");
        assert!(matches!(&stmts[0], Stmt::Attempt { handler: Some(_), .. }));
    }
}
