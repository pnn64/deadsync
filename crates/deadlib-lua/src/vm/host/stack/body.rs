//! Direct execution of a builtin body inside an existing native activation.
use super::*;

/// A builtin's executable body, independent of the closure that supplied it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeBuiltinBody(u64);
impl NativeBuiltinBody {
    /// Opaque implementation code for native ABI adapters in this crate version.
    pub fn implementation_code(self) -> u64 {
        self.0
    }
    /// Decode an implementation code supplied by this version's native adapter.
    /// This does not create or retain a Lua closure.
    pub fn from_implementation_code(code: u64) -> Option<Self> {
        matches!(code, 0..=59 | 64..=78 | 80..=85 | 96..=104 | 112..=122 | 128..=170 | MATCH..=TRACEBACK)
            .then_some(Self(match code { 21 => 9, 139 => 128, _ => code }))
    }
}
pub(in crate::vm) const MATCH: u64 = 180;
pub(in crate::vm) const LINES: u64 = 181;
pub(in crate::vm) const WRAP: u64 = 182;
pub(in crate::vm) const MAIN: u64 = 183;
pub(in crate::vm) const TRACEBACK: u64 = 184;

#[derive(Debug)]
pub(in crate::vm) struct Body {
    thread: ThreadId,
    owner: usize,
    code: u64,
    closure: Value,
    visible_owner: Option<usize>,
    hook: Option<NativeHookEvent>,
    complete: Option<(usize, usize)>,
    yielded: Option<(usize, usize)>,
}

impl Vm {
    pub(in crate::vm) fn builtin_hook(&self, code: u64) -> Option<NativeHookEvent> {
        self.active_native.and_then(|owner| {
            self.native_bodies
                .iter()
                .rev()
                .find(|b| b.thread == self.thread && b.owner == owner && b.code == code)
                .and_then(|b| b.hook)
        })
    }
    pub(in crate::vm) fn body_code(&self, thread: ThreadId, index: usize) -> Option<u64> {
        self.native_bodies
            .iter()
            .rev()
            .find(|b| b.thread == thread && b.owner == index)
            .map(|b| b.code)
    }
    pub(in crate::vm) fn body_kind(&self, index: usize, kind: FrameKind) -> FrameKind {
        self.body_code(self.thread, index)
            .map_or(kind, FrameKind::Native)
    }
    pub(in crate::vm) fn builtin_closure(&self, code: u64) -> u64 {
        self.active_native
            .and_then(|owner| {
                self.native_bodies
                    .iter()
                    .rev()
                    .find(|b| b.thread == self.thread && b.owner == owner && b.code == code)
            })
            .and_then(|b| match b.closure {
                Value::Native(id) => Some(id),
                _ => None,
            })
            .unwrap_or(code)
    }
    pub(in crate::vm) fn builtin_environment(&self, code: u64) -> TableId {
        self.builtin_context(code)
            .and_then(|v| self.closure_environment(v))
            .unwrap_or_else(|| self.native_environment(code))
    }
    pub(in crate::vm) fn builtin_context(&self, code: u64) -> Option<Value> {
        self.active_native.and_then(|owner| {
            self.native_bodies
                .iter()
                .rev()
                .find(|b| b.thread == self.thread && b.owner == owner && b.code == code)
                .map(|b| b.closure)
        })
    }
    pub(in crate::vm) fn builtin_visible_owner(&self) -> Option<usize> {
        self.active_native.and_then(|owner| {
            self.native_bodies
                .iter()
                .rev()
                .find(|b| b.thread == self.thread && b.owner == owner)
                .and_then(|b| b.visible_owner)
        })
    }
    pub(in crate::vm) fn complete_native_body(
        &mut self,
        frame: Frame,
        source: usize,
        count: usize,
    ) -> bool {
        let index = self.frames.len() - 1;
        let Some(body) = self
            .native_bodies
            .last()
            .filter(|b| b.thread == self.thread && b.owner == index)
        else {
            return false;
        };
        let code = body.code;
        let mut frame = frame;
        let source = self.native_return_stack(code, &mut frame, source, count);
        self.frames[index] = frame;
        self.native_bodies.last_mut().unwrap().complete = Some((source, count));
        true
    }
    pub(in crate::vm) fn suspend_native_body(&mut self, index: usize) -> bool {
        let Some(threads::Transition::Yield { source, count }) = self.transition else {
            return false;
        };
        let Some(body) = self
            .native_bodies
            .last_mut()
            .filter(|b| b.thread == self.thread && b.owner == index)
        else {
            return false;
        };
        body.yielded = Some((source, count));
        true
    }
}

