//! Borrow the persistent main stack without inventing a Lua/C activation.
use super::*;

impl Vm {
    #[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
    pub(crate) fn prepare_native_close(&mut self) {
        debug_assert_eq!(self.thread, self.main);
        debug_assert!(self.frames.is_empty() && self.active_native.is_none());
        // lua_close drops the main CallInfo's values before running GC tag
        // methods. In particular, a finalizer can collect weak stack-only roots.
        self.close_upvalues(0);
        self.truncate_registers(0);
        self.top = 0;
        let state = &mut self.threads[slot_index(self.main.0)];
        state.api_stack = 0..0;
        state.depth = 0;
        state.native_base_calls = 0;
        state.native_level = 0;
        self.set_thread_native_bias(self.main, 0);
    }

    #[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
    pub(crate) fn native_root_panic(&mut self, error: &Error) {
        let state = &mut self.threads[slot_index(self.main.0)];
        state.resume_status = threads::NativeThreadStatus::for_error(error);
        state.status = threads::Status::Dead;
        state.resumer = None;
        state.native_resumer = false;
        state.resumed_values = None;
        state.entry = None;
        state.depth = 0;
        state.native_level = state.native_base_calls;
        // Outside an entry the Rust model includes one implicit root charge;
        // with_native_stack removes it before the next C API operation.
        let bias = state.native_base_calls;
        self.set_thread_native_bias(self.main, bias);
    }

    /// Borrow the idle main thread's persistent Lua 5.1 native stack.
    ///
    /// Values remain on this stack between entries and are collector roots.
    /// The entry adds no visible function frame. The ordinary native stack,
    /// parser, protected calls, coroutine and collector APIs are available.
    /// Other VM execution methods replace the main stack with their results.
    /// The complete stack remains when the borrow ends.
    ///
    /// # Errors
    /// Rejects an executing VM, standalone context or oversized initial stack.
    /// Unprotected errors unwind the operation and leave only their Lua error
    /// value, if present, on the main stack. A Rust panic follows the build's
    /// panic strategy; discard the VM after an unwind, as with a host callback panic.
    pub fn with_native_stack<R>(
        &mut self,
        action: impl FnOnce(&mut NativeStack<'_, '_>) -> Result<R, Error>,
    ) -> Result<R, Error> {
        if self.thread != self.main
            || !self.frames.is_empty()
            || self.active_native.is_some()
            || self.standalone != 0
        {
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                "native root stack requires an idle main thread",
            ));
        }
        if self.top > limits::MAX_CSTACK {
            return Err(Error::new(ErrorKind::Runtime, 0, "native stack overflow"));
        }
        self.reset_budget();
        let count = self.top;
        self.truncate_registers(count);
        let thread = self.main;
        let bias = self.threads[slot_index(thread.0)].native_bias;
        // Ordinary Rust execution includes its implicit root luaD_call charge.
        // Borrowing base_ci itself has no such call.
        self.set_thread_native_bias(thread, bias.wrapping_sub(1));
        self.push_record(Frame {
            function: FrameKind::Native(CONT_NATIVE_WINDOW),
            pc: 0,
            base: 0,
            stack_start: 0,
            result: 0,
            wanted: 0,
            barriers: 0,
            vararg_start: 0,
            vararg_count: count as u32,
            tail_calls: 0,
            c_level_bias: 0,
        });
        self.active_native = Some(0);
        let (result, end) = {
            let mut call = HostCall {
                vm: self,
                hook: None,
                id: 0,
                data: None,
                activation: None,
                arguments: 0,
                count,
                output: 0,
                results: count,
                abort: None,
                stack_opened: true,
            };
            let mut stack = NativeStack {
                call: &mut call,
                base: 0,
                frame: 0,
                finished: false,
            };
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| action(&mut stack)));
            stack.finished = true;
            let result = match outcome {
                Ok(result) => stack.call.abort.take().map_or(result, Err),
                Err(payload) => {
                    stack.call.vm.gc.closing = true;
                    std::panic::resume_unwind(payload);
                }
            };
            (result, stack.end())
        };
        self.active_native = None;
        let result = match result {
            Ok(value) => {
                self.frames.clear();
                self.restore_calls();
                self.trim_hooks();
                self.truncate_registers(end);
                self.top = end;
                Ok(value)
            }
            Err(error) => {
                let mut line = 0;
                self.abort_threads_to(thread, &mut line);
                let error = self.finish_execution(Err(error)).unwrap_err();
                Err(error)
            }
        };
        let bias = self.threads[slot_index(thread.0)].native_bias;
        self.set_thread_native_bias(thread, bias.wrapping_add(1));
        self.threads[slot_index(thread.0)].api_stack = 0..self.top;
        result
    }
}
