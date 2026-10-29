//! Shared-heap Lua threads with vector context switches and an explicit scheduler.
use super::*;

pub(super) const CO_CREATE: u64 = 80;
pub(super) const CO_RESUME: u64 = 81;
pub(super) const CO_RUNNING: u64 = 82;
pub(super) const CO_STATUS: u64 = 83;
pub(super) const CO_WRAP: u64 = 84;
pub(super) const CO_YIELD: u64 = 85;
pub(super) const HOST_YIELDED: u32 = u32::MAX;
pub(super) const HOST_RESUME_RETURNED: u32 = u32::MAX - 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Status {
    Running,
    Suspended,
    Normal,
    Dead,
}

#[cfg(test)]
mod waiting_error_tests {
    use super::*;
    fn current(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        s.push_thread()?;
        s.finish(1)
    }
    fn raise_waiting(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let thread = s.to_thread(1).unwrap();
        let value = s.value(2).unwrap();
        s.thread_stack(thread)?.push(value)?;
        let mut error = s.raise(value);
        error.target_thread = Some(thread);
        Err(error)
    }
    fn ignore_nested_error(call: &mut HostCall<'_>) -> Result<(), Error> {
        let function = call.arguments()[0];
        let _ = call.call(function, &[]);
        Ok(())
    }
    fn setup() -> Vm {
        let mut vm = Vm::new();
        for (name, callback) in [
            (b"current".as_slice(), current as HostFunction),
            (b"raise_waiting", raise_waiting),
            (b"ignore_nested_error", ignore_nested_error),
        ] {
            let f = vm.new_host_function(callback, &[]).unwrap();
            vm.set_global(name, f).unwrap();
        }
        vm
    }
    #[test]
    fn a_rust_protected_call_on_the_wrong_thread_cannot_swallow_a_nonlocal_error() {
        let mut vm = setup();
        vm.execute(&crate::compile(b"local parent=current();local token={};local after=false;local child=coroutine.create(function()ignore_nested_error(function()raise_waiting(parent,token)end);after=true end);local ok,e=pcall(function()return coroutine.resume(child)end);assert(not ok and e==token and not after and coroutine.status(child)=='normal');local weak=setmetatable({child},{__mode='v'});child=nil;collectgarbage();collectgarbage();assert(weak[1]==nil)").unwrap()).unwrap();
    }
    #[test]
    fn public_execution_errors_do_not_retain_an_internal_thread_destination() {
        let mut vm = setup();
        let e=vm.execute(&crate::compile(b"marker={};local parent=current();child=coroutine.create(function()raise_waiting(parent,marker)end);return coroutine.resume(child)").unwrap()).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Runtime);
        assert_eq!(e.target_thread, None);
        assert_eq!(e.value(), Some(vm.get_global(b"marker").unwrap()));
        assert_eq!(vm.execute(&crate::compile(b"assert(coroutine.status(child)=='normal');child=nil;collectgarbage();return 42").unwrap()).unwrap(),&[Value::Number(42.)]);
    }
}
/// Lua 5.1 execution status retained by a thread, separate from coroutine.status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NativeThreadStatus {
    /// Fresh, running, normal or successfully completed execution.
    Ok = 0,
    /// Execution is suspended at a yield.
    Yielded = 1,
    /// The coroutine failed with an unprotected runtime error.
    RuntimeError = 2,
    /// Execution failed while processing an error.
    ErrorHandler = 5,
}
impl Status {
    fn name(self) -> &'static [u8] {
        match self {
            Self::Running => b"running",
            Self::Suspended => b"suspended",
            Self::Normal => b"normal",
            Self::Dead => b"dead",
        }
    }
}

#[derive(Debug)]
pub(super) enum NativeKind {
    Builtin(u64),
    Match,
    Wrap,
    Lines,
    Host(host::Function),
    StandaloneMain,
    StandaloneTraceback,
}
#[derive(Debug)]
pub(super) struct NativeClosure {
    pub env: TableId,
    pub kind: NativeKind,
    pub captures: Box<[Value]>,
}

#[derive(Debug)]
pub(super) enum Transition {
    Resume(ThreadId),
    Yield { source: usize, count: usize },
    HookYield { base: usize, top: usize },
}

