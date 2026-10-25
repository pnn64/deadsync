//! Cold Lua 5.1 stack and prototype inspection; ordinary dispatch stores no names.
use super::*;

pub(super) const GET_INFO: u64 = 112;
pub(super) const GET_UPVALUE: u64 = 113;
pub(super) const SET_UPVALUE: u64 = 114;
pub(super) const GET_LOCAL: u64 = 115;
pub(super) const SET_LOCAL: u64 = 116;
pub(super) const TRACEBACK: u64 = 117;
pub(super) const GET_REGISTRY: u64 = 120;

#[derive(Clone, Copy)]
pub(super) enum StackEntry {
    Frame(usize),
    Tail,
}

enum LocalSlot {
    Register(usize),
    Copy(Value),
}

enum LocalName {
    Named(usize, usize, usize),
    Temporary,
}

fn c_bytes(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}
pub(super) fn integer(value: f64) -> i32 {
    // MSVC x64's signed conversion returns MIN for non-finite/out-of-range
    // values; Lua 5.1 then narrows lua_Integer to its 32-bit stack index.
    let value = if !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value) {
        i64::MIN
    } else {
        value as i64
    };
    value as i32
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
    fn debug_nups(&self, id: u64) -> usize {
        match id {
            PAIRS | IPAIRS | NEW_PROXY => 1,
            _ if id >= strings::ITERATOR_FIRST => {
                match self.native_closures.get(id).map(|c| &c.kind) {
                    Some(threads::NativeKind::Match(_)) => 3,
                    Some(threads::NativeKind::Wrap(_)) => 1,
                    Some(threads::NativeKind::Lines { .. }) => 2,
                    None => 0,
                }
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
                    c_bytes(&data.name),
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
                    i64::from(proto.lines[pc])
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
        let Some(&cell) = n
            .checked_sub(1)
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| closure.upvalues.get(n))
        else {
            return Ok(0);
        };
        let data = Arc::clone(&self.programs[closure.program].data);
        let name = c_bytes(&data.prototypes[closure.prototype].debug.upvalues[n as usize - 1]);
        if id == SET_UPVALUE {
            self.set_cell(cell as usize, self.argument(base, count, 2));
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

    fn debug_register(&self, thread: ThreadId, register: usize) -> Option<Value> {
        if thread == self.thread {
            self.registers.get(register).copied()
        } else {
            self.threads[slot_index(thread.0)]
                .registers
                .get(register)
                .copied()
        }
    }
    fn debug_set_register(&mut self, thread: ThreadId, register: usize, value: Value) {
        if thread == self.thread {
            self.registers[register] = value;
        } else {
            self.threads[slot_index(thread.0)].registers[register] = value;
        }
    }
    fn local_slot(
        &self,
        thread: ThreadId,
        index: usize,
        n: usize,
    ) -> Option<(LocalName, LocalSlot)> {
        let frames = self.debug_frames(thread);
        let frame = frames[index];
        match frame.function {
            FrameKind::Lua(id) => {
                let closure = &self.functions[slot_index(id.0)];
                let proto = &self.programs[closure.program].data.prototypes[closure.prototype];
                let pc = frame.pc.saturating_sub(1);
                let mut active = [0u64; 4];
                let mut named = 0;
                for (local_index, local) in proto.debug.locals.iter().enumerate() {
                    if local.start <= pc && pc < local.end {
                        if named == n {
                            return Some((
                                LocalName::Named(closure.program, closure.prototype, local_index),
                                LocalSlot::Register(frame.base + usize::from(local.register)),
                            ));
                        }
                        named += 1;
                        active[usize::from(local.register) / 64] |= 1u64 << (local.register % 64);
                    }
                }
                let limit = frames
                    .get(index + 1)
                    .map_or(usize::from(proto.registers), |f| {
                        f.result.saturating_sub(frame.base)
                    })
                    .min(usize::from(proto.registers));
                // A call hook runs above the newly allocated full register
                // window, before the first instruction has produced live values.
                if frames
                    .get(index + 1)
                    .is_some_and(|f| matches!(f.function, FrameKind::Native(CONT_HOOK)))
                {
                    let mut wanted = n.checked_sub(named)?;
                    for r in 0..usize::from(proto.registers) {
                        if active[r / 64] & (1u64 << (r % 64)) == 0 {
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
                let current = thread == self.thread && index + 1 == frames.len();
                let position = match id {
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
                    threads::CO_YIELD if frame.pc != 0 => return None,
                    LOAD_READER if frame.pc != 0 => {
                        if n >= 3 {
                            return None;
                        }
                        frame.base + n
                    }
                    package::REQUIRE if matches!(frame.pc, 5 | 7) => {
                        if n >= 5 {
                            return None;
                        }
                        frame.base + n
                    }
                    package::MODULE if frame.pc == 12 => {
                        if n >= frame.argc() + 3 {
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
                            frame.base + 3
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
        let slot = match entry {
            StackEntry::Frame(i) => {
                let frames = self.debug_frames(thread);
                if id == SET_LOCAL
                    && thread != self.thread
                    && i + 1 == frames.len()
                    && n == 1
                    && matches!(frames[i].function, FrameKind::Native(threads::CO_YIELD))
                    && frames[i].pc != 0
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
        let value = match slot {
            LocalSlot::Register(r) => self
                .debug_register(thread, r)
                .expect("validated debug register"),
            LocalSlot::Copy(v) => v,
        };
        if id == SET_LOCAL
            && let LocalSlot::Register(r) = slot
        {
            self.debug_set_register(thread, r, self.argument(base, count, arg + 2));
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
