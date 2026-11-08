//! Indexed inspection and mutation of Lua and native closure captures.
use super::*;

#[derive(Clone, Copy)]
enum Capture {
    Lua { cell: usize, name: StringId },
    Native { function: u64, index: usize },
}

impl Vm {
    fn native_capture(&self, function: Value, number: usize) -> Option<Capture> {
        let index = number.checked_sub(1)?;
        match function {
            Value::Function(id) => {
                let closure = &self.functions[slot_index(id.0)];
                let name =
                    *self.programs[closure.program].upvalue_names[closure.prototype].get(index)?;
                Some(Capture::Lua {
                    cell: *closure.upvalues.get(index)? as usize,
                    name,
                })
            }
            Value::Native(id) if id >= strings::ITERATOR_FIRST => {
                self.native_closures.get(id)?.captures.get(index)?;
                Some(Capture::Native {
                    function: id,
                    index,
                })
            }
            _ => None,
        }
    }
    fn native_capture_name(&self, capture: Capture) -> &[u8] {
        match capture {
            Capture::Lua { name, .. } => self.strings.bytes[slot_index(name.0)].c_bytes(),
            Capture::Native { .. } => b"\0",
        }
    }
    fn native_capture_value(&self, capture: Capture) -> Value {
        match capture {
            Capture::Lua { cell, .. } => self.cell_value(cell),
            Capture::Native { function, index } => {
                self.native_closures[slot_index(function)].captures[index]
            }
        }
    }
    fn write_native_capture(&mut self, capture: Capture, value: Value) -> Result<(), Error> {
        match capture {
            Capture::Lua { cell, .. } => {
                self.set_cell(cell, value);
                Ok(())
            }
            Capture::Native { function, index } => self.set_native_capture(function, index, value),
        }
    }
}

impl NativeStack<'_, '_> {
    /// Push an indexed closure's capture and borrow its NUL-terminated name.
    /// Capture numbers start at one. Native captures have an empty name.
    /// Absent captures and nonfunctions return None without changing the top.
    /// Imported Lua captures without retained debug names are absent here.
    /// Name storage stays stable while the function remains reachable, including
    /// across collection and nested calls; embedded NUL bytes are retained.
    ///
    /// # Errors
    /// Rejects a pending unprotected error or a full stack for a valid capture.
    pub fn get_upvalue(&mut self, function: isize, number: usize) -> Result<Option<&[u8]>, Error> {
        self.check_error()?;
        let Some(capture) = self
            .value(function)
            .and_then(|f| self.call.vm.native_capture(f, number))
        else {
            return Ok(None);
        };
        let value = self.call.vm.native_capture_value(capture);
        self.push(value)?;
        Ok(Some(self.call.vm.native_capture_name(capture)))
    }
    /// Pop the top value into a closure capture and borrow its NUL-terminated name.
    /// Invalid captures/nonfunctions return None and leave the top unchanged.
    /// Lua cells may belong to the current or a suspended thread; native writes
    /// change the same slots used by builtin execution and collector tracing.
    /// Name storage has the same lifetime as in [`Self::get_upvalue`].
    ///
    /// # Errors
    /// Rejects a pending unprotected error or an empty value stack.
    pub fn set_upvalue(&mut self, function: isize, number: usize) -> Result<Option<&[u8]>, Error> {
        self.check_error()?;
        let value = self
            .value(-1)
            .ok_or_else(|| self.error(b"native upvalue stack underflow"))?;
        let Some(capture) = self
            .value(function)
            .and_then(|f| self.call.vm.native_capture(f, number))
        else {
            return Ok(None);
        };
        self.call.vm.write_native_capture(capture, value)?;
        self.set_top(self.top() - 1)?;
        Ok(Some(self.call.vm.native_capture_name(capture)))
    }
}

impl NativeThreadStack<'_> {
    /// Push an indexed closure's capture and borrow its NUL-terminated name.
    /// Invalid captures leave the top unchanged. This does not activate the thread.
    ///
    /// # Errors
    /// Rejects a full stack when a capture exists.
    pub fn get_upvalue(&mut self, function: isize, number: usize) -> Result<Option<&[u8]>, Error> {
        let Some(capture) = self
            .value(function)
            .and_then(|f| self.vm.native_capture(f, number))
        else {
            return Ok(None);
        };
        let value = self.vm.native_capture_value(capture);
        self.push(value)?;
        Ok(Some(self.vm.native_capture_name(capture)))
    }
    /// Pop the top value into an indexed closure's capture and borrow its name.
    /// Invalid captures leave the top unchanged. Lua cells may be open on any thread.
    ///
    /// # Errors
    /// Rejects an empty stack for the replacement value.
    pub fn set_upvalue(&mut self, function: isize, number: usize) -> Result<Option<&[u8]>, Error> {
        let value = self
            .value(-1)
            .ok_or_else(|| self.error(b"native upvalue stack underflow"))?;
        let Some(capture) = self
            .value(function)
            .and_then(|f| self.vm.native_capture(f, number))
        else {
            return Ok(None);
        };
        self.vm.write_native_capture(capture, value)?;
        self.set_top(self.top() - 1)?;
        Ok(Some(self.vm.native_capture_name(capture)))
    }
}
