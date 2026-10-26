//! Mutable native argument/temporary storage in the VM register window.
use super::*;

/// A rooted mutable stack for one synchronous native callback.
///
/// Positive indexes start at one; negative indexes count back from the top.
/// This window is separate from closure captures. Nested calls preserve the
/// live prefix and use the host API's protected, non-yielding call boundary.
/// Call [`Self::finish`] to select the last values as callback results. Dropping
/// the stack without finishing selects zero results; the window remains
/// available to fault/return inspectors until the native frame exits.
pub struct NativeStack<'call, 'vm> {
    call: &'call mut HostCall<'vm>,
    base: usize,
    frame: usize,
    finished: bool,
}

impl<'vm> HostCall<'vm> {
    /// Open a mutable native stack initialized with the callback's arguments.
    ///
    /// # Errors
    /// Rejects an existing result writer/stack or more than 8,000 arguments.
    pub fn native_stack(&mut self) -> Result<NativeStack<'_, 'vm>, Error> {
        let frame = self.vm.active_native.expect("active host callback");
        if self.stack_opened || self.results != 0 || self.output != self.vm.frames[frame].base {
            return Err(self.error(b"native stack must precede result writes"));
        }
        if self.count > limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        let base = self.output;
        self.vm
            .resize_registers((base + self.count).max(self.vm.registers.len()));
        self.vm
            .registers
            .copy_within(self.arguments..self.arguments + self.count, base);
        // Call arguments have moved into this window. Clear their old owned
        // temporaries so suspended-call maps do not retain popped arguments.
        let start = self.arguments.min(base);
        let end = (self.arguments + self.count).min(base);
        self.vm.registers[start..end].fill(Value::Nil);
        self.vm.registers.truncate(base + self.count);
        self.results = self.count;
        self.stack_opened = true;
        let mut stack = NativeStack {
            call: self,
            base,
            frame,
            finished: false,
        };
        stack.sync();
        Ok(stack)
    }
}

