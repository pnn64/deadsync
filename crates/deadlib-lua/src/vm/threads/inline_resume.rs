//! Project short register-only stretches of bytecode between coroutine yields.
//! The real native yield frame stays visible throughout each suspension.
use super::*;
use crate::bytecode::{Instruction, NUMERIC_WHILE, OPEN_ARGS, Op, RK};
mod cycle;
mod feedback;
mod numeric;

const REGISTER_LIMIT: usize = 16;
const INSTRUCTION_LIMIT: usize = 16;

struct IndexYield {
    control: usize,
    function: usize,
    upvalue: usize,
}

struct IndexResume {
    thread: ThreadId,
    control: usize,
    function: usize,
    yield_register: Option<usize>,
    index: f64,
    limit: f64,
    step: f64,
}

struct ProjectedYield {
    thread: ThreadId,
    function: FunctionId,
    base: usize,
    pc: usize,
    numeric: Option<numeric::YieldBody>,
}

struct Projection<'a> {
    registers: &'a mut [Value],
    constants: &'a [Value],
    captures: &'a [u32],
    upvalues: &'a Arena<Cell>,
    threads: &'a Arena<Thread>,
    code: &'a [Instruction],
    parent: ThreadId,
    parent_registers: &'a [Value],
}

impl Projection<'_> {
    fn operand(&self, operand: u32) -> Value {
        if operand < RK {
            self.registers[operand as usize]
        } else {
            self.constants[(operand - RK) as usize]
        }
    }

    /// Follow only instructions that can finish using scratch registers.
    fn next_yield(&mut self, mut pc: usize) -> Option<(usize, Instruction)> {
        for _ in 0..INSTRUCTION_LIMIT {
            let i = self.code[pc];
            let a = usize::from(i.a);
            pc += 1;
            match i.op {
                Op::Call if i.b != OPEN_ARGS && self.registers[a] == Value::Native(CO_YIELD) => {
                    return Some((pc - 1, i));
                }
                Op::GetUpvalue => {
                    let cell = self.captures[i.c as usize] as usize;
                    self.registers[a] = match self.upvalues[cell] {
                        Cell::Closed(value) => value,
                        Cell::Open(owner, register) if owner == self.parent => {
                            self.parent_registers[register]
                        }
                        Cell::Open(owner, register) => {
                            self.threads[slot_index(owner.0)].registers[register]
                        }
                    };
                }
                Op::Load => {
                    if i.b == 0 {
                        self.registers[a] = self.constants[i.c as usize];
                    }
                }
                Op::Move if i.c != OPEN => self.registers[a] = self.registers[usize::from(i.b)],
                Op::Add | Op::Sub | Op::Mul | Op::Div => {
                    let (Value::Number(x), Value::Number(y)) =
                        (self.operand(u32::from(i.b)), self.operand(i.c))
                    else {
                        return None;
                    };
                    self.registers[a] = Value::Number(Vm::number_op(i.op, x, y));
                }
                Op::Neg => {
                    let Value::Number(x) = self.registers[usize::from(i.b)] else {
                        return None;
                    };
                    self.registers[a] = Value::Number(-x);
                }
                Op::Jump if i.b != NUMERIC_WHILE => pc = i.c as usize,
                Op::JumpFalse | Op::JumpTrue => {
                    if self.registers[a].is_truthy() == (i.op == Op::JumpTrue) {
                        pc = i.c as usize;
                    }
                }
                Op::ForLoop => {
                    let (Value::Number(index), Value::Number(limit), Value::Number(step)) = (
                        self.registers[a],
                        self.registers[a + 1],
                        self.registers[a + 2],
                    ) else {
                        return None;
                    };
                    let index = index + step;
                    if index_in_range(index, limit, step) {
                        self.registers[a] = Value::Number(index);
                        self.registers[a + 3] = Value::Number(index);
                        pc = i.c as usize;
                    }
                }
                Op::Close => {}
                _ => return None,
            }
        }
        None
    }
}

fn index_in_range(index: f64, limit: f64, step: f64) -> bool {
    if step > 0.0 {
        index <= limit
    } else {
        index >= limit
    }
}

