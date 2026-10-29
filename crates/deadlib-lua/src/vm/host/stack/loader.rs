//! Protected synchronous text/binary loading from native reader callbacks.
use super::*;

/// Lua 5.1 parser outcomes. Instruction limits escape as Rust errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NativeLoadStatus {
    /// Parsing succeeded. A reader yield can change the visible stack window.
    Ok = 0,
    /// The reader or an initial collector callback raised a Lua error.
    RuntimeError = 2,
    /// Text syntax or native-bytecode validation failed.
    SyntaxError = 3,
    /// An inherited error handler failed.
    ErrorHandler = 5,
    /// A file could not be opened, reopened or read.
    FileError = 6,
}
#[derive(Clone, Copy)]
struct Scope {
    owner: ThreadId,
    depth: usize,
    top: usize,
    active: Option<usize>,
    base: usize,
    call_base: usize,
    native_bias: u16,
}

impl NativeStack<'_, '_> {
    /// Compile text or native Lua 5.1 bytes through a synchronous reader.
    /// None/empty blocks signal EOF; the initial empty lookahead is not sticky.
    /// Readers use this same native stack and may reenter Lua. Success appends
    /// a function at the parser's final top. A permitted reader yield can leave
    /// that function below the newly selected window. Parser/runtime failure
    /// restores the entry top and CallInfo base before appending its error.
    /// Changes to caller slots remain visible. Names use the prefix
    /// before the first NUL; None selects `?`.
    /// Text readers also see the active constants-table/prototype pairs; native
    /// bytecode readers see each unfinished prototype, matching Lua 5.1's roots.
    ///
    /// # Errors
    /// Rejects pending unprotected errors and stack overflow. Instruction limits
    /// escape rather than becoming a recoverable parser status.
    pub fn load_reader(
        &mut self,
        name: Option<&[u8]>,
        reader: impl FnMut(&mut Self) -> Result<Option<Vec<u8>>, Error>,
    ) -> Result<NativeLoadStatus, Error> {
        self.load_reader_impl(name, None, reader)
    }
    pub(super) fn load_reader_impl(
        &mut self,
        name: Option<&[u8]>,
        file: Option<loads::FileReader>,
        mut reader: impl FnMut(&mut Self) -> Result<Option<Vec<u8>>, Error>,
    ) -> Result<NativeLoadStatus, Error> {
        self.check_error()?;
        if !self.check_stack(1) {
            return Err(self.error(b"native stack overflow"));
        }
        let scope = Scope {
            owner: self.call.vm.thread,
            depth: self.call.vm.frames.len(),
            top: self.call.vm.top,
            active: self.call.vm.active_native,
            base: self.base + self.top(),
            call_base: self.native_call_base(),
            native_bias: self.call.vm.threads[slot_index(self.call.vm.thread.0)].native_bias,
        };
        let depth = self.call.vm.native_depth(scope.depth);
        self.call.vm.push_record(Frame {
            function: FrameKind::Native(CONT_HOST_LOAD),
            pc: 1,
            base: scope.base,
            stack_start: scope.base,
            result: scope.base,
            wanted: 2,
            barriers: self.call.vm.frames.last().unwrap().barriers,
            vararg_start: 0,
            vararg_count: 0,
            tail_calls: 0,
            c_level_bias: 0,
        });
        let name = name.unwrap_or(b"?");
        let name = &name[..name
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(name.len())];
        let index = self
            .call
            .vm
            .start_native_load(scope.depth, name, depth, file);
        let mut first = true;
        loop {
            if !first {
                let result = self.call.vm.resume_native_load(index);
                let end = match self.apply_load_roots(index) {
                    Ok(end) => end,
                    Err(error) => return self.catch_load_error(scope, error),
                };
                match result {
                    Ok(Some(program)) => {
                        let function = self.call.vm.load(&program);
                        self.call.vm.retain_load_prototypes(index, function);
                        // A reader's Lua/C frames can remain suspended after
                        // lua_load itself has finished. Keep their stack and
                        // expire the parser's protection without unwinding them.
                        let pending = self.call.vm.frames.len() > scope.depth + 1;
                        self.finish_load_scope(scope, end, pending);
                        let top = end + 1;
                        self.call.vm.resize_registers(top);
                        self.call.vm.registers[end] = Value::Function(function);
                        self.call.vm.gc_new_value(Value::Function(function));
                        self.call.results = top
                            .checked_sub(self.base)
                            .ok_or_else(|| self.error(b"native parser stack underflow"))?;
                        if pending {
                            self.call.vm.top = top;
                        }
                        self.sync();
                        return Ok(NativeLoadStatus::Ok);
                    }
                    Err(error) => {
                        let message = self.call.vm.native_load_message(index, &error);
                        let value = self.intern(&message);
                        self.call
                            .vm
                            .set_thread_native_bias(scope.owner, scope.native_bias);
                        self.restore_load_base(scope);
                        self.finish_load_scope(scope, scope.base, false);
                        self.set_top(scope.base - self.base)?;
                        self.push(value)?;
                        return Ok(NativeLoadStatus::SyntaxError);
                    }
                    Ok(None) => {}
                }
            }
            let outcome = reader(self);
            let error = self
                .call
                .abort
                .take()
                .or_else(|| outcome.as_ref().err().cloned());
            if let Some(error) = error {
                return self.catch_load_error(scope, error);
            }
            let bytes = outcome.expect("reader outcome without error");
            self.call.vm.feed_native_load(index, bytes.as_deref());
            if first {
                first = false;
                if let Err(error) = self.check_gc() {
                    self.call.abort = None;
                    return self.catch_load_error(scope, error);
                }
            }
        }
    }
    /// Compile a fixed text/native buffer, preserving native loader statuses.
    ///
    /// # Errors
    /// Follows the pending-error, stack and quota rules of [`Self::load_reader`].
    pub fn load_buffer(
        &mut self,
        bytes: &[u8],
        name: Option<&[u8]>,
    ) -> Result<NativeLoadStatus, Error> {
        let mut bytes = Some(bytes.to_vec());
        self.load_reader(name, move |_| Ok(bytes.take()))
    }
    fn restore_load_base(&mut self, scope: Scope) {
        if matches!(
            self.call.vm.frames[self.frame].function,
            FrameKind::Native(CONT_NATIVE_WINDOW)
        ) {
            self.base = scope.call_base;
            self.call.output = self.base;
        }
    }
    fn finish_load_scope(&mut self, scope: Scope, end: usize, pending: bool) {
        let mut line = 0;
        self.call.vm.abort_threads_to(scope.owner, &mut line);
        self.call.vm.end_native_load(scope.depth);
        if pending {
            self.call.vm.frames[scope.depth].pc = host::API_YIELDED;
            self.call.vm.frames[scope.depth].vararg_count = 0;
        } else {
            self.call.vm.discard_finalizers(scope.depth);
            self.call.vm.frames.truncate(scope.depth);
        }
        self.call.vm.restore_calls();
        self.call.vm.trim_substitutions();
        self.call.vm.trim_sorts();
        self.call.vm.trim_loads();
        self.call.vm.trim_tracebacks();
        self.call.vm.trim_hooks();
        self.call.vm.top = if pending { end } else { scope.top };
        self.call.vm.active_native = scope.active;
        self.call.results = end.saturating_sub(self.base);
        self.call.vm.truncate_registers(end);
        self.sync();
    }
    fn catch_load_error(&mut self, scope: Scope, error: Error) -> Result<NativeLoadStatus, Error> {
        if error.kind() == ErrorKind::Limit {
            self.call
                .vm
                .set_thread_native_bias(scope.owner, scope.native_bias);
            self.restore_load_base(scope);
            self.finish_load_scope(scope, scope.base, false);
            self.call.abort = Some(error.clone());
            return Err(error);
        }
        let outcome = self
            .call
            .vm
            .drive_host(scope.owner, scope.depth, Err(error), OPEN);
        let (status, value) = match outcome {
            Ok(2) => {
                let status = if self.call.vm.registers[scope.base] == Value::Number(5.) {
                    NativeLoadStatus::ErrorHandler
                } else {
                    NativeLoadStatus::RuntimeError
                };
                (status, self.call.vm.registers[scope.base + 1])
            }
            Ok(_) => unreachable!("native parser error outcome"),
            Err(error) => {
                self.restore_load_base(scope);
                self.finish_load_scope(scope, scope.base, false);
                self.call.abort = Some(error.clone());
                return Err(error);
            }
        };
        self.restore_load_base(scope);
        self.finish_load_scope(scope, scope.base, false);
        self.set_top(scope.base - self.base)?;
        self.push(value)?;
        Ok(status)
    }
}
