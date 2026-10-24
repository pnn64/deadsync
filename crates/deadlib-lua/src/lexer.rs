//! Lua 5.1 byte-oriented lexical analysis in the C locale.

use crate::{Error, ErrorKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Eof,
    Name,
    Number,
    String,
    And,
    Break,
    Do,
    Else,
    ElseIf,
    End,
    False,
    For,
    Function,
    If,
    In,
    Local,
    Nil,
    Not,
    Or,
    Repeat,
    Return,
    Then,
    True,
    Until,
    While,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Hash,
    Eq,
    Ne,
    Le,
    Ge,
    Lt,
    Gt,
    Assign,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semi,
    Colon,
    Comma,
    Dot,
    Concat,
    Dots,
}

#[derive(Debug)]
pub(crate) struct Token {
    pub kind: Kind,
    pub line: u32,
    pub text: Box<[u8]>,
    pub number: f64,
}

pub(crate) struct Lexer<'a> {
    source: &'a [u8],
    pos: usize,
    line: u32,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a [u8]) -> Self {
        Self {
            source,
            pos: 0,
            line: 1,
        }
    }

    fn byte(&self) -> Option<u8> {
        self.source.get(self.pos).copied()
    }
    fn peek(&self, n: usize) -> Option<u8> {
        self.source.get(self.pos + n).copied()
    }
    fn error(&self, message: &str) -> Error {
        Error::new(ErrorKind::Syntax, self.line, message)
    }

    fn newline(&mut self) -> Result<(), Error> {
        let first = self.byte();
        self.pos += 1;
        if matches!(
            (first, self.byte()),
            (Some(b'\r'), Some(b'\n')) | (Some(b'\n'), Some(b'\r'))
        ) {
            self.pos += 1;
        }
        self.line = self
            .line
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::Limit, self.line, "too many source lines"))?;
        Ok(())
    }

    // Return delimiter length and '=' count without consuming a failed match.
    fn long_open(&self) -> Option<(usize, usize)> {
        if self.byte() != Some(b'[') {
            return None;
        }
        let mut end = self.pos + 1;
        while self.source.get(end) == Some(&b'=') {
            end += 1;
        }
        (self.source.get(end) == Some(&b'[')).then_some((end + 1 - self.pos, end - self.pos - 1))
    }

    fn long(&mut self, comment: bool, len: usize, equals: usize) -> Result<Box<[u8]>, Error> {
        self.pos += len;
        if matches!(self.byte(), Some(b'\n' | b'\r')) {
            self.newline()?;
        }
        let mut bytes = Vec::new();
        loop {
            match self.byte() {
                None => {
                    return Err(self.error(if comment {
                        "unfinished long comment"
                    } else {
                        "unfinished long string"
                    }));
                }
                Some(b']') => {
                    let end = self.pos + equals + 1;
                    if self
                        .source
                        .get(self.pos + 1..end)
                        .is_some_and(|s| s.iter().all(|&c| c == b'='))
                        && self.source.get(end) == Some(&b']')
                    {
                        self.pos = end + 1;
                        return Ok(bytes.into_boxed_slice());
                    }
                    if !comment {
                        bytes.push(b']');
                    }
                    self.pos += 1;
                }
                Some(b'\n' | b'\r') => {
                    self.newline()?;
                    if !comment {
                        bytes.push(b'\n');
                    }
                }
                Some(c) => {
                    if !comment {
                        bytes.push(c);
                    }
                    self.pos += 1;
                }
            }
        }
    }

    fn quoted(&mut self, quote: u8) -> Result<Box<[u8]>, Error> {
        self.pos += 1;
        let mut bytes = Vec::new();
        loop {
            match self.byte() {
                None | Some(b'\n' | b'\r') => return Err(self.error("unfinished string")),
                Some(c) if c == quote => {
                    self.pos += 1;
                    return Ok(bytes.into_boxed_slice());
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let c = self.byte().ok_or_else(|| self.error("unfinished string"))?;
                    if matches!(c, b'\n' | b'\r') {
                        self.newline()?;
                        bytes.push(b'\n');
                        continue;
                    }
                    if c.is_ascii_digit() {
                        let mut n = 0u16;
                        for _ in 0..3 {
                            match self.byte() {
                                Some(d @ b'0'..=b'9') => {
                                    n = n * 10 + u16::from(d - b'0');
                                    self.pos += 1;
                                }
                                _ => break,
                            }
                        }
                        bytes.push(
                            u8::try_from(n).map_err(|_| self.error("escape sequence too large"))?,
                        );
                    } else {
                        bytes.push(match c {
                            b'a' => 7,
                            b'b' => 8,
                            b'f' => 12,
                            b'n' => 10,
                            b'r' => 13,
                            b't' => 9,
                            b'v' => 11,
                            c => c,
                        });
                        self.pos += 1;
                    }
                }
                Some(c) => {
                    bytes.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    pub fn next(&mut self) -> Result<Token, Error> {
        loop {
            match self.byte() {
                Some(b' ' | b'\t' | 11 | 12) => self.pos += 1,
                Some(b'\n' | b'\r') => self.newline()?,
                Some(b'-') if self.peek(1) == Some(b'-') => {
                    self.pos += 2;
                    if let Some((len, equals)) = self.long_open() {
                        self.long(true, len, equals)?;
                    } else {
                        while !matches!(self.byte(), None | Some(b'\n' | b'\r')) {
                            self.pos += 1;
                        }
                    }
                }
                _ => break,
            }
        }
        let line = self.line;
        let mut token = Token {
            kind: Kind::Eof,
            line,
            text: Box::default(),
            number: 0.0,
        };
        let Some(c) = self.byte() else {
            return Ok(token);
        };
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = self.pos;
            self.pos += 1;
            while self
                .byte()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                self.pos += 1;
            }
            let text = &self.source[start..self.pos];
            token.kind = match text {
                b"and" => Kind::And,
                b"break" => Kind::Break,
                b"do" => Kind::Do,
                b"else" => Kind::Else,
                b"elseif" => Kind::ElseIf,
                b"end" => Kind::End,
                b"false" => Kind::False,
                b"for" => Kind::For,
                b"function" => Kind::Function,
                b"if" => Kind::If,
                b"in" => Kind::In,
                b"local" => Kind::Local,
                b"nil" => Kind::Nil,
                b"not" => Kind::Not,
                b"or" => Kind::Or,
                b"repeat" => Kind::Repeat,
                b"return" => Kind::Return,
                b"then" => Kind::Then,
                b"true" => Kind::True,
                b"until" => Kind::Until,
                b"while" => Kind::While,
                _ => Kind::Name,
            };
            if token.kind == Kind::Name {
                token.text = text.into();
            }
            return Ok(token);
        }
        if c.is_ascii_digit() || (c == b'.' && self.peek(1).is_some_and(|x| x.is_ascii_digit())) {
            let start = self.pos;
            self.pos += 1;
            while self.byte().is_some_and(|c| c.is_ascii_digit() || c == b'.') {
                self.pos += 1;
            }
            if matches!(self.byte(), Some(b'e' | b'E')) {
                self.pos += 1;
                if matches!(self.byte(), Some(b'+' | b'-')) {
                    self.pos += 1;
                }
            }
            while self
                .byte()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                self.pos += 1;
            }
            token.number = parse_number(&self.source[start..self.pos])
                .ok_or_else(|| self.error("malformed number"))?;
            token.kind = Kind::Number;
            return Ok(token);
        }
        if matches!(c, b'\'' | b'"') {
            token.text = self.quoted(c)?;
            token.kind = Kind::String;
            return Ok(token);
        }
        if c == b'[' {
            if let Some((len, equals)) = self.long_open() {
                token.text = self.long(false, len, equals)?;
                token.kind = Kind::String;
                return Ok(token);
            }
            if self.peek(1) == Some(b'=') {
                return Err(self.error("invalid long string delimiter"));
            }
        }
        self.pos += 1;
        token.kind = match c {
            b'+' => Kind::Plus,
            b'-' => Kind::Minus,
            b'*' => Kind::Star,
            b'/' => Kind::Slash,
            b'%' => Kind::Percent,
            b'^' => Kind::Caret,
            b'#' => Kind::Hash,
            b'=' => {
                if self.byte() == Some(b'=') {
                    self.pos += 1;
                    Kind::Eq
                } else {
                    Kind::Assign
                }
            }
            b'~' if self.byte() == Some(b'=') => {
                self.pos += 1;
                Kind::Ne
            }
            b'<' => {
                if self.byte() == Some(b'=') {
                    self.pos += 1;
                    Kind::Le
                } else {
                    Kind::Lt
                }
            }
            b'>' => {
                if self.byte() == Some(b'=') {
                    self.pos += 1;
                    Kind::Ge
                } else {
                    Kind::Gt
                }
            }
            b'(' => Kind::LParen,
            b')' => Kind::RParen,
            b'{' => Kind::LBrace,
            b'}' => Kind::RBrace,
            b'[' => Kind::LBracket,
            b']' => Kind::RBracket,
            b';' => Kind::Semi,
            b':' => Kind::Colon,
            b',' => Kind::Comma,
            b'.' => {
                if self.byte() == Some(b'.') {
                    self.pos += 1;
                    if self.byte() == Some(b'.') {
                        self.pos += 1;
                        Kind::Dots
                    } else {
                        Kind::Concat
                    }
                } else {
                    Kind::Dot
                }
            }
            _ => return Err(self.error("unexpected character")),
        };
        Ok(token)
    }
}

