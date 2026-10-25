//! Lower the shared register ISA to native Lua words; imports retain their words.
use super::fail;
use crate::{
    Error,
    bytecode::{OPEN, OPEN_ARGS, Op, ProgramData, Prototype, Upvalue},
};
use std::collections::HashMap;

pub(super) struct Lowered {
    pub code: Vec<u32>,
    pub lines: Vec<u32>,
    pub pcs: Vec<u32>,
    pub children: Vec<usize>,
    pub stack: u8,
}
struct Builder {
    code: Vec<u32>,
    lines: Vec<u32>,
    patches: Vec<(usize, usize)>,
    line: u32,
    max: u16,
}
impl Builder {
    fn abc(&mut self, op: u32, a: u16, b: u16, c: u32) -> Result<(), Error> {
        if a >= 250 || b > 511 || c > 511 {
            return Err(fail());
        }
        self.code
            .push(op | (u32::from(a) << 6) | (u32::from(b) << 23) | (c << 14));
        self.lines.push(self.line);
        Ok(())
    }
    fn abx(&mut self, op: u32, a: u16, bx: u32) -> Result<(), Error> {
        if a >= 250 || bx > 262143 {
            return Err(fail());
        }
        self.code.push(op | (u32::from(a) << 6) | (bx << 14));
        self.lines.push(self.line);
        Ok(())
    }
    fn jump(&mut self, op: u32, a: u16, target: usize) -> Result<(), Error> {
        self.patches.push((self.code.len(), target));
        self.abx(op, a, 131071)
    }
    fn reg(&mut self, r: u16) -> Result<u16, Error> {
        if r >= 250 {
            return Err(fail());
        }
        self.max = self.max.max(r + 1);
        Ok(r)
    }
}

struct Gap {
    start: usize,
    end: usize,
    register: u16,
    count: u16,
}

