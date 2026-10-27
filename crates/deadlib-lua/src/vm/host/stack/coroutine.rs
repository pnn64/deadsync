//! Native coroutine entry points using the shared thread scheduler.
use super::*;

impl NativeStack<'_, '_> {
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
            && (state.resume_status == threads::NativeThreadStatus::Yielded
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
        state.depth = state.native_level as u16;
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
            self.call.vm.abort_threads_to(owner, &mut line);
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
        if count > self.top() {
            return Err(self.error(b"invalid native yield result count"));
        }
        if self.call.vm.thread == self.call.vm.main || self.call.vm.frames[self.frame].barriers > 1
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
        let source = self.base + self.top() - count;
        self.call.vm.frames[self.frame].pc = threads::HOST_YIELDED;
        self.call.vm.transition = Some(threads::Transition::Yield { source, count });
        self.call.output = source;
        self.call.results = count;
        self.finished = true;
        Ok(())
    }
}
