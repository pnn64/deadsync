//! Prove that the next Lua reader call returns nil without observable effects.
use super::*;

pub(super) fn returns_nil(vm: &Vm, reader: Value) -> bool {
    let Value::Function(reader) = reader else {
        return false;
    };
    let closure = &vm.functions[slot_index(reader.0)];
    let program = &vm.programs[closure.program];
    let prototype = &program.data.prototypes[closure.prototype];
    let mut registers = [Value::Nil; 12];
    if prototype.compat_arg || usize::from(prototype.registers) > registers.len() {
        return false;
    }
    let constants = program.bound_constants(closure.prototype);
    let mut pc = 0;
    // Reject long or cyclic paths, and leave every VM value unchanged.
    for _ in 0..32 {
        let Some(i) = prototype.code().get(pc).copied() else {
            return false;
        };
        let a = usize::from(i.a);
        let b = usize::from(i.b);
        let c = i.c as usize;
        pc += 1;
        match i.op {
            Op::Load if i.b == 0 => registers[a] = constants[c],
            // The scratch registers start nil, and none of these instructions
            // can open an upvalue in the reader's own frame.
            Op::Load | Op::Close => {}
            Op::Move if i.c != OPEN => registers[a] = registers[b],
            Op::GetUpvalue => registers[a] = vm.cell_value(closure.upvalues[c] as usize),
            Op::SetUpvalue
                if registers[a] == Value::Nil
                    && vm.cell_value(closure.upvalues[c] as usize) == Value::Nil => {}
            Op::Not => registers[a] = Value::Boolean(!registers[b].is_truthy()),
            Op::Bool if c == 0 => registers[a] = registers[b],
            Op::Bool => registers[a] = Value::Boolean(!registers[b].is_truthy()),
            Op::Jump if i.b == 0 => pc = c,
            Op::JumpFalse | Op::JumpTrue => {
                if registers[a].is_truthy() == (i.op == Op::JumpTrue) {
                    pc = c;
                }
            }
            Op::Return if c == 0 => return true,
            Op::Return if i.c != OPEN => return registers[a] == Value::Nil,
            _ => return false,
        }
    }
    false
}
