//! Run adjacent producer and consumer loops while neither side can call out.
//! A failed guard leaves the original resume ready to execute normally.
//!
//! The producer consumes a numeric reply, advances a wrapped array index and
//! yields a table. The consumer reads a numeric field and feeds it back on the
//! next call. Decode both loops before running them over the parked frame.
use super::*;

struct NumericLoop {
    control: usize,
    index: f64,
    limit: f64,
    step: f64,
}

impl NumericLoop {
    fn read(registers: &[Value], control: usize) -> Option<Self> {
        let [
            Value::Number(index),
            Value::Number(limit),
            Value::Number(step),
        ] = *registers.get(control..control + 3)?
        else {
            return None;
        };
        Some(Self {
            control,
            index,
            limit,
            step,
        })
    }

    #[inline]
    fn next(&self) -> Option<f64> {
        let next = self.index + self.step;
        index_in_range(next, self.limit, self.step).then_some(next)
    }

    fn write(&self, registers: &mut [Value]) {
        let index = Value::Number(self.index);
        registers[self.control] = index;
        registers[self.control + 3] = index;
    }
}

fn back_edge(code: &[Instruction], pc: usize) -> Option<(usize, Instruction)> {
    let pc = pc + usize::from(code.get(pc)?.op == Op::Close);
    let edge = *code.get(pc)?;
    (edge.op == Op::ForLoop).then_some((pc, edge))
}

fn number(registers: &[Value], slot: usize) -> Option<f64> {
    let Value::Number(value) = *registers.get(slot)? else {
        return None;
    };
    Some(value)
}

impl Vm {
    fn closed_capture(&self, closure: &Closure, index: u32) -> Option<Value> {
        let cell = *closure.upvalues.get(index as usize)? as usize;
        match self.upvalues[cell] {
            Cell::Closed(value) => Some(value),
            Cell::Open(..) => None,
        }
    }
}

struct Consumer {
    cycle: NumericLoop,
    call: usize,
    exit: usize,
    reply: usize,
    sum: usize,
    assert_slot: usize,
    assert_results: usize,
    field: StringId,
    thread: ThreadId,
}

impl Consumer {
    fn decode(vm: &Vm, frame: Frame, pc: usize) -> Option<Self> {
        let FrameKind::Lua(function) = frame.function else {
            return None;
        };
        let closure = &vm.functions[slot_index(function.0)];
        let program = &vm.programs[closure.program];
        let code = &program.data.prototypes[closure.prototype].code;
        let constants = program.bound_constants(closure.prototype);
        let (exit, edge) = back_edge(code, pc + 6)?;
        let [
            resume,
            target,
            reply,
            call,
            check,
            status,
            assert,
            field,
            add,
        ] = *code.get(edge.c as usize..pc + 6)?
        else {
            return None;
        };
        let a = u16::from(call.a);
        let reply_slot = u16::from(field.a);
        let sum = u16::from(add.a);
        let control = u16::from(edge.a);
        if resume.op != Op::GetUpvalue
            || resume.a != call.a
            || target.op != Op::GetUpvalue
            || u16::from(target.a) != a + 1
            || reply.op != Op::Move
            || reply.c != 0
            || u16::from(reply.a) != a + 2
            || reply.b != reply_slot
            || call.op != Op::Call
            || call.b != 2
            || call.c != 2
            || check.op != Op::GetGlobal
            || u16::from(check.a) != a + 2
            || status.op != Op::Move
            || status.c != 0
            || u16::from(status.a) != a + 3
            || status.b != a
            || assert.op != Op::Call
            || assert.a != check.a
            || assert.b != 1
            || assert.c > 1
            || field.op != Op::GetTable
            || field.b != a + 1
            || add.op != Op::Add
            || add.b != sum
            || add.c != u32::from(reply_slot)
            || reply_slot == sum
            || reply_slot >= control
            || sum >= control
            || a < control + 4
            || vm.closed_capture(closure, resume.c)? != Value::Native(CO_RESUME)
        {
            return None;
        }
        let Value::Thread(thread) = vm.closed_capture(closure, target.c)? else {
            return None;
        };
        let Value::String(name) = *constants.get(check.c as usize)? else {
            return None;
        };
        let Value::String(field) = *constants.get(field.c.checked_sub(RK)? as usize)? else {
            return None;
        };
        let base = frame.base;
        let call = base + usize::from(a);
        if vm.tables[slot_index(closure.env.0)].get_string(name) != Value::Native(0)
            || vm.registers[call + 1] != Value::Thread(thread)
            || thread == vm.thread
        {
            return None;
        }
        Some(Self {
            cycle: NumericLoop::read(&vm.registers, base + usize::from(control))?,
            call,
            exit: exit + 1,
            reply: base + usize::from(reply_slot),
            sum: base + usize::from(sum),
            assert_slot: base + usize::from(check.a),
            assert_results: assert.c as usize,
            field,
            thread,
        })
    }
}