#[derive(Debug)]
pub(super) struct Thread {
    pub(super) registers: Vec<Value>,
    pub(super) frames: Vec<Frame>,
    pub(super) call_slots: u16,
    pub(super) open_upvalues: Vec<u32>,
    pub(super) globals: TableId,
    pub(super) top: usize,
    pub(super) active_native: Option<usize>,
    pub(super) substitutions: Vec<strings::Substitution>,
    pub(super) sorts: Vec<tables::Sort>,
    pub(super) loads: Vec<loads::Loading>,
    pub(super) tracebacks: Vec<traces::Traceback>,
    pub(super) hooks: hooks::Hooks,
    pub(super) entry: Option<(usize, usize)>,
    pub(super) api_stack: std::ops::Range<usize>,
    pub(super) native_resumer: bool,
    pub(super) native_level: u16,
    pub(super) abandoned: bool,
    pub(super) native_bias: u16,
    pub(super) native_base_calls: u16,
    pub(super) resumed_values: Option<std::ops::Range<usize>>,
    pub(super) status: Status,
    pub(super) resume_status: NativeThreadStatus,
    pub(super) resumer: Option<ThreadId>,
    pub(super) depth: u16,
    pub(super) line: u32,
    pub(super) host_states: Vec<host::ThreadData>,
}
impl Thread {
    pub fn main(globals: TableId) -> Self {
        Self {
            registers: Vec::new(),
            frames: Vec::new(),
            call_slots: 8,
            open_upvalues: Vec::new(),
            globals,
            top: 0,
            active_native: None,
            substitutions: Vec::new(),
            sorts: Vec::new(),
            loads: Vec::new(),
            tracebacks: Vec::new(),
            hooks: hooks::Hooks::default(),
            entry: None,
            api_stack: 0..0,
            native_resumer: false,
            native_level: 0,
            abandoned: false,
            native_bias: 0,
            native_base_calls: 0,
            resumed_values: None,
            status: Status::Running,
            resume_status: NativeThreadStatus::Ok,
            resumer: None,
            depth: 0,
            line: 0,
            host_states: Vec::new(),
        }
    }
    fn new(entry: FunctionId, globals: TableId) -> Self {
        Self {
            registers: vec![Value::Function(entry)],
            top: 1,
            api_stack: 0..1,
            globals,
            status: Status::Suspended,
            ..Self::main(globals)
        }
    }
}

