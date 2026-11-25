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
            return if crate::locale::is_control(byte) {
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

/// Ordinary tokens borrow offsets in the source retained by the loader.
/// Escaped and synthesized text owns the bytes the compiler will read.
#[derive(Debug)]
pub(crate) enum Lexeme {
    Source {
        start: u32,
        end: u32,
        /// One-based identity in the parser's per-function constant pool.
        string_id: u32,
    },
    Owned(Text),
}

impl Default for Lexeme {
    fn default() -> Self {
        Self::Source {
            start: 0,
            end: 0,
            string_id: 0,
        }
    }
}

impl Lexeme {
    fn source(input: &[u8], start: usize, end: usize) -> Result<Self, crate::Error> {
        Ok(if let Ok(end) = u32::try_from(end) {
            Self::Source {
                start: start as u32,
                end,
                string_id: 0,
            }
        } else {
            Self::Owned(Text::new(&input[start..end])?)
        })
    }

    pub(crate) fn bytes<'a>(&'a self, input: &'a [u8]) -> &'a [u8] {
        match self {
            Self::Source { start, end, .. } => &input[*start as usize..*end as usize],
            Self::Owned(text) => text,
        }
    }

    pub(crate) fn set_string_slot(&mut self, slot: u32) {
        if let Self::Source { string_id, .. } = self {
            *string_id = slot.checked_add(1).unwrap_or(0);
        }
    }

    pub(crate) fn string_slot(&self) -> Option<usize> {
        match *self {
            Self::Source { string_id, .. } => string_id.checked_sub(1).map(|id| id as usize),
            Self::Owned(_) => None,
        }
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        match self {
            Self::Source { .. } => 0,
            Self::Owned(text) => text.heap_bytes(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Token {
    pub kind: Kind,
    pub line: u32,
    pub end_line: u32,
    /// Assigned by the parser when interning a name token.
    pub symbol: u32,
    pub text: Lexeme,
    pub number: f64,
}

impl Default for Token {
    fn default() -> Self {
        Self {
            kind: Kind::Eof,
            line: 0,
            end_line: 0,
            symbol: 0,
            text: Lexeme::default(),
            number: 0.0,
        }
    }
}

mod scanner;
pub(crate) use scanner::Scanner;

pub(crate) fn short_decimal_integer(bytes: &[u8]) -> Option<f64> {
    // Every decimal integer with at most 15 digits is exactly representable
    // as binary64, including ones with leading zeroes.
    if !(1..=15).contains(&bytes.len()) {
        return None;
    }
    let (mut integer, tail) = if let Some((head, tail)) = bytes.split_first_chunk::<8>() {
        let word = u64::from_le_bytes(*head);
        let digits = word.wrapping_sub(0x3030_3030_3030_3030);
        // In the first invalid lane, either subtracting '0' or adding 0x46
        // sets the high bit. Valid lower lanes cannot borrow or carry into it.
        if (digits | word.wrapping_add(0x4646_4646_4646_4646)) & 0x8080_8080_8080_8080 != 0 {
            return None;
        }
        let pairs = (digits * 10 + (digits >> 8)) & 0x00ff_00ff_00ff_00ff;
        let quads = (pairs * 100 + (pairs >> 16)) & 0x0000_ffff_0000_ffff;
        ((quads & 0xffff) * 10_000 + (quads >> 32), tail)
    } else {
        (0, bytes)
    };
    for &byte in tail {
        let digit = byte.wrapping_sub(b'0');
        if digit > 9 {
            return None;
        }
        integer = integer * 10 + u64::from(digit);
    }
    Some(integer as f64)
}

pub(crate) fn parse_number(bytes: &[u8]) -> Option<f64> {
    if let Some(integer) = short_decimal_integer(bytes) {
        return Some(integer);
    }
    let nul = if bytes.len() < 32 {
        bytes.iter().position(|&b| b == 0)
    } else {
        crate::pattern::find_byte(bytes, 0)
    };
    let bytes = &bytes[..nul.unwrap_or(bytes.len())];
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
        let hex = if hex.len() >= 64 && hex.as_bytes()[0] == b'0' {
            // Keep one digit so an all-zero mantissa still parses as a number.
            let padding = crate::pattern::byte_run(hex.as_bytes(), b'0') - 1;
            &hex[padding..]
        } else {
            hex
        };
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
    if unsigned.len() >= 64 && unsigned.starts_with("00000000") {
        // Keep one digit before any decimal point or exponent.
        let start = crate::pattern::byte_run(unsigned.as_bytes(), b'0') - 1;
        let magnitude: f64 = unsigned[start..].parse().ok()?;
        return Some(if negative { -magnitude } else { magnitude });
    }
    // Rust accepts NaN/inf for runtime conversions; source numerals always start with a digit.
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{parse_number, short_decimal_integer};

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

    #[test]
    fn decimal_integer_checks_every_byte_in_the_word() {
        for lane in 0..8 {
            for byte in u8::MIN..=u8::MAX {
                let mut input = *b"876543210123456";
                input[lane] = byte;
                let expected = std::str::from_utf8(&input)
                    .ok()
                    .and_then(|text| text.parse::<u64>().ok())
                    .filter(|_| byte.is_ascii_digit())
                    .map(|number| number as f64);
                assert_eq!(short_decimal_integer(&input), expected, "{input:?}");
            }
        }
    }

    #[test]
    fn hexadecimal_padding_keeps_zero_signs_exponents_and_invalid_tails() {
        for len in [63, 64, 65, 4096] {
            let zeros = "0".repeat(len);
            for (suffix, expected) in [
                ("", Some(-0.0)),
                ("2AP2", Some(-168.0)),
                ("p-8", Some(-0.0)),
                ("p", None),
                ("z", None),
            ] {
                let input = format!("-0x{zeros}{suffix}");
                assert_eq!(
                    parse_number(input.as_bytes()).map(f64::to_bits),
                    expected.map(f64::to_bits),
                    "{len} zeros before {suffix:?}"
                );
            }
        }
    }
}
