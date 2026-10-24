//! Register instructions and structural verification of immutable compiled programs.

use crate::{Error, ErrorKind};
use std::sync::Arc;

pub(crate) const OPEN: u32 = u32::MAX;
pub(crate) const OPEN_ARGS: u16 = u16::MAX;

pub(crate) fn chunk_id(name: &[u8]) -> Vec<u8> {
    let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
    if let Some(name) = name.strip_prefix(b"=") {
        return name[..name.len().min(59)].to_vec();
    }
    if let Some(name) = name.strip_prefix(b"@") {
        let limit = 60 - b" '...' \0".len();
        if name.len() > limit {
            let mut out = b"...".to_vec();
            out.extend_from_slice(&name[name.len() - limit..]);
            return out;
        }
        return name.to_vec();
    }
    let limit = 60 - b" [string \"...\"] \0".len();
    let len = name
        .iter()
        .position(|&b| matches!(b, b'\n' | b'\r'))
        .unwrap_or(name.len())
        .min(limit);
    let mut out = b"[string \"".to_vec();
    out.extend_from_slice(&name[..len]);
    if len < name.len() {
        out.extend_from_slice(b"...");
    }
    out.extend_from_slice(b"\"]");
    out
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Op {
    Load,
    Move,
    GetGlobal,
    SetGlobal,
    NewTable,
    GetTable,
    SetTable,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Neg,
    Not,
    Len,
    Jump,
    JumpFalse,
    JumpTrue,
    ForPrep,
    ForLoop,
    Call,
    Return,
    Closure,
    GetUpvalue,
    SetUpvalue,
    Close,
    Vararg,
    SetList,
    TailCall,
}

/// Eight-byte instructions keep full-width constant and jump indices.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub(crate) struct Instruction {
    pub op: Op,
    pub a: u8,
    pub b: u16,
    pub c: u32,
}

#[derive(Clone, Debug)]
pub(crate) enum Constant {
    Nil,
    Boolean(bool),
    Number(f64),
    String(Box<[u8]>),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Upvalue {
    Local(u8),
    Outer(u16),
}

#[derive(Clone, Debug)]
pub(crate) struct Prototype {
    pub code: Box<[Instruction]>,
    pub lines: Box<[u32]>,
    pub constants: Box<[Constant]>,
    pub registers: u16,
    pub params: u16,
    pub vararg: bool,
    pub compat_arg: bool,
    pub upvalues: Box<[Upvalue]>,
}

#[derive(Clone, Debug)]
pub(crate) struct ProgramData {
    pub prototypes: Box<[Prototype]>,
    pub entry: u32,
    pub name: Box<[u8]>,
}

/// Verified bytecode and immutable constants, independent of any runtime heap.
#[derive(Clone, Debug)]
pub struct Program {
    pub(crate) data: Arc<ProgramData>,
}

impl Program {
    /// Return the number of register instructions.
    pub fn instruction_count(&self) -> usize {
        self.data.prototypes.iter().map(|p| p.code.len()).sum()
    }

    /// Return the number of statically reserved register slots.
    pub fn register_count(&self) -> u16 {
        self.data.prototypes[self.data.entry as usize].registers
    }

    pub(crate) fn new(prototypes: Vec<Prototype>, entry: u32, name: &[u8]) -> Self {
        Self {
            data: Arc::new(ProgramData {
                prototypes: prototypes.into_boxed_slice(),
                entry,
                name: name.into(),
            }),
        }
    }

