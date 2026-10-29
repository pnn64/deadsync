//! A C API invocation can return at a yield without a coroutine resumer.
use super::*;

#[derive(Debug)]
pub(in crate::vm) struct ApiCall {
    pub thread: ThreadId,
    pub frame: usize,
    pub barriers: u16,
    pub status: NativeCallStatus,
    pub yielded: Option<std::ops::Range<usize>>,
}
pub(in crate::vm) struct ApiResult {
    pub status: NativeCallStatus,
    pub yielded: Option<std::ops::Range<usize>>,
}

impl Vm {
    pub(in crate::vm) fn api_yield_allowed(&self, barriers: u16) -> bool {
        self.api_calls
            .iter()
            .rev()
            .find(|s| s.thread == self.thread)
            .is_some_and(|s| barriers <= s.barriers)
    }
    pub(in crate::vm) fn run_thread_api(
        &mut self,
        call: usize,
        arguments: usize,
        results: Option<usize>,
        handler: Option<usize>,
        protected: bool,
    ) -> Result<ApiResult, Error> {
        let owner = self.thread;
        let depth = self.frames.len();
        let active = self.active_native;
        let barriers = self.frames.last().map_or(0, |f| f.barriers);
        self.api_calls.push(ApiCall {
            thread: owner,
            frame: depth,
            barriers: barriers + 1,
            status: NativeCallStatus::Ok,
            yielded: None,
        });
        self.push_record(Frame {
            function: FrameKind::Native(if protected {
                CONT_HOST_PCALL
            } else {
                CONT_HOST_CALL
            }),
            pc: u32::from(protected),
            base: call,
            stack_start: call,
            result: call,
            wanted: FRAME_OPEN,
            barriers,
            vararg_start: handler.unwrap_or(call),
            vararg_count: u32::from(handler.is_some()),
            tail_calls: 0,
            c_level_bias: 0,
        });
        let protection = self.native_protected;
        self.native_protected |= protected;
        let wanted = results.map_or(OPEN, |n| n as u32);
        self.truncate_registers(call + 1 + arguments);
        let start = self
            .start_call(call, arguments, wanted, false)
            .map_err(|error| {
                if error.kind() == ErrorKind::Runtime && error.value().is_none() {
                    let value = self.intern(error.message_bytes());
                    error.with_value(value)
                } else {
                    error
                }
            });
        let result = self.drive_host(owner, depth, start, wanted);
        self.native_protected = protection;
        let scope = self.api_calls.pop().expect("C API protection scope");
        if let Err(error) = &result
            && error.target_thread.is_some_and(|target| target != owner)
        {
            let mut line = 0;
            self.leave_threads_to(owner, &mut line)?;
            self.active_native = active;
            return Err(error.clone());
        }
        if scope.yielded.is_some() {
            self.active_native = active;
            // Keep one invisible continuation to finish the pending Lua call
            // later. Its handler is no longer installed after the API returns.
            self.frames[depth].pc = API_YIELDED;
            self.frames[depth].vararg_count = 0;
            return Ok(ApiResult {
                status: NativeCallStatus::Ok,
                yielded: scope.yielded,
            });
        }
        if !protected
            && self.thread == owner
            && let Err(error) = &result
            && error.kind() == ErrorKind::Runtime
        {
            self.active_native = active;
            return Err(error.clone());
        }
        let mut line = 0;
        self.abort_threads_to(owner, &mut line);
        self.active_native = active;
        self.close_upvalues(call);
        self.discard_finalizers(depth);
        self.frames.truncate(depth);
        self.restore_calls();
        self.trim_substitutions();
        self.trim_sorts();
        self.trim_loads();
        self.trim_tracebacks();
        if result.is_err() {
            self.trim_hooks();
        }
        match result {
            Ok(count) => {
                self.truncate_registers(call + count);
                self.top = call + count;
                Ok(ApiResult {
                    status: scope.status,
                    yielded: None,
                })
            }
            Err(error) => {
                self.truncate_registers(call);
                if let Some(value) = error.value() {
                    self.registers.push(value);
                }
                self.top = self.registers.len();
                Err(error)
            }
        }
    }
}
