//! Project short register-only stretches of bytecode between coroutine yields.
//! The real native yield frame stays visible throughout each suspension.
use super::*;
use crate::bytecode::{Instruction, OPEN_ARGS, Op};

const REGISTER_LIMIT: usize = 16;
const INSTRUCTION_LIMIT: usize = 16;

struct IndexYield {
    control: usize,
    function: usize,
    upvalue: usize,
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
            || !matches!(suspended.wanted, 0 | FRAME_OPEN)
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
            line: prototype.metadata[caller.pc() - 1].line,
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
        // Resume consumes the copied thread argument before writing results.
        self.registers[call + 1] = Value::Nil;
        if wanted != 0 {
            self.registers[call] = Value::Boolean(true);
            let copied = (wanted - 1).min(values.len());
            self.registers[call + 1..call + 1 + copied].copy_from_slice(&values[..copied]);
            self.registers[call + 1 + copied..call + wanted].fill(Value::Nil);
        }
        self.top = call + wanted;
        self.gc.pending = false;
    }

    pub(in crate::vm) fn try_inline_coroutine_resume(
        &mut self,
        call: usize,
        wanted: usize,
        checkpoint_ready: bool,
    ) -> bool {
        let Value::Thread(thread) = self.registers[call + 1] else {
            return false;
        };
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

        if let Some(plan) = IndexYield::decode(&prototype.code, caller.pc(), suspended, base)
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
            let in_range = if step > 0.0 {
                index <= limit
            } else {
                index >= limit
            };
            let yield_function = self.cell_value(closure.upvalues[plan.upvalue] as usize);
            let level = self.simple_resume_level();
            if !in_range || yield_function != Value::Native(CO_YIELD) || level >= 200 {
                return false;
            }
            let line = prototype.metadata[caller.pc() - 1].line;
            let index = Value::Number(index);
            let function = base + plan.function;
            let state = &mut self.threads[slot_index(thread.0)];
            state.registers[control] = index;
            state.registers[control + 3] = index;
            state.registers[function] = yield_function;
            state.registers[function + 1] = index;
            state.registers[end..].fill(Value::Nil);
            state.park_after_lua_yield(function + 1, level, line);
            self.publish_resume_results(call, wanted, &[index]);
            return true;
        }

        // The projection only writes local scratch registers. Any unsupported
        // operation can go through the regular scheduler with the original stack.
        let mut registers = [Value::Nil; REGISTER_LIMIT];
        registers[..width].copy_from_slice(&state.registers[base..end]);
        let mut hints = [[0; 2]; simple::FIELD_HINTS];
        let mut cx = simple::Context {
            registers: &mut registers[..width],
            constants: &program.constants[closure.prototype],
            field_hints: &mut hints,
            tables: &mut self.tables,
            strings: &self.strings.bytes,
            upvalue_ids: &closure.upvalues,
            upvalues: &self.upvalues,
            threads: &self.threads,
            thread,
            code: &prototype.code,
            stop_before_for_loop: None,
            ignored_numeric_while: None,
            env: closure.env,
            base: 0,
            gc_pending: false,
            closed: true,
            index_name: self.meta_names[Meta::Index as usize],
            resolvers: None,
        };
        let mut pc = caller.pc();
        let mut next_yield = None;
        for _ in 0..INSTRUCTION_LIMIT {
            let instruction = prototype.code[pc];
            match instruction.op {
                Op::Call
                    if instruction.b != OPEN_ARGS
                        && cx.registers[usize::from(instruction.a)] == Value::Native(CO_YIELD) =>
                {
                    next_yield = Some(instruction);
                    break;
                }
                Op::GetUpvalue => {
                    let cell = cx.upvalue_ids[instruction.c as usize] as usize;
                    cx.registers[usize::from(instruction.a)] = match cx.upvalues[cell] {
                        Cell::Closed(value) => value,
                        Cell::Open(owner, register) if owner == self.thread => {
                            self.registers[register]
                        }
                        Cell::Open(owner, register) => {
                            cx.threads[slot_index(owner.0)].registers[register]
                        }
                    };
                    pc += 1;
                }
                Op::Load
                | Op::Move
                | Op::Add
                | Op::Sub
                | Op::Mul
                | Op::Div
                | Op::Neg
                | Op::Jump
                | Op::JumpTrue
                | Op::JumpFalse
                | Op::ForLoop
                | Op::Close => {
                    let Some(next) = simple::step(&mut cx, pc) else {
                        return false;
                    };
                    pc = next;
                }
                _ => return false,
            }
        }
        let Some(instruction) = next_yield else {
            return false;
        };
        let count = usize::from(instruction.b);
        let child_end = end + count.max(2) + 4;
        if child_end > state.registers.capacity() {
            return false;
        }
        let level = self.simple_resume_level();
        if level >= 200 {
            return false;
        }
        let child_call = usize::from(instruction.a);
        let args = &registers[child_call + 1..child_call + 1 + count];
        let line = prototype.metadata[pc].line;

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
            wanted: frame_wanted(instruction.c),
            barriers: 1,
            vararg_start: child_call + 1,
            vararg_count: count as u32,
            tail_calls: 0,
            c_level_bias: 0,
        };
        state.park_after_lua_yield(child_call + 1, level, line);
        self.publish_resume_results(call, wanted, args);
        true
    }
}
