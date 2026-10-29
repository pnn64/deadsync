//! External native hooks share the interrupted activation's stack.
use super::*;
use crate::vm::hooks::{self, Callback};

/// Installed hook callback. Lua hooks use the debug library's registry table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NativeHook {
    /// No C hook pointer is installed.
    None,
    /// The debug library's Lua hook dispatcher is installed.
    Lua,
    /// An external host function is installed and retained by the thread.
    Host(Value),
}

/// Per-thread hook controls, including the original signed count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeHookInfo {
    pub function: NativeHook,
    pub mask: u8,
    pub count: i32,
}

/// Native Lua 5.1 hook event. Event numbers are call=0, return=1, line=2,
/// count=3 and tail return=4. Only line events supply a current line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeHookEvent {
    pub event: i32,
    pub current_line: i32,
    pub frame: NativeDebugFrame,
}

impl NativeStack<'_, '_> {
    /// Install an external hook on a live thread. The callback must be a host
    /// function created by this VM. It runs without an extra visible call frame;
    /// `HostCall::hook_event` supplies its activation record and event. Its native
    /// stack aliases the interrupted activation, and temporary pushes are removed
    /// after delivery. Calls from a hook suppress recursive hook delivery.
    ///
    /// Mask bits are call=1, return=2, line=4 and count=8. None or a zero mask
    /// clears the callback; other masks narrow to a byte after this check.
    /// Setting a hook resets its instruction counter, including zero/negative
    /// counts. Lua debug-library hook entries are preserved.
    ///
    /// # Errors
    /// Rejects pending errors, invalid threads and non-host callbacks.
    pub fn set_hook(
        &mut self,
        thread: ThreadId,
        function: NativeHook,
        mask: i32,
        count: i32,
    ) -> Result<(), Error> {
        self.check_error()?;
        self.call.vm.validate_value(Value::Thread(thread))?;
        let callback = match if mask == 0 {
            NativeHook::None
        } else {
            function
        } {
            NativeHook::None => Callback::None,
            NativeHook::Lua => Callback::Lua,
            NativeHook::Host(Value::Native(id)) if self.call.vm.host_callback(id).is_some() => {
                Callback::Host(id)
            }
            _ => return Err(self.error(b"native hook requires a host function")),
        };
        let mask = if matches!(callback, Callback::None) {
            0
        } else {
            mask as u8
        };
        if let Callback::Host(id) = callback {
            self.call
                .vm
                .gc_barrier(Value::Thread(thread), Value::Native(id));
        }
        let state = if thread == self.thread_id() {
            &mut self.call.vm.hooks
        } else {
            &mut self.call.vm.threads[slot_index(thread.0)].hooks
        };
        state.callback = callback;
        state.mask = mask;
        state.count = count;
        state.ticks = count;
        Ok(())
    }

    /// Query native hook controls without changing the selected thread or stack.
    /// # Errors
    /// Rejects pending errors and invalid/collected threads.
    pub fn hook(&self, thread: ThreadId) -> Result<NativeHookInfo, Error> {
        self.check_error()?;
        self.call.vm.validate_value(Value::Thread(thread))?;
        let state = if thread == self.thread_id() {
            &self.call.vm.hooks
        } else {
            &self.call.vm.threads[slot_index(thread.0)].hooks
        };
        Ok(NativeHookInfo {
            function: match state.callback {
                Callback::None => NativeHook::None,
                Callback::Lua => NativeHook::Lua,
                Callback::Host(id) => NativeHook::Host(Value::Native(id)),
            },
            mask: state.mask,
            count: state.count,
        })
    }
    /// Borrow private binding data of a host function (including a retained hook).
    pub fn host_function_data<T: Any + Send + Sync>(&self, function: Value) -> Option<&T> {
        let Value::Native(id) = function else {
            return None;
        };
        let threads::NativeKind::Host(function) = &self.call.vm.native_closures.get(id)?.kind
        else {
            return None;
        };
        function.data.as_deref()?.downcast_ref()
    }

    /// Push the debug library's registry hook table and this thread's Lua
    /// callback, as its native hook dispatcher does. Returns whether the callback
    /// is a function. This does not install or invoke a hook.
    /// # Errors
    /// Rejects missing/replaced hook storage, pending errors or stack overflow.
    pub fn push_debug_hook(&mut self) -> Result<bool, Error> {
        self.check_error()?;
        let (table, function) = self.call.vm.lua_hook_values()?;
        self.push(Value::Table(table))?;
        self.push(function)?;
        Ok(matches!(function, Value::Function(_) | Value::Native(_)))
    }
}