impl Vm {
    pub(super) fn calling_function(&self, thread: ThreadId) -> Option<Value> {
        // Scheduler continuations do not replace Lua's current call identity.
        self.debug_frames(thread)
            .iter()
            .rev()
            .find(|f| f.is_call())
            .map(|frame| match frame.function {
                FrameKind::Lua(id) => Value::Function(id),
                FrameKind::Native(id) => Value::Native(id),
            })
    }
    pub(super) fn closure_environment(&self, function: Value) -> Option<TableId> {
        match function {
            Value::Function(id) => Some(self.functions[slot_index(id.0)].env),
            Value::Native(id) => Some(self.native_environment(id)),
            _ => None,
        }
    }
    pub(super) fn set_closure_environment(&mut self, function: Value, env: TableId) {
        match function {
            Value::Function(id) => {
                self.functions[slot_index(id.0)].env = env;
                self.gc_barrier(function, Value::Table(env));
            }
            Value::Native(id) => self.set_native_environment(id, env),
            _ => unreachable!("validated calling function"),
        }
    }
    pub(super) fn calling_environment(&self, thread: ThreadId) -> TableId {
        self.calling_function(thread)
            .and_then(|f| self.closure_environment(f))
            .unwrap_or_else(|| self.thread_environment(thread))
    }
    pub(super) fn allocate_thread(&mut self) -> ThreadId {
        let id = ThreadId(self.threads.insert(Thread {
            status: Status::Suspended,
            hooks: self.hooks.inherited(),
            ..Thread::main(self.globals)
        }));
        self.gc_new_value(Value::Thread(id));
        id
    }
    pub(super) fn coroutine_status(&self, id: ThreadId) -> Status {
        if id == self.thread {
            return Status::Running;
        }
        let state = &self.threads[slot_index(id.0)];
        if state.resume_status == NativeThreadStatus::Ok
            && state.frames.is_empty()
            && state.status != Status::Normal
        {
            if state.api_stack.is_empty() {
                Status::Dead
            } else {
                Status::Suspended
            }
        } else {
            state.status
        }
    }
    pub(super) fn open_threads(&mut self) {
        let table = self.new_table();
        for (i, name) in [
            b"create".as_slice(),
            b"resume",
            b"running",
            b"status",
            b"wrap",
            b"yield",
        ]
        .iter()
        .enumerate()
        {
            let name = self.intern(name);
            self.tables[slot_index(table.0)]
                .set(name, Value::Native(CO_CREATE + i as u64))
                .expect("coroutine names are valid");
        }
        self.set_global(b"coroutine", Value::Table(table))
            .expect("coroutine name is valid");
    }
    pub(super) fn new_native(
        &mut self,
        kind: NativeKind,
        captures: &[Value],
    ) -> Result<Value, Error> {
        let env = self.calling_environment(self.thread);
        self.native_closures
            .note_alloc(std::mem::size_of_val(captures));
        let id = self.native_closures.insert(NativeClosure {
            env,
            kind,
            captures: captures.into(),
        });
        self.gc_new_value(Value::Native(id));
        Ok(Value::Native(id))
    }
    pub(super) fn scheduled_native(&self, id: u64) -> u8 {
        match id {
            host::stack::body::WRAP => return 1,
            host::stack::body::MAIN => return 2,
            host::stack::body::TRACEBACK => return 3,
            _ => {}
        }
        if id < strings::ITERATOR_FIRST {
            return 0;
        }
        match self.native_closures.get(id).map(|c| &c.kind) {
            Some(NativeKind::Wrap) => 1,
            Some(NativeKind::StandaloneMain) => 2,
            Some(NativeKind::StandaloneTraceback) => 3,
            _ => 0,
        }
    }
    pub(super) fn native_environment(&self, id: u64) -> TableId {
        if id >= strings::ITERATOR_FIRST {
            self.native_closures[slot_index(id)].env
        } else {
            self.native_envs
                .get(&id)
                .copied()
                .unwrap_or(self.initial_globals)
        }
    }
    pub(super) fn set_native_environment(&mut self, id: u64, env: TableId) {
        if id >= strings::ITERATOR_FIRST {
            self.native_closures[slot_index(id)].env = env;
        } else {
            self.native_envs.insert(id, env);
        }
        self.gc_barrier(Value::Native(id), Value::Table(env));
    }
    pub(super) fn thread_environment(&self, id: ThreadId) -> TableId {
        if id == self.thread {
            self.globals
        } else {
            self.threads[slot_index(id.0)].globals
        }
    }
    pub(super) fn set_thread_environment(&mut self, id: ThreadId, env: TableId) {
        if id == self.thread {
            self.globals = env;
        } else {
            self.threads[slot_index(id.0)].globals = env;
        }
        self.gc_barrier(Value::Thread(id), Value::Table(env));
    }
    pub(super) fn cell_value(&self, id: usize) -> Value {
        match self.upvalues[id] {
            Cell::Closed(value) => value,
            Cell::Open(thread, register) => {
                if thread == self.thread {
                    self.registers[register]
                } else {
                    self.threads[slot_index(thread.0)].registers[register]
                }
            }
        }
    }
    pub(super) fn set_cell(&mut self, id: usize, value: Value) {
        match self.upvalues[id] {
            Cell::Closed(_) => self.upvalues[id] = Cell::Closed(value),
            Cell::Open(thread, register) => {
                if thread == self.thread {
                    self.registers[register] = value;
                } else {
                    self.threads[slot_index(thread.0)].registers[register] = value;
                }
            }
        }
        self.gc_cell_barrier(id, value);
    }
    // Active vectors stay directly in Vm for ordinary dispatch. Switching swaps
    // ownership with the inactive thread slots; it never copies stack contents.
    fn exchange_thread(&mut self, id: ThreadId, line: &mut u32) {
        let state = &mut self.threads[slot_index(id.0)];
        std::mem::swap(&mut self.registers, &mut state.registers);
        std::mem::swap(&mut self.frames, &mut state.frames);
        std::mem::swap(&mut self.call_slots, &mut state.call_slots);
        std::mem::swap(&mut self.open_upvalues, &mut state.open_upvalues);
        std::mem::swap(&mut self.globals, &mut state.globals);
        std::mem::swap(&mut self.top, &mut state.top);
        std::mem::swap(&mut self.active_native, &mut state.active_native);
        std::mem::swap(&mut self.substitutions, &mut state.substitutions);
        std::mem::swap(&mut self.sorts, &mut state.sorts);
        std::mem::swap(&mut self.loads, &mut state.loads);
        std::mem::swap(&mut self.tracebacks, &mut state.tracebacks);
        std::mem::swap(&mut self.hooks, &mut state.hooks);
        std::mem::swap(line, &mut state.line);
    }
    pub(super) fn switch_thread(&mut self, id: ThreadId, line: &mut u32) {
        self.exchange_thread(self.thread, line);
        self.exchange_thread(id, line);
        self.thread = id;
    }
    pub(super) fn apply_transition(&mut self, line: &mut u32) -> Result<Option<usize>, Error> {
        match self.transition.take() {
            None => {}
            Some(Transition::Resume(id)) => {
                self.switch_thread(id, line);
                if let Some((call, count)) = self.threads[slot_index(id.0)].entry.take()
                    && let Some(actual) = self.start_lua_call(call, count, OPEN, false)?
                {
                    self.top = call + actual;
                    return Ok(Some(self.top));
                }
            }
            Some(Transition::Yield { source, count }) => {
                return Ok(self.yield_thread(source..source + count, line));
            }
            Some(Transition::HookYield { base, top }) => {
                return Ok(self.yield_thread(base..top, line));
            }
        }
        Ok(None)
    }
    fn yield_thread(&mut self, range: std::ops::Range<usize>, line: &mut u32) -> Option<usize> {
        if let Some(scope) = self
            .api_calls
            .iter_mut()
            .rev()
            .find(|s| s.thread == self.thread)
        {
            scope.yielded = Some(range.clone());
            let state = &mut self.threads[slot_index(self.thread.0)];
            state.resume_status = NativeThreadStatus::Yielded;
            state.status = Status::Suspended;
            self.top = range.end;
            state.api_stack = range;
            return Some(0);
        }
        self.finish_thread_range(range, false, true, line)
            .then_some(0)
    }
    pub(super) fn finish_thread(
        &mut self,
        source: usize,
        count: usize,
        failed: bool,
        suspended: bool,
        line: &mut u32,
    ) -> bool {
        self.finish_thread_range(source..source + count, failed, suspended, line)
    }
    fn finish_thread_range(
        &mut self,
        values: std::ops::Range<usize>,
        failed: bool,
        suspended: bool,
        line: &mut u32,
    ) -> bool {
        let source = values.start;
        let inverted = values.end < values.start;
        let count = if inverted { 1 } else { values.len() };
        let id = self.thread;
        let host = self.frames.last().is_some_and(
            |f| matches!(f.function, FrameKind::Native(id) if self.host_callback(id).is_some()),
        );
        let level = self
            .native_calls_for(id, self.frames.len())
            .wrapping_sub(u16::from(host))
            .wrapping_sub(1);
        self.threads[slot_index(id.0)].native_level = level;
        let parent = self.threads[slot_index(id.0)]
            .resumer
            .take()
            .expect("resumed coroutine has a parent");
        if std::mem::take(&mut self.threads[slot_index(id.0)].native_resumer) {
            let state = &mut self.threads[slot_index(id.0)];
            state.status = if suspended {
                Status::Suspended
            } else {
                Status::Dead
            };
            state.resume_status = if failed {
                state.resume_status
            } else if suspended {
                NativeThreadStatus::Yielded
            } else {
                NativeThreadStatus::Ok
            };
            let base = if suspended {
                source
            } else if failed {
                state.api_stack.start
            } else {
                0
            };
            state.api_stack = base..values.end;
            self.top = values.end;
            self.threads[slot_index(parent.0)].status = Status::Running;
            self.switch_thread(parent, line);
            return true;
        }
        let caller_index = self.threads[slot_index(parent.0)].frames.len() - 1;
        let body = self.body_code(parent, caller_index);
        let parent_function = if inverted {
            self.calling_function(parent).unwrap_or(Value::Nil)
        } else {
            Value::Nil
        };
        let caller = &mut self.threads[slot_index(parent.0)];
        let frame = caller
            .frames
            .last_mut()
            .expect("resumer has an awaiting frame");
        let prefix = usize::from(
            body == Some(CO_RESUME) || matches!(frame.function, FrameKind::Native(CO_RESUME)),
        );
        let base = frame.base;
        let arguments = frame.argc();
        caller.registers.resize(
            caller
                .registers
                .len()
                .max(base + arguments + prefix + count),
            Value::Nil,
        );
        if prefix != 0 {
            caller.registers[base + arguments] = Value::Boolean(!failed && !inverted);
        }
        if inverted {
            // auxresume passes the signed offset to lua_xmove. A negative
            // count advances the source top and copies no values; coresume
            // then treats that offset as its failure flag. Its remaining C
            // slot is the thread argument (or the wrap closure below base).
            caller.registers[base + arguments + prefix] = if prefix != 0 {
                Value::Thread(id)
            } else {
                parent_function
            };
        } else {
            caller.registers[base + arguments + prefix..base + arguments + prefix + count]
                .copy_from_slice(&self.registers[values]);
        }
        caller.top = base + arguments + prefix + count;
        frame.vararg_start = base;
        frame.vararg_count = (arguments + prefix + count) as u32;
        // Low two bits select the continuation; higher bits retain the C
        // argument prefix below its results, including return-hook aliases.
        frame.pc = ((arguments as u32) << 2) | if failed || inverted { 3 } else { 2 };
        caller.status = Status::Running;
        self.threads[slot_index(id.0)].status = if suspended {
            Status::Suspended
        } else {
            Status::Dead
        };
        self.threads[slot_index(id.0)].resume_status = if failed {
            self.threads[slot_index(id.0)].resume_status
        } else if suspended {
            NativeThreadStatus::Yielded
        } else {
            NativeThreadStatus::Ok
        };
        if suspended {
            self.threads[slot_index(id.0)].api_stack = source..source;
            self.top = source;
        } else if !failed {
            self.threads[slot_index(id.0)].api_stack = 0..0;
            self.top = 0;
        }
        if !suspended && !failed {
            self.registers.clear();
        }
        self.switch_thread(parent, line);
        false
    }
    pub(super) fn fail_thread(&mut self, error: &Error, line: &mut u32) {
        self.threads[slot_index(self.thread.0)].resume_status = if error.bypass_handler {
            NativeThreadStatus::ErrorHandler
        } else {
            NativeThreadStatus::RuntimeError
        };
        self.discard_finalizers(0);
        let value = error.value().unwrap_or_else(|| {
            let value = self.intern(error.message().as_bytes());
            self.located_error(value, 1)
        });
        if let Some(frame) = self.frames.last().copied() {
            let (base, values) = match frame.function {
                FrameKind::Native(id) => {
                    let base = frame.vararg_start;
                    let count = if id == 1 {
                        frame.argc().min(1)
                    } else {
                        frame.argc()
                    };
                    let mut values = self.registers[base..base + count].to_vec();
                    if self.scheduled_native(id) != 1
                        && (id != 1 || values.last().copied() != Some(value))
                    {
                        values.push(value);
                    }
                    (base, values)
                }
                FrameKind::Lua(id) => {
                    let f = &self.functions[slot_index(id.0)];
                    let count =
                        self.programs[f.program].data.prototypes[f.prototype].debug_registers();
                    let mut values = self.registers[frame.base..frame.base + count].to_vec();
                    values.push(self.intern(error.message_bytes()));
                    values.push(value);
                    (frame.base, values)
                }
            };
            self.resize_registers(self.registers.len().max(base + values.len()));
            self.registers[base..base + values.len()].copy_from_slice(&values);
            self.top = base + values.len();
            self.threads[slot_index(self.thread.0)].api_stack = base..self.top;
            if matches!(frame.function, FrameKind::Native(_)) {
                let f = self.frames.last_mut().unwrap();
                f.base = base;
                f.stack_start = base;
                f.vararg_count = values.len() as u32;
            }
        } else {
            // A rejected entry has no call frame, but its raw diagnostic is
            // still on the base stack before lua_resume adds the error copy.
            self.registers.push(value);
            self.top = self.registers.len();
            self.threads[slot_index(self.thread.0)].api_stack = 0..self.top;
        }
        let source = self.top;
        self.truncate_registers(source);
        self.registers.push(value);
        // Keep faulting frames and open cells for debug inspection and escaped
        // closures. A dead thread cannot execute again or overwrite those cells.
        self.finish_thread(source, 1, true, false, line);
    }
    pub(super) fn abort_threads(&mut self, line: &mut u32) {
        self.abort_threads_to(self.main, line);
    }
    pub(super) fn leave_threads_to(
        &mut self,
        target: ThreadId,
        line: &mut u32,
    ) -> Result<(), Error> {
        // A jump to another state abandons these invocations. Their frames and
        // open cells remain inspectable until the threads become unreachable.
        self.transition = None;
        while self.thread != target {
            let id = self.thread;
            let host=self.frames.last().is_some_and(|frame| {
                frame.pc!=HOST_RESUME_RETURNED && matches!(frame.function,FrameKind::Native(id)if self.host_callback(id).is_some())
            });
            let level = self
                .native_calls_for(id, self.frames.len())
                .wrapping_sub(u16::from(host));
            let state = &mut self.threads[slot_index(id.0)];
            let parent = state.resumer.take().ok_or_else(|| {
                Error::new(ErrorKind::Runtime, 0, "invalid nonlocal error destination")
            })?;
            state.status = Status::Normal;
            state.abandoned = true;
            state.native_level = level;
            state.native_resumer = false;
            state.resumed_values = None;
            self.switch_thread(parent, line);
        }
        // A panic callback can reset this waiting owner before an error leaves
        // its child. Restoring the Rust invocation must not revive that state.
        let state = &mut self.threads[slot_index(target.0)];
        if state.status != Status::Dead || !self.frames.is_empty() {
            state.status = Status::Running;
        }
        Ok(())
    }
    pub(super) fn abort_threads_to(&mut self, target: ThreadId, line: &mut u32) {
        self.transition = None;
        while self.thread != target {
            let id = self.thread;
            let parent = self.threads[slot_index(id.0)]
                .resumer
                .take()
                .expect("active thread has a resumer");
            self.close_upvalues(0);
            self.discard_finalizers(0);
            self.frames.clear();
            self.restore_calls();
            self.trim_substitutions();
            self.sorts.clear();
            self.trim_loads();
            self.trim_tracebacks();
            self.trim_hooks();
            self.registers.clear();
            self.top = 0;
            self.threads[slot_index(id.0)].api_stack = 0..0;
            self.threads[slot_index(id.0)].native_resumer = false;
            self.threads[slot_index(id.0)].resumed_values = None;
            self.threads[slot_index(id.0)].status = Status::Dead;
            self.threads[slot_index(id.0)].resume_status = NativeThreadStatus::RuntimeError;
            self.switch_thread(parent, line);
        }
        self.threads[slot_index(target.0)].status = Status::Running;
    }
    pub(super) fn coroutine_native(
        &mut self,
        id: u64,
        base: usize,
        count: usize,
    ) -> Result<usize, Error> {
        let result = match id {
            CO_CREATE | CO_WRAP => {
                let Value::Function(entry) = self.argument(base, count, 0) else {
                    return Err(self.argument_error(0, b"Lua function expected"));
                };
                let thread = ThreadId(self.threads.insert(Thread::new(entry, self.globals)));
                self.threads.note_alloc(size_of::<Value>());
                self.threads[slot_index(thread.0)].hooks = self.hooks.inherited();
                self.gc_new_value(Value::Thread(thread));
                if id == CO_WRAP {
                    self.new_native(NativeKind::Wrap, &[Value::Thread(thread)])?
                } else {
                    Value::Thread(thread)
                }
            }
            CO_RUNNING => {
                if self.thread == self.main {
                    Value::Nil
                } else {
                    Value::Thread(self.thread)
                }
            }
            CO_STATUS => {
                let Value::Thread(thread) = self.argument(base, count, 0) else {
                    return Err(self.argument_error(0, b"coroutine expected"));
                };
                let status = self.coroutine_status(thread);
                self.intern(status.name())
            }
            _ => unreachable!("coroutine continuations are scheduled"),
        };
        self.registers[base] = result;
        Ok(1)
    }
    pub(super) fn native_depth(&self, end: usize) -> usize {
        usize::from(self.native_calls_for(self.thread, end).wrapping_sub(1))
    }
    pub(super) fn native_calls_for(&self, thread: ThreadId, end: usize) -> u16 {
        let frames = self.debug_frames(thread);
        let iterators = frames[..end]
            .windows(2)
            .filter(|pair| {
                if pair[1].barriers <= pair[0].barriers {
                    return false;
                }
                let FrameKind::Lua(id) = pair[0].function else {
                    return false;
                };
                let closure = &self.functions[slot_index(id.0)];
                let prototype = &self.programs[closure.program].data.prototypes[closure.prototype];
                pair[0]
                    .pc
                    .checked_sub(1)
                    .and_then(|pc| prototype.code.get(pc as usize))
                    .is_some_and(|i| i.op == Op::CallIter)
            })
            .count();
        let boundaries = frames[..end]
            .iter().enumerate()
            .filter(|(index, f)| {
                if f.pc == host::API_YIELDED && matches!(f.function, FrameKind::Native(CONT_RAW | CONT_BOOL | CONT_NOT | CONT_IGNORE)) {
                    return false;
                }
                if frames.get(index + 1).is_some_and(|next| {
                    matches!(next.function, FrameKind::Native(CONT_RETURN))
                        || (matches!(next.function, FrameKind::Native(CONT_HOOK | CONT_NATIVE_HOOK)) && f.pc == 0)
                }) { return false; }
                // The standalone outer main is the implicit root C call,
                // already accounted for by the depth-limit convention.
                let host = f.pc!=HOST_RESUME_RETURNED && matches!(f.function, FrameKind::Native(id) if self.host_callback(id).is_some())
                    && !frames[index + 1..end].iter()
                        .take_while(|next| !next.is_call())
                        .any(|next| matches!(next.function, FrameKind::Native(CONT_GC)));
                let hook_call = matches!(f.function, FrameKind::Native(CONT_NATIVE_HOOK))
                    && frames[index + 1..end].iter().take_while(|next| !next.is_call()).any(|next| {
                        matches!(next.function, FrameKind::Native(CONT_HOST_META))
                            || (matches!(next.function, FrameKind::Native(CONT_HOST_CALL | CONT_HOST_PCALL)) && next.wanted == FRAME_OPEN)
                    });
                let kind = self.body_code(thread, *index).map_or(f.function, FrameKind::Native);
                matches!(
                    kind,
                    FrameKind::Native(
                        PCALL
                            | debug::CONSOLE
                            | XPCALL
                            | TOSTRING
                            | PRINT
                            | DO_FILE
                            | package::REQUIRE
                            | package::MODULE
                            | strings::STR_GSUB
                            | tables::TAB_FOREACH
                            | tables::TAB_FOREACHI
                            | tables::TAB_SORT
                            | CONT_RAW
                            | CONT_BOOL
                            | CONT_NOT
                            | CONT_IGNORE
                            | CONT_HANDLER
                            | CONT_GC
                            | CONT_HOOK
                            | CONT_STANDALONE
                    )
                ) || (matches!(f.function, FrameKind::Native(id) if self.scheduled_native(id) == 3))
                    || host || hook_call
                    || (loads::protected(kind)
                        && !matches!(kind, FrameKind::Native(CONT_HOST_LOAD))
                        && matches!(f.pc, 1 | 3))
            })
            .count();
        ((usize::from(self.threads[slot_index(thread.0)].depth)
            + boundaries
            + iterators
            + self.parser_nesting_for(thread, end)
            + 1) as u16)
            .wrapping_add(self.threads[slot_index(thread.0)].native_bias)
    }

