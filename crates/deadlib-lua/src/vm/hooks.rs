//! Per-thread hook controls and non-recursive event delivery.
use super::*;
use crate::bytecode::Prototype;
mod returns;

pub(super) fn stack_top(proto: &Prototype, frame: Frame, next: Option<&Frame>) -> Option<usize> {
    let next = next?;
    match next.function {
        FrameKind::Native(CONT_RETURN) => Some(next.vararg_start + next.argc()),
        FrameKind::Native(CONT_HOOK) => Some(frame.base + proto.debug_registers()),
        FrameKind::Native(CONT_NATIVE_HOOK) => Some(next.vararg_start + next.argc()),
        FrameKind::Native(CONT_TRACE) => {
            let previous = (next.vararg_count as usize)
                .checked_sub(1)
                .and_then(|pc| proto.code.get(pc));
            Some(
                if previous.is_some_and(|i| {
                    matches!(i.op, Op::Call | Op::Vararg | Op::Move) && i.c == OPEN
                }) {
                    next.result
                } else {
                    frame.base + proto.debug_registers()
                },
            )
        }
        _ => None,
    }
}

pub(super) const GET_HOOK: u64 = 118;
pub(super) const SET_HOOK: u64 = 119;
pub(super) const CALL: u8 = 1;
pub(super) const RETURN: u8 = 2;
pub(super) const LINE: u8 = 4;
pub(super) const COUNT: u8 = 8;
const REGISTRY_KEY: Value = Value::LightUserdata(u64::MAX - 1);

#[derive(Debug, Default)]
pub(super) struct Hooks {
    pub callback: Callback,
    pub host_running: Option<u64>,
    pub host_owner: Option<usize>,
    pub hidden_start: usize,
    pub hidden: Vec<Value>,
    // Unrooted physical slots beneath a C hook's saved top. Lowering the
    // public top releases roots without erasing their bits. Stack traversal
    // clears them, and writes followed by another pop replace the saved bits.
    pub popped_start: usize,
    pub popped: Vec<Value>,
    pub yielding: bool,
    pub yield_top: usize,
    pub yield_owner: Option<usize>,
    pub resumed_top: Option<(usize, usize)>,
    pub mask: u8,
    pub finalizers: u16,
    pub count: i32,
    pub ticks: i32,
    pub running: Option<u16>,
    pub enter: Option<u16>,
    pub tail: Option<u16>,
    last: Option<(u16, u32)>,
    skip: Option<(u16, u32)>,
}
#[derive(Clone, Copy, Debug, Default)]
pub(super) enum Callback {
    #[default]
    None,
    Lua,
    Host(u64),
}
impl Hooks {
    pub fn preserve_popped(&mut self, registers: &[Value], end: usize) {
        if self.popped.is_empty() {
            return;
        }
        let start = end.max(self.popped_start);
        let end = registers.len().min(self.popped_start + self.popped.len());
        if start < end {
            self.popped[start - self.popped_start..end - self.popped_start]
                .copy_from_slice(&registers[start..end]);
        }
    }
    pub fn allowed(&self) -> bool {
        self.running.is_none() && self.finalizers == 0
    }
    fn tick(&mut self) -> bool {
        self.ticks = self.ticks.wrapping_sub(1);
        let hit = self.mask & COUNT != 0 && self.ticks == 0;
        if hit {
            self.ticks = self.count;
        }
        hit
    }
    pub fn inherited(&self) -> Self {
        Self {
            callback: self.callback,
            mask: self.mask,
            count: self.count,
            ticks: self.count,
            ..Self::default()
        }
    }
}

fn visible(frame: Frame) -> bool {
    frame.is_call()
}

