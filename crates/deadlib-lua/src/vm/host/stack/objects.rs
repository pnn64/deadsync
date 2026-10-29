//! Native userdata storage, environments and scalar-to-string conversion.
use super::*;

impl NativeStack<'_, '_> {
    /// Allocate and push zero-initialized, pointer/double-aligned userdata.
    /// The payload address remains stable until this userdata is collected.
    ///
    /// # Errors
    /// Rejects a pending error, full stack or payload allocation failure.
    pub fn new_userdata(&mut self, size: usize) -> Result<(), Error> {
        self.check_error()?;
        if self.top() == limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        self.call.vm.reserve_registers(self.end() + 1)?;
        let env = self.context_environment();
        let id = self.call.vm.try_allocate_userdata(size, env)?;
        self.push(Value::Userdata(id))
    }
    /// Borrow full userdata payload bytes; other types return None.
    pub fn userdata_bytes(&self, index: isize) -> Option<&[u8]> {
        let Value::Userdata(id) = self.value(index)? else {
            return None;
        };
        self.call.vm.userdata_bytes(id)
    }
    /// Borrow a full userdata payload for mutation; other types return None.
    ///
    /// # Errors
    /// Returns any pending unprotected error.
    pub fn userdata_bytes_mut(&mut self, index: isize) -> Result<Option<&mut [u8]>, Error> {
        self.check_error()?;
        let Some(Value::Userdata(id)) = self.value(index) else {
            return Ok(None);
        };
        Ok(self.call.vm.userdata_bytes_mut(id))
    }
    /// Push a function, userdata or thread environment; other values yield nil.
    ///
    /// # Errors
    /// Rejects a pending error or a full stack.
    pub fn get_environment(&mut self, index: isize) -> Result<(), Error> {
        self.check_error()?;
        let env = match self.value(index) {
            Some(Value::Function(id)) => Some(self.call.vm.functions[slot_index(id.0)].env),
            Some(Value::Native(id)) => Some(self.call.vm.native_environment(id)),
            Some(Value::Userdata(id)) => Some(self.call.vm.userdata[slot_index(id.0)].env),
            Some(Value::Thread(id)) => Some(self.call.vm.thread_environment(id)),
            _ => None,
        };
        self.push(env.map_or(Value::Nil, Value::Table))
    }
    /// Pop a table and replace a function, userdata or thread environment.
    /// Returns false for other target types, still consuming the table.
    ///
    /// # Errors
    /// Rejects pending errors, an invalid index or a top value that is not a table.
    pub fn set_environment(&mut self, index: isize) -> Result<bool, Error> {
        self.check_error()?;
        let target = self
            .value(index)
            .ok_or_else(|| self.error(b"invalid native stack index"))?;
        let Some(Value::Table(env)) = self.value(-1) else {
            return Err(self.error(b"native environment must be a table"));
        };
        let supported = match target {
            Value::Function(id) => {
                self.call.vm.functions[slot_index(id.0)].env = env;
                self.call.vm.gc_barrier(target, Value::Table(env));
                true
            }
            Value::Native(id) => {
                self.call.vm.set_native_environment(id, env);
                true
            }
            Value::Userdata(id) => {
                self.call.vm.userdata[slot_index(id.0)].env = env;
                self.call.vm.gc_barrier(target, Value::Table(env));
                true
            }
            Value::Thread(id) => {
                self.call.vm.set_thread_environment(id, env);
                true
            }
            _ => false,
        };
        self.set_top(self.top() - 1)?;
        Ok(supported)
    }
    /// Borrow a string's bytes, converting a number in its original slot.
    /// Other types, including absent slots, return None. Captures support conversion.
    ///
    /// # Errors
    /// Returns any pending error or a failed slot replacement.
    pub fn to_bytes(&mut self, index: isize) -> Result<Option<&[u8]>, Error> {
        let id = self.coerce_string(index)?;
        Ok(id.and_then(|id| self.call.vm.string_bytes(id)))
    }
    /// Borrow a string including its trailing NUL, converting numbers in place.
    /// Embedded NUL bytes remain part of the string. The logical length is one
    /// less than this slice's length. Storage is stable while the string is rooted.
    ///
    /// # Errors
    /// Returns any pending error or a failed slot replacement.
    pub fn to_c_bytes(&mut self, index: isize) -> Result<Option<&[u8]>, Error> {
        let id = self.coerce_string(index)?;
        Ok(id.map(|id| self.call.vm.strings.bytes[slot_index(id.0)].c_bytes()))
    }
    fn coerce_string(&mut self, index: isize) -> Result<Option<StringId>, Error> {
        self.check_error()?;
        let value = self.value(index);
        let id = match value {
            Some(Value::String(id)) => id,
            Some(Value::Number(number)) => {
                let value = self.intern(&native::format_number(number));
                self.set(index, value)?;
                let Value::String(id) = value else {
                    unreachable!()
                };
                id
            }
            _ => return Ok(None),
        };
        Ok(Some(id))
    }
    /// Lua 5.1 C API raw length. Numeric values convert to strings in place.
    ///
    /// # Errors
    /// Returns any pending error or a failed numeric conversion write.
    pub fn object_len(&mut self, index: isize) -> Result<usize, Error> {
        self.check_error()?;
        match self.value(index) {
            Some(Value::Table(id)) => Ok(self.call.vm.tables[slot_index(id.0)].len()),
            Some(Value::Userdata(_)) => Ok(self.userdata_bytes(index).unwrap().len()),
            _ => Ok(self.to_bytes(index)?.map_or(0, <[u8]>::len)),
        }
    }
}
