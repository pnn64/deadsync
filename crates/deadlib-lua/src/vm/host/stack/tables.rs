//! Native table operations share VM lookup, metamethod and collector paths.
use super::*;

impl NativeStack<'_, '_> {
    fn indexed_value(&self, index: isize) -> Result<Value, Error> {
        self.value(index)
            .ok_or_else(|| self.error(b"invalid native stack index"))
    }
    /// Create and push a table with array and record capacity hints.
    ///
    /// # Errors
    /// Rejects a pending error or full native stack.
    pub fn create_table(&mut self, array: usize, records: usize) -> Result<(), Error> {
        self.check_error()?;
        if self.top() == limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        let table = Table::with_capacity(array, records)?;
        self.call.vm.tables.note_alloc(table.heap_bytes());
        self.call.vm.gc.pending = true;
        let table = Value::Table(TableId(self.call.vm.tables.insert(table)));
        self.call.vm.gc_new_value(table);
        self.push(table)
    }
    fn read_table(
        &mut self,
        target: Value,
        key: Value,
        raw: bool,
    ) -> Result<(Value, Option<std::ops::Range<usize>>), Error> {
        self.check_error()?;
        if raw {
            return Ok((self.call.vm.table(target)?.get(key), None));
        }
        let TagResult {
            range,
            called,
            yielded,
        } = self.run_tag_action(Action::Get([target, key]))?;
        let value = self.call.vm.registers[range.start];
        if called {
            self.restore_window_call_base();
        }
        Ok((value, yielded))
    }
    fn write_table(
        &mut self,
        target: Value,
        key: Value,
        value: Value,
        raw: bool,
    ) -> Result<Option<std::ops::Range<usize>>, Error> {
        self.check_error()?;
        if raw {
            let Value::Table(table) = target else {
                return Err(self.error(b"raw native access requires a table"));
            };
            self.call.vm.raw_set(table, key, value)?;
            return Ok(None);
        }
        let TagResult {
            called, yielded, ..
        } = self.run_tag_action(Action::Set([target, key, value]))?;
        if called {
            self.restore_window_call_base();
        }
        Ok(yielded)
    }
    /// Replace the top key with a table lookup; raw access bypasses metamethods.
    ///
    /// # Errors
    /// Runtime/metamethod errors propagate without unwinding the fault stack.
    /// Raw access requires a table. Rejects missing keys or invalid indexes.
    pub fn get_table(&mut self, index: isize, raw: bool) -> Result<(), Error> {
        self.check_error()?;
        let target = self.indexed_value(index)?;
        let key = self.indexed_value(-1)?;
        let destination = self.end() - 1;
        let (value, yielded) = self.read_table(target, key, raw)?;
        self.call.vm.registers[destination] = value;
        self.call.vm.gc_new_value(value);
        if let Some(yielded) = yielded {
            self.base = yielded.start;
            self.set_window_end(yielded.end - 1);
            self.call.vm.top = yielded.end - 1;
        } else {
            self.call.vm.truncate_registers(self.end());
        }
        Ok(())
    }
    /// Push a field lookup without adding its key to the visible native stack.
    ///
    /// # Errors
    /// Propagates lookup/metamethod errors and rejects a full stack or invalid index.
    pub fn get_field(&mut self, index: isize, name: &[u8]) -> Result<(), Error> {
        self.check_error()?;
        if self.top() == limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        let target = self.indexed_value(index)?;
        let key = self.intern(name);
        let destination = self.end();
        let (value, yielded) = self.read_table(target, key, false)?;
        if let Some(yielded) = yielded {
            self.call.vm.registers[destination] = value;
            self.call.vm.gc_new_value(value);
            self.base = yielded.start;
            self.set_window_end(yielded.end);
            self.call.vm.top = yielded.end;
            Ok(())
        } else {
            self.push(value)
        }
    }
    /// Pop a key and value after successful assignment to the indexed target.
    /// Raw access bypasses metamethods.
    ///
    /// # Errors
    /// Propagates table/metamethod errors, retaining original inputs on failure.
    pub fn set_table(&mut self, index: isize, raw: bool) -> Result<(), Error> {
        self.check_error()?;
        let target = self.indexed_value(index)?;
        let key = self.indexed_value(-2)?;
        let value = self.indexed_value(-1)?;
        if let Some(yielded) = self.write_table(target, key, value, raw)? {
            self.base = yielded.start;
            self.set_window_end(yielded.end - 2);
            self.call.vm.top = yielded.end - 2;
            Ok(())
        } else {
            self.set_top(self.top() - 2)
        }
    }
    /// Pop the top value after successful field assignment.
    ///
    /// # Errors
    /// Propagates table/metamethod errors, retaining original inputs on failure.
    pub fn set_field(&mut self, index: isize, name: &[u8]) -> Result<(), Error> {
        self.check_error()?;
        let target = self.indexed_value(index)?;
        let value = self.indexed_value(-1)?;
        let key = self.intern(name);
        if let Some(yielded) = self.write_table(target, key, value, false)? {
            self.base = yielded.start;
            self.set_window_end(yielded.end - 1);
            self.call.vm.top = yielded.end - 1;
            Ok(())
        } else {
            self.set_top(self.top() - 1)
        }
    }
    /// Push a raw integer-key lookup.
    ///
    /// # Errors
    /// Rejects non-tables, invalid indexes, pending errors or a full stack.
    pub fn raw_get_integer(&mut self, index: isize, key: i32) -> Result<(), Error> {
        self.check_error()?;
        let target = self.indexed_value(index)?;
        let (value, _) = self.read_table(target, Value::Number(f64::from(key)), true)?;
        self.push(value)
    }
    /// Pop the top value after raw integer-key assignment.
    ///
    /// # Errors
    /// Rejects non-tables, invalid indexes, pending errors or missing values.
    pub fn raw_set_integer(&mut self, index: isize, key: i32) -> Result<(), Error> {
        self.check_error()?;
        let target = self.indexed_value(index)?;
        let value = self.indexed_value(-1)?;
        self.write_table(target, Value::Number(f64::from(key)), value, true)?;
        self.set_top(self.top() - 1)
    }
    /// Push the actual metatable, bypassing `__metatable`; false leaves the stack unchanged.
    ///
    /// # Errors
    /// Rejects a pending error, invalid index or a full stack when pushing a result.
    pub fn get_metatable(&mut self, index: isize) -> Result<bool, Error> {
        self.check_error()?;
        let value = self.indexed_value(index)?;
        let Some(table) = self.call.vm.metatable(value) else {
            return Ok(false);
        };
        self.push(Value::Table(table))?;
        Ok(true)
    }
    /// Pop a table or nil and set the actual metatable, bypassing `__metatable`.
    /// Primitive values update their shared type metatable.
    ///
    /// # Errors
    /// Rejects pending errors, invalid indexes or a top value other than table/nil.
    pub fn set_metatable(&mut self, index: isize) -> Result<(), Error> {
        self.check_error()?;
        let value = self.indexed_value(index)?;
        let table = match self.indexed_value(-1)? {
            Value::Nil => None,
            Value::Table(table) => Some(table),
            _ => return Err(self.error(b"native metatable must be a table or nil")),
        };
        self.call.vm.set_metatable(value, table);
        self.set_top(self.top() - 1)
    }
    /// Replace a traversal key with the next key/value pair, or pop it at the end.
    ///
    /// # Errors
    /// Rejects non-tables, invalid traversal keys, missing values or a full stack.
    pub fn next_table(&mut self, index: isize) -> Result<bool, Error> {
        self.check_error()?;
        let target = self.indexed_value(index)?;
        let key = self.indexed_value(-1)?;
        let next = self
            .call
            .vm
            .table(target)?
            .next(key)
            .map_err(|error| self.call.vm.operand_error(error))?;
        if let Some((key, value)) = next {
            if self.top() == limits::MAX_CSTACK {
                return Err(self.error(b"native stack overflow"));
            }
            self.set(-1, key)?;
            self.push(value)?;
            Ok(true)
        } else {
            self.set_top(self.top() - 1)?;
            Ok(false)
        }
    }
}
