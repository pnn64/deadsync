//! Native library functions and explicit callback continuations.
use super::*;

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
        count: usize,
        wanted: u32,
        tail: bool,
    ) -> Result<Option<usize>, Error> {
        debug_assert!(matches!(
            self.frames.last().map(|f| f.function),
            Some(FrameKind::Native(_))
        ));
        if let Some(frame) = self.frames.last()
            && (usize::from(frame.barriers)
                + usize::from(self.threads[slot_index(self.thread.0)].depth)
                >= 198
                || self.threads[slot_index(self.thread.0)].native_bias != 0
                || !self.loads.is_empty())
        {
            self.check_c_call()?;
        }
        self.start_lua_call(call, count, wanted, tail)
    }
    pub(super) fn start_lua_call(
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
                return Err(self.type_error(target, "call", Some(call)));
            }
            self.resize_registers(self.registers.len().max(call + count + 2));
            self.registers.copy_within(call..call + count + 1, call + 1);
            self.registers[call] = handler;
            count += 1;
        }
        match self.registers[call] {
            Value::Function(id) => {
                if tail {
                    if self.hooks.mask & hooks::CALL != 0 && self.hooks.allowed() {
                        self.hooked_tail(id, call, count)?;
                    } else {
                        self.check_calls()?;
                        self.tail_frame(id, call, count)?;
                    }
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
                        | LOAD_FILE
                        | DO_FILE
                        | CONT_FILE
                        | debug::TRACEBACK
                        | debug::CONSOLE
                        | COLLECT_GARBAGE
                        | XPCALL
                        | TOSTRING
                        | PRINT
                        | os::TIME
                        | strings::STR_GSUB
                        | tables::TAB_FOREACH
                        | tables::TAB_FOREACHI
                        | tables::TAB_SORT
                        | threads::CO_RESUME
                        | threads::CO_YIELD
                ) || package::contains(id)
                    || self.scheduled_native(id) != 0
                    || (self.hooks.mask != 0 && self.hooks.allowed()) =>
            {
                self.check_calls()?;
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
                    c_level_bias: 0,
                });
                Ok(None)
            }
            Value::Native(id) => {
                self.check_calls()?;
                let index = self.frames.len();
                let base = self.registers.len();
                let host = self.host_callback(id);
                // Calls that can fail after emitting results keep their actual
                // arguments available to the error handler's stack inspector.
                let separate = matches!(id, strings::STR_FIND | strings::STR_MATCH)
                    || (io::FIRST..=io::LAST).contains(&id)
                    || id >= strings::ITERATOR_FIRST;
                if separate && host.is_none() {
                    self.resize_registers(base + count + 1);
                    self.registers[base] = Value::Native(id);
                    self.registers
                        .copy_within(call + 1..call + count + 1, base + 1);
                }
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
                    c_level_bias: 0,
                });
                let previous = self.active_native.replace(index);
                // Host callbacks read the original arguments and write separate
                // results, so they need no duplicated C argument stack.
                let result = match host {
                    Some(callback) => self.host_native(id, base, count, callback),
                    None => {
                        let source = if separate { base } else { call };
                        self.native(id, source, count)
                            .map(|actual| (source, actual))
                    }
                }
                .map_err(|error| {
                    if error.kind() == ErrorKind::Runtime && error.value().is_none() {
                        let value = self.intern(error.message_bytes());
                        let value = self.located_error(value, 1);
                        error.with_value(value)
                    } else {
                        error
                    }
                });
                self.active_native = previous;
                if result.is_ok() && self.frames[index].pc == threads::HOST_YIELDED {
                    return Ok(None);
                }
                if let Ok((source, actual)) = result
                    && self.hooks.mask & hooks::RETURN != 0
                    && self.hooks.allowed()
                {
                    self.return_frame(self.frames[index], source, actual);
                    return Ok(None);
                }
                let (source, actual) = result?;
                self.relocate_native_error_functions(base..self.registers.len(), call + 1);
                if separate {
                    self.registers.copy_within(source..source + actual, call);
                    self.registers.truncate(base.max(call + actual));
                }
                self.frames.pop();
                Ok(Some(actual))
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
        // A native API operation already crosses its host callback's C-call
        // boundary. Its result continuation must not count that boundary twice.
        let host_action = self.frames.last().is_some_and(|f| {
            (matches!(f.function, FrameKind::Native(CONT_HOST_CALL))
                && f.pc == 0
                && f.wanted != FRAME_OPEN)
                || (matches!(f.function, FrameKind::Native(CONT_CONCAT))
                    && self.frames.iter().rev().nth(1).is_some_and(|parent| {
                        matches!(parent.function, FrameKind::Native(CONT_HOST_CONCAT))
                    }))
        });
        let base = self.registers.len();
        self.registers.push(function);
        self.registers.extend_from_slice(args);
        self.push_record(Frame {
            function: FrameKind::Native(if host_action {
                CONT_HOST_META
            } else {
                continuation
            }),
            pc: if host_action { continuation as u32 } else { 1 },
            base,
            stack_start: base,
            result,
            wanted: if continuation == CONT_IGNORE { 0 } else { 1 },
            barriers: self
                .frames
                .last()
                .map_or(1, |f| f.barriers + u16::from(!host_action)),
            vararg_start: 0,
            vararg_count: 0,
            tail_calls: 0,
            c_level_bias: 0,
        });
        let wanted = if continuation == CONT_IGNORE { 0 } else { 1 };
        if let Some(actual) = self.start_call(base, args.len(), wanted, false)? {
            self.finish_call(base, actual, wanted);
        }
        Ok(())
    }
    pub(super) fn protected_failure(&mut self, index: usize, value: Value) {
        let frame = self.frames[index];
        self.set_thread_native_bias(self.thread, frame.c_level_bias);
        self.close_upvalues(frame.base);
        self.discard_finalizers(index + 1);
        self.frames.truncate(index + 1);
        self.restore_calls();
        self.trim_substitutions();
        self.trim_sorts();
        let base = if matches!(frame.function, FrameKind::Native(LOAD_READER)) {
            frame.base + 3
        } else {
            frame.base
        };
        if matches!(frame.function, FrameKind::Native(CONT_HOST_LOAD)) {
            self.resize_registers(frame.base);
        } else {
            self.registers.truncate(base);
        }
        let first = if loads::protected(frame.function) {
            Value::Nil
        } else {
            Value::Boolean(false)
        };
        self.registers.extend_from_slice(&[first, value]);
        self.frames[index].pc = 2;
        if matches!(frame.function, FrameKind::Native(LOAD_READER)) {
            self.frames[index].vararg_count = 5;
        }
        if matches!(frame.function, FrameKind::Native(CONT_HOST_PCALL)) {
            self.frames[index].vararg_count = 0;
        }
        self.trim_loads();
        self.trim_tracebacks();
        self.trim_hooks();
        self.top = base + 2;
    }
    pub(super) fn catch_error(&mut self, original: &Error) -> Result<bool, Error> {
        self.catch_error_from(original, 0)
    }

    pub(super) fn catch_error_from(
        &mut self,
        original: &Error,
        floor: usize,
    ) -> Result<bool, Error> {
        let mut pending = None;
        loop {
            let error = pending.as_ref().unwrap_or(original);
            let Some(relative) = self.frames[floor..].iter().rposition(|f| {
                (matches!(
                    f.function,
                    FrameKind::Native(
                        PCALL
                            | XPCALL
                            | CONT_STANDALONE
                            | CONT_HOST_CALL
                            | CONT_HOST_PCALL
                            | debug::CONSOLE
                    )
                ) && matches!(f.pc, 1 | 3 | 7))
                    || (loads::protected(f.function) && matches!(f.pc, 1 | 3))
            }) else {
                return Ok(false);
            };
            let index = floor + relative;
            let frame = self.frames[index];
            let value = if let Some(value) = error.value() {
                value
            } else {
                let value = self.intern(error.message_bytes());
                self.located_error(value, 1)
            };
            if error.bypass_handler {
                self.protected_failure(index, value);
                if matches!(
                    frame.function,
                    FrameKind::Native(CONT_HOST_PCALL | CONT_HOST_LOAD)
                ) {
                    self.frames[index].pc = 5;
                }
                return Ok(true);
            }
            // lua_load protects the parser while retaining the current error
            // function. An enclosing xpcall transforms reader runtime errors even
            // though the loader subsequently returns nil plus that transformed value.
            let inherited = if loads::protected(frame.function) {
                // An unprotected host call keeps its Rust boundary outside
                // this search's unwind floor, but lua_load still inherits the
                // enclosing error function. A protected host boundary stops
                // inheritance just like any other handler-free protection.
                self.frames[..index].iter().rposition(|f| {
                    (matches!(
                        f.function,
                        FrameKind::Native(
                            PCALL
                                | XPCALL
                                | CONT_STANDALONE
                                | CONT_HOST_CALL
                                | CONT_HOST_PCALL
                                | debug::CONSOLE
                        )
                    ) && matches!(f.pc, 1 | 3 | 7))
                        || (loads::protected(f.function) && f.pc == 3)
                })
            } else {
                None
            };
            let has_handler = |f: Frame| {
                matches!(f.function, FrameKind::Native(XPCALL))
                    || (matches!(f.function, FrameKind::Native(CONT_HOST_PCALL)) && f.argc() != 0)
                    || (matches!(f.function, FrameKind::Native(CONT_STANDALONE)) && f.pc != 7)
            };
            let handler_index = if has_handler(frame) {
                Some(index)
            } else {
                inherited.filter(|&i| has_handler(self.frames[i]))
            };
            if let Some(handler_index) = handler_index {
                self.frames[index].pc = 3;
                let handler_frame = self.frames[handler_index];
                let handler = self
                    .registers
                    .get(match handler_frame.function {
                        FrameKind::Native(CONT_STANDALONE) => handler_frame.base,
                        FrameKind::Native(CONT_HOST_PCALL) => handler_frame.vararg_start,
                        _ => handler_frame.vararg_start + 1,
                    })
                    .copied()
                    .unwrap_or(Value::Nil);
                if matches!(handler, Value::Function(_) | Value::Native(_)) {
                    self.charge()?;
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
                        c_level_bias: 0,
                    });
                    match self.start_call(base, 1, 1, false) {
                        Ok(Some(actual)) => self.finish_call(base, actual, 1),
                        Ok(None) => {}
                        Err(next) if next.kind() == ErrorKind::Runtime => {
                            pending = Some(next);
                            continue;
                        }
                        Err(next) => return Err(next),
                    }
                    return Ok(true);
                }
            }
            let handler_failed = frame.pc() == 3
                || has_handler(frame)
                || handler_index.is_some()
                || inherited.is_some_and(|i| self.frames[i].pc == 3);
            let value = if handler_failed {
                self.intern(b"error in error handling")
            } else {
                value
            };
            self.protected_failure(index, value);
            if handler_failed
                && matches!(
                    frame.function,
                    FrameKind::Native(CONT_HOST_PCALL | CONT_HOST_LOAD)
                )
            {
                self.frames[index].pc = 5;
            }
            return Ok(true);
        }
    }
    pub(super) fn step_native(&mut self, id: u64, frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let index = self.frames.len() - 1;
        let scheduled = self.scheduled_native(id);
        match id {
            _ if frame.pc == threads::HOST_YIELDED && self.host_callback(id).is_some() => {
                if matches!(self.transition, Some(threads::Transition::Yield { .. })) {
                    return Ok(None);
                }
                let values = self.threads[slot_index(self.thread.0)]
                    .resumed_values
                    .take()
                    .expect("native yield resume arguments");
                self.frames[index].vararg_count = (self.top - frame.vararg_start) as u32;
                Ok(self.return_frame(self.frames[index], values.start, values.len()))
            }
            CONT_HOST_LOAD if matches!(frame.pc, 2 | 5) => {
                self.frames.pop();
                self.registers[base] = Value::Number(if frame.pc == 5 { 5. } else { 2. });
                self.top = base + 2;
                Ok(Some(2))
            }
            CONT_HOST_CALL | CONT_HOST_CONCAT => self.host_return(frame),
            CONT_HOST_PCALL => Ok(self.host_protected_return(frame)),
            CONT_HOST_META => {
                if matches!(u64::from(frame.pc), CONT_BOOL | CONT_NOT) {
                    self.registers[base] = Value::Boolean(
                        self.registers[base].is_truthy() == (u64::from(frame.pc) == CONT_BOOL),
                    );
                }
                Ok(self.return_frame(frame, base, usize::from(frame.wanted != 0)))
            }
            CONT_STANDALONE => self.standalone_driver(frame),
            debug::CONSOLE => self.debug_console_step(frame),
            _ if scheduled >= 2 => self.standalone_native(id, frame, scheduled),
            CONT_HOOK | CONT_TRACE | CONT_RETURN => self.hook_step(id, frame),
            debug::TRACEBACK => self.traceback_step(frame).map_err(|error| {
                // lua_concat raises in the C traceback frame; it does not add
                // the caller location used by Lua-level arithmetic failures.
                if error.kind() == ErrorKind::Runtime && error.value().is_none() {
                    let value = self.intern(error.message_bytes());
                    error.with_value(value)
                } else {
                    error
                }
            }),
            LOAD_READER => self.load_step(frame),
            strings::STR_DUMP => self.dump_step(frame),
            os::TIME => self.time_step(frame),
            LOAD_FILE | DO_FILE => self.file_load_step(id, frame),
            CONT_FILE => self.file_load_step(LOAD_FILE, frame),
            id if package::contains(id) => self.package_step(id, frame),
            COLLECT_GARBAGE => self.collect_step(frame),
            CONT_GC => self.finalizer_step(frame),
            CONT_CONCAT => {
                self.frames.pop();
                self.registers.truncate(frame.base);
                self.top = frame.pc();
                self.concat_range(
                    frame.result,
                    frame.vararg_start,
                    frame.vararg_start + frame.argc() - 1,
                )?;
                self.maybe_gc();
                Ok(None)
            }
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
                        return Err(self.argument_error(minimum - 1, b"value expected"));
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
                        return Err(self.argument_error(0, b"value expected"));
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
                    if matches!(value, Value::Number(_)) {
                        self.registers[frame.vararg_start] = self.registers[base];
                    }
                }
                Ok(self.return_frame(frame, base, 1))
            }
            PRINT => self.print_step(frame),
            strings::STR_GSUB => self.substitute_step(frame),
            tables::TAB_FOREACH | tables::TAB_FOREACHI => self.foreach_step(id, frame),
            tables::TAB_SORT => self.sort_step(frame),
            threads::CO_RESUME | threads::CO_YIELD => self.coroutine_step(id, frame),
            _ if scheduled == 1 => self.coroutine_step(id, frame),
            _ => {
                if let Some(callback) = self.host_callback(id) {
                    let (source, actual) = self.host_native(id, base, frame.argc(), callback)?;
                    if self.frames[index].pc == threads::HOST_YIELDED {
                        return Ok(None);
                    }
                    return Ok(self.return_frame(self.frames[index], source, actual));
                }
                // Return hooks run before results replace the caller's slots.
                self.registers[base] = Value::Native(id);
                self.registers.copy_within(
                    frame.vararg_start..frame.vararg_start + frame.argc(),
                    base + 1,
                );
                let actual = self.native(id, base, frame.argc())?;
                Ok(self.return_frame(frame, base, actual))
            }
        }
    }
    pub(super) fn default_string(&mut self, value: Value) -> Value {
        match value {
            Value::String(_) => value,
            Value::Number(n) => self.intern(format_number(n).as_slice()),
            Value::Nil => self.intern(b"nil"),
            Value::Boolean(b) => self.intern(if b { b"true" } else { b"false" }),
            Value::ParserPrototype(_) => {
                self.intern(format!("proto: {}", identity::pointer_text(0)).as_bytes())
            }
            _ => {
                let pointer = self
                    .object_pointer(value)
                    .expect("live object has an identity");
                self.intern(
                    format!("{}: {}", value.type_name(), identity::pointer_text(pointer))
                        .as_bytes(),
                )
            }
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
                if completed != 0 {
                    io::stdout_write(b"\t").map_err(io_error)?;
                }
                io::stdout_write(bytes).map_err(io_error)?;
            }
        }
        let next = if frame.pc() == 1 { 0 } else { completed + 1 };
        if next >= frame.argc() {
            if let Some(output) = &mut self.output {
                output.push(b'\n');
            } else {
                io::stdout_write(b"\n").map_err(io_error)?;
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
        Err(self.argument_error(0, b"invalid level"))
    }
    pub(super) fn stack_kind(&self, level: usize) -> Result<FrameKind, Error> {
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
        let line =
            prototype.debug_line(frame.pc().saturating_sub(1).min(prototype.lines.len() - 1));
        let mut bytes = crate::bytecode::chunk_id(program.source(prototype));
        bytes.extend_from_slice(format!(":{line}: ").as_bytes());
        bytes.extend_from_slice(
            &self
                .concat_bytes(value)
                .expect("error value is a string or number"),
        );
        self.intern(&bytes)
    }
    pub(super) fn native_where(&self, level: isize) -> Vec<u8> {
        if level <= 0 {
            return Vec::new();
        }
        let Ok(Some(index)) = self.stack_frame(level as usize) else {
            return Vec::new();
        };
        let frame = self.frames[index];
        let FrameKind::Lua(function) = frame.function else {
            return Vec::new();
        };
        let closure = &self.functions[slot_index(function.0)];
        let program = &self.programs[closure.program].data;
        let prototype = &program.prototypes[closure.prototype];
        let line =
            prototype.debug_line(frame.pc().saturating_sub(1).min(prototype.lines.len() - 1));
        if line == 0 {
            return Vec::new();
        }
        let mut prefix = crate::bytecode::chunk_id(program.source(prototype));
        prefix.extend_from_slice(format!(":{line}: ").as_bytes());
        prefix
    }
}
impl Vm {
    pub(super) fn native(&mut self, id: u64, base: usize, count: usize) -> Result<usize, Error> {
        let closure = id;
        let id = if id >= strings::ITERATOR_FIRST {
            match self.native_closures.get(id).map(|c| &c.kind) {
                Some(threads::NativeKind::Builtin(code)) => *code,
                _ => id,
            }
        } else {
            id
        };
        if id == debug::GET_REGISTRY {
            self.registers[base] = Value::Table(self.registry);
            return Ok(1);
        }
        if (io::FIRST..=io::LAST).contains(&id) {
            return self.io_native(id, base, count);
        }
        if (os::FIRST..=os::LAST).contains(&id) {
            return self.os_native(id, base, count);
        }
        if matches!(id, hooks::GET_HOOK | hooks::SET_HOOK) {
            return self.hook_native(id, base, count);
        }
        if (debug::GET_INFO..=debug::SET_LOCAL).contains(&id) {
            return self.debug_native(id, base, count);
        }
        if id == NEW_PROXY {
            return self.proxy_native(closure, base, count);
        }
        if matches!(id, COLLECT_GARBAGE | GC_INFO) {
            return self.gc_native(id, base, count).map(|_| 1);
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
            if matches!(
                self.native_closures.get(id).map(|c| &c.kind),
                Some(threads::NativeKind::Lines)
            ) {
                return self.io_iterator(id, base);
            }
            return self.match_iterator(id, base);
        }
        let arg = |vm: &Self, index: usize| {
            if index < count {
                vm.registers[base + 1 + index]
            } else {
                Value::Nil
            }
        };
        let value = match id {
            0 => {
                if count == 0 {
                    return Err(self.argument_error(0, b"value expected"));
                }
                if !arg(self, 0).is_truthy() {
                    let text = match arg(self, 1) {
                        Value::Nil => self.intern(b"assertion failed!"),
                        v @ Value::String(_) => v,
                        v @ Value::Number(_) => self.default_string(v),
                        _ => return Err(self.argument_type_error(base, count, 1, "string")),
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
                    self.number(arg(self, 1))
                        .ok_or_else(|| self.argument_type_error(base, count, 1, "number"))?
                        as isize
                };
                let object = self.located_error(arg(self, 0), level);
                return Err(Error::raised(object, message));
            }
            2 => {
                if count == 0 {
                    return Err(self.argument_error(0, b"value expected"));
                }
                self.intern(arg(self, 0).type_name().as_bytes())
            }
            3 => self.tonumber_native(base, count)?,
            4 => {
                if count < 2 {
                    return Err(self.argument_error(count, b"value expected"));
                }
                Value::Boolean(arg(self, 0) == arg(self, 1))
            }
            5 => {
                let table = self.table_argument(base, count)?;
                if count < 2 {
                    return Err(self.argument_error(count, b"value expected"));
                }
                self.tables[slot_index(table.0)].get(arg(self, 1))
            }
            6 => {
                let table = self.table_argument(base, count)?;
                if count < 3 {
                    return Err(self.argument_error(count, b"value expected"));
                }
                let (t, k, v) = (arg(self, 0), arg(self, 1), arg(self, 2));
                self.raw_set(table, k, v)?;
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
                let n = self.integer_argument(base, count, 0, None)? as i32;
                let available = count.saturating_sub(1);
                let start = if n < 0 {
                    available as i64 + i64::from(n)
                } else {
                    i64::from(n) - 1
                };
                if start < 0 {
                    return Err(self.argument_error(0, b"index out of range"));
                }
                let start = (start as usize).min(available);
                self.registers
                    .copy_within(base + 2 + start..base + 2 + available, base);
                return Ok(available - start);
            }
            8 => {
                let id = self.table_argument(base, count)?;
                let start = self.integer_argument(base, count, 1, Some(1))? as i32;
                let end = if count < 3 || arg(self, 2) == Value::Nil {
                    self.tables[slot_index(id.0)].len() as i32
                } else {
                    self.integer_argument(base, count, 2, None)? as i32
                };
                if start > end {
                    return Ok(0);
                }
                let results = i64::from(end) - i64::from(start) + 1;
                if results > limits::MAX_CSTACK.saturating_sub(count) as i64 {
                    return Err(Error::new(
                        ErrorKind::Runtime,
                        0,
                        "too many results to unpack",
                    ));
                }
                let results = results as usize;
                self.resize_registers(self.registers.len().max(base + results));
                let table = &self.tables[slot_index(id.0)];
                for i in 0..results {
                    let v = table.get(Value::Number((i64::from(start) + i as i64) as f64));
                    self.registers[base + i] = v;
                }
                return Ok(results);
            }
            NEXT | PAIRS_NEXT => {
                let table = self.table_argument(base, count)?;
                let next = self.tables[slot_index(table.0)]
                    .next(arg(self, 1))
                    .map_err(|error| self.operand_error(error))?;
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
                self.table_argument(base, count)?;
                self.resize_registers(self.registers.len().max(base + 3));
                self.registers[base] = self.native_closures[slot_index(closure)].captures[0];
                self.registers[base + 1] = table;
                self.registers[base + 2] = if id == PAIRS {
                    Value::Nil
                } else {
                    Value::Number(0.0)
                };
                return Ok(3);
            }
            IPAIRS_STEP => {
                let n = self
                    .number(arg(self, 1))
                    .ok_or_else(|| self.argument_type_error(base, count, 1, "number"))?;
                let next = (n as i32).wrapping_add(1);
                let table = self.table_argument(base, count)?;
                let value = self.tables[slot_index(table.0)].get(Value::Number(f64::from(next)));
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
                    return Err(self.argument_error(0, b"value expected"));
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
                        return Err(self.argument_type_error(base, count, 1, "table"));
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
                    self.gc_barrier(target, Value::Table(env));
                    target
                }
            }
            GET_ENV | SET_ENV => {
                if id == SET_ENV && !matches!(arg(self, 1), Value::Table(_)) {
                    return Err(self.argument_type_error(base, count, 1, "table"));
                }
                let target = arg(self, 0);
                let function = match target {
                    Value::Function(id) => FrameKind::Lua(id),
                    Value::Native(id) => FrameKind::Native(id),
                    Value::Nil if id == GET_ENV => self.stack_kind(1)?,
                    _ => {
                        let level = self
                            .number(target)
                            .ok_or_else(|| self.argument_type_error(base, count, 0, "number"))?
                            as isize;
                        if level < 0 {
                            return Err(self.argument_error(0, b"level must be non-negative"));
                        }
                        if id == SET_ENV && level == 0 {
                            let Value::Table(env) = arg(self, 1) else {
                                unreachable!("environment type checked");
                            };
                            self.set_thread_environment(self.thread, env);
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
                        self.gc_barrier(Value::Function(id), Value::Table(env));
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
                let source = self.string_argument(base, count, 0)?;
                let source = self.strings.bytes[slot_index(source.0)].to_vec();
                let name = match arg(self, 1) {
                    Value::Nil => source.clone(),
                    _ => {
                        let id = self.string_argument(base, count, 1)?;
                        self.strings.bytes[slot_index(id.0)].to_vec()
                    }
                };
                let depth = usize::from(self.native_calls_for(self.thread, self.frames.len()));
                let compiled = loads::compile(&source, &name, depth);
                match compiled {
                    Ok(program) => Value::Function(self.load(&program)),
                    Err(error) => {
                        let message = loads::message(&name, &error, source.first() == Some(&27));
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
                    return Err(self.argument_error(0, b"value expected"));
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
                if id == SET_META {
                    self.table_argument(base, count)?;
                }
                let target = arg(self, 0);
                if count < 2 {
                    return Err(self.argument_error(1, b"nil or table expected"));
                }
                let mt = match arg(self, 1) {
                    Value::Nil => None,
                    Value::Table(id) => Some(id),
                    _ => return Err(self.argument_error(1, b"nil or table expected")),
                };
                if id == SET_META && self.metafield(target, Meta::Protected) != Value::Nil {
                    return Err(Error::new(
                        ErrorKind::Runtime,
                        0,
                        "cannot change a protected metatable",
                    ));
                }
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

// Locale punctuation is applied after the pure Rust %.14g conversion.
pub(super) fn format_number(n: f64) -> Vec<u8> {
    let mut bytes = format_number_c(n).into_bytes();
    crate::locale::localize(&mut bytes);
    bytes
}
fn format_number_c(n: f64) -> String {
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
