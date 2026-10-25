//! Native library functions and explicit callback continuations.
use super::*;
use std::io::Write;

impl Vm {
    pub(super) fn finish_call(&mut self, call: usize, actual: usize, wanted: u32) {
        let wanted = if wanted == OPEN {
            actual
        } else {
            wanted as usize
        };
        self.resize_registers(self.registers.len().max(call + wanted));
        if wanted > actual {
            self.registers[call + actual..call + wanted].fill(Value::Nil);
        }
        self.top = call + wanted;
    }
    // Return Some(result count) for immediate calls; None means a frame was scheduled.
    pub(super) fn start_call(
        &mut self,
        call: usize,
        mut count: usize,
        wanted: u32,
        tail: bool,
    ) -> Result<Option<usize>, Error> {
        if !matches!(self.registers[call], Value::Function(_) | Value::Native(_)) {
            let target = self.registers[call];
            let handler = self.metafield(target, Meta::Call);
            if !matches!(handler, Value::Function(_) | Value::Native(_)) {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    0,
                    format!("attempt to call a {} value", target.type_name()),
                ));
            }
            self.resize_registers(self.registers.len().max(call + count + 2));
            self.registers.copy_within(call..call + count + 1, call + 1);
            self.registers[call] = handler;
            count += 1;
        }
        match self.registers[call] {
            Value::Function(id) => {
                if tail {
                    self.tail_frame(id, call, count)?;
                } else {
                    self.push_frame(id, call, count, wanted)?;
                }
                Ok(None)
            }
            Value::Native(id)
                if matches!(
                    id,
                    PCALL
                        | LOAD_READER
                        | COLLECT_GARBAGE
                        | XPCALL
                        | TOSTRING
                        | PRINT
                        | strings::STR_GSUB
                        | tables::TAB_FOREACH
                        | tables::TAB_FOREACHI
                        | tables::TAB_SORT
                        | threads::CO_RESUME
                        | threads::CO_YIELD
                ) || self.is_wrapper(id) =>
            {
                if self.frames.len() >= 10000 {
                    return Err(Error::new(ErrorKind::Runtime, 0, "C stack overflow"));
                }
                let base = self.registers.len();
                self.resize_registers(base + count.max(2) + 4);
                self.push_record(Frame {
                    function: FrameKind::Native(id),
                    pc: 0,
                    base,
                    stack_start: base,
                    result: call,
                    wanted: if tail {
                        FRAME_OPEN
                    } else {
                        frame_wanted(wanted)
                    },
                    barriers: self.frames.last().map_or(1, |f| f.barriers + 1),
                    vararg_start: call + 1,
                    vararg_count: count as u32,
                    tail_calls: 0,
                });
                Ok(None)
            }
            Value::Native(id) => {
                if self.frames.len() >= 10000 {
                    return Err(Error::new(ErrorKind::Runtime, 0, "C stack overflow"));
                }
                let index = self.frames.len();
                let base = self.registers.len();
                self.push_record(Frame {
                    function: FrameKind::Native(id),
                    pc: 0,
                    base,
                    stack_start: base,
                    result: call,
                    wanted: frame_wanted(wanted),
                    barriers: self.frames.last().map_or(1, |f| f.barriers + 1),
                    vararg_start: call + 1,
                    vararg_count: count as u32,
                    tail_calls: 0,
                });
                let previous = self.active_native.replace(index);
                let result = self.native(id, call, count).map(Some).map_err(|error| {
                    if error.kind() == ErrorKind::Runtime && error.value().is_none() {
                        let value = self.intern(error.message().as_bytes());
                        let value = self.located_error(value, 1);
                        error.with_value(value)
                    } else {
                        error
                    }
                });
                self.active_native = previous;
                if result.is_ok() {
                    self.frames.pop();
                }
                result
            }
            _ => unreachable!("call target was normalized"),
        }
    }
    pub(super) fn meta_call(
        &mut self,
        function: Value,
        args: &[Value],
        result: usize,
        continuation: u64,
    ) -> Result<(), Error> {
        if self.frames.len() >= 10000 {
            return Err(Error::new(ErrorKind::Runtime, 0, "C stack overflow"));
        }
        let base = self.registers.len();
        self.registers.push(function);
        self.registers.extend_from_slice(args);
        self.push_record(Frame {
            function: FrameKind::Native(continuation),
            pc: 1,
            base,
            stack_start: base,
            result,
            wanted: if continuation == CONT_IGNORE { 0 } else { 1 },
            barriers: self.frames.last().map_or(1, |f| f.barriers + 1),
            vararg_start: 0,
            vararg_count: 0,
            tail_calls: 0,
        });
        let wanted = if continuation == CONT_IGNORE { 0 } else { 1 };
        if let Some(actual) = self.start_call(base, args.len(), wanted, false)? {
            self.finish_call(base, actual, wanted);
        }
        Ok(())
    }
    pub(super) fn protected_failure(&mut self, index: usize, value: Value) {
        let frame = self.frames[index];
        self.close_upvalues(frame.base);
        self.discard_finalizers(index + 1);
        self.frames.truncate(index + 1);
        self.trim_substitutions();
        self.trim_sorts();
        self.registers.truncate(frame.base);
        let first = if matches!(frame.function, FrameKind::Native(LOAD_READER)) {
            Value::Nil
        } else {
            Value::Boolean(false)
        };
        self.registers.extend_from_slice(&[first, value]);
        self.frames[index].pc = 2;
        self.trim_loads();
        self.top = frame.base + 2;
    }
    pub(super) fn catch_error(&mut self, error: &Error, _line: u32) -> Result<bool, Error> {
        let Some(index) = self.frames.iter().rposition(|f| {
            (matches!(f.function, FrameKind::Native(PCALL | XPCALL)) && matches!(f.pc, 1 | 3))
                || (matches!(f.function, FrameKind::Native(LOAD_READER)) && matches!(f.pc, 1 | 3))
        }) else {
            return Ok(false);
        };
        let frame = self.frames[index];
        let value = if let Some(value) = error.value() {
            value
        } else {
            let value = self.intern(error.message().as_bytes());
            self.located_error(value, 1)
        };
        // lua_load protects the parser while retaining the current error
        // function. An enclosing xpcall transforms reader runtime errors even
        // though the loader subsequently returns nil plus that transformed value.
        let inherited = if matches!(frame.function, FrameKind::Native(LOAD_READER)) {
            self.frames[..index].iter().rposition(|f| {
                (matches!(f.function, FrameKind::Native(PCALL | XPCALL)) && matches!(f.pc, 1 | 3))
                    || (matches!(f.function, FrameKind::Native(LOAD_READER)) && f.pc == 3)
            })
        } else {
            None
        };
        let handler_index = if matches!(frame.function, FrameKind::Native(XPCALL)) && frame.pc == 1
        {
            Some(index)
        } else if frame.pc == 1 {
            inherited.filter(|&i| {
                matches!(self.frames[i].function, FrameKind::Native(XPCALL))
                    && self.frames[i].pc == 1
            })
        } else {
            None
        };
        if let Some(handler_index) = handler_index {
            self.frames[index].pc = 3;
            let handler = self.registers[self.frames[handler_index].vararg_start + 1];
            if matches!(handler, Value::Function(_) | Value::Native(_)) {
                // Preserve faulting frames until the error handler has returned.
                let base = self.registers.len();
                self.registers.extend_from_slice(&[handler, value]);
                self.push_record(Frame {
                    function: FrameKind::Native(CONT_HANDLER),
                    pc: 1,
                    base,
                    stack_start: base,
                    result: 0,
                    wanted: 1,
                    barriers: self.frames.last().map_or(1, |f| f.barriers + 1),
                    vararg_start: index,
                    vararg_count: 0,
                    tail_calls: 0,
                });
                match self.start_call(base, 1, 1, false) {
                    Ok(Some(actual)) => self.finish_call(base, actual, 1),
                    Ok(None) => {}
                    Err(_) => {
                        let value = self.intern(b"error in error handling");
                        self.protected_failure(index, value);
                    }
                }
                return Ok(true);
            }
        }
        let value = if frame.pc() == 3
            || matches!(frame.function, FrameKind::Native(XPCALL))
            || handler_index.is_some()
            || inherited.is_some_and(|i| self.frames[i].pc == 3)
        {
            self.intern(b"error in error handling")
        } else {
            value
        };
        self.protected_failure(index, value);
        Ok(true)
    }
    pub(super) fn step_native(&mut self, id: u64, frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let index = self.frames.len() - 1;
        match id {
            LOAD_READER => self.load_step(frame),
            COLLECT_GARBAGE => self.collect_step(frame),
            CONT_GC => self.finalizer_step(frame),
            CONT_RAW | CONT_BOOL | CONT_NOT | CONT_IGNORE => {
                if matches!(id, CONT_BOOL | CONT_NOT) {
                    self.registers[base] =
                        Value::Boolean(self.registers[base].is_truthy() == (id == CONT_BOOL));
                }
                Ok(self.return_frame(frame, base, usize::from(id != CONT_IGNORE)))
            }
            CONT_HANDLER => {
                let value = self.registers[base];
                self.protected_failure(frame.vararg_start, value);
                Ok(None)
            }
            PCALL | XPCALL => {
                if frame.pc() == 0 {
                    let minimum = if id == XPCALL { 2 } else { 1 };
                    if frame.argc() < minimum {
                        return Err(Error::new(ErrorKind::Runtime, 0, "value expected"));
                    }
                    let count = if id == XPCALL { 0 } else { frame.argc() - 1 };
                    self.registers
                        .copy_within(frame.vararg_start..frame.vararg_start + count + 1, base);
                    self.frames[index].pc = 1;
                    if let Some(actual) = self.start_call(base, count, OPEN, false)? {
                        self.finish_call(base, actual, OPEN);
                    }
                    Ok(None)
                } else if frame.pc() == 1 {
                    let count = self.top - base;
                    self.resize_registers(self.registers.len().max(base + count + 1));
                    self.registers.copy_within(base..base + count, base + 1);
                    self.registers[base] = Value::Boolean(true);
                    Ok(self.return_frame(frame, base, count + 1))
                } else {
                    Ok(self.return_frame(frame, base, 2))
                }
            }
            TOSTRING => {
                if frame.pc() == 0 {
                    if frame.argc() == 0 {
                        return Err(Error::new(ErrorKind::Runtime, 0, "value expected"));
                    }
                    let value = self.registers[frame.vararg_start];
                    let handler = self.metafield(value, Meta::ToString);
                    if handler != Value::Nil {
                        self.registers[base] = handler;
                        self.registers[base + 1] = value;
                        self.frames[index].pc = 1;
                        if let Some(actual) = self.start_call(base, 1, 1, false)? {
                            self.finish_call(base, actual, 1);
                        }
                        return Ok(None);
                    }
                    self.registers[base] = self.default_string(value);
                }
                Ok(self.return_frame(frame, base, 1))
            }
            PRINT => self.print_step(frame),
            strings::STR_GSUB => self.substitute_step(frame),
            tables::TAB_FOREACH | tables::TAB_FOREACHI => self.foreach_step(id, frame),
            tables::TAB_SORT => self.sort_step(frame),
            threads::CO_RESUME | threads::CO_YIELD => self.coroutine_step(id, frame),
            id if self.is_wrapper(id) => self.coroutine_step(id, frame),
            _ => unreachable!("native continuation ID"),
        }
    }
    fn default_string(&mut self, value: Value) -> Value {
        match value {
            Value::String(_) => value,
            Value::Number(n) => self.intern(format_number(n).as_bytes()),
            Value::Nil => self.intern(b"nil"),
            Value::Boolean(b) => self.intern(if b { b"true" } else { b"false" }),
            Value::Table(id) => self.intern(format!("table: 0x{:x}", id.0).as_bytes()),
            Value::Function(id) => {
                self.intern(format!("function: 0x{:x}", u128::from(id.0) << 1).as_bytes())
            }
            Value::Native(id) => {
                self.intern(format!("function: 0x{:x}", (u128::from(id) << 1) | 1).as_bytes())
            }
            Value::Thread(id) => self.intern(format!("thread: 0x{:x}", id.0).as_bytes()),
            Value::Userdata(id) => self.intern(format!("userdata: 0x{:x}", id.0).as_bytes()),
        }
    }
    fn print_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let index = self.frames.len() - 1;
        if frame.pc() == 0 {
            self.frames[index].pc = 1;
            let key = self.intern(b"tostring");
            self.get_table(Value::Table(self.globals), key, base + 1)?;
            return Ok(None);
        }
        let completed = frame.pc().saturating_sub(2);
        if frame.pc() >= 2 {
            let value = self.registers[base + 2];
            let value = match value {
                Value::String(_) => value,
                Value::Number(_) => self.default_string(value),
                _ => {
                    return Err(Error::new(
                        ErrorKind::Runtime,
                        0,
                        "'tostring' must return a string to 'print'",
                    ));
                }
            };
            let Value::String(id) = value else {
                unreachable!("print string conversion");
            };
            let bytes = &self.strings.bytes[slot_index(id.0)];
            let bytes = &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())];
            if let Some(output) = &mut self.output {
                if completed != 0 {
                    output.push(b'\t');
                }
                output.extend_from_slice(bytes);
            } else {
                let mut stdout = std::io::stdout().lock();
                if completed != 0 {
                    stdout.write_all(b"\t").map_err(io_error)?;
                }
                stdout.write_all(bytes).map_err(io_error)?;
            }
        }
        let next = if frame.pc() == 1 { 0 } else { completed + 1 };
        if next >= frame.argc() {
            if let Some(output) = &mut self.output {
                output.push(b'\n');
            } else {
                std::io::stdout().write_all(b"\n").map_err(io_error)?;
            }
            return Ok(self.return_frame(frame, base, 0));
        }
        self.registers[base + 2] = self.registers[base + 1];
        self.registers[base + 3] = self.registers[frame.vararg_start + next];
        self.frames[index].pc = (next + 2) as u32;
        if let Some(actual) = self.start_call(base + 2, 1, 1, false)? {
            self.finish_call(base + 2, actual, 1);
        }
        Ok(None)
    }
}