struct Producer {
    cycle: NumericLoop,
    table: TableId,
    width: f64,
    offset: f64,
    slot: usize,
    sum: usize,
    payload: usize,
    line: u32,
}

impl Producer {
    fn decode(vm: &Vm, state: &Thread) -> Option<Self> {
        let (frame, suspended) = state.ordinary_yield()?;
        let FrameKind::Lua(function) = frame.function else {
            return None;
        };
        let closure = &vm.functions[slot_index(function.0)];
        let program = &vm.programs[closure.program];
        let prototype = &program.data.prototypes[closure.prototype];
        let code = &prototype.code;
        let constants = program.bound_constants(closure.prototype);
        let pc = frame.pc();
        let [add, width, modulo, wrap] = *code.get(pc..pc + 4)? else {
            return None;
        };
        let (_, edge) = back_edge(code, pc + 4)?;
        let [yield_capture, table_capture, payload, call] = *code.get(edge.c as usize..pc)? else {
            return None;
        };
        let a = u16::from(call.a);
        let slot = u16::from(wrap.a);
        let sum = u16::from(add.a);
        let control = u16::from(edge.a);
        let base = frame.base;
        let end = base + usize::from(prototype.registers);
        let first_arg = base + usize::from(a) + 1;
        if add.op != Op::Add
            || add.b != sum
            || add.c != u32::from(a)
            || width.op != Op::GetUpvalue
            || u16::from(width.a) != a + 1
            || modulo.op != Op::Mod
            || modulo.a != width.a
            || modulo.b != slot
            || modulo.c != u32::from(width.a)
            || wrap.op != Op::Add
            || wrap.b != u16::from(modulo.a)
            || yield_capture.op != Op::GetUpvalue
            || yield_capture.a != call.a
            || table_capture.op != Op::GetUpvalue
            || table_capture.a != width.a
            || payload.op != Op::GetTable
            || payload.a != table_capture.a
            || payload.b != u16::from(table_capture.a)
            || payload.c != u32::from(slot)
            || call.op != Op::Call
            || call.b != 1
            || call.c != 1
            || slot == sum
            || slot >= control
            || sum >= control
            || a < control + 4
            || suspended.result + 1 != first_arg
            || suspended.vararg_start != first_arg
            || suspended.vararg_count != 1
            || suspended.wanted != 1
            || suspended.stack_start != end
            || suspended.base != end
            || state.registers.len() != end + 6
            || state.registers[first_arg - 1] != Value::Native(CO_YIELD)
            || vm.closed_capture(closure, yield_capture.c)? != Value::Native(CO_YIELD)
        {
            return None;
        }
        let Value::Number(width) = vm.closed_capture(closure, width.c)? else {
            return None;
        };
        let Value::Number(offset) = *constants.get(wrap.c.checked_sub(RK)? as usize)? else {
            return None;
        };
        let Value::Table(table) = vm.closed_capture(closure, table_capture.c)? else {
            return None;
        };
        Some(Self {
            cycle: NumericLoop::read(&state.registers, base + usize::from(control))?,
            table,
            width,
            offset,
            slot: base + usize::from(slot),
            sum: base + usize::from(sum),
            payload: first_arg,
            line: prototype.instruction_line(pc - 1),
        })
    }
}

