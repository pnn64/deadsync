//! Reuse lexical transitions in identical suffixes of recently seen chunks.
//! The grammar still consumes every token and resolves names in the new chunk.
use super::*;

const BLOCK: usize = 128;
const MIN_SUFFIX: usize = 3 * BLOCK;
const MAX_TOKENS: usize = 4096;
const MAX_INACTIVE: usize = 3;

#[derive(Debug)]
struct Entry {
    number: f64,
    kind: Kind,
    before: u16,
    after: u16,
    lines: u16,
    token_lines: u16,
    start: u16,
    end: u16,
}

#[derive(Debug, Default)]
enum Mode {
    #[default]
    Idle,
    Record,
    Replay {
        first: usize,
        offset: i32,
        next: usize,
    },
}

#[derive(Debug, Default)]
struct History {
    source: Vec<u8>,
    tokens: Vec<Entry>,
}
impl History {
    fn heap_bytes(&self) -> usize {
        self.source.capacity() + self.tokens.capacity() * size_of::<Entry>()
    }
}

#[derive(Debug, Default)]
pub(crate) struct ClosedTokens {
    active: History,
    inactive: Vec<History>,
    mode: Mode,
}

impl ClosedTokens {
    pub(super) fn begin(&mut self, input: &[u8]) {
        self.mode = Mode::Idle;
        if !(MIN_SUFFIX..=u16::MAX as usize).contains(&input.len()) || !input.is_ascii() {
            return;
        }
        self.select_history(&input[input.len() - BLOCK..]);
        let suffix = self
            .active
            .source
            .rchunks(BLOCK)
            .zip(input.rchunks(BLOCK))
            .take_while(|(old, new)| old == new)
            .map(|(old, _)| old.len())
            .sum::<usize>();
        if suffix >= MIN_SUFFIX && !self.active.tokens.is_empty() {
            let first = self.active.source.len() - suffix;
            let next = self
                .active
                .tokens
                .partition_point(|token| usize::from(token.before) < first);
            self.mode = Mode::Replay {
                first: input.len() - suffix,
                offset: input.len() as i32 - self.active.source.len() as i32,
                next,
            };
            return;
        }

        // Record only after a second similar source. Independent chunks pay
        // for comparing and retaining the bytes without building a token list.
        self.active.source.clear();
        self.active.tokens.clear();
        if self.active.source.try_reserve_exact(input.len()).is_ok() {
            self.active.source.extend_from_slice(input);
            if suffix >= MIN_SUFFIX {
                self.mode = Mode::Record;
            }
        }
    }

    fn select_history(&mut self, tail: &[u8]) {
        if self.active.source.ends_with(tail) || self.active.source.is_empty() {
            return;
        }
        let before = self
            .inactive
            .iter()
            .position(|trace| trace.source.ends_with(tail));
        let next = if let Some(before) = before {
            self.inactive.remove(before)
        } else if self.inactive.len() == MAX_INACTIVE {
            self.inactive.remove(0)
        } else if self.inactive.try_reserve(1).is_ok() {
            History::default()
        } else {
            return;
        };
        self.inactive
            .push(std::mem::replace(&mut self.active, next));
    }

    pub(super) fn end(&mut self) {
        self.mode = Mode::Idle;
    }

    pub(super) fn heap_bytes(&self) -> usize {
        self.active.heap_bytes()
            + self.inactive.capacity() * size_of::<History>()
            + self.inactive.iter().map(History::heap_bytes).sum::<usize>()
    }

    pub(super) fn recording(&self) -> bool {
        matches!(self.mode, Mode::Record)
    }

    pub(super) fn record(&mut self, before: usize, after: usize, lines: u32, token: &Token) {
        let tokens = &mut self.active.tokens;
        if tokens.len() == MAX_TOKENS || tokens.try_reserve(1).is_err() {
            self.end();
            return;
        }
        let Lexeme::Source { start, end, .. } = token.text else {
            return;
        };
        tokens.push(Entry {
            number: token.number,
            kind: token.kind,
            before: before as u16,
            after: after as u16,
            lines: lines as u16,
            token_lines: (token.end_line - token.line) as u16,
            start: start as u16,
            end: end as u16,
        });
    }

