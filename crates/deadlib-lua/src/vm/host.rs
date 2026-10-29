//! Direct Rust function pointers over the native register call boundary.
use super::*;
use std::any::Any;
mod api;
pub(in crate::vm) mod stack;
mod thread;
pub(super) use api::ApiCall;
pub(in crate::vm) use stack::Body;
pub use stack::{
    NativeBuiltinBody, NativeCallStatus, NativeDebugFrame, NativeDebugInfo, NativeDebugString,
    NativeHook, NativeHookEvent, NativeHookInfo, NativeLoadStatus, NativeStack, NativeThreadStack,
};
pub use thread::HostThreadState;
pub(super) use thread::ThreadData;
pub(super) const API_YIELDED: u32 = 8;
pub(super) const API_TAG_CALL: u32 = 9;

enum Action<'a> {
    Call(Value, &'a [Value]),
    ProtectedCall(Value, &'a [Value], Option<usize>),
    Get([Value; 2]),
    Set([Value; 3]),
    Compare(Op, [Value; 2]),
    ConcatPair([Value; 2]),
    Concat { first: usize, last: usize },
    Finalize,
    GcCheck,
}
impl Action<'_> {
    fn arguments(&self) -> &[Value] {
        match self {
            Self::Call(_, values) | Self::ProtectedCall(_, values, _) => values,
            Self::Get(values) => values,
            Self::Set(values) => values,
            Self::Compare(..)
            | Self::ConcatPair(..)
            | Self::Concat { .. }
            | Self::Finalize
            | Self::GcCheck => &[],
        }
    }
}

/// A synchronous Rust callback. Append results through [`HostCall::push`].
///
/// Arguments and captures borrow VM storage. Callbacks may allocate Lua values
/// and return Lua errors. [`HostCall::call`] invokes Lua through a protected
/// nested boundary. A callback cannot suspend its Rust stack.
/// Rust panics follow the build's panic strategy. After unwinding, discard the
/// VM; its Lua finalizers are skipped because execution was interrupted.
pub type HostFunction = fn(&mut HostCall<'_>) -> Result<(), Error>;

#[derive(Clone)]
pub(super) struct Function {
    callback: HostFunction,
    data: Option<Arc<dyn Any + Send + Sync>>,
}
impl std::fmt::Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostFunction")
            .field("has_data", &self.data.is_some())
            .finish_non_exhaustive()
    }
}
impl Function {
    pub(super) fn heap_bytes(&self) -> usize {
        self.data.as_deref().map_or(0, std::mem::size_of_val)
    }
}

/// A native callback's arguments, captured values and result writer.
///
/// Indices are zero-based. Arguments stay unchanged when results are appended.
/// Values copied out of this context follow the ordinary [`Value`] rooting
/// rules. Results and captures remain collector roots during Lua execution.
pub struct HostCall<'vm> {
    vm: &'vm mut Vm,
    hook: Option<NativeHookEvent>,
    id: u64,
    data: Option<Arc<dyn Any + Send + Sync>>,
    activation: Option<Value>,
    arguments: usize,
    count: usize,
    output: usize,
    results: usize,
    abort: Option<Error>,
    stack_opened: bool,
}

impl Vm {
    pub(super) fn set_native_capture(
        &mut self,
        function: u64,
        index: usize,
        value: Value,
    ) -> Result<(), Error> {
        self.validate_value(value)?;
        let slot = self
            .native_closures
            .get_mut(function)
            .and_then(|closure| closure.captures.get_mut(index))
            .ok_or_else(|| Error::new(ErrorKind::Runtime, 0, "invalid host capture index"))?;
        *slot = value;
        self.gc_barrier(Value::Native(function), value);
        Ok(())
    }
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
    /// Returns an error for invalid/collected captures or more than 255 captures.
    pub fn new_host_function(
        &mut self,
        callback: HostFunction,
        captures: &[Value],
    ) -> Result<Value, Error> {
        self.host_function(callback, captures, None)
    }