impl Vm {
    pub(in crate::vm) fn host_hook(
        &mut self,
        id: u64,
        event: &[u8],
        line: Option<u32>,
        owner: usize,
    ) -> Result<(), Error> {
        let callback = self.host_callback(id).expect("retained host hook");
        let frame = self.frames[owner];
        let mut base = match frame.function {
            FrameKind::Lua(_) => frame.base,
            FrameKind::Native(_) => frame.vararg_start,
        };
        let mut end = match frame.function {
            FrameKind::Lua(function) => {
                let closure = &self.functions[slot_index(function.0)];
                let proto = &self.programs[closure.program].data.prototypes[closure.prototype];
                hooks::stack_top(proto, frame, self.frames.get(owner + 1))
                    .unwrap_or(frame.base + proto.debug_registers())
            }
            FrameKind::Native(_) => self
                .frames
                .get(owner + 1)
                .filter(|f| matches!(f.function, FrameKind::Native(CONT_RETURN)))
                .map_or(base + frame.argc(), |f| {
                    (f.vararg_start + f.argc()).max(base + frame.argc())
                }),
        };
        if matches!(frame.function, FrameKind::Lua(_)) {
            if let Some((_, top)) = self.hooks.resumed_top.filter(|&(i, _)| i == owner)
                && !self
                    .frames
                    .get(owner + 1)
                    .is_some_and(|f| matches!(f.function, FrameKind::Native(CONT_RETURN)))
            {
                end = top;
            }
            if self.hooks.yielding {
                base = self.hooks.yield_base;
            }
        }
        let event = NativeHookEvent {
            event: match event {
                b"call" => 0,
                b"return" => 1,
                b"line" => 2,
                b"count" => 3,
                b"tail return" => 4,
                _ => unreachable!("hook event"),
            },
            current_line: line.map_or(-1, |n| n as i32),
            frame: NativeDebugFrame::from_index(
                self.thread,
                if event == b"tail return" {
                    0
                } else {
                    self.frames[..=owner].iter().filter(|f| f.is_call()).count() as i32
                },
            ),
        };
        let depth = self.frames.len();
        let active = self.active_native;
        let top = self.top;
        self.resize_registers(self.registers.len().max(end));
        self.hooks.hidden = self.registers[end..].to_vec();
        self.hooks.hidden_start = end;
        self.hooks.popped = self.registers[base..end].to_vec();
        self.hooks.popped_start = base;
        self.hooks.host_owner = Some(owner);
        self.push_record(Frame {
            function: FrameKind::Native(CONT_NATIVE_HOOK),
            pc: 0,
            base,
            stack_start: end,
            result: top,
            wanted: 0,
            barriers: frame.barriers,
            vararg_start: base,
            vararg_count: (end - base) as u32,
            tail_calls: 0,
            c_level_bias: 0,
        });
        self.hooks.running = Some(depth as u16);
        self.hooks.host_running = Some(id);
        self.active_native = Some(depth);
        let data = self.host_binding(id).and_then(|f| f.data.clone());
        let mut context = HostCall {
            vm: self,
            hook: Some(event),
            id,
            data,
            activation: None,
            arguments: base,
            count: end - base,
            output: end,
            results: 0,
            abort: None,
            stack_opened: false,
        };
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(&mut context)));
        let result = match outcome {
            Ok(result) => context.abort.take().map_or(result, Err),
            Err(payload) => {
                context.vm.gc.closing = true;
                std::panic::resume_unwind(payload);
            }
        };
        self.active_native = active;
        if result.is_ok() {
            if self.hooks.yielding {
                self.hooks.yield_end = end;
            }
            self.frames.truncate(depth);
            let live = self.registers.len().min(end);
            self.hooks.preserve_popped(&self.registers, live);
            self.registers.resize(end, Value::Nil);
            if live < end {
                self.registers[live..end].copy_from_slice(&self.hooks.popped[live - base..]);
            }
            self.hooks.popped.clear();
            self.resize_registers(end + self.hooks.hidden.len());
            self.registers[end..].copy_from_slice(&self.hooks.hidden);
            self.hooks.hidden.clear();
            self.top = top;
            self.hooks.running = None;
            self.hooks.host_running = None;
            self.hooks.host_owner = None;
        }
        result
    }
}
