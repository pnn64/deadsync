//! Native library functions and explicit callback continuations.
use super::*;
use crate::memory::fallible::{self, VecExt};

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
                self.reserve_registers(base + count.max(2) + 4)?;
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
                if matches!(
                    id,
                    0 | 3..=5 | NEXT | PAIRS_NEXT | 32..=41 | 46..=51 | 53 | 54
                        | strings::STR_LEN
                        | strings::STR_BYTE
                ) && let Some(actual) = self.try_fast_native(id, call, count)
                {
                    return Ok(Some(actual));
                }
                let index = self.frames.len();
                let base = self.registers.len();
                let host = self.host_callback(id);
                // Calls that can fail after emitting results keep their actual
                // arguments available to the error handler's stack inspector.
                let separate = matches!(id, strings::STR_FIND | strings::STR_MATCH)
                    || (io::FIRST..=io::LAST).contains(&id)
                    || id >= strings::ITERATOR_FIRST;
                if separate && host.is_none() {
                    self.reserve_registers(base + count + 1)?;
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
                .map_err(|error| self.attach_error_value(error, 1));
                self.active_native = previous;
                if result.is_ok() && self.frames[index].pc == threads::HOST_YIELDED {
                    return Ok(None);
                }
                if let Ok((source, actual)) = result
                    && self.hooks.mask & hooks::RETURN != 0
                    && self.hooks.allowed()
                {
                    self.return_frame(self.frames[index], source, actual)?;
                    return Ok(None);
                }
                let (source, actual) = result?;
                self.relocate_native_error_functions(base..self.registers.len(), call + 1);
                if separate {
                    self.registers.copy_within(source..source + actual, call);
                    self.truncate_registers(base.max(call + actual));
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
        self.reserve_registers(base + 1 + args.len())?;
        self.reserve_frames(1)?;
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
        let kind = self.body_kind(index, frame.function);
        self.set_thread_native_bias(self.thread, frame.c_level_bias);
        self.close_upvalues(frame.base);
        self.discard_finalizers(index + 1);
        self.frames.truncate(index + 1);
        self.restore_calls();
        self.trim_substitutions();
        self.trim_sorts();
        let base = if matches!(kind, FrameKind::Native(LOAD_READER)) {
            frame.base + 3
        } else {
            frame.base
        };
        if matches!(frame.function, FrameKind::Native(CONT_HOST_LOAD)) {
            self.resize_registers(frame.base);
        } else {
            self.truncate_registers(base);
        }
        let first = if loads::protected(kind) {
            Value::Nil
        } else {
            Value::Boolean(false)
        };
        self.registers.extend_from_slice(&[first, value]);
        self.frames[index].pc = 2;
        if matches!(kind, FrameKind::Native(LOAD_READER)) {
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

    fn protection_active(&self, index: usize) -> bool {
        self.thread_protection_active(self.thread, index)
    }
    fn thread_protection_active(&self, thread: ThreadId, index: usize) -> bool {
        let frames = if thread == self.thread {
            &self.frames
        } else {
            &self.threads[slot_index(thread.0)].frames
        };
        // A C function's lua_pcall/lua_load has already returned before its
        // return hook runs. Keep the visible frame, but do not catch hook errors
        // with a protection scope that has finished executing.
        if frames
            .get(index + 1)
            .is_some_and(|next| matches!(next.function, FrameKind::Native(CONT_RETURN)))
        {
            return false;
        }
        let mut frame = frames[index];
        frame.function = self
            .body_code(thread, index)
            .map_or(frame.function, FrameKind::Native);
        (matches!(
            frame.function,
            FrameKind::Native(
                PCALL
                    | XPCALL
                    | CONT_STANDALONE
                    | CONT_HOST_CALL
                    | CONT_HOST_PCALL
                    | debug::CONSOLE
            )
        ) && matches!(frame.pc, 1 | 3 | 7))
            || (loads::protected(frame.function) && matches!(frame.pc, 1 | 3))
    }
    pub(super) fn catch_error_from(
        &mut self,
        original: &Error,
        floor: usize,
    ) -> Result<bool, Error> {
        let mut pending = None;
        loop {
            let error = pending.as_ref().unwrap_or(original);
            if error
                .target_thread
                .is_some_and(|target| target != self.thread)
            {
                return Err(error.clone());
            }
            let Some(index) = (floor..self.frames.len()).rfind(|&i| self.protection_active(i))
            else {
                return Ok(false);
            };
            let mut frame = self.frames[index];
            frame.function = self.body_kind(index, frame.function);
            let value = match self.error_value(error, 1) {
                Ok(value) => value,
                Err(error) => {
                    pending = Some(error);
                    continue;
                }
            };
            if error.kind() == ErrorKind::Memory || error.bypass_handler {
                if error.kind() == ErrorKind::Memory {
                    // Return the prepared protected result before retrying automatic
                    // collection: the next scheduler step must not allocate merely
                    // to deliver an allocation failure while rejection persists.
                    self.defer_gc_until_allocation();
                }
                self.protected_failure(index, value);
                if matches!(
                    frame.function,
                    FrameKind::Native(CONT_HOST_PCALL | CONT_HOST_LOAD)
                ) {
                    self.frames[index].pc = if error.kind() == ErrorKind::Memory {
                        4
                    } else {
                        5
                    };
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
                (0..index).rfind(|&i| {
                    self.protection_active(i)
                        && (!loads::protected(self.body_kind(i, self.frames[i].function))
                            || self.frames[i].pc == 3)
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
                inherited.filter(|&i| {
                    has_handler(Frame {
                        function: self.body_kind(i, self.frames[i].function),
                        ..self.frames[i]
                    })
                })
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
                        Err(next) if next.catchable() => {
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
                if matches!(
                    self.transition,
                    Some(threads::Transition::Yield { .. } | threads::Transition::HookYield { .. })
                ) {
                    return Ok(None);
                }
                let values = self.take_resume_values(frame)?;
                self.frames[index].vararg_count = (self.top - frame.vararg_start) as u32;
                self.return_frame(self.frames[index], values.start, values.len())
            }
            CONT_HOST_LOAD if matches!(frame.pc, 2 | 4 | 5) => {
                self.frames.pop();
                self.registers[base] = Value::Number(f64::from(frame.pc));
                self.top = base + 2;
                Ok(Some(2))
            }
            CONT_HOST_CALL | CONT_HOST_LOAD | CONT_NATIVE_WINDOW
                if frame.pc == host::API_YIELDED =>
            {
                Ok(self.host_protected_return(frame))
            }
            CONT_HOST_CALL | CONT_HOST_CONCAT => self.host_return(frame),
            CONT_HOST_PCALL => Ok(self.host_protected_return(frame)),
            CONT_HOST_META => {
                if matches!(u64::from(frame.pc), CONT_BOOL | CONT_NOT) {
                    self.registers[base] = Value::Boolean(
                        self.registers[base].is_truthy() == (u64::from(frame.pc) == CONT_BOOL),
                    );
                }
                self.return_frame(frame, base, usize::from(frame.wanted != 0))
            }
            CONT_STANDALONE => self.standalone_driver(frame),
            debug::CONSOLE => self.debug_console_step(frame),
            _ if scheduled >= 2 => self.standalone_native(id, frame, scheduled),
            CONT_HOOK | CONT_TRACE | CONT_RETURN => self.hook_step(id, frame),
            debug::TRACEBACK => self.traceback_step(frame).map_err(|error| {
                // Traceback concatenation raises in its own native frame.
                self.attach_error_value(error, 0)
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
                self.truncate_registers(frame.base);
                self.top = frame.pc();
                self.concat_range(
                    frame.result,
                    frame.vararg_start,
                    frame.vararg_start + frame.argc() - 1,
                )?;
                self.maybe_gc()?;
                Ok(None)
            }
            CONT_RAW | CONT_BOOL | CONT_NOT | CONT_IGNORE => {
                if matches!(id, CONT_BOOL | CONT_NOT) {
                    self.registers[base] =
                        Value::Boolean(self.registers[base].is_truthy() == (id == CONT_BOOL));
                }
                self.return_frame(frame, base, usize::from(id != CONT_IGNORE))
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
                    self.return_frame(frame, base, count + 1)
                } else {
                    self.return_frame(frame, base, 2)
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
                    self.registers[base] = self.default_string(value)?;
                    if matches!(value, Value::Number(_)) {
                        self.registers[frame.vararg_start] = self.registers[base];
                    }
                }
                self.return_frame(frame, base, 1)
            }
            PRINT => self.print_step(frame),
            strings::STR_GSUB => self.substitute_step(frame),
            tables::TAB_FOREACH | tables::TAB_FOREACHI => self.foreach_step(id, frame),
            tables::TAB_SORT => {
                let mut line = None;
                let result = self.sort_step(frame, &mut line);
                self.sort_line = line;
                result
            }
            threads::CO_RESUME | threads::CO_YIELD => self.coroutine_step(id, frame),
            _ if scheduled == 1 => self.coroutine_step(id, frame),
            _ => {
                if let Some(callback) = self.host_callback(id) {
                    let (source, actual) = self.host_native(id, base, frame.argc(), callback)?;
                    if self.frames[index].pc == threads::HOST_YIELDED {
                        return Ok(None);
                    }
                    return self.return_frame(self.frames[index], source, actual);
                }
                // Return hooks run before results replace the caller's slots.
                self.registers[base] = self.builtin_context(id).unwrap_or(Value::Native(id));
                self.registers.copy_within(
                    frame.vararg_start..frame.vararg_start + frame.argc(),
                    base + 1,
                );
                let actual = self.native(id, base, frame.argc())?;
                self.return_frame(self.frames[index], base, actual)
            }
        }
    }
    fn default_string(&mut self, value: Value) -> Result<Value, Error> {
        match value {
            Value::String(_) => Ok(value),
            Value::Number(n) => self.intern_number(n),
            Value::Nil => self.try_intern(b"nil"),
            Value::Boolean(b) => self.try_intern(if b { b"true" } else { b"false" }),
            Value::ParserPrototype(_) => self.try_intern(
                fallible::format(format_args!("proto: {}", identity::pointer_display(0)))?
                    .as_bytes(),
            ),
            _ => {
                let pointer = self
                    .object_pointer(value)
                    .expect("live object has an identity");
                self.try_intern(
                    fallible::format(format_args!(
                        "{}: {}",
                        value.type_name(),
                        identity::pointer_display(pointer)
                    ))?
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
            let key = self.try_intern(b"tostring")?;
            self.get_table(Value::Table(self.globals), key, base + 1)?;
            return Ok(None);
        }
        let completed = frame.pc().saturating_sub(2);
        if frame.pc() >= 2 {
            let value = self.registers[base + 2];
            let value = match value {
                Value::String(_) => value,
                Value::Number(number) => self.intern_number(number)?,
                _ => {
                    return Err(Error::runtime_static(
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
                    output.try_push(b'\t')?;
                }
                output.try_extend_from_slice(bytes)?;
            } else {
                if completed != 0 {
                    self.io.write_print(b"\t")?;
                }
                self.io.write_print(bytes)?;
            }
        }
        let next = if frame.pc() == 1 { 0 } else { completed + 1 };
        if next >= frame.argc() {
            if let Some(output) = &mut self.output {
                output.try_push(b'\n')?;
            } else {
                self.io.write_print(b"\n")?;
            }
            return self.return_frame(frame, base, 0);
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

impl Vm {
    fn stack_frame(&self, mut level: usize) -> Result<Option<usize>, Error> {
        if level == 0 {
            return Ok(None);
        }
        let requested = level;
        level -= 1;
        for (index, frame) in self.frames.iter().enumerate().rev() {
            if self.active_native == Some(index) || self.builtin_visible_owner() == Some(index) {
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
        if level == 0
            && let Some(Value::Function(id)) = self.builtin_context(GET_ENV)
        {
            return Ok(FrameKind::Lua(id));
        }
        self.stack_frame(level)
            .map(|index| index.map_or(FrameKind::Native(GET_ENV), |i| self.frames[i].function))
    }
    pub(super) fn try_located_error(&mut self, value: Value, level: isize) -> Result<Value, Error> {
        use crate::memory::fallible::{self, VecExt};
        if level <= 0 || !matches!(value, Value::String(_) | Value::Number(_)) {
            return Ok(value);
        }
        let Ok(Some(index)) = self.stack_frame(level as usize) else {
            return Ok(value);
        };
        let frame = self.frames[index];
        let FrameKind::Lua(function) = frame.function else {
            return Ok(value);
        };
        let closure = &self.functions[slot_index(function.0)];
        let program = &self.programs[closure.program].data;
        let prototype = &program.prototypes[closure.prototype];
        let line =
            prototype.debug_line(frame.pc().saturating_sub(1).min(prototype.lines.len() - 1));
        let mut chunk = [0; 60];
        let used = crate::bytecode::chunk_id_into(program.source(prototype), &mut chunk);
        let build = || -> Result<Vec<u8>, Error> {
            let mut bytes = Vec::new();
            bytes.try_extend_from_slice(&chunk[..used])?;
            bytes.try_extend_from_slice(fallible::format(format_args!(":{line}: "))?.as_bytes())?;
            match value {
                Value::String(id) => bytes
                    .try_extend_from_slice(self.string_bytes(id).expect("live error string"))?,
                Value::Number(number) => {
                    bytes.try_extend_from_slice(&try_format_number(number)?)?
                }
                _ => unreachable!(),
            }
            Ok(bytes)
        };
        let bytes = build().map_err(|_| self.memory_failure())?;
        self.try_intern(&bytes)
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
        let closure = self.builtin_closure(id);
        let id = if id >= strings::ITERATOR_FIRST {
            match self.native_closures.get(id).map(|c| &c.kind) {
                Some(threads::NativeKind::Builtin(code)) => *code,
                _ => id,
            }
        } else {
            id
        };
        if id == host::stack::body::MATCH {
            return self.match_iterator(closure, base);
        }
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
        if (strings::STR_FIRST..=strings::STR_GFIND_REMOVED).contains(&id) {
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
        let value = match id {
            0 => {
                if count == 0 {
                    return Err(self.argument_error(0, b"value expected"));
                }
                if !arg(self, 0).is_truthy() {
                    use crate::memory::fallible::SliceExt;
                    let bytes = match arg(self, 1) {
                        Value::Nil => b"assertion failed!".try_to_vec(),
                        Value::String(id) => {
                            let bytes = self.string_bytes(id).unwrap_or_default();
                            bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
                                .try_to_vec()
                        }
                        Value::Number(number) => try_format_number(number),
                        _ => return Err(self.argument_type_error(base, count, 1, "string")),
                    };
                    let error = self.runtime_message(bytes);
                    if error.kind() == ErrorKind::Memory {
                        return Err(error);
                    }
                    let object = self.try_intern(error.message_bytes())?;
                    let object = self.try_located_error(object, 1)?;
                    return Err(error.with_value(object));
                }
                self.registers.copy_within(base + 1..base + 1 + count, base);
                return Ok(count);
            }
            1 => {
                use crate::memory::fallible::SliceExt;
                let error = match arg(self, 0) {
                    Value::String(id) => {
                        self.runtime_message(self.string_bytes(id).unwrap_or_default().try_to_vec())
                    }
                    value => Error::raised_static(value, "Lua error"),
                };
                if error.kind() == ErrorKind::Memory {
                    return Err(error);
                }
                let level = if count < 2 || arg(self, 1) == Value::Nil {
                    1
                } else {
                    self.number(arg(self, 1))
                        .ok_or_else(|| self.argument_type_error(base, count, 1, "number"))?
                        as isize
                };
                let object = self.try_located_error(arg(self, 0), level)?;
                return Err(error.with_value(object));
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
                        Value::Function(_) | Value::Native(_) => {
                            self.set_closure_environment(target, env)?
                        }
                        Value::Thread(id) => self.set_thread_environment(id, env)?,
                        Value::Userdata(id) => {
                            self.reserve_gc_barrier(target, Value::Table(env))?;
                            self.userdata[slot_index(id.0)].env = env;
                        }
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
                            self.set_thread_environment(self.thread, env)?;
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
                        self.set_closure_environment(Value::Function(id), env)?;
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
                let name = match arg(self, 1) {
                    Value::Nil => source,
                    _ => self.string_argument(base, count, 1)?,
                };
                let binary = self.strings.bytes[slot_index(source.0)].first() == Some(&27);
                let depth = usize::from(self.native_calls_for(self.thread, self.frames.len()));
                // Compilation is synchronous and cannot reenter Lua. Borrowing
                // these rooted strings avoids unbudgeted copies of the source.
                let compiled = crate::memory::compilation::Workspace::new(self.registers.memory())
                    .and_then(|workspace| {
                        workspace.run(|| {
                            loads::compile(
                                &self.strings.bytes[slot_index(source.0)],
                                &self.strings.bytes[slot_index(name.0)],
                                depth,
                                &mut self.compile_spare,
                            )
                        })
                    });
                let compiled = compiled.and_then(|program| self.load_fresh(&program));
                match compiled {
                    Ok(function) => Value::Function(function),
                    Err(error) => {
                        let value = if error.kind() == ErrorKind::Memory {
                            Value::String(self.memory_error)
                        } else {
                            let message = loads::message(
                                &self.strings.bytes[slot_index(name.0)],
                                &error,
                                binary,
                            )?;
                            self.try_intern(&message)?
                        };
                        self.reserve_registers(base + 2)?;
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
                self.set_metatable(target, mt)?;
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
    try_format_number(n).expect("allocate formatted number")
}
pub(super) fn try_format_number(n: f64) -> Result<Vec<u8>, Error> {
    number_text(n).try_into_localized()
}

// Binary64 %.14g needs at most 22 ASCII bytes, including sign/exponent.
pub(super) struct NumberText {
    bytes: [u8; 64],
    len: usize,
}
impl NumberText {
    fn new() -> Self {
        Self {
            bytes: [0; 64],
            len: 0,
        }
    }
    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
    pub(super) fn try_into_localized(self) -> Result<Vec<u8>, Error> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(self.len)
            .map_err(|_| Error::memory())?;
        bytes.extend_from_slice(&self.bytes[..self.len]);
        crate::locale::try_localize(&mut bytes)?;
        Ok(bytes)
    }
    fn text(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).unwrap()
    }
}
impl std::fmt::Write for NumberText {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let end = self.len + text.len();
        let target = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        target.copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

fn short_decimal(n: f64) -> Option<(u64, u64, usize)> {
    let magnitude = n.abs();
    if magnitude < 0.0001 {
        return None;
    }
    // Below 1e14 the product's rounding error is smaller than half a unit
    // at fourteen significant digits. An integral product therefore identifies
    // the same short decimal as the general conversion, including 0.29-like
    // inputs that require trimming an extra trailing zero after multiplication.
    for (factor, width) in [
        (10u64, 1usize),
        (100, 2),
        (1000, 3),
        (10000, 4),
        (100000, 5),
        (1000000, 6),
    ] {
        let value = magnitude * factor as f64;
        if value >= 100_000_000_000_000.0 {
            break;
        }
        let integer = value as u64;
        if value == integer as f64 {
            return Some((integer, factor, width));
        }
    }
    None
}

pub(super) fn number_text(n: f64) -> NumberText {
    use std::fmt::Write;
    let mut output = NumberText::new();
    if !n.is_finite() {
        if n.is_nan() {
            write!(
                output,
                "{}{}",
                if n.is_sign_negative() { "-" } else { "" },
                crate::format::nan_body(n)
            )
            .unwrap();
        } else if n < 0.0 {
            output.write_str("-inf").unwrap();
        } else {
            output.write_str("inf").unwrap();
        }
    } else if n.abs() < 100_000_000_000_000.0 && n.fract() == 0.0 {
        // Integers with at most fourteen digits need no decimal rounding.
        // Keep the sign of zero, which an integer cast would discard.
        if n == 0.0 && n.is_sign_negative() {
            output.write_str("-0").unwrap();
        } else {
            write!(output, "{}", n as i64).unwrap();
        }
    } else if let Some((scaled, factor, width)) = short_decimal(n) {
        if n.is_sign_negative() {
            output.write_str("-").unwrap();
        }
        write!(output, "{}.{:0width$}", scaled / factor, scaled % factor).unwrap();
        while output.bytes[output.len - 1] == b'0' {
            output.len -= 1;
        }
        if output.bytes[output.len - 1] == b'.' {
            output.len -= 1;
        }
    } else {
        let mut scientific = NumberText::new();
        write!(scientific, "{n:.13e}").unwrap();
        let (digits, exp) = scientific
            .text()
            .split_once('e')
            .expect("scientific formatting includes exponent");
        let exponent: i32 = exp.parse().expect("formatted exponent is an integer");
        if !(-4..14).contains(&exponent) {
            let digits = digits.trim_end_matches('0').trim_end_matches('.');
            write!(
                output,
                "{digits}e{}{abs:02}",
                if exponent < 0 { '-' } else { '+' },
                abs = exponent.abs()
            )
            .unwrap();
        } else {
            // Reuse the already rounded significant digits for fixed notation.
            let magnitude = if let Some(rest) = digits.strip_prefix('-') {
                output.write_str("-").unwrap();
                rest
            } else {
                digits
            };
            let mut decimal = [0; 14];
            for (slot, byte) in decimal
                .iter_mut()
                .zip(magnitude.bytes().filter(|&b| b != b'.'))
            {
                *slot = byte;
            }
            let end = decimal.iter().rposition(|&b| b != b'0').unwrap() + 1;
            if exponent < 0 {
                output.write_str("0.").unwrap();
                for _ in exponent..-1 {
                    output.write_str("0").unwrap();
                }
                output
                    .write_str(std::str::from_utf8(&decimal[..end]).unwrap())
                    .unwrap();
            } else {
                let whole = exponent as usize + 1;
                output
                    .write_str(std::str::from_utf8(&decimal[..whole]).unwrap())
                    .unwrap();
                if whole < end {
                    output.write_str(".").unwrap();
                    output
                        .write_str(std::str::from_utf8(&decimal[whole..end]).unwrap())
                        .unwrap();
                }
            }
        }
    }
    output
}
