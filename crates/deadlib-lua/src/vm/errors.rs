//! Operand provenance is reconstructed only when formatting a runtime failure.
use super::*;
use crate::bytecode::{Instruction, NameKind, Prototype};
use crate::memory::fallible::{self, SliceExt, VecExt};

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
            Op::Call | Op::CallIter | Op::TailCall | Op::Vararg => register >= a,
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
                let name = if instruction.c >= crate::bytecode::RK {
                    if let Constant::String(name) =
                        &proto.constants[(instruction.c - crate::bytecode::RK) as usize]
                    {
                        name.as_ref()
                    } else {
                        b"?"
                    }
                } else if local_name(proto, producer, key).is_none()
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
    /// Rust diagnostics own their bytes independently of the VM. A failed copy
    /// uses the startup-rooted memory value, including during persistent denial.
    pub(super) fn runtime_message(&self, bytes: Result<Vec<u8>, Error>) -> Error {
        match bytes {
            Ok(bytes) => {
                let error = Error::try_from_bytes(ErrorKind::Runtime, 0, bytes);
                if error.kind() == ErrorKind::Memory {
                    self.memory_failure()
                } else {
                    error
                }
            }
            Err(_) => self.memory_failure(),
        }
    }
    pub(super) fn error_value(&mut self, error: &Error, level: isize) -> Result<Value, Error> {
        if error.kind() == ErrorKind::Memory {
            return Ok(Value::String(self.memory_error));
        }
        if let Some(value) = error.value() {
            return Ok(value);
        }
        let value = self.try_intern(error.message_bytes())?;
        self.try_located_error(value, level)
    }
    pub(super) fn attach_error_value(&mut self, error: Error, level: isize) -> Error {
        if error.kind() == ErrorKind::Memory {
            return error.with_value(Value::String(self.memory_error));
        }
        if error.kind() != ErrorKind::Runtime || error.value().is_some() {
            return error;
        }
        match self.error_value(&error, level) {
            Ok(value) => error.with_value(value),
            Err(mut failed) => {
                failed.target_thread = error.target_thread;
                failed
            }
        }
    }
    #[cold]
    pub(super) fn argument_error(&self, index: usize, detail: &[u8]) -> Error {
        self.argument_error_at(index as i64 + 1, detail)
    }

    #[cold]
    pub(super) fn argument_error_at(&self, argument: i64, detail: &[u8]) -> Error {
        let build = || -> Result<Vec<u8>, Error> {
            let frame = self
                .active_native
                .or_else(|| self.frames.len().checked_sub(1));
            let (name, kind) = frame
                .and_then(|frame| self.debug_name_ref(self.thread, frame))
                .unwrap_or((b"?", b""));
            let method = kind == b"method";
            let argument = argument.saturating_sub(i64::from(method));
            let bad_self = method && argument == 0;
            let mut bytes = if bad_self {
                b"calling '".try_to_vec()?
            } else {
                fallible::format(format_args!("bad argument #{argument} to '"))?.into_bytes()
            };
            bytes.try_extend_from_slice(name)?;
            bytes.try_extend_from_slice(if bad_self { b"' on bad self (" } else { b"' (" })?;
            bytes.try_extend_from_slice(detail)?;
            bytes.try_push(b')')?;
            Ok(bytes)
        };
        self.runtime_message(build())
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
        match fallible::format(format_args!("{expected} expected, got {actual}")) {
            Ok(detail) => self.argument_error(index, detail.as_bytes()),
            Err(_) => self.memory_failure(),
        }
    }

    #[cold]
    pub(super) fn argument_option_error(&self, index: usize, option: &[u8]) -> Error {
        let build = || -> Result<Vec<u8>, Error> {
            let mut detail = b"invalid option '".try_to_vec()?;
            detail.try_extend_from_slice(
                &option[..option.iter().position(|&b| b == 0).unwrap_or(option.len())],
            )?;
            detail.try_push(b'\'')?;
            Ok(detail)
        };
        match build() {
            Ok(detail) => self.argument_error(index, &detail),
            Err(_) => self.memory_failure(),
        }
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
            Op::Call | Op::CallIter | Op::TailCall | Op::SetTable => usize::from(instruction.a),
            Op::GetTable | Op::Neg | Op::Len => usize::from(instruction.b),
            Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Mod
            | Op::Pow
            | Op::Concat
            | Op::ConcatRange => {
                if right {
                    instruction.c as usize
                } else {
                    usize::from(instruction.b)
                }
            }
            _ => return None,
        };
        (register < crate::bytecode::RK as usize).then_some(frame.base + register)
    }

    #[cold]
    pub(super) fn type_error(
        &mut self,
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
            if matches!(instruction.op, Op::Call | Op::CallIter | Op::TailCall)
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
        let build = || -> Result<Vec<u8>, Error> {
            let mut message =
                fallible::format(format_args!("attempt to {operation} "))?.into_bytes();
            if let Some((name, kind)) = name {
                message.try_extend_from_slice(kind.bytes())?;
                message.try_extend_from_slice(b" '")?;
                message.try_extend_from_slice(
                    &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())],
                )?;
                message.try_extend_from_slice(
                    fallible::format(format_args!("' (a {} value)", value.type_name()))?.as_bytes(),
                )?;
            } else {
                message.try_extend_from_slice(
                    fallible::format(format_args!("a {} value", value.type_name()))?.as_bytes(),
                )?;
            }
            Ok(message)
        };
        let error = self.runtime_message(build());
        self.operand_error(error)
    }

    #[cold]
    pub(super) fn operand_error(&mut self, error: Error) -> Error {
        // VM/API operand errors originate in the current visible call frame.
        // luaL_error argument/domain diagnostics separately skip their C frame.
        if self
            .frames
            .iter()
            .rev()
            .find(|f| f.is_call())
            .is_some_and(|f| matches!(f.function, FrameKind::Native(_)))
        {
            self.attach_error_value(error, 0)
        } else {
            error
        }
    }
}
