//! Route selected-state errors and reset unprotected states before C panic.
use super::*;

impl NativeStack<'_, '_> {
    pub(crate) fn flush_process_streams(&mut self) {
        self.call.vm.flush_process_streams();
    }
    pub(crate) fn route_waiting_error(&self, thread: ThreadId, error: &Error) -> Option<Error> {
        let state = self.call.vm.threads.get(thread.0)?;
        if state.status != threads::Status::Normal || !self.call.vm.thread_has_protection(thread) {
            return None;
        }
        let mut error = error.clone();
        error.target_thread = Some(thread);
        Some(error)
    }
    pub(crate) fn waiting_thread_error(
        &mut self,
        thread: ThreadId,
    ) -> Result<Option<Error>, Error> {
        let protected = self.call.vm.thread_has_protection(thread);
        let target = self.thread_stack(thread)?;
        if target.vm.threads[slot_index(thread.0)].status != threads::Status::Normal || !protected {
            return Ok(None);
        }
        let value = target
            .value(-1)
            .ok_or_else(|| target.error(b"missing native error value"))?;
        let mut error = target.vm.host_error(value);
        error.target_thread = Some(thread);
        Ok(Some(error))
    }
    fn panic_target(&mut self, thread: ThreadId) -> Result<NativeThreadStack<'_>, Error> {
        let target = self.thread_stack(thread)?;
        let waiting = target.vm.threads[slot_index(thread.0)].status == threads::Status::Normal
            && !target.vm.thread_has_protection(thread);
        if !waiting
            && !matches!(
                target.vm.threads[slot_index(thread.0)].status,
                threads::Status::Suspended | threads::Status::Dead
            )
        {
            return Err(target.error(b"cross-thread protected error routing is not implemented"));
        }
        Ok(target)
    }
    pub(crate) fn push_thread_panic_error(
        &mut self,
        thread: ThreadId,
        error: &Error,
    ) -> Result<(), Error> {
        let mut target = self.panic_target(thread)?;
        let value = error
            .value()
            .unwrap_or_else(|| target.intern(error.message_bytes()));
        target.push(value)?;
        target.vm.threads[slot_index(thread.0)].resume_status =
            threads::NativeThreadStatus::for_error(error);
        Ok(())
    }
    #[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
    pub(crate) fn prepare_thread_panic(
        &mut self,
        thread: ThreadId,
        reset: bool,
        status: threads::NativeThreadStatus,
    ) -> Result<(), Error> {
        let target = self.panic_target(thread)?;
        let value = target
            .value(-1)
            .ok_or_else(|| target.error(b"missing native error value"))?;
        if !reset {
            let state = &mut self.call.vm.threads[slot_index(thread.0)];
            state.resume_status = status;
            state.status = threads::Status::Dead;
            return Ok(());
        }
        let owner = self.thread_id();
        let root = self.call.vm.pin(Value::Thread(owner))?;
        let mut line = 0;
        self.call.vm.switch_thread(thread, &mut line);
        self.call.vm.close_upvalues(0);
        self.call.vm.discard_finalizers(0);
        self.call.vm.frames.clear();
        self.call.vm.restore_calls();
        self.call.vm.trim_substitutions();
        self.call.vm.trim_sorts();
        self.call.vm.trim_loads();
        self.call.vm.trim_tracebacks();
        let mut hooks = crate::vm::hooks::Hooks::default();
        hooks.callback = self.call.vm.hooks.callback;
        hooks.mask = self.call.vm.hooks.mask;
        hooks.count = self.call.vm.hooks.count;
        hooks.ticks = self.call.vm.hooks.ticks;
        self.call.vm.hooks = hooks;
        self.call.vm.registers.clear();
        self.call.vm.registers.push(value);
        self.call.vm.gc_new_value(value);
        self.call.vm.top = 1;
        self.call.vm.active_native = None;
        let state = &mut self.call.vm.threads[slot_index(thread.0)];
        state.api_stack = 0..1;
        state.resume_status = status;
        state.status = threads::Status::Dead;
        state.resumer = None;
        state.native_resumer = false;
        state.resumed_values = None;
        state.entry = None;
        state.depth = 0;
        state.native_level = state.native_base_calls;
        state.native_bias = state.native_base_calls.wrapping_sub(1);
        self.call.vm.switch_thread(owner, &mut line);
        self.call
            .vm
            .unpin(root)
            .expect("retained native panic caller");
        self.call.results = self.call.vm.frames[self.frame].argc();
        self.sync();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panic_without_a_callback_keeps_the_stack_until_process_exit() {
        fn inspect(call: &mut HostCall<'_>) -> Result<(), Error> {
            let mut s = call.native_stack()?;
            let owner = s.thread_id();
            let child = s.new_thread()?;
            {
                let mut target = s.thread_stack(child)?;
                target.push(Value::Number(17.))?;
                target.push(Value::Boolean(true))?;
            }
            s.prepare_thread_panic(child, false, threads::NativeThreadStatus::RuntimeError)?;
            let target = s.thread_stack(child)?;
            assert_eq!(target.top(), 2);
            assert_eq!(target.value(1), Some(Value::Number(17.)));
            assert_eq!(target.value(2), Some(Value::Boolean(true)));
            assert_eq!(
                s.thread_status(child),
                Some(threads::NativeThreadStatus::RuntimeError)
            );
            s.prepare_thread_panic(child, true, threads::NativeThreadStatus::RuntimeError)?;
            let target = s.thread_stack(child)?;
            assert_eq!(target.top(), 1);
            assert_eq!(target.value(1), Some(Value::Boolean(true)));
            assert_eq!(s.thread_id(), owner);
            s.finish(1)
        }
        let mut vm = Vm::new();
        let f = vm.new_host_function(inspect, &[]).unwrap();
        vm.set_global(b"inspect", f).unwrap();
        vm.execute(
            &crate::compile(
                b"local co=inspect();assert(coroutine.status(co)=='dead');collectgarbage()",
            )
            .unwrap(),
        )
        .unwrap();
    }
}
