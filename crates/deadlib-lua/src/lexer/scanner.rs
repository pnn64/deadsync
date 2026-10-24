//! Resumable lexical state over append-only input, with no token replay.
use super::{Kind, Token, parse_number};
use crate::{Error, ErrorKind};

#[derive(Clone, Copy, Debug)]
enum State {
    Skip,
    Comment,
    LineComment,
    Word,
    Number,
    Exponent,
    NumberTail,
    Bracket {
        comment: bool,
        equals: usize,
    },
    LongStart {
        comment: bool,
        equals: usize,
    },
    Long {
        comment: bool,
        equals: usize,
    },
    LongClose {
        comment: bool,
        equals: usize,
        seen: usize,
    },
    Quoted(u8),
    Escape(u8),
    Decimal {
        quote: u8,
        value: u16,
        count: u8,
    },
    Emit,
    Fault(&'static str),
}

fn empty(line: u32) -> Token {
    Token {
        kind: Kind::Eof,
        line,
        text: Box::default(),
        number: 0.0,
    }
}

/// Input retains its prefix between calls. `None` requests more bytes; an EOF
/// token is emitted only after `closed` is true. Partial tokens retain their
/// cursor and decoded string bytes rather than rescanning earlier input.
pub(crate) struct Scanner {
    pos: usize,
    start: usize,
    line: u32,
    state: State,
    token: Token,
    text: Vec<u8>,
}

impl Default for Scanner {
    fn default() -> Self {
        Self {
            pos: 0,
            start: 0,
            line: 1,
            state: State::Skip,
            token: empty(1),
            text: Vec::new(),
        }
    }
}

impl Scanner {
    fn error(&self, message: &str) -> Error {
        Error::new(ErrorKind::Syntax, self.line, message)
    }

