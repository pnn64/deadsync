//! Execute synchronous APIs on another thread without resuming its entry.
use super::*;

impl NativeStack<'_, '_> {
    /// Call the function below the selected stack's last arguments without a
    /// local error handler. Results replace the function and its arguments.
    /// A permitted yield leaves that thread's new frames pending for resumption.
    ///
    /// # Errors
    /// Retains faulting frames for the selected thread's enclosing handler or
    /// native panic callback. Propagates invalid counts and fatal quotas.
    pub fn call_unprotected_thread(
        &mut self,
        thread: ThreadId,
        arguments: usize,
        results: Option<usize>,
    ) -> Result<usize, Error> {
        self.with_thread_stack(thread, |stack| stack.call_unprotected(arguments, results))
    }
    /// Apply a collector operation in the selected thread's context.
    /// Heap controls are shared; finalizers observe that thread and its calling
    /// environment. Successful operations retain its execution status. A tag
    /// call can restore a suspended C thread's earlier CallInfo base.
    ///
    /// # Errors
    /// Rejects invalid identities and propagates finalizer errors and fatal quotas.
    pub fn gc_thread(&mut self, thread: ThreadId, operation: GcOperation) -> Result<i32, Error> {
        self.with_thread_stack(thread, |stack| stack.gc(operation))
    }
    /// Create an empty thread on the selected stack, inheriting its globals and hooks.
    /// The child is pushed on that stack and does not begin executing.
    ///
    /// # Errors
    /// Rejects invalid threads, stack bounds, collector errors and fatal quotas.
    pub fn new_thread_on(&mut self, thread: ThreadId) -> Result<ThreadId, Error> {
        self.with_thread_stack(thread, |stack| stack.new_thread())
    }
    /// Concatenate the last values on the selected stack using its metamethods.
    /// Pending execution and status remain intact after ordinary completion.
    /// A permitted yield leaves Lua frames pending and can put the physical
    /// top below its new base; inspect [`NativeThreadStack::top_offset`].
    ///
    /// # Errors
    /// Propagates invalid counts, stack bounds, metamethod errors and fatal quotas.
    pub fn concatenate_thread(&mut self, thread: ThreadId, count: usize) -> Result<(), Error> {
        self.with_thread_stack(thread, |stack| stack.concatenate(count))
    }
    #[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
    pub(crate) fn concatenate_primitives_thread(
        &mut self,
        thread: ThreadId,
        count: usize,
    ) -> Result<(), Error> {
        self.with_thread_stack(thread, |stack| stack.concatenate_primitives(count))
    }
    #[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
    pub(crate) fn check_thread_gc(&mut self, thread: ThreadId) -> Result<(), Error> {
        self.with_thread_stack(thread, |stack| stack.check_gc())
    }
    pub(super) fn with_thread_stack<R>(
        &mut self,
        thread: ThreadId,
        action: impl FnOnce(&mut NativeStack<'_, '_>) -> Result<R, Error>,
    ) -> Result<R, Error> {
        if thread == self.thread_id() {
            return action(self);
        }
        let target = self.thread_stack(thread)?;
        let base = target.base;
        let count = target.top;
        let end = target.end;
        let waiting = target.frame;
        let state = &target.vm.threads[slot_index(thread.0)];
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
        let status = state.status;
        let abandoned = state.abandoned;
        let resume_status = state.resume_status;
        let entry_level = self.call.vm.thread_call_level(thread);
        let owner = self.thread_id();
        let owner_status = self.call.vm.threads[slot_index(owner.0)].status;
        let root = self.call.vm.pin(Value::Thread(owner))?;
        self.call.vm.threads[slot_index(thread.0)].abandoned = false;
        self.call.vm.threads[slot_index(owner.0)].status = threads::Status::Normal;
        let mut line = 0;
        self.call.vm.switch_thread(thread, &mut line);
        let active = self.call.vm.active_native;
        let depth = self.call.vm.frames.len();
        let original_bias = self.call.vm.threads[slot_index(thread.0)].native_bias;
        let adjustment = entry_level.wrapping_sub(self.call.vm.native_calls_for(thread, depth));
        self.call
            .vm
            .set_thread_native_bias(thread, original_bias.wrapping_add(adjustment));
        self.call.vm.push_record(Frame {
            function: FrameKind::Native(CONT_NATIVE_WINDOW),
            pc: 0,
            base,
            stack_start: base,
            result: restored_base,
            wanted: 0,
            barriers: self.call.vm.frames.last().map_or(1, |f| f.barriers),
            vararg_start: base,
            vararg_count: count as u32,
            tail_calls: 0,
            c_level_bias: 0,
        });
        self.call.vm.active_native = Some(depth);
        let (outcome, end, final_base) = {
            let mut call = HostCall {
                vm: self.call.vm,
                hook: None,
                id: 0,
                arguments: base,
                count,
                output: base.min(end),
                results: count,
                abort: None,
                stack_opened: true,
            };
            let mut stack = NativeStack {
                call: &mut call,
                base,
                frame: depth,
                finished: false,
            };
            let outcome = action(&mut stack);
            // An unprotected call retains its fault stack. Publishing the old
            // window end would let the panic error slot overwrite an open
            // local before resetstack closes that local's upvalue.
            let end = if outcome
                .as_ref()
                .is_err_and(|error| error.kind() == ErrorKind::Runtime)
            {
                stack.call.vm.registers.len().max(stack.call.vm.top)
            } else {
                stack.end()
            };
            stack.finished = true;
            (outcome, end, stack.base)
        };
        if outcome
            .as_ref()
            .is_err_and(|error| error.target_thread.is_some())
        {
            self.call.vm.leave_threads_to(thread, &mut line)?;
        } else {
            self.call.vm.abort_threads_to(thread, &mut line);
        }
        // A panic on this waiting state can have removed the entire invocation.
        // Keep its reset stack, status and C-call count when Rust returns through
        // the abandoned API wrapper.
        let reset = self.call.vm.frames.is_empty()
            && self.call.vm.threads[slot_index(thread.0)].status == threads::Status::Dead;
        if reset {
            self.call.vm.active_native = None;
        } else {
            self.call.vm.active_native = active;
            let pending = self.call.vm.frames.len() > depth + 1;
            if pending {
                self.call.vm.frames[depth].pc = host::API_YIELDED;
                self.call.vm.frames[depth].vararg_count = 0;
            } else {
                self.call.vm.frames.truncate(depth);
            }
            self.call.vm.restore_calls();
            self.call.vm.top = end;
            let bias = self.call.vm.threads[slot_index(thread.0)]
                .native_bias
                .wrapping_sub(adjustment);
            self.call.vm.set_thread_native_bias(thread, bias);
            // Each operation retains its actual final base. A completed call can
            // restore the CallInfo base, while a primitive keeps a yielded suffix.
            let base = final_base;
            {
                let state = &mut self.call.vm.threads[slot_index(thread.0)];
                state.abandoned = abandoned;
                if state.resume_status == resume_status {
                    state.status = status;
                } else if state.resume_status == threads::NativeThreadStatus::Yielded {
                    state.status = threads::Status::Suspended;
                }
                state.api_stack = base..end;
                state.native_level = entry_level.wrapping_add(bias.wrapping_sub(original_bias));
            }
            if let Some(frame) = waiting.filter(|_| !pending) {
                self.call.vm.frames[frame].vararg_start = base;
                self.call.vm.frames[frame].vararg_count = (end - base) as u32;
            }
        }
        self.call.vm.switch_thread(owner, &mut line);
        self.call.vm.threads[slot_index(owner.0)].status = owner_status;
        self.call
            .vm
            .unpin(root)
            .expect("retained native API caller");
        self.call.results = self.call.vm.frames[self.frame].argc();
        self.sync();
        if let Err(error) = &outcome
            && (error.kind() == ErrorKind::Limit || error.target_thread.is_some())
        {
            self.call.abort = Some(error.clone());
        }
        outcome
    }
}

#[cfg(all(test, feature = "native-c-api", any(windows, target_os = "linux")))]
mod tests {
    use super::*;
    #[test]
    fn a_selected_gc_check_needs_no_public_stack_result_slot() {
        fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
            let mut s = call.native_stack()?;
            let owner = s.thread_id();
            let target = s.new_thread()?;
            for _ in 0..limits::MAX_CSTACK {
                s.thread_stack(target)?.push(Value::Number(17.))?;
            }
            s.check_thread_gc(target)?;
            assert_eq!(s.thread_id(), owner);
            assert_eq!(s.top(), 1);
            assert_eq!(s.thread_stack(target)?.top(), limits::MAX_CSTACK);
            assert_eq!(s.thread_stack(target)?.value(-1), Some(Value::Number(17.)));
            s.finish(1)
        }
        let mut vm = Vm::new();
        let f = vm.new_host_function(callback, &[]).unwrap();
        vm.set_global(b"check", f).unwrap();
        vm.execute(
            &crate::compile(
                b"local co=check();assert(coroutine.status(co)=='suspended');collectgarbage()",
            )
            .unwrap(),
        )
        .unwrap();
    }
}
