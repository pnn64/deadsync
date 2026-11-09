//! Translate verified native words once, then use the shared register VM.
use super::{Chunk, Prototype as Native, invalid};
use crate::bytecode::{
    ClosureBinding, Constant, DebugCall, DebugInfo, DebugLocal, Instruction, NameKind, NativeDebug,
    OPEN, OPEN_ARGS, Op, Program, Prototype, Upvalue,
};
use crate::memory::fallible::Map as HashMap;
use crate::{
    Error, ErrorKind,
    memory::fallible::{self, SliceExt, VecExt},
};

fn fields(word: u32) -> (usize, u8, u16, u16) {
    (
        (word & 63) as usize,
        (word >> 6) as u8,
        (word >> 23) as u16,
        ((word >> 14) & 511) as u16,
    )
}
fn target(pc: usize, word: u32) -> usize {
    (pc as i64 + 1 + i64::from(word >> 14) - 131071) as usize
}

pub(super) fn program(mut chunk: Chunk) -> Result<Program, Error> {
    let name = chunk.prototypes[0].source.take();
    let name = match name {
        Some(name) => name,
        None => b"=?".try_boxed()?,
    };
    let mut sources = HashMap::new();
    sources.try_reserve(1).map_err(|_| Error::memory())?;
    sources.insert(name.try_boxed()?, 0u32);
    let mut source_ids = fallible::repeat(0, chunk.prototypes.len())?;
    let mut counts = fallible::with_capacity(chunk.prototypes.len())?;
    counts.try_extend(chunk.prototypes.iter().map(|p| p.nups))?;
    let mut prototypes = fallible::with_capacity(chunk.prototypes.len())?;
    for (id, p) in chunk.prototypes.into_iter().enumerate() {
        if let Some(source) = p.source.as_ref() {
            let next = sources.len() as u32;
            source_ids[id] = if let Some(&existing) = sources.get(source.as_ref()) {
                existing
            } else {
                sources.try_reserve(1).map_err(|_| Error::memory())?;
                sources.insert(source.try_boxed()?, next);
                next
            };
        }
        for &child in &p.children {
            source_ids[child] = source_ids[id];
        }
        prototypes.try_push(translate(p, &counts, source_ids[id])?)?;
    }
    let mut pool = fallible::with_capacity(sources.len() - 1)?;
    pool.resize_with(sources.len() - 1, Box::<[u8]>::default);
    for (source, id) in sources {
        if id != 0 {
            pool[id as usize - 1] = source;
        }
    }
    let program = Program::try_from_data(crate::bytecode::ProgramData {
        prototypes: prototypes.try_into_boxed_slice()?,
        entry: 0,
        name,
        sources: pool.try_into_boxed_slice()?,
    })?;
    program
        .verify()
        .map_err(|_| Error::new_static(ErrorKind::Syntax, 0, "bad code"))?;
    Ok(program)
}

struct Builder {
    code: Vec<Instruction>,
    lines: Vec<u32>,
    hooks: Vec<u32>,
    patches: Vec<(usize, usize)>,
    constants: Vec<Constant>,
    line: u32,
}
impl Builder {
    fn emit(&mut self, op: Op, a: u8, b: u16, c: u32) -> Result<usize, Error> {
        let pc = self.code.len();
        self.code.try_push(Instruction { op, a, b, c })?;
        self.lines.try_push(self.line)?;
        self.hooks.try_push(0)?;
        Ok(pc)
    }
    fn jump(&mut self, op: Op, a: u8, target: usize) -> Result<(), Error> {
        let pc = self.emit(op, a, 0, 0)?;
        self.patches.try_push((pc, target))?;
        Ok(())
    }
    fn constant(&mut self, value: Constant) -> Result<u32, Error> {
        let index = self.constants.len() as u32;
        self.constants.try_push(value)?;
        Ok(index)
    }
}

