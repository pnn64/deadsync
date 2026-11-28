//! Byte strings, pattern callbacks, iterators and Lua 5.1 formatting.
use super::*;
use crate::pattern::{
    Capture, Matched, Pattern, Work, byte_run, find_byte, find_dense_literal, has_magic,
};
#[cfg(test)]
mod allocation_tests;
#[cfg(test)]
mod substitution_allocation_tests;

pub(super) const STR_FIRST: u64 = 64;
pub(super) const STR_LEN: u64 = 64;
pub(super) const STR_SUB: u64 = 65;
pub(super) const STR_REP: u64 = 66;
pub(super) const STR_REVERSE: u64 = 67;
pub(super) const STR_BYTE: u64 = 68;
pub(super) const STR_CHAR: u64 = 69;
pub(super) const STR_LOWER: u64 = 70;
pub(super) const STR_UPPER: u64 = 71;
pub(super) const STR_FIND: u64 = 72;
pub(super) const STR_MATCH: u64 = 73;
pub(super) const STR_GMATCH: u64 = 74;
pub(super) const STR_GSUB: u64 = 75;
pub(super) const STR_FORMAT: u64 = 76;

const FORMAT_ARGUMENTS: usize = 16;

/// Integral number text has at most fourteen digits and a minus sign.
#[derive(Clone, Copy)]
enum PlainFormatArgument<'a> {
    String(&'a [u8]),
    Integer { bytes: [u8; 15], len: u8 },
}

impl PlainFormatArgument<'_> {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::String(bytes) => bytes,
            Self::Integer { bytes, len } => &bytes[..usize::from(*len)],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PlainFormat {
    ends: [u16; FORMAT_ARGUMENTS],
    fields: usize,
}

impl PlainFormat {
    fn scan(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > u16::MAX as usize {
            return None;
        }
        let mut plan = Self {
            ends: [0; FORMAT_ARGUMENTS],
            fields: 0,
        };
        let mut start = 0;
        while let Some(offset) = find_byte(&bytes[start..], b'%') {
            let at = start + offset;
            if bytes.get(at + 1) != Some(&b's') {
                return None;
            }
            *plan.ends.get_mut(plan.fields)? = at as u16;
            plan.fields += 1;
            start = at + 2;
        }
        (plan.fields >= 2).then_some(plan)
    }
}
pub(super) const STR_DUMP: u64 = 77;
pub(super) const STR_GFIND_REMOVED: u64 = 78;
pub(super) const ITERATOR_FIRST: u64 = 1u64 << 32;

#[derive(Debug)]
enum SubAction {
    Concat(usize),
    Byte(u8),
    String(StringId, std::ops::Range<usize>),
    Replacement(StringId, usize),
    Advance,
    Complete(u8),
}

/// How a batched substitution step found its next position.
enum Scan {
    /// A match at the cursor is stored in the substitution.
    Found,
    /// The scan copied unmatched bytes or replaced a match in the buffer.
    Advanced,
    /// Use one per-position attempt.
    Single,
}

#[derive(Debug)]
pub(super) struct Substitution {
    pub owner: usize,
    source: StringId,
    pattern: StringId,
    cursor: usize,
    count: i32,
    maximum: i32,
    top: usize,
    level: usize,
    output: Buffer<u8>,
    // Buffer concatenations can invoke Lua after debug mutation of C slots.
    // Continuations allocate only when such a metamethod actually suspends us.
    actions: Buffer<SubAction>,
    matched: Option<Matched>,
    anchored: bool,
    done: bool,
}
impl Substitution {
    pub(super) fn roots(&self) -> impl Iterator<Item = Value> + '_ {
        [Value::String(self.source), Value::String(self.pattern)]
            .into_iter()
            .chain(self.actions.iter().filter_map(|action| match action {
                SubAction::String(id, _) | SubAction::Replacement(id, _) => {
                    Some(Value::String(*id))
                }
                _ => None,
            }))
    }
    pub(super) fn heap_bytes(&self) -> usize {
        self.output.capacity() + self.actions.capacity() * std::mem::size_of::<SubAction>()
    }
}

pub(super) fn relative(index: i64, len: usize) -> i64 {
    if index < 0 {
        index.saturating_add(len as i64).saturating_add(1).max(0)
    } else {
        index
    }
}
pub(super) fn slice_range(start: i64, end: i64, len: usize) -> std::ops::Range<usize> {
    let start = start.max(1);
    let end = end.min(len as i64);
    if end < start {
        0..0
    } else {
        start as usize - 1..end as usize
    }
}
pub(super) fn find_start(index: i64, len: usize) -> usize {
    relative(index, len).saturating_sub(1).clamp(0, len as i64) as usize
}

pub(super) fn char_byte(number: f64) -> Option<u8> {
    if number > -1.0 && number < 256.0 {
        Some(number as i32 as u8)
    } else {
        u8::try_from(lua_integer(number) as i32).ok()
    }
}

