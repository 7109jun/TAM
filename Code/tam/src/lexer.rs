//! Lexer. 줄바꿈은 문장 종료로 취급하며, 이어지는 줄은 여기서 걸러낸다.
//!
//! 줄바꿈 규칙:
//! - 줄 끝이 이항 연산자, `,`, `(`, `[`, `{` 등이면 다음 줄로 이어진다.
//! - 다음 줄이 `.`, `)`, `]`로 시작하면 이어진다.
//! - 그 외에는 `Newline` 토큰이 문장의 끝이 된다.

use crate::diag::{Diag, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Int(i128),
    Float(f64),
    Str(String),
    // 키워드
    Fn,
    Struct,
    As,
    Let,
    Mut,
    Return,
    If,
    Else,
    While,
    Break,
    Continue,
    For,
    In,
    Match,
    Extern,
    Use,
    Impl,
    Pub,
    Unsafe,
    Const,
    True,
    False,
    // 구분자
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    ColonColon,
    Question,
    FatArrow,
    Semi,
    Dot,
    DotDot,
    // 연산자
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Assign,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Bang,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    DotDotEq,
    Newline,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

impl Tok {
    /// 이 토큰으로 줄이 끝나면 다음 줄로 이어지는가.
    fn continues_line(&self) -> bool {
        matches!(
            self,
            Tok::Plus
                | Tok::Minus
                | Tok::Star
                | Tok::Slash
                | Tok::Percent
                | Tok::Assign
                | Tok::EqEq
                | Tok::Ne
                | Tok::Lt
                | Tok::Le
                | Tok::Gt
                | Tok::Ge
                | Tok::AndAnd
                | Tok::OrOr
                | Tok::Bang
                | Tok::Amp
                | Tok::Pipe
                | Tok::Caret
                | Tok::Tilde
                | Tok::Shl
                | Tok::PlusEq
                | Tok::MinusEq
                | Tok::StarEq
                | Tok::SlashEq
                | Tok::PercentEq
                | Tok::AmpEq
                | Tok::PipeEq
                | Tok::CaretEq
                | Tok::ShlEq
                | Tok::DotDotEq
                | Tok::Comma
                | Tok::Colon
                | Tok::ColonColon
                | Tok::FatArrow
                | Tok::Dot
                | Tok::DotDot
                | Tok::LParen
                | Tok::LBracket
                | Tok::LBrace
        )
    }

    /// 진단 메시지용 설명.
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(s) => format!("identifier `{s}`"),
            Tok::Int(n) => format!("integer `{n}`"),
            Tok::Float(x) => format!("float `{x}`"),
            Tok::Str(_) => "string literal".to_string(),
            Tok::Newline => "end of line".to_string(),
            Tok::Eof => "end of file".to_string(),
            other => format!("`{}`", other.text()),
        }
    }

    fn text(&self) -> &'static str {
        match self {
            Tok::Fn => "fn",
            Tok::Struct => "struct",
            Tok::As => "as",
            Tok::Let => "let",
            Tok::Mut => "mut",
            Tok::Return => "return",
            Tok::If => "if",
            Tok::Else => "else",
            Tok::While => "while",
            Tok::Break => "break",
            Tok::Continue => "continue",
            Tok::For => "for",
            Tok::In => "in",
            Tok::Match => "match",
            Tok::Extern => "extern",
            Tok::Use => "use",
            Tok::Impl => "impl",
            Tok::Pub => "pub",
            Tok::Unsafe => "unsafe",
            Tok::Const => "const",
            Tok::True => "true",
            Tok::False => "false",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::LBrace => "{",
            Tok::RBrace => "}",
            Tok::LBracket => "[",
            Tok::RBracket => "]",
            Tok::Comma => ",",
            Tok::Colon => ":",
            Tok::ColonColon => "::",
            Tok::Question => "?",
            Tok::FatArrow => "=>",
            Tok::Semi => ";",
            Tok::Dot => ".",
            Tok::DotDot => "..",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::Percent => "%",
            Tok::Assign => "=",
            Tok::EqEq => "==",
            Tok::Ne => "!=",
            Tok::Lt => "<",
            Tok::Le => "<=",
            Tok::Gt => ">",
            Tok::Ge => ">=",
            Tok::AndAnd => "&&",
            Tok::OrOr => "||",
            Tok::Bang => "!",
            Tok::Amp => "&",
            Tok::Pipe => "|",
            Tok::Caret => "^",
            Tok::Tilde => "~",
            Tok::Shl => "<<",
            Tok::PlusEq => "+=",
            Tok::MinusEq => "-=",
            Tok::StarEq => "*=",
            Tok::SlashEq => "/=",
            Tok::PercentEq => "%=",
            Tok::AmpEq => "&=",
            Tok::PipeEq => "|=",
            Tok::CaretEq => "^=",
            Tok::ShlEq => "<<=",
            Tok::DotDotEq => "..=",
            Tok::Ident(_) | Tok::Int(_) | Tok::Float(_) | Tok::Str(_) | Tok::Newline | Tok::Eof => "",
        }
    }
}

