//! Cold Lua 5.1 stack and prototype inspection; ordinary dispatch stores no names.
use super::*;

pub(super) const GET_INFO: u64 = 112;
pub(super) const GET_UPVALUE: u64 = 113;
pub(super) const SET_UPVALUE: u64 = 114;
pub(super) const GET_LOCAL: u64 = 115;
pub(super) const SET_LOCAL: u64 = 116;
pub(super) const TRACEBACK: u64 = 117;
pub(super) const GET_REGISTRY: u64 = 120;
pub(super) const CONSOLE: u64 = 122;

#[derive(Clone, Copy)]
pub(super) enum StackEntry {
    Frame(usize),
    Tail,
}

pub(super) enum LocalSlot {
    Register(usize),
    HookHidden(usize),
    Copy(Value),
}

pub(super) enum LocalName {
    Named(usize, usize, usize),
    Temporary,
}

fn c_bytes(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}
pub(super) fn integer(value: f64) -> i32 {
    lua_integer(value) as i32
}

impl Vm {
    pub(super) fn debug_frames(&self, thread: ThreadId) -> &[Frame] {
        if thread == self.thread {
            &self.frames
        } else {
            &self.threads[slot_index(thread.0)].frames
        }
    }
    pub(super) fn debug_stack(&self, thread: ThreadId, mut level: i32) -> Option<StackEntry> {
        // The 5.1 API represents both negative levels and lost tail calls with
        // its synthetic tail record. Preserve that observable behavior.
        if level < 0 {
            return Some(StackEntry::Tail);
        }
        for (i, frame) in self.debug_frames(thread).iter().enumerate().rev() {
            if matches!(frame.function, FrameKind::Native(id) if (CONT_RAW..strings::ITERATOR_FIRST).contains(&id))
            {
                continue;
            }
            if level == 0 {
                return Some(StackEntry::Frame(i));
            }
            level -= 1;
            if i64::from(level) < i64::from(frame.tail_calls) {
                return Some(StackEntry::Tail);
            }
            level -= frame.tail_calls as i32;
        }
        None
    }
    pub(super) fn debug_name(
        &self,
        thread: ThreadId,
        index: usize,
    ) -> Option<(Box<[u8]>, &'static [u8])> {
        let frames = self.debug_frames(thread);
        if matches!(frames[index].function, FrameKind::Lua(_)) && frames[index].tail_calls != 0 {
            return None;
        }
        let caller = frames.get(index.checked_sub(1)?)?;
        let FrameKind::Lua(id) = caller.function else {
            return None;
        };
        let closure = &self.functions[slot_index(id.0)];
        let proto = &self.programs[closure.program].data.prototypes[closure.prototype];
        let pc = caller.pc.saturating_sub(1);
        let call = proto
            .debug
            .calls
            .binary_search_by_key(&pc, |c| c.pc)
            .ok()
            .map(|i| &proto.debug.calls[i])?;
        if call.kind.bytes().is_empty() {
            return None;
        }
        Some((c_bytes(&call.name).into(), call.kind.bytes()))
    }
    pub(super) fn debug_nups(&self, id: u64) -> usize {
        match id {
            _ if id >= strings::ITERATOR_FIRST => {
                self.native_closures.get(id).map_or(0, |c| c.captures.len())
            }
            _ => 0,
        }
    }
    fn debug_field(&mut self, table: TableId, name: &[u8], value: Value) -> Result<(), Error> {
        let key = self.intern(name);
        self.raw_set(table, key, value)
    }
    fn debug_string(&mut self, table: TableId, name: &[u8], bytes: &[u8]) -> Result<(), Error> {
        let value = self.intern(bytes);
        self.debug_field(table, name, value)
    }
    pub(super) fn debug_native(
        &mut self,
        id: u64,
        base: usize,
        count: usize,
    ) -> Result<usize, Error> {
        if matches!(id, GET_LOCAL | SET_LOCAL) {
            return self.debug_local(id, base, count);
        }
        if id != GET_INFO {
            return self.debug_upvalue(id, base, count);
        }
        let (thread, arg) = match self.argument(base, count, 0) {
            Value::Thread(id) => (id, 1),
            _ => (self.thread, 0),
        };
        // The options are checked before looking up a level or checking a target.
        let options = match self.argument(base, count, arg + 1) {
            Value::Nil => b"flnSu".to_vec(),
            _ => {
                let id = self.string_argument(base, count, arg + 1)?;
                c_bytes(&self.strings.bytes[slot_index(id.0)]).to_vec()
            }
        };
        let target = self.argument(base, count, arg);
        let (function, frame, tail) = if let Some(level) = self.number(target) {
            match self.debug_stack(thread, integer(level)) {
                None => {
                    self.registers[base] = Value::Nil;
                    return Ok(1);
                }
                Some(StackEntry::Tail) => (Value::Nil, None, true),
                Some(StackEntry::Frame(i)) => {
                    let function = match self.debug_frames(thread)[i].function {
                        FrameKind::Lua(id) => Value::Function(id),
                        FrameKind::Native(id) => Value::Native(id),
                    };
                    (function, Some(i), false)
                }
            }
        } else if matches!(target, Value::Function(_) | Value::Native(_)) {
            (target, None, false)
        } else {
            return Err(self.argument_error(arg, b"function or level expected"));
        };
        if !tail && options.iter().any(|b| !b"flnSuL".contains(b)) {
            return Err(self.argument_error(arg + 1, b"invalid option"));
        }
        let code = if let Value::Function(id) = function {
            let closure = &self.functions[slot_index(id.0)];
            Some((
                Arc::clone(&self.programs[closure.program].data),
                closure.prototype,
            ))
        } else {
            None
        };
        let table = self.new_table();
        if options.contains(&b'S') {
            let (source, what, defined) = if let Some((data, p)) = &code {
                let proto = &data.prototypes[*p];
                (
                    c_bytes(data.source(proto)),
                    if proto.debug.defined[0] == 0 {
                        b"main".as_slice()
                    } else {
                        b"Lua"
                    },
                    proto.debug.defined.map(i64::from),
                )
            } else if tail {
                (b"=(tail call)".as_slice(), b"tail".as_slice(), [-1, -1])
            } else {
                (b"=[C]".as_slice(), b"C".as_slice(), [-1, -1])
            };
            self.debug_string(table, b"source", source)?;
            self.debug_string(table, b"short_src", &crate::bytecode::chunk_id(source))?;
            self.debug_string(table, b"what", what)?;
            self.debug_field(table, b"linedefined", Value::Number(defined[0] as f64))?;
            self.debug_field(table, b"lastlinedefined", Value::Number(defined[1] as f64))?;
        }
        if options.contains(&b'l') {
            let line = match (&code, frame) {
                (Some((data, p)), Some(i)) => {
                    let proto = &data.prototypes[*p];
                    let pc = self.debug_frames(thread)[i]
                        .pc()
                        .saturating_sub(1)
                        .min(proto.lines.len() - 1);
                    proto.debug_line(pc)
                }
                _ => -1,
            };
            self.debug_field(table, b"currentline", Value::Number(line as f64))?;
        }
        if options.contains(&b'u') {
            let n = match function {
                Value::Function(id) => self.functions[slot_index(id.0)].upvalues.len(),
                Value::Native(id) => self.debug_nups(id),
                _ => 0,
            };
            self.debug_field(table, b"nups", Value::Number(n as f64))?;
        }
        if options.contains(&b'n') {
            let name = frame.and_then(|i| self.debug_name(thread, i));
            if let Some((name, kind)) = name {
                self.debug_string(table, b"name", &name)?;
                self.debug_string(table, b"namewhat", kind)?;
            } else {
                if tail {
                    self.debug_string(table, b"name", b"")?;
                }
                self.debug_string(table, b"namewhat", b"")?;
            }
        }
        if options.contains(&b'f') {
            self.debug_field(table, b"func", function)?;
        }
        if options.contains(&b'L')
            && let Some((data, p)) = &code
        {
            let lines = self.new_table();
            for &line in &data.prototypes[*p].debug.active_lines {
                self.raw_set(lines, Value::Number(f64::from(line)), Value::Boolean(true))?;
            }
            self.debug_field(table, b"activelines", Value::Table(lines))?;
        }
        self.registers[base] = Value::Table(table);
        Ok(1)
    }
    fn debug_upvalue(&mut self, id: u64, base: usize, count: usize) -> Result<usize, Error> {
        if id == SET_UPVALUE && count < 3 {
            return Err(self.argument_error(2, b"value expected"));
        }
        let n = integer(
            self.number(self.argument(base, count, 1))
                .ok_or_else(|| self.argument_type_error(base, count, 1, "number"))?,
        );
        let function = self.argument(base, count, 0);
        let Value::Function(function) = function else {
            if matches!(function, Value::Native(_)) {
                return Ok(0);
            }
            return Err(self.argument_type_error(base, count, 0, "function"));
        };
        let closure = &self.functions[slot_index(function.0)];
        let debug = &self.programs[closure.program].data.prototypes[closure.prototype].debug;
        // Native chunks may omit debug names while retaining executable cells.
        // Lua 5.1 exposes only the names present in the imported prototype.
        let named = debug
            .native
            .as_ref()
            .map_or(debug.upvalues.len(), |native| native.wire.names);
        let Some(&cell) = n
            .checked_sub(1)
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&n| n < named)
            .and_then(|n| closure.upvalues.get(n))
        else {
            return Ok(0);
        };
        let data = Arc::clone(&self.programs[closure.program].data);
        let name = c_bytes(&data.prototypes[closure.prototype].debug.upvalues[n as usize - 1]);
        if id == SET_UPVALUE {
            // lua_setupvalue consumes the actual top, including extra arguments.
            self.set_cell(cell as usize, self.argument(base, count, count - 1));
        }
        let name = self.intern(name);
        self.registers[base] = name;
        if id == GET_UPVALUE {
            self.resize_registers(self.registers.len().max(base + 2));
            self.registers[base + 1] = self.cell_value(cell as usize);
            Ok(2)
        } else {
            Ok(1)
        }
    }

    pub(super) fn debug_register(&self, thread: ThreadId, register: usize) -> Option<Value> {
        if thread == self.thread {
            self.registers.get(register).copied()
        } else {
            self.threads[slot_index(thread.0)]
                .registers
                .get(register)
                .copied()
        }
    }
    pub(super) fn debug_set_register(&mut self, thread: ThreadId, register: usize, value: Value) {
        if thread == self.thread {
            self.registers[register] = value;
        } else {
            self.threads[slot_index(thread.0)].registers[register] = value;
        }
    }
    pub(super) fn local_slot(
        &self,
        thread: ThreadId,
        index: usize,
        n: usize,
    ) -> Option<(LocalName, LocalSlot)> {
        let result = self.local_slot_inner(thread, index, n)?;
        let hooks = if thread == self.thread {
            &self.hooks
        } else {
            &self.threads[slot_index(thread.0)].hooks
        };
        if hooks.host_owner == Some(index)
            && matches!(result.0, LocalName::Named(..))
            && let LocalSlot::Register(r) = result.1
            && let Some(slot) = r
                .checked_sub(hooks.hidden_start)
                .filter(|&i| i < hooks.hidden.len())
        {
            return Some((result.0, LocalSlot::HookHidden(slot)));
        }
        Some(result)
    }
    pub(super) fn debug_local_value(&self, thread: ThreadId, slot: &LocalSlot) -> Option<Value> {
        match *slot {
            LocalSlot::Register(r) => self.debug_register(thread, r),
            LocalSlot::Copy(value) => Some(value),
            LocalSlot::HookHidden(i) => {
                let hooks = if thread == self.thread {
                    &self.hooks
                } else {
                    &self.threads[slot_index(thread.0)].hooks
                };
                hooks.hidden.get(i).copied()
            }
        }
    }
    pub(super) fn debug_write_local(&mut self, thread: ThreadId, slot: &LocalSlot, value: Value) {
        match *slot {
            LocalSlot::Register(r) => self.debug_set_register(thread, r, value),
            LocalSlot::Copy(_) => {}
            LocalSlot::HookHidden(i) => {
                let hooks = if thread == self.thread {
                    &mut self.hooks
                } else {
                    &mut self.threads[slot_index(thread.0)].hooks
                };
                hooks.hidden[i] = value;
            }
        }
        self.gc_barrier(Value::Thread(thread), value);
    }
    fn local_slot_inner(
        &self,
        thread: ThreadId,
        index: usize,
        n: usize,
    ) -> Option<(LocalName, LocalSlot)> {
        let frames = self.debug_frames(thread);
        let frame = frames[index];
        let hooks = if thread == self.thread {
            &self.hooks
        } else {
            &self.threads[slot_index(thread.0)].hooks
        };
        let native_hook = hooks
            .running
            .filter(|_| hooks.host_owner == Some(index))
            .and_then(|i| frames.get(usize::from(i)));
        match frame.function {
            FrameKind::Lua(id) => {
                let closure = &self.functions[slot_index(id.0)];
                let proto = &self.programs[closure.program].data.prototypes[closure.prototype];
                let pc = frame.pc.saturating_sub(1);
                let hook = frames[index + 1..].iter().find(|f| {
                    matches!(f.function, FrameKind::Native(CONT_HOOK)) && f.vararg_start == index
                });
                let window = native_hook
                    .map(|f| f.vararg_start + f.argc())
                    .or_else(|| hooks::stack_top(proto, frame, frames.get(index + 1)))
                    .unwrap_or(frame.base + proto.debug_registers())
                    .saturating_sub(frame.base);
                let register = |r: usize| {
                    if let Some(hook) = hook.filter(|_| r >= window) {
                        let position = hook.base + r - window;
                        if self.debug_register(thread, position).is_some() {
                            return position;
                        }
                    }
                    frame.base + r
                };
                let mut active = [0u64; 4];
                let mut named = 0;
                for (local_index, local) in proto.debug.locals.iter().enumerate() {
                    if local.start <= pc && pc < local.end {
                        if named == n {
                            return Some((
                                LocalName::Named(closure.program, closure.prototype, local_index),
                                LocalSlot::Register(register(usize::from(local.register))),
                            ));
                        }
                        named += 1;
                        active[usize::from(local.register) / 64] |= 1u64 << (local.register % 64);
                    }
                }
                // A Lua hook is called above the interrupted stack window and
                // leaves its registry hook table just below the callback.
                if hook.is_some() || native_hook.is_some() {
                    if let Some(hook) = hook.filter(|_| n >= window) {
                        return (n == window)
                            .then_some((LocalName::Temporary, LocalSlot::Register(hook.base)));
                    }
                    let mut wanted = n.checked_sub(named)?;
                    for r in 0..window {
                        if active.get(r / 64).copied().unwrap_or(0) & (1u64 << (r % 64)) == 0 {
                            if wanted == 0 {
                                return Some((
                                    LocalName::Temporary,
                                    LocalSlot::Register(frame.base + r),
                                ));
                            }
                            wanted -= 1;
                        }
                    }
                    return None;
                }
                let limit = if proto.debug.native.is_some() {
                    let instruction = proto.code[(pc as usize).min(proto.code.len() - 1)];
                    if frames.get(index + 1).is_some()
                        && matches!(instruction.op, Op::Call | Op::CallIter | Op::TailCall)
                    {
                        usize::from(instruction.a)
                    } else {
                        proto.debug_registers()
                    }
                } else {
                    frames
                        .get(index + 1)
                        .map_or(proto.debug_registers(), |f| {
                            f.result.saturating_sub(frame.base)
                        })
                        .min(proto.debug_registers())
                };
                let pc = (pc as usize).min(proto.gc_live.len() - 1);
                if let Ok(i) = proto
                    .debug
                    .calls
                    .binary_search_by_key(&(pc as u32), |c| c.pc)
                {
                    let range = &proto.debug.calls[i].temporaries;
                    let register = *proto.debug.temporaries
                        [range.start as usize..range.end as usize]
                        .get(n.checked_sub(named)?)?;
                    return Some((
                        LocalName::Temporary,
                        LocalSlot::Register(frame.base + usize::from(register)),
                    ));
                }
                let live = &proto.gc_maps[proto.gc_live[pc] as u16 as usize];
                let mut wanted = n.checked_sub(named)?;
                for r in 0..limit {
                    if live[r / 64] & !active[r / 64] & (1u64 << (r % 64)) != 0 {
                        if wanted == 0 {
                            return Some((
                                LocalName::Temporary,
                                LocalSlot::Register(frame.base + r),
                            ));
                        }
                        wanted -= 1;
                    }
                }
                None
            }
            FrameKind::Native(id) => {
                let id = self.body_code(thread, index).unwrap_or(id);
                if let Some(hook) = native_hook {
                    return (n < hook.argc()).then_some((
                        LocalName::Temporary,
                        LocalSlot::Register(hook.vararg_start + n),
                    ));
                }
                if frames
                    .get(index + 1)
                    .is_some_and(|f| matches!(f.function, FrameKind::Native(CONT_RETURN)))
                {
                    if n < frame.argc() {
                        return Some((
                            LocalName::Temporary,
                            LocalSlot::Register(frame.vararg_start + n),
                        ));
                    }
                    let hook = frames[index + 1..].iter().find(|f| {
                        matches!(f.function, FrameKind::Native(CONT_HOOK))
                            && f.vararg_start == index
                    })?;
                    return (n == frame.argc())
                        .then_some((LocalName::Temporary, LocalSlot::Register(hook.base)));
                }
                let current = thread == self.thread && index + 1 == frames.len();
                if thread != self.thread
                    && index + 1 == frames.len()
                    && self.threads[slot_index(thread.0)].resume_status
                        == threads::NativeThreadStatus::Yielded
                    && (id == threads::CO_YIELD || frame.pc == threads::HOST_YIELDED)
                {
                    let window = &self.threads[slot_index(thread.0)].api_stack;
                    return (n < window.len())
                        .then_some((LocalName::Temporary, LocalSlot::Register(window.start + n)));
                }
                if (matches!(
                    id,
                    strings::STR_DUMP | CONSOLE | threads::CO_RESUME | threads::CO_YIELD
                ) || frame.pc == threads::HOST_YIELDED
                    || self.scheduled_native(id) == 1)
                    && n == frame.argc()
                {
                    let hook = frames[index + 1..].iter().find(|f| {
                        matches!(f.function, FrameKind::Native(CONT_HOOK))
                            && f.vararg_start == index
                    })?;
                    return Some((LocalName::Temporary, LocalSlot::Register(hook.base)));
                }
                let position = if let Some(slot) = self.package_local_slot(id, frame, n) {
                    slot?
                } else {
                    match id {
                        SET_LOCAL if current => {
                            let arg = usize::from(matches!(
                                self.debug_register(thread, frame.vararg_start),
                                Some(Value::Thread(_))
                            ));
                            if n >= arg + 3 {
                                return None;
                            }
                            frame.vararg_start + n
                        }
                        PCALL if !current && frame.pc == 1 => return None,
                        XPCALL if !current && frame.pc == 1 => {
                            if n != 0 {
                                return None;
                            }
                            frame.vararg_start + 1
                        }
                        LOAD_READER if frame.pc != 0 => {
                            if n >= frame.argc() {
                                return None;
                            }
                            frame.base + n
                        }
                        TRACEBACK if frame.pc != 0 => {
                            let jobs = if thread == self.thread {
                                &self.tracebacks
                            } else {
                                &self.threads[slot_index(thread.0)].tracebacks
                            };
                            let job = jobs.iter().rev().find(|j| j.owner == index)?;
                            if n >= job.arg + job.top {
                                return None;
                            }
                            frame.base + n
                        }
                        TOSTRING if !current => {
                            if n != 0 {
                                return None;
                            }
                            frame.vararg_start
                        }
                        PRINT if !current && frame.pc >= 2 => {
                            if n == frame.argc() {
                                frame.base + 1
                            } else if n < frame.argc() {
                                frame.vararg_start + n
                            } else {
                                return None;
                            }
                        }
                        tables::TAB_FOREACH if !current && frame.pc == 1 => {
                            if n == frame.argc() {
                                frame.base + 1
                            } else if n == frame.argc() + 1 {
                                frame.base + 2
                            } else if n < frame.argc() {
                                frame.vararg_start + n
                            } else {
                                return None;
                            }
                        }
                        _ => {
                            if n >= frame.argc() {
                                return None;
                            }
                            frame.vararg_start + n
                        }
                    }
                };
                self.debug_register(thread, position)?;
                Some((LocalName::Temporary, LocalSlot::Register(position)))
            }
        }
    }
    fn debug_local(&mut self, id: u64, base: usize, count: usize) -> Result<usize, Error> {
        let (thread, arg) = match self.argument(base, count, 0) {
            Value::Thread(id) => (id, 1),
            _ => (self.thread, 0),
        };
        let level = integer(
            self.number(self.argument(base, count, arg))
                .ok_or_else(|| self.argument_type_error(base, count, arg, "number"))?,
        );
        let entry = self
            .debug_stack(thread, level)
            .ok_or_else(|| self.argument_error(arg, b"level out of range"))?;
        if id == SET_LOCAL && count < arg + 3 {
            return Err(self.argument_error(arg + 2, b"value expected"));
        }
        let n = integer(
            self.number(self.argument(base, count, arg + 1))
                .ok_or_else(|| self.argument_type_error(base, count, arg + 1, "number"))?,
        );
        if id == SET_LOCAL
            && let Some(index) = self.active_native
        {
            // A self-write can replace slot one with a thread. Preserve the
            // original optional-thread prefix for the completed C window.
            self.frames[index].pc = arg as u32;
        }
        let slot = match entry {
            StackEntry::Frame(i) => {
                let frames = self.debug_frames(thread);
                if id == SET_LOCAL
                    && thread != self.thread
                    && i + 1 == frames.len()
                    && self.threads[slot_index(thread.0)].resume_status
                        == threads::NativeThreadStatus::Yielded
                    && usize::try_from(n)
                        == Ok(self.threads[slot_index(thread.0)].api_stack.len() + 1)
                    && (matches!(frames[i].function, FrameKind::Native(threads::CO_YIELD))
                        || frames[i].pc == threads::HOST_YIELDED)
                {
                    Some((
                        LocalName::Temporary,
                        LocalSlot::Copy(self.argument(base, count, arg + 2)),
                    ))
                } else {
                    n.checked_sub(1)
                        .and_then(|n| usize::try_from(n).ok())
                        .and_then(|n| self.local_slot(thread, i, n))
                }
            }
            StackEntry::Tail => None,
        };
        let Some((name, slot)) = slot else {
            self.registers[base] = Value::Nil;
            return Ok(1);
        };
        let mut value = self
            .debug_local_value(thread, &slot)
            .expect("validated debug slot");
        if matches!(name, LocalName::Temporary) && !self.is_live_value(value) {
            // Precise roots can leave a reclaimed object's old bits in an
            // unused physical slot. Debug access must not revive that handle.
            value = Value::Nil;
        }
        if id == SET_LOCAL {
            self.debug_write_local(thread, &slot, self.argument(base, count, arg + 2));
        }
        let name = match name {
            LocalName::Named(program, prototype, local) => {
                let bytes = &self.programs[program].data.prototypes[prototype]
                    .debug
                    .locals[local]
                    .name;
                Value::String(self.strings.intern(c_bytes(bytes)))
            }
            LocalName::Temporary => self.intern(b"(*temporary)"),
        };
        self.resize_registers(self.registers.len().max(base + 2));
        self.registers[base] = name;
        if id == GET_LOCAL {
            self.registers[base + 1] = value;
            Ok(2)
        } else {
            Ok(1)
        }
    }
}