fn translate(mut p: Native, counts: &[u8], source: u32) -> Result<Prototype, Error> {
    let n = p.code.len();
    let scratch = p.stack;
    let registers = u16::from(p.stack) + 2;
    let constant_count = p.constants.len();
    let name_count = p.names.len();
    let mut local_bounds = fallible::with_capacity(p.locals.len())?;
    local_bounds.try_extend(p.locals.iter().map(|l| [l.start, l.end]))?;
    let local_bounds = local_bounds.try_into_boxed_slice()?;
    let mut local_registers = fallible::with_capacity(p.locals.len())?;
    let mut slots: Vec<Vec<usize>> = Vec::new();
    let mut ends = Vec::new();
    let mut previous = 0;
    for (index, local) in p.locals.iter().enumerate() {
        if local.start < previous || local.start > local.end || local.end as usize > n {
            return Err(invalid("bad code"));
        }
        previous = local.start;
        ends.retain(|&end| end > local.start);
        let register = ends.len();
        if register >= usize::from(p.stack) {
            return Err(invalid("bad code"));
        }
        ends.try_push(local.end)?;
        let additional = (register + 1).saturating_sub(slots.len());
        fallible::reserve(&mut slots, additional)?;
        slots.resize_with(slots.len().max(register + 1), Vec::new);
        slots[register].try_push(index)?;
        local_registers.try_push(register as u16)?;
    }
    let mut out = Builder {
        code: fallible::with_capacity(n)?,
        lines: fallible::with_capacity(n)?,
        hooks: fallible::with_capacity(n)?,
        patches: Vec::new(),
        constants: std::mem::take(&mut p.constants),
        line: 0,
    };
    let mut map = fallible::repeat(0u32, n + 1)?;
    let mut bindings = Vec::new();
    let mut calls = Vec::new();
    let mut temporaries = Vec::new();
    let mut payload = false;
    for (pc, mapped) in map[..n].iter_mut().enumerate() {
        let first = out.code.len();
        *mapped = first as u32;
        if std::mem::take(&mut payload) {
            continue;
        }
        out.line = p.lines.get(pc).copied().unwrap_or(0);
        let word = p.code[pc];
        let (op, a, b, c) = fields(word);
        match op {
            0 => {
                out.emit(Op::Move, a, b, 0)?;
            }
            1 => {
                out.emit(Op::Load, a, 0, word >> 14)?;
            }
            2 => {
                let k = out.constant(Constant::Boolean(b != 0))?;
                out.emit(Op::Load, a, 0, k)?;
                if c != 0 {
                    out.jump(Op::Jump, 0, pc + 2)?;
                }
            }
            3 => {
                let k = out.constant(Constant::Nil)?;
                for r in u16::from(a)..=b {
                    out.emit(Op::Load, r as u8, 0, k)?;
                }
            }
            4 | 8 => {
                out.emit(
                    if op == 4 {
                        Op::GetUpvalue
                    } else {
                        Op::SetUpvalue
                    },
                    a,
                    0,
                    u32::from(b),
                )?;
            }
            5 | 7 => {
                out.emit(
                    if op == 5 {
                        Op::GetGlobal
                    } else {
                        Op::SetGlobal
                    },
                    a,
                    0,
                    word >> 14,
                )?;
            }
            6 => {
                let key = c;
                out.emit(Op::GetTable, a, b, u32::from(key))?;
            }
            9 => {
                let key = b;
                let value = c;
                out.emit(Op::SetTable, a, key, u32::from(value))?;
            }
            10 => {
                out.emit(Op::NewTable, a, b, u32::from(c))?;
            }
            11 => {
                out.emit(Op::Move, a + 1, b, 0)?;
                let key = c;
                out.emit(Op::GetTable, a, b, u32::from(key))?;
            }
            12..=17 => {
                let left = b;
                let right = c;
                out.emit(
                    [Op::Add, Op::Sub, Op::Mul, Op::Div, Op::Mod, Op::Pow][op - 12],
                    a,
                    left,
                    u32::from(right),
                )?;
            }
            18..=20 => {
                out.emit([Op::Neg, Op::Not, Op::Len][op - 18], a, b, 0)?;
            }
            21 => {
                out.emit(Op::ConcatRange, a, b, u32::from(c))?;
            }
            22 => {
                out.jump(Op::Jump, 0, target(pc, word))?;
            }
            23..=25 => {
                let left = b;
                let right = c;
                out.emit(
                    [Op::Eq, Op::Lt, Op::Le][op - 23],
                    scratch,
                    left,
                    u32::from(right),
                )?;
                out.jump(
                    if a == 0 { Op::JumpFalse } else { Op::JumpTrue },
                    scratch,
                    target(pc + 1, p.code[pc + 1]),
                )?;
                out.jump(Op::Jump, 0, pc + 2)?;
            }
            26 => {
                out.jump(
                    if c == 0 { Op::JumpFalse } else { Op::JumpTrue },
                    a,
                    target(pc + 1, p.code[pc + 1]),
                )?;
                out.jump(Op::Jump, 0, pc + 2)?;
            }
            27 => {
                // A matching test copies B before taking its embedded jump.
                let skip = out.emit(
                    if c == 0 { Op::JumpTrue } else { Op::JumpFalse },
                    b as u8,
                    0,
                    0,
                )?;
                out.emit(Op::Move, a, b, 0)?;
                out.jump(Op::Jump, 0, target(pc + 1, p.code[pc + 1]))?;
                out.code[skip].c = out.code.len() as u32;
                out.jump(Op::Jump, 0, pc + 2)?;
            }
            28 | 29 => {
                let call = out.emit(
                    if op == 28 { Op::Call } else { Op::TailCall },
                    a,
                    if b == 0 { OPEN_ARGS } else { b - 1 },
                    if c == 0 { OPEN } else { u32::from(c - 1) },
                )?;
                call_info(
                    &p,
                    &slots,
                    &out.constants,
                    pc,
                    call,
                    a,
                    (&mut calls, &mut temporaries),
                )?;
            }
            30 => {
                out.emit(
                    Op::Return,
                    a,
                    0,
                    if b == 0 { OPEN } else { u32::from(b - 1) },
                )?;
            }
            31 | 32 => {
                out.jump(
                    if op == 31 { Op::ForLoop } else { Op::ForPrep },
                    a,
                    target(pc, word),
                )?;
            }
            33 => {
                for offset in 0..3 {
                    out.emit(Op::Move, a + 3 + offset, u16::from(a + offset), 0)?;
                }
                let call = out.emit(Op::CallIter, a + 3, 2, u32::from(c))?;
                call_info(
                    &p,
                    &slots,
                    &out.constants,
                    pc,
                    call,
                    a + 3,
                    (&mut calls, &mut temporaries),
                )?;
                let k = out.constant(Constant::Nil)?;
                out.emit(Op::Load, scratch + 1, 0, k)?;
                out.emit(Op::Eq, scratch, u16::from(a + 3), u32::from(scratch + 1))?;
                out.jump(Op::JumpTrue, scratch, pc + 2)?;
                out.emit(Op::Move, a + 2, u16::from(a + 3), 0)?;
                out.jump(Op::Jump, 0, target(pc + 1, p.code[pc + 1]))?;
            }
            34 => {
                let block = if c == 0 {
                    payload = true;
                    p.code[pc + 1]
                } else {
                    u32::from(c)
                };
                let start = block
                    .checked_sub(1)
                    .and_then(|b| b.checked_mul(50))
                    .and_then(|b| b.checked_add(1))
                    .ok_or_else(|| invalid("bad code"))?;
                out.emit(Op::SetList, a, if b == 0 { OPEN_ARGS } else { b }, start)?;
            }
            35 => {
                out.emit(Op::Close, a, 0, 0)?;
            }
            36 => {
                let child = p.children[(word >> 14) as usize];
                let nups = usize::from(counts[child]);
                let captures_iter = p.code[pc + 1..pc + 1 + nups].iter().map(|&w| {
                    let (op, _, b, _) = fields(w);
                    if op == 0 {
                        Upvalue::Local(b as u8)
                    } else {
                        Upvalue::Outer(b)
                    }
                });
                let mut captures = fallible::with_capacity(nups)?;
                captures.try_extend(captures_iter)?;
                let captures = captures.try_into_boxed_slice()?;
                let binding = bindings.len() as u32;
                bindings.try_push(ClosureBinding {
                    prototype: child as u32,
                    upvalues: captures,
                })?;
                out.emit(Op::Closure, a, 1, binding)?;
                if nups != 0 {
                    out.jump(Op::Jump, 0, pc + 1 + nups)?;
                }
            }
            37 => {
                out.emit(
                    Op::Vararg,
                    a,
                    0,
                    if b == 0 { OPEN } else { u32::from(b - 1) },
                )?;
            }
            _ => return Err(invalid("bad code")),
        }
        // Native dispatch counts once, even when an instruction consumes a
        // following jump, binding words or an extended SETLIST count.
        out.hooks[first] = if out.line == 0 { u32::MAX } else { out.line };
    }
    map[n] = out.code.len() as u32;
    for (pc, native) in out.patches {
        out.code[pc].c = *map.get(native).ok_or_else(|| invalid("bad code"))?;
    }
    let mut locals: Vec<DebugLocal> = fallible::with_capacity(p.locals.len())?;
    for (local, register) in p.locals.into_iter().zip(local_registers) {
        let start = map[local.start as usize];
        locals.try_push(DebugLocal {
            name: local.name.unwrap_or_default(),
            start,
            end: map[local.end as usize],
            register,
        })?;
    }
    let mut names = fallible::with_capacity(usize::from(p.nups))?;
    names.try_extend(p.names.into_iter().map(Option::unwrap_or_default))?;
    while names.len() < usize::from(p.nups) {
        names.try_push(b"?".try_boxed()?)?;
    }
    let mut active_lines = p.lines.try_to_vec()?;
    active_lines.retain(|&l| l != 0);
    active_lines.sort_unstable();
    active_lines.dedup();
    let mut mask = [0u64; 4];
    for r in 0..usize::from(p.stack) {
        mask[r / 64] |= 1u64 << (r % 64);
    }
    let mut masks = [mask; 4];
    for (bits, mask) in masks.iter_mut().enumerate() {
        for offset in 0..2 {
            if bits & (1 << offset) != 0 {
                let r = usize::from(p.stack) + offset;
                mask[r / 64] |= 1u64 << (r % 64);
            }
        }
    }
    let mut gc_live = fallible::repeat(0u32, out.code.len())?;
    let mut masks = masks.try_to_vec()?;
    let mut call_masks = [None; 256];
    for range in map.windows(2) {
        let mut live = 0;
        for pc in (range[0] as usize..range[1] as usize).rev() {
            let after = live;
            let (read, write) = scratch_usage(out.code[pc], scratch);
            live = (live & !write) | read;
            let instruction = out.code[pc];
            let before = if matches!(instruction.op, Op::Call | Op::CallIter | Op::TailCall) {
                // Lua callees occupy the argument slots in the native stack.
                // Our separate window roots those values through the callee;
                // retaining caller copies would keep overwritten values alive.
                if let Some(index) = call_masks[usize::from(instruction.a)] {
                    index
                } else {
                    let mut mask = [0u64; 4];
                    for r in 0..=usize::from(instruction.a) {
                        mask[r / 64] |= 1u64 << (r % 64);
                    }
                    let index = masks.len() as u16;
                    masks.try_push(mask)?;
                    call_masks[usize::from(instruction.a)] = Some(index);
                    index
                }
            } else {
                u16::from(live)
            };
            gc_live[pc] = u32::from(before) | (u32::from(after) << 16);
        }
    }
    Ok(Prototype {
        gc_live: gc_live.try_into_boxed_slice()?,
        gc_maps: masks.try_into_boxed_slice()?,
        code: out.code.try_into_boxed_slice()?,
        lines: out.lines.try_into_boxed_slice()?,
        constants: out.constants.try_into_boxed_slice()?,
        registers,
        params: u16::from(p.params),
        vararg: p.vararg & 2 != 0,
        compat_arg: p.vararg & 4 != 0,
        upvalues: fallible::repeat(Upvalue::Outer(0), usize::from(p.nups))?
            .try_into_boxed_slice()?,
        bindings: bindings.try_into_boxed_slice()?,
        debug: DebugInfo {
            stack: 0,
            native: Some(fallible::boxed(NativeDebug {
                source,
                stack: u16::from(p.stack),
                has_lines: !p.lines.is_empty(),
                wire: super::Wire {
                    code: p.code.try_into_boxed_slice()?,
                    lines: p.lines.try_into_boxed_slice()?,
                    locals: local_bounds,
                    children: p.children.try_into_boxed_slice()?,
                    constants: constant_count,
                    names: name_count,
                    vararg: p.vararg,
                },
            })?),
            defined: p.defined,
            upvalues: names.try_into_boxed_slice()?,
            locals: locals.try_into_boxed_slice()?,
            calls: calls.try_into_boxed_slice()?,
            active_lines: active_lines.try_into_boxed_slice()?,
            hook_lines: out.hooks.try_into_boxed_slice()?,
            temporaries: temporaries.try_into_boxed_slice()?,
        },
    })
}

