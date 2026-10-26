//! Direct Rust function pointers over the native register call boundary.
use super::*;

/// A synchronous Rust callback. Append results through [`HostCall::push`].
///
/// Arguments and captures borrow VM storage. Callbacks may allocate Lua values
/// and return Lua errors, but this interface does not reenter Lua or suspend.
/// Rust panics follow the build's panic strategy. After unwinding, discard the
/// VM; its Lua finalizers are skipped because execution was interrupted.
pub type HostFunction = fn(&mut HostCall<'_>) -> Result<(), Error>;

#[derive(Debug)]
pub(super) struct Function {
    callback: HostFunction,
    pub upvalues: Box<[Value]>,
}

/// A native callback's arguments, captured values and result writer.
///
/// Indices are zero-based. Arguments stay unchanged when results are appended.
/// Values copied out of this context follow the ordinary [`Value`] rooting
/// rules. Results and captures remain collector roots during Lua execution.
pub struct HostCall<'vm> {
    vm: &'vm mut Vm,
    id: u64,
    arguments: usize,
    count: usize,
    output: usize,
    results: usize,
}

impl Vm {
    /// Create a native Lua function backed by a Rust function pointer.
    ///
    /// Captures must originate in this VM. They are copied once and traced with
    /// the closure. Each registration
    /// has a distinct identity. Store or pin the result before executing Lua or
    /// collecting. The function inherits the current native/Lua environment.
    /// Calls use existing register storage and need no argument/result vectors.
    ///
    /// ```
    /// use deadlib_lua::{Error, HostCall, Value, Vm, compile};
    /// fn add(call: &mut HostCall<'_>) -> Result<(), Error> {
    ///     let sum = call.number(0)? + call.number(1)?;
    ///     call.push(Value::Number(sum))
    /// }
    /// let mut vm = Vm::new();
    /// let add = vm.new_host_function(add, &[])?;
    /// vm.set_global(b"add", add)?;
    /// assert_eq!(vm.execute(&compile(b"return add(17, 25)")?)?, &[Value::Number(42.0)]);
    /// # Ok::<(), Error>(())
    /// ```
    ///
    /// # Errors
    /// Returns an error for invalid/collected captures or more than 60 captures.
    pub fn new_host_function(
        &mut self,
        callback: HostFunction,
        captures: &[Value],
    ) -> Result<Value, Error> {
        if captures.len() > 60 {
            return Err(Error::new(
                ErrorKind::Limit,
                0,
                "too many host function captures",
            ));
        }
        for &value in captures {
            self.validate_value(value)?;
        }
        let upvalues = captures.into();
        self.native_closures
            .note_alloc(std::mem::size_of_val(captures));
        self.new_native(threads::NativeKind::Host(Function { callback, upvalues }))
    }

    #[inline]
    pub(super) fn host_callback(&self, id: u64) -> Option<HostFunction> {
        if id < strings::ITERATOR_FIRST {
            return None;
        }
        match self.native_closures.get(id).map(|f| &f.kind) {
            Some(threads::NativeKind::Host(function)) => Some(function.callback),
            _ => None,
        }
    }

    pub(super) fn host_native(
        &mut self,
        id: u64,
        base: usize,
        count: usize,
        callback: HostFunction,
    ) -> Result<usize, Error> {
        let arguments = self.frames[self.active_native.expect("native host frame")].vararg_start;
        let mut context = HostCall {
            vm: self,
            id,
            arguments,
            count,
            output: base,
            results: 0,
        };
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(&mut context)));
        match outcome {
            Ok(result) => result?,
            Err(payload) => {
                // A Rust panic bypasses the Lua driver. Reuse the shutdown
                // state to prevent Drop from executing interrupted Lua frames.
                context.vm.gc.closing = true;
                std::panic::resume_unwind(payload);
            }
        }
        Ok(context.results)
    }
}