    pub(super) fn next(&mut self, pos: &mut usize, line: &mut u32, out: &mut Token) -> bool {
        let Mode::Replay {
            first,
            offset,
            ref mut next,
        } = self.mode
        else {
            return false;
        };
        if *pos < first {
            return false;
        }
        // Lexing starts at the live scanner's token boundary. A different
        // prefix can consume part of the suffix as a long string or comment.
        let shift = |old: u16| (i32::from(old) + offset) as usize;
        while let Some(token) = self.active.tokens.get(*next) {
            let before = shift(token.before);
            if before > *pos {
                return false;
            }
            *next += 1;
            if before == *pos {
                *pos = shift(token.after);
                *line += u32::from(token.lines);
                out.line = *line - u32::from(token.token_lines);
                out.end_line = *line;
                out.kind = token.kind;
                out.number = token.number;
                out.text = if matches!(token.kind, Kind::Name | Kind::String) {
                    Lexeme::Source {
                        start: shift(token.start) as u32,
                        end: shift(token.end) as u32,
                        string_id: 0,
                    }
                } else {
                    Lexeme::default()
                };
                return true;
            }
        }
        self.end();
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TokenSnapshot = (Kind, u32, u32, Vec<u8>, u64, usize);

    fn scan(scanner: &mut Scanner, source: &[u8]) -> Vec<TokenSnapshot> {
        let mut tokens = Vec::new();
        let mut token = Token::default();
        loop {
            scanner.next_closed(source, &mut token).unwrap();
            tokens.push((
                token.kind,
                token.line,
                token.end_line,
                token.text.bytes(source).to_vec(),
                token.number.to_bits(),
                scanner.position(),
            ));
            if token.kind == Kind::Eof {
                return tokens;
            }
        }
    }

    #[test]
    fn closed_tokens_follow_live_boundaries_across_alternating_sources() {
        let first =
            "value=17.125; name='a'; cooked='\\000'\r\n".repeat(27) + "-- ]=====]\r\nreturn value";
        let second = "other=42; text=[=[escaped]=]\n".repeat(29) + "return text";
        let mut history = ClosedTokens::default();
        for (tail, prefix, reuse) in [
            (&first, "local value=1;\n", false),
            (&second, "local other=19;\r\n", false),
            (&first, "local value=120;\r\n", false),
            (&second, "local other=2;\n", false),
            (&first, "local value=3999;\n\n", true),
            (&second, "local other=3;\n\n", true),
            (&first, "local text=[=====[", true),
            (&first, "--[=====[", true),
        ] {
            let source = format!("{prefix}{tail}");
            let mut scanner = Scanner::with_closed_tokens(history);
            scanner.begin_closed(source.as_bytes());
            assert_eq!(matches!(scanner.closed.mode, Mode::Replay { .. }), reuse);
            assert_eq!(
                scan(&mut scanner, source.as_bytes()),
                scan(&mut Scanner::default(), source.as_bytes()),
            );
            history = scanner.into_closed_tokens();
        }
    }
}

#[cfg(test)]
#[test]
fn replay_preserves_a_multiline_tokens_start_line() {
    let mut history = ClosedTokens::default();
    history.record(
        7,
        20,
        4,
        &Token {
            kind: Kind::String,
            line: 3,
            end_line: 5,
            text: Lexeme::Source {
                start: 11,
                end: 18,
                string_id: 0,
            },
            ..Token::default()
        },
    );
    history.mode = Mode::Replay {
        first: 11,
        offset: 4,
        next: 0,
    };
    let (mut pos, mut line) = (11, 100);
    let mut token = Token::default();
    assert!(history.next(&mut pos, &mut line, &mut token));
    assert_eq!((pos, line, token.line, token.end_line), (24, 104, 102, 104));
    assert!(matches!(
        token.text,
        Lexeme::Source {
            start: 15,
            end: 22,
            ..
        }
    ));
}