    /// Create a host closure with private owned binding data and Lua captures.
    ///
    /// Data is inaccessible to Lua and does not count as an upvalue. Retained
    /// Lua values must be in `captures`; opaque data is not traced. Data drops
    /// when the closure is collected, or after Lua shutdown finalizers. Its
    /// destructor must not reenter Lua or panic. Heap accounting includes the
    /// boxed object's size, excluding allocations it owns indirectly.
    ///
    /// # Errors
    /// Rejects invalid/collected captures or more than 255 captures.
    pub fn new_host_function_with_data<T: Any + Send + Sync>(
        &mut self,
        callback: HostFunction,
        captures: &[Value],
        data: T,
    ) -> Result<Value, Error> {
        self.host_function(callback, captures, Some(Box::new(data)))
    }

    pub(super) fn host_function(
        &mut self,
        callback: HostFunction,
        captures: &[Value],
        data: Option<Box<dyn Any + Send + Sync>>,
    ) -> Result<Value, Error> {
        if captures.len() > 255 {
            return Err(Error::new(
                ErrorKind::Limit,
                0,
                "too many host function captures",
            ));
        }
        for &value in captures {
            self.validate_value(value)?;
        }
        let function = Function {
            callback,
            data: data.map(Arc::from),
        };
        self.native_closures.note_alloc(function.heap_bytes());
        self.new_native(threads::NativeKind::Host(function), captures)
    }

