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
pub(super) struct MatchIterator {
    source: StringId,
    pattern: StringId,
    position: usize,
}
#[derive(Debug)]
pub(super) struct Substitution {
    pub owner: usize,
    source: StringId,
    pattern: StringId,
    replacement: Value,
    cursor: usize,
    count: i32,
    maximum: i32,
    output: Vec<u8>,
    matched: Option<Matched>,
    done: bool,
}
impl Substitution {
    pub(super) fn roots(&self) -> [Value; 3] {
        [
            Value::String(self.source),
            Value::String(self.pattern),
            self.replacement,
        ]
    }
    pub(super) fn heap_bytes(&self) -> usize {
        self.output.capacity()
    }
}
impl MatchIterator {
    pub(super) fn roots(&self) -> [Value; 2] {
        [Value::String(self.source), Value::String(self.pattern)]
    }
}

fn relative(index: i64, len: usize) -> i64 {
    if index < 0 {
        index.saturating_add(len as i64).saturating_add(1).max(0)
    } else {
        index
    }
}
fn integer(number: f64) -> i64 {
    if !number.is_finite() || number >= 9223372036854775808.0 || number < -9223372036854775808.0 {
        i64::MIN
    } else {
        number as i64
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
            Value::Number(n) => Ok(self.strings.intern(format_number(n).as_bytes())),
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
            .map(integer)
            .ok_or_else(|| self.argument_type_error(base, count, index, "number"))
    }
    fn prepare_pattern(&mut self, id: StringId, anchor: bool) {
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
            return Err(Error::new(
                ErrorKind::Unsupported,
                0,
                "binary function dumps are not implemented yet",
            ));
        }
        if id == STR_FORMAT {
            return self.string_format(base, count);
        }
        if id == STR_CHAR {
            let mut bytes = self.take_buffer(count);
            for index in 0..count {
                let n = self.integer_argument(base, count, index, None)?;
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
                    let count = range.len();
                    self.resize_registers(self.registers.len().max(base + count));
                    for (i, &byte) in self.strings.bytes[slot_index(source.0)][range]
                        .iter()
                        .enumerate()
                    {
                        self.registers[base + i] = Value::Number(f64::from(byte));
                    }
                    return Ok(count);
                }
            }
            STR_REP => {
                let repeat = self.integer_argument(base, count, 1, None)?.max(0) as usize;
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
                    bytes.extend(text.iter().map(|b| {
                        if id == STR_LOWER {
                            b.to_ascii_lowercase()
                        } else {
                            b.to_ascii_uppercase()
                        }
                    }));
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
                self.new_native(threads::NativeKind::Match(MatchIterator {
                    source,
                    pattern,
                    position: 0,
                }))?
            }
            _ => unreachable!("string native ID"),
        };
        self.registers[base] = value;
        Ok(1)
    }
    pub(super) fn match_iterator(&mut self, id: u64, base: usize) -> Result<usize, Error> {
        let index = slot_index(id);
        let Some(threads::NativeClosure {
            kind: threads::NativeKind::Match(iter),
            ..
        }) = self.native_closures.get(id)
        else {
            return Err(Error::new(ErrorKind::Runtime, 0, "unknown native closure"));
        };
        let (source, pattern, position) = (iter.source, iter.pattern, iter.position);
        let found = self.patterns[&(pattern, false)].find(
            &self.strings.bytes[slot_index(source.0)],
            position,
            &mut self.pattern_work,
        )?;
        if let Some(found) = found {
            let threads::NativeKind::Match(iter) = &mut self.native_closures[index].kind else {
                unreachable!("pattern iterator kind")
            };
            iter.position = found.end + usize::from(found.end == found.start);
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
    fn substitution_advance(&mut self, index: usize) {
        let state = &mut self.substitutions[index];
        let matched = state.matched.take().expect("substitution has a match");
        let source = &self.strings.bytes[slot_index(state.source.0)];
        if matched.end > matched.start {
            state.cursor = matched.end;
        } else if state.cursor < source.len() {
            state.output.push(source[state.cursor]);
            state.cursor += 1;
        } else {
            state.done = true;
        }
        if self.patterns[&(state.pattern, true)].anchored {
            state.done = true;
        }
    }
    fn replacement_string(&mut self, index: usize, text: StringId) -> Result<(), Error> {
        let state = &self.substitutions[index];
        let found = state.matched.expect("substitution has a match");
        let source = state.source;
        let text = &self.strings.bytes[slot_index(text.0)];
        let mut pos = 0;
        let bytes = &mut self.substitutions[index].output;
        while pos < text.len() {
            if text[pos] != b'%' {
                bytes.push(text[pos]);
                pos += 1;
                continue;
            }
            pos += 1;
            let c = text.get(pos).copied().unwrap_or(0);
            pos += 1;
            if c == b'0' {
                bytes.extend_from_slice(
                    &self.strings.bytes[slot_index(source.0)][found.start..found.end],
                );
            } else if c.is_ascii_digit() {
                let capture = found.capture(usize::from(c - b'1'))?;
                if capture.position() {
                    bytes.extend_from_slice(format_number((capture.start + 1) as f64).as_bytes());
                } else {
                    bytes.extend_from_slice(
                        &self.strings.bytes[slot_index(source.0)][capture.range()?],
                    );
                }
            } else {
                bytes.push(c);
            }
        }
        Ok(())
    }
    pub(super) fn substitute_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        let base = frame.base;
        let index = if frame.pc() == 0 {
            let args = frame.vararg_start - 1;
            let argc = frame.argc();
            let source = self.string_argument(args, argc, 0)?;
            let pattern = self.string_argument(args, argc, 1)?;
            let mut replacement = self.argument(args, argc, 2);
            if let Value::Number(n) = replacement {
                replacement = self.intern(format_number(n).as_bytes());
            }
            if !matches!(
                replacement,
                Value::String(_) | Value::Table(_) | Value::Function(_) | Value::Native(_)
            ) {
                return Err(self.argument_error(2, b"string/function/table expected"));
            }
            let len = self.strings.bytes[slot_index(source.0)].len();
            let maximum =
                self.integer_argument(args, argc, 3, Some(len.saturating_add(1) as i64))? as i32;
            self.prepare_pattern(pattern, true);
            let output = self.take_buffer(len);
            let index = self.substitutions.len();
            self.substitutions.push(Substitution {
                owner,
                source,
                pattern,
                replacement,
                cursor: 0,
                count: 0,
                maximum,
                output,
                matched: None,
                done: false,
            });
            self.frames[owner].pc = ((index + 1) << 1) as u32;
            index
        } else {
            (frame.pc() >> 1) - 1
        };
        if frame.pc() & 1 != 0 {
            let value = self.registers[base];
            let state = &self.substitutions[index];
            if !value.is_truthy() {
                let matched = state.matched.expect("substitution has a match");
                let source = state.source;
                self.substitutions[index].output.extend_from_slice(
                    &self.strings.bytes[slot_index(source.0)][matched.start..matched.end],
                );
            } else {
                let string = match value {
                    Value::String(id) => id,
                    Value::Number(n) => self.strings.intern(format_number(n).as_bytes()),
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Runtime,
                            0,
                            format!("invalid replacement value (a {})", value.type_name()),
                        ));
                    }
                };
                self.substitutions[index]
                    .output
                    .extend_from_slice(&self.strings.bytes[slot_index(string.0)]);
            }
            self.substitution_advance(index);
            self.frames[owner].pc = ((index + 1) << 1) as u32;
        }
        if self.substitutions[index].done
            || self.substitutions[index].count >= self.substitutions[index].maximum
        {
            let mut state = self
                .substitutions
                .pop()
                .expect("active substitution exists");
            assert_eq!(
                index,
                self.substitutions.len(),
                "substitution stack is ordered"
            );
            state
                .output
                .extend_from_slice(&self.strings.bytes[slot_index(state.source.0)][state.cursor..]);
            self.registers[base] = self.finish_buffer(state.output);
            self.registers[base + 1] = Value::Number(f64::from(state.count));
            return Ok(self.return_frame(frame, base, 2));
        }
        let state = &self.substitutions[index];
        let (source, pattern, cursor, replacement) =
            (state.source, state.pattern, state.cursor, state.replacement);
        Pattern::reset(&mut self.pattern_work);
        let found = self.patterns[&(pattern, true)].match_at(
            &self.strings.bytes[slot_index(source.0)],
            cursor,
            &mut self.pattern_work,
        )?;
        if let Some(found) = found {
            self.substitutions[index].matched = Some(found);
            self.substitutions[index].count += 1;
            match replacement {
                Value::String(text) => {
                    self.replacement_string(index, text)?;
                    self.substitution_advance(index);
                }
                Value::Table(_) => {
                    let key = self.capture_value(source, found.capture(0)?)?;
                    self.frames[owner].pc |= 1;
                    self.get_table(replacement, key, base)?;
                }
                Value::Function(_) | Value::Native(_) => {
                    let argc = self.capture_results(base + 1, source, found, true)?;
                    self.registers[base] = replacement;
                    self.frames[owner].pc |= 1;
                    if let Some(actual) = self.start_call(base, argc, 1, false)? {
                        self.finish_call(base, actual, 1);
                    }
                }
                _ => unreachable!("substitution replacement type checked"),
            }
        } else {
            let state = &mut self.substitutions[index];
            let source = &self.strings.bytes[slot_index(source.0)];
            if cursor < source.len() {
                state.output.push(source[cursor]);
                state.cursor += 1;
            } else {
                state.done = true;
            }
            if self.patterns[&(pattern, true)].anchored {
                state.done = true;
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
                    return Err(Error::new(
                        ErrorKind::Runtime,
                        0,
                        format!("invalid option '%{}' to 'format'", c as char),
                    ));
                }
            }
        }
        self.registers[base] = self.finish_buffer(output);
        Ok(1)
    }
}
