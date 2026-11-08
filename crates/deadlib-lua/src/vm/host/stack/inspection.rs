//! Lua 5.1 native activation records and stack inspection.
use super::*;
use crate::vm::debug::{LocalName, LocalSlot, StackEntry};

/// A Lua 5.1 activation-record index on a particular thread.
/// This handle does not retain the thread. It is valid only while the inspected
/// call is still present; index zero represents a lost tail call/base record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeDebugFrame {
    thread: ThreadId,
    index: i32,
}
impl NativeDebugFrame {
    /// Reconstruct an activation record retained by a native adapter.
    /// Inspection validates the thread and index before accessing storage.
    pub fn from_index(thread: ThreadId, index: i32) -> Self {
        Self { thread, index }
    }
    /// Thread to which the activation record belongs.
    pub fn thread(self) -> ThreadId {
        self.thread
    }
    /// Visible CallInfo ordinal, suitable for Lua 5.1's `lua_Debug.i_ci`.
    pub fn index(self) -> i32 {
        self.index
    }
}

/// Stable NUL-terminated debug string storage, borrowed with
/// [`NativeStack::debug_bytes`]. Interned strings remain valid while their
/// owning program is reachable. This handle itself is not a collector root.
#[derive(Clone, Copy, Debug)]
pub enum NativeDebugString {
    /// Process-lifetime storage including the final NUL.
    Static(&'static [u8]),
    /// VM string storage including a final NUL when borrowed.
    Interned(StringId),
}

/// Requested fields from `lua_getinfo`. Absent fields must be left unchanged
/// in a C activation record. A requested name may itself be NULL.
#[derive(Debug, Default)]
pub struct NativeDebugInfo {
    /// False for an invalid option; valid options still take effect.
    pub valid: bool,
    /// Source name (`S`).
    pub source: Option<NativeDebugString>,
    /// NUL-terminated function kind (`S`).
    pub what: Option<&'static [u8]>,
    /// First and last definition line (`S`).
    pub defined: Option<[i32; 2]>,
    /// Bytes written to short_src (`S`), including NUL and '=' source padding.
    pub short_source: Option<Vec<u8>>,
    /// Current line, or -1 (`l`).
    pub current_line: Option<i32>,
    /// Number of captures (`u`).
    pub upvalues: Option<usize>,
    /// Inferred name (`n`); inner None is a C NULL pointer.
    pub name: Option<Option<NativeDebugString>>,
    /// NUL-terminated inferred name category (`n`).
    pub name_what: Option<&'static [u8]>,
}

impl Vm {
    fn native_debug_string(&mut self, program: usize, bytes: &[u8]) -> NativeDebugString {
        let Value::String(id) = self.intern(bytes) else {
            unreachable!("intern returns a string")
        };
        let names = &mut self.programs[program].debug_strings;
        if !names.contains(&id) {
            let before = names.capacity();
            names.push(id);
            let growth = (names.capacity() - before) * size_of::<StringId>();
            self.programs.note_alloc(growth);
        }
        self.gc_new_value(Value::String(id));
        NativeDebugString::Interned(id)
    }
}

impl NativeStack<'_, '_> {
    fn debug_entry(&self, record: NativeDebugFrame) -> Result<StackEntry, Error> {
        self.check_error()?;
        self.call.vm.validate_value(Value::Thread(record.thread))?;
        if record.index == 0 {
            return Ok(StackEntry::Tail);
        }
        let index = usize::try_from(record.index)
            .ok()
            .and_then(|i| i.checked_sub(1))
            .and_then(|i| {
                self.call
                    .vm
                    .debug_frames(record.thread)
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.is_call())
                    .nth(i)
                    .map(|(i, _)| i)
            })
            .ok_or_else(|| self.error(b"invalid native activation record"))?;
        Ok(StackEntry::Frame(index))
    }
    fn debug_push(&mut self, thread: ThreadId, value: Value) -> Result<(), Error> {
        if thread == self.thread_id() {
            self.push(value)
        } else {
            self.thread_stack(thread)?.push(value)
        }
    }
    fn debug_top(&mut self, thread: ThreadId) -> Result<usize, Error> {
        if thread == self.thread_id() {
            Ok(self.top())
        } else {
            Ok(self.thread_stack(thread)?.top())
        }
    }
    fn debug_pop(&mut self, thread: ThreadId) -> Result<Value, Error> {
        if thread == self.thread_id() {
            let value = self
                .value(-1)
                .ok_or_else(|| self.error(b"native debug stack underflow"))?;
            self.set_top(self.top() - 1)?;
            Ok(value)
        } else {
            let mut stack = self.thread_stack(thread)?;
            let value = stack
                .value(-1)
                .ok_or_else(|| stack.error(b"native debug stack underflow"))?;
            stack.set_top(stack.top() - 1)?;
            Ok(value)
        }
    }
    /// Find a visible call without changing any stack values. Negative levels
    /// and lost tail calls return the synthetic zero record, as in Lua 5.1.
    ///
    /// # Errors
    /// Rejects a pending error or a collected/invalid thread.
    pub fn get_stack(
        &self,
        thread: ThreadId,
        level: i32,
    ) -> Result<Option<NativeDebugFrame>, Error> {
        self.check_error()?;
        self.call.vm.validate_value(Value::Thread(thread))?;
        Ok(self.call.vm.debug_stack(thread, level).map(|entry| {
            let index = match entry {
                StackEntry::Tail => 0,
                StackEntry::Frame(i) => self.call.vm.debug_frames(thread)[..=i]
                    .iter()
                    .filter(|f| f.is_call())
                    .count() as i32,
            };
            NativeDebugFrame { thread, index }
        }))
    }
    /// Borrow NUL-terminated storage, or None if an interned name was collected.
    pub fn debug_bytes(&self, name: NativeDebugString) -> Option<&[u8]> {
        match name {
            NativeDebugString::Static(bytes) => Some(bytes),
            NativeDebugString::Interned(id) => self
                .call
                .vm
                .strings
                .bytes
                .get(id.0)
                .map(ByteString::c_bytes),
        }
    }
    fn native_local(
        &self,
        record: NativeDebugFrame,
        n: i32,
    ) -> Result<Option<(LocalName, LocalSlot)>, Error> {
        let entry = self.debug_entry(record)?;
        let Some(n) = n.checked_sub(1).and_then(|n| usize::try_from(n).ok()) else {
            return Ok(None);
        };
        Ok(match entry {
            StackEntry::Frame(i) => self.call.vm.local_slot(record.thread, i, n),
            StackEntry::Tail => {
                let vm = &self.call.vm;
                let limit = vm
                    .debug_frames(record.thread)
                    .iter()
                    .find(|f| f.is_call())
                    .map_or_else(
                        || {
                            if record.thread == vm.thread {
                                self.base + self.top()
                            } else {
                                vm.threads[slot_index(record.thread.0)].api_stack.end
                            }
                        },
                        |f| f.result,
                    );
                (n < limit).then_some((LocalName::Temporary, LocalSlot::Register(n)))
            }
        })
    }
    fn native_local_name(&mut self, name: LocalName) -> NativeDebugString {
        match name {
            LocalName::Temporary => NativeDebugString::Static(b"(*temporary)\0"),
            LocalName::Named(p, proto, local) => {
                let data = Arc::clone(&self.call.vm.programs[p].data);
                self.call
                    .vm
                    .native_debug_string(p, &data.prototypes[proto].debug.locals[local].name)
            }
        }
    }
    /// Push a local on the inspected thread's native stack and return its name.
    /// Missing locals leave that stack unchanged. Numbers start at one.
    ///
    /// # Errors
    /// Rejects invalid activation records, pending errors or stack overflow.
    pub fn get_local(
        &mut self,
        record: NativeDebugFrame,
        n: i32,
    ) -> Result<Option<NativeDebugString>, Error> {
        let Some((name, slot)) = self.native_local(record, n)? else {
            return Ok(None);
        };
        let value = self
            .call
            .vm
            .debug_local_value(record.thread, &slot)
            .unwrap_or(Value::Nil);
        let value = if self.call.vm.is_live_value(value) {
            value
        } else {
            Value::Nil
        };
        self.debug_push(record.thread, value)?;
        Ok(Some(self.native_local_name(name)))
    }
    /// Pop the inspected thread's top into a local. The value is popped even
    /// when the local number is invalid; name lookup includes that top value.
    ///
    /// # Errors
    /// Rejects invalid activation records, pending errors or an empty stack.
    pub fn set_local(
        &mut self,
        record: NativeDebugFrame,
        n: i32,
    ) -> Result<Option<NativeDebugString>, Error> {
        let slot = self.native_local(record, n)?;
        if self.debug_top(record.thread)? == 0 {
            return Err(self.error(b"native debug stack underflow"));
        }
        // Write before popping: the local can alias the value being popped.
        let value = if record.thread == self.thread_id() {
            self.value(-1).unwrap()
        } else {
            self.thread_stack(record.thread)?.value(-1).unwrap()
        };
        if let Some((_, slot)) = &slot {
            self.call.vm.debug_write_local(record.thread, slot, value);
        }
        self.debug_pop(record.thread)?;
        Ok(slot.map(|(name, _)| self.native_local_name(name)))
    }
    /// Inspect selected fields with Lua 5.1 options `SlnufL`. A leading `>`
    /// pops a function from the selected thread and ignores the record index.
    /// Options end at NUL. Invalid options set `valid` false while valid fields
    /// and pushes still take effect. `f` pushes once before `L`, regardless of
    /// option order or duplicates. Tail records populate all fields.
    ///
    /// # Errors
    /// Rejects invalid records, nonfunction/absent `>` targets, pending errors
    /// and native stack overflow.
    pub fn get_info(
        &mut self,
        record: NativeDebugFrame,
        options: &[u8],
    ) -> Result<NativeDebugInfo, Error> {
        self.check_error()?;
        self.call.vm.validate_value(Value::Thread(record.thread))?;
        let options = &options[..options
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(options.len())];
        let (options, function, frame) = if let Some(options) = options.strip_prefix(b">") {
            let function = if record.thread == self.thread_id() {
                self.value(-1)
            } else {
                self.thread_stack(record.thread)?.value(-1)
            };
            let Some(function @ (Value::Function(_) | Value::Native(_))) = function else {
                return Err(self.error(b"native debug function expected"));
            };
            self.debug_pop(record.thread)?;
            (options, function, None)
        } else {
            match self.debug_entry(record)? {
                StackEntry::Tail => (options, Value::Nil, None),
                StackEntry::Frame(i) => {
                    let f = match self.call.vm.debug_frames(record.thread)[i].function {
                        FrameKind::Lua(id) => Value::Function(id),
                        FrameKind::Native(id) => Value::Native(id),
                    };
                    (options, f, Some(i))
                }
            }
        };
        let tail = function == Value::Nil;
        let mut info = NativeDebugInfo {
            valid: tail || options.iter().all(|b| b"SlnufL".contains(b)),
            ..NativeDebugInfo::default()
        };
        let code = if let Value::Function(id) = function {
            let c = &self.call.vm.functions[slot_index(id.0)];
            Some((
                c.program,
                c.prototype,
                Arc::clone(&self.call.vm.programs[c.program].data),
            ))
        } else {
            None
        };
        if tail || options.contains(&b'S') {
            let (source, what, defined) = if let Some((p, proto, data)) = &code {
                let proto = &data.prototypes[*proto];
                (
                    self.call.vm.native_debug_string(*p, data.source(proto)),
                    if proto.debug.defined[0] == 0 {
                        b"main\0".as_slice()
                    } else {
                        b"Lua\0"
                    },
                    proto.debug.defined.map(|n| n as i32),
                )
            } else if tail {
                (
                    NativeDebugString::Static(b"=(tail call)\0"),
                    b"tail\0".as_slice(),
                    [-1, -1],
                )
            } else {
                (
                    NativeDebugString::Static(b"=[C]\0"),
                    b"C\0".as_slice(),
                    [-1, -1],
                )
            };
            let bytes = self.debug_bytes(source).unwrap();
            let bytes = &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())];
            let mut short = crate::bytecode::chunk_id(bytes);
            short.push(0);
            // Lua's '=' branch uses strncpy across all LUA_IDSIZE bytes.
            if bytes.first() == Some(&b'=') {
                short.resize(60, 0);
            }
            info.source = Some(source);
            info.what = Some(what);
            info.defined = Some(defined);
            info.short_source = Some(short);
        }
        if tail || options.contains(&b'l') {
            info.current_line = Some(match (&code, frame) {
                (Some((_, p, data)), Some(i)) => {
                    let proto = &data.prototypes[*p];
                    proto.debug_line(
                        self.call.vm.debug_frames(record.thread)[i]
                            .pc()
                            .saturating_sub(1)
                            .min(proto.lines.len() - 1),
                    ) as i32
                }
                _ => -1,
            });
        }
        if tail || options.contains(&b'u') {
            info.upvalues = Some(match function {
                Value::Function(id) => self.call.vm.functions[slot_index(id.0)].upvalues.len(),
                Value::Native(id) => self.call.vm.debug_nups(id),
                _ => 0,
            });
        }
        if tail || options.contains(&b'n') {
            let name = frame.and_then(|i| {
                let vm = &self.call.vm;
                vm.debug_name(record.thread, i).map(|(_, kind)| {
                    let caller = vm.debug_frames(record.thread)[i - 1];
                    let FrameKind::Lua(id) = caller.function else {
                        unreachable!("debug name has Lua caller")
                    };
                    let c = &vm.functions[slot_index(id.0)];
                    let data = Arc::clone(&vm.programs[c.program].data);
                    (
                        c.program,
                        c.prototype,
                        caller.pc.saturating_sub(1),
                        data,
                        kind,
                    )
                })
            });
            if let Some((p, proto, pc, data, kind)) = name {
                let calls = &data.prototypes[proto].debug.calls;
                let call = &calls[calls.binary_search_by_key(&pc, |c| c.pc).unwrap()];
                info.name = Some(Some(self.call.vm.native_debug_string(p, &call.name)));
                info.name_what = Some(match kind {
                    b"global" => b"global\0",
                    b"local" => b"local\0",
                    b"field" => b"field\0",
                    b"method" => b"method\0",
                    b"upvalue" => b"upvalue\0",
                    _ => b"\0",
                });
            } else {
                info.name = Some(tail.then_some(NativeDebugString::Static(b"\0")));
                info.name_what = Some(b"\0");
            }
        }
        if options.contains(&b'f') {
            self.debug_push(record.thread, function)?;
        }
        if options.contains(&b'L') {
            let lines = if let Some((_, p, data)) = &code {
                let table = self.call.vm.new_table();
                for &line in &data.prototypes[*p].debug.active_lines {
                    self.call.vm.raw_set(
                        table,
                        Value::Number(f64::from(line)),
                        Value::Boolean(true),
                    )?;
                }
                Value::Table(table)
            } else {
                Value::Nil
            };
            self.debug_push(record.thread, lines)?;
        }
        Ok(info)
    }
}