impl IndexResume {
    #[inline]
    fn resume(&mut self, cx: &mut simple::Context<'_, &mut Arena<Thread>>, call: usize) -> bool {
        // Only a parent register can change the captured function in this loop.
        if self
            .yield_register
            .is_some_and(|register| cx.registers[register] != Value::Native(CO_YIELD))
        {
            return false;
        }
        let index = self.index + self.step;
        if !index_in_range(index, self.limit, self.step) {
            return false;
        }
        self.index = index;
        cx.registers[call] = Value::Boolean(true);
        cx.registers[call + 1] = Value::Number(index);
        true
    }

    fn write_back(&self, threads: &mut Arena<Thread>) {
        let state = &mut threads[slot_index(self.thread.0)];
        let index = Value::Number(self.index);
        state.registers[self.control] = index;
        state.registers[self.control + 3] = index;
        state.registers[self.function + 1] = index;
    }
}

/// Reuse validated parked thread state between observer-free calls.
#[derive(Default)]
pub(in crate::vm) struct ResumeHints {
    plans: [Option<IndexResume>; 2],
    projected: Option<ProjectedYield>,
}

impl ResumeHints {
    #[inline]
    pub(in crate::vm) fn resume(
        &mut self,
        cx: &mut simple::Context<'_, &mut Arena<Thread>>,
        call: usize,
    ) -> bool {
        let Value::Thread(thread) = cx.registers[call + 1] else {
            return false;
        };
        self.plans
            .iter_mut()
            .flatten()
            .find(|plan| plan.thread == thread)
            .is_some_and(|plan| plan.resume(cx, call))
    }

    pub(in crate::vm) fn write_back(&mut self, threads: &mut Arena<Thread>) {
        for plan in self.plans.iter().flatten() {
            plan.write_back(threads);
        }
        if let Some(plan) = &mut self.projected
            && let Some(body) = &mut plan.numeric
        {
            body.write_back(threads, plan.thread, plan.base);
        }
    }

    #[inline(never)]
    pub(in crate::vm) fn resume_projected(
        &mut self,
        cx: &mut simple::Context<'_, &mut Arena<Thread>>,
        functions: &Arena<Closure>,
        programs: &Arena<LoadedProgram>,
        call: usize,
        wanted: usize,
    ) -> bool {
        let Some(plan) = &mut self.projected else {
            return false;
        };
        if cx.registers[call + 1] != Value::Thread(plan.thread) {
            return false;
        }
        if let Some(body) = &mut plan.numeric {
            return body.resume(plan.thread, plan.base, cx, call, wanted);
        }
        let closure = &functions[slot_index(plan.function.0)];
        let program = &programs[closure.program];
        let prototype = &program.data.prototypes[closure.prototype];
        let width = usize::from(prototype.registers);
        let live = plan.base..plan.base + width;
        let state = &cx.threads[slot_index(plan.thread.0)];
        let mut registers = [Value::Nil; REGISTER_LIMIT];
        registers[..width].copy_from_slice(&state.registers[live.clone()]);
        let mut child = Projection {
            registers: &mut registers[..width],
            constants: program.bound_constants(closure.prototype),
            captures: &closure.upvalues,
            upvalues: cx.upvalues,
            threads: cx.threads,
            code: &prototype.code,
            parent: cx.thread,
            parent_registers: cx.registers,
        };
        let Some((pc, instruction)) = child.next_yield(plan.pc) else {
            return false;
        };
        if pc + 1 != plan.pc {
            return false;
        }
        let args = usize::from(instruction.a) + 1;
        let payload = &registers[args..args + usize::from(instruction.b)];
        cx.threads[slot_index(plan.thread.0)].registers[live].copy_from_slice(&registers[..width]);
        write_resume_results(cx.registers, call, wanted, payload);
        true
    }

    #[inline]
    pub(in crate::vm) fn run_consumer_cycle(
        &mut self,
        cx: &mut simple::Context<'_, &mut Arena<Thread>>,
        pc: usize,
        top: &mut usize,
    ) -> Option<(usize, bool)> {
        if let Some(plan) = &mut self.projected
            && let Some(body) = &mut plan.numeric
        {
            return cycle::run(cx, pc, top, plan.thread, |cx, call, wanted| {
                body.resume(plan.thread, plan.base, cx, call, wanted)
            });
        }
        if let [Some(plan), None] = &mut self.plans {
            return cycle::run(cx, pc, top, plan.thread, |cx, call, wanted| {
                wanted == 2 && plan.resume(cx, call)
            });
        }
        if let [Some(first), Some(second)] = &mut self.plans {
            return cycle::pair::run(cx, pc, top, first, second);
        }
        None
    }

