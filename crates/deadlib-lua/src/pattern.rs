//! Lua 5.1 byte patterns with compact instructions and explicit backtracking.
use crate::{Error, ErrorKind};

const UNFINISHED: usize = usize::MAX;
const POSITION: usize = usize::MAX - 1;
const UNUSED: usize = usize::MAX - 2;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Capture {
    pub start: usize,
    pub end: usize,
}
impl Capture {
    pub fn position(self) -> bool {
        self.end == POSITION
    }
    pub fn range(self) -> Result<std::ops::Range<usize>, Error> {
        if self.end >= UNUSED {
            return Err(error("unfinished capture"));
        }
        Ok(self.start..self.end)
    }
}
const EMPTY: Capture = Capture {
    start: 0,
    end: UNUSED,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Matched {
    pub start: usize,
    pub end: usize,
    pub captures: [Capture; 32],
    pub count: u8,
}
impl Matched {
    pub fn capture(self, index: usize) -> Result<Capture, Error> {
        if self.count == 0 && index == 0 {
            return Ok(Capture {
                start: self.start,
                end: self.end,
            });
        }
        if index >= usize::from(self.count) {
            return Err(error("invalid capture index"));
        }
        let c = self.captures[index];
        if c.end == UNFINISHED {
            return Err(error("unfinished capture"));
        }
        Ok(c)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Kind {
    Byte,
    Any,
    Set,
    Open,
    Close,
    Position,
    Balance,
    Frontier,
    Reference,
    End,
    Error,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Repeat {
    One,
    Optional,
    Star,
    Plus,
    Minimal,
}
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct Instruction {
    kind: Kind,
    repeat: Repeat,
    padding: u16,
    arg: u32,
}
#[derive(Debug)]
pub(crate) struct Pattern {
    code: Box<[Instruction]>,
    sets: Box<[[u64; 4]]>,
    pub anchored: bool,
}

impl Pattern {
    pub fn heap_bytes(&self) -> usize {
        std::mem::size_of_val(self.code.as_ref()) + std::mem::size_of_val(self.sets.as_ref())
    }
}

#[derive(Clone, Copy, Debug)]
enum Alternative {
    Once,
    Shorter(usize),
    Longer(usize),
}
#[derive(Clone, Copy, Debug)]
struct Choice {
    pc: usize,
    pos: usize,
    undo: usize,
    count: u8,
    alternative: Alternative,
}

#[derive(Debug)]
pub(crate) struct Work {
    choices: Vec<Choice>,
    captures: [Capture; 32],
    undo: [(u8, Capture); 64],
    undo_len: usize,
    count: u8,
    steps: u64,
}
impl Default for Work {
    fn default() -> Self {
        Self {
            choices: Vec::new(),
            captures: [EMPTY; 32],
            undo: [(0, EMPTY); 64],
            undo_len: 0,
            count: 0,
            steps: 0,
        }
    }
}
impl Work {
    pub fn heap_bytes(&self) -> usize {
        self.choices.capacity() * std::mem::size_of::<Choice>()
    }
}
fn error(message: &str) -> Error {
    Error::new(ErrorKind::Runtime, 0, message)
}

fn class(byte: u8, class: u8) -> bool {
    let hit = match class.to_ascii_lowercase() {
        b'a' => byte.is_ascii_alphabetic(),
        b'c' => byte.is_ascii_control(),
        b'd' => byte.is_ascii_digit(),
        b'l' => byte.is_ascii_lowercase(),
        b'p' => byte.is_ascii_punctuation(),
        b's' => matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12),
        b'u' => byte.is_ascii_uppercase(),
        b'w' => byte.is_ascii_alphanumeric(),
        b'x' => byte.is_ascii_hexdigit(),
        b'z' => byte == 0,
        _ => return byte == class,
    };
    hit != class.is_ascii_uppercase()
}
fn set_has(set: &[u64; 4], byte: u8) -> bool {
    set[usize::from(byte) / 64] & (1u64 << (byte % 64)) != 0
}

// Bracket syntax is parsed into a bitmap; errors remain deferred instructions.
fn bracket(pattern: &[u8], start: usize) -> Option<(usize, [u64; 4])> {
    let mut first = start + 1;
    let negate = pattern.get(first) == Some(&b'^');
    first += usize::from(negate);
    let mut end = first;
    loop {
        if end >= pattern.len() {
            return None;
        }
        if pattern[end] == b'%' && end + 1 < pattern.len() {
            end += 1;
        }
        end += 1;
        if pattern.get(end) == Some(&b']') {
            break;
        }
    }
    let mut set = [0u64; 4];
    for byte in 0..=255u8 {
        let mut p = first;
        let mut hit = false;
        while p < end {
            if pattern[p] == b'%' && p + 1 < end {
                hit |= class(byte, pattern[p + 1]);
                p += 2;
            } else if p + 2 < end && pattern[p + 1] == b'-' {
                hit |= byte >= pattern[p] && byte <= pattern[p + 2];
                p += 3;
            } else {
                hit |= byte == pattern[p];
                p += 1;
            }
        }
        if hit != negate {
            set[usize::from(byte) / 64] |= 1u64 << (byte % 64);
        }
    }
    Some((end + 1, set))
}

impl Pattern {
    pub fn compile(pattern: &[u8], anchor: bool) -> Self {
        // Lua 5.1's pattern interpreter treats a zero byte as the pattern terminator.
        let pattern = &pattern[..pattern
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(pattern.len())];
        let anchored = anchor && pattern.first() == Some(&b'^');
        let mut p = usize::from(anchored);
        let mut code = Vec::with_capacity(pattern.len());
        let mut sets = Vec::new();
        let mut open = Vec::new();
        let mut captures = 0u32;
        while p < pattern.len() {
            let mut kind = Kind::Byte;
            let mut arg = u32::from(pattern[p]);
            let mut quantifiable = false;
            match pattern[p] {
                b'(' => {
                    if captures >= 32 {
                        kind = Kind::Error;
                        arg = 4;
                    } else {
                        arg = captures;
                        captures += 1;
                        if pattern.get(p + 1) == Some(&b')') {
                            kind = Kind::Position;
                            p += 1;
                        } else {
                            kind = Kind::Open;
                            open.push(arg);
                        }
                    }
                }
                b')' => {
                    if let Some(slot) = open.pop() {
                        kind = Kind::Close;
                        arg = slot;
                    } else {
                        kind = Kind::Error;
                        arg = 5;
                    }
                }
                b'$' if p + 1 == pattern.len() => kind = Kind::End,
                b'.' => {
                    kind = Kind::Any;
                    quantifiable = true;
                }
                b'[' => {
                    if let Some((end, set)) = bracket(pattern, p) {
                        kind = Kind::Set;
                        arg = sets.len() as u32;
                        sets.push(set);
                        p = end - 1;
                        quantifiable = true;
                    } else {
                        kind = Kind::Error;
                        arg = 1;
                    }
                }
                b'%' => {
                    p += 1;
                    match pattern.get(p).copied() {
                        None => {
                            kind = Kind::Error;
                            arg = 0;
                        }
                        Some(b'b') => {
                            if let (Some(&left), Some(&right)) =
                                (pattern.get(p + 1), pattern.get(p + 2))
                            {
                                kind = Kind::Balance;
                                arg = u32::from(left) | (u32::from(right) << 8);
                                p += 2;
                            } else {
                                kind = Kind::Error;
                                arg = 3;
                            }
                        }
                        Some(b'f') => {
                            if pattern.get(p + 1) == Some(&b'[') {
                                if let Some((end, set)) = bracket(pattern, p + 1) {
                                    kind = Kind::Frontier;
                                    arg = sets.len() as u32;
                                    sets.push(set);
                                    p = end - 1;
                                } else {
                                    kind = Kind::Error;
                                    arg = 1;
                                }
                            } else {
                                kind = Kind::Error;
                                arg = 2;
                            }
                        }
                        Some(c @ b'0'..=b'9') => {
                            kind = Kind::Reference;
                            arg = u32::from(c - b'0');
                        }
                        Some(c) => {
                            kind = Kind::Set;
                            arg = sets.len() as u32;
                            let mut set = [0u64; 4];
                            for b in 0..=255u8 {
                                if class(b, c) {
                                    set[usize::from(b) / 64] |= 1u64 << (b % 64);
                                }
                            }
                            sets.push(set);
                            quantifiable = true;
                        }
                    }
                }
                _ => quantifiable = true,
            }
            p += 1;
            let repeat = if quantifiable {
                match pattern.get(p) {
                    Some(b'?') => Repeat::Optional,
                    Some(b'*') => Repeat::Star,
                    Some(b'+') => Repeat::Plus,
                    Some(b'-') => Repeat::Minimal,
                    _ => Repeat::One,
                }
            } else {
                Repeat::One
            };
            if repeat != Repeat::One {
                p += 1;
            }
            code.push(Instruction {
                kind,
                repeat,
                padding: 0,
                arg,
            });
            if kind == Kind::Error {
                break;
            }
        }
        Self {
            code: code.into_boxed_slice(),
            sets: sets.into_boxed_slice(),
            anchored,
        }
    }
    fn atom(&self, pc: usize, byte: u8) -> bool {
        let i = self.code[pc];
        match i.kind {
            Kind::Byte => byte == i.arg as u8,
            Kind::Any => true,
            Kind::Set => set_has(&self.sets[i.arg as usize], byte),
            _ => unreachable!("repeated pattern atom"),
        }
    }
    pub fn find(
        &self,
        source: &[u8],
        start: usize,
        work: &mut Work,
    ) -> Result<Option<Matched>, Error> {
        work.steps = 0;
        for pos in start..=source.len() {
            if let Some(found) = self.match_at(source, pos, work)? {
                return Ok(Some(found));
            }
            if self.anchored {
                break;
            }
        }
        Ok(None)
    }
    pub fn match_at(
        &self,
        source: &[u8],
        start: usize,
        work: &mut Work,
    ) -> Result<Option<Matched>, Error> {
        work.choices.clear();
        work.captures.fill(EMPTY);
        work.undo_len = 0;
        work.count = 0;
        let mut pc = 0;
        let mut pos = start;
        'match_loop: loop {
            work.steps += 1;
            if work.steps > 10_000_000 {
                return Err(Error::new(
                    ErrorKind::Limit,
                    0,
                    "pattern matching limit exceeded",
                ));
            }
            if pc == self.code.len() {
                return Ok(Some(Matched {
                    start,
                    end: pos,
                    captures: work.captures,
                    count: work.count,
                }));
            }
            let i = self.code[pc];
            let mut matched = true;
            match i.kind {
                Kind::Byte | Kind::Any | Kind::Set => {
                    let one = source.get(pos).is_some_and(|&b| self.atom(pc, b));
                    match i.repeat {
                        Repeat::One => {
                            if one {
                                pos += 1;
                            } else {
                                matched = false;
                            }
                        }
                        Repeat::Minimal => {
                            if one {
                                work.choices.push(Choice {
                                    pc: pc + 1,
                                    pos: pos + 1,
                                    undo: work.undo_len,
                                    count: work.count,
                                    alternative: Alternative::Longer(pc),
                                });
                            }
                        }
                        Repeat::Optional => {
                            if one {
                                work.choices.push(Choice {
                                    pc: pc + 1,
                                    pos,
                                    undo: work.undo_len,
                                    count: work.count,
                                    alternative: Alternative::Once,
                                });
                                pos += 1;
                            }
                        }
                        Repeat::Star | Repeat::Plus => {
                            let min = pos + usize::from(i.repeat == Repeat::Plus);
                            if i.repeat == Repeat::Plus && !one {
                                matched = false;
                            } else {
                                while source.get(pos).is_some_and(|&b| self.atom(pc, b)) {
                                    pos += 1;
                                }
                                if pos > min {
                                    work.choices.push(Choice {
                                        pc: pc + 1,
                                        pos: pos - 1,
                                        undo: work.undo_len,
                                        count: work.count,
                                        alternative: Alternative::Shorter(min),
                                    });
                                }
                            }
                        }
                    }
                }
                Kind::Open | Kind::Position | Kind::Close => {
                    let slot = i.arg as usize;
                    if i.kind == Kind::Close && work.captures[slot].end != UNFINISHED {
                        return Err(error("invalid pattern capture"));
                    }
                    work.undo[work.undo_len] = (slot as u8, work.captures[slot]);
                    work.undo_len += 1;
                    if i.kind == Kind::Close {
                        work.captures[slot].end = pos;
                    } else {
                        work.captures[slot] = Capture {
                            start: pos,
                            end: if i.kind == Kind::Open {
                                UNFINISHED
                            } else {
                                POSITION
                            },
                        };
                        work.count = work.count.max(slot as u8 + 1);
                    }
                }
                Kind::Balance => {
                    let left = i.arg as u8;
                    let right = (i.arg >> 8) as u8;
                    if source.get(pos) != Some(&left) {
                        matched = false;
                    } else {
                        let mut depth = 1usize;
                        let mut end = pos + 1;
                        while end < source.len() {
                            let b = source[end];
                            end += 1;
                            if b == right {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            } else if b == left {
                                depth += 1;
                            }
                        }
                        if depth == 0 {
                            pos = end;
                        } else {
                            matched = false;
                        }
                    }
                }
                Kind::Frontier => {
                    let set = &self.sets[i.arg as usize];
                    let previous = if pos == 0 { 0 } else { source[pos - 1] };
                    let next = source.get(pos).copied().unwrap_or(0);
                    matched = !set_has(set, previous) && set_has(set, next);
                }
                Kind::Reference => {
                    let slot = i.arg.wrapping_sub(1) as usize;
                    if slot >= usize::from(work.count) || work.captures[slot].end == UNFINISHED {
                        return Err(error("invalid capture index"));
                    }
                    let capture = work.captures[slot];
                    if capture.position() {
                        matched = false;
                    } else {
                        let bytes = &source[capture.start..capture.end];
                        if source.get(pos..pos + bytes.len()) == Some(bytes) {
                            pos += bytes.len();
                        } else {
                            matched = false;
                        }
                    }
                }
                Kind::End => matched = pos == source.len(),
                Kind::Error => {
                    return Err(error(match i.arg {
                        0 => "malformed pattern (ends with '%')",
                        1 => "malformed pattern (missing ']')",
                        2 => "missing '[' after '%f' in pattern",
                        3 => "unbalanced pattern",
                        4 => "too many captures",
                        _ => "invalid pattern capture",
                    }));
                }
            }
            if matched {
                pc += 1;
                continue;
            }
            if let Some(mut choice) = work.choices.pop() {
                while work.undo_len > choice.undo {
                    work.undo_len -= 1;
                    let (slot, old) = work.undo[work.undo_len];
                    work.captures[usize::from(slot)] = old;
                }
                work.count = choice.count;
                pc = choice.pc;
                pos = choice.pos;
                match choice.alternative {
                    Alternative::Once => {}
                    Alternative::Shorter(min) => {
                        if choice.pos > min {
                            choice.pos -= 1;
                            work.choices.push(choice);
                        }
                    }
                    Alternative::Longer(atom) => {
                        if source.get(choice.pos).is_some_and(|&b| self.atom(atom, b)) {
                            choice.pos += 1;
                            work.choices.push(choice);
                        }
                    }
                }
                continue 'match_loop;
            }
            return Ok(None);
        }
    }
    pub fn reset(work: &mut Work) {
        work.steps = 0;
    }
}
