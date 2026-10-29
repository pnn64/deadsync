//! Mutable native argument/temporary storage in the VM register window.
use super::*;
pub(in crate::vm) mod body;
pub(in crate::vm) use body::Body;
pub use body::NativeBuiltinBody;
mod coroutine;
mod dump;
mod file;
mod hooks;
mod inspection;
pub(in crate::vm) mod io_callbacks;
pub use hooks::{NativeHook, NativeHookEvent, NativeHookInfo};
pub use inspection::{NativeDebugFrame, NativeDebugInfo, NativeDebugString};
mod loader;
mod objects;
mod parser_roots;
mod protected;
mod root;
mod scalars;
mod tables;
mod thread;
mod thread_stack;
pub use thread_stack::NativeThreadStack;
mod upvalues;
pub use loader::NativeLoadStatus;
pub use protected::NativeCallStatus;

/// A rooted mutable stack for a synchronous callback or idle main-stack entry.
///
/// Positive indexes start at one; negative indexes count back from the top.
/// Lua 5.1 pseudo-indexes access the registry (-10000), closure environment
/// (-10001), thread globals (-10002) and captures (-10003 for the first).
/// In an external hook they address the interrupted function. The Rust hook's
/// own captures and binding data remain accessible through [`HostCall`].
/// This window is separate from closure captures. Nested calls preserve the
/// live prefix and cannot yield across this callback. [`Self::call`] protects
/// errors; [`Self::call_unprotected`] propagates them with their fault frames.
/// Call [`Self::finish`] to select the last values as callback results, or
/// [`Self::yield_results`] to end the callback at a coroutine yield. Dropping
/// the stack without finishing selects zero results; the window remains
/// available to fault/return inspectors until the native frame exits.
/// [`Vm::with_native_stack`] instead borrows the complete persistent main stack;
/// the borrow adds no visible call and does not need a result selection.
pub struct NativeStack<'call, 'vm> {
    call: &'call mut HostCall<'vm>,
    base: usize,
    frame: usize,
    finished: bool,
}

struct TagResult {
    range: std::ops::Range<usize>,
    called: bool,
    yielded: Option<std::ops::Range<usize>>,
}

impl<'vm> HostCall<'vm> {
    /// Open a mutable native stack initialized with the callback's arguments.
    ///
    /// # Errors
    /// Rejects an existing result writer/stack or more than 8,000 arguments.
    pub fn native_stack(&mut self) -> Result<NativeStack<'_, 'vm>, Error> {
        let frame = self.vm.active_native.expect("active host callback");
        if self.hook.is_some() && !self.stack_opened {
            if self.results != 0 {
                return Err(self.error(b"native stack must precede result writes"));
            }
            self.stack_opened = true;
            let base = self.arguments;
            self.output = base;
            self.results = self.count;
            return Ok(NativeStack {
                call: self,
                base,
                frame,
                finished: false,
            });
        }
        if self.stack_opened || self.results != 0 || self.output != self.vm.frames[frame].base {
            return Err(self.error(b"native stack must precede result writes"));
        }
        if self.count > limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        let base = self.output;
        self.vm
            .resize_registers((base + self.count).max(self.vm.registers.len()));
        self.vm
            .registers
            .copy_within(self.arguments..self.arguments + self.count, base);
        self.vm
            .relocate_native_error_functions(self.arguments..self.arguments + self.count, base);
        // Call arguments have moved into this window. Clear their old owned
        // temporaries so suspended-call maps do not retain popped arguments.
        let start = self.arguments.min(base);
        let end = (self.arguments + self.count).min(base);
        self.vm.registers[start..end].fill(Value::Nil);
        self.vm.truncate_registers(base + self.count);
        self.results = self.count;
        self.stack_opened = true;
        let mut stack = NativeStack {
            call: self,
            base,
            frame,
            finished: false,
        };
        stack.sync();
        Ok(stack)
    }
}

