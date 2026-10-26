//! Direct Rust function pointers over the native register call boundary.
use super::*;
mod stack;
pub use stack::NativeStack;

/// A synchronous Rust callback. Append results through [`HostCall::push`].
///
/// Arguments and captures borrow VM storage. Callbacks may allocate Lua values
/// and return Lua errors. [`HostCall::call`] invokes Lua through a protected
/// nested boundary. A callback cannot suspend its Rust stack.
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
    abort: Option<Error>,
    stack_opened: bool,
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
    ) -> Result<(usize, usize), Error> {
        let arguments = self.frames[self.active_native.expect("native host frame")].vararg_start;
        let mut context = HostCall {
            vm: self,
            id,
            arguments,
            count,
            output: base,
            results: 0,
            abort: None,
            stack_opened: false,
        };
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(&mut context)));
        let result = match outcome {
            Ok(result) => result,
            Err(payload) => {
                // A Rust panic bypasses the Lua driver. Reuse the shutdown
                // state to prevent Drop from executing interrupted Lua frames.
                context.vm.gc.closing = true;
                std::panic::resume_unwind(payload);
            }
        };
        if let Some(error) = context.abort.take() {
            return Err(error);
        }
        result?;
        Ok((context.output, context.results))
    }

    fn host_error(&self, value: Value) -> Error {
        let bytes = match value {
            Value::String(id) => self.string_bytes(id).unwrap().to_vec(),
            Value::Number(n) => format_number(n),
            _ => b"Lua error".to_vec(),
        };
        Error::from_bytes(ErrorKind::Runtime, 0, bytes).with_value(value)
    }

    pub(super) fn host_return(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        self.frames.pop();
        if frame.pc == 2 {
            Err(self.host_error(self.registers[frame.base + 1]))
        } else {
            Ok(Some(self.top - frame.base))
        }
    }

    fn drive_host(
        &mut self,
        owner: ThreadId,
        depth: usize,
        start: Result<Option<usize>, Error>,
    ) -> Result<usize, Error> {
        let mut line = 0;
        let mut pending = match start {
            Ok(Some(count)) => {
                let base = self.frames[depth].base;
                self.finish_call(base, count, OPEN);
                None
            }
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        };
        loop {
            let result = pending.take().unwrap_or_else(|| self.run(&mut line));
            match result {
                Ok(count) if self.thread == owner && self.frames.len() == depth => {
                    return Ok(count);
                }
                Ok(count) if self.thread != owner => {
                    self.finish_thread(0, count, false, false, &mut line)
                }
                Err(error) if self.thread == owner && self.frames.len() == depth => {
                    return Err(error.at_line(line));
                }
                Err(error) if error.kind() == ErrorKind::Runtime => {
                    let floor = if self.thread == owner { depth } else { 0 };
                    match self.catch_error_from(&error, floor) {
                        Ok(true) => {}
                        Ok(false) if self.thread != owner => self.fail_thread(&error, &mut line),
                        Ok(false) => return Err(error.at_line(line)),
                        Err(error) => return Err(error.at_line(line)),
                    }
                }
                Err(error) => return Err(error.at_line(line)),
                Ok(_) => {
                    return Err(Error::new(
                        ErrorKind::Runtime,
                        0,
                        "invalid host call boundary",
                    ));
                }
            }
        }
    }
}