fn scratch_usage(i: Instruction, scratch: u8) -> (u8, u8) {
    let bit = |r: u32| {
        if r >= u32::from(scratch) && r < u32::from(scratch) + 2 {
            1u8 << (r - u32::from(scratch))
        } else {
            0
        }
    };
    let a = bit(u32::from(i.a));
    let b = bit(u32::from(i.b));
    let c = bit(i.c);
    match i.op {
        Op::Load | Op::GetGlobal | Op::NewTable | Op::Closure | Op::GetUpvalue | Op::Vararg => {
            (0, a)
        }
        Op::Move | Op::Neg | Op::Not | Op::Len => (b, a),
        Op::GetTable
        | Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Mod
        | Op::Pow
        | Op::Eq
        | Op::Lt
        | Op::Le => (b | c, a),
        Op::SetTable => (a | b | c, 0),
        Op::JumpTrue | Op::JumpFalse | Op::SetGlobal | Op::SetUpvalue => (a, 0),
        Op::Call | Op::CallIter | Op::TailCall => {
            let read = if i.b == OPEN_ARGS {
                0
            } else {
                (u32::from(i.a)..=u32::from(i.a) + u32::from(i.b)).fold(0, |mask, r| mask | bit(r))
            };
            let write = if i.c == OPEN {
                3
            } else {
                (u32::from(i.a)..u32::from(i.a) + i.c).fold(0, |mask, r| mask | bit(r))
            };
            (read, write)
        }
        _ => (0, 0),
    }
}

