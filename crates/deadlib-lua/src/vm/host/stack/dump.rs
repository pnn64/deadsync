//! Native writer callbacks share the Lua 5.1 serializer's exact block plan.
use super::*;

impl NativeStack<'_, '_> {
    /// Serialize the Lua function on top without changing this stack.
    /// A non-Lua function/value returns 1 without invoking the writer. Writer
    /// statuses are returned unchanged; the first nonzero status stops callbacks.
    /// Blocks follow Lua 5.1 field/vector/string boundaries, including empty
    /// debug vectors. Their bytes are borrowed only during each callback.
    /// Writers use this same native window and may reenter Lua or collect.
    ///
    /// # Errors
    /// Rejects pending unprotected errors and unsupported export limits.
    /// Writer errors and instruction limits escape unprotected and stay pending,
    /// even if the caller ignores the returned Result.
    pub fn dump(
        &mut self,
        mut writer: impl FnMut(&mut Self, &[u8]) -> Result<i32, Error>,
    ) -> Result<i32, Error> {
        self.check_error()?;
        let Some(Value::Function(function)) = self.value(-1) else {
            return Ok(1);
        };
        let closure = &self.call.vm.functions[slot_index(function.0)];
        let (bytes, ends) = crate::chunk51::dump_blocks(
            &self.call.vm.programs[closure.program].data,
            closure.prototype,
        )?;
        let mut start = 0;
        for end in ends {
            let result = writer(self, &bytes[start..end]);
            if let Some(error) = self
                .call
                .abort
                .clone()
                .or_else(|| result.as_ref().err().cloned())
            {
                self.call.abort = Some(error.clone());
                return Err(error);
            }
            let status = result.expect("writer outcome without error");
            if status != 0 {
                return Ok(status);
            }
            start = end;
        }
        Ok(0)
    }
}