    /// Verify operands, register ranges, constants, and control flow targets.
    ///
    /// # Errors
    /// Returns a bytecode error if any instruction is malformed.
    pub fn verify(&self) -> Result<(), Error> {
        if self
            .data
            .prototypes
            .get(self.data.entry as usize)
            .is_none_or(|p| !p.upvalues.is_empty())
        {
            return Err(Error::new(
                ErrorKind::Bytecode,
                0,
                "invalid entry prototype",
            ));
        }
        for prototype in &self.data.prototypes {
            prototype.verify(&self.data.prototypes)?;
        }
        Ok(())
    }
}

impl Prototype {
    fn verify(&self, prototypes: &[Prototype]) -> Result<(), Error> {
        let fail = |pc: usize, message: &str| {
            Error::new(
                ErrorKind::Bytecode,
                self.lines.get(pc).copied().unwrap_or(0),
                message,
            )
        };
        if self.registers > 256
            || self.registers == 0
            || self.params > self.registers
            || self.upvalues.len() > 60
            || self.lines.len() != self.code.len()
            || self.code.is_empty()
        {
            return Err(fail(0, "invalid program layout"));
        }
        for (pc, i) in self.code.iter().enumerate() {
            let reg = |r: u32| {
                if r < u32::from(self.registers) {
                    Ok(())
                } else {
                    Err(fail(pc, "register out of bounds"))
                }
            };
            let range = |start: u32, count: u32| {
                if start
                    .checked_add(count)
                    .is_some_and(|end| end <= u32::from(self.registers))
                {
                    Ok(())
                } else {
                    Err(fail(pc, "register range out of bounds"))
                }
            };
            match i.op {
                Op::Load | Op::GetGlobal | Op::SetGlobal => {
                    reg(u32::from(i.a))?;
                    let k = self
                        .constants
                        .get(i.c as usize)
                        .ok_or_else(|| fail(pc, "constant out of bounds"))?;
                    if i.op != Op::Load && !matches!(k, Constant::String(_)) {
                        return Err(fail(pc, "global name must be a string"));
                    }
                }
                Op::NewTable => reg(u32::from(i.a))?,
                Op::SetList => {
                    reg(u32::from(i.a))?;
                    if i.b != OPEN_ARGS {
                        range(u32::from(i.a) + 1, u32::from(i.b))?;
                    }
                    if i.c == 0 {
                        return Err(fail(pc, "invalid array starting index"));
                    }
                }
                Op::Closure => {
                    reg(u32::from(i.a))?;
                    let child = prototypes
                        .get(i.c as usize)
                        .ok_or_else(|| fail(pc, "prototype out of bounds"))?;
                    for capture in &child.upvalues {
                        match capture {
                            Upvalue::Local(r) => reg(u32::from(*r))?,
                            Upvalue::Outer(u) if usize::from(*u) < self.upvalues.len() => {}
                            _ => return Err(fail(pc, "capture out of bounds")),
                        }
                    }
                }
                Op::GetUpvalue | Op::SetUpvalue => {
                    reg(u32::from(i.a))?;
                    if i.c as usize >= self.upvalues.len() {
                        return Err(fail(pc, "upvalue out of bounds"));
                    }
                }
                Op::Close => reg(u32::from(i.a))?,
                Op::Vararg => {
                    if !self.vararg {
                        return Err(fail(pc, "vararg in fixed-arity function"));
                    }
                    if i.c == OPEN {
                        reg(u32::from(i.a))?;
                    } else {
                        range(u32::from(i.a), i.c)?;
                    }
                }
                Op::Move | Op::Neg | Op::Not | Op::Len => {
                    reg(u32::from(i.a))?;
                    reg(u32::from(i.b))?;
                }
                Op::Add
                | Op::Sub
                | Op::Mul
                | Op::Div
                | Op::Mod
                | Op::Pow
                | Op::Concat
                | Op::Eq
                | Op::Ne
                | Op::Lt
                | Op::Le
                | Op::Gt
                | Op::Ge
                | Op::GetTable
                | Op::SetTable => {
                    reg(u32::from(i.a))?;
                    reg(u32::from(i.b))?;
                    reg(i.c)?;
                }
                Op::ForPrep | Op::ForLoop => {
                    range(u32::from(i.a), 4)?;
                }
                Op::JumpFalse | Op::JumpTrue => reg(u32::from(i.a))?,
                Op::Jump => {}
                Op::Call | Op::TailCall => {
                    reg(u32::from(i.a))?;
                    if i.b != OPEN_ARGS {
                        range(u32::from(i.a) + 1, u32::from(i.b))?;
                    }
                    if i.op == Op::Call && i.c != OPEN {
                        range(u32::from(i.a), i.c)?;
                    }
                }
                Op::Return => {
                    if i.c != OPEN {
                        range(u32::from(i.a), i.c)?;
                    } else {
                        reg(u32::from(i.a))?;
                    }
                }
            }
            if matches!(
                i.op,
                Op::Jump | Op::JumpFalse | Op::JumpTrue | Op::ForPrep | Op::ForLoop
            ) && i.c as usize >= self.code.len()
            {
                return Err(fail(pc, "jump target out of bounds"));
            }
            if pc + 1 == self.code.len() && !matches!(i.op, Op::Return | Op::Jump | Op::TailCall) {
                return Err(fail(pc, "instruction can fall off end of program"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_rejects_corrupt_operands_and_edges() {
        let good = crate::compile(b"local x = 1; while x < 3 do x = x + 1 end; return x").unwrap();
        for pc in 0..good.instruction_count() {
            let mut p = good.clone();
            let data = Arc::make_mut(&mut p.data);
            let prototype = &mut data.prototypes[data.entry as usize];
            prototype.code[pc] = Instruction {
                op: Op::Jump,
                a: 0,
                b: 0,
                c: u32::MAX,
            };
            assert_eq!(p.verify().unwrap_err().kind(), ErrorKind::Bytecode);
            let data = Arc::make_mut(&mut p.data);
            data.prototypes[data.entry as usize].code[pc] = Instruction {
                op: Op::Load,
                a: 0,
                b: 0,
                c: u32::MAX,
            };
            assert!(p.verify().is_err());
            let data = Arc::make_mut(&mut p.data);
            data.prototypes[data.entry as usize].code[pc] = Instruction {
                op: Op::Move,
                a: 0,
                b: u16::MAX,
                c: 0,
            };
            assert!(p.verify().is_err());
        }
    }
}
