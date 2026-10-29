//! Lua 5.1 native scalar conversions, comparisons and concatenation.
use super::*;

impl NativeStack<'_, '_> {
    /// Read a number or numeric string without changing its original value.
    /// Returns None for absent or nonnumeric values, including booleans.
    pub fn to_number(&self, index: isize) -> Option<f64> {
        self.value(index)
            .and_then(|value| self.call.vm.number(value))
    }
    /// Truncate a number/numeric string to a pointer-width signed integer.
    /// Invalid types return None. Out-of-range numbers and NaNs yield the
    /// minimum integer, matching the Windows/MSVC Lua 5.1 reference.
    pub fn to_integer(&self, index: isize) -> Option<isize> {
        self.to_number(index)
            .map(|number| lua_integer(number) as isize)
    }
    /// Lua truth conversion: absent values, nil and false are false.
    pub fn to_boolean(&self, index: isize) -> bool {
        self.value(index).is_some_and(Value::is_truthy)
    }
    /// Push an integer converted to Lua's binary64 number representation.
    ///
    /// # Errors
    /// Rejects a pending unprotected error or a full stack.
    pub fn push_integer(&mut self, number: isize) -> Result<(), Error> {
        self.push(Value::Number(number as f64))
    }
    /// Compare two indexed values using Lua equality and shared __eq handlers.
    /// An absent index returns false. The native stack is unchanged.
    ///
    /// # Errors
    /// Propagates metamethod failures without unwinding their fault frames.
    pub fn equal(&mut self, left: isize, right: isize) -> Result<bool, Error> {
        self.compare_values(Op::Eq, left, right)
    }
    /// Compare two indexed values using Lua less-than semantics and __lt.
    /// An absent index returns false. Numeric strings are not coerced.
    ///
    /// # Errors
    /// Propagates type/metamethod failures, preserving their fault frames.
    pub fn less_than(&mut self, left: isize, right: isize) -> Result<bool, Error> {
        self.compare_values(Op::Lt, left, right)
    }
    fn compare_values(&mut self, op: Op, left: isize, right: isize) -> Result<bool, Error> {
        self.check_error()?;
        let (Some(left), Some(right)) = (self.value(left), self.value(right)) else {
            return Ok(false);
        };
        let destination = self.end();
        let TagResult {
            range,
            called,
            yielded,
        } = self.run_tag_action(Action::Compare(op, [left, right]))?;
        let result = self.call.vm.registers[range.start].is_truthy();
        if let Some(yielded) = yielded {
            let value = self.call.vm.registers[range.start];
            self.call.vm.registers[destination] = value;
            self.call.vm.gc_new_value(value);
            self.base = yielded.start;
            self.set_window_end(yielded.end - 1);
            self.call.vm.top = yielded.end - 1;
            return Ok(result);
        }
        if called {
            self.restore_window_call_base();
        }
        self.call.vm.truncate_registers(self.base + self.top());
        Ok(result)
    }
    /// Replace the last `count` values with Lua concatenation of those values.
    /// Zero pushes an empty string; one leaves the value untouched. Coercions,
    /// reductions and callbacks use the original native operand window. Its
    /// visible top and roots remain intact until concatenation succeeds.
    ///
    /// # Errors
    /// Rejects a missing operand, full stack for count zero, or a pending error.
    /// Type/metamethod failures preserve partially reduced operands and frames.
    pub fn concatenate(&mut self, count: usize) -> Result<(), Error> {
        self.check_error()?;
        let prefix = self
            .top()
            .checked_sub(count)
            .ok_or_else(|| self.error(b"native concat stack underflow"))?;
        if count == 0 {
            let empty = self.intern(b"");
            return self.push(empty);
        }
        if count == 1 {
            return Ok(());
        }
        if matches!(
            self.call.vm.frames[self.frame].function,
            FrameKind::Native(CONT_NATIVE_WINDOW)
        ) {
            return self.concatenate_window(count);
        }
        let range = self.call.run_action(
            Action::Concat {
                first: self.base + prefix,
                last: self.base + self.top() - 1,
            },
            false,
        )?;
        let value = self.call.vm.registers[range.start];
        self.set_top(prefix + 1)?;
        self.set(-1, value)
    }
    fn concatenate_window(&mut self, count: usize) -> Result<(), Error> {
        let mut end = self.end();
        let mut total = count;
        let mut last = self.top() - 1;
        while total > 1 {
            let right = self
                .base
                .checked_add(last)
                .ok_or_else(|| self.error(b"invalid native concatenation window"))?;
            self.call
                .vm
                .resize_registers(self.call.vm.registers.len().max(right + 1));
            let primitive = |v| matches!(v, Value::String(_) | Value::Number(_));
            let mut n = 2;
            if primitive(self.call.vm.registers[right - 1])
                && primitive(self.call.vm.registers[right])
            {
                let empty = matches!(self.call.vm.registers[right], Value::String(id)
                    if self.call.vm.string_bytes(id).unwrap().is_empty());
                if !empty {
                    n = 1;
                    while n < total && primitive(self.call.vm.registers[right - n]) {
                        n += 1;
                    }
                }
                let first = right + 1 - n;
                self.call.vm.concat_range(first, first, right)?;
            } else {
                let values = [
                    self.call.vm.registers[right - 1],
                    self.call.vm.registers[right],
                ];
                let thread = self.thread_id();
                let depth = self.call.vm.frames.len();
                let barriers = self.call.vm.frames.last().map_or(0, |f| f.barriers);
                self.call.vm.api_calls.push(ApiCall {
                    thread,
                    frame: depth,
                    barriers: barriers + 1,
                    status: NativeCallStatus::Ok,
                    yielded: None,
                });
                let bias = self.call.vm.threads[slot_index(thread.0)].native_bias;
                self.call
                    .vm
                    .set_thread_native_bias(thread, bias.wrapping_add(1));
                let outcome = self.call.run_action(Action::ConcatPair(values), false);
                let scope = self
                    .call
                    .vm
                    .api_calls
                    .pop()
                    .expect("concatenation API scope");
                if outcome.is_ok() {
                    let bias = self.call.vm.threads[slot_index(thread.0)].native_bias;
                    self.call
                        .vm
                        .set_thread_native_bias(thread, bias.wrapping_sub(1));
                }
                let range = outcome?;
                let value = self.call.vm.registers[range.start];
                self.call.vm.registers[right - 1] = value;
                self.call.vm.gc_new_value(value);
                if let Some(yielded) = scope.yielded {
                    end = yielded.end - 1;
                    self.base = yielded.start;
                    self.set_window_end(end);
                    self.call.vm.top = end;
                } else {
                    self.call.vm.truncate_registers(end);
                    self.restore_window_call_base();
                }
            }
            // Lua 5.1 keeps these loop offsets relative even when a tag-method
            // return changes L->base. The physical top is reduced only at exit.
            total -= n - 1;
            last -= n - 1;
        }
        let top = end - (count - 1);
        if !self.call.vm.frames[self.frame + 1..]
            .iter()
            .any(|frame| frame.pc == host::API_YIELDED)
        {
            self.call.vm.truncate_registers(top);
        }
        self.set_window_end(top);
        self.call.vm.top = top;
        Ok(())
    }
    /// Concatenate only string/number operands without entering the scheduler
    /// or checking the collector, matching Lua 5.1's formatted-message builder.
    /// Zero and one operands have the same stack effects as `concatenate`.
    ///
    /// # Errors
    /// Rejects pending errors, bad counts, nonprimitive operands or a full
    /// stack for count zero. Type rejection leaves all operands unchanged.
    pub fn concatenate_primitives(&mut self, count: usize) -> Result<(), Error> {
        self.check_error()?;
        let prefix = self
            .top()
            .checked_sub(count)
            .ok_or_else(|| self.error(b"native concat stack underflow"))?;
        if self.call.vm.registers[self.base + prefix..self.base + self.top()]
            .iter()
            .any(|value| !matches!(value, Value::String(_) | Value::Number(_)))
        {
            return Err(self.error(b"native formatted parts must be strings or numbers"));
        }
        if count < 2 {
            return self.concatenate(count);
        }
        self.call.vm.concat_range(
            self.base + prefix,
            self.base + prefix,
            self.base + self.top() - 1,
        )?;
        self.set_top(prefix + 1)
    }
}