struct Lx {
    chars: Vec<char>,
    i: usize,
    line: u32,
    col: u32,
}

impl Lx {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.i + 1).copied()
    }

    fn peek3(&self) -> Option<char> {
        self.chars.get(self.i + 2).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.i).copied()?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn eat(&mut self, want: char) -> bool {
        if self.peek() == Some(want) {
            self.bump();
            true
        } else {
            false
        }
    }
}

fn keyword(s: &str) -> Option<Tok> {
    Some(match s {
        "fn" => Tok::Fn,
        "struct" => Tok::Struct,
        "as" => Tok::As,
        "let" => Tok::Let,
        "mut" => Tok::Mut,
        "return" => Tok::Return,
        "if" => Tok::If,
        "else" => Tok::Else,
        "while" => Tok::While,
        "break" => Tok::Break,
        "continue" => Tok::Continue,
        "for" => Tok::For,
        "in" => Tok::In,
        "match" => Tok::Match,
        "extern" => Tok::Extern,
        "use" => Tok::Use,
        "impl" => Tok::Impl,
        "pub" => Tok::Pub,
        "unsafe" => Tok::Unsafe,
        "const" => Tok::Const,
        "true" => Tok::True,
        "false" => Tok::False,
        _ => return None,
    })
}

/// 파일 id를 토큰/진단의 span에 찍어서 lex한다.
pub fn lex_file(src: &str, file: u16) -> Result<Vec<Token>, Diag> {
    match lex(src) {
        Ok(mut toks) => {
            for t in &mut toks {
                t.span.file = file;
            }
            Ok(toks)
        }
        Err(mut d) => {
            d.span.file = file;
            Err(d)
        }
    }
}