fn call_info(
    p: &Native,
    slots: &[Vec<usize>],
    constants: &[Constant],
    pc: usize,
    call: usize,
    register: u8,
    output: (&mut Vec<DebugCall>, &mut Vec<u8>),
) -> Result<(), Error> {
    let (calls, temporaries) = output;
    let (name, kind) = if fields(p.code[pc]).0 == 33 {
        (b"(for generator)".as_slice(), NameKind::Local)
    } else {
        object_name(p, slots, constants, pc, register).unwrap_or((b"", NameKind::Unknown))
    };
    let start = temporaries.len() as u32;
    let named = slots
        .iter()
        .take_while(|slot| local_at(p, slot, pc).is_some())
        .count();
    temporaries.try_extend((named as u8..register).filter(|&r| r < p.stack))?;
    calls.try_push(DebugCall {
        pc: call as u32,
        name: name.try_boxed()?,
        kind,
        temporaries: start..temporaries.len() as u32,
    })?;
    Ok(())
}

fn object_name<'a>(
    p: &'a Native,
    slots: &[Vec<usize>],
    constants: &'a [Constant],
    mut pc: usize,
    mut register: u8,
) -> Option<(&'a [u8], NameKind)> {
    // Walk backwards through register moves without recursion. Native local
    // slots are assigned by their order in the active local-variable list.
    loop {
        if let Some(local) = slots
            .get(usize::from(register))
            .and_then(|slot| local_at(p, slot, pc))
        {
            return Some((local.name.as_deref().unwrap_or_default(), NameKind::Local));
        }
        let mut found = None;
        for index in (0..pc).rev() {
            let (op, a, b, c) = fields(p.code[index]);
            if op == 22 && target(index, p.code[index]) > pc {
                continue;
            }
            if a != register {
                continue;
            }
            found = Some((index, op, b, c));
            break;
        }
        let (index, op, b, c) = found?;
        let key = |k: usize| match constants.get(k) {
            Some(Constant::String(s)) => s.as_ref(),
            _ => b"?",
        };
        match op {
            0 if (b as u8) < register => {
                pc = index;
                register = b as u8;
            }
            5 => return Some((key((p.code[index] >> 14) as usize), NameKind::Global)),
            4 => {
                return Some((
                    p.names
                        .get(usize::from(b))
                        .and_then(Option::as_deref)
                        .unwrap_or(b"?"),
                    NameKind::Upvalue,
                ));
            }
            6 | 11 => {
                return Some((
                    if c & 256 != 0 {
                        key(usize::from(c & 255))
                    } else {
                        b"?"
                    },
                    if op == 11 {
                        NameKind::Method
                    } else {
                        NameKind::Field
                    },
                ));
            }
            _ => return None,
        }
    }
}

fn local_at<'a>(p: &'a Native, slot: &[usize], pc: usize) -> Option<&'a super::Local> {
    let index = slot
        .partition_point(|&i| p.locals[i].start as usize <= pc)
        .checked_sub(1)?;
    let local = &p.locals[slot[index]];
    (pc < local.end as usize).then_some(local)
}