impl Vm {
    pub(super) fn lua_hook_values(&self) -> Result<(TableId, Value), Error> {
        let Value::Table(table) = self.tables[slot_index(self.registry.0)].get(REGISTRY_KEY) else {
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                "attempt to index a nil value",
            ));
        };
        Ok((
            table,
            self.tables[slot_index(table.0)].get(Value::LightUserdata(self.thread.0)),
        ))
    }
    fn hook_table(&mut self) -> Result<TableId, Error> {
        if let Value::Table(table) = self.tables[slot_index(self.registry.0)].get(REGISTRY_KEY) {
            return Ok(table);
        }
        let table = self.new_table();
        self.raw_set(self.registry, REGISTRY_KEY, Value::Table(table))?;
        Ok(table)
    }
    pub(super) fn trim_hooks(&mut self) {
        if self.hooks.running.is_some_and(|i| {
            !self.frames.get(usize::from(i)).is_some_and(|f| {
                matches!(f.function, FrameKind::Native(CONT_HOOK | CONT_NATIVE_HOOK))
            })
        }) {
            self.hooks.running = None;
            self.hooks.host_running = None;
            self.hooks.host_owner = None;
            self.hooks.hidden.clear();
            self.hooks.popped.clear();
            self.hooks.yielding = false;
        }
        if self
            .hooks
            .enter
            .is_some_and(|i| usize::from(i) >= self.frames.len())
        {
            self.hooks.enter = None;
        }
        if self
            .hooks
            .tail
            .is_some_and(|i| usize::from(i) >= self.frames.len())
        {
            self.hooks.tail = None;
        }
        self.hooks.skip = None;
        self.hooks.last = None;
        if self
            .hooks
            .yield_owner
            .is_some_and(|i| i >= self.frames.len())
        {
            self.hooks.yield_owner = None;
        }
        if self
            .hooks
            .resumed_top
            .is_some_and(|(i, _)| i >= self.frames.len())
        {
            self.hooks.resumed_top = None;
        }
    }
    pub(super) fn hook_native(
        &mut self,
        id: u64,
        base: usize,
        count: usize,
    ) -> Result<usize, Error> {
        let (thread, arg) = match self.argument(base, count, 0) {
            Value::Thread(thread) => (thread, 1),
            _ => (self.thread, 0),
        };
        if id == SET_HOOK {
            let value = self.argument(base, count, arg);
            let mut mask = 0;
            let mut ticks = 0;
            if value != Value::Nil {
                let text = self.string_argument(base, count, arg + 1)?;
                let text = &self.strings.bytes[slot_index(text.0)];
                let text = &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())];
                mask |= u8::from(text.contains(&b'c')) * CALL;
                mask |= u8::from(text.contains(&b'r')) * RETURN;
                mask |= u8::from(text.contains(&b'l')) * LINE;
                if !matches!(value, Value::Function(_) | Value::Native(_)) {
                    return Err(self.argument_type_error(base, count, arg, "function"));
                }
                let index = self.argument(base, count, arg + 2);
                if index != Value::Nil {
                    ticks =
                        debug::integer(self.number(index).ok_or_else(|| {
                            self.argument_type_error(base, count, arg + 2, "number")
                        })?);
                }
                if ticks > 0 {
                    mask |= COUNT;
                }
            }
            let table = self.hook_table()?;
            self.raw_set(table, Value::LightUserdata(thread.0), value)?;
            let state = if thread == self.thread {
                &mut self.hooks
            } else {
                &mut self.threads[slot_index(thread.0)].hooks
            };
            state.mask = mask;
            state.callback = if value == Value::Nil || mask == 0 {
                Callback::None
            } else {
                Callback::Lua
            };
            state.count = ticks;
            state.ticks = ticks;
            state.last = None;
            return Ok(0);
        }
        let state = if thread == self.thread {
            &self.hooks
        } else {
            &self.threads[slot_index(thread.0)].hooks
        };
        let (mask, count, callback) = (state.mask, state.count, state.callback);
        let mut text = [0; 3];
        let mut len = 0;
        for (flag, byte) in [(CALL, b'c'), (RETURN, b'r'), (LINE, b'l')] {
            if mask & flag != 0 {
                text[len] = byte;
                len += 1;
            }
        }
        self.resize_registers(self.registers.len().max(base + 3));
        self.registers[base] = if matches!(callback, Callback::Host(_)) {
            self.intern(b"external hook")
        } else {
            let table = self.hook_table()?;
            self.tables[slot_index(table.0)].get(Value::LightUserdata(thread.0))
        };
        self.registers[base + 1] = self.intern(&text[..len]);
        self.registers[base + 2] = Value::Number(f64::from(count));
        Ok(3)
    }
    pub(super) fn hook_call_record(&mut self, frame: Frame) {
        if self.hooks.mask & CALL != 0 && self.hooks.allowed() && visible(frame) {
            self.hooks.enter = Some((self.frames.len() - 1) as u16);
        }
    }
    fn hook_callback(
        &mut self,
        event: &[u8],
        line: Option<u32>,
        owner: usize,
        pc: u32,
    ) -> Result<(), Error> {
        if !self.hooks.allowed() {
            return Ok(());
        }
        match self.hooks.callback {
            Callback::None => return Ok(()),
            Callback::Host(id) => return self.host_hook(id, event, line, owner),
            Callback::Lua => {}
        }
        let Value::Table(table) = self.tables[slot_index(self.registry.0)].get(REGISTRY_KEY) else {
            return Ok(());
        };
        let function = self.tables[slot_index(table.0)].get(Value::LightUserdata(self.thread.0));
        if !matches!(function, Value::Function(_) | Value::Native(_)) {
            return Ok(());
        }
        let base = self.registers.len();
        let top = self.top;
        let event = self.intern(event);
        self.registers.extend_from_slice(&[
            Value::Table(table),
            function,
            event,
            line.map_or(Value::Nil, |n| Value::Number(f64::from(n))),
        ]);
        self.push_record(Frame {
            function: FrameKind::Native(CONT_HOOK),
            pc: 1,
            base,
            stack_start: base,
            result: top,
            wanted: 0,
            barriers: self.frames.last().map_or(1, |f| f.barriers + 1),
            vararg_start: owner,
            vararg_count: pc,
            tail_calls: 0,
            c_level_bias: 0,
        });
        self.hooks.running = Some((self.frames.len() - 1) as u16);
        if let Some(actual) = self.start_call(base + 1, 2, 0, false)? {
            self.finish_call(base + 1, actual, 0);
        }
        Ok(())
    }
    pub(super) fn hook_entry(&mut self, index: usize) -> Result<bool, Error> {
        if self.hooks.enter != Some(index as u16) {
            return Ok(false);
        }
        self.hooks.enter = None;
        if self.hooks.mask & CALL == 0 || !self.hooks.allowed() {
            return Ok(false);
        }
        let pc = self.frames[index].pc;
        if let FrameKind::Lua(id) = self.frames[index].function {
            let c = &self.functions[slot_index(id.0)];
            let p = &self.programs[c.program].data.prototypes[c.prototype];
            self.frames[index].pc =
                p.debug.hook_lines.iter().position(|&l| l != 0).unwrap_or(0) as u32 + 1;
        }
        self.hook_callback(b"call", None, index, pc)?;
        if self.hooks.allowed() {
            self.frames[index].pc = pc;
        }
        Ok(!self.hooks.allowed())
    }
    pub(super) fn hooked_tail(
        &mut self,
        function: FunctionId,
        call: usize,
        count: usize,
    ) -> Result<(), Error> {
        self.push_frame(function, call, count, OPEN)?;
        self.hooks.tail = Some((self.frames.len() - 1) as u16);
        Ok(())
    }
    pub(super) fn merge_hook_tail(&mut self, index: usize) -> Result<(), Error> {
        if self.hooks.tail != Some(index as u16)
            || !self.hooks.allowed()
            || self.hooks.enter.is_some()
        {
            return Ok(());
        }
        self.hooks.tail = None;
        let frame = self.frames.pop().expect("pending tail callee");
        let FrameKind::Lua(id) = frame.function else {
            unreachable!("Lua tail callee");
        };
        let closure = &self.functions[slot_index(id.0)];
        let proto = &self.programs[closure.program].data.prototypes[closure.prototype];
        let params = usize::from(proto.params);
        let compat = proto
            .compat_arg
            .then(|| self.registers[frame.base + params]);
        let call = self.registers.len();
        self.registers.push(Value::Function(id));
        self.registers
            .extend_from_within(frame.base..frame.base + params);
        self.registers
            .extend_from_within(frame.vararg_start..frame.vararg_start + frame.argc());
        self.tail_frame(id, call, params + frame.argc())?;
        if let Some(arg) = compat {
            let base = self.frames.last().unwrap().base;
            self.registers[base + params] = arg;
        }
        self.hooks.last = None;
        Ok(())
    }
    pub(super) fn hook_instruction(
        &mut self,
        index: usize,
        pc: usize,
        line: u32,
    ) -> Result<bool, Error> {
        if self.hooks.yield_owner == Some(index)
            && self.threads[slot_index(self.thread.0)]
                .resumed_values
                .is_some()
        {
            self.hooks.yield_owner = None;
            self.hooks.resumed_top = Some((index, self.top));
            self.threads[slot_index(self.thread.0)].resumed_values = None;
        }
        if self.hooks.skip == Some((index as u16, pc as u32)) {
            self.hooks.skip = None;
            return Ok(false);
        }
        if line == 0 || self.hooks.mask & (LINE | COUNT) == 0 {
            return Ok(false);
        }
        let FrameKind::Lua(id) = self.frames[index].function else {
            unreachable!("Lua hook instruction");
        };
        let closure = &self.functions[slot_index(id.0)];
        let proto = &self.programs[closure.program].data.prototypes[closure.prototype];
        let previous = self
            .hooks
            .last
            .filter(|&(i, _)| usize::from(i) == index)
            .map_or_else(
                || {
                    proto.debug.hook_lines[..pc]
                        .iter()
                        .rposition(|&l| l != 0)
                        .unwrap_or(0)
                },
                |(_, p)| p as usize,
            );
        let line_event = self.hooks.mask & LINE != 0
            && (pc == 0 || pc <= previous || line != proto.debug.hook_lines[previous]);
        self.hooks.last = Some((index as u16, pc as u32));
        let count = self.hooks.tick();
        if !self.hooks.allowed() || (!count && !line_event) {
            return Ok(false);
        }
        let base = self.registers.len();
        self.push_record(Frame {
            function: FrameKind::Native(CONT_TRACE),
            pc: 0,
            base,
            stack_start: base,
            result: self.top,
            wanted: 0,
            barriers: self.frames[index].barriers,
            vararg_start: index,
            vararg_count: pc as u32,
            tail_calls: (u32::from(count) << 1) | u32::from(line_event),
            c_level_bias: 0,
        });
        self.frames[index].pc = pc as u32 + 1;
        Ok(true)
    }
    pub(super) fn defer_hook_return(
        &mut self,
        mut frame: Frame,
        mut source: usize,
        count: usize,
    ) -> bool {
        if self.hooks.mask & RETURN == 0 || !self.hooks.allowed() || !visible(frame) {
            return false;
        }
        self.close_upvalues(frame.base);
        if let FrameKind::Native(id) = frame.function {
            source = self.native_return_stack(id, &mut frame, source, count);
            *self.frames.last_mut().expect("returning native frame") = frame;
        }
        let base = self.registers.len();
        self.push_record(Frame {
            function: FrameKind::Native(CONT_RETURN),
            pc: 0,
            base,
            stack_start: base,
            result: self.top,
            wanted: 0,
            barriers: frame.barriers,
            vararg_start: source,
            vararg_count: count as u32,
            tail_calls: 0,
            c_level_bias: 0,
        });
        true
    }
    pub(super) fn hook_step(&mut self, id: u64, frame: Frame) -> Result<Option<usize>, Error> {
        let index = self.frames.len() - 1;
        match id {
            CONT_HOOK => {
                self.frames.pop();
                self.truncate_registers(frame.base);
                self.top = frame.result;
                self.frames[frame.vararg_start].pc = frame.vararg_count;
                self.hooks.running = None;
            }
            CONT_TRACE => {
                let owner = frame.vararg_start;
                if frame.tail_calls != 0 {
                    let count = frame.tail_calls & 2 != 0;
                    self.frames[index].tail_calls &= if count { !2 } else { !1 };
                    let line = if count {
                        None
                    } else {
                        let FrameKind::Lua(id) = self.frames[owner].function else {
                            unreachable!();
                        };
                        let c = &self.functions[slot_index(id.0)];
                        let proto = &self.programs[c.program].data.prototypes[c.prototype];
                        Some(if proto.debug.native.is_some() {
                            proto.lines[frame.argc()]
                        } else {
                            proto.debug.hook_lines[frame.argc()]
                        })
                    };
                    self.hook_callback(
                        if count { b"count" } else { b"line" },
                        line,
                        owner,
                        frame.vararg_count + 1,
                    )?;
                } else {
                    self.frames.pop();
                    self.truncate_registers(frame.base);
                    self.top = frame.result;
                    self.frames[owner].pc = frame.vararg_count;
                    if std::mem::take(&mut self.hooks.yielding) {
                        self.hooks.skip = None;
                        self.hooks.last = None;
                        self.hooks.yield_owner = Some(owner);
                        self.transition = Some(threads::Transition::Yield {
                            source: self.hooks.yield_top,
                            count: 0,
                        });
                    } else {
                        self.hooks.skip = Some((owner as u16, frame.vararg_count));
                        self.hooks.last = Some((owner as u16, frame.vararg_count));
                    }
                }
            }
            CONT_RETURN => {
                let owner = index - 1;
                if frame.pc == 0 {
                    self.frames[index].pc = 1;
                    self.hook_callback(b"return", None, owner, self.frames[owner].pc)?;
                } else if self.hooks.mask & RETURN != 0
                    && self.frames[owner].tail_calls != 0
                    && matches!(self.frames[owner].function, FrameKind::Lua(_))
                {
                    self.frames[owner].tail_calls -= 1;
                    self.hook_callback(b"tail return", None, owner, self.frames[owner].pc)?;
                } else {
                    self.frames.pop();
                    return Ok(self.finish_frame(
                        self.frames[owner],
                        frame.vararg_start,
                        frame.argc(),
                    ));
                }
            }
            _ => unreachable!("hook continuation"),
        }
        Ok(None)
    }
}
