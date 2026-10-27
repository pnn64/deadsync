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
        let range = self
            .call
            .run_action(Action::Compare(op, [left, right]), false)?;
        let Value::Boolean(result) = self.call.vm.registers[range.start] else {
            unreachable!("comparison result")
        };
        self.call.vm.registers.truncate(self.base + self.top());
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