    pub(in crate::vm) fn clear(&mut self) {
        self.plans = [None, None];
        self.projected = None;
    }

    fn remove(&mut self, thread: ThreadId) {
        if self.plans[0]
            .as_ref()
            .is_some_and(|plan| plan.thread == thread)
        {
            self.plans[0] = self.plans[1].take();
        } else if self.plans[1]
            .as_ref()
            .is_some_and(|plan| plan.thread == thread)
        {
            self.plans[1] = None;
        }
    }

    fn insert(&mut self, plan: IndexResume) {
        self.plans[1] = self.plans[0].replace(plan);
    }
}

fn write_resume_results(registers: &mut [Value], call: usize, wanted: usize, values: &[Value]) {
    // Resume consumes the copied thread argument before writing results.
    registers[call + 1] = Value::Nil;
    if wanted != 0 {
        registers[call] = Value::Boolean(true);
        let copied = (wanted - 1).min(values.len());
        registers[call + 1..call + 1 + copied].copy_from_slice(&values[..copied]);
        registers[call + 1 + copied..call + wanted].fill(Value::Nil);
    }
}

#[derive(Clone, Copy)]
pub(in crate::vm) struct ParkedIndex {
    pub index: f64,
    pub limit: f64,
    pub step: f64,
    thread: ThreadId,
    control: usize,
    function: usize,
    level: u16,
    line: u32,
}

impl IndexYield {
    /// Decode a loop that yields its numeric index through a captured function.
    fn decode(code: &[Instruction], pc: usize, frame: Frame, base: usize) -> Option<Self> {
        let loop_pc = pc + usize::from(code.get(pc)?.op == Op::Close);
        let loop_instruction = code.get(loop_pc)?;
        if loop_instruction.op != Op::ForLoop {
            return None;
        }
        let [get, argument, call] = code.get(loop_instruction.c as usize..pc)? else {
            return None;
        };
        let control = usize::from(loop_instruction.a);
        let function = usize::from(call.a);
        if call.op != Op::Call
            || call.b != 1
            || get.op != Op::GetUpvalue
            || get.a != call.a
            || function < control + 4
            || argument.op != Op::Move
            || argument.c != 0
            || usize::from(argument.a) != function + 1
            || usize::from(argument.b) != control + 3
            || frame.result != base + function
            || frame.vararg_start != base + function + 1
            || frame.vararg_count != 1
            || frame.wanted != frame_wanted(call.c)
        {
            return None;
        }
        Some(Self {
            control,
            function,
            upvalue: get.c as usize,
        })
    }
}

impl Thread {
    fn ordinary_yield(&self) -> Option<(Frame, Frame)> {
        let (caller, suspended) = self.plain_suspended_builtin(CO_YIELD)?;
        if self.status != Status::Suspended
            || self.resume_status != NativeThreadStatus::Yielded
            || self.native_resumer
            || self.resumer.is_some()
            || self.resumed_values.is_some()
            || self.abandoned
            || !self.api_stack.is_empty()
            || self.api_stack.end != suspended.vararg_start
            || suspended.barriers != 1
            || suspended.c_level_bias != 0
            || suspended.tail_calls != 0
            || self.frames.len() >= usize::from(self.call_slots)
            || !self.loads.is_empty()
            || self.frames[..self.frames.len() - 1]
                .iter()
                .any(|frame| frame.barriers != 0 || !matches!(frame.function, FrameKind::Lua(_)))
        {
            return None;
        }
        Some((caller, suspended))
    }

    fn park_after_lua_yield(&mut self, source: usize, level: u16, line: u32) {
        self.api_stack = source..source;
        self.top = source;
        self.depth = level;
        self.native_level = level;
        self.native_bias = 0;
        self.native_base_calls = level + 1;
        self.line = line;
    }
}

