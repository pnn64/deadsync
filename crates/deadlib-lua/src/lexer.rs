//! Lua 5.1 byte-oriented lexical analysis with locale character classes.

use crate::memory::text::Text;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Byte(u8),
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

impl Kind {
    pub(crate) fn static_label(self) -> Option<&'static [u8]> {
        match self {
            Self::Byte(_) => None,
            Self::Eof => Some(b"<eof>"),
            Self::Name => Some(b"<name>"),
            Self::Number => Some(b"<number>"),
            Self::String => Some(b"<string>"),
            Self::And => Some(b"and"),
            Self::Break => Some(b"break"),
            Self::Do => Some(b"do"),
            Self::Else => Some(b"else"),
            Self::ElseIf => Some(b"elseif"),
            Self::End => Some(b"end"),
            Self::False => Some(b"false"),
            Self::For => Some(b"for"),
            Self::Function => Some(b"function"),
            Self::If => Some(b"if"),
            Self::In => Some(b"in"),
            Self::Local => Some(b"local"),
            Self::Nil => Some(b"nil"),
            Self::Not => Some(b"not"),
            Self::Or => Some(b"or"),
            Self::Repeat => Some(b"repeat"),
            Self::Return => Some(b"return"),
            Self::Then => Some(b"then"),
            Self::True => Some(b"true"),
            Self::Until => Some(b"until"),
            Self::While => Some(b"while"),
            Self::Plus => Some(b"+"),
            Self::Minus => Some(b"-"),
            Self::Star => Some(b"*"),
            Self::Slash => Some(b"/"),
            Self::Percent => Some(b"%"),
            Self::Caret => Some(b"^"),
            Self::Hash => Some(b"#"),
            Self::Eq => Some(b"=="),
            Self::Ne => Some(b"~="),
            Self::Le => Some(b"<="),
            Self::Ge => Some(b">="),
            Self::Lt => Some(b"<"),
            Self::Gt => Some(b">"),
            Self::Assign => Some(b"="),
            Self::LParen => Some(b"("),
            Self::RParen => Some(b")"),
            Self::LBrace => Some(b"{"),
            Self::RBrace => Some(b"}"),
            Self::LBracket => Some(b"["),
            Self::RBracket => Some(b"]"),
            Self::Semi => Some(b";"),
            Self::Colon => Some(b":"),
            Self::Comma => Some(b","),
            Self::Dot => Some(b"."),
            Self::Concat => Some(b".."),
            Self::Dots => Some(b"..."),
        }
    }
    pub(crate) fn try_label(self) -> Result<Vec<u8>, crate::Error> {
        use crate::memory::fallible::{self, SliceExt};
        if let Self::Byte(byte) = self {
            return if crate::locale::class(byte, b'c') {
                Ok(fallible::format(format_args!("char({byte})"))?.into_bytes())
            } else {
                fallible::vec([byte])
            };
        }
        self.static_label()
            .expect("non-byte token has a static label")
            .try_to_vec()
    }
}

#[derive(Debug)]
pub(crate) struct Token {
    pub kind: Kind,
    pub line: u32,
    pub end_line: u32,
    pub text: Text,
    pub number: f64,
}

impl Default for Token {
    fn default() -> Self {
        Self {
            kind: Kind::Eof,
            line: 0,
            end_line: 0,
            text: Text::default(),
            number: 0.0,
        }
    }
}

mod scanner;
pub(crate) use scanner::Scanner;

pub(crate) fn parse_number(bytes: &[u8]) -> Option<f64> {
    // Every decimal integer with at most 15 digits is exactly representable
    // as binary64, including ones with leading zeroes.
    if !bytes.is_empty() && bytes.len() <= 15 && bytes[0].is_ascii_digit() {
        let mut integer = 0u64;
        let mut all_digits = true;
        for &byte in bytes {
            if !byte.is_ascii_digit() {
                all_digits = false;
                break;
            }
            integer = integer * 10 + u64::from(byte - b'0');
        }
        if all_digits {
            return Some(integer as f64);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::parse_number;

    #[test]
    fn short_decimal_integers_match_binary64_parsing_at_every_length() {
        for len in 1..=15 {
            let mut digits = vec![b'0'; len];
            for sample in 0..100 {
                for (index, byte) in digits.iter_mut().enumerate() {
                    *byte = b'0' + ((sample * 17 + len * 13 + index * 7) % 10) as u8;
                }
                let expected = std::str::from_utf8(&digits)
                    .unwrap()
                    .parse::<f64>()
                    .unwrap();
                assert_eq!(parse_number(&digits).unwrap().to_bits(), expected.to_bits());
            }
        }

        for (digits, integer) in [
            (b"000000000000000".as_slice(), 0u64),
            (b"000000000000001", 1),
            (b"000123456789012", 123_456_789_012),
            (b"999999999999999", 999_999_999_999_999),
        ] {
            assert_eq!(
                parse_number(digits).unwrap().to_bits(),
                (integer as f64).to_bits(),
                "{digits:?}"
            );
        }
    }

    #[test]
    fn other_number_forms_keep_rounding_and_invalid_input_behavior() {
        for (input, expected) in [
            (
                b"9007199254740991".as_slice(),
                Some(9_007_199_254_740_991.0),
            ),
            (b"9007199254740992", Some(9_007_199_254_740_992.0)),
            (b"9007199254740993", Some(9_007_199_254_740_992.0)),
            (b"9007199254740995", Some(9_007_199_254_740_996.0)),
            (b"0000000000000001", Some(1.0)),
            (b"-0", Some(-0.0)),
            (b"+01", Some(1.0)),
            (b" 01 ", Some(1.0)),
            (b"1.25", Some(1.25)),
            (b"1e3", Some(1000.0)),
            (b"0x10", Some(16.0)),
            (b"0X1p3", Some(8.0)),
            (b"1\0ignored", Some(1.0)),
            (b"", None),
            (b"1e", None),
            (b"1e+", None),
            (b"1.2.3", None),
            (b"12invalid", None),
            (b"0x", None),
            (b"1\xff", None),
        ] {
            assert_eq!(
                parse_number(input).map(f64::to_bits),
                expected.map(f64::to_bits),
                "{input:?}"
            );
        }
    }
}
