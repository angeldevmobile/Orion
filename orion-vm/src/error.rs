/// error.rs — Sistema de errores estructurados de Orion
///
/// Reemplaza los errores tipo String dispersos por OrionError con span,
/// que permite renderizar errores con contexto visual del código fuente.

use serde::Serialize;

// ─── Tipos públicos ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Span {
    pub line: u32,
    pub col:  u32,
    pub len:  u32,
}

impl Span {
    pub fn new(line: u32, col: u32) -> Self {
        Span { line, col, len: 1 }
    }

    pub fn with_len(line: u32, col: u32, len: u32) -> Self {
        Span { line, col, len }
    }

    pub fn unknown() -> Self {
        Span { line: 0, col: 0, len: 0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorKind {
    Lexer,
    Parse,
    Codegen,
    Type,
    Runtime,
}

impl ErrorKind {
    fn label(&self) -> &str {
        match self {
            ErrorKind::Lexer   => "error léxico",
            ErrorKind::Parse   => "error sintáctico",
            ErrorKind::Codegen => "error de compilación",
            ErrorKind::Type    => "error de tipos",
            ErrorKind::Runtime => "error en ejecución",
        }
    }

    fn color(&self) -> &str {
        match self {
            ErrorKind::Runtime => "\x1b[31;1m",
            _                  => "\x1b[33;1m",
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrionError {
    pub kind:   ErrorKind,
    pub message: String,
    pub span:   Span,
    pub hint:   Option<String>,
    pub file:   String,
    /// Líneas adicionales del stack trace (VM)
    pub frames: Vec<String>,
}

impl OrionError {
    pub fn new(kind: ErrorKind, message: impl Into<String>, span: Span) -> Self {
        OrionError {
            kind,
            message: message.into(),
            span,
            hint:   None,
            file:   String::new(),
            frames: Vec::new(),
        }
    }

    pub fn runtime(message: impl Into<String>, line: u32) -> Self {
        OrionError::new(ErrorKind::Runtime, message, Span::new(line, 1))
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = file.into();
        self
    }

    pub fn with_frames(mut self, frames: Vec<String>) -> Self {
        self.frames = frames;
        self
    }

    /// Renderiza el error con contexto visual del código fuente.
    ///
    /// Produce output estilo Rust/Elm:
    ///
    /// ```text
    ///   error sintáctico  →  main.orx:5:3
    ///   Se esperaba '}' pero encontró 'else'
    ///
    ///    |
    ///  5 │ if x > 0 {
    ///    │           ^ aquí
    ///
    ///   = ayuda: cierra el bloque con '}'
    /// ```
    pub fn render(&self, source: &str) -> String {
        const RED:    &str = "\x1b[31;1m";
        const YELLOW: &str = "\x1b[33;1m";
        const CYAN:   &str = "\x1b[36m";
        const DIM:    &str = "\x1b[90m";
        const BOLD:   &str = "\x1b[1m";
        const RST:    &str = "\x1b[0m";

        let _ = YELLOW;
        let color = self.kind.color();
        let label = self.kind.label();

        let mut out = String::new();
        out.push('\n');

        // ── Cabecera ──────────────────────────────────────────────────────────
        out.push_str(&format!("  {color}{BOLD}{label}{RST}"));
        if !self.file.is_empty() && self.span.line > 0 {
            out.push_str(&format!(
                "  {DIM}→  {}:{}:{}{RST}",
                self.file, self.span.line, self.span.col
            ));
        } else if !self.file.is_empty() {
            out.push_str(&format!("  {DIM}→  {}{RST}", self.file));
        }
        out.push('\n');

        // ── Mensaje principal ─────────────────────────────────────────────────
        out.push_str(&format!("  {BOLD}{}{RST}\n", self.message));

        // ── Fragmento del código fuente ───────────────────────────────────────
        if self.span.line > 0 && !source.is_empty() {
            let source_lines: Vec<&str> = source.lines().collect();
            let line_idx = (self.span.line as usize).saturating_sub(1);

            if let Some(src_line) = source_lines.get(line_idx) {
                let line_num_str = self.span.line.to_string();
                let pad = " ".repeat(line_num_str.len());

                out.push('\n');
                out.push_str(&format!("  {DIM}{pad} │{RST}\n"));
                out.push_str(&format!(
                    "  {DIM}{line_num_str} │{RST} {CYAN}{src_line}{RST}\n"
                ));

                // Puntero ^^^^^^^^^
                let col = (self.span.col as usize).saturating_sub(1);
                let len = (self.span.len as usize).max(1);
                let pointer = format!("{}{}", " ".repeat(col), "^".repeat(len));
                out.push_str(&format!(
                    "  {DIM}{pad} │{RST} {RED}{BOLD}{pointer}{RST}\n"
                ));
            }
        }

        // ── Stack frames (errores de runtime) ────────────────────────────────
        if !self.frames.is_empty() {
            out.push('\n');
            for frame in &self.frames {
                out.push_str(&format!("  {DIM}{frame}{RST}\n"));
            }
        }

        // ── Hint ──────────────────────────────────────────────────────────────
        if let Some(hint) = &self.hint {
            out.push('\n');
            out.push_str(&format!("  {YELLOW}={RST} {YELLOW}ayuda:{RST} {hint}\n"));
        }

        out.push('\n');
        out
    }
}

// ─── Conversiones desde tipos de error existentes ────────────────────────────

impl From<crate::token::LexError> for OrionError {
    fn from(e: crate::token::LexError) -> Self {
        OrionError::new(
            ErrorKind::Lexer,
            e.message,
            Span::new(e.line, e.col),
        )
    }
}

impl From<crate::parser::ParseError> for OrionError {
    fn from(e: crate::parser::ParseError) -> Self {
        OrionError::new(
            ErrorKind::Parse,
            e.message,
            Span::new(e.line, e.col),
        )
    }
}

impl From<crate::codegen::CodegenError> for OrionError {
    fn from(e: crate::codegen::CodegenError) -> Self {
        OrionError::new(
            ErrorKind::Codegen,
            e.message,
            Span::new(e.line, 1),
        )
    }
}

// ─── Parseo de errores del VM (que vienen como String) ───────────────────────

/// Convierte el string de error del VM en un OrionError estructurado.
///
/// El VM produce strings con formato:
///   "Linea 5 | mensaje del error\n    en foo (linea 3)\n    en main (linea 1)"
pub fn parse_vm_error(raw: &str, file: &str) -> OrionError {
    let mut lines = raw.lines();
    let first = lines.next().unwrap_or(raw);

    // Intentar extraer "Linea N | mensaje"
    let (line_num, message) = if let Some(rest) = first.strip_prefix("Linea ") {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let after_digits = &rest[digits.len()..];
        let msg = after_digits
            .strip_prefix(" | ")
            .unwrap_or(after_digits)
            .trim()
            .to_string();
        let n: u32 = digits.parse().unwrap_or(0);
        (n, msg)
    } else {
        (0, first.to_string())
    };

    // Resto son stack frames
    let frames: Vec<String> = lines
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    OrionError::new(ErrorKind::Runtime, message, Span::new(line_num, 1))
        .with_file(file)
        .with_frames(frames)
}

// ─── Tipos serializables para --check-json (LSP diagnostics) ─────────────────

/// Un diagnóstico en formato LSP-compatible, serializable a JSON.
#[derive(Serialize)]
pub struct LspDiagnostic {
    /// 1 = Error, 2 = Warning, 3 = Info
    pub severity: u8,
    pub kind:    String,
    pub message: String,
    pub line:    u32,
    pub col:     u32,
    pub len:     u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint:    Option<String>,
}

/// Resultado completo de `--check-json`.
#[derive(Serialize)]
pub struct CheckResult {
    pub ok:          bool,
    pub diagnostics: Vec<LspDiagnostic>,
}

impl OrionError {
    pub fn to_lsp_diagnostic(&self) -> LspDiagnostic {
        LspDiagnostic {
            severity: match self.kind {
                ErrorKind::Type => 2,
                _               => 1,
            },
            kind:    format!("{:?}", self.kind),
            message: self.message.clone(),
            line:    self.span.line,
            col:     self.span.col,
            len:     self.span.len.max(1),
            hint:    self.hint.clone(),
        }
    }
}

/// Convierte un TypeIssue del typechecker a LspDiagnostic.
pub fn type_issue_to_lsp(issue: &crate::typechecker::TypeIssue) -> LspDiagnostic {
    LspDiagnostic {
        severity: if issue.kind == "error" { 1 } else { 2 },
        kind:    "Type".into(),
        message: issue.message.clone(),
        line:    issue.line,
        col:     1,
        len:     1,
        hint:    None,
    }
}