impl Vm {
    pub(in crate::vm) fn resume_has_spare_stack(&self) -> bool {
        self.frames.len() < self.frames.capacity()
            && self.frames.len() + 1 < usize::from(self.call_slots)
            && self.registers.capacity() - self.registers.len() >= 6
    }

    pub(in crate::vm) fn parked_numeric_yield(&self, thread: ThreadId) -> Option<ParkedIndex> {
        if thread == self.thread {
            return None;
        }
        let state = &self.threads[slot_index(thread.0)];
        let (caller, suspended) = state.ordinary_yield()?;
        if !matches!(suspended.wanted, 0 | FRAME_OPEN) {
            return None;
        }
        let FrameKind::Lua(function) = caller.function else {
            return None;
        };
        let closure = &self.functions[slot_index(function.0)];
        let prototype = &self.programs[closure.program].data.prototypes[closure.prototype];
        let base = caller.base;
        let end = base + usize::from(prototype.registers);
        if end != suspended.stack_start || end != suspended.base || state.registers.len() != end + 6
        {
            return None;
        }
        let plan = IndexYield::decode(&prototype.code, caller.pc(), suspended, base)?;
        let control = base + plan.control;
        let (Value::Number(index), Value::Number(limit), Value::Number(step)) = (
            state.registers[control],
            state.registers[control + 1],
            state.registers[control + 2],
        ) else {
            return None;
        };
        let level = self.simple_resume_level();
        if level >= 200
            || self.cell_value(closure.upvalues[plan.upvalue] as usize) != Value::Native(CO_YIELD)
        {
            return None;
        }
        Some(ParkedIndex {
            index,
            limit,
            step,
            thread,
            control,
            function: base + plan.function,
            level,
            line: prototype.instruction_line(caller.pc() - 1),
        })
    }

    pub(in crate::vm) fn advance_parked_yield(&mut self, parked: ParkedIndex, index: f64) {
        let state = &mut self.threads[slot_index(parked.thread.0)];
        let value = Value::Number(index);
        state.registers[parked.control] = value;
        state.registers[parked.control + 3] = value;
        state.registers[parked.function] = Value::Native(CO_YIELD);
        state.registers[parked.function + 1] = value;
        let scratch = state.frames.last().expect("parked yield has a frame").base;
        state.registers[scratch..].fill(Value::Nil);
        state.park_after_lua_yield(parked.function + 1, parked.level, parked.line);
    }

    fn simple_resume_level(&self) -> u16 {
        if let [frame] = &*self.frames
            && matches!(frame.function, FrameKind::Lua(_))
            && self.loads.is_empty()
        {
            let state = &self.threads[slot_index(self.thread.0)];
            return state.depth.wrapping_add(1).wrapping_add(state.native_bias);
        }
        self.native_calls_for(self.thread, self.frames.len())
    }

    fn publish_resume_results(&mut self, call: usize, wanted: usize, values: &[Value]) {
        write_resume_results(&mut self.registers, call, wanted, values);
        self.top = call + wanted;
        self.gc.pending = false;
    }

