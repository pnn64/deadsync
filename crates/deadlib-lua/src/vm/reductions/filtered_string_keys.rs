//! Execute a guarded numeric loop that builds string keys behind a remainder test.
use super::{Cell, FrameKind, Instruction, Op, Prototype, TableId, Value, Vm, integer, slot_index};
use crate::{
    Error,
    bytecode::{COMPARE_BRANCH, RK},
    vm::{StringId, math::MATH_FMOD},
};

struct KeyLoop {
    control: usize,
    target: TableId,
    prefix: StringId,
    divisor: u32,
    skip: f64,
    first: u32,
    last: u32,
}

impl KeyLoop {
    fn decode(
        vm: &Vm,
        code: &[Instruction],
        base: usize,
        body: usize,
        backedge: usize,
    ) -> Option<Self> {
        let end = *code.get(backedge)?;
        if end.op != Op::ForLoop
            || end.c as usize != body
            || end.a != code.get(body.checked_sub(1)?)?.a
            || end.a > 249
        {
            return None;
        }
        let control = end.a;
        let index = control + 3;
        let scratch = control + 4;
        let i = u32::from(index);
        let s = u32::from(scratch);
        let b = u16::from(scratch);
        let window = code.get(body..backedge)?;
        if window.len() != 12 {
            return None;
        }
        let library = window[0].c;
        let modulo = window[1].c;
        let divisor = window[3].c;
        let skip = window[5].c;
        let prefix = window[7].c;
        let target = window[10].a;
        let expected = [
            (Op::GetGlobal, scratch, 0, library),
            (Op::GetTable, scratch, b, modulo),
            (Op::Move, scratch + 1, u16::from(index), 0),
            (Op::Load, scratch + 2, 0, divisor),
            (Op::Call, scratch, 2, 1),
            (Op::Ne, scratch + 2, b, skip),
            (
                Op::JumpFalse,
                scratch + 2,
                COMPARE_BRANCH,
                (backedge - 1) as u32,
            ),
            (Op::Load, scratch, 0, prefix),
            (Op::Move, scratch + 1, u16::from(index), 0),
            (Op::Concat, scratch, b, s + 1),
            (Op::SetTable, target, b, i),
            (Op::Close, index, 0, 0),
        ];
        if target >= control
            || !window
                .iter()
                .zip(expected)
                .all(|(op, expected)| (op.op, op.a, op.b, op.c) == expected)
        {
            return None;
        }

        let control = base + usize::from(control);
        let first = u32::try_from(integer(vm.registers[control])? + 1).ok()?;
        let last = u32::try_from(integer(vm.registers[control + 1])?).ok()?;
        if last.saturating_sub(first) < 16
            || vm.registers[control + 2] != Value::Number(1.0)
            || vm.open_upvalues.iter().any(|&id| {
                matches!(vm.upvalues[id as usize], Cell::Open(thread, register)
                    if thread == vm.thread && register >= control)
            })
        {
            return None;
        }
        let FrameKind::Lua(function) = vm.frames.last()?.function else {
            return None;
        };
        let closure = &vm.functions[slot_index(function.0)];
        let constants = vm.programs[closure.program].bound_constants(closure.prototype);
        let Value::String(library) = *constants.get(library as usize)? else {
            return None;
        };
        let Value::String(modulo) = *constants.get(modulo.checked_sub(RK)? as usize)? else {
            return None;
        };
        let Value::Table(math) = vm.tables[slot_index(closure.env.0)].get_string(library) else {
            return None;
        };
        if vm.tables[slot_index(math.0)].get_string(modulo) != Value::Native(MATH_FMOD) {
            return None;
        }
        let Value::Table(target) = vm.registers[base + usize::from(target)] else {
            return None;
        };
        if target == closure.env
            || target == math
            || vm.tables[slot_index(target.0)].metatable().is_some()
        {
            return None;
        }
        let divisor = u32::try_from(integer(*constants.get(divisor as usize)?)?).ok()?;
        let Value::Number(skip) = *constants.get(skip.checked_sub(RK)? as usize)? else {
            return None;
        };
        let Value::String(prefix) = *constants.get(prefix as usize)? else {
            return None;
        };
        (divisor != 0).then_some(Self {
            control,
            target,
            prefix,
            divisor,
            skip,
            first,
            last,
        })
    }
}

