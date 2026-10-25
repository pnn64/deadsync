//! Operand provenance is reconstructed only when formatting a runtime failure.
use super::*;
use crate::bytecode::{Instruction, NameKind, Prototype};

fn definition(proto: &Prototype, end: usize, register: u8) -> Option<(usize, Instruction)> {
    let mut last = None;
    let mut pc = 0;
    while pc < end {
        let instruction = proto.code[pc];
        let a = instruction.a;
        let test_only = matches!(instruction.op, Op::JumpFalse | Op::JumpTrue)
            && (proto.debug.hook_lines[pc] == 0
                || last.is_some_and(|(_, previous): (usize, Instruction)| {
                    previous.c as usize == pc
                        && matches!(
                            (previous.op, instruction.op),
                            (Op::JumpFalse, Op::JumpTrue) | (Op::JumpTrue, Op::JumpFalse)
                        )
                }));
        let writes = match instruction.op {
            Op::SetGlobal
            | Op::SetTable
            | Op::SetUpvalue
            | Op::SetList
            | Op::Close
            | Op::Jump
            | Op::Return => false,
            Op::Call | Op::TailCall | Op::Vararg => register >= a,
            Op::Move if instruction.c == OPEN => register >= a,
            Op::ForPrep | Op::ForLoop => (a..=a + 3).contains(&register),
            Op::JumpFalse | Op::JumpTrue if test_only => false,
            _ => register == a,
        };
        if writes {
            last = Some((pc, instruction));
        }
        if matches!(
            instruction.op,
            Op::Jump | Op::JumpFalse | Op::JumpTrue | Op::ForPrep | Op::ForLoop
        ) && !test_only
            && instruction.c as usize > pc
            && instruction.c as usize <= end
        {
            pc = instruction.c as usize;
        } else {
            pc += 1;
        }
    }
    last
}

fn local_name(proto: &Prototype, pc: usize, register: u8) -> Option<&[u8]> {
    proto.debug.locals.iter().rev().find_map(|local| {
        (local.register == u16::from(register)
            && local.start as usize <= pc
            && pc < local.end as usize)
            .then_some(local.name.as_ref())
    })
}

fn object_name(proto: &Prototype, pc: usize, mut register: u8) -> Option<(&[u8], NameKind)> {
    loop {
        if let Some(name) = local_name(proto, pc, register) {
            return Some((name, NameKind::Local));
        }
        let (producer, instruction) = definition(proto, pc, register)?;
        match instruction.op {
            Op::GetGlobal => {
                let Constant::String(name) = &proto.constants[instruction.c as usize] else {
                    return None;
                };
                return Some((name, NameKind::Global));
            }
            Op::GetUpvalue => {
                return Some((
                    &proto.debug.upvalues[instruction.c as usize],
                    NameKind::Upvalue,
                ));
            }
            Op::GetTable => {
                let key = instruction.c as u8;
                let name = if local_name(proto, producer, key).is_none()
                    && let Some((_, load)) = definition(proto, producer, key)
                    && load.op == Op::Load
                    && let Constant::String(name) = &proto.constants[load.c as usize]
                {
                    name.as_ref()
                } else {
                    b"?"
                };
                return Some((name, NameKind::Field));
            }
            Op::Move if instruction.c != OPEN && instruction.b < u16::from(register) => {
                register = instruction.b as u8;
            }
            _ => return None,
        }
    }
}

impl Vm {
    #[cold]
    pub(super) fn argument_error(&self, index: usize, detail: &[u8]) -> Error {
        let frame = self
            .active_native
            .or_else(|| self.frames.len().checked_sub(1));
        let name = frame.and_then(|frame| self.debug_name(self.thread, frame));
        let (name, method) = name
            .as_ref()
            .map_or((b"?".as_slice(), false), |(name, kind)| {
                (name.as_ref(), *kind == b"method")
            });
        let mut bytes = if method && index == 0 {
            b"calling '".to_vec()
        } else {
            format!("bad argument #{} to '", index + usize::from(!method)).into_bytes()
        };
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(if method && index == 0 {
            b"' on bad self ("
        } else {
            b"' ("
        });
        bytes.extend_from_slice(detail);
        bytes.push(b')');
        Error::from_bytes(ErrorKind::Runtime, 0, bytes)
    }

    #[cold]
    pub(super) fn argument_type_error(
        &self,
        base: usize,
        count: usize,
        index: usize,
        expected: &str,
    ) -> Error {
        let actual = if index < count {
            self.argument(base, count, index).type_name()
        } else {
            "no value"
        };
        self.argument_error(
            index,
            format!("{expected} expected, got {actual}").as_bytes(),
        )
    }

    #[cold]
    pub(super) fn argument_option_error(&self, index: usize, option: &[u8]) -> Error {
        let mut detail = b"invalid option '".to_vec();
        detail.extend_from_slice(
            &option[..option.iter().position(|&b| b == 0).unwrap_or(option.len())],
        );
        detail.push(b'\'');
        self.argument_error(index, &detail)
    }

    #[cold]
    pub(super) fn error_register(&self, right: bool) -> Option<usize> {
        let frame = self.frames.last()?;
        let FrameKind::Lua(id) = frame.function else {
            return None;
        };
        let closure = &self.functions[slot_index(id.0)];
        let proto = &self.programs[closure.program].data.prototypes[closure.prototype];
        let instruction = proto.code[frame.pc().checked_sub(1)?];
        let register = match instruction.op {
            Op::Call | Op::TailCall | Op::SetTable => usize::from(instruction.a),
            Op::GetTable | Op::Neg | Op::Len => usize::from(instruction.b),
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow | Op::Concat => {
                if right {
                    instruction.c as usize
                } else {
                    usize::from(instruction.b)
                }
            }
            _ => return None,
        };
        Some(frame.base + register)
    }

    #[cold]
    pub(super) fn type_error(
        &self,
        value: Value,
        operation: &str,
        register: Option<usize>,
    ) -> Error {
        let name = register.and_then(|register| {
            let frame = self.frames.last()?;
            let FrameKind::Lua(id) = frame.function else {
                return None;
            };
            let closure = &self.functions[slot_index(id.0)];
            let proto = &self.programs[closure.program].data.prototypes[closure.prototype];
            let register = register.checked_sub(frame.base)?;
            if register >= usize::from(proto.registers) {
                return None;
            }
            let pc = frame.pc().checked_sub(1)?;
            let instruction = proto.code[pc];
            if matches!(instruction.op, Op::Call | Op::TailCall)
                && register == usize::from(instruction.a)
                && let Ok(index) = proto
                    .debug
                    .calls
                    .binary_search_by_key(&(pc as u32), |c| c.pc)
            {
                let call = &proto.debug.calls[index];
                if matches!(call.kind, NameKind::Method) {
                    return Some((call.name.as_ref(), call.kind));
                }
            }
            object_name(proto, pc, register as u8)
        });
        let mut message = format!("attempt to {operation} ").into_bytes();
        if let Some((name, kind)) = name {
            message.extend_from_slice(kind.bytes());
            message.extend_from_slice(b" '");
            message.extend_from_slice(
                &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())],
            );
            message.extend_from_slice(format!("' (a {} value)", value.type_name()).as_bytes());
        } else {
            message.extend_from_slice(format!("a {} value", value.type_name()).as_bytes());
        }
        Error::from_bytes(ErrorKind::Runtime, 0, message)
    }
}
