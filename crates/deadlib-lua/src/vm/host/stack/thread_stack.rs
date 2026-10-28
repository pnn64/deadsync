//! Non-executing access to another thread's public native stack.
use super::*;
mod calls;
mod execution;
mod loader;
mod objects;
#[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
mod panic;
mod tables;

/// A borrowed native stack on an inactive thread in the same VM.
/// Operations do not activate the thread, run Lua or change its execution status.
/// Values on the visible stack are collector roots while the thread is live.
pub struct NativeThreadStack<'vm> {
    pub(super) vm: &'vm mut Vm,
    thread: ThreadId,
    base: usize,
    top: usize,
    end: usize,
    frame: Option<usize>,
}
impl NativeStack<'_, '_> {
    /// Create an empty thread sharing this VM's heap, registry and current globals.
    /// Inherited hooks follow the ordinary coroutine creation rules. The new
    /// thread is pushed on this stack and has native status Ok and no values.
    ///
    /// # Errors
    /// Propagates pending errors, collector finalizer errors or stack overflow.
    pub fn new_thread(&mut self) -> Result<ThreadId, Error> {
        self.check_error()?;
        if !self.check_stack(1) {
            return Err(self.error(b"native stack overflow"));
        }
        self.check_gc()?;
        let thread = self.call.vm.allocate_thread();
        self.push(Value::Thread(thread))?;
        Ok(thread)
    }
    /// Borrow another live thread's native stack without executing it.
    ///
    /// # Errors
    /// Rejects pending errors, the active thread and collected/invalid identities.
    pub fn thread_stack(&mut self, thread: ThreadId) -> Result<NativeThreadStack<'_>, Error> {
        self.check_error()?;
        self.call.vm.validate_value(Value::Thread(thread))?;
        if thread == self.call.vm.thread {
            return Err(self.error(b"thread stack is already active"));
        }
        let state = &self.call.vm.threads[slot_index(thread.0)];
        let frame = (state.status == threads::Status::Normal).then(|| state.frames.len() - 1);
        let range = frame.map_or_else(
            || state.api_stack.clone(),
            |i| {
                let f = state.frames[i];
                f.vararg_start..f.vararg_start + f.argc()
            },
        );
        Ok(NativeThreadStack {
            vm: self.call.vm,
            thread,
            base: range.start,
            top: range.len(),
            end: range.end,
            frame,
        })
    }
    /// Move the last count values between two native stacks in this VM.
    /// Order and identities are preserved, with no metamethods or Lua calls.
    /// Moving to the same thread is a no-op, including a count beyond its top.
    ///
    /// # Errors
    /// Rejects pending errors, stale threads, insufficient source values or a
    /// destination above the native stack bound. Validation precedes mutation.
    pub fn move_thread_values(
        &mut self,
        from: ThreadId,
        to: ThreadId,
        count: usize,
    ) -> Result<(), Error> {
        self.check_error()?;
        self.call.vm.validate_value(Value::Thread(from))?;
        self.call.vm.validate_value(Value::Thread(to))?;
        if from == to {
            return Ok(());
        }
        let source = if from == self.thread_id() {
            self.top()
        } else {
            self.thread_stack(from)?.top()
        };
        let Some(prefix) = source.checked_sub(count) else {
            return Err(self.error(b"native source stack underflow"));
        };
        let reserved = if to == self.thread_id() {
            self.check_stack(count)
        } else {
            self.thread_stack(to)?.check_stack(count)
        };
        if !reserved {
            return Err(self.error(b"native destination stack overflow"));
        }
        let values: Vec<_> = if from == self.thread_id() {
            (prefix + 1..=source)
                .map(|i| self.value(i as isize).unwrap())
                .collect()
        } else {
            let stack = self.thread_stack(from)?;
            (prefix + 1..=source)
                .map(|i| stack.value(i as isize).unwrap())
                .collect()
        };
        for &value in &values {
            self.call.vm.validate_value(value)?;
        }
        if to == self.thread_id() {
            for value in values {
                self.push(value)?;
            }
        } else {
            let mut stack = self.thread_stack(to)?;
            for value in values {
                stack.push(value)?;
            }
        }
        if from == self.thread_id() {
            self.set_top(prefix)?;
        } else {
            self.thread_stack(from)?.set_top(prefix)?;
        }
        Ok(())
    }
}
impl NativeThreadStack<'_> {
    /// Push this thread, reporting whether it is the VM's main thread.
    ///
    /// # Errors
    /// Rejects native stack overflow.
    pub fn push_thread(&mut self) -> Result<bool, Error> {
        self.push(Value::Thread(self.thread))?;
        Ok(self.thread == self.vm.main)
    }
    /// Number of values in this thread's public native window.
    pub fn top(&self) -> usize {
        self.top
    }
    /// Signed Lua 5.1 stack offset, including a top below a yielded base.
    pub fn top_offset(&self) -> isize {
        self.end as isize - self.base as isize
    }
    fn position(&self, index: isize) -> Option<usize> {
        let p = if index > 0 {
            index - 1
        } else {
            (self.top as isize).checked_add(index)?
        };
        (p >= 0 && (p as usize) < self.top).then(|| self.base + p as usize)
    }
    fn function(&self) -> Option<Value> {
        self.vm.calling_function(self.thread)
    }
    /// Read ordinary indexes or registry/global/environment/capture pseudo-indexes.
    pub fn value(&self, index: isize) -> Option<Value> {
        match index {
            -10000 => Some(Value::Table(self.vm.registry)),
            -10002 => Some(Value::Table(self.vm.thread_environment(self.thread))),
            -10001 => Some(Value::Table(self.vm.calling_environment(self.thread))),
            ..=-10003 => self
                .function()
                .and_then(|function| match function {
                    Value::Native(id) => self.vm.native_closures.get(id),
                    _ => None,
                })
                .and_then(|f| {
                    NativeStack::capture_index(index).and_then(|i| f.captures.get(i).copied())
                }),
            _ => self
                .position(index)
                .map(|p| self.vm.threads[slot_index(self.thread.0)].registers[p]),
        }
    }
    /// Reserve slots without changing the top; false indicates overflow/allocation failure.
    pub fn check_stack(&mut self, extra: usize) -> bool {
        let Some(end) = self
            .end
            .checked_add(extra)
            .filter(|_| extra <= limits::MAX_CSTACK)
        else {
            return false;
        };
        self.reserve_end(end)
    }
    fn reserve_end(&mut self, end: usize) -> bool {
        if end > self.base + limits::MAX_CSTACK {
            return false;
        }
        let state = &mut self.vm.threads[slot_index(self.thread.0)];
        let before = state.registers.capacity();
        if state
            .registers
            .try_reserve(end.saturating_sub(state.registers.len()))
            .is_err()
        {
            return false;
        }
        let growth = (state.registers.capacity() - before) * size_of::<Value>();
        self.vm.threads.note_alloc(growth);
        true
    }
    /// Resize this native window, filling newly exposed values with nil.
    ///
    /// # Errors
    /// Rejects sizes above 8,000 slots or allocation failure.
    pub fn set_top(&mut self, top: usize) -> Result<(), Error> {
        if top > limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        self.set_end(self.base + top)
    }
    fn set_end(&mut self, end: usize) -> Result<(), Error> {
        if end > self.end && !self.reserve_end(end) {
            return Err(self.error(b"native stack overflow"));
        }
        let state = &mut self.vm.threads[slot_index(self.thread.0)];
        if state.registers.len() < end {
            state.registers.resize(end, Value::Nil);
        }
        if end > self.end {
            state.registers[self.end..end].fill(Value::Nil);
        }
        state.top = end;
        state.api_stack = self.base..end;
        self.top = end.saturating_sub(self.base);
        if let Some(i) = self.frame {
            state.frames[i].vararg_count = self.top as u32;
        }
        self.end = end;
        Ok(())
    }
    /// Push a live value and keep its graph reachable from this thread.
    ///
    /// # Errors
    /// Rejects stale values, overflow or allocation failure.
    pub fn push(&mut self, value: Value) -> Result<(), Error> {
        self.vm.validate_value(value)?;
        let at = self.end;
        self.set_end(at + 1)?;
        self.vm.threads[slot_index(self.thread.0)].registers[at] = value;
        self.vm.gc_barrier(Value::Thread(self.thread), value);
        Ok(())
    }
    /// Replace an ordinary slot or supported pseudo-index, preserving the top.
    ///
    /// # Errors
    /// Rejects stale values, absent indexes, invalid pseudo-index assignments
    /// and an environment replacement when no function is active.
    pub fn set(&mut self, index: isize, value: Value) -> Result<(), Error> {
        self.vm.validate_value(value)?;
        if (-10002..=-10000).contains(&index) {
            let Value::Table(table) = value else {
                return Err(self.error(b"native pseudo-index requires a table"));
            };
            match index {
                -10000 => {
                    self.vm.registry = table;
                    self.vm.gc_new_value(value);
                }
                -10002 => self.vm.set_thread_environment(self.thread, table),
                _ => {
                    let function = self
                        .function()
                        .ok_or_else(|| self.error(b"no calling environment"))?;
                    self.vm.set_closure_environment(function, table);
                }
            }
            return Ok(());
        }
        if index <= -10003 {
            let Some(Value::Native(id)) = self.function() else {
                return Err(self.error(b"invalid native capture index"));
            };
            let i = NativeStack::capture_index(index)
                .ok_or_else(|| self.error(b"invalid native capture index"))?;
            return self.vm.set_native_capture(id, i, value);
        }
        let p = self
            .position(index)
            .ok_or_else(|| self.error(b"invalid native stack index"))?;
        self.vm.threads[slot_index(self.thread.0)].registers[p] = value;
        self.vm.gc_barrier(Value::Thread(self.thread), value);
        Ok(())
    }
    /// Move the top value to an ordinary index, shifting subsequent values up.
    ///
    /// # Errors
    /// Rejects absent or pseudo-indexes.
    pub fn insert(&mut self, index: isize) -> Result<(), Error> {
        let p = self
            .position(index)
            .ok_or_else(|| self.error(b"invalid native stack index"))?;
        let end = self.base + self.top - 1;
        let r = &mut self.vm.threads[slot_index(self.thread.0)].registers;
        let v = r[end];
        r.copy_within(p..end, p + 1);
        r[p] = v;
        Ok(())
    }
    /// Remove an ordinary slot and shift the following values down.
    ///
    /// # Errors
    /// Rejects absent or pseudo-indexes.
    pub fn remove(&mut self, index: isize) -> Result<(), Error> {
        let p = self
            .position(index)
            .ok_or_else(|| self.error(b"invalid native stack index"))?;
        self.vm.threads[slot_index(self.thread.0)]
            .registers
            .copy_within(p + 1..self.base + self.top, p);
        self.set_top(self.top - 1)
    }
    /// Convert numbers/numeric strings without changing the slot.
    pub fn to_number(&self, index: isize) -> Option<f64> {
        self.value(index).and_then(|v| self.vm.number(v))
    }
    /// Convert to the reference profile's pointer-width lua_Integer.
    pub fn to_integer(&self, index: isize) -> Option<isize> {
        self.to_number(index).map(|n| lua_integer(n) as isize)
    }
    /// Convert numbers to strings in place and borrow stable interned bytes.
    ///
    /// # Errors
    /// Propagates invalid pseudo-index replacements.
    pub fn to_bytes(&mut self, index: isize) -> Result<Option<&[u8]>, Error> {
        let mut value = self.value(index);
        if let Some(Value::Number(n)) = value {
            let string = self.vm.intern(&format_number(n));
            self.set(index, string)?;
            value = Some(string);
        }
        Ok(match value {
            Some(Value::String(id)) => self.vm.string_bytes(id),
            _ => None,
        })
    }
    /// Borrow a coerced byte string including its trailing zero byte.
    ///
    /// # Errors
    /// Propagates failed slot replacement.
    pub fn to_c_bytes(&mut self, index: isize) -> Result<Option<&[u8]>, Error> {
        self.to_bytes(index)?;
        Ok(match self.value(index) {
            Some(Value::String(id)) => Some(self.vm.strings.bytes[slot_index(id.0)].c_bytes()),
            _ => None,
        })
    }
    /// Raw C API length, converting numeric slots to strings in place.
    ///
    /// # Errors
    /// Propagates failed slot replacement.
    pub fn object_len(&mut self, index: isize) -> Result<usize, Error> {
        match self.value(index) {
            Some(Value::Table(id)) => Ok(self.vm.tables[slot_index(id.0)].len()),
            Some(Value::Userdata(id)) => Ok(self.vm.userdata[slot_index(id.0)].bytes.len()),
            _ => Ok(self.to_bytes(index)?.map_or(0, <[u8]>::len)),
        }
    }
    /// Read a full userdata's stable payload address, or a light userdata address.
    pub fn userdata_pointer(&mut self, index: isize) -> Option<usize> {
        match self.value(index) {
            Some(v @ (Value::Userdata(_) | Value::LightUserdata(_))) => self.vm.object_pointer(v),
            _ => None,
        }
    }
    /// Read a stable object pointer without retaining the value independently.
    pub fn to_pointer(&mut self, index: isize) -> Option<usize> {
        let value = self.value(index)?;
        self.vm.object_pointer(value)
    }
    /// Read an indexed thread identity.
    pub fn to_thread(&self, index: isize) -> Option<ThreadId> {
        match self.value(index) {
            Some(Value::Thread(id)) => Some(id),
            _ => None,
        }
    }
    /// Intern arbitrary bytes without pushing a slot.
    pub fn intern(&mut self, bytes: &[u8]) -> Value {
        self.vm.intern(bytes)
    }
    /// Build an error for a rejected non-executing stack operation.
    pub fn error(&self, bytes: &[u8]) -> Error {
        Error::from_bytes(ErrorKind::Runtime, 0, bytes.to_vec())
    }
}
