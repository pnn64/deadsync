//! Cold Lua 5.1 stack and prototype inspection; ordinary dispatch stores no names.
use super::*;

pub(super) const GET_INFO: u64 = 112;
pub(super) const GET_UPVALUE: u64 = 113;
pub(super) const SET_UPVALUE: u64 = 114;

#[derive(Clone, Copy)]
enum StackEntry {
    Frame(usize),
    Tail,
}

fn c_bytes(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}
fn bad(index: usize, message: &str) -> Error {
    Error::new(
        ErrorKind::Runtime,
        0,
        format!("bad argument #{} ({message})", index + 1),
    )
}

impl Vm {
    fn debug_frames(&self, thread: ThreadId) -> &[Frame] {
        if thread == self.thread {
            &self.frames
        } else {
            &self.threads[slot_index(thread.0)].frames
        }
    }
    fn debug_stack(&self, thread: ThreadId, mut level: i32) -> Option<StackEntry> {
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
    fn debug_name(&self, thread: ThreadId, index: usize) -> Option<(Box<[u8]>, &'static [u8])> {
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
        Some((c_bytes(&call.name).into(), call.kind.bytes()))
    }
    fn debug_nups(&self, id: u64) -> usize {
        match id {
            PAIRS | IPAIRS | NEW_PROXY => 1,
            _ if id >= strings::ITERATOR_FIRST => {
                match self.native_closures.get(id).map(|c| &c.kind) {
                    Some(threads::NativeKind::Match(_)) => 3,
                    Some(threads::NativeKind::Wrap(_)) => 1,
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
            match self.debug_stack(thread, level as i64 as i32) {
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
            return Err(bad(arg, "function or level expected"));
        };
        if !tail && options.iter().any(|b| !b"flnSuL".contains(b)) {
            return Err(bad(arg + 1, "invalid option"));
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
            return Err(bad(2, "value expected"));
        }
        let n = self
            .number(self.argument(base, count, 1))
            .ok_or_else(|| bad(1, "number expected"))? as i64 as i32;
        let function = self.argument(base, count, 0);
        let Value::Function(function) = function else {
            if matches!(function, Value::Native(_)) {
                return Ok(0);
            }
            return Err(bad(0, "function expected"));
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
}