impl HostCall<'_> {
    /// Invoke a Lua/native function or callable value without leaving this callback.
    ///
    /// Results borrow register storage and remain rooted until another call or
    /// result write reuses it. Retain objects in captures/results before then.
    /// The call shares the outer instruction quota and cannot yield across this
    /// native boundary; child coroutines may resume/yield normally.
    ///
    /// ```
    /// use deadlib_lua::{Error, HostCall, Value, Vm, compile};
    /// fn invoke(call: &mut HostCall<'_>) -> Result<(), Error> {
    ///     let function = call.arguments()[0];
    ///     call.call_and_return(function, &[Value::Number(42.)])
    /// }
    /// let mut vm = Vm::new();
    /// let host = vm.new_host_function(invoke, &[])?;
    /// vm.set_global(b"host", host)?;
    /// let program = compile(b"return host(function(x) return x, nil, false end)")?;
    /// assert_eq!(vm.execute(&program)?, &[Value::Number(42.), Value::Nil, Value::Boolean(false)]);
    /// # Ok::<(), Error>(())
    /// ```
    ///
    /// # Errors
    /// Returns child Lua errors with their original error value. Runtime errors
    /// can be handled in Rust. Instruction/resource limits remain fatal to the
    /// outer entry even if this callback ignores the returned error.
    pub fn call(&mut self, function: Value, arguments: &[Value]) -> Result<&[Value], Error> {
        let range = self.call_range(function, arguments)?;
        Ok(&self.vm.registers[range])
    }

    /// Invoke a callable and append all of its results without an intermediate vector.
    ///
    /// # Errors
    /// Returns the errors from [`Self::call`] or the native result-count limit.
    pub fn call_and_return(&mut self, function: Value, arguments: &[Value]) -> Result<(), Error> {
        let range = self.call_range(function, arguments)?;
        let count = range.len();
        let arguments = if self.stack_opened { 0 } else { self.count };
        if count > limits::MAX_CSTACK.saturating_sub(arguments + self.results) {
            return Err(self.error(b"too many host function results"));
        }
        self.results += count;
        Ok(())
    }

    fn call_range(
        &mut self,
        function: Value,
        arguments: &[Value],
    ) -> Result<std::ops::Range<usize>, Error> {
        if let Some(error) = &self.abort {
            return Err(error.clone());
        }
        self.vm.validate_value(function)?;
        for &value in arguments {
            self.vm.validate_value(value)?;
        }
        if arguments.len() > u32::MAX as usize {
            let error = Error::new(ErrorKind::Limit, 0, "too many host call arguments");
            self.abort = Some(error.clone());
            return Err(error);
        }
        let owner = self.vm.thread;
        let depth = self.vm.frames.len();
        let top = self.vm.top;
        let active = self.vm.active_native;
        let base = self.output + self.results;
        self.vm.registers.truncate(base);
        self.vm.resize_registers(base + 1 + arguments.len());
        self.vm.registers[base] = function;
        self.vm.registers[base + 1..].copy_from_slice(arguments);
        self.vm.push_record(Frame {
            function: FrameKind::Native(CONT_HOST_CALL),
            pc: 1,
            base,
            stack_start: base,
            result: base,
            wanted: FRAME_OPEN,
            barriers: self.vm.frames.last().unwrap().barriers,
            vararg_start: base + 1,
            vararg_count: arguments.len() as u32,
            tail_calls: 0,
        });
        let start = self
            .vm
            .start_call(base, arguments.len(), OPEN, false)
            .map_err(|error| {
                // There is no Lua call-site location for failure to enter a value
                // supplied by Rust. Preserve the native-boundary diagnostic.
                if error.kind() == ErrorKind::Runtime && error.value().is_none() {
                    let value = self.vm.intern(error.message_bytes());
                    error.with_value(value)
                } else {
                    error
                }
            });
        let result = self.vm.drive_host(owner, depth, start);
        let mut line = 0;
        self.vm.abort_threads_to(owner, &mut line);
        self.vm.close_upvalues(base);
        self.vm.discard_finalizers(depth);
        self.vm.frames.truncate(depth);
        self.vm.restore_calls();
        self.vm.trim_substitutions();
        self.vm.trim_sorts();
        self.vm.trim_loads();
        self.vm.trim_tracebacks();
        if result.is_err() {
            self.vm.trim_hooks();
        }
        self.vm.active_native = active;
        self.vm.top = top;
        match result {
            Ok(count) => {
                self.vm.registers.truncate(base + count);
                Ok(base..base + count)
            }
            Err(error) => {
                self.vm.registers.truncate(base);
                if let Some(value) = error.value() {
                    self.vm.registers.push(value);
                }
                if error.kind() == ErrorKind::Limit {
                    self.abort = Some(error.clone());
                }
                Err(error)
            }
        }
    }

    /// Borrow positional arguments, including explicit nil values. After opening
    /// a native stack, this borrows its current complete window instead.
    pub fn arguments(&self) -> &[Value] {
        let (start, count) = self.argument_window();
        &self.vm.registers[start..start + count]
    }
    fn argument_window(&self) -> (usize, usize) {
        if self.stack_opened {
            let frame = self.vm.frames[self.vm.active_native.expect("active host callback")];
            (frame.vararg_start, frame.argc())
        } else {
            (self.arguments, self.count)
        }
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
                let (start, count) = self.argument_window();
                self.vm
                    .argument_type_error(start - 1, count, index, "number")
            })
    }

    /// Append a result without copying or replacing the arguments.
    ///
    /// # Errors
    /// Returns an error for invalid/collected values or the 8,000-slot native
    /// stack bound, including the original arguments.
    pub fn push(&mut self, value: Value) -> Result<(), Error> {
        self.vm.validate_value(value)?;
        let arguments = if self.stack_opened { 0 } else { self.count };
        if self.results >= limits::MAX_CSTACK.saturating_sub(arguments) {
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
        self.vm.host_error(value)
    }
}