pub(super) fn lower(data: &ProgramData, p: &Prototype) -> Result<Lowered, Error> {
    let n = p.code.len();
    let mut skip = vec![false; n];
    let mut methods = vec![None; n];
    let mut gaps = Vec::new();
    for pc in 2..n {
        let get = p.code[pc];
        let key = p.code[pc - 1];
        let receiver = p.code[pc - 2];
        if get.op == Op::GetTable
            && key.op == Op::Load
            && receiver.op == Op::Move
            && get.b == u16::from(get.a) + 1
            && u16::from(receiver.a) == get.b
            && receiver.b == u16::from(get.a)
            && u32::from(key.a) == get.c
        {
            skip[pc - 2] = true;
            if key.c < 256 {
                skip[pc - 1] = true;
            }
            methods[pc] = Some(if key.c < 256 { key.c | 256 } else { get.c });
        }
    }
    // Recognize the compiler's iterator-call sequence. Native TFORLOOP owns
    // its call scratch slots and implicit nil test, so no helper locals leak.
    for (pc, i) in p.code.iter().enumerate() {
        if i.op != Op::CallIter {
            continue;
        }
        if pc < 3 || pc + 4 >= n || i.b != 2 || i.c == 0 || i.c == OPEN {
            return Err(fail());
        }
        let a = u16::from(i.a);
        let c = i.c as u16;
        for offset in 0..3 {
            let mv = p.code[pc - 3 + offset];
            if mv.op != Op::Move
                || u16::from(mv.a) != a + offset as u16
                || mv.b + 3 != u16::from(mv.a)
            {
                return Err(fail());
            }
            skip[pc - 3 + offset] = true;
        }
        if p.code[pc + 1].op != Op::Load
            || p.code[pc + 2].op != Op::Eq
            || p.code[pc + 3].op != Op::JumpTrue
            || p.code[pc + 4].op != Op::Move
        {
            return Err(fail());
        }
        skip[pc + 1..pc + 5].fill(true);
        gaps.push(Gap {
            start: pc + 5,
            end: p.code[pc + 3].c as usize,
            register: a + c,
            count: c.max(3) + 1 - c,
        });
    }
    let mut out = Builder {
        code: Vec::with_capacity(n),
        lines: Vec::with_capacity(n),
        patches: Vec::new(),
        line: 0,
        max: p.params.max(u16::from(p.compat_arg) + p.params).max(2),
    };
    let mut pcs = vec![0; n + 1];
    let mut children = Vec::new();
    let mut child_slots = HashMap::new();
    let mut scopes: Vec<&Gap> = Vec::new();
    let mut next_gap = 0;
    let mut next_local = 0;
    for (pc, i) in p.code.iter().enumerate() {
        scopes.retain(|g| pc < g.end);
        while next_gap < gaps.len() && gaps[next_gap].start <= pc {
            scopes.push(&gaps[next_gap]);
            next_gap += 1;
        }
        let mapped = |r: u16| {
            r - scopes
                .iter()
                .filter(|g| r >= g.register)
                .map(|g| g.count)
                .sum::<u16>()
        };
        while next_local < p.debug.locals.len() && p.debug.locals[next_local].start as usize <= pc {
            let local = &p.debug.locals[next_local];
            if pc < local.end as usize {
                out.reg(mapped(local.register))?;
            }
            next_local += 1;
        }
        pcs[pc] = out.code.len() as u32;
        if skip[pc] {
            continue;
        }
        out.line = p.lines[pc];
        let a = mapped(u16::from(i.a));
        let b = mapped(i.b.min(255));
        let c = mapped(i.c.min(255) as u16);
        if let Some(key) = methods[pc] {
            out.reg(a + 1)?;
            let key = if key & 256 != 0 { key } else { u32::from(c) };
            out.abc(11, a, a, key)?;
            continue;
        }
        match i.op {
            Op::Load => {
                if i.b == 0 {
                    out.reg(a)?;
                    out.abx(1, a, i.c)?;
                }
            }
            Op::Move | Op::Bool => {
                if i.op == Op::Bool {
                    continue;
                }
                if i.c == OPEN {
                    return Err(fail());
                }
                out.reg(a)?;
                out.reg(b)?;
                out.abc(0, a, b, 0)?;
            }
            Op::GetGlobal | Op::SetGlobal => {
                out.reg(a)?;
                out.abx(if i.op == Op::GetGlobal { 5 } else { 7 }, a, i.c)?;
            }
            Op::GetUpvalue | Op::SetUpvalue => {
                out.reg(a)?;
                out.abc(if i.op == Op::GetUpvalue { 4 } else { 8 }, a, i.c as u16, 0)?;
            }
            Op::NewTable => {
                out.reg(a)?;
                out.abc(10, a, 0, 0)?;
            }
            Op::GetTable
            | Op::SetTable
            | Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Mod
            | Op::Pow => {
                out.reg(a)?;
                out.reg(b)?;
                out.reg(c)?;
                let op = match i.op {
                    Op::GetTable => 6,
                    Op::SetTable => 9,
                    Op::Add => 12,
                    Op::Sub => 13,
                    Op::Mul => 14,
                    Op::Div => 15,
                    Op::Mod => 16,
                    Op::Pow => 17,
                    _ => unreachable!(),
                };
                out.abc(op, a, b, u32::from(c))?;
            }
            Op::Neg | Op::Not | Op::Len => {
                out.reg(a)?;
                out.reg(b)?;
                out.abc(
                    match i.op {
                        Op::Neg => 18,
                        Op::Not => 19,
                        _ => 20,
                    },
                    a,
                    b,
                    0,
                )?;
            }
            Op::Concat | Op::ConcatRange => {
                if b >= c || (i.op == Op::Concat && c != b + 1) {
                    return Err(fail());
                }
                out.reg(a)?;
                out.reg(c)?;
                out.abc(21, a, b, u32::from(c))?;
            }
            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                let dest = if let Some(next) = p.code.get(pc + 1).filter(|n| n.op == Op::Bool) {
                    mapped(u16::from(next.a))
                } else {
                    a
                };
                out.reg(dest)?;
                out.reg(b)?;
                out.reg(c)?;
                let (op, left, right) = match i.op {
                    Op::Eq | Op::Ne => (23, b, c),
                    Op::Lt => (24, b, c),
                    Op::Le => (25, b, c),
                    Op::Gt => (24, c, b),
                    _ => (25, c, b),
                };
                out.abc(op, u16::from(i.op != Op::Ne), left, u32::from(right))?;
                out.abx(22, 0, 131072)?;
                out.abc(2, dest, 0, 1)?;
                out.abc(2, dest, 1, 0)?;
            }
            Op::Jump => out.jump(22, 0, i.c as usize)?,
            Op::JumpFalse | Op::JumpTrue => {
                out.reg(a)?;
                out.abc(26, a, 0, u32::from(i.op == Op::JumpTrue))?;
                out.jump(22, 0, i.c as usize)?;
            }
            Op::ForPrep | Op::ForLoop => {
                out.reg(a + 3)?;
                out.jump(if i.op == Op::ForPrep { 32 } else { 31 }, a, i.c as usize)?;
            }
            Op::Call | Op::TailCall => {
                out.reg(a)?;
                if i.b != OPEN_ARGS {
                    out.reg(a + i.b)?;
                }
                if i.op == Op::Call && i.c != OPEN && i.c != 0 {
                    out.reg(a + i.c as u16 - 1)?;
                }
                out.abc(
                    if i.op == Op::Call { 28 } else { 29 },
                    a,
                    if i.b == OPEN_ARGS { 0 } else { i.b + 1 },
                    if i.op == Op::TailCall || i.c == OPEN {
                        0
                    } else {
                        i.c + 1
                    },
                )?;
            }
            Op::CallIter => {
                out.reg(a + i.c as u16 - 1)?;
                out.abc(33, a - 3, 0, i.c)?;
                out.jump(22, 0, pc + 5)?;
                out.jump(22, 0, p.code[pc + 3].c as usize)?;
            }
            Op::Return | Op::Vararg => {
                out.reg(a)?;
                if i.c != OPEN && i.c != 0 {
                    out.reg(a + i.c as u16 - 1)?;
                }
                out.abc(
                    if i.op == Op::Return { 30 } else { 37 },
                    a,
                    if i.c == OPEN { 0 } else { i.c as u16 + 1 },
                    0,
                )?;
            }
            Op::SetList => {
                out.reg(a)?;
                if i.b != OPEN_ARGS {
                    out.reg(a + i.b)?;
                }
                if (i.c - 1) % 50 != 0 {
                    return Err(fail());
                }
                let block = (i.c - 1) / 50 + 1;
                out.abc(
                    34,
                    a,
                    if i.b == OPEN_ARGS { 0 } else { i.b },
                    if block < 512 { block } else { 0 },
                )?;
                if block >= 512 {
                    out.code.push(block);
                    out.lines.push(out.line);
                }
            }
            Op::Close => {
                out.reg(a)?;
                out.abc(35, a, 0, 0)?;
            }
            Op::Closure => {
                if i.b != 0 {
                    return Err(fail());
                }
                let child = i.c as usize;
                let index = *child_slots.entry(child).or_insert_with(|| {
                    children.push(child);
                    children.len() - 1
                });
                out.reg(a)?;
                out.abx(36, a, index as u32)?;
                for up in &data.prototypes[child].upvalues {
                    match *up {
                        Upvalue::Local(r) => {
                            let r = mapped(u16::from(r));
                            out.reg(r)?;
                            out.abc(0, 0, r, 0)?;
                        }
                        Upvalue::Outer(u) => out.abc(4, 0, u, 0)?,
                    }
                }
            }
        }
    }
    pcs[n] = out.code.len() as u32;
    for (pc, target) in out.patches {
        let target = *pcs.get(target).ok_or_else(fail)?;
        let offset = i64::from(target) - pc as i64 - 1;
        if !(-131071..=131071).contains(&offset) {
            return Err(fail());
        }
        out.code[pc] = (out.code[pc] & 0x3fff) | (((offset + 131071) as u32) << 14);
    }
    // Preserve the fixed parameter/compatibility window even if never read.
    if out.max > 250 {
        return Err(fail());
    }
    Ok(Lowered {
        code: out.code,
        lines: out.lines,
        pcs,
        children,
        stack: out.max as u8,
    })
}