    pub(in crate::vm) fn try_inline_coroutine_resume(
        &mut self,
        call: usize,
        wanted: usize,
        checkpoint_ready: bool,
        hints: &mut ResumeHints,
    ) -> bool {
        let Value::Thread(thread) = self.registers[call + 1] else {
            return false;
        };
        hints.projected = None;
        hints.remove(thread);
        if thread == self.thread
            || (!checkpoint_ready && !(self.sort_fast_context() && self.can_skip_gc_checkpoint()))
            || !self.resume_has_spare_stack()
        {
            return false;
        }
        let state = &self.threads[slot_index(thread.0)];
        let Some((caller, suspended)) = state.ordinary_yield() else {
            return false;
        };
        if !matches!(suspended.wanted, 0 | FRAME_OPEN) {
            return false;
        }
        let FrameKind::Lua(function) = caller.function else {
            unreachable!("plain suspension has a Lua caller");
        };
        let closure = &self.functions[slot_index(function.0)];
        let program = &self.programs[closure.program];
        let prototype = &program.data.prototypes[closure.prototype];
        let width = usize::from(prototype.registers);
        let base = caller.base;
        let end = base + width;
        if width > REGISTER_LIMIT
            || end != suspended.stack_start
            || end != suspended.base
            || end > state.registers.len()
        {
            return false;
        }

        let index_yield = IndexYield::decode(&prototype.code, caller.pc(), suspended, base);
        if let Some(plan) = &index_yield
            && state.registers.len() == end + 6
        {
            let control = base + plan.control;
            let (Value::Number(index), Value::Number(limit), Value::Number(step)) = (
                state.registers[control],
                state.registers[control + 1],
                state.registers[control + 2],
            ) else {
                return false;
            };
            let index = index + step;
            let upvalue = closure.upvalues[plan.upvalue] as usize;
            let yield_function = self.cell_value(upvalue);
            let level = self.simple_resume_level();
            if !index_in_range(index, limit, step)
                || yield_function != Value::Native(CO_YIELD)
                || level >= 200
            {
                return false;
            }
            let line = prototype.instruction_line(caller.pc() - 1);
            let value = Value::Number(index);
            let function = base + plan.function;
            let state = &mut self.threads[slot_index(thread.0)];
            state.registers[control] = value;
            state.registers[control + 3] = value;
            state.registers[function] = yield_function;
            state.registers[function + 1] = value;
            state.registers[end..].fill(Value::Nil);
            state.park_after_lua_yield(function + 1, level, line);
            if wanted == 2 {
                let yield_register = match self.upvalues[upvalue] {
                    Cell::Open(owner, register) if owner == self.thread => Some(register),
                    _ => None,
                };
                hints.insert(IndexResume {
                    thread,
                    control,
                    function,
                    yield_register,
                    index,
                    limit,
                    step,
                });
            }
            self.publish_resume_results(call, wanted, &[value]);
            return true;
        }

        // The projection only writes local scratch registers. Any unsupported
        // operation can go through the regular scheduler with the original stack.
        let mut registers = [Value::Nil; REGISTER_LIMIT];
        registers[..width].copy_from_slice(&state.registers[base..end]);
        let mut child = Projection {
            registers: &mut registers[..width],
            constants: program.bound_constants(closure.prototype),
            captures: &closure.upvalues,
            upvalues: &self.upvalues,
            threads: &self.threads,
            code: &prototype.code,
            parent: self.thread,
            parent_registers: &self.registers,
        };
        let Some((pc, instruction)) = child.next_yield(caller.pc()) else {
            return false;
        };
        let count = usize::from(instruction.b);
        let child_call = usize::from(instruction.a);
        let args = &registers[child_call + 1..child_call + 1 + count];
        let child_end = end + count.max(2) + 4;
        if child_end > state.registers.capacity() {
            return false;
        }
        let level = self.simple_resume_level();
        if level >= 200 {
            return false;
        }
        let line = prototype.instruction_line(pc);
        let child_wanted = frame_wanted(instruction.c);

        let state = &mut self.threads[slot_index(thread.0)];
        state.registers[base..end].copy_from_slice(&registers[..width]);
        state.registers.truncate(end);
        state.registers.resize(child_end, Value::Nil);
        let child_call = base + child_call;
        let native = state.frames.len() - 1;
        state.frames[native - 1].pc = (pc + 1) as u32;
        state.frames[native] = Frame {
            function: FrameKind::Native(CO_YIELD),
            pc: 1,
            base: end,
            stack_start: end,
            result: child_call,
            wanted: child_wanted,
            barriers: 1,
            vararg_start: child_call + 1,
            vararg_count: count as u32,
            tail_calls: 0,
            c_level_bias: 0,
        };
        state.park_after_lua_yield(child_call + 1, level, line);
        if pc + 1 == caller.pc() && index_yield.is_none() && matches!(child_wanted, 0 | FRAME_OPEN)
        {
            // This projection published the exact frame that the next repeat
            // can keep. Its Lua registers are the only part that will vary.
            hints.projected = Some(ProjectedYield {
                thread,
                function,
                base,
                pc: pc + 1,
                numeric: numeric::YieldBody::decode(
                    self,
                    closure,
                    program,
                    pc + 1,
                    &registers[..width],
                ),
            });
        }
        self.publish_resume_results(call, wanted, args);
        true
    }
}
