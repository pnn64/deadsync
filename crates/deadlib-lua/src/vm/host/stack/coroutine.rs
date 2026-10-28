//! Native coroutine entry points using the shared thread scheduler.
use super::*;

impl NativeStack<'_, '_> {
    /// Read a live thread's Lua 5.1 C-call count, including active parser levels.
    /// Returns None for a collected or invalid thread identity.
    pub fn thread_call_level(&self, thread: ThreadId) -> Option<u16> {
        self.call.vm.threads.get(thread.0)?;
        Some(self.call.vm.thread_call_level(thread))
    }
    /// Copy Lua 5.1's C-call count between live threads without moving values.
    /// This is the operation used by `lua_setlevel` before coroutine resume.
    ///
    /// # Errors
    /// Rejects pending errors and collected/invalid thread identities.
    pub fn copy_thread_call_level(&mut self, from: ThreadId, to: ThreadId) -> Result<(), Error> {
        self.check_error()?;
        self.call.vm.validate_value(Value::Thread(from))?;
        self.call.vm.validate_value(Value::Thread(to))?;
        let level = self.call.vm.thread_call_level(from);
        let current = self.call.vm.thread_call_level(to);
        let state = &mut self.call.vm.threads[slot_index(to.0)];
        if to == self.call.vm.thread
            || (state.status == threads::Status::Normal && !state.abandoned)
        {
            let bias = state.native_bias.wrapping_add(level.wrapping_sub(current));
            self.call.vm.set_thread_native_bias(to, bias);
        } else {
            state.native_level = level;
        }
        Ok(())
    }
    /// Resume a thread with the last `arguments` values on its own stack.
    /// Yielded values, final results and errors remain on that thread's stack.
    /// A successfully completed thread may receive a new entry function.
    ///
    /// # Errors
    /// Rejects invalid handles/argument counts and propagates execution limits.
    pub fn resume_thread(
        &mut self,
        thread: ThreadId,
        arguments: usize,
    ) -> Result<threads::NativeThreadStatus, Error> {
        self.check_error()?;
        self.call.vm.validate_value(Value::Thread(thread))?;
        let owner = self.call.vm.thread;
        let state = &self.call.vm.threads[slot_index(thread.0)];
        let runnable = thread != owner
            && ((state.resume_status == threads::NativeThreadStatus::Yielded
                && !state.frames.is_empty())
                || (state.resume_status == threads::NativeThreadStatus::Ok
                    && state.frames.is_empty()));
        if !runnable || state.native_level >= 200 {
            let message = if runnable {
                b"C stack overflow".as_slice()
            } else {
                b"cannot resume non-suspended coroutine".as_slice()
            };
            let value = self.intern(message);
            if thread == owner {
                self.set_top(0)?;
                self.push(value)?;
            } else {
                let mut stack = self.thread_stack(thread)?;
                stack.set_top(0)?;
                stack.push(value)?;
            }
            return Ok(threads::NativeThreadStatus::RuntimeError);
        }
        self.call.vm.resume_api_barriers(thread);
        let state = &mut self.call.vm.threads[slot_index(thread.0)];
        if arguments > state.api_stack.len()
            || (state.frames.is_empty() && arguments == state.api_stack.len())
        {
            return Err(self.error(b"invalid coroutine resume arguments"));
        }
        state.registers.truncate(state.api_stack.end);
        if state.frames.is_empty() {
            state.entry = Some((state.api_stack.end - arguments - 1, arguments));
        } else {
            state.resumed_values = Some(state.api_stack.end - arguments..state.api_stack.end);
        }
        state.top = state.api_stack.end;
        state.depth = state.native_level;
        state.native_bias = 0;
        state.native_base_calls = state.native_level.wrapping_add(1);
        state.resumer = Some(owner);
        state.native_resumer = true;
        state.status = threads::Status::Running;
        state.resume_status = threads::NativeThreadStatus::Ok;
        self.call.vm.threads[slot_index(owner.0)].status = threads::Status::Normal;
        self.call.vm.transition = Some(threads::Transition::Resume(thread));
        let mut line = 0;
        let mut pending = self.call.vm.apply_transition(&mut line);
        let result = loop {
            match pending {
                Ok(Some(count)) if self.call.vm.thread != owner => {
                    self.call
                        .vm
                        .finish_thread(0, count, false, false, &mut line);
                }
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::Runtime => {
                    if error.target_thread.is_some_and(|target| target != thread) {
                        break Err(error);
                    }
                    if error.target_thread == Some(thread) {
                        self.call.vm.leave_threads_to(thread, &mut line)?;
                    }
                    match self.call.vm.catch_error(&error) {
                        Ok(true) => {}
                        Ok(false) => self.call.vm.fail_thread(&error, &mut line),
                        Err(error) => break Err(error),
                    }
                }
                Err(error) => break Err(error),
            }
            if self.call.vm.thread == owner {
                break Ok(self.call.vm.threads[slot_index(thread.0)].resume_status);
            }
            pending = self.call.vm.run(&mut line).map(Some);
        };
        if let Err(error) = &result {
            if error.target_thread.is_some() {
                self.call.vm.leave_threads_to(owner, &mut line)?;
                if self.call.vm.frames.is_empty()
                    && self.call.vm.threads[slot_index(owner.0)].status == threads::Status::Dead
                {
                    // A child can panic its unprotected waiting resumer, then
                    // escape to an outer state. That panic already removed
                    // this C invocation; it has no frame left to synchronize.
                    self.call.abort = Some(error.clone());
                    return Err(error.clone());
                }
                // The resumer's C call exits before the destination's handler
                // runs. Its visible frame belongs to this restored owner.
                self.call.vm.frames[self.frame].pc = threads::HOST_RESUME_RETURNED;
            } else {
                self.call.vm.abort_threads_to(owner, &mut line);
            }
            self.call.abort = Some(error.clone());
        }
        self.call.results = self.call.vm.frames[self.frame].argc();
        self.sync();
        result
    }