pub(crate) fn parse_number(bytes: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_matches(|c: char| c.is_ascii_whitespace());
    let (negative, unsigned) = if let Some(s) = text.strip_prefix('-') {
        (true, s)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    if let Some(hex) = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        // Retain 64 significant bits plus a sticky bit, then round once to binary64.
        let (digits, exp) = if let Some((a, b)) = hex.split_once(['p', 'P']) {
            (a, b.parse::<i32>().ok()?)
        } else {
            (hex, 0)
        };
        if digits.is_empty() {
            return None;
        }
        let mut bits = 0u64;
        let mut kept = 0u32;
        let mut skipped = 0i32;
        let mut sticky = false;
        for c in digits.bytes() {
            let d = (c as char).to_digit(16)?;
            if kept == 0 && d == 0 {
                continue;
            }
            if kept < 16 {
                bits = (bits << 4) | u64::from(d);
                kept += 1;
            } else {
                skipped = skipped.saturating_add(4);
                sticky |= d != 0;
            }
        }
        if sticky {
            bits |= 1;
        }
        let value = (bits as f64) * 2f64.powi(exp.saturating_add(skipped));
        return Some(if negative { -value } else { value });
    }
    // Rust accepts NaN/inf for runtime conversions; source numerals always start with a digit.
    text.parse().ok()
}
