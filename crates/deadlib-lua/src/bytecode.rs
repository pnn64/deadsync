//! Register instructions and structural verification of immutable compiled programs.

use crate::{Error, ErrorKind};
use std::sync::Arc;

pub(crate) const OPEN: u32 = u32::MAX;
pub(crate) const OPEN_ARGS: u16 = u16::MAX;
pub(crate) const COMPARE_BRANCH: u16 = 1;
pub(crate) const BOOLEAN_SKIP: u16 = 2;
/// On RK-capable operands, values below this boundary are register indices;
/// larger values are constant indices plus this boundary.
pub(crate) const RK: u32 = 256;

pub(crate) fn chunk_id(name: &[u8]) -> Vec<u8> {
    chunk_id_limit(name, 60)
}

pub(crate) fn chunk_id_limit(name: &[u8], size: usize) -> Vec<u8> {
    let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
    if let Some(name) = name.strip_prefix(b"=") {
        return name[..name.len().min(size - 1)].to_vec();
    }
    if let Some(name) = name.strip_prefix(b"@") {
        let limit = size - b" '...' \0".len();
        if name.len() > limit {
            let mut out = b"...".to_vec();
            out.extend_from_slice(&name[name.len() - limit..]);
            return out;
        }
        return name.to_vec();
    }
    let limit = size - b" [string \"...\"] \0".len();
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
    Bool,
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
    CallIter,
    ConcatRange,
}
impl Op {
    pub(crate) fn rk_b(self) -> bool {
        matches!(
            self,
            Self::Add
                | Self::Sub
                | Self::Mul
                | Self::Div
                | Self::Mod
                | Self::Pow
                | Self::Eq
                | Self::Ne
                | Self::Lt
                | Self::Le
                | Self::Gt
                | Self::Ge
                | Self::SetTable
        )
    }
    pub(crate) fn rk_c(self) -> bool {
        self.rk_b() || self == Self::GetTable
    }
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
pub(crate) struct ClosureBinding {
    pub prototype: u32,
    pub upvalues: Box<[Upvalue]>,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeDebug {
    // Zero selects the program name; other indices select its source pool.
    pub source: u32,
    pub stack: u16,
    pub has_lines: bool,
    pub wire: crate::chunk51::Wire,
}

#[derive(Clone, Debug)]
pub(crate) struct DebugLocal {
    pub name: Box<[u8]>,
    pub start: u32,
    pub end: u32,
    pub register: u16,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum NameKind {
    Unknown,
    Local,
    Global,
    Upvalue,
    Field,
    Method,
}
impl NameKind {
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Unknown => b"",
            Self::Local => b"local",
            Self::Global => b"global",
            Self::Upvalue => b"upvalue",
            Self::Field => b"field",
            Self::Method => b"method",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DebugCall {
    pub pc: u32,
    pub name: Box<[u8]>,
    pub kind: NameKind,
    pub temporaries: std::ops::Range<u32>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DebugInfo {
    pub stack: u16,
    pub native: Option<Box<NativeDebug>>,
    pub defined: [u32; 2],
    pub upvalues: Box<[Box<[u8]>]>,
    pub locals: Box<[DebugLocal]>,
    pub calls: Box<[DebugCall]>,
    pub active_lines: Box<[u32]>,
    pub hook_lines: Box<[u32]>,
    pub temporaries: Box<[u8]>,
}
impl DebugInfo {
    pub fn heap_bytes(&self) -> usize {
        self.native.as_ref().map_or(0, |n| {
            std::mem::size_of_val(n.as_ref()) + n.wire.heap_bytes()
        }) + std::mem::size_of_val(self.upvalues.as_ref())
            + std::mem::size_of_val(self.active_lines.as_ref())
            + std::mem::size_of_val(self.hook_lines.as_ref())
            + self.upvalues.iter().map(|n| n.len()).sum::<usize>()
            + std::mem::size_of_val(self.locals.as_ref())
            + self.locals.iter().map(|l| l.name.len()).sum::<usize>()
            + std::mem::size_of_val(self.calls.as_ref())
            + self.calls.iter().map(|c| c.name.len()).sum::<usize>()
            + self.temporaries.len()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Prototype {
    pub code: Box<[Instruction]>,
    pub lines: Box<[u32]>,
    pub gc_live: Box<[u32]>,
    pub gc_maps: Box<[[u64; 4]]>,
    pub constants: Box<[Constant]>,
    pub registers: u16,
    pub params: u16,
    pub vararg: bool,
    pub compat_arg: bool,
    pub upvalues: Box<[Upvalue]>,
    pub bindings: Box<[ClosureBinding]>,
    pub debug: DebugInfo,
}

#[derive(Clone, Debug)]
pub(crate) struct ProgramData {
    pub prototypes: Box<[Prototype]>,
    pub entry: u32,
    pub name: Box<[u8]>,
    pub sources: Box<[Box<[u8]>]>,
}

impl ProgramData {
    pub fn source(&self, prototype: &Prototype) -> &[u8] {
        match prototype.debug.native.as_ref().map_or(0, |n| n.source) {
            0 => &self.name,
            n => &self.sources[n as usize - 1],
        }
    }
}

/// Verified bytecode and immutable constants, independent of any runtime heap.
///
/// While the host holds a program or its clones, each VM that loaded it retains
/// its entry closure and function environment. Dropping the last host copy makes
/// those bindings collectible; escaped Lua closures still retain their code.
#[derive(Clone, Debug)]
pub struct Program {
    pub(crate) data: Arc<ProgramData>,
    // Runtime code owners do not keep the host's VM binding alive.
    pub(crate) lifetime: Arc<()>,
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
            lifetime: Arc::new(()),
            data: Arc::new(ProgramData {
                prototypes: prototypes.into_boxed_slice(),
                entry,
                name: name.into(),
                sources: Box::default(),
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
            .is_none_or(|p| !p.upvalues.is_empty() && p.debug.native.is_none())
        {
            return Err(Error::new(
                ErrorKind::Bytecode,
                0,
                "invalid entry prototype",
            ));
        }
        for prototype in &self.data.prototypes {
            if prototype
                .debug
                .native
                .as_ref()
                .is_some_and(|n| n.source as usize > self.data.sources.len())
            {
                return Err(Error::new(ErrorKind::Bytecode, 0, "invalid source index"));
            }
            prototype.verify(&self.data.prototypes)?;
        }
        Ok(())
    }
}

impl Prototype {
    pub fn debug_line(&self, pc: usize) -> i64 {
        if self.debug.native.as_ref().is_some_and(|n| !n.has_lines) {
            0
        } else {
            i64::from(self.lines[pc] as i32)
        }
    }

    pub fn debug_registers(&self) -> usize {
        usize::from(self.debug.native.as_ref().map_or(
            if self.debug.stack == 0 {
                self.registers
            } else {
                self.debug.stack
            },
            |n| n.stack,
        ))
    }

    fn verify(&self, prototypes: &[Prototype]) -> Result<(), Error> {
        let fail = |pc: usize, message: &str| {
            Error::new(
                ErrorKind::Bytecode,
                self.lines.get(pc).copied().unwrap_or(0),
                message,
            )
        };
        if self.registers > 256
            || self.debug.stack > self.registers
            || self.registers == 0
            || self.params > self.registers
            || self.upvalues.len() > if self.debug.native.is_some() { 255 } else { 60 }
            || self
                .debug
                .native
                .as_ref()
                .is_some_and(|n| n.stack == 0 || n.stack > self.registers)
            || self.lines.len() != self.code.len()
            || self.debug.hook_lines.len() != self.code.len()
            || self.gc_live.len() != self.code.len()
            || self.gc_live.iter().any(|&m| {
                (m as u16 as usize) >= self.gc_maps.len()
                    || ((m >> 16) as usize) >= self.gc_maps.len()
            })
            || self.gc_maps.iter().any(|mask| {
                (usize::from(self.registers)..256).any(|r| mask[r / 64] & (1u64 << (r % 64)) != 0)
            })
            || self.code.is_empty()
            || self.debug.upvalues.len() != self.upvalues.len()
            || self.debug.defined[0] > self.debug.defined[1]
            || self.debug.active_lines.first() == Some(&0)
            || self.debug.active_lines.windows(2).any(|l| l[0] >= l[1])
            || self.debug.locals.iter().any(|l| {
                l.start > l.end || l.end as usize > self.code.len() || l.register >= self.registers
            })
            || self.debug.calls.windows(2).any(|c| c[0].pc >= c[1].pc)
            || self.debug.calls.iter().any(|c| {
                let Some(i) = self.code.get(c.pc as usize) else {
                    return true;
                };
                !matches!(i.op, Op::Call | Op::TailCall | Op::CallIter)
                    || c.temporaries.start > c.temporaries.end
                    || c.temporaries.end as usize > self.debug.temporaries.len()
                    || self.debug.temporaries
                        [c.temporaries.start as usize..c.temporaries.end as usize]
                        .windows(2)
                        .any(|r| r[0] >= r[1])
                    || self.debug.temporaries
                        [c.temporaries.start as usize..c.temporaries.end as usize]
                        .iter()
                        .any(|&r| {
                            r >= i.a
                                || u16::from(r) >= self.registers
                                || self.gc_maps[self.gc_live[c.pc as usize] as u16 as usize]
                                    [usize::from(r) / 64]
                                    & (1u64 << (r % 64))
                                    == 0
                        })
            })
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
                    if i.op == Op::Load && (i.b > 1 || (i.b == 1 && !matches!(k, Constant::Nil))) {
                        return Err(fail(pc, "invalid initial nil marker"));
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
                    let (target, captures) = match i.b {
                        0 => (i.c, None),
                        1 => {
                            let binding = self
                                .bindings
                                .get(i.c as usize)
                                .ok_or_else(|| fail(pc, "closure binding out of bounds"))?;
                            (binding.prototype, Some(binding.upvalues.as_ref()))
                        }
                        _ => return Err(fail(pc, "invalid closure binding mode")),
                    };
                    let child = prototypes
                        .get(target as usize)
                        .ok_or_else(|| fail(pc, "prototype out of bounds"))?;
                    let captures = captures.unwrap_or(&child.upvalues);
                    if captures.len() != child.upvalues.len() {
                        return Err(fail(pc, "invalid capture count"));
                    }
                    for capture in captures {
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
                Op::Move | Op::Bool | Op::Neg | Op::Not | Op::Len => {
                    reg(u32::from(i.a))?;
                    reg(u32::from(i.b))?;
                    if i.op == Op::Bool
                        && (i.c > 1
                            || pc == 0
                            || !matches!(
                                self.code[pc - 1].op,
                                Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge
                            )
                            || u16::from(self.code[pc - 1].a) != i.b)
                    {
                        return Err(fail(pc, "invalid comparison materialization"));
                    }
                }
                Op::Add
                | Op::Sub
                | Op::Mul
                | Op::Div
                | Op::Mod
                | Op::Pow
                | Op::Eq
                | Op::Ne
                | Op::Lt
                | Op::Le
                | Op::Gt
                | Op::Ge
                | Op::GetTable
                | Op::SetTable => {
                    reg(u32::from(i.a))?;
                    let operand = |value: u32| {
                        if value < RK {
                            reg(value)
                        } else if (value - RK) as usize >= self.constants.len() {
                            Err(fail(pc, "constant operand out of bounds"))
                        } else {
                            Ok(())
                        }
                    };
                    if i.op.rk_b() {
                        operand(u32::from(i.b))?;
                    } else {
                        reg(u32::from(i.b))?;
                    }
                    operand(i.c)?;
                }
                Op::ForPrep | Op::ForLoop => {
                    range(u32::from(i.a), 4)?;
                }
                Op::Concat | Op::ConcatRange => {
                    reg(u32::from(i.a))?;
                    reg(u32::from(i.b))?;
                    reg(i.c)?;
                    if i.op == Op::ConcatRange && u32::from(i.b) >= i.c {
                        return Err(fail(pc, "invalid concatenation range"));
                    }
                }
                Op::JumpFalse | Op::JumpTrue => {
                    reg(u32::from(i.a))?;
                    if i.b > COMPARE_BRANCH
                        || (i.b == COMPARE_BRANCH
                            && (pc == 0
                                || !matches!(
                                    self.code[pc - 1].op,
                                    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge
                                )
                                || self.code[pc - 1].a != i.a))
                    {
                        return Err(fail(pc, "invalid comparison branch"));
                    }
                }
                Op::Jump => {
                    if i.b != 0
                        && (i.b != BOOLEAN_SKIP
                            || pc == 0
                            || i.c as usize != pc + 2
                            || !self.code.get(pc + 1).is_some_and(|next| {
                                let previous = self.code[pc - 1];
                                previous.op == Op::Load
                                    && next.op == Op::Load
                                    && previous.a == next.a
                                    && matches!(
                                        self.constants.get(previous.c as usize),
                                        Some(Constant::Boolean(false))
                                    )
                                    && matches!(
                                        self.constants.get(next.c as usize),
                                        Some(Constant::Boolean(true))
                                    )
                            }))
                    {
                        return Err(fail(pc, "invalid boolean skip"));
                    }
                }
                Op::Call | Op::TailCall | Op::CallIter => {
                    reg(u32::from(i.a))?;
                    if i.b != OPEN_ARGS {
                        range(u32::from(i.a) + 1, u32::from(i.b))?;
                    }
                    if i.op != Op::TailCall && i.c != OPEN {
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
            if matches!(
                i.op,
                Op::Jump | Op::JumpFalse | Op::JumpTrue | Op::ForPrep | Op::ForLoop
            ) && (self.code[i.c as usize].op == Op::Bool
                || matches!(
                    self.code[i.c as usize],
                    Instruction {
                        op: Op::JumpFalse | Op::JumpTrue,
                        b: COMPARE_BRANCH,
                        ..
                    } | Instruction {
                        op: Op::Jump,
                        b: BOOLEAN_SKIP,
                        ..
                    }
                ))
            {
                return Err(fail(pc, "jump into comparison materialization"));
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
    fn verifier_rejects_invalid_comparison_materialization_and_hook_maps() {
        let good = crate::compile(b"local x=1; if x<3 then x=x+1 end;return x==2").unwrap();
        for corrupt in 0..4 {
            let mut p = good.clone();
            let data = Arc::make_mut(&mut p.data);
            let proto = &mut data.prototypes[data.entry as usize];
            let boolean = proto.code.iter().position(|i| i.op == Op::Bool).unwrap();
            match corrupt {
                0 => proto.code[boolean].b = proto.registers,
                1 => proto.code[boolean].c = 2,
                2 => {
                    let jump = proto
                        .code
                        .iter_mut()
                        .find(|i| matches!(i.op, Op::Jump | Op::JumpFalse | Op::JumpTrue))
                        .unwrap();
                    jump.c = boolean as u32;
                }
                _ => proto.debug.hook_lines = Box::new([]),
            }
            assert_eq!(p.verify().unwrap_err().kind(), ErrorKind::Bytecode);
        }
    }

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

    #[test]
    fn verifier_rejects_broken_boolean_branch_and_skip_boundaries() {
        let good = crate::compile(b"local a,b,c=2,3,false;return not((a<b)or c)").unwrap();
        for corrupt in 0..7 {
            let mut p = good.clone();
            let data = Arc::make_mut(&mut p.data);
            let proto = &mut data.prototypes[data.entry as usize];
            let branch = proto
                .code
                .iter()
                .position(|i| matches!(i.op, Op::JumpFalse | Op::JumpTrue) && i.b == 1)
                .unwrap();
            let skip = proto
                .code
                .iter()
                .position(|i| i.op == Op::Jump && i.b == 2)
                .unwrap();
            let jump = proto
                .code
                .iter()
                .position(|i| i.op == Op::Jump && i.b == 0)
                .unwrap();
            match corrupt {
                0 => proto.code[branch].b = 2,
                1 => proto.code[branch].a = (proto.code[branch].a + 1) % proto.registers as u8,
                2 => proto.code[skip].b = 1,
                3 => proto.code[skip].c -= 1,
                4 => proto.code[skip - 1].c = 0,
                5 => proto.code[jump].c = branch as u32,
                _ => proto.code[jump].c = skip as u32,
            }
            assert_eq!(p.verify().unwrap_err().kind(), ErrorKind::Bytecode);
        }
    }

    #[test]
    fn verifier_bounds_constant_operands_and_source_debug_windows() {
        let good = crate::compile(b"local a=2;local t={key=a};return a+3,3-a,t.key").unwrap();
        let proto = &good.data.prototypes[good.data.entry as usize];
        assert!(proto.code.iter().any(|i| i.op.rk_c() && i.c >= RK));
        assert!(
            proto
                .code
                .iter()
                .any(|i| i.op.rk_b() && u32::from(i.b) >= RK)
        );
        for corrupt in 0..4 {
            let mut bad = good.clone();
            let data = Arc::make_mut(&mut bad.data);
            let p = &mut data.prototypes[data.entry as usize];
            let count = p.constants.len() as u32;
            match corrupt {
                0 => p.code.iter_mut().find(|i| i.op.rk_b()).unwrap().b = (RK + count) as u16,
                1 => p.code.iter_mut().find(|i| i.op.rk_c()).unwrap().c = RK + count,
                2 => p.code.iter_mut().find(|i| i.op == Op::GetTable).unwrap().b = RK as u16,
                _ => p.debug.stack = p.registers + 1,
            }
            assert_eq!(bad.verify().unwrap_err().kind(), ErrorKind::Bytecode);
        }
    }

    #[test]
    fn verifier_rejects_corrupt_collector_metadata() {
        let good = crate::compile(b"local t={};return t").unwrap();
        for corrupt in 0..3 {
            let mut p = good.clone();
            let data = Arc::make_mut(&mut p.data);
            let prototype = &mut data.prototypes[data.entry as usize];
            match corrupt {
                0 => prototype.gc_live = Box::new([]),
                1 => prototype.gc_live[0] = u32::MAX,
                _ => prototype.gc_maps[0][3] = 1u64 << 63,
            }
            assert_eq!(p.verify().unwrap_err().kind(), ErrorKind::Bytecode);
        }
    }

    #[test]
    fn verifier_rejects_corrupt_debug_ranges_names_and_call_sites() {
        let good =
            crate::compile(b"local x=1;local function f()return x end;f();return f()").unwrap();
        for corrupt in 0..10 {
            let mut p = good.clone();
            let data = Arc::make_mut(&mut p.data);
            let proto = &mut data.prototypes[data.entry as usize];
            match corrupt {
                0 => proto.debug.locals[0].start = u32::MAX,
                1 => proto.debug.locals[0].end = u32::MAX,
                2 => proto.debug.locals[0].register = proto.registers,
                3 => proto.debug.calls[0].pc = u32::MAX,
                4 => proto.debug.calls[0].pc = 0,
                5 => proto.debug.calls[1].pc = proto.debug.calls[0].pc,
                6 => proto.debug.active_lines = Box::new([3, 2]),
                7 => {
                    data.prototypes
                        .iter_mut()
                        .find(|p| !p.upvalues.is_empty())
                        .unwrap()
                        .debug
                        .upvalues = Box::new([])
                }
                8 => proto.debug.calls[0].temporaries = 0..u32::MAX,
                _ => {
                    proto.debug.temporaries =
                        Box::new([proto.code[proto.debug.calls[0].pc as usize].a]);
                    proto.debug.calls[0].temporaries = 0..1;
                }
            }
            assert_eq!(
                p.verify()
                    .expect_err(&format!("corrupt debug case {corrupt}"))
                    .kind(),
                ErrorKind::Bytecode
            );
        }
    }
}