impl NativeStack<'_, '_> {
    pub(in crate::vm) fn call_close_body(
        &mut self,
        function: Value,
    ) -> Result<Option<usize>, Error> {
        if let Some(body) = self.builtin_body_of(function) {
            let count = self.call_builtin_body(body)?;
            if count >= 0 {
                return Ok(Some(count as usize));
            }
            let count = if self.call.hook.is_some() {
                0
            } else {
                self.top()
            };
            self.request_yield(count)?;
            return Ok(None);
        }
        let Value::Native(id) = function else {
            return Err(self.error(b"invalid file close handler"));
        };
        let body = self
            .call
            .vm
            .host_binding(id)
            .cloned()
            .ok_or_else(|| self.error(b"invalid file close handler"))?;
        let activation = self.context_function();
        let arguments = self.top();
        let output = self.call.vm.frames[self.frame].base;
        let mut context = HostCall {
            vm: self.call.vm,
            hook: self.call.hook,
            id: self.call.id,
            data: body.data,
            activation,
            arguments: self.base,
            count: arguments,
            output,
            results: 0,
            abort: None,
            stack_opened: false,
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (body.callback)(&mut context)
        }));
        let result = match result {
            Ok(result) => context.abort.take().map_or(result, Err),
            Err(payload) => {
                context.vm.gc.closing = true;
                std::panic::resume_unwind(payload)
            }
        };
        let output = context.output;
        let count = context.results;
        let yielded = context.vm.frames[self.frame].pc == threads::HOST_YIELDED
            || (context.hook.is_some() && context.vm.hooks.yielding);
        let frame = context.vm.frames[self.frame];
        self.base = frame.vararg_start;
        self.call.output = self.base;
        self.call.results = frame.argc();
        result?;
        if yielded {
            self.call.output = output;
            self.call.results = count;
            self.finished = true;
            Ok(None)
        } else {
            Ok(Some(count))
        }
    }

    /// Read a builtin body without retaining or calling its closure.
    /// Foreign host functions retain their own binding/function-pointer APIs.
    pub fn builtin_body(&self, index: isize) -> Option<NativeBuiltinBody> {
        self.builtin_body_of(self.value(index)?)
    }
    /// Inspect an already rooted value's builtin implementation.
    pub fn builtin_body_of(&self, function: Value) -> Option<NativeBuiltinBody> {
        let Value::Native(id) = function else {
            return None;
        };
        let code = if id < CONT_RAW {
            id
        } else {
            match self.call.vm.native_closures.get(id)?.kind {
                threads::NativeKind::Builtin(code) => code,
                threads::NativeKind::Match => MATCH,
                threads::NativeKind::Lines => LINES,
                threads::NativeKind::Wrap => WRAP,
                threads::NativeKind::StandaloneMain => MAIN,
                threads::NativeKind::StandaloneTraceback => TRACEBACK,
                _ => return None,
            }
        };
        NativeBuiltinBody::from_implementation_code(code)
    }

    /// Execute a builtin body in this activation, with its current closure and
    /// mutable stack. No extra call frame or body call/return hook is delivered.
    /// The entire resulting C window replaces this stack. The returned count
    /// selects its result suffix; a negative count requests a native yield.
    ///
    /// # Errors
    /// Propagates errors while retaining faulting children. Yielding from a hook
    /// follows the ordinary zero-result line/count-hook restriction.
    pub fn call_builtin_body(&mut self, body: NativeBuiltinBody) -> Result<isize, Error> {
        self.check_error()?;
        if body.0 == threads::CO_YIELD {
            self.check_yield(self.top())?;
        }
        let closure = self.context_function().unwrap_or(Value::Nil);
        let visible_owner = self.call.vm.frames[..=self.frame]
            .iter()
            .rposition(|f| f.is_call());
        let vm = &mut self.call.vm;
        let original = vm.frames[self.frame];
        let workspace = vm.registers.len();
        let arguments = if self.call.hook.is_none() {
            vm.registers.push(closure);
            vm.registers
                .extend_from_within(self.base..self.base + self.call.results);
            vm.relocate_native_error_functions(
                self.base..self.base + self.call.results,
                workspace + 1,
            );
            vm.registers[self.base..self.base + self.call.results].fill(Value::Nil);
            workspace + 1
        } else {
            self.base
        };
        let control = vm.registers.len();
        vm.resize_registers(control + self.call.results.max(2) + 4);
        vm.frames[self.frame] = Frame {
            base: control,
            vararg_start: arguments,
            pc: 0,
            ..original
        };
        let capacity = vm.native_bodies.capacity();
        vm.native_bodies.push(Body {
            thread: vm.thread,
            owner: self.frame,
            code: body.0,
            closure,
            visible_owner,
            hook: self.call.hook,
            complete: None,
            yielded: None,
        });
        vm.gc_note_control((vm.native_bodies.capacity() - capacity) * size_of::<Body>());
        let owner = vm.thread;
        let depth = self.frame;
        let top = vm.top;
        let mut line = 0;
        let result = loop {
            let result = vm.run(&mut line);
            let context = vm.native_bodies.last().unwrap();
            if context.complete.is_some() || context.yielded.is_some() {
                break Ok(());
            }
            match result {
                Ok(count) if vm.thread != owner => {
                    vm.finish_thread(0, count, false, false, &mut line);
                }
                Err(error) if error.kind() == ErrorKind::Runtime => {
                    let floor = if vm.thread == owner { depth } else { 0 };
                    match vm.catch_error_from(&error, floor) {
                        Ok(true) => {}
                        Ok(false) if vm.thread != owner => vm.fail_thread(&error, &mut line),
                        Ok(false) => break Err(error.at_line(line)),
                        Err(error) => break Err(error.at_line(line)),
                    }
                }
                Err(error) => break Err(error.at_line(line)),
                Ok(_) => {
                    break Err(Error::new(
                        ErrorKind::Runtime,
                        0,
                        "invalid builtin body boundary",
                    ));
                }
            }
        };
        let context = vm.native_bodies.pop().unwrap();
        if result.is_err() {
            vm.abort_threads_to(owner, &mut line);
        }
        vm.active_native = Some(depth);
        vm.top = top;
        if let Err(error) = result {
            if vm.thread == owner && vm.frames.len() > depth {
                let fault = vm.frames[depth];
                vm.frames[depth] = Frame {
                    vararg_start: fault.vararg_start,
                    vararg_count: fault.vararg_count,
                    ..original
                };
                self.call.results = fault.argc();
                self.base = fault.vararg_start;
                self.call.output = self.base;
            }
            self.call.abort = Some(error.clone());
            return Err(error);
        }
        let frame = vm.frames[depth];
        let (start, len, count, yielded) = if let Some((source, count)) = context.yielded {
            vm.transition = None;
            let start = frame.vararg_start.min(source);
            let end = (frame.vararg_start + frame.argc()).max(source + count);
            (start, end - start, count, Some(source - start))
        } else {
            let (_, count) = context.complete.unwrap();
            (frame.vararg_start, frame.argc(), count, None)
        };
        vm.relocate_native_error_functions(start..start + len, self.base);
        vm.registers.copy_within(start..start + len, self.base);
        vm.truncate_registers(self.base + len);
        vm.frames[depth] = original;
        self.call.results = len;
        self.sync();
        if let Some(offset) = yielded {
            // lua_yield exposes a suffix while ci->base still owns the prefix.
            // The C wrapper returns immediately, then the ordinary native
            // yield path publishes this suffix without discarding that prefix.
            self.base += offset;
            self.call.output = self.base;
            self.call.results = count;
            Ok(-1)
        } else {
            Ok(count as isize)
        }
    }
}