impl HostCall<'_> {
    /// Borrow the positional arguments, including explicit nil values.
    pub fn arguments(&self) -> &[Value] {
        &self.vm.registers[self.arguments..self.arguments + self.count]
    }

    /// Borrow the closure's captured values.
    pub fn captures(&self) -> &[Value] {
        let threads::NativeKind::Host(f) = &self.vm.native_closures[slot_index(self.id)].kind
        else {
            unreachable!("active host function")
        };
        &f.upvalues
    }

    /// Replace a captured value without changing its slot or closure identity.
    ///
    /// # Errors
    /// Returns an error for a missing slot or invalid/collected value.
    pub fn set_capture(&mut self, index: usize, value: Value) -> Result<(), Error> {
        self.vm.validate_value(value)?;
        let threads::NativeKind::Host(f) = &mut self.vm.native_closures[slot_index(self.id)].kind
        else {
            unreachable!("active host function")
        };
        let slot = f
            .upvalues
            .get_mut(index)
            .ok_or_else(|| Error::new(ErrorKind::Runtime, 0, "invalid host capture index"))?;
        *slot = value;
        self.vm.gc_barrier(Value::Native(self.id), value);
        Ok(())
    }

    /// Read a number, accepting Lua numeric strings.
    ///
    /// # Errors
    /// Returns a native argument error for a missing or nonnumeric value.
    pub fn number(&self, index: usize) -> Result<f64, Error> {
        if index >= limits::MAX_CSTACK {
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                "invalid host argument index",
            ));
        }
        self.arguments()
            .get(index)
            .and_then(|&value| self.vm.number(value))
            .ok_or_else(|| {
                self.vm
                    .argument_type_error(self.arguments - 1, self.count, index, "number")
            })
    }

    /// Append a result without copying or replacing the arguments.
    ///
    /// # Errors
    /// Returns an error for invalid/collected values or the 8,000-slot native
    /// stack bound, including the original arguments.
    pub fn push(&mut self, value: Value) -> Result<(), Error> {
        self.vm.validate_value(value)?;
        if self.results >= limits::MAX_CSTACK.saturating_sub(self.count) {
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                "too many host function results",
            ));
        }
        let position = self.output + self.results;
        self.vm
            .resize_registers(self.vm.registers.len().max(position + 1));
        self.vm.registers[position] = value;
        self.vm.gc_new_value(value);
        self.results += 1;
        Ok(())
    }

    /// Intern an arbitrary byte string in this VM.
    pub fn intern(&mut self, bytes: &[u8]) -> Value {
        self.vm.intern(bytes)
    }

    /// Borrow the bytes of a live string in this VM.
    pub fn string_bytes(&self, id: StringId) -> Option<&[u8]> {
        self.vm.string_bytes(id)
    }

    /// Create a Lua table. Store, capture or return it before leaving this call.
    pub fn new_table(&mut self) -> TableId {
        self.vm.new_table()
    }

    /// Read a table without invoking metamethods.
    ///
    /// # Errors
    /// Returns an error for invalid/collected table or key handles.
    pub fn raw_table_get(&self, table: TableId, key: Value) -> Result<Value, Error> {
        self.vm.validate_value(Value::Table(table))?;
        self.vm.validate_value(key)?;
        Ok(self.vm.tables[slot_index(table.0)].get(key))
    }

    /// Set a table entry without invoking metamethods.
    ///
    /// # Errors
    /// Returns an error for invalid handles, nil keys or NaN keys.
    pub fn raw_table_set(&mut self, table: TableId, key: Value, value: Value) -> Result<(), Error> {
        self.vm.raw_table_set(table, key, value)
    }

    /// Allocate zero-initialized userdata in this native function's environment.
    pub fn new_userdata(&mut self, size: usize) -> Value {
        Value::Userdata(
            self.vm
                .allocate_userdata(size, self.vm.native_environment(self.id)),
        )
    }

    /// Borrow live userdata storage.
    pub fn userdata_bytes(&self, id: UserdataId) -> Option<&[u8]> {
        self.vm.userdata_bytes(id)
    }

    /// Mutably borrow live userdata storage, preserving its size and identity.
    pub fn userdata_bytes_mut(&mut self, id: UserdataId) -> Option<&mut [u8]> {
        self.vm.userdata_bytes_mut(id)
    }

    /// Construct a Lua runtime error with exact message bytes.
    pub fn error(&self, message: &[u8]) -> Error {
        Error::from_bytes(ErrorKind::Runtime, 0, message.to_vec())
    }

    /// Construct a Lua error preserving the supplied value's identity.
    /// Invalid or collected handles become ordinary invalid-handle errors.
    pub fn raise(&self, value: Value) -> Error {
        if let Err(error) = self.vm.validate_value(value) {
            return error;
        }
        let bytes = match value {
            Value::String(id) => self.vm.string_bytes(id).unwrap().to_vec(),
            Value::Number(n) => format_number(n),
            _ => b"Lua error".to_vec(),
        };
        Error::from_bytes(ErrorKind::Runtime, 0, bytes).with_value(value)
    }
}