    /// Check whether this callback can yield its last `count` values.
    ///
    /// # Errors
    /// Rejects an invalid result count or a metamethod/C-call boundary.
    pub fn check_yield(&self, count: usize) -> Result<(), Error> {
        self.check_error()?;
        if self
            .call
            .hook
            .is_some_and(|e| !matches!(e.event, 2 | 3) || count != 0)
        {
            return Err(self.error(b"attempt to yield across metamethod/C-call boundary"));
        }
        if count > self.top() {
            return Err(self.error(b"invalid native yield result count"));
        }
        if self.call.vm.thread == self.call.vm.main
            || (self.call.vm.frames[self.frame].barriers > 1
                && !self
                    .call
                    .vm
                    .api_yield_allowed(self.call.vm.frames[self.frame].barriers))
            || self.call.vm.thread_call_level(self.call.vm.thread)
                > self.call.vm.threads[slot_index(self.call.vm.thread.0)].native_base_calls
        {
            return Err(self.error(b"attempt to yield across metamethod/C-call boundary"));
        }
        Ok(())
    }

    /// Yield the last `count` values, returning normally through the Rust host.
    /// The callback ends here; resuming supplies its return values without
    /// invoking the callback again or retaining a suspended Rust stack.
    ///
    /// # Errors
    /// Rejects an invalid result count or a metamethod/C-call boundary.
    pub fn yield_results(mut self, count: usize) -> Result<(), Error> {
        if let Err(error) = self.check_yield(count) {
            let value = self.intern(error.message_bytes());
            return Err(error.with_value(value));
        }
        if self.call.hook.is_some() {
            self.call.vm.hooks.yielding = true;
            self.finished = true;
            return Ok(());
        }
        let source = self.base + self.top() - count;
        self.call.vm.frames[self.frame].pc = threads::HOST_YIELDED;
        self.call.vm.transition = Some(threads::Transition::Yield { source, count });
        self.call.output = source;
        self.call.results = count;
        self.finished = true;
        Ok(())
    }
}