impl NativeStack<'_, '_> {
    fn sync(&mut self) {
        self.call.vm.frames[self.frame].vararg_start = self.base;
        self.call.vm.frames[self.frame].vararg_count = self.call.results as u32;
    }
    fn position(&self, index: isize) -> Option<usize> {
        let position = if index > 0 {
            index - 1
        } else {
            (self.top() as isize).checked_add(index)?
        };
        (position >= 0 && (position as usize) < self.top()).then(|| self.base + position as usize)
    }
    /// Number of currently live stack values.
    pub fn top(&self) -> usize {
        self.call.results
    }
    /// Read a stack value; zero and out-of-range indexes return None.
    pub fn value(&self, index: isize) -> Option<Value> {
        self.position(index).map(|p| self.call.vm.registers[p])
    }
    /// Reserve extra slots without changing the visible top.
    /// Returns false for allocation failure or the 8,000-slot limit.
    pub fn check_stack(&mut self, extra: usize) -> bool {
        let Some(top) = self
            .top()
            .checked_add(extra)
            .filter(|&top| top <= limits::MAX_CSTACK)
        else {
            return false;
        };
        let before = self.call.vm.registers.capacity();
        let needed = (self.base + top).saturating_sub(self.call.vm.registers.len());
        if self.call.vm.registers.try_reserve(needed).is_err() {
            return false;
        }
        self.call.vm.gc_note_control(
            (self.call.vm.registers.capacity() - before) * std::mem::size_of::<Value>(),
        );
        true
    }
    /// Resize the stack, filling new slots with nil and releasing popped roots.
    ///
    /// # Errors
    /// Returns a runtime error above the 8,000-slot bound.
    pub fn set_top(&mut self, top: usize) -> Result<(), Error> {
        if top > limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        self.call.vm.resize_registers(self.base + top);
        self.call.results = top;
        self.sync();
        Ok(())
    }
    /// Push a live value, retaining it across nested calls and collection.
    ///
    /// # Errors
    /// Rejects collected/invalid handles or a full stack.
    pub fn push(&mut self, value: Value) -> Result<(), Error> {
        self.call.vm.validate_value(value)?;
        let top = self.top();
        self.set_top(top + 1)?;
        self.call.vm.registers[self.base + top] = value;
        self.call.vm.gc_new_value(value);
        Ok(())
    }
    /// Replace a value without changing the top.
    ///
    /// # Errors
    /// Rejects an invalid stack index or collected/invalid value.
    pub fn set(&mut self, index: isize, value: Value) -> Result<(), Error> {
        self.call.vm.validate_value(value)?;
        let position = self
            .position(index)
            .ok_or_else(|| self.error(b"invalid native stack index"))?;
        self.call.vm.registers[position] = value;
        self.call.vm.gc_new_value(value);
        Ok(())
    }
    /// Read a number, accepting numeric strings in any live stack slot.
    ///
    /// # Errors
    /// Returns a native argument/type error for an absent or nonnumeric slot.
    pub fn number(&self, index: isize) -> Result<f64, Error> {
        let relative = self.position(index).map_or(self.top(), |p| p - self.base);
        self.value(index)
            .and_then(|v| self.call.vm.number(v))
            .ok_or_else(|| {
                self.call
                    .vm
                    .argument_type_error(self.base - 1, self.top(), relative, "number")
            })
    }
    /// Protected call of the function below the last `arguments` stack values.
    /// Results replace the function and arguments; None keeps all results.
    ///
    /// # Errors
    /// Preserves Lua error identity and the host call's sticky resource limits.
    /// The faulting child frames are unwound, as with [`HostCall::call`].
    pub fn call(&mut self, arguments: usize, results: Option<usize>) -> Result<usize, Error> {
        let prefix = self
            .top()
            .checked_sub(
                arguments
                    .checked_add(1)
                    .ok_or_else(|| self.error(b"invalid native call count"))?,
            )
            .ok_or_else(|| self.error(b"native call stack underflow"))?;
        if results.is_some_and(|n| n > limits::MAX_CSTACK - prefix) {
            return Err(self.error(b"native stack overflow"));
        }
        let function = self.call.vm.registers[self.base + prefix];
        let values =
            self.call.vm.registers[self.base + prefix + 1..self.base + self.top()].to_vec();
        self.call.results = prefix;
        self.sync();
        let outcome = self.call.call_range(function, &values);
        match outcome {
            Ok(range) => {
                let count = results.unwrap_or(range.len());
                if count > limits::MAX_CSTACK - prefix {
                    self.call.vm.registers.truncate(self.base + prefix);
                    return Err(self.error(b"native stack overflow"));
                }
                self.call.vm.resize_registers(self.base + prefix + count);
                self.call.results = prefix + count;
                self.sync();
                Ok(count)
            }
            Err(error) => {
                self.call.results = self.call.vm.registers.len() - self.base;
                self.sync();
                Err(error)
            }
        }
    }
    /// Select the last `count` values as callback results, preserving the full
    /// window for return-hook inspection and mutation.
    ///
    /// # Errors
    /// Rejects a result count greater than the current top.
    pub fn finish(mut self, count: usize) -> Result<(), Error> {
        if count > self.top() {
            return Err(self.error(b"invalid native result count"));
        }
        self.call.output = self.base + self.top() - count;
        self.call.results = count;
        self.finished = true;
        Ok(())
    }
    /// Intern byte strings in this VM; push/store the result before collection.
    pub fn intern(&mut self, bytes: &[u8]) -> Value {
        self.call.intern(bytes)
    }
    /// Construct a native runtime error with exact bytes.
    pub fn error(&self, message: &[u8]) -> Error {
        self.call.error(message)
    }
    /// Construct an error retaining a Lua value's identity.
    pub fn raise(&self, value: Value) -> Error {
        self.call.raise(value)
    }
}
impl Drop for NativeStack<'_, '_> {
    fn drop(&mut self) {
        if !self.finished {
            self.call.output = self.base + self.top();
            self.call.results = 0;
        }
    }
}