    pub(super) fn thread_call_level(&self, thread: ThreadId) -> u16 {
        let state = &self.threads[slot_index(thread.0)];
        if thread != self.thread && (state.status != Status::Normal || state.abandoned) {
            return state.native_level;
        }
        let (frames, active) = if thread == self.thread {
            (&self.frames, self.active_native)
        } else {
            (&state.frames, state.active_native)
        };
        let host = active.and_then(|i| frames.get(i)).is_some_and(|f| {
            f.pc != HOST_RESUME_RETURNED
                && matches!(f.function, FrameKind::Native(id) if self.host_callback(id).is_some())
        });
        self.native_calls_for(thread, frames.len())
            .wrapping_sub(u16::from(host))
    }

    pub(super) fn set_thread_native_bias(&mut self, thread: ThreadId, bias: u16) {
        let state = &mut self.threads[slot_index(thread.0)];
        let change = bias.wrapping_sub(state.native_bias);
        state.native_bias = bias;
        let loads = if thread == self.thread {
            &mut self.loads
        } else {
            &mut state.loads
        };
        for job in loads {
            job.shift_native_depth(change);
        }
    }
    pub(super) fn resume_api_barriers(&mut self, thread: ThreadId) {
        let frames = &mut self.threads[slot_index(thread.0)].frames;
        if let Some(index) = frames.iter().rposition(|frame| {
            frame.pc == host::API_YIELDED
                && matches!(
                    frame.function,
                    FrameKind::Native(
                        CONT_HOST_CALL | CONT_HOST_PCALL | CONT_HOST_LOAD | CONT_NATIVE_WINDOW
                    )
                )
        }) {
            let barriers = frames
                .get(index + 1)
                .filter(|frame| {
                    frame.pc == host::API_YIELDED
                        && matches!(
                            frame.function,
                            FrameKind::Native(CONT_RAW | CONT_BOOL | CONT_NOT | CONT_IGNORE)
                        )
                })
                .map_or(frames[index].barriers, |frame| frame.barriers);
            // The API's C invocation has already returned. Resumed Lua keeps
            // only boundaries introduced above that escaped invocation.
            for frame in &mut frames[index..] {
                frame.barriers = frame.barriers.saturating_sub(barriers);
            }
        }
    }
    pub(super) fn take_resume_values(
        &mut self,
        frame: Frame,
    ) -> Result<std::ops::Range<usize>, Error> {
        let values = self.threads[slot_index(self.thread.0)]
            .resumed_values
            .take();
        let Some(values) = values.filter(|values| {
            self.top >= frame.vararg_start
                && values.start <= values.end
                && values.end <= self.registers.len()
        }) else {
            // An API can leave multiple native yields pending on one thread.
            // Only its topmost yielded call receives resume arguments. Reaching
            // an older call without a new resume is an invalid continuation.
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                "invalid coroutine resume state",
            ));
        };
        Ok(values)
    }
    pub(super) fn coroutine_step(&mut self, id: u64, frame: Frame) -> Result<Option<usize>, Error> {
        let base = frame.base;
        let owner = self.frames.len() - 1;
        if id == CO_YIELD {
            if frame.pc() == 0 {
                if self.thread == self.main
                    || (frame.barriers > 1 && !self.api_yield_allowed(frame.barriers))
                    || self.thread_call_level(self.thread)
                        > self.threads[slot_index(self.thread.0)].native_base_calls
                {
                    let message = "attempt to yield across metamethod/C-call boundary";
                    let value = self.intern(message.as_bytes());
                    return Err(Error::raised(value, message));
                }
                self.frames[owner].pc = 1;
                self.transition = Some(Transition::Yield {
                    source: frame.vararg_start,
                    count: frame.argc(),
                });
                return Ok(None);
            }
            let values = self.take_resume_values(frame)?;
            self.frames[owner].vararg_count = (self.top - frame.vararg_start) as u32;
            return Ok(self.return_frame(self.frames[owner], values.start, values.len()));
        }
        let wrapped = id != CO_RESUME;
        let phase = frame.pc & 3;
        if phase >= 2 {
            let source = base + (frame.pc >> 2) as usize;
            if wrapped && phase == 3 {
                let value = self.located_error(self.registers[source], 1);
                self.registers[source] = value;
                let message = match value {
                    Value::String(s) => {
                        String::from_utf8_lossy(&self.strings.bytes[slot_index(s.0)]).into_owned()
                    }
                    _ => format!("error object is a {} value", value.type_name()),
                };
                return Err(Error::raised(value, message));
            }
            return Ok(self.return_frame(frame, source, self.top - source));
        }
        let thread = if wrapped {
            let Value::Thread(thread) =
                self.native_closures[slot_index(self.builtin_closure(id))].captures[0]
            else {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    0,
                    "invalid coroutine wrapper capture",
                ));
            };
            thread
        } else {
            let Value::Thread(thread) = self.argument(frame.vararg_start - 1, frame.argc(), 0)
            else {
                return Err(self.argument_error(0, b"coroutine expected"));
            };
            thread
        };
        let mut status = self.coroutine_status(thread);
        let target = &self.threads[slot_index(thread.0)];
        if target.resume_status == NativeThreadStatus::Yielded && target.frames.is_empty() {
            // A protected parser can unwind the yielded reader's CallInfo
            // while retaining status 1. There is no frame left to resume.
            status = Status::Dead;
        }
        // A resumed thread inherits the caller's C-call count. Suspended native
        // callbacks and tag-method continuations each add one boundary; ordinary
        // Lua calls do not. The host's initial call accounts for the remaining one.
        let level = self.native_calls_for(self.thread, owner);
        if status != Status::Suspended || level >= 200 {
            let message = if status != Status::Suspended {
                format!(
                    "cannot resume {} coroutine",
                    String::from_utf8_lossy(status.name())
                )
            } else {
                "C stack overflow".into()
            };
            if status == Status::Suspended
                && level >= 200
                && self.threads[slot_index(thread.0)].frames.is_empty()
            {
                // lua_resume's initial-stack overflow error replaces the entry
                // function; auxresume removes that error, leaving a dead thread.
                let state = &mut self.threads[slot_index(thread.0)];
                state.entry = None;
                state.registers.clear();
                state.top = 0;
                state.api_stack = 0..0;
                state.status = Status::Dead;
            }
            let value = self.intern(message.as_bytes());
            let arguments =
                self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].to_vec();
            let start = base + arguments.len();
            self.resize_registers(self.registers.len().max(start + 2));
            self.registers[base..start].copy_from_slice(&arguments);
            if wrapped {
                self.registers[start] = value;
            } else {
                self.registers[start] = Value::Boolean(false);
                self.registers[start + 1] = value;
            }
            self.top = start + if wrapped { 1 } else { 2 };
            self.frames[owner].vararg_start = base;
            self.frames[owner].vararg_count = (self.top - base) as u32;
            self.frames[owner].pc = ((arguments.len() as u32) << 2) | 3;
            return Ok(None);
        }
        let offset = usize::from(!wrapped);
        let count = frame.argc() - offset;
        self.resume_api_barriers(thread);
        let args =
            &self.registers[frame.vararg_start + offset..frame.vararg_start + offset + count];
        let state = &mut self.threads[slot_index(thread.0)];
        let capacity = state.registers.capacity();
        if state.frames.is_empty() {
            let call = state.api_stack.end - 1;
            state.registers.truncate(state.api_stack.end);
            state.registers.extend_from_slice(args);
            state.top = state.registers.len();
            state.entry = Some((call, count));
        } else {
            let call = state.api_stack.end;
            state
                .registers
                .resize(state.registers.len().max(call + count), Value::Nil);
            state.registers[call..call + count].copy_from_slice(args);
            state.top = call + count;
            state.resumed_values = Some(call..call + count);
        }
        state.resumer = Some(self.thread);
        state.status = Status::Running;
        state.resume_status = NativeThreadStatus::Ok;
        state.depth = level;
        state.native_level = level;
        state.native_bias = 0;
        state.native_base_calls = level.wrapping_add(1);
        let growth = (state.registers.capacity() - capacity) * std::mem::size_of::<Value>();
        self.threads.note_alloc(growth);
        // These are the caller's owned call temporaries. auxresume has moved
        // them away; the child now owns the values and the remaining C window
        // below retains only the explicit thread argument.
        self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].fill(Value::Nil);
        self.threads[slot_index(self.thread.0)].status = Status::Normal;
        // auxresume has moved its arguments off the C window. Keep the thread
        // argument in its owned native result area while the child executes.
        if !wrapped {
            self.registers[frame.base] = Value::Thread(thread);
        }
        self.frames[owner].vararg_start = frame.base;
        self.frames[owner].vararg_count = usize::from(!wrapped) as u32;
        self.frames[owner].pc = 1;
        self.transition = Some(Transition::Resume(thread));
        Ok(None)
    }
}
