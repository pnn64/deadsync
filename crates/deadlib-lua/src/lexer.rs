//! Lua 5.1 byte-oriented lexical analysis in the C locale.

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
    pub end_line: u32,
    pub text: Box<[u8]>,
    pub number: f64,
}

mod scanner;
pub(crate) use scanner::Scanner;

pub(crate) fn parse_number(bytes: &[u8]) -> Option<f64> {
    let bytes = &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())];
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_matches(|c: char| matches!(c, '\t'..='\r' | ' '));
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