impl NativeStack<'_, '_> {
    fn run_tag_action(&mut self, action: Action<'_>) -> Result<TagResult, Error> {
        let selected = matches!(
            self.call.vm.frames[self.frame].function,
            FrameKind::Native(CONT_NATIVE_WINDOW)
        );
        if selected {
            self.call.vm.api_calls.push(ApiCall {
                thread: self.thread_id(),
                frame: self.call.vm.frames.len(),
                barriers: self.call.vm.frames[self.frame].barriers + 2,
                status: NativeCallStatus::Ok,
                yielded: None,
            });
        }
        let outcome = self.call.run_action_with_call(action, false);
        let yielded = if selected {
            self.call
                .vm
                .api_calls
                .pop()
                .expect("tag-method API scope")
                .yielded
        } else {
            None
        };
        let (range, called) = outcome?;
        Ok(TagResult {
            range,
            called,
            yielded,
        })
    }
    /// Apply a collector operation while preserving this native stack.
    /// Collection and completed steps run finalizers through the shared child
    /// scheduler, with no extra visible Lua/native function frame. The result
    /// is zero for controls/collection, an integer count, the previous control
    /// value, or zero/one for an unfinished/completed step. Counts estimate this
    /// engine's live storage; collector work units differ from PUC Lua.
    ///
    /// # Errors
    /// Propagates finalizer errors with their fault frames and Lua error value.
    /// A pending unprotected error or an instruction limit also rejects the call.
    pub fn gc(&mut self, operation: GcOperation) -> Result<i32, Error> {
        self.check_error()?;
        let (result, dispatch) = self.call.vm.gc_control(operation);
        if dispatch && !self.call.vm.gc.finalizers.is_empty() {
            let tag_calls = self.call.vm.gc.tag_calls;
            self.call.run_action(Action::Finalize, false)?;
            if self.call.vm.gc.tag_calls != tag_calls {
                self.restore_window_call_base();
            }
            self.call.vm.truncate_registers(self.end());
        }
        Ok(result)
    }
    fn capture_index(index: isize) -> Option<usize> {
        usize::try_from((-10003isize).checked_sub(index)?).ok()
    }
    fn context_function(&self) -> Option<Value> {
        if let Some(activation) = self.call.activation {
            return Some(activation);
        }
        // A C hook executes inside the interrupted activation. HostCall.id
        // still identifies the registered Rust hook and its private binding.
        let window = matches!(
            self.call.vm.frames[self.frame].function,
            FrameKind::Native(CONT_NATIVE_WINDOW)
        );
        if self.call.hook.is_some() || window {
            let end = if window {
                self.call.vm.frames.len()
            } else {
                self.frame
            };
            self.call.vm.frames[..end]
                .iter()
                .rev()
                .find(|f| f.is_call())
                .map(|frame| match frame.function {
                    FrameKind::Lua(id) => Value::Function(id),
                    FrameKind::Native(id) => Value::Native(id),
                })
        } else {
            Some(Value::Native(self.call.id))
        }
    }
    fn context_environment(&self) -> TableId {
        self.context_function()
            .and_then(|f| self.call.vm.closure_environment(f))
            .unwrap_or(self.call.vm.globals)
    }
    fn check_error(&self) -> Result<(), Error> {
        if let Some(error) = &self.call.abort {
            return Err(error.clone());
        }
        Ok(())
    }
    fn check_gc(&mut self) -> Result<(), Error> {
        self.check_error()?;
        let tag_calls = self.call.vm.gc.tag_calls;
        self.call.run_action(Action::GcCheck, false)?;
        if self.call.vm.gc.tag_calls != tag_calls {
            self.restore_window_call_base();
        }
        self.call.vm.truncate_registers(self.end());
        Ok(())
    }
    fn sync(&mut self) {
        self.call.vm.frames[self.frame].vararg_start = self.base;
        self.call.vm.frames[self.frame].vararg_count = self.call.results as u32;
    }
    fn end(&self) -> usize {
        self.call.output + self.call.results
    }
    fn set_window_end(&mut self, end: usize) {
        self.call.output = self.base.min(end);
        self.call.results = end.saturating_sub(self.base);
        self.sync();
    }
    fn restore_window_call_base(&mut self) {
        if !matches!(
            self.call.vm.frames[self.frame].function,
            FrameKind::Native(CONT_NATIVE_WINDOW)
        ) {
            return;
        }
        let base = self.native_call_base();
        // luaD_poscall restores ci->base even when lua_yield temporarily
        // exposed only a suffix of that C activation's arguments.
        let end = self.end();
        self.base = base;
        self.set_window_end(end);
    }
    fn native_call_base(&self) -> usize {
        self.call
            .vm
            .frames
            .iter()
            .rev()
            .find(|f| f.is_call())
            .map_or(self.call.vm.frames[self.frame].result, |f| {
                match f.function {
                    FrameKind::Lua(_) => f.base,
                    FrameKind::Native(_) => f.vararg_start,
                }
            })
    }
    fn position(&self, index: isize) -> Option<usize> {
        let position = if index > 0 {
            index - 1
        } else {
            (self.top() as isize).checked_add(index)?
        };
        (position >= 0 && (position as usize) < self.top()).then(|| self.base + position as usize)
    }
    /// Number of currently live stack values.
    pub fn top(&self) -> usize {
        self.call.results
    }
    /// Signed stack offset reported by Lua 5.1's `lua_gettop`.
    /// A selected-thread API can return below its yielded base; `top` then
    /// reports zero accessible values while this offset remains negative.
    pub fn top_offset(&self) -> isize {
        self.end() as isize - self.base as isize
    }
    /// Read an indexed object's stable opaque pointer; strings/scalars yield None.
    /// The pointer does not retain the object and must not be dereferenced.
    pub fn to_pointer(&mut self, index: isize) -> Option<usize> {
        let value = self.value(index)?;
        self.call.vm.object_pointer(value)
    }
    /// Get an opaque pointer identity for a live VM-owned value without pushing it.
    /// This has the lifetime and type rules of [`Vm::object_pointer`].
    pub fn object_pointer(&mut self, value: Value) -> Option<usize> {
        self.call.vm.object_pointer(value)
    }
    /// Read a stack value or Lua 5.1 pseudo-index. Absent indexes return None.
    pub fn value(&self, index: isize) -> Option<Value> {
        match index {
            -10000 => Some(Value::Table(self.call.vm.registry)),
            -10001 => Some(Value::Table(self.context_environment())),
            -10002 => Some(Value::Table(self.call.vm.globals)),
            ..=-10003 => {
                let Value::Native(id) = self.context_function()? else {
                    return None;
                };
                Self::capture_index(index).and_then(|i| {
                    self.call
                        .vm
                        .native_closures
                        .get(id)?
                        .captures
                        .get(i)
                        .copied()
                })
            }
            _ => self.position(index).map(|p| self.call.vm.registers[p]),
        }
    }
    /// Reserve extra slots without changing the visible top.
    /// Returns false for allocation failure or the 8,000-slot limit.
    pub fn check_stack(&mut self, extra: usize) -> bool {
        if self.call.abort.is_some() {
            return false;
        }
        let Some(end) = self
            .end()
            .checked_add(extra)
            .filter(|&end| extra <= limits::MAX_CSTACK && end <= self.base + limits::MAX_CSTACK)
        else {
            return false;
        };
        let before = self.call.vm.registers.capacity();
        let needed = end.saturating_sub(self.call.vm.registers.len());
        if self.call.vm.registers.try_reserve(needed).is_err() {
            return false;
        }
        self.call.vm.gc_note_control(
            (self.call.vm.registers.capacity() - before) * std::mem::size_of::<Value>(),
        );
        true
    }
    /// Resize the stack, filling new slots with nil and releasing popped roots.
    ///
    /// # Errors
    /// Returns a runtime error above the 8,000-slot bound.
    pub fn set_top(&mut self, top: usize) -> Result<(), Error> {
        self.check_error()?;
        if top > limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        let end = self.base + top;
        let before = self.end();
        self.call.vm.resize_registers(end);
        if before < end {
            self.call.vm.registers[before..end].fill(Value::Nil);
        }
        self.set_window_end(end);
        Ok(())
    }
    /// Push a live value, retaining it across nested calls and collection.
    ///
    /// # Errors
    /// Rejects collected/invalid handles or a full stack.
    pub fn push(&mut self, value: Value) -> Result<(), Error> {
        self.check_error()?;
        self.call.vm.validate_value(value)?;
        if !self.check_stack(1) {
            return Err(self.error(b"native stack overflow"));
        }
        let at = self.end();
        self.call.vm.resize_registers(at + 1);
        self.set_window_end(at + 1);
        self.call.vm.registers[at] = value;
        self.call.vm.gc_new_value(value);
        Ok(())
    }
    /// Replace a value without changing the top.
    ///
    /// # Errors
    /// Rejects an invalid stack index or collected/invalid value.
    pub fn set(&mut self, index: isize, value: Value) -> Result<(), Error> {
        self.check_error()?;
        self.call.vm.validate_value(value)?;
        match index {
            -10002..=-10000 => {
                let Value::Table(table) = value else {
                    return Err(self.error(b"native pseudo-index requires a table"));
                };
                match index {
                    -10000 => {
                        self.call.vm.registry = table;
                        self.call.vm.gc_new_value(value);
                    }
                    -10001 => {
                        let function = self
                            .context_function()
                            .ok_or_else(|| self.error(b"no calling environment"))?;
                        self.call.vm.set_closure_environment(function, table);
                    }
                    _ => self
                        .call
                        .vm
                        .set_thread_environment(self.call.vm.thread, table),
                }
                return Ok(());
            }
            ..=-10003 => {
                let index = Self::capture_index(index)
                    .ok_or_else(|| self.error(b"invalid native capture index"))?;
                let Some(Value::Native(id)) = self.context_function() else {
                    return Err(self.error(b"invalid native capture index"));
                };
                return self.call.vm.set_native_capture(id, index, value);
            }
            _ => {}
        }
        let position = self
            .position(index)
            .ok_or_else(|| self.error(b"invalid native stack index"))?;
        self.call.vm.registers[position] = value;
        self.call.vm.gc_new_value(value);
        Ok(())
    }
    /// Remove an ordinary stack slot and shift later values down.
    ///
    /// # Errors
    /// Rejects pending errors, absent indexes and pseudo-indexes.
    pub fn remove(&mut self, index: isize) -> Result<(), Error> {
        self.check_error()?;
        let position = self
            .position(index)
            .ok_or_else(|| self.error(b"invalid native stack index"))?;
        let end = self.base + self.top();
        self.call
            .vm
            .registers
            .copy_within(position + 1..end, position);
        self.set_top(self.top() - 1)
    }
    /// Move the top value into an ordinary stack slot, shifting values up.
    ///
    /// # Errors
    /// Rejects pending errors, absent indexes and pseudo-indexes.
    pub fn insert(&mut self, index: isize) -> Result<(), Error> {
        self.check_error()?;
        let position = self
            .position(index)
            .ok_or_else(|| self.error(b"invalid native stack index"))?;
        let end = self.base + self.top() - 1;
        let value = self.call.vm.registers[end];
        self.call
            .vm
            .registers
            .copy_within(position..end, position + 1);
        self.call.vm.registers[position] = value;
        Ok(())
    }
    /// Read a number, accepting numeric strings in any live stack slot.
    ///
    /// # Errors
    /// Returns a native argument/type error for an absent or nonnumeric slot.
    pub fn number(&self, index: isize) -> Result<f64, Error> {
        self.check_error()?;
        let value = self.value(index);
        value.and_then(|v| self.call.vm.number(v)).ok_or_else(|| {
            self.call.vm.argument_error_at(
                index as i64,
                format!(
                    "number expected, got {}",
                    value.map_or("no value", Value::type_name)
                )
                .as_bytes(),
            )
        })
    }
    /// Replace the last `captures` values with a new host closure.
    /// The closure inherits this callback's environment and owns `data` privately.
    /// Lua values retained by data must also occur in the traced captures.
    ///
    /// # Errors
    /// Rejects a pending error, capture underflow, more than 255 captures or a
    /// full stack. A rejected operation leaves the current stack unchanged.
    pub fn push_closure<T: std::any::Any + Send + Sync>(
        &mut self,
        callback: HostFunction,
        captures: usize,
        data: T,
    ) -> Result<(), Error> {
        self.check_error()?;
        let prefix = self
            .top()
            .checked_sub(captures)
            .ok_or_else(|| self.error(b"native capture stack underflow"))?;
        if prefix >= limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        let values = self.call.vm.registers[self.base + prefix..self.base + self.top()].to_vec();
        let function = self
            .call
            .vm
            .new_host_function_with_data(callback, &values, data)?;
        self.set_top(prefix)?;
        self.push(function)
    }
    /// Protected call of the function below the last `arguments` stack values.
    /// Results replace the function and arguments; None keeps all results.
    ///
    /// # Errors
    /// Preserves Lua error identity and the host call's sticky resource limits.
    /// The faulting child frames are unwound, as with [`HostCall::call`].
    pub fn call(&mut self, arguments: usize, results: Option<usize>) -> Result<usize, Error> {
        self.call_impl(arguments, results, true)
    }
    /// Unprotected call of the function below the last `arguments` values.
    /// Results replace the function and arguments; None keeps all results.
    ///
    /// # Errors
    /// Runtime failures retain child frames, locals and open upvalues for the
    /// enclosing Lua error handler. Any failure must escape this callback;
    /// later fallible stack operations and callback return repeat the error,
    /// even if Rust ignores it. Resource failures clean up child threads first.
    pub fn call_unprotected(
        &mut self,
        arguments: usize,
        results: Option<usize>,
    ) -> Result<usize, Error> {
        let result = self.call_impl(arguments, results, false);
        if let Err(error) = &result {
            self.call.abort = Some(error.clone());
        }
        result
    }
    fn call_impl(
        &mut self,
        arguments: usize,
        results: Option<usize>,
        protected: bool,
    ) -> Result<usize, Error> {
        self.check_error()?;
        let prefix = self
            .top()
            .checked_sub(
                arguments
                    .checked_add(1)
                    .ok_or_else(|| self.error(b"invalid native call count"))?,
            )
            .ok_or_else(|| self.error(b"native call stack underflow"))?;
        if results.is_some_and(|n| n > limits::MAX_CSTACK - prefix) {
            return Err(self.error(b"native stack overflow"));
        }
        if !protected
            && matches!(
                self.call.vm.frames[self.frame].function,
                FrameKind::Native(CONT_NATIVE_WINDOW)
            )
        {
            let thread = self.thread_id();
            let bias = self.call.vm.threads[slot_index(thread.0)].native_bias;
            self.call
                .vm
                .set_thread_native_bias(thread, bias.wrapping_add(1));
            let outcome =
                self.call
                    .vm
                    .run_thread_api(self.base + prefix, arguments, results, None, false);
            if outcome.is_ok() {
                let bias = self.call.vm.threads[slot_index(thread.0)].native_bias;
                self.call
                    .vm
                    .set_thread_native_bias(thread, bias.wrapping_sub(1));
            }
            let result = outcome?;
            let count = match &result.yielded {
                Some(range) => range.len(),
                None => self.call.vm.top - self.base - prefix,
            };
            if let Some(range) = result.yielded {
                self.base = range.start;
                self.set_window_end(range.end);
            } else {
                self.call.results = self.call.vm.top - self.base;
                self.restore_window_call_base();
            }
            self.sync();
            return Ok(count);
        }
        let function = self.call.vm.registers[self.base + prefix];
        let values =
            self.call.vm.registers[self.base + prefix + 1..self.base + self.top()].to_vec();
        self.call.results = prefix;
        self.sync();
        let selected = matches!(
            self.call.vm.frames[self.frame].function,
            FrameKind::Native(CONT_NATIVE_WINDOW)
        );
        let thread = self.thread_id();
        if selected {
            let bias = self.call.vm.threads[slot_index(thread.0)].native_bias;
            self.call
                .vm
                .set_thread_native_bias(thread, bias.wrapping_add(1));
        }
        let outcome = self.call.call_range(function, &values, protected);
        if selected {
            let bias = self.call.vm.threads[slot_index(thread.0)].native_bias;
            self.call
                .vm
                .set_thread_native_bias(thread, bias.wrapping_sub(1));
        }
        match outcome {
            Ok(range) => {
                let count = results.unwrap_or(range.len());
                if count > limits::MAX_CSTACK - prefix {
                    self.call.vm.truncate_registers(self.base + prefix);
                    return Err(self.error(b"native stack overflow"));
                }
                self.call.vm.resize_registers(self.base + prefix + count);
                self.call.results = prefix + count;
                self.sync();
                self.restore_window_call_base();
                Ok(count)
            }
            Err(error) => {
                if protected {
                    self.call.results = self.call.vm.registers.len() - self.base;
                    self.sync();
                }
                Err(error)
            }
        }
    }
    /// Select the last `count` values as callback results, preserving the full
    /// window for return-hook inspection and mutation.
    ///
    /// # Errors
    /// Rejects a result count greater than the current top.
    pub fn finish(mut self, count: usize) -> Result<(), Error> {
        self.check_error()?;
        if count > self.top() {
            return Err(self.error(b"invalid native result count"));
        }
        self.call.output = self.base + self.top() - count;
        self.call.results = count;
        self.finished = true;
        Ok(())
    }
    /// Intern byte strings in this VM; push/store the result before collection.
    pub fn intern(&mut self, bytes: &[u8]) -> Value {
        self.call.intern(bytes)
    }
    /// Construct a native runtime error with exact bytes.
    pub fn error(&self, message: &[u8]) -> Error {
        self.call.error(message)
    }
    /// Construct a Lua 5.1 argument error with caller naming and method/self
    /// adjustment. Negative indexes are reported literally. The error receives
    /// its source location when it escapes the current native callback.
    pub fn argument_error(&self, index: isize, detail: &[u8]) -> Error {
        self.call.vm.argument_error_at(index as i64, detail)
    }
    /// Push a Lua 5.1 source-location prefix at the requested stack level.
    /// Level zero, C frames, missing frames and unknown lines yield an empty
    /// string. Levels count visible calls and tail-call placeholders.
    ///
    /// # Errors
    /// Rejects a pending unprotected error or a full stack.
    pub fn push_where(&mut self, level: isize) -> Result<(), Error> {
        self.check_error()?;
        let bytes = self.call.vm.native_where(level);
        let prefix = self.intern(&bytes);
        self.push(prefix)
    }
    /// Construct an error retaining a Lua value's identity.
    pub fn raise(&self, value: Value) -> Error {
        self.call.raise(value)
    }
}
impl Drop for NativeStack<'_, '_> {
    fn drop(&mut self) {
        if !self.finished {
            self.call.output = self.base + self.top();
            self.call.results = 0;
        }
    }
}
