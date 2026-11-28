//! Return an unchanged fixed parameter without constructing a Lua frame.
use super::{FunctionId, Value, Vm, slot_index};
use crate::bytecode::{OPEN_ARGS, Op};

#[derive(Clone, Copy)]
pub(super) struct Projection {
    argument: u8,
    registers: u16,
    line: u32,
}

#[derive(Default)]
pub(super) struct Cache([Option<(FunctionId, Option<Projection>)>; 8]);

impl Cache {
    #[inline]
    pub(super) fn get(&mut self, vm: &Vm, function: FunctionId) -> Option<Projection> {
        let index = slot_index(function.0) % self.0.len();
        if let Some((cached, projection)) = self.0[index]
            && cached == function
        {
            return projection;
        }
        let projection = Self::decode(vm, function);
        self.0[index] = Some((function, projection));
        projection
    }

    #[cold]
    fn decode(vm: &Vm, function: FunctionId) -> Option<Projection> {
        let closure = &vm.functions[slot_index(function.0)];
        let proto = &vm.programs[closure.program].data.prototypes[closure.prototype];
        if proto.vararg || proto.compat_arg {
            return None;
        }
        let first = proto.code()[0];
        let (argument, pc) = match first.op {
            Op::Return if first.c == 1 => (first.a, 0),
            Op::Move if first.c == 0 => {
                let result = proto.code().get(1)?;
                if result.op != Op::Return || result.c != 1 || result.a != first.a {
                    return None;
                }
                (u8::try_from(first.b).ok()?, 1)
            }
            _ => return None,
        };
        (u16::from(argument) < proto.params).then(|| Projection {
            argument,
            registers: proto.registers,
            line: proto.instruction_line(pc),
        })
    }
}

impl Vm {
    #[inline]
    pub(super) fn try_return_argument(
        &mut self,
        projection: Projection,
        call: usize,
        args: u16,
        wanted: u32,
        checkpoint_ready: bool,
        error_line: &mut u32,
    ) -> bool {
        let count = if args == OPEN_ARGS {
            let Some(count) = self.top.checked_sub(call + 1) else {
                return false;
            };
            count
        } else {
            usize::from(args)
        };
        if !self.has_numeric_call_space(usize::from(projection.registers))
            || !checkpoint_ready && !(self.sort_fast_context() && self.sort_gc_ready())
        {
            return false;
        }
        self.top = call + usize::from(wanted != 0);
        if wanted != 0 {
            let argument = usize::from(projection.argument);
            self.registers[call] = if argument < count {
                self.registers[call + 1 + argument]
            } else {
                Value::Nil
            };
        }
        *error_line = projection.line;
        true
    }
}