impl Vm {
    pub(super) fn open_strings(&mut self) -> Result<(), Error> {
        let table = self.try_new_table()?;
        for (i, name) in [
            b"len".as_slice(),
            b"sub",
            b"rep",
            b"reverse",
            b"byte",
            b"char",
            b"lower",
            b"upper",
            b"find",
            b"match",
            b"gmatch",
            b"gsub",
            b"format",
            b"dump",
        ]
        .iter()
        .enumerate()
        {
            let key = self.try_intern(name)?;
            self.raw_set(table, key, Value::Native(STR_FIRST + i as u64))?;
        }
        let key = self.try_intern(b"gfind")?;
        self.raw_set(table, key, Value::Native(STR_GMATCH))?;
        self.set_global(b"string", Value::Table(table))?;
        let mt = self.try_new_table()?;
        self.raw_set(
            mt,
            Value::String(self.meta_names[Meta::Index as usize]),
            Value::Table(table),
        )?;
        self.type_metatables[3] = Some(mt);
        Ok(())
    }
    pub(super) fn argument(&self, base: usize, count: usize, index: usize) -> Value {
        if index < count {
            self.registers[base + 1 + index]
        } else {
            Value::Nil
        }
    }
    pub(super) fn string_argument(
        &mut self,
        base: usize,
        count: usize,
        index: usize,
    ) -> Result<StringId, Error> {
        match self.argument(base, count, index) {
            Value::String(id) => Ok(id),
            Value::Number(n) => {
                let Value::String(id) = self.intern_number(n)? else {
                    unreachable!()
                };
                self.registers[base + 1 + index] = Value::String(id);
                if let Some(frame) = self.active_native.map(|i| self.frames[i])
                    && frame.base == base
                    && frame.vararg_start != base + 1
                {
                    self.registers[frame.vararg_start + index] = Value::String(id);
                }
                self.gc_new_value(Value::String(id));
                Ok(id)
            }
            _ => Err(self.argument_type_error(base, count, index, "string")),
        }
    }
    pub(super) fn integer_argument(
        &self,
        base: usize,
        count: usize,
        index: usize,
        default: Option<i64>,
    ) -> Result<i64, Error> {
        let value = self.argument(base, count, index);
        if value == Value::Nil
            && let Some(default) = default
        {
            return Ok(default);
        }
        self.number(value)
            .map(lua_integer)
            .ok_or_else(|| self.argument_type_error(base, count, index, "number"))
    }
    fn prepare_pattern(&mut self, id: StringId, anchor: bool) -> Result<usize, Error> {
        let revision = crate::locale::revision();
        if revision != self.pattern_locale {
            self.patterns.clear();
            self.pattern_locale = revision;
        }
        let key = (id, anchor);
        if let Some(index) = self.patterns.find(key) {
            return Ok(index);
        }
        let before = self.patterns.storage_bytes();
        let reserved = self.patterns.try_reserve(1);
        self.gc_note_control(self.patterns.storage_bytes().saturating_sub(before));
        reserved.map_err(|_| self.memory_failure())?;
        let pattern = Pattern::compile(
            &self.strings.bytes[slot_index(id.0)],
            anchor,
            self.functions.memory().clone(),
        )
        .map_err(|_| self.memory_failure())?;
        self.gc_note_control(pattern.heap_bytes());
        Ok(self.patterns.insert(key, pattern))
    }
    fn with_pattern(
        &mut self,
        pattern: StringId,
        anchor: bool,
        source: StringId,
        action: impl FnOnce(&Pattern, &[u8], &mut Work) -> Result<Option<Matched>, Error>,
    ) -> Result<Option<Matched>, Error> {
        let pattern_index = self.prepare_pattern(pattern, anchor)?;
        let before = self.pattern_work.heap_bytes();
        let result = action(
            self.patterns.get_at(pattern_index),
            &self.strings.bytes[slot_index(source.0)],
            &mut self.pattern_work,
        );
        // Failed matching can retain a successfully grown backtracking buffer.
        self.gc_note_control(self.pattern_work.heap_bytes().saturating_sub(before));
        result.map_err(|error| self.pattern_error(error))
    }
    fn pattern_error(&self, error: Error) -> Error {
        if error.kind() == ErrorKind::Memory {
            self.memory_failure()
        } else {
            error
        }
    }
    fn capture_value(&mut self, source: StringId, capture: Capture) -> Result<Value, Error> {
        if capture.position() {
            Ok(Value::Number((capture.start + 1) as f64))
        } else {
            self.string_slice(source, capture.range()?)
        }
    }
    fn capture_results(
        &mut self,
        base: usize,
        source: StringId,
        matched: Matched,
        whole: bool,
    ) -> Result<usize, Error> {
        let count = if matched.count == 0 {
            usize::from(whole)
        } else {
            usize::from(matched.count)
        };
        self.reserve_registers(base + count)?;
        self.resize_registers(self.registers.len().max(base + count));
        for index in 0..count {
            let capture = matched.capture(index)?;
            self.registers[base + index] = self.capture_value(source, capture)?;
        }
        Ok(count)
    }
    #[inline(never)]
    fn reverse_large_string(&mut self, source: StringId, len: usize) -> Result<Value, Error> {
        let mut bytes = self.try_take_buffer(len)?;
        let text = &self.strings.bytes[slot_index(source.0)];
        let (prefix, blocks) = text.as_rchunks::<64>();
        for block in blocks.iter().rev() {
            let mut reversed = *block;
            reversed.reverse();
            bytes.extend_from_slice(&reversed);
        }
        for &byte in prefix.iter().rev() {
            bytes.push(byte);
        }
        self.try_finish_buffer(bytes)
    }