    #[inline]
    pub(super) fn host_callback(&self, id: u64) -> Option<HostFunction> {
        if let Some(callback) = io::callback(id) {
            return Some(callback);
        }
        if id < strings::ITERATOR_FIRST {
            return None;
        }
        match self.native_closures.get(id).map(|f| &f.kind) {
            Some(threads::NativeKind::Host(function)) => Some(function.callback),
            Some(threads::NativeKind::Builtin(code)) => io::callback(*code),
            Some(threads::NativeKind::Lines) => Some(stack::io_callbacks::readline),
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
        let data = self.host_binding(id).and_then(|f| f.data.clone());
        let activation = self.builtin_context(id);
        let hook = self.builtin_hook(id);
        let mut context = HostCall {
            vm: self,
            hook,
            id,
            data,
            activation,
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

    fn host_binding(&self, id: u64) -> Option<&Function> {
        let threads::NativeKind::Host(function) = &self.native_closures.get(id)?.kind else {
            return None;
        };
        Some(function)
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
            Ok(Some(if frame.wanted == FRAME_OPEN {
                self.top - frame.base
            } else {
                usize::from(frame.wanted)
            }))
        }
    }

    pub(super) fn host_protected_return(&mut self, frame: Frame) -> Option<usize> {
        self.frames.pop();
        if frame.pc == API_YIELDED {
            // The C API invocation already returned at a yield. Continue the
            // retained Lua computation without reactivating its protection.
            return self.frames.is_empty().then_some(self.top);
        }
        let status = match frame.pc {
            2 => 2,
            5 => 5,
            _ => 0,
        };
        if status != 0 {
            self.registers[frame.base] = self.registers[frame.base + 1];
            self.top = frame.base + 1;
        }
        if frame.base == frame.stack_start {
            let scope = self.api_calls.last_mut().expect("active C API call");
            debug_assert_eq!(scope.thread, self.thread);
            scope.status = match status {
                2 => NativeCallStatus::RuntimeError,
                5 => NativeCallStatus::ErrorHandler,
                _ => NativeCallStatus::Ok,
            };
            return Some(self.top - frame.base);
        }
        // Status occupies a hidden slot preceding results.
        self.registers[frame.stack_start] = Value::Number(f64::from(status));
        Some(self.top - frame.stack_start)
    }

    pub(super) fn relocate_native_error_functions(
        &mut self,
        source: std::ops::Range<usize>,
        destination: usize,
    ) {
        if !self.native_protected {
            return;
        }
        for frame in &mut self.frames {
            if matches!(frame.function, FrameKind::Native(CONT_HOST_PCALL))
                && frame.argc() != 0
                && source.contains(&frame.vararg_start)
            {
                frame.vararg_start = destination + frame.vararg_start - source.start;
            }
        }
    }

    pub(super) fn discard_native_error_function_slots(&mut self, source: std::ops::Range<usize>) {
        if !self.native_protected {
            return;
        }
        for frame in &self.frames {
            if matches!(frame.function, FrameKind::Native(CONT_HOST_PCALL))
                && frame.argc() != 0
                && source.contains(&frame.vararg_start)
            {
                self.registers[frame.vararg_start] = Value::Nil;
            }
        }
    }

    fn drive_host(
        &mut self,
        owner: ThreadId,
        depth: usize,
        start: Result<Option<usize>, Error>,
        wanted: u32,
    ) -> Result<usize, Error> {
        let mut line = 0;
        let mut pending = match start {
            Ok(Some(count)) => {
                let base = self.frames[depth].base;
                self.finish_call(base, count, wanted);
                None
            }
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        };
        loop {
            let result = pending.take().unwrap_or_else(|| self.run(&mut line));
            if self
                .api_calls
                .last()
                .is_some_and(|s| s.thread == owner && s.frame == depth && s.yielded.is_some())
            {
                return Ok(0);
            }
            match result {
                Ok(count) if self.thread == owner && self.frames.len() == depth => {
                    return Ok(count);
                }
                Ok(count) if self.thread != owner => {
                    self.finish_thread(0, count, false, false, &mut line);
                }
                Err(error) if self.thread == owner && self.frames.len() == depth => {
                    return Err(error.at_line(line));
                }
                Err(error) if error.kind() == ErrorKind::Runtime => {
                    if error.target_thread.is_some_and(|target| target != owner) {
                        return Err(error);
                    }
                    if error.target_thread == Some(owner) {
                        self.leave_threads_to(owner, &mut line)?;
                    }
                    let floor = if self.thread == owner { depth } else { 0 };
                    match self.catch_error_from(&error, floor) {
                        Ok(true) => {}
                        Ok(false) if self.thread != owner => self.fail_thread(&error, &mut line),
                        Ok(false) => return Err(error.at_line(line)),
                        Err(error) if error.target_thread.is_some() => pending = Some(Err(error)),
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
    /// Event being delivered by an external native hook, without a Lua call frame.
    pub fn hook_event(&self) -> Option<NativeHookEvent> {
        self.hook
    }
    /// Borrow private binding data of the executing host callback.
    /// Direct body invocation keeps this binding separate from the caller's
    /// Lua environment and captured values.
    /// The borrow ends before a mutable operation can reenter Lua.
    pub fn function_data<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.data.as_deref()?.downcast_ref()
    }

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
        let range = self.call_range(function, arguments, true)?;
        Ok(&self.vm.registers[range])
    }

    /// Invoke a callable and append all of its results without an intermediate vector.
    ///
    /// # Errors
    /// Returns the errors from [`Self::call`] or the native result-count limit.
    pub fn call_and_return(&mut self, function: Value, arguments: &[Value]) -> Result<(), Error> {
        let range = self.call_range(function, arguments, true)?;
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
        protected: bool,
    ) -> Result<std::ops::Range<usize>, Error> {
        self.run_action(Action::Call(function, arguments), protected)
    }

    fn run_action(
        &mut self,
        action: Action<'_>,
        protected: bool,
    ) -> Result<std::ops::Range<usize>, Error> {
        self.run_action_with_call(action, protected)
            .map(|(range, _)| range)
    }
    // A completed tag-method call restores ci->base. A primitive operation or
    // table-valued metamethod chain does not, even though both use this driver.
    fn run_action_with_call(
        &mut self,
        action: Action<'_>,
        protected: bool,
    ) -> Result<(std::ops::Range<usize>, bool), Error> {
        if let Some(error) = &self.abort {
            return Err(error.clone());
        }
        if let Action::Call(function, _) | Action::ProtectedCall(function, _, _) = action {
            self.vm.validate_value(function)?;
        }
        if let Action::Compare(_, values) | Action::ConcatPair(values) = action {
            for value in values {
                self.vm.validate_value(value)?;
            }
        }
        let handler = match action {
            Action::ProtectedCall(_, _, handler) => handler,
            _ => None,
        };
        if let Some(handler) = handler {
            self.vm.validate_value(self.vm.registers[handler])?;
        }
        let c_protected = matches!(action, Action::ProtectedCall(..));
        let arguments = action.arguments();
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
        let scratch = self.output + self.results;
        let base = scratch + usize::from(c_protected);
        if c_protected {
            self.vm
                .relocate_native_error_functions(scratch..self.vm.registers.len(), base);
        }
        self.vm.truncate_registers(scratch);
        self.vm.resize_registers(base + 1 + arguments.len());
        if c_protected {
            self.vm.registers[scratch] = Value::Nil;
        }
        self.vm.registers[base] = match action {
            Action::Call(function, _) | Action::ProtectedCall(function, _, _) => function,
            _ => Value::Nil,
        };
        self.vm.registers[base + 1..].copy_from_slice(arguments);
        let selected_tag =
            matches!(
                action,
                Action::Get(..) | Action::Set(..) | Action::Compare(..)
            ) && self.vm.frames.last().is_some_and(|frame| {
                matches!(frame.function, FrameKind::Native(CONT_NATIVE_WINDOW))
            });
        self.vm.push_record(Frame {
            function: FrameKind::Native(match action {
                Action::Concat { .. } => CONT_HOST_CONCAT,
                _ if c_protected => CONT_HOST_PCALL,
                _ => CONT_HOST_CALL,
            }),
            // A selected window has no real C callback to account for the tag
            // method's luaD_call. Keep that call's ordinary counted continuation.
            pc: if selected_tag {
                API_TAG_CALL
            } else {
                u32::from(protected)
            },
            base,
            stack_start: scratch,
            result: base,
            wanted: match action {
                Action::Call(..) | Action::ProtectedCall(..) => FRAME_OPEN,
                Action::Get(..)
                | Action::Compare(..)
                | Action::ConcatPair(..)
                | Action::Concat { .. } => 1,
                Action::Set(..) | Action::Finalize | Action::GcCheck => 0,
            },
            // A bare thread still enters Lua through this C API boundary.
            barriers: self.vm.frames.last().map_or(1, |f| f.barriers),
            vararg_start: match action {
                Action::Concat { first, .. } => first,
                _ if c_protected => {
                    handler.map_or(scratch, |slot| slot + usize::from(slot >= scratch))
                }
                _ => base + 1,
            },
            vararg_count: match action {
                Action::Concat { first, last, .. } => (last - first + 1) as u32,
                _ if c_protected => u32::from(handler.is_some()),
                _ => arguments.len() as u32,
            },
            tail_calls: 0,
            c_level_bias: 0,
        });
        let previous_protection = self.vm.native_protected;
        self.vm.native_protected |= c_protected;
        let start = match action {
            Action::Call(..) | Action::ProtectedCall(..) => {
                self.vm.start_call(base, arguments.len(), OPEN, false)
            }
            Action::Get([target, key]) => self.vm.get_table(target, key, base).map(|()| None),
            Action::Set([target, key, value]) => {
                self.vm.set_table(target, key, value).map(|()| None)
            }
            Action::Compare(op, [left, right]) => {
                self.vm.compare(op, left, right, base).map(|()| None)
            }
            Action::ConcatPair([left, right]) => {
                self.vm.concatenate(left, right, base).map(|()| None)
            }
            Action::Concat { first, last, .. } => {
                self.vm.concat_range(base, first, last).map(|()| None)
            }
            Action::Finalize => {
                self.vm.push_finalizers(false);
                Ok(None)
            }
            Action::GcCheck => {
                self.vm.maybe_gc();
                Ok(None)
            }
        }
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
        if selected_tag
            && matches!(action, Action::Set(..))
            && let Some(frame) = self.vm.frames.get_mut(depth + 1)
        {
            // A setter's C call requested no results. If it yields and is
            // resumed later, its return still leaves the original operands
            // below the tag-function slot, rather than resetting top to zero.
            frame.result = scratch;
        }
        let synchronous = matches!(
            action,
            Action::Get(..) | Action::Set(..) | Action::Compare(..) | Action::ConcatPair(..)
        ) && self.vm.thread == owner
            && self.vm.frames.len() == depth + 1
            && matches!(start, Ok(None));
        // A raw result needs no interpreter entry. Entering the driver here
        // would run an unrelated GC check inside the still-active C callback.
        let result = if synchronous {
            self.vm
                .host_return(self.vm.frames[depth])
                .map(|count| count.expect("completed synchronous host action"))
        } else {
            self.vm.drive_host(owner, depth, start, OPEN)
        };
        self.vm.native_protected = previous_protection;
        if let Err(error) = &result
            && error.target_thread.is_some_and(|target| target != owner)
        {
            let mut line = 0;
            self.vm.leave_threads_to(owner, &mut line)?;
            self.vm.active_native = active;
            self.abort = Some(error.clone());
            return Err(error.clone());
        }
        if matches!(
            action,
            Action::Get(..) | Action::Set(..) | Action::Compare(..) | Action::ConcatPair(..)
        ) && let Some(range) = self
            .vm
            .api_calls
            .last()
            .filter(|scope| scope.thread == owner && scope.frame == depth)
            .and_then(|scope| scope.yielded.clone())
        {
            // luaD_call can return while a selected thread remains yielded.
            // callTMres still consumes the last physical slot as its result.
            self.vm.active_native = active;
            self.vm.frames[depth].pc = API_YIELDED;
            self.vm.frames[depth].vararg_count = 0;
            if selected_tag {
                // luaD_call has returned, although the tag method's Lua frame
                // remains pending. Its C-call charge has already been removed.
                self.vm.frames[depth + 1].pc = API_YIELDED;
            }
            let first = if matches!(action, Action::Set(..)) {
                range.end
            } else {
                range.end - 1
            };
            return Ok((first..range.end, false));
        }
        if !protected
            && self.vm.thread == owner
            && let Err(error) = &result
            && error.kind() == ErrorKind::Runtime
        {
            // Return through Rust/C first, retaining the Lua fault stack for
            // an enclosing handler. This callback cannot resume execution.
            self.vm.active_native = active;
            self.abort = Some(error.clone());
            return Err(error.clone());
        }
        let mut line = 0;
        self.vm.abort_threads_to(owner, &mut line);
        self.vm.close_upvalues(scratch);
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
                self.vm.truncate_registers(scratch + count);
                Ok((scratch..scratch + count, !synchronous))
            }
            Err(error) => {
                self.vm.truncate_registers(scratch);
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
        let function = self.activation.unwrap_or(Value::Native(self.id));
        let Value::Native(id) = function else {
            return &[];
        };
        self.vm.native_closures.get(id).map_or(&[], |f| &f.captures)
    }

    /// Replace a captured value without changing its slot or closure identity.
    ///
    /// # Errors
    /// Returns an error for a missing slot or invalid/collected value.
    pub fn set_capture(&mut self, index: usize, value: Value) -> Result<(), Error> {
        let Value::Native(id) = self.activation.unwrap_or(Value::Native(self.id)) else {
            return Err(self.error(b"invalid host capture index"));
        };
        self.vm.set_native_capture(id, index, value)
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
        if let Some(error) = &self.abort {
            return Err(error.clone());
        }
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
