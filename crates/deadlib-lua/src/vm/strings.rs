//! Byte strings, pattern callbacks, iterators and Lua 5.1 formatting.
use super::*;
use crate::pattern::{Capture, Matched, Pattern};

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
pub(super) const STR_DUMP: u64 = 77;
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
    output: Vec<u8>,
    // Buffer concatenations can invoke Lua after debug mutation of C slots.
    // Continuations allocate only when such a metamethod actually suspends us.
    actions: Vec<SubAction>,
    matched: Option<Matched>,
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

fn relative(index: i64, len: usize) -> i64 {
    if index < 0 {
        index.saturating_add(len as i64).saturating_add(1).max(0)
    } else {
        index
    }
}
impl Vm {
    pub(super) fn open_strings(&mut self) {
        let table = self.new_table();
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
            let key = self.intern(name);
            self.tables[slot_index(table.0)]
                .set(key, Value::Native(STR_FIRST + i as u64))
                .expect("string names are valid");
        }
        let key = self.intern(b"gfind");
        self.tables[slot_index(table.0)]
            .set(key, Value::Native(STR_GMATCH))
            .expect("string names are valid");
        self.set_global(b"string", Value::Table(table))
            .expect("string name is valid");
        let mt = self.new_table();
        self.tables[slot_index(mt.0)]
            .set(
                Value::String(self.meta_names[Meta::Index as usize]),
                Value::Table(table),
            )
            .expect("string metatable key is valid");
        self.type_metatables[3] = Some(mt);
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
                let id = self.strings.intern(format_number(n).as_slice());
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
    fn prepare_pattern(&mut self, id: StringId, anchor: bool) {
        let revision = crate::locale::revision();
        if revision != self.pattern_locale {
            self.patterns.clear();
            self.pattern_locale = revision;
        }
        self.patterns
            .entry((id, anchor))
            .or_insert_with(|| Pattern::compile(&self.strings.bytes[slot_index(id.0)], anchor));
    }
    fn capture_value(&mut self, source: StringId, capture: Capture) -> Result<Value, Error> {
        if capture.position() {
            Ok(Value::Number((capture.start + 1) as f64))
        } else {
            Ok(Value::String(
                self.strings.substring(source, capture.range()?),
            ))
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
        self.resize_registers(self.registers.len().max(base + count));
        for index in 0..count {
            let capture = matched.capture(index)?;
            self.registers[base + index] = self.capture_value(source, capture)?;
        }
        Ok(count)
    }
    pub(super) fn string_native(
        &mut self,
        id: u64,
        base: usize,
        count: usize,
    ) -> Result<usize, Error> {
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
            let bytes =
                crate::chunk51::dump(&self.programs[closure.program].data, closure.prototype)?;
            self.registers[base] = self.intern(&bytes);
            return Ok(1);
        }
        if id == STR_FORMAT {
            return self.string_format(base, count);
        }
        if id == STR_CHAR {
            let mut bytes = self.take_buffer(count);
            for index in 0..count {
                let n = self.integer_argument(base, count, index, None)? as i32;
                if !(0..=255).contains(&n) {
                    return Err(self.argument_error(index, b"invalid value"));
                }
                bytes.push(n as u8);
            }
            self.registers[base] = self.finish_buffer(bytes);
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
                let start = start.max(1);
                let end = end.min(len as i64);
                let range = if end < start {
                    0..0
                } else {
                    start as usize - 1..end as usize
                };
                if id == STR_SUB {
                    Value::String(self.strings.substring(source, range))
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
                    self.resize_registers(self.registers.len().max(base + results));
                    for (i, &byte) in self.strings.bytes[slot_index(source.0)][range]
                        .iter()
                        .enumerate()
                    {
                        self.registers[base + i] = Value::Number(f64::from(byte));
                    }
                    return Ok(results);
                }
            }
            STR_REP => {
                let repeat = (self.integer_argument(base, count, 1, None)? as i32).max(0) as usize;
                if len == 0 || repeat == 0 {
                    self.registers[base] = self.intern(b"");
                    return Ok(1);
                }
                let size = len
                    .checked_mul(repeat)
                    .ok_or_else(|| Error::new(ErrorKind::Limit, 0, "string length overflow"))?;
                let mut bytes = self.take_buffer(0);
                bytes
                    .try_reserve_exact(size)
                    .map_err(|_| Error::new(ErrorKind::Limit, 0, "string length overflow"))?;
                for _ in 0..repeat {
                    bytes.extend_from_slice(&self.strings.bytes[slot_index(source.0)]);
                }
                self.finish_buffer(bytes)
            }
            STR_REVERSE | STR_LOWER | STR_UPPER => {
                let mut bytes = self.take_buffer(len);
                let text = &self.strings.bytes[slot_index(source.0)];
                if id == STR_REVERSE {
                    bytes.extend(text.iter().rev().copied());
                } else {
                    crate::locale::map_case(text, id == STR_UPPER, &mut bytes);
                }
                self.finish_buffer(bytes)
            }
            STR_FIND | STR_MATCH => {
                let pattern = self.string_argument(base, count, 1)?;
                let start = relative(self.integer_argument(base, count, 2, Some(1))?, len)
                    .saturating_sub(1)
                    .clamp(0, len as i64) as usize;
                let raw = self.strings.bytes[slot_index(pattern.0)].as_ref();
                let c_pattern = &raw[..raw.iter().position(|&b| b == 0).unwrap_or(raw.len())];
                let plain = id == STR_FIND
                    && (self.argument(base, count, 3).is_truthy()
                        || !c_pattern.iter().any(|b| b"^$*+?.([%-".contains(b)));
                if plain {
                    let text = &self.strings.bytes[slot_index(source.0)];
                    let found = if raw.is_empty() {
                        Some(start)
                    } else {
                        text[start..]
                            .windows(raw.len())
                            .position(|w| w == raw)
                            .map(|i| start + i)
                    };
                    if let Some(pos) = found {
                        let end = pos + raw.len();
                        self.resize_registers(self.registers.len().max(base + 2));
                        self.registers[base] = Value::Number((pos + 1) as f64);
                        self.registers[base + 1] = Value::Number(end as f64);
                        return Ok(2);
                    }
                    Value::Nil
                } else {
                    self.prepare_pattern(pattern, true);
                    let found = self.patterns[&(pattern, true)].find(
                        &self.strings.bytes[slot_index(source.0)],
                        start,
                        &mut self.pattern_work,
                    )?;
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
                self.prepare_pattern(pattern, false);
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
                Value::Number(number) => self.intern(&native::format_number(number)),
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
        self.prepare_pattern(pattern, false);
        let found = self.patterns[&(pattern, false)].find(
            &self.strings.bytes[slot_index(source.0)],
            position,
            &mut self.pattern_work,
        )?;
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
            let state = self.substitutions.pop().expect("substitution exists");
            self.buffers.push(state.output);
        }
    }
    fn substitution_push(&mut self, index: usize, value: Value) {
        let state = &mut self.substitutions[index];
        let frame = &mut self.frames[state.owner];
        let position = frame.vararg_start + state.top;
        state.top += 1;
        frame.vararg_count = state.top as u32;
        self.resize_registers(self.registers.len().max(position + 1));
        self.registers[position] = value;
    }
    fn substitution_pop(&mut self, index: usize, count: usize) {
        let state = &mut self.substitutions[index];
        state.top -= count;
        let frame = &mut self.frames[state.owner];
        frame.vararg_count = state.top as u32;
        self.registers.truncate(frame.vararg_start + state.top);
    }
    fn substitution_flush(&mut self, index: usize) -> bool {
        if self.substitutions[index].output.is_empty() {
            return false;
        }
        let mut bytes = std::mem::take(&mut self.substitutions[index].output);
        let value = self.intern(&bytes);
        bytes.clear();
        self.substitutions[index].output = bytes;
        self.substitution_push(index, value);
        self.substitutions[index].level += 1;
        true
    }
    fn substitution_concat_done(&mut self, index: usize, count: usize) {
        let state = &self.substitutions[index];
        let start = self.frames[state.owner].vararg_start + state.top - count;
        let value = self.registers[start];
        self.substitution_pop(index, count);
        self.substitution_push(index, value);
    }
    fn substitution_concat(&mut self, index: usize, count: usize) -> Result<bool, Error> {
        if count == 1 {
            return Ok(true);
        }
        if count == 0 {
            let value = self.intern(b"");
            self.substitution_push(index, value);
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
            self.substitutions[index]
                .actions
                .push(SubAction::Concat(count));
            return Ok(false);
        }
        self.substitution_concat_done(index, count);
        Ok(true)
    }
    fn substitution_length(&mut self, position: usize) -> usize {
        let mut value = self.registers[position];
        if matches!(value, Value::Number(_)) {
            value = self.default_string(value);
            self.registers[position] = value;
        }
        match value {
            Value::String(id) => self.strings.bytes[slot_index(id.0)].len(),
            Value::Table(id) => self.tables[slot_index(id.0)].len(),
            Value::Userdata(id) => self.userdata[slot_index(id.0)].bytes.len(),
            _ => 0,
        }
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
        let mut size = self.substitution_length(end - 1);
        let mut count = 1;
        while count < level {
            let next = self.substitution_length(end - count - 1);
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
            self.substitution_flush(index);
            if !self.substitution_adjust(index)? {
                self.substitutions[index]
                    .actions
                    .insert(queued, SubAction::Byte(byte));
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
                self.substitution_flush(index);
                if !self.substitution_adjust(index)? {
                    self.substitutions[index]
                        .actions
                        .insert(queued, SubAction::String(source, cursor..range.end));
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
        let value = if matches!(value, Value::Number(_)) {
            self.default_string(value)
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
            if self.substitution_flush(index) {
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
            let found = state.matched.expect("substitution has a match");
            let source = state.source;
            self.substitution_pop(index, 1);
            let value = Value::String(self.strings.substring(source, found.start..found.end));
            self.substitution_push(index, value);
        }
        self.substitution_value(index)
    }
    fn substitution_advance(&mut self, index: usize) -> Result<bool, Error> {
        let state = &self.substitutions[index];
        let matched = state.matched.expect("substitution has a match");
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
        if self.strings.bytes[slot_index(state.pattern.0)].first() == Some(&b'^') {
            state.done = true;
        }
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
        let found = state.matched.expect("substitution has a match");
        let source = state.source;
        while pos < self.strings.bytes[slot_index(text.0)].len() {
            let c = self.strings.bytes[slot_index(text.0)][pos];
            if c != b'%' {
                pos += 1;
                if !self.substitution_byte(index, c)? {
                    self.substitutions[index]
                        .actions
                        .insert(queued, SubAction::Replacement(text, pos));
                    return Ok(false);
                }
                continue;
            }
            pos += 1;
            let c = self.strings.bytes[slot_index(text.0)]
                .get(pos)
                .copied()
                .unwrap_or(0);
            pos += 1;
            if c == b'0' {
                if !self.substitution_string(index, source, found.start..found.end)? {
                    self.substitutions[index]
                        .actions
                        .insert(queued, SubAction::Replacement(text, pos));
                    return Ok(false);
                }
            } else if crate::locale::is_digit(c) {
                let capture = found.capture(usize::from(c - b'1'))?;
                let value = self.capture_value(source, capture)?;
                self.substitution_push(index, value);
                if !self.substitution_value(index)? {
                    self.substitutions[index]
                        .actions
                        .insert(queued, SubAction::Replacement(text, pos));
                    return Ok(false);
                }
            } else {
                if !self.substitution_byte(index, c)? {
                    self.substitutions[index]
                        .actions
                        .insert(queued, SubAction::Replacement(text, pos));
                    return Ok(false);
                }
            }
        }
        Ok(true)
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
                self.substitutions[index]
                    .actions
                    .insert(queued, SubAction::Complete(1));
                return Ok(None);
            }
        }
        if phase <= 1 {
            self.substitution_flush(index);
            if !self.substitution_concat(index, self.substitutions[index].level)? {
                self.substitutions[index]
                    .actions
                    .insert(queued, SubAction::Complete(2));
                return Ok(None);
            }
        }
        let count = self.substitutions[index].count;
        self.substitution_push(index, Value::Number(f64::from(count)));
        let state = self
            .substitutions
            .pop()
            .expect("active substitution exists");
        assert_eq!(
            index,
            self.substitutions.len(),
            "substitution stack is ordered"
        );
        self.buffers.push(state.output);
        let frame = self.frames[state.owner];
        Ok(self.return_frame(frame, frame.vararg_start + state.top - 2, 2))
    }
    pub(super) fn substitute_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        let base = frame.base;
        let index = if frame.pc() == 0 {
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
            self.prepare_pattern(pattern, true);
            let output = self.take_buffer(io::LINE_BUFFER);
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
                actions: Vec::new(),
                matched: None,
                done: false,
            });
            self.frames[owner].pc = ((index + 1) << 2) as u32;
            index
        } else {
            (frame.pc() >> 2) - 1
        };
        if let Some(action) = self.substitutions[index].actions.pop() {
            match action {
                SubAction::Concat(count) => self.substitution_concat_done(index, count),
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
                self.registers.truncate(end + 1);
                self.substitution_push(index, value);
            } else {
                self.registers.truncate(end);
            }
            let complete = self.substitution_replacement(index)?;
            self.frames[owner].pc = ((index + 1) << 2) as u32;
            if !complete {
                self.substitutions[index]
                    .actions
                    .insert(0, SubAction::Advance);
                return Ok(None);
            }
            if !self.substitution_advance(index)? {
                return Ok(None);
            }
        }
        if self.substitutions[index].done
            || self.substitutions[index].count >= self.substitutions[index].maximum
        {
            return self.substitution_complete(index, 0);
        }
        let state = &self.substitutions[index];
        let (source, pattern, cursor) = (state.source, state.pattern, state.cursor);
        self.prepare_pattern(pattern, true);
        let found = self.patterns[&(pattern, true)].match_at(
            &self.strings.bytes[slot_index(source.0)],
            cursor,
            &mut self.pattern_work,
        )?;
        if let Some(found) = found {
            self.substitutions[index].matched = Some(found);
            self.substitutions[index].count += 1;
            let frame = self.frames[owner];
            let replacement =
                self.argument(frame.vararg_start - 1, self.substitutions[index].top, 2);
            match replacement {
                Value::String(_) | Value::Number(_) => {
                    let text = self.string_argument(frame.vararg_start - 1, frame.argc(), 2)?;
                    if !self.replacement_string(index, text, 0)? {
                        self.substitutions[index]
                            .actions
                            .insert(0, SubAction::Advance);
                        return Ok(None);
                    }
                    if !self.substitution_advance(index)? {
                        return Ok(None);
                    }
                }
                Value::Table(_) => {
                    let key = self.capture_value(source, found.capture(0)?)?;
                    self.substitution_push(index, key);
                    let call = frame.vararg_start + self.substitutions[index].top - 1;
                    self.frames[owner].pc |= 2;
                    self.get_table(replacement, key, call)?;
                }
                Value::Function(_) | Value::Native(_) => {
                    let call = frame.vararg_start + self.substitutions[index].top;
                    let argc = self.capture_results(call + 1, source, found, true)?;
                    self.registers[call] = replacement;
                    self.frames[owner].pc |= 1;
                    if let Some(actual) = self.start_call(call, argc, 1, false)? {
                        self.finish_call(call, actual, 1);
                    }
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
                        self.substitutions[index]
                            .actions
                            .insert(0, SubAction::Advance);
                        return Ok(None);
                    }
                    if !self.substitution_advance(index)? {
                        return Ok(None);
                    }
                }
            }
        } else {
            if cursor < self.strings.bytes[slot_index(source.0)].len() {
                self.substitutions[index].cursor += 1;
                if self.patterns[&(pattern, true)].anchored {
                    self.substitutions[index].done = true;
                }
                self.substitution_byte(index, self.strings.bytes[slot_index(source.0)][cursor])?;
            } else {
                self.substitutions[index].done = true;
            }
            if self.patterns[&(pattern, true)].anchored {
                self.substitutions[index].done = true;
            }
        }
        Ok(None)
    }
    fn string_format(&mut self, base: usize, count: usize) -> Result<usize, Error> {
        let id = self.string_argument(base, count, 0)?;
        let mut output = self.take_buffer(self.strings.bytes[slot_index(id.0)].len());
        let mut pos = 0;
        let mut arg = 0;
        while pos < self.strings.bytes[slot_index(id.0)].len() {
            let bytes = &self.strings.bytes[slot_index(id.0)];
            let c = bytes[pos];
            pos += 1;
            if c != b'%' {
                output.push(c);
                continue;
            }
            if bytes.get(pos) == Some(&b'%') {
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
                        crate::format::quote(bytes, &mut output);
                    } else {
                        output.extend_from_slice(&spec.string(bytes));
                    }
                }
                b'c' | b'd' | b'i' | b'o' | b'u' | b'x' | b'X' | b'e' | b'E' | b'f' | b'g'
                | b'G' => {
                    let n = self
                        .number(self.argument(base, count, arg))
                        .ok_or_else(|| self.argument_type_error(base, count, arg, "number"))?;
                    let bytes = match spec.conversion {
                        b'c' => spec.character(n),
                        b'd' | b'i' | b'o' | b'u' | b'x' | b'X' => spec.integer(n),
                        _ => spec.float(n),
                    };
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
        self.registers[base] = self.finish_buffer(output);
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
        Ok(self.return_frame(frame, frame.base + 1, 1))
    }
}