    fn newline(&mut self, input: &[u8], closed: bool) -> Result<bool, Error> {
        if self.pos + 1 == input.len() && !closed {
            return Ok(false);
        }
        let first = input[self.pos];
        self.pos += 1;
        if matches!(
            (first, input.get(self.pos)),
            (b'\r', Some(b'\n')) | (b'\n', Some(b'\r'))
        ) {
            self.pos += 1;
        }
        self.line = self
            .line
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::Limit, self.line, "too many source lines"))?;
        Ok(true)
    }

    fn emit(&mut self, kind: Kind) {
        self.token.kind = kind;
        self.state = State::Emit;
    }

    fn string(&mut self) {
        self.token.text = std::mem::take(&mut self.text).into_boxed_slice();
        self.emit(Kind::String);
    }

    pub(crate) fn next(&mut self, input: &[u8], closed: bool) -> Result<Option<Token>, Error> {
        loop {
            let current = input.get(self.pos).copied();
            // Lua's character reader fetches the next current byte before a
            // token is returned, including after a final quote or punctuation.
            if current.is_none() && !closed {
                return Ok(None);
            }
            match self.state {
                State::Fault(message) => return Err(self.error(message)),
                State::Emit => {
                    self.state = State::Skip;
                    return Ok(Some(std::mem::replace(&mut self.token, empty(self.line))));
                }
                State::Skip => {
                    self.token.line = self.line;
                    let Some(c) = current else {
                        self.emit(Kind::Eof);
                        continue;
                    };
                    match c {
                        b' ' | b'\t' | 11 | 12 => self.pos += 1,
                        b'\n' | b'\r' => {
                            if !self.newline(input, closed)? {
                                return Ok(None);
                            }
                        }
                        b'-' => {
                            if self.pos + 1 == input.len() && !closed {
                                return Ok(None);
                            }
                            self.pos += 1;
                            if input.get(self.pos) == Some(&b'-') {
                                self.pos += 1;
                                self.state = State::Comment;
                            } else {
                                self.emit(Kind::Minus);
                            }
                        }
                        b'\'' | b'"' => {
                            self.pos += 1;
                            self.state = State::Quoted(c);
                        }
                        b'[' => {
                            self.pos += 1;
                            self.state = State::Bracket {
                                comment: false,
                                equals: 0,
                            };
                        }
                        b'0'..=b'9' => {
                            self.start = self.pos;
                            self.pos += 1;
                            self.state = State::Number;
                        }
                        b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                            self.start = self.pos;
                            self.pos += 1;
                            self.state = State::Word;
                        }
                        b'.' => {
                            if self.pos + 1 == input.len() && !closed {
                                return Ok(None);
                            }
                            if input.get(self.pos + 1).is_some_and(u8::is_ascii_digit) {
                                self.start = self.pos;
                                self.pos += 1;
                                self.state = State::Number;
                            } else if input.get(self.pos + 1) == Some(&b'.') {
                                if self.pos + 2 == input.len() && !closed {
                                    return Ok(None);
                                }
                                if input.get(self.pos + 2) == Some(&b'.') {
                                    self.pos += 3;
                                    self.emit(Kind::Dots);
                                } else {
                                    self.pos += 2;
                                    self.emit(Kind::Concat);
                                }
                            } else {
                                self.pos += 1;
                                self.emit(Kind::Dot);
                            }
                        }
                        b'=' | b'<' | b'>' | b'~' => {
                            if self.pos + 1 == input.len() && !closed {
                                return Ok(None);
                            }
                            self.pos += 1;
                            let equal = input.get(self.pos) == Some(&b'=');
                            if equal {
                                self.pos += 1;
                            }
                            self.emit(match (c, equal) {
                                (b'=', true) => Kind::Eq,
                                (b'=', false) => Kind::Assign,
                                (b'<', true) => Kind::Le,
                                (b'<', false) => Kind::Lt,
                                (b'>', true) => Kind::Ge,
                                (b'>', false) => Kind::Gt,
                                (b'~', true) => Kind::Ne,
                                _ => {
                                    self.state = State::Fault("unexpected character");
                                    continue;
                                }
                            });
                        }
                        _ => {
                            self.pos += 1;
                            self.emit(match c {
                                b'+' => Kind::Plus,
                                b'*' => Kind::Star,
                                b'/' => Kind::Slash,
                                b'%' => Kind::Percent,
                                b'^' => Kind::Caret,
                                b'#' => Kind::Hash,
                                b'(' => Kind::LParen,
                                b')' => Kind::RParen,
                                b'{' => Kind::LBrace,
                                b'}' => Kind::RBrace,
                                b']' => Kind::RBracket,
                                b';' => Kind::Semi,
                                b':' => Kind::Colon,
                                b',' => Kind::Comma,
                                _ => {
                                    self.state = State::Fault("unexpected character");
                                    continue;
                                }
                            });
                        }
                    }
                }
                State::Comment => {
                    if current == Some(b'[') {
                        self.pos += 1;
                        self.state = State::Bracket {
                            comment: true,
                            equals: 0,
                        };
                    } else {
                        self.state = State::LineComment;
                    }
                }
                State::LineComment => {
                    while input
                        .get(self.pos)
                        .is_some_and(|&c| !matches!(c, b'\n' | b'\r'))
                    {
                        self.pos += 1;
                    }
                    if self.pos < input.len() || closed {
                        self.state = State::Skip;
                    }
                }
                State::Word => {
                    while input
                        .get(self.pos)
                        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
                    {
                        self.pos += 1;
                    }
                    if self.pos == input.len() && !closed {
                        return Ok(None);
                    }
                    let text = &input[self.start..self.pos];
                    let kind = match text {
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
                    if kind == Kind::Name {
                        self.token.text = text.into();
                    }
                    self.emit(kind);
                }
                State::Number => {
                    while input
                        .get(self.pos)
                        .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
                    {
                        self.pos += 1;
                    }
                    if self.pos == input.len() && !closed {
                        return Ok(None);
                    }
                    if matches!(input.get(self.pos), Some(b'e' | b'E')) {
                        self.pos += 1;
                        self.state = State::Exponent;
                    } else {
                        self.state = State::NumberTail;
                    }
                }
                State::Exponent => {
                    if matches!(current, Some(b'+' | b'-')) {
                        self.pos += 1;
                    }
                    self.state = State::NumberTail;
                }
                State::NumberTail => {
                    while input
                        .get(self.pos)
                        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
                    {
                        self.pos += 1;
                    }
                    if self.pos == input.len() && !closed {
                        return Ok(None);
                    }
                    self.token.number = parse_number(&input[self.start..self.pos])
                        .ok_or_else(|| self.error("malformed number"))?;
                    self.emit(Kind::Number);
                }
                State::Bracket {
                    comment,
                    mut equals,
                } => {
                    while input.get(self.pos) == Some(&b'=') {
                        self.pos += 1;
                        equals += 1;
                    }
                    self.state = State::Bracket { comment, equals };
                    if self.pos == input.len() && !closed {
                        return Ok(None);
                    }
                    if input.get(self.pos) == Some(&b'[') {
                        self.pos += 1;
                        self.state = State::LongStart { comment, equals };
                    } else if comment {
                        self.state = State::LineComment;
                    } else if equals == 0 {
                        self.emit(Kind::LBracket);
                    } else {
                        return Err(self.error("invalid long string delimiter"));
                    }
                }
                State::LongStart { comment, equals } => {
                    if matches!(current, Some(b'\n' | b'\r')) && !self.newline(input, closed)? {
                        return Ok(None);
                    }
                    self.state = State::Long { comment, equals };
                }
                State::Long { comment, equals } => match current {
                    None => {
                        return Err(self.error(if comment {
                            "unfinished long comment"
                        } else {
                            "unfinished long string"
                        }));
                    }
                    Some(b']') => {
                        self.pos += 1;
                        self.state = State::LongClose {
                            comment,
                            equals,
                            seen: 0,
                        };
                    }
                    Some(b'\n' | b'\r') => {
                        if !self.newline(input, closed)? {
                            return Ok(None);
                        }
                        if !comment {
                            self.text.push(b'\n');
                        }
                    }
                    Some(_) => {
                        let rest = &input[self.pos..];
                        let count = rest
                            .iter()
                            .position(|&b| matches!(b, b']' | b'\n' | b'\r'))
                            .unwrap_or(rest.len());
                        if !comment {
                            self.text.extend_from_slice(&rest[..count]);
                        }
                        self.pos += count;
                    }
                },
                State::LongClose {
                    comment,
                    equals,
                    mut seen,
                } => {
                    while input.get(self.pos) == Some(&b'=') {
                        self.pos += 1;
                        seen += 1;
                    }
                    self.state = State::LongClose {
                        comment,
                        equals,
                        seen,
                    };
                    if self.pos == input.len() && !closed {
                        return Ok(None);
                    }
                    if seen == equals && input.get(self.pos) == Some(&b']') {
                        self.pos += 1;
                        if comment {
                            self.state = State::Skip;
                        } else {
                            self.string();
                        }
                    } else {
                        if !comment {
                            self.text.push(b']');
                            self.text.extend(std::iter::repeat_n(b'=', seen));
                        }
                        self.state = State::Long { comment, equals };
                    }
                }
                State::Quoted(quote) => match current {
                    None | Some(b'\n' | b'\r') => return Err(self.error("unfinished string")),
                    Some(c) if c == quote => {
                        self.pos += 1;
                        self.string();
                    }
                    Some(b'\\') => {
                        self.pos += 1;
                        self.state = State::Escape(quote);
                    }
                    Some(_) => {
                        let rest = &input[self.pos..];
                        let count = rest
                            .iter()
                            .position(|&b| b == quote || matches!(b, b'\\' | b'\n' | b'\r'))
                            .unwrap_or(rest.len());
                        self.text.extend_from_slice(&rest[..count]);
                        self.pos += count;
                    }
                },
                State::Escape(quote) => match current {
                    None => return Err(self.error("unfinished string")),
                    Some(b'\n' | b'\r') => {
                        if !self.newline(input, closed)? {
                            return Ok(None);
                        }
                        self.text.push(b'\n');
                        self.state = State::Quoted(quote);
                    }
                    Some(b'0'..=b'9') => {
                        self.state = State::Decimal {
                            quote,
                            value: 0,
                            count: 0,
                        }
                    }
                    Some(c) => {
                        self.text.push(match c {
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
                        self.state = State::Quoted(quote);
                    }
                },
                State::Decimal {
                    quote,
                    mut value,
                    mut count,
                } => {
                    while count < 3 && input.get(self.pos).is_some_and(u8::is_ascii_digit) {
                        value = value * 10 + u16::from(input[self.pos] - b'0');
                        count += 1;
                        self.pos += 1;
                    }
                    self.state = State::Decimal {
                        quote,
                        value,
                        count,
                    };
                    if self.pos == input.len() && !closed {
                        return Ok(None);
                    }
                    self.text.push(
                        u8::try_from(value).map_err(|_| self.error("escape sequence too large"))?,
                    );
                    self.state = State::Quoted(quote);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type Observed = (Kind, u32, Vec<u8>, u64);
    type Outcome = Result<Vec<Observed>, (ErrorKind, u32, String)>;

    fn tokens(source: &[u8], ends: &[usize]) -> Outcome {
        let mut scanner = Scanner::default();
        let mut result = Vec::new();
        for &end in ends {
            loop {
                let token = scanner
                    .next(&source[..end], end == source.len())
                    .map_err(|e| (e.kind(), e.line(), e.message().into()))?;
                let Some(token) = token else {
                    break;
                };
                let done = token.kind == Kind::Eof;
                result.push((
                    token.kind,
                    token.line,
                    token.text.into_vec(),
                    token.number.to_bits(),
                ));
                if done {
                    return Ok(result);
                }
            }
        }
        Ok(result)
    }

    #[test]
    fn chunk_boundaries_preserve_tokens_and_failures() {
        let corpus: &[&[u8]] = &[
            b"and break do else elseif end false for function if in local nil not or repeat return then true until while",
            b"-- comment\r\nlocal a,b=0xff,1.25e-3;return a..'x',a~=b,a<=b,a>=b,...",
            b"'a\\000\\25\\255z\\q\\a\\b\\f\\n\\r\\t\\v\\\\\\\"\\\'' \"x\\\r\ny\"",
            b"[==[\r\nlong]=]one]==] --[=[hello\r\nworld]=]\nname [ key ]",
            b"--[=invalid is a line comment\nreturn -- eof",
            b". .. ... .25 1e+10 0XABC + - * / % ^ # == ~= <= >= ( ) { } [ ] ; : ,",
            b"[=bad", b"'unfinished", b"'\\999'", b"1.2.3", b"--[=[unfinished", b"'newline\r\n'", b"@ next", b"\0", b"~",
        ];
        for &source in corpus {
            let expected = tokens(source, &[source.len()]);
            for split in 0..=source.len() {
                assert_eq!(
                    tokens(source, &[split, source.len()]),
                    expected,
                    "split={split}: {source:?}"
                );
            }
            assert_eq!(
                tokens(source, &(0..=source.len()).collect::<Vec<_>>()),
                expected,
                "one-byte input: {source:?}"
            );
        }
    }

    #[test]
    fn token_completion_fetches_the_next_current_character() {
        for source in [b"+".as_slice(), b"'abc'", b"return", b"[=[x]=]"] {
            let mut scan = Scanner::default();
            assert!(scan.next(source, false).unwrap().is_none());
            let mut extended = source.to_vec();
            extended.push(b' ');
            assert!(scan.next(&extended, false).unwrap().is_some());
            assert!(scan.next(&extended, false).unwrap().is_none());
            assert_eq!(scan.next(&extended, true).unwrap().unwrap().kind, Kind::Eof);
        }
        let mut scan = Scanner::default();
        assert!(scan.next(b"@", false).unwrap().is_none());
        assert!(scan.next(b"@x", false).is_err());
    }

    #[test]
    fn large_tokens_resume_without_replaying_partial_input() {
        let cases = [
            format!("'{}'", "x".repeat(100_000)),
            format!("[=[{}]=]", "x".repeat(100_000)),
            format!("--[=[{}]=]\nreturn", "x".repeat(100_000)),
            "name".repeat(25_000),
        ];
        for source in cases {
            let input = source.as_bytes();
            assert_eq!(
                tokens(input, &(0..=input.len()).collect::<Vec<_>>()),
                tokens(input, &[input.len()])
            );
        }
    }

    #[test]
    fn next_character_requests_match_the_lua51_reader_boundary() {
        let lua = lua_reference::Lua::new();
        for fragment in [
            b")".as_slice(),
            b"@",
            b"+",
            b"'abc'",
            b"return",
            b"[=[x]=]",
            b"'\\999",
        ] {
            let observed = lua
                .load(
                    br#"
                local text=...;local tag={};local n=0
                local ok,f,e=pcall(load,function()n=n+1;if n==1 then return text end;error(tag)end)
                return n,ok,f==nil,e==tag
            "#
                    .as_slice(),
                )
                .call::<(u32, bool, bool, bool)>(lua.create_string(fragment).unwrap())
                .unwrap();
            assert_eq!(observed, (2, true, true, true), "{fragment:?}");
            assert!(
                Scanner::default().next(fragment, false).unwrap().is_none(),
                "{fragment:?}"
            );
        }
    }

    #[test]
    fn generated_binary_input_is_independent_of_chunk_size() {
        let mut random = 0x41c6_4e6du32;
        for case in 0..2000 {
            let mut source = Vec::new();
            for _ in 0..case % 65 {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                source.push((random >> 24) as u8);
            }
            let expected = tokens(&source, &[source.len()]);
            let ends = (0..=source.len()).collect::<Vec<_>>();
            assert_eq!(tokens(&source, &ends), expected, "{source:?}");
        }
    }
}
