//! Compact tokens for the parser of small, closed source chunks.

use super::{
    Kind, Lexeme, Scanner, Token, ascii_identifier_end, decimal_token, keyword, word_boundary,
};
use crate::Error;

fn short_identifier_end(bytes: &[u8]) -> usize {
    let (head, tail) = bytes.split_at(bytes.len().min(8));
    head.iter()
        .position(|&byte| word_boundary(byte))
        .unwrap_or_else(|| head.len() + ascii_identifier_end(tail))
}

#[derive(Clone, Copy)]
pub(crate) struct DirectToken {
    pub kind: Kind,
    pub line: u32,
    pub end_line: u32,
    /// Number bits or a pair of source offsets, according to the token kind.
    payload: u64,
}

impl DirectToken {
    pub(crate) fn number(self) -> f64 {
        debug_assert_eq!(self.kind, Kind::Number);
        f64::from_bits(self.payload)
    }
}

fn span(start: u32, end: u32) -> u64 {
    u64::from(start) | (u64::from(end) << 32)
}

impl Default for DirectToken {
    fn default() -> Self {
        Self {
            kind: Kind::Eof,
            line: 1,
            end_line: 1,
            payload: 0,
        }
    }
}

/// The direct parser only needs source coordinates for ordinary spellings.
/// Escape and locale processing continue through the common scanner.
#[derive(Default)]
pub(crate) struct DirectScanner {
    scanner: Scanner,
    slow_token: Token,
}

impl DirectScanner {
    pub(crate) fn take_text(&mut self, token: DirectToken) -> Lexeme {
        debug_assert!(matches!(token.kind, Kind::Name | Kind::String));
        let start = token.payload as u32;
        let end = (token.payload >> 32) as u32;
        if start == u32::MAX {
            std::mem::take(&mut self.slow_token.text)
        } else {
            Lexeme::Source {
                start,
                end,
                string_id: 0,
            }
        }
    }

    pub(crate) fn next(&mut self, input: &[u8], token: &mut DirectToken) -> Result<(), Error> {
        debug_assert!(input.len() <= u32::MAX as usize);
        if self.next_ascii(input, token) {
            return Ok(());
        }
        let slow = &mut self.slow_token;
        self.scanner.next_closed(input, slow)?;
        let payload = match (&slow.text, slow.kind) {
            (_, Kind::Number) => slow.number.to_bits(),
            (Lexeme::Source { start, end, .. }, _) => span(*start, *end),
            (Lexeme::Owned(_), _) => span(u32::MAX, 0),
        };
        *token = DirectToken {
            kind: slow.kind,
            line: slow.line,
            end_line: slow.end_line,
            payload,
        };
        Ok(())
    }

    fn next_ascii(&mut self, input: &[u8], token: &mut DirectToken) -> bool {
        let mut start = self.scanner.pos;
        let c = loop {
            match input.get(start) {
                Some(b' ' | b'\t' | 11 | 12) => start += 1,
                Some(&c) => break c,
                None => return false,
            }
        };
        self.scanner.pos = start;
        let mut end = start + 1;
        let mut payload = 0;
        let kind = match c {
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                end += short_identifier_end(&input[end..]);
                if input.get(end).is_some_and(|&b| b >= 128) {
                    return false;
                }
                payload = span(start as u32, end as u32);
                if end == start + 1 {
                    Kind::Name
                } else {
                    keyword(&input[start..end])
                }
            }
            b'0'..=b'9' => {
                let Some((value, finish)) = decimal_token(input, start, c) else {
                    return false;
                };
                payload = value.to_bits();
                end = finish;
                Kind::Number
            }
            b'=' | b'<' | b'>' | b'~' => {
                let equal = input.get(end) == Some(&b'=');
                end += usize::from(equal);
                match (c, equal) {
                    (b'=', false) => Kind::Assign,
                    (b'=', true) => Kind::Eq,
                    (b'<', false) => Kind::Lt,
                    (b'<', true) => Kind::Le,
                    (b'>', false) => Kind::Gt,
                    (b'>', true) => Kind::Ge,
                    (b'~', true) => Kind::Ne,
                    _ => return false,
                }
            }
            b'(' => Kind::LParen,
            b')' => Kind::RParen,
            b',' => Kind::Comma,
            b';' => Kind::Semi,
            _ => return false,
        };
        self.scanner.pos = end;
        let line = self.scanner.line;
        *token = DirectToken {
            kind,
            line,
            end_line: line,
            payload,
        };
        true
    }
}

#[cfg(test)]
#[test]
fn closed_tokens_keep_spelling_and_line_information() {
    let inputs: &[&[u8]] = &[
        b"\r\n  local a = -1.25e+6; if a>=2 and not false then f(a) end",
        b"long_name_12345 = [[long\n string]]\n f('a\\n\\000z')",
        b"--[=[long\r\ncomment]=]\r-- line\nvalue = 0x10",
        b"identifier\0..\0x; 1e-",
        b"f('unterminated",
        b"local a_very_long_identifier_92 = [[",
        b"  \t\x0b\x0c",
        b"\n\n\n\n\n\n\n\n\nx~=y",
    ];
    for input in inputs {
        let mut reference = Scanner::default();
        let mut direct = DirectScanner::default();
        let mut expected = Token::default();
        let mut actual = DirectToken::default();
        loop {
            let expected_result = reference.next_closed(input, &mut expected);
            let actual_result = direct.next(input, &mut actual);
            assert_eq!(actual_result, expected_result, "{input:?}");
            if expected_result.is_err() {
                break;
            }
            assert_eq!(
                (actual.kind, actual.line, actual.end_line),
                (expected.kind, expected.line, expected.end_line)
            );
            match actual.kind {
                Kind::Eof => break,
                Kind::Name | Kind::String => {
                    assert_eq!(
                        direct.take_text(actual).bytes(input),
                        expected.text.bytes(input)
                    );
                }
                Kind::Number => assert_eq!(actual.number().to_bits(), expected.number.to_bits()),
                _ => {}
            }
        }
    }
}