struct Digits {
    bytes: [u8; 10],
    start: usize,
}

impl Digits {
    fn new(mut value: u32) -> Self {
        let mut bytes = [b'0'; 10];
        let mut start = bytes.len() - 1;
        while value >= 10 {
            bytes[start] += (value % 10) as u8;
            value /= 10;
            start -= 1;
        }
        bytes[start] += value as u8;
        Self { bytes, start }
    }

    fn advance(&mut self) {
        for digit in self.bytes[self.start..].iter_mut().rev() {
            if *digit != b'9' {
                *digit += 1;
                return;
            }
            *digit = b'0';
        }
        self.start -= 1;
        self.bytes[self.start] = b'1';
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes[self.start..]
    }
}

impl Vm {
    #[cold]
    pub(in crate::vm) fn reduce_filtered_string_keys(
        &mut self,
        prototype: &Prototype,
        base: usize,
        body: usize,
        backedge: usize,
        error_line: &mut u32,
    ) -> Result<Option<usize>, Error> {
        if !self.sort_fast_context()
            || !self.registers.memory().uses_global_allocator()
            || self.frames.len() == self.frames.capacity()
            || self.frames.len() + 1 >= usize::from(self.call_slots)
        {
            return Ok(None);
        }
        let Some(plan) = KeyLoop::decode(self, &prototype.code, base, body, backedge) else {
            return Ok(None);
        };
        let frame = self.frames.len() - 1;
        let scratch = plan.control + 4;
        let mut remainder = plan.first % plan.divisor;
        let mut field_hint = 0;
        let mut digits = Digits::new(plan.first);
        let mut gc_limit = self.string_store_limit();
        for index in plan.first..=plan.last {
            // For nonnegative integers both native fmod and integer remainder
            // are exact. Leave any native call with a live collector checkpoint
            // to ordinary dispatch, with the previous loop index intact.
            if self.string_store_debt() >= gc_limit {
                break;
            }
            self.gc.pending = false;
            let value = Value::Number(f64::from(index));
            let numeric_remainder = f64::from(remainder);
            remainder += 1;
            if remainder == plan.divisor {
                remainder = 0;
            }
            let store = numeric_remainder != plan.skip;
            self.registers[plan.control] = value;
            self.registers[plan.control + 3] = value;
            self.registers[scratch] = Value::Number(numeric_remainder);
            self.registers[scratch + 1] = value;
            self.registers[scratch + 2] = Value::Boolean(store);
            self.top = scratch + 1;
            if !store {
                digits.advance();
                continue;
            }

            // Before each fallible operation, record the same registers and PC
            // as dispatch. A scheduled finalizer resumes at the next Lua op.
            self.registers[scratch] = Value::String(plan.prefix);
            let write = body + 10;
            self.frames[frame].pc = write as u32;
            *error_line = prototype.instruction_line(write - 1);
            let mut bytes = self.try_take_buffer(0)?;
            bytes
                .try_extend_from_slice(self.string_bytes(plan.prefix).unwrap_or_default())
                .and_then(|_| bytes.try_extend_from_slice(digits.as_bytes()))
                .map_err(|_| self.memory_failure())?;
            self.registers[scratch] = self.try_finish_buffer(bytes)?;
            digits.advance();
            if self.string_store_debt() >= gc_limit {
                self.maybe_gc()?;
                if self.frames.len() != frame + 1 {
                    return Ok(Some(write));
                }
                gc_limit = self.string_store_limit();
            }
            let close = write + 1;
            self.frames[frame].pc = close as u32;
            *error_line = prototype.instruction_line(write);
            let key = self.registers[scratch];
            let allow_new = self.gc_can_change_field_liveness();
            if !matches!(key, Value::String(name) if !self.gc.pending
                && self.tables[slot_index(plan.target.0)]
                    .set_string_scalar_in_place(name, value, &mut field_hint, allow_new))
            {
                self.raw_set(plan.target, key, value)?;
            }
            if self.gc.pending {
                if self.string_store_debt() >= gc_limit {
                    self.maybe_gc()?;
                    if self.frames.len() != frame + 1 {
                        return Ok(Some(close));
                    }
                    gc_limit = self.string_store_limit();
                } else {
                    self.gc.pending = false;
                }
            }
        }
        Ok(Some(backedge))
    }
}
