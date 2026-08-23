#![allow(dead_code)]
use crate::token::{Token, TokenKind};
use crate::ast::{ActDef, Expr, FieldDef, Handler, MatchArm, Param, Pattern, Stmt};

//   Error de parsing                              

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub line: u32,
    pub col: u32,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SyntaxError [line {}, col {}]: {}", self.line, self.col, self.message)
    }
}

//   Punto de entrada                              

pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>, ParseError> {
    let mut p = Parser::new(tokens);
    p.parse_program()
}

/// Une los argumentos de un `show` multi-argumento en una sola expresión
/// (`str(a) + " " + str(b)`), separados por espacio como el print de Python.
/// Con un solo argumento se devuelve tal cual (sin envolver en str()).
fn join_show_args(mut values: Vec<Expr>) -> Expr {
    if values.is_empty() { return Expr::Str(String::new()); }
    if values.len() == 1 { return values.pop().unwrap(); }
    let to_str = |v: Expr| Expr::Call {
        callee: Box::new(Expr::Ident("str".into())),
        args: vec![v],
        kwargs: Vec::new(),
    };
    let mut it = values.into_iter();
    let mut acc = to_str(it.next().unwrap());
    for v in it {
        let spaced = Expr::BinaryOp {
            op: "+".into(),
            left: Box::new(acc),
            right: Box::new(Expr::Str(" ".into())),
        };
        acc = Expr::BinaryOp {
            op: "+".into(),
            left: Box::new(spaced),
            right: Box::new(to_str(v)),
        };
    }
    acc
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

    fn current_col(&self) -> u32 {
        self.tokens.get(self.pos).map(|t| t.col).unwrap_or(0)
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
                message: format!("Expected {:?}, found {:?}", expected, self.peek()),
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
                message: format!("Expected an identifier, found {:?}", self.peek()),
                line, col,
            })
        }
    }

    /// Como expect_ident pero también acepta CUALQUIER keyword como nombre de
    /// atributo/método. Después de un `.` no hay ambigüedad sintáctica posible
    /// (igual que en Python/JS), así que reservar palabras ahí solo rompe APIs
    /// legítimas: `ai.ask()`, `fs.read()`, `random.int()`, `net.error`…
    /// Antes era una whitelist y cualquier keyword olvidada volvía inusable a
    /// la función del módulo.
    fn expect_attr_name(&mut self) -> Result<String, ParseError> {
        use crate::token::TokenKind::Ident;
        let name = match self.peek().clone() {
            Ident(n) => n,
            other => match other.keyword_text() {
                Some(kw) => kw.to_string(),
                None => {
                    let line = self.current_line();
                    let col = self.tokens.get(self.pos).map(|t| t.col).unwrap_or(0);
                    return Err(ParseError {
                        message: format!("Expected an identifier, found {:?}", self.peek()),
                        line, col,
                    });
                }
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

    //   Docstrings

    /// Consume líneas `/// texto` consecutivas y las une en un solo string.
    fn collect_doc(&mut self) -> Option<String> {
        let mut lines = Vec::new();
        while let TokenKind::DocComment(text) = self.peek().clone() {
            lines.push(text);
            self.pos += 1;
            // saltar semicolons/newlines entre líneas de doc
            while matches!(self.peek(), TokenKind::Semicolon) { self.pos += 1; }
        }
        if lines.is_empty() { None } else { Some(lines.join("\n")) }
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

    //   Parámetros de función                          

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
            _ => return Err(self.err("Expected a type")),
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
            // argumento con nombre: `ident = expr`
            if let TokenKind::Ident(name) = self.peek().clone() {
                if matches!(self.peek_at(1), TokenKind::Assign) {
                    self.pos += 2; // salta nombre y '='
                    let val = self.parse_expression()?;
                    kwargs.push((name, val));
                    if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
                    continue;
                }
            }
            // un posicional después de uno con nombre es ambiguo
            if !kwargs.is_empty() {
                let line = self.current_line();
                let col = self.tokens.get(self.pos).map(|t| t.col).unwrap_or(0);
                return Err(ParseError {
                    message: "a positional argument cannot follow a named one".to_string(),
                    line, col,
                });
            }
            args.push(self.parse_spreadable()?);
            if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
        }
        self.expect(&TokenKind::RParen)?;
        Ok((args, kwargs))
    }

    /// Un elemento que además admite `...expr` para expandir una lista.
    ///
    /// Solo se llama desde los dos sitios donde expandir significa algo: los
    /// literales de lista y los argumentos de una llamada. En el resto de
    /// posiciones se sigue parseando con `parse_expression`, así que un `...`
    /// suelto sigue siendo un error, que es lo correcto.
    ///
    /// Aquí `...` está en posición de OPERANDO. El `...` infijo (rango
    /// inclusivo) se resuelve en `parse_expression`, cuando ya hay una
    /// expresión a la izquierda. Los dos usos del símbolo no se cruzan porque
    /// se deciden en momentos distintos del parseo.
    fn parse_spreadable(&mut self) -> Result<Expr, ParseError> {
        if matches!(self.peek(), TokenKind::DotDotDot) {
            self.pos += 1;
            let inner = self.parse_expression()?;
            return Ok(Expr::Spread(Box::new(inner)));
        }
        self.parse_expression()
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

        // Rangos. Tres formas, dos semánticas:
        //
        //   1..4    exclusivo   [1,2,3]     forma corta de siempre
        //   1..<4   exclusivo   [1,2,3]     igual, pero dice en el símbolo dónde corta
        //   1...4   INCLUSIVO   [1,2,3,4]
        //
        // `..` se queda exclusivo porque ya lo era —el bucle compara con `Lt`— y
        // porque coincide con `range()`, que también excluye el extremo. Cambiarlo
        // habría alterado en silencio los bucles ya escritos, que es justo lo que
        // no se puede hacer. `..<` existe para quien prefiera no tener que
        // acordarse, y `...` cubre el caso que hasta ahora obligaba a escribir
        // `a..(b+1)`.
        //
        // Aquí `...` solo puede ser infijo: se llega con `expr` ya parseada. El
        // `...` prefijo (spread) se resuelve donde empieza un operando, así que
        // los dos usos del mismo símbolo no se cruzan nunca.
        let range_op = match self.peek() {
            TokenKind::DotDot    => Some(".."),
            TokenKind::DotDotLt  => Some("..<"),
            TokenKind::DotDotDot => Some("..."),
            _ => None,
        };
        if let Some(op) = range_op {
            self.pos += 1;
            let right = self.parse_or()?;
            expr = Expr::BinaryOp { op: op.into(), left: Box::new(expr), right: Box::new(right) };
        }

        // is-check: expr is ShapeName
        if matches!(self.peek(), TokenKind::Is) {
            self.pos += 1;
            let shape = self.expect_ident()?;
            expr = Expr::IsCheck { expr: Box::new(expr), shape };
        }

        // Ternario: cond ? si_si : si_no
        //
        // Va el último y asocia por la derecha, así que `a ? b : c ? d : e` se
        // lee `a ? b : (c ? d : e)`, que es la cadena else-if de toda la vida.
        // Las dos ramas se parsean con `parse_expression` completa: dentro de un
        // ternario cabe otro, un `|>` o un rango sin tener que poner paréntesis.
        if matches!(self.peek(), TokenKind::Question) {
            self.pos += 1;
            let then_e = self.parse_expression()?;
            self.expect(&TokenKind::Colon)?;
            let else_e = self.parse_expression()?;
            expr = Expr::Ternary {
                cond:   Box::new(expr),
                then_e: Box::new(then_e),
                else_e: Box::new(else_e),
            };
        }

        Ok(expr)
    }

    /// Inserta `value` como primer argumento del destino de un `|>`.
    ///
    /// Se admiten las cuatro formas que tienen un sitio natural donde meterlo:
    /// nombre suelto, llamada, método y lambda. Cualquier otra cosa —un número,
    /// una lista, un `a + b`— no es invocable, y se rechaza aquí con el sitio
    /// exacto en vez de dejar que el error salga mucho más abajo hablando de
    /// `__call__`, que no es nada que el programador haya escrito.
    fn pipe_into(value: Expr, stage: Expr, line: u32, col: u32) -> Result<Expr, ParseError> {
        let prepend = |args: Vec<Expr>| {
            let mut v = Vec::with_capacity(args.len() + 1);
            v.push(value.clone());
            v.extend(args);
            v
        };

        Ok(match stage {
            // x |> f
            Expr::Ident(_) => Expr::Call {
                callee: Box::new(stage),
                args:   vec![value],
                kwargs: Vec::new(),
            },

            // x |> f(a, b)   →  f(x, a, b)
            Expr::Call { callee, args, kwargs } => Expr::Call {
                callee,
                args: prepend(args),
                kwargs,
            },

            // x |> obj.act(a)  →  obj.act(x, a)
            Expr::CallMethod { method, receiver, args, kwargs } => Expr::CallMethod {
                method,
                receiver,
                args: prepend(args),
                kwargs,
            },

            // x |> mod.f  (sin paréntesis)  →  mod.f(x)
            Expr::AttrAccess { object, attr } => Expr::CallMethod {
                method:   attr,
                receiver: object,
                args:     vec![value],
                kwargs:   Vec::new(),
            },

            // x |> (n) => n * 2
            lam @ Expr::Lambda { .. } => Expr::Call {
                callee: Box::new(lam),
                args:   vec![value],
                kwargs: Vec::new(),
            },

            _ => {
                return Err(ParseError {
                    message: "a la derecha de '|>' hace falta algo invocable: \
                              un nombre de función, una llamada, un método o una lambda"
                        .to_string(),
                    line,
                    col,
                })
            }
        })
    }

    /// Parsea el patrón de un brazo de `match`.
    ///
    /// La forma se decide por el primer token, sin retroceder:
    ///
    ///   `_`            comodín
    ///   `[` ...        lista
    ///   `{` ...        dict          (la llave del CUERPO viene después)
    ///   `Ident` `(`    shape
    ///   `Ident`        ligadura
    ///   lo demás       valor, se compara por igualdad
    ///
    /// El shape va con paréntesis, `Forma(a, b)`, y no con llaves: `Forma {`
    /// seguido de la llave del cuerpo no se podría distinguir de una ligadura
    /// llamada `Forma` con su cuerpo detrás.
    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        match self.peek().clone() {
            TokenKind::LBracket => {
                self.pos += 1;
                let mut elems = Vec::new();
                while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                    elems.push(self.parse_pattern()?);
                    if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
                }
                self.expect(&TokenKind::RBracket)?;
                Ok(Pattern::List(elems))
            }

            TokenKind::LBrace => {
                self.pos += 1;
                let fields = self.parse_pattern_fields(&TokenKind::RBrace)?;
                self.expect(&TokenKind::RBrace)?;
                Ok(Pattern::Dict(fields))
            }

            TokenKind::Ident(name) => {
                // `Forma(...)` → shape.  `_` → comodín.  Lo demás → ligadura.
                if matches!(self.peek_at(1), TokenKind::LParen) {
                    self.pos += 2; // nombre y '('
                    let fields = self.parse_pattern_fields(&TokenKind::RParen)?;
                    self.expect(&TokenKind::RParen)?;
                    return Ok(Pattern::Shape { name, fields });
                }
                self.pos += 1;
                if name == "_" {
                    Ok(Pattern::Wildcard)
                } else {
                    Ok(Pattern::Bind(name))
                }
            }

            _ => Ok(Pattern::Value(self.parse_expression()?)),
        }
    }

    /// Campos de un patrón de dict o de shape, hasta `cierre`.
    ///
    /// Dos formas por campo: `clave: patrón` y la abreviatura `clave`, que
    /// significa `clave: clave` — el caso corriente de "sácame ese campo con su
    /// propio nombre" sin tener que escribirlo dos veces.
    fn parse_pattern_fields(
        &mut self,
        cierre: &TokenKind,
    ) -> Result<Vec<(String, Pattern)>, ParseError> {
        let mut fields = Vec::new();
        while !matches!(self.peek(), TokenKind::Eof) && self.peek() != cierre {
            let key = match self.peek().clone() {
                TokenKind::Ident(k) => { self.pos += 1; k }
                TokenKind::Str(k)   => { self.pos += 1; k }
                _ => return Err(self.err(
                    "expected a field name in the pattern".to_string(),
                )),
            };
            let pat = if matches!(self.peek(), TokenKind::Colon) {
                self.pos += 1;
                self.parse_pattern()?
            } else {
                Pattern::Bind(key.clone())
            };
            fields.push((key, pat));
            if matches!(self.peek(), TokenKind::Comma) { self.pos += 1; }
        }
        Ok(fields)
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
            let line = self.current_line();
            let col = self.current_col();
            vec![Stmt::Expr { expr, line, col }]
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
        let mut left = self.parse_bit_or()?;
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
            let right = self.parse_bit_or()?;
            left = Expr::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    // Bit a bit, en tres niveles: `|` más suelto que `^`, y `^` más que `&`.
    //
    // Van por DEBAJO de la comparación, no por encima como en C. En C
    // `a & b == c` significa `a & (b == c)`, que sorprende a todo el mundo y es
    // un error tan clásico que los compiladores avisan de él. Aquí se agrupa
    // como se lee: `(a & b) == c`.

    fn parse_bit_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bit_xor()?;
        while matches!(self.peek(), TokenKind::Pipe) {
            self.pos += 1;
            let right = self.parse_bit_xor()?;
            left = Expr::BinaryOp { op: "|".into(), left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_bit_xor(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bit_and()?;
        while matches!(self.peek(), TokenKind::Caret) {
            self.pos += 1;
            let right = self.parse_bit_and()?;
            left = Expr::BinaryOp { op: "^".into(), left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_bit_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_shift()?;
        while matches!(self.peek(), TokenKind::Ampersand) {
            self.pos += 1;
            let right = self.parse_shift()?;
            left = Expr::BinaryOp { op: "&".into(), left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    /// Desplazamientos. Más apretados que los otros operadores de bits y más
    /// sueltos que la aritmética, así que `a << 2 + 1` es `a << 3`.
    fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pipe()?;
        loop {
            let op = match self.peek() {
                TokenKind::Shl => "<<",
                TokenKind::Shr => ">>",
                _ => break,
            }.to_string();
            self.pos += 1;
            let right = self.parse_pipe()?;
            left = Expr::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    /// Pipe: `valor |> destino` mete el valor como PRIMER argumento del destino.
    ///
    ///   x |> f            =>  f(x)
    ///   x |> f(a, b)      =>  f(x, a, b)
    ///   x |> mod.act(a)   =>  mod.act(x, a)
    ///
    /// Es azúcar puro de parser: sale la misma `Call` que se habría escrito a
    /// mano, así que VM, JIT, AOT y typechecker lo tratan sin enterarse.
    ///
    /// Vive entre la comparación y la aritmética, y esa posición es la que hace
    /// que se lea como se espera en los dos casos que importan:
    ///
    ///   a + b |> f   =>  f(a + b)      (la suma entra entera)
    ///   x |> len > 3 =>  (x |> len) > 3
    ///
    /// Si estuviera más abajo, la segunda intentaría invocar `len > 3`, que no
    /// es invocable, y habría que poner paréntesis para algo que se lee solo.
    fn parse_pipe(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_arith()?;
        while matches!(self.peek(), TokenKind::PipeOp) {
            self.pos += 1;
            let line = self.current_line();
            let col  = self.tokens.get(self.pos).map(|t| t.col).unwrap_or(0);
            // Una lambda como etapa (`x |> (n) => n * 2`) hay que reconocerla
            // aquí: los niveles de precedencia no miran si viene una, eso solo
            // se comprueba al entrar a una expresión, y sin esto el `=>`
            // reventaba contra el paréntesis de los parámetros.
            let stage = if self.is_lambda_ahead() {
                self.parse_lambda()?
            } else {
                self.parse_arith()?
            };
            expr = Self::pipe_into(expr, stage, line, col)?;
        }
        Ok(expr)
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
                // Acceso estático: `Shape::act`.
                //
                // Se resuelve a un identificador con el nombre ya compuesto,
                // "Shape::act", que es como codegen registra los acts estáticos.
                // A partir de aquí es un nombre de función corriente: la llamada
                // la monta el brazo de abajo y ni la VM ni el JIT necesitan
                // saber que esto vino de un shape. El nombre no puede chocar con
                // ninguno del usuario porque el lexer nunca mete `::` dentro de
                // un identificador.
                TokenKind::DoubleColon => {
                    self.pos += 1;
                    let member = self.expect_ident()?;
                    let shape = match &expr {
                        Expr::Ident(n) => n.clone(),
                        _ => return Err(self.err(
                            "'::' goes on a shape name: Shape::act(...)".to_string(),
                        )),
                    };
                    expr = Expr::Ident(format!("{shape}::{member}"));
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
                // Si el nombre de tipo va seguido de `.` es acceso a miembro sobre una
                // variable (p.ej. un namespace de módulo importado como `list`/`dict`),
                // no un cast. Resolver como identificador en minúscula para no chocar
                // con los nombres de tipo `List`/`Dict`.
                if matches!(self.peek(), TokenKind::Dot) {
                    Ok(Expr::Ident(name.to_lowercase()))
                } else {
                    Ok(Expr::Ident(name))
                }
            }

            // Paréntesis agrupados
            TokenKind::LParen => {
                self.pos += 1;
                let expr = self.parse_expression()?;
                self.expect(&TokenKind::RParen)?;
                Ok(expr)
            }

            // Lista: [a, b, c]  y con expansión: [a, ...otra, c]
            TokenKind::LBracket => {
                self.pos += 1;
                let mut elems = Vec::new();
                while !matches!(self.peek(), TokenKind::RBracket | TokenKind::Eof) {
                    elems.push(self.parse_spreadable()?);
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
                        _ => return Err(self.err("Expected a dictionary key")),
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

            // super — usado como super.metodo(args); el postfix lo convierte en
            // CallMethod con receiver Ident("super"), que codegen traduce a CallSuper.
            TokenKind::Super => {
                self.pos += 1;
                Ok(Expr::Ident("super".to_string()))
            }

            // await como expresión: result = await future
            // Usamos parse_postfix (no parse_primary) para capturar la llamada
            // completa: `await f(x)` = Await(Call f), no Await(Ident f) seguido
            // de una llamada `(x)` sobre el resultado (que daba "__call__").
            TokenKind::Await => {
                self.pos += 1;
                let inner = self.parse_postfix()?;
                Ok(Expr::Await(Box::new(inner)))
            }

            kind => Err(self.err(format!("Unexpected token in expression: {:?}", kind))),
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // DECLARACIONES
    // ══════════════════════════════════════════════════════════════════════════

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        self.skip_newlines();
        // collect any leading docstrings before the actual statement
        let doc = self.collect_doc();
        self.skip_newlines();
        let line = self.current_line();
        let col  = self.current_col();
        match self.peek().clone() {

            //   const x = expr
            TokenKind::Const => {
                self.pos += 1;
                let name = self.expect_ident()?;
                self.expect(&TokenKind::Assign)?;
                let value = self.parse_expression()?;
                Ok(Stmt::Const { name, value, doc, line, col })
            }

            //   show expr[, expr...]  |  show(expr, expr...)
            // Multi-argumento estilo print de Python: se desugara a
            // str(a) + " " + str(b) para no tocar VM/JIT (la instrucción
            // Show sigue recibiendo UNA expresión).
            TokenKind::Show => {
                self.pos += 1;
                let mut values: Vec<Expr> = Vec::new();
                let save = self.pos;
                match self.parse_expression() {
                    Ok(first) => {
                        values.push(first);
                        while matches!(self.peek(), TokenKind::Comma) {
                            self.pos += 1;
                            values.push(self.parse_expression()?);
                        }
                    }
                    Err(e) => {
                        // `show("a: ", x)`: el grupo parentizado aborta en la
                        // coma → reintentar como lista de argumentos de llamada.
                        if !matches!(self.tokens.get(save).map(|t| &t.kind), Some(TokenKind::LParen)) {
                            return Err(e);
                        }
                        self.pos = save + 1; // saltar LParen
                        if !matches!(self.peek(), TokenKind::RParen) {
                            values.push(self.parse_expression()?);
                            while matches!(self.peek(), TokenKind::Comma) {
                                self.pos += 1;
                                values.push(self.parse_expression()?);
                            }
                        }
                        self.expect(&TokenKind::RParen)?;
                    }
                }
                Ok(Stmt::Show { value: join_show_args(values), line, col })
            }

            //   return [expr]
            TokenKind::Return => {
                self.pos += 1;
                let value = if !matches!(self.peek(), TokenKind::RBrace | TokenKind::Semicolon | TokenKind::Eof) {
                    Some(self.parse_expression()?)
                } else { None };
                Ok(Stmt::Return { value, line, col })
            }

            //   break
            TokenKind::Break    => { self.pos += 1; Ok(Stmt::Break { line, col }) }

            //   continue
            TokenKind::Continue => { self.pos += 1; Ok(Stmt::Continue { line, col }) }

            //   extern fn name(params) -> ret from "lib"
            TokenKind::Extern => {
                self.pos += 1; // extern
                self.expect(&TokenKind::Fn)?;
                let name = self.expect_ident()?;
                let params = self.parse_params()?;
                let ret_type = if matches!(self.peek(), TokenKind::ThinArrow) {
                    self.pos += 1;
                    self.parse_type_name().ok()
                } else { None };
                // from "lib_name"
                let lib = if let TokenKind::Ident(kw) = self.peek().clone() {
                    if kw == "from" {
                        self.pos += 1;
                        if let TokenKind::Str(lib_name) = self.peek().clone() {
                            self.pos += 1;
                            lib_name
                        } else {
                            return Err(self.err("Expected a library name as a string after 'from'"));
                        }
                    } else { String::new() }
                } else { String::new() };
                Ok(Stmt::ExternFn { name, params, ret_type, lib, line, col })
            }

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
                Ok(Stmt::Fn { name, type_params, params, body, ret_type, doc, line, col })
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
                Ok(Stmt::AsyncFn { name, type_params, params, body, ret_type, doc, line, col })
            }

            //   if cond { } [else { }]
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
                Ok(Stmt::If { cond, then_body, else_body, line, col })
            }

            //   while cond { }
            TokenKind::While => {
                self.pos += 1;
                let cond = self.parse_expression()?;
                let body = self.parse_block()?;
                Ok(Stmt::While { cond, body, line, col })
            }

            //   for var in expr { }
            TokenKind::For => {
                self.pos += 1;
                let var = self.expect_ident()?;
                self.expect(&TokenKind::In)?;
                let iter = self.parse_expression()?;
                let body = self.parse_block()?;
                Ok(Stmt::For { var, iter, body, line, col })
            }

            //   match expr { pattern { } ... }
            TokenKind::Match => {
                self.pos += 1;
                let expr = self.parse_expression()?;
                self.expect(&TokenKind::LBrace)?;
                let mut arms = Vec::new();
                loop {
                    self.skip_newlines();
                    if matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) { break; }
                    let pattern = self.parse_pattern()?;
                    // Guarda opcional: `patrón if condición { ... }`
                    let guard = if matches!(self.peek(), TokenKind::If) {
                        self.pos += 1;
                        Some(self.parse_expression()?)
                    } else {
                        None
                    };
                    let body = self.parse_block()?;
                    arms.push(MatchArm { pattern, guard, body });
                }
                self.expect(&TokenKind::RBrace)?;
                Ok(Stmt::Match { expr, arms, line, col })
            }

            //   use "path" [as alias] [take [fn1, fn2]]
            TokenKind::Use => {
                self.pos += 1;
                let path = match self.peek().clone() {
                    TokenKind::Str(s)   => { self.pos += 1; s }
                    TokenKind::Ident(n) => { self.pos += 1; n }
                    _ => return Err(self.err("Expected a module path after 'use'")),
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
                Ok(Stmt::Use { path, alias, selective, line, col })
            }

            //   attempt { } handle err { }
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
                Ok(Stmt::Attempt { body, handler, line, col })
            }

            //   with h = modulo.abrir(...) { ... }  — recurso con ámbito:
            //   libera con modulo.free(h) al salir, también si hay error.
            TokenKind::With => {
                self.pos += 1;
                let var = self.expect_ident()?;
                self.expect(&TokenKind::Assign)?;
                let init = self.parse_expression()?;
                // El init debe ser `ident.fn(...)`: es lo que le dice al
                // compilador QUÉ free llamar (modulo.free). Sin esa forma no
                // hay módulo dueño conocido y la liberación sería adivinanza.
                match &init {
                    Expr::CallMethod { receiver, .. }
                        if matches!(receiver.as_ref(), Expr::Ident(_)) => {}
                    _ => return Err(ParseError {
                        message: "with expects a module resource: `with h = module.open(...) { ... }` (the block releases it with module.free(h))".into(),
                        line, col,
                    }),
                }
                let body = self.parse_block()?;
                // La garantía de with es "el recurso SIEMPRE se libera al
                // salir del bloque". return salta directo fuera de la función
                // y break/continue fuera del bloque, saltándose el free — se
                // rechazan en vez de fugar en silencio.
                validate_with_body(&body, 0)?;
                Ok(Stmt::With { var, init, body, line, col })
            }

            //   error expr
            TokenKind::ErrorKw => {
                self.pos += 1;
                let msg = self.parse_expression()?;
                Ok(Stmt::ErrorStmt { msg, line, col })
            }

            //   think expr
            TokenKind::Think => {
                self.pos += 1;
                let prompt = self.parse_expression()?;
                Ok(Stmt::Think { prompt, line, col })
            }

            //   learn expr
            TokenKind::Learn => {
                self.pos += 1;
                let text = self.parse_expression()?;
                Ok(Stmt::Learn { text, line, col })
            }

            //   sense expr
            TokenKind::Sense => {
                self.pos += 1;
                let query = self.parse_expression()?;
                Ok(Stmt::Sense { query, line, col })
            }

            //   spawn expr
            TokenKind::Spawn => {
                self.pos += 1;
                let call = self.parse_expression()?;
                Ok(Stmt::Spawn { call, line, col })
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
                Ok(Stmt::Ask { prompt, var, cast, choices, line, col })
            }

            //   read "path" [as type] -> var
            TokenKind::Read => {
                self.pos += 1;
                let path = self.parse_expression()?;
                if matches!(self.peek(), TokenKind::As) {
                    self.pos += 1;
                    self.parse_type_name().ok(); // consumir tipo (ignorado por ahora)
                }
                self.expect(&TokenKind::ThinArrow)?;
                let var = self.expect_ident()?;
                Ok(Stmt::Read { path, var, line, col })
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
                Ok(Stmt::Write { path, content, line, col })
            }

            //   append "path" with expr
            TokenKind::Append => {
                self.pos += 1;
                let path = self.parse_expression()?;
                self.expect(&TokenKind::With)?;
                let content = self.parse_expression()?;
                Ok(Stmt::Append { path, content, line, col })
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
                    vec![Stmt::Expr { expr: fn_expr, line, col }]
                };
                Ok(Stmt::Serve { port, routes, line, col })
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
                let mut on_error = None;
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
                        TokenKind::OnError => {
                            self.pos += 1;
                            // on_error(err) { ... }  — err recibe el mensaje del error
                            let params = self.parse_params()?;
                            let body = self.parse_block()?;
                            on_error = Some((params, body));
                        }
                        // `act` normal o `static act`. El único cambio es que el
                        // estático no recibe instancia; el resto se parsea igual.
                        //
                        // `static` se reconoce AQUÍ, y solo si le sigue un `act`.
                        // Fuera de esa posición sigue siendo un identificador
                        // como cualquier otro, así que `router.static(...)` y
                        // una variable llamada `static` siguen funcionando.
                        TokenKind::Act | TokenKind::Ident(_)
                            if matches!(self.peek(), TokenKind::Act)
                                || (matches!(self.peek(), TokenKind::Ident(n) if n == "static")
                                    && matches!(self.peek_at(1), TokenKind::Act)) =>
                        {
                            let is_static = !matches!(self.peek(), TokenKind::Act);
                            if is_static {
                                self.pos += 1; // 'static'
                            }
                            self.pos += 1;
                            let act_name = self.expect_ident()?;
                            let params = self.parse_params()?;
                            let ret_type = if matches!(self.peek(), TokenKind::ThinArrow) {
                                self.pos += 1;
                                self.parse_type_name().ok()
                            } else {
                                None
                            };
                            let body = self.parse_block()?;
                            acts.push(ActDef { name: act_name, params, ret_type, body, is_static });
                        }
                        TokenKind::Ident(_) => {
                            // campo: name [: type] [= default]  |  name: default_expr
                            let fname = self.expect_ident()?;
                            let mut type_hint = None;
                            let mut default = None;
                            if matches!(self.peek(), TokenKind::Colon) {
                                self.pos += 1; // consume ':'
                                // Distinción: tipo vs valor default.
                                // El lexer no emite tokens de newline, así que después de un tipo
                                // puede venir directamente el siguiente Ident (campo/act/using).
                                // Primitivos (int, string, bool…) son siempre tipos.
                                // Para Ident (shapes de usuario), verificamos que lo siguiente
                                // sea inicio de nueva declaración o '='.
                                let is_primitive_type = matches!(self.peek(),
                                    TokenKind::TypeInt | TokenKind::TypeFloat | TokenKind::TypeBool |
                                    TokenKind::TypeString | TokenKind::TypeList | TokenKind::TypeDict |
                                    TokenKind::TypeAny | TokenKind::TypeAuto
                                );
                                let is_ident_type = matches!(self.peek(), TokenKind::Ident(_));
                                if is_primitive_type {
                                    type_hint = Some(self.parse_type_name()?);
                                    if matches!(self.peek(), TokenKind::Assign) {
                                        self.pos += 1;
                                        default = Some(self.parse_expression()?);
                                    }
                                } else if is_ident_type {
                                    // Ident es tipo si lo que sigue es inicio de nueva
                                    // declaración (campo, act, on_create, using, '}') o '='/','
                                    let after = self.tokens.get(self.pos + 1)
                                        .map(|t| &t.kind).unwrap_or(&TokenKind::Eof);
                                    let after_is_decl = matches!(after,
                                        TokenKind::Assign | TokenKind::Semicolon |
                                        TokenKind::RBrace | TokenKind::Eof | TokenKind::Comma |
                                        TokenKind::Ident(_) | TokenKind::Act |
                                        TokenKind::OnCreate | TokenKind::OnError | TokenKind::Using |
                                        TokenKind::LBracket
                                    );
                                    if after_is_decl {
                                        type_hint = Some(self.parse_type_name()?);
                                        if matches!(self.peek(), TokenKind::Assign) {
                                            self.pos += 1;
                                            default = Some(self.parse_expression()?);
                                        }
                                    } else {
                                        default = Some(self.parse_expression()?);
                                    }
                                } else {
                                    // Literal (Bool, Int, Float, Str, Null…): valor default
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
                Ok(Stmt::Shape { name, type_params, fields, on_create, on_error, acts, using, doc, line, col })
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
                            return Ok(Stmt::TypedAssign { name: vname, type_hint, value, line, col });
                        }
                    }
                }

                // x = expr
                if matches!(self.peek(), TokenKind::Assign) {
                    self.pos += 1;
                    let value = self.parse_expression()?;
                    if let Expr::Ident(name) = expr {
                        return Ok(Stmt::Assign { name, value, line, col });
                    }
                    if let Expr::Index { object, index } = expr {
                        return Ok(Stmt::AssignIndex { object: *object, index: *index, value, line, col });
                    }
                    if let Expr::AttrAccess { object, attr } = expr {
                        return Ok(Stmt::AssignAttr { object: *object, attr, value, line, col });
                    }
                    return Err(self.err("Invalid assignment target"));
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
                        return Ok(Stmt::AugAssign { name, op, value, line, col });
                    }
                    return Err(self.err("Expected an identifier in compound assignment"));
                }

                // await var = await future
                if let Expr::Await(inner) = expr {
                    return Ok(Stmt::Await { expr: *inner, var: None, line, col });
                }

                Ok(Stmt::Expr { expr, line, col })
            }
        }
    }
}

//   Validación del cuerpo de `with`

/// La garantía de `with` es que el recurso se libera SIEMPRE al salir del
/// bloque. `return` (sale de la función) y `break`/`continue` (salen del
/// bloque hacia un loop exterior) esquivarían el free — se rechazan con un
/// error claro en vez de fugar en silencio. Los loops DENTRO del cuerpo sí
/// pueden usar break/continue (saltan dentro del bloque), y las funciones
/// anidadas (fn/lambda/shape) son ámbitos nuevos: no se recorren.
fn validate_with_body(body: &[Stmt], loop_depth: usize) -> Result<(), ParseError> {
    for s in body {
        match s {
            Stmt::Return { line, col, .. } => {
                return Err(ParseError {
                    message: "return inside `with` would skip releasing the resource; assign the result to a variable and return after the block".into(),
                    line: *line, col: *col,
                });
            }
            Stmt::Break { line, col } | Stmt::Continue { line, col } if loop_depth == 0 => {
                return Err(ParseError {
                    message: "break/continue inside `with` would jump out of the block without releasing the resource; leave the loop after the block".into(),
                    line: *line, col: *col,
                });
            }
            Stmt::While { body, .. } | Stmt::For { body, .. } => {
                validate_with_body(body, loop_depth + 1)?;
            }
            Stmt::If { then_body, else_body, .. } => {
                validate_with_body(then_body, loop_depth)?;
                validate_with_body(else_body, loop_depth)?;
            }
            Stmt::Match { arms, .. } => {
                for arm in arms { validate_with_body(&arm.body, loop_depth)?; }
            }
            Stmt::Attempt { body, handler, .. } => {
                validate_with_body(body, loop_depth)?;
                if let Some(h) = handler { validate_with_body(&h.body, loop_depth)?; }
            }
            Stmt::With { body, .. } => {
                // el with anidado ya validó su propio cuerpo al parsearse,
                // pero respecto al with EXTERIOR aplican las mismas reglas
                validate_with_body(body, loop_depth)?;
            }
            _ => {}
        }
    }
    Ok(())
}

//   Tests

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
    fn test_show_multi_arg() {
        // Estilo llamada: show("a: ", x) — antes fallaba con "Expected ')'"
        let stmts = parse_src(r#"show("total: ", x)"#);
        assert!(matches!(&stmts[0], Stmt::Show { value: Expr::BinaryOp { .. }, .. }));

        // Estilo bare: show a, b, c
        let stmts = parse_src("show a, b, c");
        assert!(matches!(&stmts[0], Stmt::Show { value: Expr::BinaryOp { .. }, .. }));

        // Un solo argumento sigue intacto (sin envolver en str())
        let stmts = parse_src("show(42)");
        assert!(matches!(&stmts[0], Stmt::Show { value: Expr::Int(42), .. }));
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

    #[test]
    fn test_with_parsea() {
        let stmts = parse_src("with f = frame.open(\"d.csv\") { k = frame.count(f) }");
        match &stmts[0] {
            Stmt::With { var, init, body, .. } => {
                assert_eq!(var, "f");
                assert!(matches!(init, Expr::CallMethod { .. }));
                assert_eq!(body.len(), 1);
            }
            other => panic!("se esperaba Stmt::With, dio {:?}", other),
        }
    }

    #[test]
    fn test_with_rechaza_init_no_modulo() {
        let tokens = crate::lexer::lex("with h = [1, 2] { show h }").unwrap();
        let err = parse(tokens).expect_err("init sin modulo.fn(...) debe fallar");
        assert!(err.message.contains("module resource"));
    }

    #[test]
    fn test_with_permite_break_en_loop_interno() {
        // el break vive en un loop DEL cuerpo: legal
        let stmts = parse_src(
            "with f = m.abrir(1) { while yes { break } }");
        assert!(matches!(&stmts[0], Stmt::With { .. }));
    }
}