    pub(super) fn string_native(
        &mut self,
        id: u64,
        base: usize,
        count: usize,
    ) -> Result<usize, Error> {
        if id == STR_GFIND_REMOVED {
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                "'string.gfind' was renamed to 'string.gmatch'",
            ));
        }
        if id == STR_DUMP {
            let value = self.argument(base, count, 0);
            if !matches!(value, Value::Function(_) | Value::Native(_)) {
                return Err(self.argument_type_error(base, count, 0, "function"));
            }
            // Lua 5.1 discards extra arguments before producing the dump.
            self.registers[base + 2..base + 1 + count].fill(Value::Nil);
            if let Some(index) = self.active_native {
                self.frames[index].vararg_count = 1;
            }
            let Value::Function(function) = value else {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    0,
                    "unable to dump given function",
                ));
            };
            let closure = &self.functions[slot_index(function.0)];
            let workspace = crate::memory::compilation::Workspace::new(self.registers.memory())?;
            let bytes = workspace.run(|| {
                crate::chunk51::dump(&self.programs[closure.program].data, closure.prototype)
            })?;
            self.registers[base] = self.try_intern(&bytes)?;
            return Ok(1);
        }
        if id == STR_FORMAT {
            return self.string_format(base, count);
        }
        if id == STR_CHAR {
            let mut bytes = self.try_take_buffer(count)?;
            if count != 0 {
                bytes.resize(count, 0);
                let args = &self.registers[base + 1..base + 1 + count];
                for (index, (&value, destination)) in args.iter().zip(&mut bytes).enumerate() {
                    let number = match value {
                        Value::Number(number) => number,
                        _ => self.number(value).ok_or_else(|| {
                            self.argument_type_error(base, count, index, "number")
                        })?,
                    };
                    *destination = char_byte(number)
                        .ok_or_else(|| self.argument_error(index, b"invalid value"))?;
                }
            }
            self.registers[base] = self.try_finish_buffer(bytes)?;
            return Ok(1);
        }
        let source = self.string_argument(base, count, 0)?;
        let len = self.strings.bytes[slot_index(source.0)].len();
        let value = match id {
            STR_LEN => Value::Number(len as f64),
            STR_SUB | STR_BYTE => {
                let start = relative(
                    self.integer_argument(
                        base,
                        count,
                        1,
                        if id == STR_BYTE { Some(1) } else { None },
                    )?,
                    len,
                );
                let end = relative(
                    self.integer_argument(
                        base,
                        count,
                        2,
                        Some(if id == STR_SUB { -1 } else { start }),
                    )?,
                    len,
                );
                let range = slice_range(start, end, len);
                if id == STR_SUB {
                    self.string_slice(source, range)?
                } else {
                    let results = range.len();
                    if results > i32::MAX as usize {
                        return Err(Error::new(ErrorKind::Runtime, 0, "string slice too long"));
                    }
                    if results != 0 && results > limits::MAX_CSTACK.saturating_sub(count) {
                        return Err(Error::new(
                            ErrorKind::Runtime,
                            0,
                            "stack overflow (string slice too long)",
                        ));
                    }
                    self.reserve_registers(base + results)?;
                    self.resize_registers(self.registers.len().max(base + results));
                    let bytes = &self.strings.bytes[slot_index(source.0)][range];
                    for (register, &byte) in
                        self.registers[base..base + results].iter_mut().zip(bytes)
                    {
                        *register = Value::Number(f64::from(byte));
                    }
                    return Ok(results);
                }
            }
            STR_REP => {
                let repeat = (self.integer_argument(base, count, 1, None)? as i32).max(0) as usize;
                if len == 0 || repeat <= 1 {
                    self.registers[base] = if repeat == 1 {
                        self.string_slice(source, 0..len)?
                    } else {
                        self.try_intern(b"")?
                    };
                    return Ok(1);
                }
                let size = len
                    .checked_mul(repeat)
                    .ok_or_else(|| Error::new(ErrorKind::Limit, 0, "string length overflow"))?;
                let mut bytes = self.try_take_buffer(0)?;
                bytes
                    .try_reserve_exact(size)
                    .map_err(|_| self.memory_failure())?;
                let text = &self.strings.bytes[slot_index(source.0)];
                if let [byte] = text.as_ref() {
                    bytes.resize(size, *byte);
                } else {
                    bytes.extend_from_slice(text);
                    while bytes.len() < size {
                        let len = bytes.len();
                        bytes.extend_from_within(0..len.min(size - len));
                    }
                }
                self.try_finish_buffer(bytes)?
            }
            STR_REVERSE if len >= 4096 => self.reverse_large_string(source, len)?,
            STR_REVERSE | STR_LOWER | STR_UPPER => {
                let mut bytes = self.try_take_buffer(len)?;
                bytes.extend_from_slice(&self.strings.bytes[slot_index(source.0)]);
                if id == STR_REVERSE {
                    bytes.reverse();
                } else {
                    crate::locale::map_case(&mut bytes, id == STR_UPPER);
                }
                self.try_finish_buffer(bytes)?
            }
            STR_FIND | STR_MATCH => {
                let pattern = self.string_argument(base, count, 1)?;
                let start = find_start(self.integer_argument(base, count, 2, Some(1))?, len);
                let raw = self.strings.bytes[slot_index(pattern.0)].as_ref();
                let plain = id == STR_FIND
                    && (self.argument(base, count, 3).is_truthy() || !has_magic(raw));
                if plain {
                    let text = &self.strings.bytes[slot_index(source.0)][start..];
                    let found = 'literal: {
                        if raw.is_empty() {
                            break 'literal Some(0);
                        }
                        if raw.len() > text.len() {
                            break 'literal None;
                        }
                        if let [byte] = raw {
                            break 'literal find_byte(text, *byte);
                        }
                        // For long searches, use whichever end of the needle
                        // occurs less often in a small sample of the haystack.
                        let mut anchor = 0;
                        if text.len() >= 1024 {
                            if text.starts_with(raw) {
                                break 'literal Some(0);
                            }
                            let last = raw.len() - 1;
                            let (first_hits, last_hits) =
                                text[..64].iter().fold((0u8, 0u8), |(first, end), &byte| {
                                    (
                                        first + u8::from(byte == raw[0]),
                                        end + u8::from(byte == raw[last]),
                                    )
                                });
                            if last_hits < first_hits {
                                anchor = last;
                            }
                            if first_hits.min(last_hits) >= 4 {
                                break 'literal find_dense_literal(text, raw, anchor);
                            }
                        }
                        let mut cursor = anchor;
                        let candidates = &text[..text.len() - raw.len() + anchor + 1];
                        loop {
                            let Some(offset) = find_byte(&candidates[cursor..], raw[anchor]) else {
                                break None;
                            };
                            cursor += offset;
                            if text[cursor - anchor..].starts_with(raw) {
                                break Some(cursor - anchor);
                            }
                            cursor += 1;
                        }
                    };
                    if let Some(offset) = found {
                        let pos = start + offset;
                        let end = pos + raw.len();
                        self.resize_registers(self.registers.len().max(base + 2));
                        self.registers[base] = Value::Number((pos + 1) as f64);
                        self.registers[base + 1] = Value::Number(end as f64);
                        return Ok(2);
                    }
                    Value::Nil
                } else {
                    let found =
                        self.with_pattern(pattern, true, source, |pattern, text, work| {
                            pattern.find(text, start, work)
                        })?;
                    if let Some(found) = found {
                        if id == STR_MATCH {
                            return self.capture_results(base, source, found, true);
                        }
                        let n = self.capture_results(base + 2, source, found, false)?;
                        self.resize_registers(self.registers.len().max(base + 2));
                        self.registers[base] = Value::Number((found.start + 1) as f64);
                        self.registers[base + 1] = Value::Number(found.end as f64);
                        return Ok(n + 2);
                    }
                    Value::Nil
                }
            }
            STR_GMATCH => {
                let pattern = self.string_argument(base, count, 1)?;
                self.prepare_pattern(pattern, false)?;
                self.new_native(
                    threads::NativeKind::Match,
                    &[
                        Value::String(source),
                        Value::String(pattern),
                        Value::Number(0.),
                    ],
                )?
            }
            _ => unreachable!("string native ID"),
        };
        self.registers[base] = value;
        Ok(1)
    }
    pub(super) fn match_iterator(&mut self, id: u64, base: usize) -> Result<usize, Error> {
        let index = slot_index(id);
        if self.body_code(self.thread, self.active_native.expect("active iterator"))
            != Some(host::stack::body::MATCH)
            && !matches!(
                self.native_closures.get(id).map(|c| &c.kind),
                Some(threads::NativeKind::Match)
            )
        {
            return Err(Error::new(ErrorKind::Runtime, 0, "unknown native closure"));
        }
        let mut ids = [StringId(0); 2];
        for (slot, target) in ids.iter_mut().enumerate() {
            let value = self.native_closures[index].captures[slot];
            let value = match value {
                Value::Number(number) => self.intern_number(number)?,
                _ => value,
            };
            let Value::String(string) = value else {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    0,
                    "invalid pattern iterator capture",
                ));
            };
            self.set_native_capture(id, slot, value)?;
            *target = string;
        }
        let [source, pattern] = ids;
        let position = self
            .number(self.native_closures[index].captures[2])
            .map(lua_integer)
            .unwrap_or(0) as usize;
        let found = self.with_pattern(pattern, false, source, |pattern, text, work| {
            pattern.find(text, position, work)
        })?;
        if let Some(found) = found {
            self.native_closures[index].captures[2] =
                Value::Number((found.end + usize::from(found.end == found.start)) as f64);
            self.capture_results(base, source, found, true)
        } else {
            Ok(0)
        }
    }
    pub(super) fn trim_substitutions(&mut self) {
        while self
            .substitutions
            .last()
            .is_some_and(|s| s.owner >= self.frames.len())
        {
            self.substitutions.pop();
        }
    }
    fn substitution_push(&mut self, index: usize, value: Value) -> Result<(), Error> {
        let state = &self.substitutions[index];
        let frame = &self.frames[state.owner];
        let position = frame.vararg_start + state.top;
        self.reserve_registers(self.registers.len().max(position + 1))?;
        self.resize_registers(self.registers.len().max(position + 1));
        let state = &mut self.substitutions[index];
        state.top += 1;
        self.frames[state.owner].vararg_count = state.top as u32;
        self.registers[position] = value;
        Ok(())
    }
    fn substitution_queue(
        &mut self,
        index: usize,
        position: usize,
        action: SubAction,
    ) -> Result<(), Error> {
        let before = self.substitutions[index].actions.capacity();
        self.substitutions[index]
            .actions
            .try_reserve(1)
            .map_err(|_| self.memory_failure())?;
        self.gc_note_control(
            (self.substitutions[index].actions.capacity() - before) * size_of::<SubAction>(),
        );
        let actions = &mut self.substitutions[index].actions;
        assert!(position <= actions.len());
        actions.push(action);
        actions[position..].rotate_right(1);
        Ok(())
    }
    fn substitution_pop(&mut self, index: usize, count: usize) {
        let state = &mut self.substitutions[index];
        state.top -= count;
        let frame = &mut self.frames[state.owner];
        frame.vararg_count = state.top as u32;
        let end = frame.vararg_start + state.top;
        self.truncate_registers(end);
    }
    fn substitution_flush(&mut self, index: usize) -> Result<bool, Error> {
        if self.substitutions[index].output.is_empty() {
            return Ok(false);
        }
        let value = self
            .strings
            .try_intern(&self.substitutions[index].output)
            .map(Value::String)
            .map_err(|_| self.memory_failure())?;
        self.substitutions[index].output.clear();
        self.substitution_push(index, value)?;
        self.substitutions[index].level += 1;
        Ok(true)
    }
    fn substitution_concat_done(&mut self, index: usize, count: usize) -> Result<(), Error> {
        let state = &self.substitutions[index];
        let start = self.frames[state.owner].vararg_start + state.top - count;
        let value = self.registers[start];
        self.substitution_pop(index, count);
        self.substitution_push(index, value)
    }
    fn substitution_concat(&mut self, index: usize, count: usize) -> Result<bool, Error> {
        if count == 1 {
            return Ok(true);
        }
        if count == 0 {
            let value = self.try_intern(b"")?;
            self.substitution_push(index, value)?;
            return Ok(true);
        }
        let state = &self.substitutions[index];
        let frame = self.frames[state.owner];
        let end = frame.vararg_start + state.top;
        let start = end
            .checked_sub(count)
            .filter(|&p| p >= frame.vararg_start)
            .ok_or_else(|| Error::new(ErrorKind::Runtime, 0, "invalid native buffer stack"))?;
        let owner = state.owner;
        self.concat_range(start, start, end - 1)?;
        if self.frames.len() > owner + 1 {
            self.substitution_queue(
                index,
                self.substitutions[index].actions.len(),
                SubAction::Concat(count),
            )?;
            return Ok(false);
        }
        self.substitution_concat_done(index, count)?;
        Ok(true)
    }
    fn substitution_length(&mut self, position: usize) -> Result<usize, Error> {
        let mut value = self.registers[position];
        if let Value::Number(number) = value {
            value = self.intern_number(number)?;
            self.registers[position] = value;
        }
        Ok(match value {
            Value::String(id) => self.strings.bytes[slot_index(id.0)].len(),
            Value::Table(id) => self.tables[slot_index(id.0)].len(),
            Value::Userdata(id) => self.userdata[slot_index(id.0)].bytes.len(),
            _ => 0,
        })
    }
    fn substitution_adjust(&mut self, index: usize) -> Result<bool, Error> {
        let state = &self.substitutions[index];
        let level = state.level;
        if level > state.top {
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                "invalid native buffer stack",
            ));
        }
        if level <= 1 {
            return Ok(true);
        }
        let end = self.frames[state.owner].vararg_start + state.top;
        let mut size = self.substitution_length(end - 1)?;
        let mut count = 1;
        while count < level {
            let next = self.substitution_length(end - count - 1)?;
            if level - count + 1 < 10 && size <= next {
                break;
            }
            size = size.saturating_add(next);
            count += 1;
        }
        let complete = self.substitution_concat(index, count)?;
        self.substitutions[index].level = level - count + 1;
        Ok(complete)
    }
    fn substitution_byte(&mut self, index: usize, byte: u8) -> Result<bool, Error> {
        let queued = self.substitutions[index].actions.len();
        if self.substitutions[index].output.len() == io::LINE_BUFFER {
            self.substitution_flush(index)?;
            if !self.substitution_adjust(index)? {
                self.substitution_queue(index, queued, SubAction::Byte(byte))?;
                return Ok(false);
            }
        }
        self.substitutions[index].output.push(byte);
        Ok(true)
    }
    fn substitution_string(
        &mut self,
        index: usize,
        source: StringId,
        range: std::ops::Range<usize>,
    ) -> Result<bool, Error> {
        let queued = self.substitutions[index].actions.len();
        let mut cursor = range.start;
        while cursor < range.end {
            if self.substitutions[index].output.len() == io::LINE_BUFFER {
                self.substitution_flush(index)?;
                if !self.substitution_adjust(index)? {
                    self.substitution_queue(
                        index,
                        queued,
                        SubAction::String(source, cursor..range.end),
                    )?;
                    return Ok(false);
                }
            }
            let end = range
                .end
                .min(cursor + io::LINE_BUFFER - self.substitutions[index].output.len());
            self.substitutions[index]
                .output
                .extend_from_slice(&self.strings.bytes[slot_index(source.0)][cursor..end]);
            cursor = end;
        }
        Ok(true)
    }
    fn substitution_value(&mut self, index: usize) -> Result<bool, Error> {
        let state = &self.substitutions[index];
        let frame = self.frames[state.owner];
        let last = frame.vararg_start + state.top - 1;
        let value = self.registers[last];
        let value = if let Value::Number(number) = value {
            self.intern_number(number)?
        } else {
            value
        };
        let Value::String(id) = value else {
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                format!("invalid replacement value (a {})", value.type_name()),
            ));
        };
        self.registers[last] = value;
        let len = self.strings.bytes[slot_index(id.0)].len();
        if len <= io::LINE_BUFFER - self.substitutions[index].output.len() {
            self.substitutions[index]
                .output
                .extend_from_slice(&self.strings.bytes[slot_index(id.0)]);
            self.substitution_pop(index, 1);
            Ok(true)
        } else {
            if self.substitution_flush(index)? {
                self.registers.swap(last, last + 1);
            }
            self.substitutions[index].level += 1;
            self.substitution_adjust(index)
        }
    }
    fn substitution_replacement(&mut self, index: usize) -> Result<bool, Error> {
        let state = &self.substitutions[index];
        let frame = self.frames[state.owner];
        let last = frame.vararg_start + state.top - 1;
        if !self.registers[last].is_truthy() {
            let found = state.matched.as_ref().expect("substitution has a match");
            let range = found.start..found.end;
            let source = state.source;
            self.substitution_pop(index, 1);
            let value = self.string_slice(source, range)?;
            self.substitution_push(index, value)?;
        }
        self.substitution_value(index)
    }
    fn substitution_advance(&mut self, index: usize) -> Result<bool, Error> {
        let state = &self.substitutions[index];
        let matched = state.matched.as_ref().expect("substitution has a match");
        let source = state.source;
        let cursor = state.cursor;
        let byte = if matched.end > matched.start {
            self.substitutions[index].cursor = matched.end;
            None
        } else if cursor < self.strings.bytes[slot_index(source.0)].len() {
            self.substitutions[index].cursor += 1;
            Some(self.strings.bytes[slot_index(source.0)][cursor])
        } else {
            self.substitutions[index].done = true;
            None
        };
        let state = &mut self.substitutions[index];
        state.matched = None;
        state.done |= state.anchored;
        byte.map_or(Ok(true), |b| self.substitution_byte(index, b))
    }
    fn replacement_string(
        &mut self,
        index: usize,
        text: StringId,
        mut pos: usize,
    ) -> Result<bool, Error> {
        let queued = self.substitutions[index].actions.len();
        let state = &self.substitutions[index];
        let found = state.matched.as_ref().expect("substitution has a match");
        let (start, end) = (found.start, found.end);
        let source = state.source;
        let slot = slot_index(text.0);
        while let Some(&c) = self.strings.bytes[slot].get(pos) {
            let complete = if c != b'%' {
                let rest = &self.strings.bytes[slot][pos..];
                if rest.len() >= 3 {
                    let output = &mut self.substitutions[index].output;
                    // Leave flushes and their resumptions to substitution_byte.
                    let available = &rest[..rest.len().min(io::LINE_BUFFER - output.len())];
                    if let [_, tail @ ..] = available {
                        let len = find_byte(tail, b'%').map_or(available.len(), |at| at + 1);
                        output.extend_from_slice(&available[..len]);
                        pos += len;
                        continue;
                    }
                }
                pos += 1;
                self.substitution_byte(index, c)?
            } else {
                let c = self.strings.bytes[slot].get(pos + 1).copied().unwrap_or(0);
                pos += 2;
                if c == b'0' {
                    self.substitution_string(index, source, start..end)?
                } else if crate::locale::is_digit(c) {
                    let capture = self.substitutions[index]
                        .matched
                        .as_ref()
                        .expect("substitution has a match")
                        .capture(usize::from(c - b'1'))?;
                    let value = self.capture_value(source, capture)?;
                    self.substitution_push(index, value)?;
                    self.substitution_value(index)?
                } else {
                    if c == b'%' {
                        let tail = &self.strings.bytes[slot][pos..];
                        if tail.starts_with(b"%%") {
                            let output = &mut self.substitutions[index].output;
                            let room = io::LINE_BUFFER - output.len();
                            if room > 1 {
                                // Leave the next flush to substitution_byte.
                                let limit = tail.len().min(2 * (room - 1));
                                let extra = byte_run(&tail[..limit], b'%') / 2;
                                output.resize(output.len() + 1 + extra, b'%');
                                pos += 2 * extra;
                                continue;
                            }
                        }
                    }
                    self.substitution_byte(index, c)?
                }
            };
            if !complete {
                self.substitution_queue(index, queued, SubAction::Replacement(text, pos))?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Copy a text replacement directly when it fits in the current buffer.
    /// Hooks and native buffer observers still use the capture stack path.
    fn append_replacement(
        output: &mut Buffer<u8>,
        source: &[u8],
        mut text: &[u8],
        matched: std::ops::Range<usize>,
        capture: impl Fn(usize) -> Result<Capture, Error>,
    ) -> bool {
        let start = output.len();
        let appended = (|| {
            while let Some((&byte, rest)) = text.split_first() {
                text = rest;
                let bytes = if byte == b'%' {
                    let (&escaped, rest) = text.split_first()?;
                    text = rest;
                    match escaped {
                        b'0' => &source[matched.clone()],
                        b'1'..=b'9' => {
                            let capture = capture(usize::from(escaped - b'1')).ok()?;
                            &source[capture.range().ok()?]
                        }
                        0x80..=0xff => return None,
                        _ => {
                            if output.len() == io::LINE_BUFFER {
                                return None;
                            }
                            output.push(escaped);
                            continue;
                        }
                    }
                } else {
                    if output.len() == io::LINE_BUFFER {
                        return None;
                    }
                    output.push(byte);
                    continue;
                };
                if bytes.len() > io::LINE_BUFFER - output.len() {
                    return None;
                }
                output.extend_from_slice(bytes);
            }
            Some(())
        })();
        if appended.is_none() {
            output.truncate(start);
        }
        appended.is_some()
    }

    fn append_current_replacement(&mut self, index: usize, replacement: StringId) -> bool {
        let state = &mut self.substitutions[index];
        let found = state.matched.as_ref().expect("substitution has a match");
        Self::append_replacement(
            &mut state.output,
            &self.strings.bytes[slot_index(state.source.0)],
            &self.strings.bytes[slot_index(replacement.0)],
            found.start..found.end,
            |index| found.capture(index),
        )
    }
    fn substitution_complete(&mut self, index: usize, phase: u8) -> Result<Option<usize>, Error> {
        let queued = self.substitutions[index].actions.len();
        if phase == 0 {
            let state = &self.substitutions[index];
            let (source, cursor) = (state.source, state.cursor);
            if !self.substitution_string(
                index,
                source,
                cursor..self.strings.bytes[slot_index(source.0)].len(),
            )? {
                self.substitution_queue(index, queued, SubAction::Complete(1))?;
                return Ok(None);
            }
        }
        if phase <= 1 {
            self.substitution_flush(index)?;
            if !self.substitution_concat(index, self.substitutions[index].level)? {
                self.substitution_queue(index, queued, SubAction::Complete(2))?;
                return Ok(None);
            }
        }
        let count = self.substitutions[index].count;
        self.substitution_push(index, Value::Number(f64::from(count)))?;
        let state = self
            .substitutions
            .pop()
            .expect("active substitution exists");
        assert_eq!(
            index,
            self.substitutions.len(),
            "substitution stack is ordered"
        );
        let frame = self.frames[state.owner];
        self.return_frame(frame, frame.vararg_start + state.top - 2, 2)
    }

    fn try_plain_string_removal(
        &mut self,
        frame: Frame,
    ) -> Result<Option<(StringId, usize)>, Error> {
        const MAX_SUBJECT: usize = 1024;
        if frame.argc() != 3 || !self.registers.memory().uses_global_allocator() {
            return Ok(None);
        }
        let start = frame.vararg_start;
        let (Value::String(source), Value::String(pattern), Value::String(replacement)) = (
            self.registers[start],
            self.registers[start + 1],
            self.registers[start + 2],
        ) else {
            return Ok(None);
        };
        let pattern = &self.strings.bytes[slot_index(pattern.0)];
        let text = &self.strings.bytes[slot_index(source.0)];
        if pattern.is_empty()
            || pattern.len() > 16
            || pattern.contains(&0)
            || pattern.contains(&b')')
            || has_magic(pattern)
            || !self.strings.bytes[slot_index(replacement.0)].is_empty()
            || text.len() > MAX_SUBJECT
            || !self.sort_fast_context()
        {
            return Ok(None);
        }

        let find = |bytes: &[u8]| {
            let mut offset = 0;
            while let Some(next) = find_byte(&bytes[offset..], pattern[0]) {
                offset += next;
                if bytes[offset..].starts_with(pattern) {
                    return Some(offset);
                }
                offset += 1;
            }
            None
        };
        let Some(first) = find(text) else {
            self.strings.bytes.preserve_new_reference(source.0);
            return Ok(Some((source, 0)));
        };
        let mut output = [0; MAX_SUBJECT];
        output[..first].copy_from_slice(&text[..first]);
        let mut written = first;
        let mut count = 1;
        let mut tail = &text[first + pattern.len()..];
        while let Some(next) = find(tail) {
            output[written..written + next].copy_from_slice(&tail[..next]);
            written += next;
            count += 1;
            tail = &tail[next + pattern.len()..];
        }
        output[written..written + tail.len()].copy_from_slice(tail);
        let result = self.strings.try_intern(&output[..written + tail.len()])?;
        Ok(Some((result, count)))
    }

    pub(super) fn substitute_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        let base = frame.base;
        let index = if frame.pc() == 0 {
            if let Some((result, count)) = self.try_plain_string_removal(frame)? {
                self.registers[base] = Value::String(result);
                self.registers[base + 1] = Value::Number(count as f64);
                return self.return_frame(frame, base, 2);
            }
            let args = frame.vararg_start - 1;
            let argc = frame.argc();
            let source = self.string_argument(args, argc, 0)?;
            let pattern = self.string_argument(args, argc, 1)?;
            let replacement = self.argument(args, argc, 2);
            let len = self.strings.bytes[slot_index(source.0)].len();
            let maximum =
                self.integer_argument(args, argc, 3, Some(len.saturating_add(1) as i64))? as i32;
            if !matches!(
                replacement,
                Value::Number(_)
                    | Value::String(_)
                    | Value::Table(_)
                    | Value::Function(_)
                    | Value::Native(_)
            ) {
                return Err(self.argument_error(2, b"string/function/table expected"));
            }
            let pattern_index = self.prepare_pattern(pattern, true)?;
            let anchored = self.patterns.get_at(pattern_index).anchored;
            let mut output = Buffer::new_in(self.registers.memory().clone());
            output
                .try_reserve_exact(io::LINE_BUFFER)
                .map_err(|_| self.memory_failure())?;
            self.gc_note_control(output.capacity());
            let capacity = self.substitutions.capacity();
            self.substitutions
                .try_reserve(1)
                .map_err(|_| self.memory_failure())?;
            self.gc_note_control(
                (self.substitutions.capacity() - capacity) * size_of::<Substitution>(),
            );
            self.reserve_registers(self.registers.len().max(base + argc + 1))?;
            self.resize_registers(self.registers.len().max(base + argc + 1));
            self.registers
                .copy_within(frame.vararg_start..frame.vararg_start + argc, base + 1);
            self.registers[frame.vararg_start..frame.vararg_start + argc].fill(Value::Nil);
            self.frames[owner].vararg_start = base + 1;
            let index = self.substitutions.len();
            self.substitutions.push(Substitution {
                owner,
                source,
                pattern,
                cursor: 0,
                count: 0,
                maximum,
                top: argc,
                level: 0,
                output,
                actions: Buffer::new_in(self.registers.memory().clone()),
                matched: None,
                anchored,
                done: false,
            });
            self.frames[owner].pc = ((index + 1) << 2) as u32;
            index
        } else {
            (frame.pc() >> 2) - 1
        };
        if let Some(action) = self.substitutions[index].actions.pop() {
            match action {
                SubAction::Concat(count) => self.substitution_concat_done(index, count)?,
                SubAction::Byte(byte) => {
                    self.substitution_byte(index, byte)?;
                }
                SubAction::String(source, range) => {
                    self.substitution_string(index, source, range)?;
                }
                SubAction::Replacement(text, pos) => {
                    self.replacement_string(index, text, pos)?;
                }
                SubAction::Advance => {
                    self.substitution_advance(index)?;
                }
                SubAction::Complete(phase) => return self.substitution_complete(index, phase),
            }
            return Ok(None);
        }
        let frame = self.frames[owner];
        if frame.pc() & 3 != 0 {
            let end = frame.vararg_start + self.substitutions[index].top;
            if frame.pc() & 3 == 1 {
                let value = self.registers[end];
                self.truncate_registers(end + 1);
                self.substitution_push(index, value)?;
            } else {
                self.truncate_registers(end);
            }
            let complete = self.substitution_replacement(index)?;
            self.frames[owner].pc = ((index + 1) << 2) as u32;
            if !complete {
                self.substitution_queue(index, 0, SubAction::Advance)?;
                return Ok(None);
            }
            if !self.substitution_advance(index)? {
                return Ok(None);
            }
        }
        // Literal replacements never enter Lua. Without an instruction budget
        // or hooks, consecutive matches run in this step until the first
        // buffer flush, keeping each call's work bounded by one buffer.
        let batch = self.budget.is_none() && self.hooks.mask == 0;
        let direct_replacement =
            self.registers.memory().uses_global_allocator() && self.sort_fast_context();
        let direct_text = if direct_replacement
            && let Value::String(id) =
                self.argument(frame.vararg_start - 1, self.substitutions[index].top, 2)
        {
            Some(id)
        } else {
            None
        };
        loop {
            if self.substitutions[index].done
                || self.substitutions[index].count >= self.substitutions[index].maximum
            {
                return self.substitution_complete(index, 0);
            }
            let state = &self.substitutions[index];
            let (source, pattern, cursor) = (state.source, state.pattern, state.cursor);
            let room = io::LINE_BUFFER - state.output.len();
            // Text replacements can scan the next buffer with earlier chunks
            // already on the stack. Scanning adds no Lua references, and
            // flushes still use the native stack path.
            let scan = if batch
                && state.actions.is_empty()
                && room > 0
                && (state.level == 0 || direct_text.is_some() && self.gc_checkpoint_idle())
            {
                self.substitution_scan(index, direct_text)?
            } else {
                Scan::Single
            };
            match scan {
                Scan::Found => {}
                Scan::Advanced => continue,
                Scan::Single => {
                    let found =
                        self.with_pattern(pattern, true, source, |pattern, text, work| {
                            pattern.match_at(text, cursor, work)
                        })?;
                    self.substitutions[index].matched = found;
                }
            }
            if self.substitutions[index].matched.is_some() {
                self.substitutions[index].count += 1;
                let frame = self.frames[owner];
                let replacement =
                    self.argument(frame.vararg_start - 1, self.substitutions[index].top, 2);
                match replacement {
                    Value::String(text)
                        if direct_replacement && self.append_current_replacement(index, text) =>
                    {
                        if !self.substitution_advance(index)? {
                            return Ok(None);
                        }
                    }
                    Value::String(_) | Value::Number(_) => {
                        let text = self.string_argument(frame.vararg_start - 1, frame.argc(), 2)?;
                        if !self.replacement_string(index, text, 0)? {
                            self.substitution_queue(index, 0, SubAction::Advance)?;
                            return Ok(None);
                        }
                        if !self.substitution_advance(index)? {
                            return Ok(None);
                        }
                    }
                    Value::Table(_) => {
                        let capture = self.substitutions[index]
                            .matched
                            .as_ref()
                            .expect("substitution has a match")
                            .capture(0)?;
                        let key = self.capture_value(source, capture)?;
                        self.substitution_push(index, key)?;
                        let call = frame.vararg_start + self.substitutions[index].top - 1;
                        self.frames[owner].pc |= 2;
                        self.get_table(replacement, key, call)?;
                        return Ok(None);
                    }
                    Value::Function(_) | Value::Native(_) => {
                        let found = self.substitutions[index]
                            .matched
                            .expect("substitution has a match");
                        let call = frame.vararg_start + self.substitutions[index].top;
                        let argc = self.capture_results(call + 1, source, found, true)?;
                        self.registers[call] = replacement;
                        self.frames[owner].pc |= 1;
                        if let Some(actual) = self.start_call(call, argc, 1, false)? {
                            self.finish_call(call, actual, 1);
                        }
                        return Ok(None);
                    }
                    _ => {
                        if self.substitutions[index].top == 0 {
                            return Err(Error::new(
                                ErrorKind::Runtime,
                                0,
                                "invalid native buffer stack",
                            ));
                        }
                        if !self.substitution_replacement(index)? {
                            self.substitution_queue(index, 0, SubAction::Advance)?;
                            return Ok(None);
                        }
                        if !self.substitution_advance(index)? {
                            return Ok(None);
                        }
                    }
                }
            } else if let Some(&byte) = self.strings.bytes[slot_index(source.0)].get(cursor) {
                let state = &mut self.substitutions[index];
                state.cursor += 1;
                state.done |= state.anchored;
                self.substitution_byte(index, byte)?;
            } else {
                self.substitutions[index].done = true;
            }
            let state = &self.substitutions[index];
            if !(batch
                && state.actions.is_empty()
                && state.level == 0
                && self.frames.len() == owner + 1)
            {
                return Ok(None);
            }
        }
    }
    /// Scan a buffer's worth of text using one compiled pattern. Return to
    /// the native stack path when a match needs a callback or a flush.
    fn substitution_scan(
        &mut self,
        index: usize,
        replacement: Option<StringId>,
    ) -> Result<Scan, Error> {
        let pattern_index = self.prepare_pattern(self.substitutions[index].pattern, true)?;
        let compiled = self.patterns.get_at(pattern_index);
        if compiled.anchored {
            return Ok(Scan::Single);
        }
        let revision = self.pattern_locale;
        let first = compiled.leading_byte();
        let state = &mut self.substitutions[index];
        let text = &self.strings.bytes[slot_index(state.source.0)];
        let replacement = replacement.map(|id| &self.strings.bytes[slot_index(id.0)]);
        let before = self.pattern_work.heap_bytes();
        let result = (|| {
            loop {
                let cursor = state.cursor;
                let room = io::LINE_BUFFER - state.output.len();
                let limit = (cursor + room).min(text.len());
                let mut at = cursor;
                let found = loop {
                    if let Some(first) = first {
                        at += first.find_in(&text[at..limit]).unwrap_or(limit - at);
                    }
                    if at == limit && (at < text.len() || first.is_some()) {
                        break None;
                    }
                    if let Some(found) = compiled.match_span(text, at, &mut self.pattern_work)? {
                        break Some(found);
                    }
                    if at == text.len() {
                        break None;
                    }
                    at += 1;
                };
                state.output.extend_from_slice(&text[cursor..at]);
                state.cursor = at;
                let Some(found) = found else {
                    state.done = at == text.len();
                    return Ok(Scan::Advanced);
                };
                // A nonempty match can advance without a flush. Use its
                // capture offsets before the next scan reuses the work buffer.
                if let Some(replacement) = replacement
                    && found.end > found.start
                    && Self::append_replacement(
                        &mut state.output,
                        text,
                        replacement,
                        found.start..found.end,
                        |index| self.pattern_work.capture(found, index),
                    )
                {
                    state.cursor = found.end;
                    state.count += 1;
                    if state.count == state.maximum
                        || state.output.len() == io::LINE_BUFFER
                        || crate::locale::revision() != revision
                    {
                        return Ok(Scan::Advanced);
                    }
                } else {
                    state.matched = Some(self.pattern_work.materialize(found));
                    return Ok(Scan::Found);
                }
            }
        })();
        self.gc_note_control(self.pattern_work.heap_bytes().saturating_sub(before));
        result.map_err(|error| self.pattern_error(error))
    }
    fn string_format(&mut self, base: usize, count: usize) -> Result<usize, Error> {
        if self.registers.memory().allocator().is_none()
            && self.registers.memory().budget_usage().is_none()
            && self.try_plain_string_format(base, count)?
        {
            return Ok(1);
        }
        let mut scratch = crate::format::Scratch::new(self.registers.memory().clone());
        let result = self.string_format_inner(base, count, &mut scratch);
        self.gc_note_control(scratch.allocated);
        result
    }

    fn try_plain_string_format(&mut self, base: usize, count: usize) -> Result<bool, Error> {
        let Value::String(id) = self.argument(base, count, 0) else {
            return Ok(false);
        };
        let format = &self.strings.bytes[slot_index(id.0)];
        let plan = match self.plain_format {
            Some((cached, plan)) if cached == id => plan,
            _ => {
                let plan = PlainFormat::scan(format);
                self.plain_format = Some((id, plan));
                plan
            }
        };
        let Some(plan) = plan.filter(|plan| plan.fields < count) else {
            return Ok(false);
        };
        let mut arguments = [None; FORMAT_ARGUMENTS];
        let mut total = format.len() - 2 * plan.fields;
        let values = &self.registers[base + 2..base + 2 + plan.fields];
        for (input, &value) in arguments.iter_mut().zip(values) {
            let argument = match value {
                Value::String(id) => PlainFormatArgument::String(
                    crate::format::plain_string_argument(&self.strings.bytes[slot_index(id.0)]),
                ),
                // The default allocator and an unobserved native frame allow
                // formatting without publishing a coerced argument string.
                Value::Number(number) if self.hooks.mask == 0 => {
                    let Some(text) = native::NumberText::short_integer(number) else {
                        return Ok(false);
                    };
                    let text = text.as_bytes();
                    let mut bytes = [0; 15];
                    bytes[..text.len()].copy_from_slice(text);
                    PlainFormatArgument::Integer {
                        bytes,
                        len: text.len() as u8,
                    }
                }
                _ => return Ok(false),
            };
            let Some(size) = total.checked_add(argument.bytes().len()) else {
                return Ok(false);
            };
            total = size;
            *input = Some(argument);
        }
        let Some(capacity) = total.checked_add(1) else {
            return Ok(false);
        };
        let mut output = Buffer::new_in(self.registers.memory().clone());
        output
            .try_reserve_exact(capacity)
            .map_err(|_| self.memory_failure())?;
        let mut start = 0;
        for (&end, argument) in plan.ends[..plan.fields].iter().zip(arguments) {
            let end = usize::from(end);
            output.extend_from_slice(&format[start..end]);
            output.extend_from_slice(argument.expect("validated format argument").bytes());
            start = end + 2;
        }
        output.extend_from_slice(&format[start..]);
        self.registers[base] = Value::String(self.strings.try_intern_owned(output)?);
        Ok(true)
    }
    fn string_format_inner(
        &mut self,
        base: usize,
        count: usize,
        scratch: &mut crate::format::Scratch,
    ) -> Result<usize, Error> {
        let id = self.string_argument(base, count, 0)?;
        let slot = slot_index(id.0);
        let mut output = crate::format::buffer(scratch, self.strings.bytes[slot].len())?;
        let mut pos = 0;
        let mut arg = 0;
        while pos < self.strings.bytes[slot].len() {
            let bytes = &self.strings.bytes[slot];
            if bytes[pos] != b'%' {
                // Fill the current buffer before requesting its next growth.
                let room = (output.capacity() - output.len()).max(1);
                let literal = &bytes[pos..pos + room.min(bytes.len() - pos)];
                let len = find_byte(&literal[1..], b'%').map_or(literal.len(), |at| at + 1);
                scratch.reserve(&mut output, len)?;
                output.extend_from_slice(&literal[..len]);
                pos += len;
                continue;
            }
            pos += 1;
            if bytes.get(pos) == Some(&b'%') {
                let tail = &bytes[pos + 1..];
                if tail.starts_with(b"%%") {
                    // Stop before a growth so the allocator sees the same prefix.
                    let room = output.capacity() - output.len();
                    if room > 1 {
                        let limit = (tail.len() / 2).min(room - 1) * 2;
                        let extra = byte_run(&tail[..limit], b'%') / 2;
                        output.resize(output.len() + 1 + extra, b'%');
                        pos += 1 + 2 * extra;
                        continue;
                    }
                }
                scratch.reserve(&mut output, 1)?;
                output.push(b'%');
                pos += 1;
                continue;
            }
            arg += 1;
            if arg >= count {
                return Err(self.argument_error(arg, b"no value"));
            }
            let spec = crate::format::Spec::parse(bytes, &mut pos)?;
            match spec.conversion {
                b's' | b'q' => {
                    let text = self.string_argument(base, count, arg)?;
                    let bytes = &self.strings.bytes[slot_index(text.0)];
                    if spec.conversion == b'q' {
                        crate::format::quote(bytes, &mut output, scratch)?;
                    } else if let Some(bytes) = spec.plain_string_bytes(bytes) {
                        scratch.reserve(&mut output, bytes.len())?;
                        output.extend_from_slice(bytes);
                    } else {
                        let bytes = spec.string(bytes, scratch)?;
                        scratch.reserve(&mut output, bytes.len())?;
                        output.extend_from_slice(&bytes);
                    }
                }
                b'c' | b'd' | b'i' | b'o' | b'u' | b'x' | b'X' | b'e' | b'E' | b'f' | b'g'
                | b'G' => {
                    let n = self
                        .number(self.argument(base, count, arg))
                        .ok_or_else(|| self.argument_type_error(base, count, arg, "number"))?;
                    let bytes = match spec.conversion {
                        b'c' => spec.character(n, scratch)?,
                        b'd' | b'i' | b'o' | b'u' | b'x' | b'X' => spec.integer(n, scratch)?,
                        _ => spec.float(n, scratch)?,
                    };
                    scratch.reserve(&mut output, bytes.len())?;
                    output.extend_from_slice(&bytes);
                }
                c => {
                    let mut message = b"invalid option '%".to_vec();
                    // Lua 5.1's diagnostic %c passes a two-byte C string, so
                    // a NUL conversion contributes an empty string.
                    if c != 0 {
                        message.push(c);
                    }
                    message.extend_from_slice(b"' to 'format'");
                    return Err(Error::from_bytes(ErrorKind::Runtime, 0, message));
                }
            }
        }
        self.registers[base] = Value::String(self.strings.try_intern(&output)?);
        Ok(1)
    }

    pub(super) fn dump_step(&mut self, mut frame: Frame) -> Result<Option<usize>, Error> {
        let args = frame.vararg_start - 1;
        let function = self.argument(args, frame.argc(), 0);
        self.resize_registers(self.registers.len().max(frame.base + frame.argc() + 1));
        self.registers.copy_within(
            frame.vararg_start..frame.vararg_start + frame.argc(),
            frame.base + 1,
        );
        self.string_native(STR_DUMP, frame.base, frame.argc())?;
        let value = self.registers[frame.base];
        self.registers[frame.base] = function;
        self.registers[frame.base + 1] = value;
        frame.vararg_start = frame.base;
        frame.vararg_count = 2;
        frame.pc = 1;
        let index = self.frames.len() - 1;
        self.frames[index] = frame;
        self.return_frame(frame, frame.base + 1, 1)
    }
}

#[cfg(test)]
mod format_allocation_tests;
