//! Lua 5.1 byte patterns with compact instructions and explicit backtracking.
use crate::memory::{Buffer, Memory};
use crate::{Error, locale::add_class_bits};

mod cache;
pub(crate) use cache::Cache;
#[cfg(test)]
mod allocation_tests;

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
    code: Buffer<Instruction>,
    sets: Buffer<[u64; 4]>,
    pub anchored: bool,
}

impl Pattern {
    pub fn heap_bytes(&self) -> usize {
        self.code.capacity() * std::mem::size_of::<Instruction>()
            + self.sets.capacity() * std::mem::size_of::<[u64; 4]>()
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
    choices: Buffer<Choice>,
    captures: [Capture; 32],
    undo: [(u8, Capture); 64],
    undo_len: usize,
    count: u8,
}
impl Default for Work {
    fn default() -> Self {
        Self::new_in(Memory::default())
    }
}
impl Work {
    pub(crate) fn new_in(memory: Memory) -> Self {
        Self {
            choices: Buffer::new_in(memory),
            captures: [EMPTY; 32],
            undo: [(0, EMPTY); 64],
            undo_len: 0,
            count: 0,
        }
    }
}
impl Work {
    pub fn heap_bytes(&self) -> usize {
        self.choices.capacity() * std::mem::size_of::<Choice>()
    }
    fn push_choice(&mut self, choice: Choice) -> Result<(), Error> {
        self.choices.try_reserve(1).map_err(|_| Error::memory())?;
        self.choices.push(choice);
        Ok(())
    }
}
fn error(message: &'static str) -> Error {
    Error::runtime_static(message)
}

fn set_has(set: &[u64; 4], byte: u8) -> bool {
    set[usize::from(byte) / 64] & (1u64 << (byte % 64)) != 0
}

pub(crate) fn find_byte(bytes: &[u8], byte: u8) -> Option<usize> {
    if bytes.len() < 16 {
        return bytes.iter().position(|&candidate| candidate == byte);
    }
    if bytes[0] == byte {
        return Some(0);
    }
    if bytes.len() >= 32 {
        return find_byte_in_blocks(bytes, byte);
    }
    let needle = u64::from_le_bytes([byte; 8]);
    let (words, tail) = bytes.as_chunks::<8>();
    for (index, &word) in words.iter().enumerate() {
        let xor = u64::from_le_bytes(word) ^ needle;
        let hits = xor.wrapping_sub(0x0101_0101_0101_0101) & !xor & 0x8080_8080_8080_8080;
        if hits != 0 {
            // Borrows can flag a later lane. The least significant hit is exact.
            return Some(index * 8 + (hits.trailing_zeros() / 8) as usize);
        }
    }
    tail.iter()
        .position(|&candidate| candidate == byte)
        .map(|index| words.len() * 8 + index)
}

#[inline(never)]
fn find_byte_in_blocks(bytes: &[u8], byte: u8) -> Option<usize> {
    let (blocks, tail) = bytes.as_chunks::<32>();
    for (index, block) in blocks.iter().enumerate() {
        // Reduce the whole block so LLVM can compare its bytes in parallel.
        if block
            .iter()
            .fold(false, |hit, &candidate| hit | (candidate == byte))
        {
            return block
                .iter()
                .position(|&candidate| candidate == byte)
                .map(|offset| index * 32 + offset);
        }
    }
    tail.iter()
        .position(|&candidate| candidate == byte)
        .map(|offset| bytes.len() - tail.len() + offset)
}

pub(crate) fn byte_run(bytes: &[u8], byte: u8) -> usize {
    let (blocks, tail) = bytes.as_chunks::<32>();
    let block = [byte; 32];
    let (prefix, rest) = match blocks.iter().position(|bytes| *bytes != block) {
        Some(index) => (index * 32, blocks[index].as_slice()),
        None => (bytes.len() - tail.len(), tail),
    };
    let expected = u64::from_le_bytes([byte; 8]);
    let (words, tail) = rest.as_chunks::<8>();
    for (index, &word) in words.iter().enumerate() {
        let different = u64::from_le_bytes(word) ^ expected;
        if different != 0 {
            return prefix + index * 8 + (different.trailing_zeros() / 8) as usize;
        }
    }
    prefix
        + words.len() * 8
        + tail
            .iter()
            .position(|&candidate| candidate != byte)
            .unwrap_or(tail.len())
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
    let mut members = &pattern[first..end];
    while let [start, rest @ ..] = members {
        if let [b'%', code, rest @ ..] = members {
            add_class_bits(&mut set, *code);
            members = rest;
            continue;
        }
        if let [b'-', last, rest @ ..] = rest {
            members = rest;
            if start <= last {
                let first_word = usize::from(*start / 64);
                let last_word = usize::from(*last / 64);
                let head = u64::MAX << (*start % 64);
                let tail = u64::MAX >> (63 - *last % 64);
                if first_word == last_word {
                    set[first_word] |= head & tail;
                } else {
                    set[first_word] |= head;
                    set[first_word + 1..last_word].fill(u64::MAX);
                    set[last_word] |= tail;
                }
            }
        } else {
            members = rest;
            set[usize::from(*start / 64)] |= 1u64 << (*start % 64);
        }
    }
    if negate {
        set = set.map(|word| !word);
    }
    Some((end + 1, set))
}

impl Pattern {
    pub fn compile(pattern: &[u8], anchor: bool, memory: Memory) -> Result<Self, Error> {
        // Lua 5.1's pattern interpreter treats a zero byte as the pattern terminator.
        let pattern = &pattern[..find_byte(pattern, 0).unwrap_or(pattern.len())];
        let anchored = anchor && pattern.first() == Some(&b'^');
        let mut p = usize::from(anchored);
        let mut code = Buffer::new_in(memory.clone());
        code.try_reserve_exact(pattern.len())
            .map_err(|_| Error::memory())?;
        let mut sets = Buffer::new_in(memory);
        let mut open = [0u32; 32];
        let mut open_count = 0;
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
                            open[open_count] = arg;
                            open_count += 1;
                        }
                    }
                }
                b')' => {
                    if open_count != 0 {
                        open_count -= 1;
                        kind = Kind::Close;
                        arg = open[open_count];
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
                        sets.try_reserve(1).map_err(|_| Error::memory())?;
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
                                    sets.try_reserve(1).map_err(|_| Error::memory())?;
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
                        Some(c) if crate::locale::is_digit(c) => {
                            kind = Kind::Reference;
                            arg = u32::from(c - b'0');
                        }
                        Some(c) => {
                            kind = Kind::Set;
                            arg = sets.len() as u32;
                            let mut set = [0; 4];
                            add_class_bits(&mut set, c);
                            sets.try_reserve(1).map_err(|_| Error::memory())?;
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
        Ok(Self {
            code,
            sets,
            anchored,
        })
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
    /// A byte every match must start with. Attempts elsewhere fail on the
    /// first instruction without reaching any later instruction or error.
    pub fn first_byte(&self) -> Option<u8> {
        self.code
            .first()
            .filter(|i| {
                matches!(i.kind, Kind::Byte) && matches!(i.repeat, Repeat::One | Repeat::Plus)
            })
            .map(|i| i.arg as u8)
    }
    pub fn find(
        &self,
        source: &[u8],
        start: usize,
        work: &mut Work,
    ) -> Result<Option<Matched>, Error> {
        let first = self.first_byte().filter(|_| !self.anchored);
        let mut pos = start;
        while pos <= source.len() {
            if let Some(byte) = first {
                match find_byte(&source[pos..], byte) {
                    Some(offset) => pos += offset,
                    None => return Ok(None),
                }
            }
            if let Some(found) = self.match_at(source, pos, work)? {
                return Ok(Some(found));
            }
            if self.anchored {
                break;
            }
            pos += 1;
        }
        Ok(None)
    }
    pub fn match_at(
        &self,
        source: &[u8],
        start: usize,
        work: &mut Work,
    ) -> Result<Option<Matched>, Error> {
        // Slots at or beyond `count` are never read, and every opened slot is
        // written before use, so earlier attempts' captures need no reset.
        work.choices.clear();
        work.undo_len = 0;
        work.count = 0;
        let mut pc = 0;
        let mut pos = start;
        'match_loop: loop {
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
                    let one = || source.get(pos).is_some_and(|&b| self.atom(pc, b));
                    match i.repeat {
                        Repeat::One => {
                            if one() {
                                pos += 1;
                            } else {
                                matched = false;
                            }
                        }
                        Repeat::Minimal => {
                            if one() {
                                work.push_choice(Choice {
                                    pc: pc + 1,
                                    pos: pos + 1,
                                    undo: work.undo_len,
                                    count: work.count,
                                    alternative: Alternative::Longer(pc),
                                })?;
                            }
                        }
                        Repeat::Optional => {
                            if one() {
                                work.push_choice(Choice {
                                    pc: pc + 1,
                                    pos,
                                    undo: work.undo_len,
                                    count: work.count,
                                    alternative: Alternative::Once,
                                })?;
                                pos += 1;
                            }
                        }
                        Repeat::Star | Repeat::Plus => {
                            let rest = source.get(pos..).unwrap_or_default();
                            let run = match i.kind {
                                Kind::Any => rest.len(),
                                Kind::Byte => byte_run(rest, i.arg as u8),
                                Kind::Set => {
                                    let set = &self.sets[i.arg as usize];
                                    rest.iter()
                                        .position(|&byte| !set_has(set, byte))
                                        .unwrap_or(rest.len())
                                }
                                _ => unreachable!("repeated pattern atom"),
                            };
                            let min = usize::from(i.repeat == Repeat::Plus);
                            if run < min {
                                matched = false;
                            } else {
                                let start = pos;
                                pos += run;
                                if run > min {
                                    work.push_choice(Choice {
                                        pc: pc + 1,
                                        pos: pos - 1,
                                        undo: work.undo_len,
                                        count: work.count,
                                        alternative: Alternative::Shorter(start + min),
                                    })?;
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
}

#[cfg(test)]
mod byte_search_tests {
    use super::{byte_run, find_byte};

    #[test]
    fn byte_scans_preserve_word_block_and_tail_boundaries() {
        for needle in 0..=u8::MAX {
            // A lane differing by one can inherit a borrow from a match.
            let fill = needle ^ 1;
            let mut bytes = [fill; 76];
            for start in 0..8 {
                for len in 0..=68 {
                    let text = &bytes[start..start + len];
                    assert_eq!(find_byte(text, needle), None);
                    assert_eq!(byte_run(text, fill), len);
                    for index in 0..len {
                        bytes[start + index] = needle;
                        let text = &bytes[start..start + len];
                        assert_eq!(find_byte(text, needle), Some(index));
                        assert_eq!(byte_run(text, fill), index);
                        bytes[start + index] = fill;
                    }
                }
            }
        }
    }
}
