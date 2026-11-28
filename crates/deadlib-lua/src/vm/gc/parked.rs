//! Collector stack maps for coroutines parked at an ordinary Lua yield.
use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Stack {
    function: FunctionId,
    pc: u32,
    base: u8,
    prefix: u8,
    args_start: u8,
    args_end: u8,
    cutoff: u8,
    lua_end: u8,
}

impl Stack {
    pub(super) fn capture(thread: &threads::Thread) -> Option<Self> {
        let [caller, suspension] = &*thread.frames else {
            return None;
        };
        let FrameKind::Lua(function) = caller.function else {
            return None;
        };
        let len = thread.registers.len();
        let cutoff = thread.api_stack.end;
        if !matches!(suspension.function, FrameKind::Native(threads::CO_YIELD))
            || suspension.pc() != 1
            || len > 64
            || cutoff > len
            || cutoff > suspension.base
            || !thread.api_stack.is_empty()
            || thread.hooks.resumed_top.is_some()
            || thread.hooks.yield_owner.is_some()
        {
            return None;
        }
        let lua_end = suspension.stack_start.min(len);
        Some(Self {
            function,
            pc: caller.pc,
            base: caller.base.min(len) as u8,
            prefix: caller.result.min(cutoff) as u8,
            args_start: caller.vararg_start.min(cutoff) as u8,
            args_end: caller
                .vararg_start
                .saturating_add(caller.argc())
                .min(cutoff) as u8,
            cutoff: cutoff as u8,
            lua_end: lua_end as u8,
        })
    }
}

fn below(end: u8) -> u64 {
    u64::MAX.checked_shr(64 - u32::from(end)).unwrap_or(0)
}

#[derive(Clone, Copy)]
struct Masks {
    live: u64,
    dead: u64,
}

#[derive(Default)]
pub(super) struct Layouts([Option<(Stack, Masks)>; 2]);

impl Layouts {
    #[inline]
    fn get(&mut self, vm: &Vm, stack: Stack) -> Masks {
        if let Some((_, masks)) = self.0.iter().flatten().find(|(key, _)| *key == stack) {
            return *masks;
        }
        let function = &vm.functions[slot_index(stack.function.0)];
        let prototype = &vm.programs[function.program].data.prototypes[function.prototype];
        let pc = (stack.pc as usize)
            .saturating_sub(1)
            .min(prototype.code.len() - 1);
        // The Lua instruction awaits its native yield frame. Its before-map
        // owns the Lua slots. The yield frame's arguments have been moved,
        // and its native scratch starts above the physical stack cutoff.
        let map = prototype.metadata[pc].gc_live as u16 as usize;
        let slots = prototype.gc_maps[map][0]
            .checked_shl(u32::from(stack.base))
            .unwrap_or(0);
        let args = below(stack.args_end) & !below(stack.args_start);
        let live = (slots | below(stack.prefix) | args) & below(stack.cutoff);
        let dead = below(stack.lua_end) & !below(stack.base) & !live;
        let masks = Masks { live, dead };
        self.0[1] = self.0[0].replace((stack, masks));
        masks
    }
}

impl Vm {
    #[inline]
    fn queue_unmarked_stack_value(&self, value: Value, work: &mut Work) {
        // The parked frame itself supplies this root.
        if matches!(value, Value::Native(threads::CO_YIELD)) {
            return;
        }
        let Some(trace) = edge(value) else {
            return;
        };
        let marked = match value {
            Value::String(id) => self.strings.bytes.marked(id.0),
            value => self.marked_value(value),
        };
        if !marked {
            work.push(trace);
        }
    }

    pub(super) fn trace_parked_stack(
        &mut self,
        id: ThreadId,
        stack: Stack,
        layouts: &mut Layouts,
        work: &mut Work,
    ) -> Result<(), Error> {
        let Masks { mut live, mut dead } = layouts.get(self, stack);
        let additional = live.count_ones() as usize + 2;
        if additional > work.capacity() - work.len() {
            self.reserve_trace_work(work, additional)?;
        }
        work.pop();
        self.threads.mark(id.0);
        let thread = &self.threads[slot_index(id.0)];
        if !self.tables.marked(thread.globals.0) {
            work.push(Trace::Table(thread.globals, false));
        }
        if !self.functions.marked(stack.function.0) {
            work.push(Trace::Function(stack.function, false));
        }
        while live != 0 {
            let slot = live.trailing_zeros() as usize;
            live &= live - 1;
            self.queue_unmarked_stack_value(thread.registers[slot], work);
        }
        if !self.marked_value(Value::Native(threads::CO_YIELD)) {
            work.push(Trace::Native(threads::CO_YIELD, false));
        }
        let registers = &mut self.threads[slot_index(id.0)].registers;
        while dead != 0 {
            let slot = dead.trailing_zeros() as usize;
            dead &= dead - 1;
            if registers[slot] != Value::Nil {
                registers[slot] = Value::Nil;
            }
        }
        Ok(())
    }
}
