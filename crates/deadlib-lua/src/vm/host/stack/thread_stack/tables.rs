//! Raw table access on another thread, without activating it or running Lua.
use super::*;

impl NativeThreadStack<'_> {
    fn indexed_value(&self, index: isize) -> Result<Value, Error> {
        self.value(index)
            .ok_or_else(|| self.error(b"invalid native stack index"))
    }
    /// Create and push a table with array and record capacity hints.
    ///
    /// # Errors
    /// Rejects allocation failure, table overflow or a full stack.
    pub fn create_table(&mut self, array: usize, records: usize) -> Result<(), Error> {
        if self.top == limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        self.reserve_end(self.end + 1)?;
        let table = Value::Table(self.vm.try_table_with_capacity(array, records)?);
        self.vm.gc.pending = true;
        self.push(table)
    }
    /// Replace the top key with a raw table lookup, bypassing metamethods.
    ///
    /// # Errors
    /// Rejects a non-table target, absent index or missing key.
    pub fn raw_get(&mut self, index: isize) -> Result<(), Error> {
        let target = self.indexed_value(index)?;
        let key = self.indexed_value(-1)?;
        let value = self.vm.table(target)?.get(key);
        self.set(-1, value)
    }
    /// Pop the top key and value after raw assignment, bypassing metamethods.
    ///
    /// # Errors
    /// Rejects a non-table target, invalid key, absent input or allocation failure.
    /// Failed writes retain both inputs.
    pub fn raw_set(&mut self, index: isize) -> Result<(), Error> {
        let Value::Table(table) = self.indexed_value(index)? else {
            return Err(self.error(b"raw native access requires a table"));
        };
        let key = self.indexed_value(-2)?;
        let value = self.indexed_value(-1)?;
        self.vm.raw_set(table, key, value)?;
        self.set_top(self.top - 2)
    }
    /// Push a raw integer-key lookup, bypassing metamethods.
    ///
    /// # Errors
    /// Rejects a non-table target, absent index or full stack.
    pub fn raw_get_integer(&mut self, index: isize, key: i32) -> Result<(), Error> {
        let target = self.indexed_value(index)?;
        let value = self.vm.table(target)?.get(Value::Number(f64::from(key)));
        self.push(value)
    }
    /// Pop the top value after raw integer-key assignment.
    ///
    /// # Errors
    /// Rejects a non-table target or absent input.
    pub fn raw_set_integer(&mut self, index: isize, key: i32) -> Result<(), Error> {
        let Value::Table(table) = self.indexed_value(index)? else {
            return Err(self.error(b"raw native access requires a table"));
        };
        let value = self.indexed_value(-1)?;
        self.vm
            .raw_set(table, Value::Number(f64::from(key)), value)?;
        self.set_top(self.top - 1)
    }
    /// Push the actual metatable; false leaves the stack unchanged.
    ///
    /// # Errors
    /// Rejects an absent index or full stack.
    pub fn get_metatable(&mut self, index: isize) -> Result<bool, Error> {
        let value = self.indexed_value(index)?;
        let Some(table) = self.vm.metatable(value) else {
            return Ok(false);
        };
        self.push(Value::Table(table))?;
        Ok(true)
    }
    /// Pop a table or nil and set the actual metatable, bypassing protection.
    ///
    /// # Errors
    /// Rejects an absent index or a value other than a table or nil.
    pub fn set_metatable(&mut self, index: isize) -> Result<(), Error> {
        let value = self.indexed_value(index)?;
        let table = match self.indexed_value(-1)? {
            Value::Nil => None,
            Value::Table(table) => Some(table),
            _ => return Err(self.error(b"native metatable must be a table or nil")),
        };
        self.vm.set_metatable(value, table);
        self.set_top(self.top - 1)
    }
    /// Replace a traversal key with the next key/value pair, or pop it at the end.
    ///
    /// # Errors
    /// Rejects a non-table target, invalid traversal key or full stack.
    pub fn next_table(&mut self, index: isize) -> Result<bool, Error> {
        let target = self.indexed_value(index)?;
        let key = self.indexed_value(-1)?;
        let next = self
            .vm
            .table(target)?
            .next(key)
            .map_err(|error| self.vm.operand_error(error))?;
        if let Some((key, value)) = next {
            if self.top == limits::MAX_CSTACK {
                return Err(self.error(b"native stack overflow"));
            }
            self.set(-1, key)?;
            self.push(value)?;
            Ok(true)
        } else {
            self.set_top(self.top - 1)?;
            Ok(false)
        }
    }
}