fn io_error(error: std::io::Error) -> Error {
    Error::new(ErrorKind::Runtime, 0, error.to_string())
}

impl Vm {
    fn stack_frame(&self, mut level: usize) -> Result<Option<usize>, Error> {
        if level == 0 {
            return Ok(None);
        }
        let requested = level;
        level -= 1;
        for (index, frame) in self.frames.iter().enumerate().rev() {
            if self.active_native == Some(index) {
                continue;
            }
            if matches!(frame.function,FrameKind::Native(id) if (CONT_RAW..strings::ITERATOR_FIRST).contains(&id))
            {
                continue;
            }
            if level == 0 {
                return Ok(Some(index));
            }
            level -= 1;
            if level < (frame.tail_calls as usize) {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    0,
                    format!("no function environment for tail call at level {requested}"),
                ));
            }
            level -= frame.tail_calls as usize;
        }
        Err(Error::new(ErrorKind::Runtime, 0, "invalid level"))
    }
    fn stack_kind(&self, level: usize) -> Result<FrameKind, Error> {
        self.stack_frame(level)
            .map(|index| index.map_or(FrameKind::Native(GET_ENV), |i| self.frames[i].function))
    }
    pub(super) fn located_error(&mut self, value: Value, level: isize) -> Value {
        if level <= 0 || !matches!(value, Value::String(_) | Value::Number(_)) {
            return value;
        }
        let Ok(Some(index)) = self.stack_frame(level as usize) else {
            return value;
        };
        let frame = self.frames[index];
        let FrameKind::Lua(function) = frame.function else {
            return value;
        };
        let closure = &self.functions[slot_index(function.0)];
        let program = &self.programs[closure.program].data;
        let prototype = &program.prototypes[closure.prototype];
        let line = prototype.lines[frame.pc().saturating_sub(1).min(prototype.lines.len() - 1)];
        let mut bytes = crate::bytecode::chunk_id(&program.name);
        bytes.extend_from_slice(format!(":{line}: ").as_bytes());
        bytes.extend_from_slice(
            &self
                .concat_bytes(value)
                .expect("error value is a string or number"),
        );
        self.intern(&bytes)
    }
}
impl Vm {
    pub(super) fn native(&mut self, id: u64, base: usize, count: usize) -> Result<usize, Error> {
        if (debug::GET_INFO..=debug::SET_UPVALUE).contains(&id) {
            return self.debug_native(id, base, count);
        }
        if id == NEW_PROXY {
            return self.proxy_native(base, count);
        }
        if matches!(id, COLLECT_GARBAGE | GC_INFO) {
            return self.gc_native(id, base, count);
        }
        if (threads::CO_CREATE..=threads::CO_YIELD).contains(&id) {
            return self.coroutine_native(id, base, count);
        }
        if (math::MATH_FIRST..=math::MATH_LAST).contains(&id) {
            return self.math_native(id, base, count);
        }
        if (strings::STR_FIRST..=strings::STR_DUMP).contains(&id) {
            return self.string_native(id, base, count);
        }
        if (tables::TAB_FIRST..=tables::TAB_SORT).contains(&id) {
            return self.table_native(id, base, count);
        }
        if id >= strings::ITERATOR_FIRST {
            return self.match_iterator(id, base);
        }
        let arg = |vm: &Self, index: usize| {
            if index < count {
                vm.registers[base + 1 + index]
            } else {
                Value::Nil
            }
        };
        let bad = |index: usize, expected: &str| {
            Error::new(
                ErrorKind::Runtime,
                0,
                format!("bad argument #{} ({expected} expected)", index + 1),
            )
        };
        let value = match id {
            0 => {
                if count == 0 {
                    return Err(bad(0, "value"));
                }
                if !arg(self, 0).is_truthy() {
                    let text = match arg(self, 1) {
                        Value::Nil => self.intern(b"assertion failed!"),
                        v @ Value::String(_) => v,
                        v @ Value::Number(_) => self.default_string(v),
                        _ => return Err(bad(1, "string")),
                    };
                    let Value::String(id) = text else {
                        unreachable!("assertion message is a string");
                    };
                    let bytes = self.string_bytes(id).unwrap_or_default();
                    let bytes = &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())];
                    let message = String::from_utf8_lossy(bytes).into_owned();
                    let object = self.intern(bytes.to_vec().as_slice());
                    let object = self.located_error(object, 1);
                    return Err(Error::raised(object, message));
                }
                self.registers.copy_within(base + 1..base + 1 + count, base);
                return Ok(count);
            }
            1 => {
                let message = match arg(self, 0) {
                    Value::String(id) => {
                        String::from_utf8_lossy(self.string_bytes(id).unwrap_or_default())
                            .into_owned()
                    }
                    _ => "Lua error".into(),
                };
                let level = if count < 2 || arg(self, 1) == Value::Nil {
                    1
                } else {
                    self.number(arg(self, 1)).ok_or_else(|| bad(1, "number"))? as isize
                };
                let object = self.located_error(arg(self, 0), level);
                return Err(Error::raised(object, message));
            }
            2 => {
                if count == 0 {
                    return Err(bad(0, "value"));
                }
                self.intern(arg(self, 0).type_name().as_bytes())
            }
            3 => self.tonumber_native(base, count)?,
            4 => {
                if count < 2 {
                    return Err(bad(count, "value"));
                }
                Value::Boolean(arg(self, 0) == arg(self, 1))
            }
            5 => {
                if count < 2 {
                    return Err(bad(count, "value"));
                }
                self.table(arg(self, 0))?.get(arg(self, 1))
            }
            6 => {
                if count < 3 {
                    return Err(bad(count, "value"));
                }
                let (t, k, v) = (arg(self, 0), arg(self, 1), arg(self, 2));
                self.table(t)?;
                let Value::Table(id) = t else {
                    unreachable!("validated table")
                };
                self.raw_set(id, k, v)?;
                t
            }
            7 => {
                if let Value::String(s) = arg(self, 0)
                    && self
                        .string_bytes(s)
                        .is_some_and(|s| s.first() == Some(&b'#'))
                {
                    self.registers[base] = Value::Number(count.saturating_sub(1) as f64);
                    return Ok(1);
                }
                let n = self.number(arg(self, 0)).ok_or_else(|| bad(0, "number"))? as isize;
                let available = count.saturating_sub(1);
                let start = if n < 0 { available as isize + n } else { n - 1 };
                if start < 0 {
                    return Err(Error::new(ErrorKind::Runtime, 0, "index out of range"));
                }
                let start = (start as usize).min(available);
                self.registers
                    .copy_within(base + 2 + start..base + 2 + available, base);
                return Ok(available - start);
            }
            8 => {
                let t = arg(self, 0);
                let table = self.table(t)?;
                let start = if count < 2 || arg(self, 1) == Value::Nil {
                    1.0
                } else {
                    self.number(arg(self, 1))
                        .ok_or_else(|| bad(1, "number"))?
                        .trunc()
                };
                let end = if count < 3 || arg(self, 2) == Value::Nil {
                    table.len() as f64
                } else {
                    self.number(arg(self, 2))
                        .ok_or_else(|| bad(2, "number"))?
                        .trunc()
                };
                let results = (end - start + 1.0).max(0.0);
                if !results.is_finite() || results > 1_000_000.0 {
                    return Err(Error::new(
                        ErrorKind::Limit,
                        0,
                        "too many results to unpack",
                    ));
                }
                let results = results as usize;
                self.resize_registers(self.registers.len().max(base + results));
                for i in 0..results {
                    let v = self.table(t)?.get(Value::Number(start + i as f64));
                    self.registers[base + i] = v;
                }
                return Ok(results);
            }
            NEXT | PAIRS_NEXT => {
                let next = self.table(arg(self, 0))?.next(arg(self, 1))?;
                if let Some((key, value)) = next {
                    self.resize_registers(self.registers.len().max(base + 2));
                    self.registers[base] = key;
                    self.registers[base + 1] = value;
                    return Ok(2);
                }
                Value::Nil
            }
            PAIRS | IPAIRS => {
                let table = arg(self, 0);
                self.table(table)?;
                self.resize_registers(self.registers.len().max(base + 3));
                self.registers[base] =
                    Value::Native(if id == PAIRS { PAIRS_NEXT } else { IPAIRS_STEP });
                self.registers[base + 1] = table;
                self.registers[base + 2] = if id == PAIRS {
                    Value::Nil
                } else {
                    Value::Number(0.0)
                };
                return Ok(3);
            }
            IPAIRS_STEP => {
                let n = self.number(arg(self, 1)).ok_or_else(|| bad(1, "number"))?;
                let next = (n as i32).wrapping_add(1);
                let value = self
                    .table(arg(self, 0))?
                    .get(Value::Number(f64::from(next)));
                if value == Value::Nil {
                    return Ok(0);
                }
                self.resize_registers(self.registers.len().max(base + 2));
                self.registers[base] = Value::Number(f64::from(next));
                self.registers[base + 1] = value;
                return Ok(2);
            }
            DEBUG_GET_ENV | DEBUG_SET_ENV => {
                if count == 0 {
                    return Err(bad(0, "value"));
                }
                let target = arg(self, 0);
                if id == DEBUG_GET_ENV {
                    match target {
                        Value::Function(id) => Value::Table(self.functions[slot_index(id.0)].env),
                        Value::Native(id) => Value::Table(self.native_environment(id)),
                        Value::Thread(id) => Value::Table(self.thread_environment(id)),
                        Value::Userdata(id) => Value::Table(self.userdata[slot_index(id.0)].env),
                        _ => Value::Nil,
                    }
                } else {
                    let Value::Table(env) = arg(self, 1) else {
                        return Err(bad(1, "table"));
                    };
                    match target {
                        Value::Function(id) => self.functions[slot_index(id.0)].env = env,
                        Value::Native(id) => self.set_native_environment(id, env),
                        Value::Thread(id) => self.set_thread_environment(id, env),
                        Value::Userdata(id) => self.userdata[slot_index(id.0)].env = env,
                        _ => {
                            return Err(Error::new(
                                ErrorKind::Runtime,
                                0,
                                "'setfenv' cannot change environment of given object",
                            ));
                        }
                    }
                    target
                }
            }
            GET_ENV | SET_ENV => {
                if id == SET_ENV && !matches!(arg(self, 1), Value::Table(_)) {
                    return Err(bad(1, "table"));
                }
                let target = arg(self, 0);
                let function = match target {
                    Value::Function(id) => FrameKind::Lua(id),
                    Value::Native(id) => FrameKind::Native(id),
                    Value::Nil if id == GET_ENV => self.stack_kind(1)?,
                    _ => {
                        let level = self.number(target).ok_or_else(|| bad(0, "number"))? as isize;
                        if level < 0 {
                            return Err(Error::new(
                                ErrorKind::Runtime,
                                0,
                                "level must be non-negative",
                            ));
                        }
                        if id == SET_ENV && level == 0 {
                            let Value::Table(env) = arg(self, 1) else {
                                unreachable!("environment type checked");
                            };
                            self.globals = env;
                            return Ok(0);
                        }
                        self.stack_kind(level as usize)?
                    }
                };
                match (id, function) {
                    (GET_ENV, FrameKind::Lua(id)) => {
                        Value::Table(self.functions[slot_index(id.0)].env)
                    }
                    (GET_ENV, FrameKind::Native(_)) => Value::Table(self.globals),
                    (SET_ENV, FrameKind::Lua(id)) => {
                        let Value::Table(env) = arg(self, 1) else {
                            unreachable!("environment type checked");
                        };
                        self.functions[slot_index(id.0)].env = env;
                        Value::Function(id)
                    }
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Runtime,
                            0,
                            "'setfenv' cannot change environment of given object",
                        ));
                    }
                }
            }
            LOAD_STRING => {
                let source = match arg(self, 0) {
                    v @ Value::String(_) | v @ Value::Number(_) => self.concat_bytes(v)?,
                    _ => return Err(bad(0, "string")),
                };
                let name = match arg(self, 1) {
                    Value::Nil => source.clone(),
                    v @ Value::String(_) | v @ Value::Number(_) => self.concat_bytes(v)?,
                    _ => return Err(bad(1, "string")),
                };
                match crate::compile_named(&source, &name) {
                    Ok(program) => Value::Function(self.load(&program)),
                    Err(error) => {
                        let mut message = crate::bytecode::chunk_id(&name);
                        message.extend_from_slice(
                            format!(":{}: {}", error.line(), error.message()).as_bytes(),
                        );
                        let value = self.intern(&message);
                        self.resize_registers(self.registers.len().max(base + 2));
                        self.registers[base] = Value::Nil;
                        self.registers[base + 1] = value;
                        return Ok(2);
                    }
                }
            }
            GET_META | DEBUG_GET_META => {
                if count == 0 {
                    return Err(bad(0, "value"));
                }
                let target = arg(self, 0);
                let protected = self.metafield(target, Meta::Protected);
                if id == GET_META && protected != Value::Nil {
                    protected
                } else {
                    self.metatable(target).map_or(Value::Nil, Value::Table)
                }
            }
            SET_META | DEBUG_SET_META => {
                if count < 2 {
                    return Err(bad(count, "value"));
                }
                let target = arg(self, 0);
                if id == SET_META {
                    self.table(target)?;
                    if self.metafield(target, Meta::Protected) != Value::Nil {
                        return Err(Error::new(
                            ErrorKind::Runtime,
                            0,
                            "cannot change a protected metatable",
                        ));
                    }
                }
                let mt = match arg(self, 1) {
                    Value::Nil => None,
                    Value::Table(id) => Some(id),
                    _ => return Err(bad(1, "nil or table")),
                };
                self.set_metatable(target, mt);
                if id == DEBUG_SET_META {
                    Value::Boolean(true)
                } else {
                    target
                }
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    0,
                    "unknown native function ID",
                ));
            }
        };
        self.registers[base] = value;
        Ok(1)
    }
}

// Lua 5.1's default formatting uses C's %.14g in the C locale.
pub(super) fn format_number(n: f64) -> String {
    if !n.is_finite() {
        return if n.is_nan() {
            format!(
                "{}{}",
                if n.is_sign_negative() { "-" } else { "" },
                crate::format::nan_body(n)
            )
        } else if n < 0.0 {
            "-inf".into()
        } else {
            "inf".into()
        };
    }
    let scientific = format!("{n:.13e}");
    let (digits, exp) = scientific
        .split_once('e')
        .expect("scientific formatting includes exponent");
    let exponent: i32 = exp.parse().expect("formatted exponent is an integer");
    if !(-4..14).contains(&exponent) {
        let digits = digits.trim_end_matches('0').trim_end_matches('.');
        format!(
            "{digits}e{}{abs:02}",
            if exponent < 0 { '-' } else { '+' },
            abs = exponent.abs()
        )
    } else {
        let decimals = (13 - exponent).max(0) as usize;
        let fixed = format!("{n:.decimals$}");
        if fixed.contains('.') {
            fixed.trim_end_matches('0').trim_end_matches('.').into()
        } else {
            fixed
        }
    }
}
