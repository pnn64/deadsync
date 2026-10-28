//! Protected calls on a selected thread without resuming its pending computation.
use super::*;

impl NativeStack<'_, '_> {
    /// Execute the function below the last arguments on a live thread's own stack.
    /// Results or one protected error replace that function and its arguments.
    /// An optional ordinary target-stack index selects an error handler.
    /// The thread's suspended computation and retained execution status are preserved
    /// on ordinary completion. A permitted yield returns Ok, exposes yielded values
    /// on that thread, and leaves its new Lua frames pending for resumption.
    ///
    /// # Errors
    /// Rejects invalid stack/count inputs and pending errors. Instruction limits
    /// abort the originating host call after both thread contexts are restored.
    pub fn protected_call_thread(
        &mut self,
        thread: ThreadId,
        arguments: usize,
        results: Option<usize>,
        error_function: Option<isize>,
    ) -> Result<NativeCallStatus, Error> {
        self.check_error()?;
        if thread == self.thread_id() {
            return self.protected_call(arguments, results, error_function);
        }
        let target = self.thread_stack(thread)?;
        let base = target.base;
        let prefix = target
            .top
            .checked_sub(
                arguments
                    .checked_add(1)
                    .ok_or_else(|| target.error(b"invalid native call count"))?,
            )
            .ok_or_else(|| target.error(b"native call stack underflow"))?;
        if results.is_some_and(|n| n > limits::MAX_CSTACK - prefix) {
            return Err(target.error(b"native stack overflow"));
        }
        let handler = error_function
            .map(|index| {
                target
                    .position(index)
                    .ok_or_else(|| target.error(b"invalid native error function index"))
            })
            .transpose()?;
        let frame = target.frame;
        let state = &target.vm.threads[slot_index(thread.0)];
        // luaD_poscall/luaD_pcall restore ci->base, which can precede the
        // temporary base installed by lua_yield for its exposed result suffix.
        let restored_base =
            state
                .frames
                .iter()
                .rev()
                .find(|f| f.is_call())
                .map_or(base, |f| match f.function {
                    FrameKind::Lua(_) => f.base,
                    FrameKind::Native(_) => f.vararg_start,
                });
        let target_status = state.status;
        let abandoned = state.abandoned;
        let entry_level = self.call.vm.thread_call_level(thread);
        let owner = self.thread_id();
        let owner_status = self.call.vm.threads[slot_index(owner.0)].status;
        let root = self.call.vm.pin(Value::Thread(owner))?;
        self.call.vm.threads[slot_index(thread.0)].abandoned = false;
        if let Some(frame) = frame {
            self.call.vm.threads[slot_index(thread.0)].frames[frame].vararg_count = prefix as u32;
        }
        self.call.vm.threads[slot_index(owner.0)].status = threads::Status::Normal;
        let mut line = 0;
        self.call.vm.switch_thread(thread, &mut line);
        // A yielded host callback has no retained Rust/C invocation. Calibrate
        // the active model to the saved C count plus this API call, rather than
        // counting that logical yielding frame again. Remove only this temporary
        // adjustment afterwards so explicit lua_setlevel writes remain visible.
        let adjustment = entry_level.wrapping_add(1).wrapping_sub(
            self.call
                .vm
                .native_calls_for(thread, self.call.vm.frames.len()),
        );
        let original_bias = self.call.vm.threads[slot_index(thread.0)].native_bias;
        let bias = original_bias.wrapping_add(adjustment);
        self.call.vm.set_thread_native_bias(thread, bias);
        let outcome = self
            .call
            .vm
            .run_thread_api(base + prefix, arguments, results, handler, true);
        let yielded = outcome
            .as_ref()
            .ok()
            .and_then(|result| result.yielded.clone());
        let end = self.call.vm.top;
        let bias = if outcome
            .as_ref()
            .is_ok_and(|result| result.status == NativeCallStatus::Ok)
        {
            self.call.vm.threads[slot_index(thread.0)]
                .native_bias
                .wrapping_sub(adjustment)
        } else {
            // luaD_pcall restores its entry nCcalls on failure, including writes
            // made by the error handler. Successful explicit writes survive.
            original_bias
        };
        self.call.vm.set_thread_native_bias(thread, bias);
        {
            let state = &mut self.call.vm.threads[slot_index(thread.0)];
            state.abandoned = abandoned;
            if let Some(range) = &yielded {
                state.api_stack = range.clone();
            } else {
                state.api_stack = restored_base..end;
                state.status = target_status;
            }
            state.native_level = entry_level.wrapping_add(bias.wrapping_sub(original_bias));
        }
        if let Some(frame) = frame.filter(|_| yielded.is_none()) {
            self.call.vm.frames[frame].vararg_start = restored_base;
            self.call.vm.frames[frame].vararg_count = (end - restored_base) as u32;
        }
        self.call.vm.switch_thread(owner, &mut line);
        self.call.vm.threads[slot_index(owner.0)].status = owner_status;
        self.call
            .vm
            .unpin(root)
            .expect("retained native caller root");
        self.call.results = self.call.vm.frames[self.frame].argc();
        self.sync();
        if let Err(error) = &outcome
            && (error.kind() == ErrorKind::Limit || error.target_thread.is_some())
        {
            self.call.abort = Some(error.clone());
        }
        outcome.map(|result| result.status)
    }
}