pub fn lex(src: &str) -> Result<Vec<Token>, Diag> {
    let mut lx = Lx {
        // Windows 편집기가 붙이는 UTF-8 BOM은 무시한다.
        chars: src.strip_prefix('\u{feff}').unwrap_or(src).chars().collect(),
        i: 0,
        line: 1,
        col: 1,
    };
    let mut raw: Vec<Token> = Vec::new();

    while let Some(c) = lx.peek() {
        let (line, col) = (lx.line, lx.col);

        if c == '\n' {
            lx.bump();
            raw.push(Token {
                tok: Tok::Newline,
                span: Span::new(line, col, 1),
            });
            continue;
        }
        if c.is_whitespace() {
            lx.bump();
            continue;
        }
        if c == '/' && lx.peek2() == Some('/') {
            while let Some(ch) = lx.peek() {
                if ch == '\n' {
                    break;
                }
                lx.bump();
            }
            continue;
        }

        if c == '"' {
            lx.bump();
            let mut text = String::new();
            loop {
                match lx.bump() {
                    None | Some('\n') => {
                        return Err(Diag::new(
                            "unterminated string literal",
                            Span::new(line, col, 1),
                            "string starts here",
                        ));
                    }
                    Some('"') => break,
                    Some('\\') => {
                        let esc_col = lx.col - 1;
                        match lx.bump() {
                            Some('n') => text.push('\n'),
                            Some('t') => text.push('\t'),
                            Some('r') => text.push('\r'),
                            Some('0') => text.push('\0'),
                            Some('\\') => text.push('\\'),
                            Some('"') => text.push('"'),
                            _ => {
                                return Err(Diag::new(
                                    "unknown escape sequence",
                                    Span::new(lx.line, esc_col, 2),
                                    "supported: \\n \\t \\r \\0 \\\\ \\\"",
                                ));
                            }
                        }
                    }
                    Some(ch) => text.push(ch),
                }
            }
            raw.push(Token {
                tok: Tok::Str(text),
                span: Span::new(line, col, lx.col - col),
            });
            continue;
        }

        if c == '\'' {
            lx.bump();
            let span1 = Span::new(line, col, 1);
            let v = match lx.peek() {
                None | Some('\n') => return Err(Diag::new("unterminated character literal", span1, "expected a character after `'`")),
                Some('\\') => {
                    lx.bump();
                    let e = lx.peek();
                    lx.bump();
                    match e {
                        Some('n') => 10,
                        Some('t') => 9,
                        Some('r') => 13,
                        Some('0') => 0,
                        Some('\\') => 92,
                        Some('\'') => 39,
                        _ => return Err(Diag::new("unknown escape in character literal", span1, "supported: \\n \\t \\r \\0 \\\\ \\'")),
                    }
                }
                Some(ch) => {
                    lx.bump();
                    ch as i64
                }
            };
            if lx.peek() != Some('\'') {
                return Err(Diag::new("unterminated character literal", span1, "expected closing `'`"));
            }
            lx.bump();
            raw.push(Token { tok: Tok::Int(v as i128), span: Span::new(line, col, lx.col - col) });
            continue;
        }

        if c.is_ascii_digit() {
            let radix: u32 = if c == '0' {
                match lx.peek2() {
                    Some('x') | Some('X') => 16,
                    Some('b') | Some('B') => 2,
                    Some('o') | Some('O') => 8,
                    _ => 10,
                }
            } else {
                10
            };
            if radix != 10 {
                lx.bump();
                lx.bump();
            }
            let mut text = String::new();
            while let Some(ch) = lx.peek() {
                if ch == '_' {
                    lx.bump();
                } else if ch.is_digit(radix) {
                    text.push(ch);
                    lx.bump();
                } else {
                    break;
                }
            }
            // 실수 리터럴: `1.5`, `1e3`, `1.5e-2` (`.` 뒤에 숫자가 올 때만. `a.b` 체이닝과 구분한다.)
            let mut is_float = false;
            if radix == 10 && lx.peek() == Some('.') && matches!(lx.peek2(), Some(d) if d.is_ascii_digit())
            {
                is_float = true;
                text.push('.');
                lx.bump();
                while let Some(ch) = lx.peek() {
                    if ch == '_' {
                        lx.bump();
                    } else if ch.is_ascii_digit() {
                        text.push(ch);
                        lx.bump();
                    } else {
                        break;
                    }
                }
            }
            if radix == 10 && matches!(lx.peek(), Some('e') | Some('E')) {
                let ok = match lx.peek2() {
                    Some(d) if d.is_ascii_digit() => true,
                    Some('+') | Some('-') => lx.peek3().map_or(false, |d| d.is_ascii_digit()),
                    _ => false,
                };
                if ok {
                    is_float = true;
                    text.push('e');
                    lx.bump();
                    if matches!(lx.peek(), Some('+') | Some('-')) {
                        text.push(lx.peek().unwrap());
                        lx.bump();
                    }
                    while let Some(ch) = lx.peek() {
                        if ch.is_ascii_digit() {
                            text.push(ch);
                            lx.bump();
                        } else {
                            break;
                        }
                    }
                }
            }
            if is_float {
                let span = Span::new(line, col, lx.col - col);
                let value: f64 = text.parse().map_err(|_| {
                    Diag::new("invalid float literal", span, "cannot parse this number")
                })?;
                raw.push(Token {
                    tok: Tok::Float(value),
                    span,
                });
                continue;
            }
            let span = Span::new(line, col, lx.col - col);
            if text.is_empty() {
                return Err(Diag::new("invalid integer literal", span, "no digits after the radix prefix"));
            }
            if matches!(lx.peek(), Some(ch) if ch.is_alphanumeric()) {
                let bad = Span::new(lx.line, lx.col, 1);
                return Err(Diag::new("invalid digit in integer literal", bad, "unexpected character"));
            }
                        let value = i128::from_str_radix(&text, radix)
                .ok()
                .filter(|v| *v <= u64::MAX as i128)
                .ok_or_else(|| Diag::new("integer literal out of range", span, "does not fit in 64 bits"))?;
            raw.push(Token {
                tok: Tok::Int(value),
                span,
            });
            continue;
        }

        if c.is_alphabetic() || c == '_' {
            let mut text = String::new();
            while let Some(ch) = lx.peek() {
                if ch.is_alphanumeric() || ch == '_' {
                    text.push(ch);
                    lx.bump();
                } else {
                    break;
                }
            }
            let span = Span::new(line, col, lx.col - col);
            let tok = keyword(&text).unwrap_or(Tok::Ident(text));
            raw.push(Token { tok, span });
            continue;
        }

        lx.bump();
        let tok = match c {
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            ',' => Tok::Comma,
            ':' => {
                if lx.eat(':') {
                    Tok::ColonColon
                } else {
                    Tok::Colon
                }
            }
            ';' => Tok::Semi,
            '.' => {
                if lx.eat('.') {
                    if lx.eat('=') {
                        Tok::DotDotEq
                    } else {
                        Tok::DotDot
                    }
                } else {
                    Tok::Dot
                }
            }
            '+' => {
                if lx.eat('=') {
                    Tok::PlusEq
                } else {
                    Tok::Plus
                }
            }
            '-' => {
                if lx.eat('=') {
                    Tok::MinusEq
                } else {
                    Tok::Minus
                }
            }
            '*' => {
                if lx.eat('=') {
                    Tok::StarEq
                } else {
                    Tok::Star
                }
            }
            '/' => {
                if lx.eat('=') {
                    Tok::SlashEq
                } else {
                    Tok::Slash
                }
            }
            '%' => {
                if lx.eat('=') {
                    Tok::PercentEq
                } else {
                    Tok::Percent
                }
            }
            '?' => Tok::Question,
            '=' => {
                if lx.eat('=') {
                    Tok::EqEq
                } else if lx.eat('>') {
                    Tok::FatArrow
                } else {
                    Tok::Assign
                }
            }
            '!' => {
                if lx.eat('=') {
                    Tok::Ne
                } else {
                    Tok::Bang
                }
            }
            '<' => {
                if lx.eat('=') {
                    Tok::Le
                } else if lx.eat('<') {
                    if lx.eat('=') {
                        Tok::ShlEq
                    } else {
                        Tok::Shl
                    }
                } else {
                    Tok::Lt
                }
            }
            '>' => {
                if lx.eat('=') {
                    Tok::Ge
                } else {
                    Tok::Gt
                }
            }
            '&' => {
                if lx.eat('&') {
                    Tok::AndAnd
                } else if lx.eat('=') {
                    Tok::AmpEq
                } else {
                    Tok::Amp
                }
            }
            '|' => {
                if lx.eat('|') {
                    Tok::OrOr
                } else if lx.eat('=') {
                    Tok::PipeEq
                } else {
                    Tok::Pipe
                }
            }
            '^' => {
                if lx.eat('=') {
                    Tok::CaretEq
                } else {
                    Tok::Caret
                }
            }
            '~' => Tok::Tilde,
            other => {
                return Err(Diag::new(
                    format!("unexpected character `{other}`"),
                    Span::new(line, col, 1),
                    "",
                ));
            }
        };
        raw.push(Token {
            tok,
            span: Span::new(line, col, lx.col - col),
        });
    }

    // 줄 이어짐 규칙 적용.
    let mut out: Vec<Token> = Vec::with_capacity(raw.len() + 1);
    for idx in 0..raw.len() {
        let t = &raw[idx];
        if t.tok == Tok::Newline {
            let Some(last) = out.last() else { continue };
            if last.tok == Tok::Newline || last.tok.continues_line() {
                continue;
            }
            let next = raw[idx + 1..].iter().find(|x| x.tok != Tok::Newline);
            if let Some(nx) = next {
                if matches!(nx.tok, Tok::Dot | Tok::RParen | Tok::RBracket) {
                    continue;
                }
            }
        }
        out.push(t.clone());
    }
    out.push(Token {
        tok: Tok::Eof,
        span: Span::new(lx.line, lx.col, 1),
    });
    Ok(out)
}