impl Vm {
    #[inline(never)]
    pub(in crate::vm) fn try_run_coroutine_feedback_cycle(&mut self, pc: usize) -> Option<usize> {
        if !self.sort_fast_context()
            || !self.can_skip_gc_checkpoint()
            || !self.resume_has_spare_stack()
            || !self.open_upvalues.is_empty()
            || !self.loads.is_empty()
            || self.active_native.is_some()
            || self.hooks.host_running.is_some()
        {
            return None;
        }
        let frame = *self.frames.last()?;
        let mut consumer = Consumer::decode(self, frame, pc)?;
        let thread = slot_index(consumer.thread.0);
        let state = &self.threads[thread];
        let mut producer = Producer::decode(self, state)?;
        let level = self.simple_resume_level();
        if level >= 200 {
            return None;
        }
        let mut reply = number(&self.registers, consumer.call + 2)?;
        let mut consumer_sum = number(&self.registers, consumer.sum)?;
        let mut slot = number(&state.registers, producer.slot)?;
        let mut producer_sum = number(&state.registers, producer.sum)?;
        let array = self.tables[slot_index(producer.table.0)].array_values();
        let mut hint = 0;
        let mut last_payload = None;
        let mut advanced = false;
        let mut next_pc = pc;
        loop {
            // A miss or non-numeric field retries this exact resume, allowing
            // the interpreter to invoke a metamethod or deliver the error.
            let Some(index) = producer.cycle.next() else {
                break;
            };
            let next_slot = Self::number_op(Op::Mod, slot, producer.width) + producer.offset;
            let key = next_slot as u32;
            if f64::from(key) != next_slot {
                break;
            }
            let Some(&Value::Table(payload)) = array.get(key.wrapping_sub(1) as usize) else {
                break;
            };
            let Value::Number(next_reply) =
                self.tables[slot_index(payload.0)].get_string_cached(consumer.field, &mut hint)
            else {
                break;
            };
            last_payload = Some(payload);
            producer.cycle.index = index;
            producer_sum += reply;
            slot = next_slot;
            reply = next_reply;
            consumer_sum += reply;
            let Some(index) = consumer.cycle.next() else {
                next_pc = consumer.exit;
                break;
            };
            consumer.cycle.index = index;
            advanced = true;
        }
        let payload = Value::Table(last_payload?);
        // Only the previous completed yield is observable at a side exit.
        // Keep the native frame and publish the producer's corresponding
        // Lua locals, then reconstruct the consumer's current instruction.
        let state = &mut self.threads[thread];
        producer.cycle.write(&mut state.registers);
        state.registers[producer.slot] = Value::Number(slot);
        state.registers[producer.sum] = Value::Number(producer_sum);
        state.registers[producer.payload] = payload;
        let scratch = state.frames.last().expect("validated yield frame").base;
        state.registers[scratch..].fill(Value::Nil);
        state.park_after_lua_yield(producer.payload, level, producer.line);

        let registers = &mut self.registers;
        if advanced {
            consumer.cycle.write(registers);
        }
        registers[consumer.reply] = Value::Number(reply);
        registers[consumer.sum] = Value::Number(consumer_sum);
        registers[consumer.assert_slot..consumer.assert_slot + 2].fill(Value::Boolean(true));
        self.top = consumer.assert_slot + consumer.assert_results;
        if next_pc == pc {
            registers[consumer.call] = Value::Native(CO_RESUME);
            registers[consumer.call + 1] = Value::Thread(consumer.thread);
            registers[consumer.call + 2] = Value::Number(reply);
        } else {
            registers[consumer.call] = Value::Boolean(true);
            registers[consumer.call + 1] = payload;
        }
        self.gc.pending = false;
        Some(next_pc)
    }
}
