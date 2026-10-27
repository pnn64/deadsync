//! Indexed inspection and mutation of Lua and native closure captures.
use super::*;

#[derive(Clone, Copy)]
enum Capture {
    Lua { cell: usize, name: StringId },
    Native { function: u64, index: usize },
}

impl NativeStack<'_, '_> {
    fn capture(&self, function: isize, number: usize) -> Option<Capture> {
        let index = number.checked_sub(1)?;
        match self.value(function)? {
            Value::Function(id) => {
                let closure = &self.call.vm.functions[slot_index(id.0)];
                let name = *self.call.vm.programs[closure.program].upvalue_names[closure.prototype]
                    .get(index)?;
                Some(Capture::Lua {
                    cell: *closure.upvalues.get(index)? as usize,
                    name,
                })
            }
            Value::Native(id) if id >= strings::ITERATOR_FIRST => {
                self.call.vm.native_closures.get(id)?.captures.get(index)?;
                Some(Capture::Native {
                    function: id,
                    index,
                })
            }
            _ => None,
        }
    }
    fn capture_name(&self, capture: Capture) -> &[u8] {
        match capture {
            Capture::Lua { name, .. } => self.call.vm.strings.bytes[slot_index(name.0)].c_bytes(),
            Capture::Native { .. } => b"\0",
        }
    }
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
        let Some(capture) = self.capture(function, number) else {
            return Ok(None);
        };
        let value = match capture {
            Capture::Lua { cell, .. } => self.call.vm.cell_value(cell),
            Capture::Native { function, index } => {
                self.call.vm.native_closures[slot_index(function)].captures[index]
            }
        };
        self.push(value)?;
        Ok(Some(self.capture_name(capture)))
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
        let Some(capture) = self.capture(function, number) else {
            return Ok(None);
        };
        match capture {
            Capture::Lua { cell, .. } => self.call.vm.set_cell(cell, value),
            Capture::Native { function, index } => {
                self.call.vm.set_native_capture(function, index, value)?;
            }
        }
        self.set_top(self.top() - 1)?;
        Ok(Some(self.capture_name(capture)))
    }
}
