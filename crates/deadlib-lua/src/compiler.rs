//! Register allocation and Lua 5.1 bytecode emission.

use crate::{
    Error, ErrorKind,
    bytecode::{
        Constant, DebugCall, DebugInfo, DebugLocal, Instruction, NameKind, OPEN, OPEN_ARGS, Op,
        Program, Prototype, Upvalue,
    },
    lexer::Kind,
    parser::{Ast, Expr, ExprId, Stmt, StmtId},
};
use std::collections::HashMap;

mod expressions;
mod statements;
mod tasks;
use tasks::Task;

struct Local<'a> {
    name: &'a [u8],
    reg: u8,
    debug: usize,
}
const TAIL: u32 = OPEN - 1;
enum Target {
    Local(u8),
    Upvalue(u16),
    Global(u32),
    Index(u8, u8),
}

#[derive(Clone, Copy)]
enum LineUse {
    Operand,
    Condition,
    Copy,
}

#[derive(Default)]
struct Function<'a> {
    code: Vec<Instruction>,
    lines: Vec<u32>,
    gc_live: Vec<u32>,
    gc_maps: Vec<[u64; 4]>,
    gc_index: HashMap<[u64; 4], u16>,
    live: [u64; 4],
    debug_hidden: [u64; 4],
    debug_named: [u64; 4],
    constants: Vec<Constant>,
    locals: Vec<Local<'a>>,
    upvalues: Vec<(&'a [u8], Upvalue)>,
    loops: Vec<(u16, Vec<usize>)>,
    next: u16,
    max: u16,
    params: u16,
    vararg: bool,
    compat_arg: bool,
    defined: [u32; 2],
    debug_locals: Vec<DebugLocal>,
    debug_calls: Vec<DebugCall>,
    debug_temporaries: Vec<u8>,
    debug_lines: Vec<u32>,
    hook_lines: Vec<u32>,
    hook_visible: usize,
}

struct Compiler<'a> {
    ast: &'a Ast,
    function: Function<'a>,
    parents: Vec<Function<'a>>,
    prototypes: Vec<Prototype>,
    folded: Vec<Option<f64>>,
    tasks: Vec<Task<'a>>,
    result: u8,
    line: u32,
}

pub(crate) fn compile(ast: Ast, name: &[u8]) -> Result<Program, Error> {
    let mut c = Compiler {
        ast: &ast,
        function: Function {
            vararg: true,
            ..Function::default()
        },
        parents: Vec::new(),
        prototypes: Vec::new(),
        folded: crate::optimizer::literals(&ast),
        tasks: vec![Task::Block(&ast.body)],
        result: 0,
        line: 1,
    };
    c.run()?;
    c.line = ast.end_line;
    c.emit(Op::Return, 0, 0, 0)?;
    let entry = c.finish();
    let p = Program::new(c.prototypes, entry, name);
    p.verify()?;
    Ok(p)
}

impl<'a> Compiler<'a> {
    fn finish(&mut self) -> u32 {
        let mut f = std::mem::take(&mut self.function);
        for (pc, i) in f.code.iter().enumerate() {
            let consumed_branch = matches!(i.op, Op::JumpFalse | Op::JumpTrue)
                && f.code[..pc]
                    .iter()
                    .rev()
                    .find(|p| p.op != Op::Close)
                    .is_some_and(|previous| {
                        matches!(
                            previous.op,
                            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Bool
                        ) && previous.a == i.a
                    });
            let branch_value = i.op == Op::Bool
                && f.code[pc + 1..]
                    .iter()
                    .find(|next| next.op != Op::Close)
                    .is_some_and(|next| {
                        matches!(next.op, Op::JumpFalse | Op::JumpTrue) && next.a == i.a
                    });
            if consumed_branch || branch_value {
                f.hook_lines[pc] = 0;
            }
        }
        f.debug_lines.retain(|&l| l != 0);
        f.debug_lines.sort_unstable();
        f.debug_lines.dedup();
        for local in &mut f.debug_locals {
            if local.end == u32::MAX {
                local.end = f.code.len() as u32;
            }
        }
        let (names, upvalues): (Vec<_>, Vec<_>) =
            f.upvalues.into_iter().map(|(n, u)| (n.into(), u)).unzip();
        let id = self.prototypes.len() as u32;
        self.prototypes.push(Prototype {
            code: f.code.into_boxed_slice(),
            lines: f.lines.into_boxed_slice(),
            gc_live: f.gc_live.into_boxed_slice(),
            gc_maps: f.gc_maps.into_boxed_slice(),
            constants: f.constants.into_boxed_slice(),
            registers: f.max.max(2),
            params: f.params,
            vararg: f.vararg,
            compat_arg: f.compat_arg,
            upvalues: upvalues.into_boxed_slice(),
            debug: DebugInfo {
                defined: f.defined,
                upvalues: names.into_boxed_slice(),
                locals: f.debug_locals.into_boxed_slice(),
                calls: f.debug_calls.into_boxed_slice(),
                active_lines: f.debug_lines.into_boxed_slice(),
                hook_lines: f.hook_lines.into_boxed_slice(),
                temporaries: f.debug_temporaries.into_boxed_slice(),
            },
        });
        id
    }
    fn upvalue(&mut self, name: &'a [u8]) -> Result<Option<u16>, Error> {
        if let Some(index) = self.function.upvalues.iter().position(|(n, _)| *n == name) {
            return Ok(Some(index as u16));
        }
        let mut owner = None;
        for (index, parent) in self.parents.iter().enumerate().rev() {
            if let Some(local) = parent.locals.iter().rev().find(|l| l.name == name) {
                owner = Some((index, Upvalue::Local(local.reg)));
                break;
            }
            if let Some(up) = parent.upvalues.iter().position(|(n, _)| *n == name) {
                owner = Some((index, Upvalue::Outer(up as u16)));
                break;
            }
        }
        let Some((owner, mut up)) = owner else {
            return Ok(None);
        };
        for index in owner + 1..self.parents.len() {
            if self.parents[index].upvalues.len() >= 60 {
                return Err(self.error(ErrorKind::Limit, "too many upvalues"));
            }
            let parent = &mut self.parents[index];
            let slot = parent.upvalues.len() as u16;
            parent.upvalues.push((name, up));
            up = Upvalue::Outer(slot);
        }
        if self.function.upvalues.len() >= 60 {
            return Err(self.error(ErrorKind::Limit, "too many upvalues"));
        }
        let index = self.function.upvalues.len() as u16;
        self.function.upvalues.push((name, up));
        Ok(Some(index))
    }
    fn close(&mut self, base: u16) -> Result<(), Error> {
        if base < self.function.max {
            self.emit(Op::Close, base as u8, 0, 0)?;
        }
        Ok(())
    }
    fn error(&self, kind: ErrorKind, message: &str) -> Error {
        Error::new(kind, self.line, message)
    }
    fn alloc(&mut self, count: u16) -> Result<u8, Error> {
        let start = self.function.next;
        self.function.next = self
            .function
            .next
            .checked_add(count)
            .filter(|n| *n <= 256)
            .ok_or_else(|| self.error(ErrorKind::Limit, "function needs too many registers"))?;
        self.function.max = self.function.max.max(self.function.next);
        u8::try_from(start)
            .map_err(|_| self.error(ErrorKind::Limit, "function needs too many registers"))
    }
    fn reset(&mut self, next: u16) {
        self.function.next = next;
        let word = usize::from(next) / 64;
        let bit = next % 64;
        if word < 4 {
            self.function.live[word] &= (1u64 << bit) - 1;
            self.function.live[word + 1..].fill(0);
            self.function.debug_hidden[word] &= (1u64 << bit) - 1;
            self.function.debug_hidden[word + 1..].fill(0);
        }
    }
    fn live_register(&mut self, register: u16) {
        if register < 256 {
            self.function.live[usize::from(register) / 64] |= 1u64 << (register % 64);
            self.function.debug_hidden[usize::from(register) / 64] &= !(1u64 << (register % 64));
        }
    }
    fn live_map(&mut self) -> Result<u16, Error> {
        let mask = self.function.live;
        if let Some(&index) = self.function.gc_index.get(&mask) {
            return Ok(index);
        }
        let index = u16::try_from(self.function.gc_maps.len())
            .map_err(|_| self.error(ErrorKind::Limit, "too many GC register maps"))?;
        self.function.gc_maps.push(mask);
        self.function.gc_index.insert(mask, index);
        Ok(index)
    }
    fn emit(&mut self, op: Op, a: u8, b: u16, c: u32) -> Result<usize, Error> {
        let pc = self.function.code.len();
        if pc >= u32::MAX as usize {
            return Err(self.error(ErrorKind::Limit, "function has too many instructions"));
        }
        let before = self.live_map()?;
        match op {
            Op::Load
            | Op::Move
            | Op::Bool
            | Op::GetGlobal
            | Op::NewTable
            | Op::Closure
            | Op::GetUpvalue
            | Op::GetTable
            | Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Mod
            | Op::Pow
            | Op::Neg
            | Op::Not
            | Op::Len
            | Op::Concat
            | Op::Eq
            | Op::Ne
            | Op::Lt
            | Op::Le
            | Op::Gt
            | Op::Ge => self.live_register(u16::from(a)),
            Op::Call | Op::TailCall | Op::Vararg => {
                let saved = self.function.next;
                self.reset(u16::from(a));
                self.function.next = saved;
                let count = if c == OPEN || op == Op::TailCall {
                    1
                } else {
                    c as u16
                };
                for r in u16::from(a)..u16::from(a) + count {
                    self.live_register(r);
                }
            }
            Op::ForLoop => self.live_register(u16::from(a) + 3),
            _ => {}
        }
        let after = self.live_map()?;
        self.function.code.push(Instruction { op, a, b, c });
        self.function.hook_visible += usize::from(op != Op::Close && self.line != 0);
        self.function.lines.push(self.line);
        self.function
            .hook_lines
            .push(if op == Op::Close { 0 } else { self.line });
        self.function
            .debug_lines
            .push(if op == Op::Close { 0 } else { self.line });
        self.function
            .gc_live
            .push(u32::from(before) | (u32::from(after) << 16));
        if op == Op::Move
            && (c == OPEN
                || u16::from(a) == b
                || (self.function.debug_named[usize::from(a) / 64] & (1u64 << (a % 64)) == 0
                    && self.function.debug_named[usize::from(b) / 64] & (1u64 << (b % 64)) == 0))
        {
            self.debug_line(0);
        }
        Ok(pc)
    }
    fn patch(&mut self, pc: usize) {
        self.function.code[pc].c = self.function.code.len() as u32;
    }
    fn debug_line(&mut self, line: u32) {
        let previous = *self.function.hook_lines.last().expect("emitted hook line");
        self.function.hook_visible =
            self.function.hook_visible + usize::from(line != 0) - usize::from(previous != 0);
        *self
            .function
            .debug_lines
            .last_mut()
            .expect("emitted debug line") = line;
        *self
            .function
            .hook_lines
            .last_mut()
            .expect("emitted hook line") = line;
    }
    fn constant(&mut self, k: Constant) -> Result<u32, Error> {
        // PUC 5.1 pools numbers by numeric equality, including +0 and -0.
        if let Some(i) = self.function.constants.iter().position(|v| match (v, &k) {
            (Constant::Nil, Constant::Nil) => true,
            (Constant::Boolean(a), Constant::Boolean(b)) => a == b,
            (Constant::Number(a), Constant::Number(b)) => a == b,
            (Constant::String(a), Constant::String(b)) => a == b,
            _ => false,
        }) {
            return Ok(i as u32);
        }
        let i = u32::try_from(self.function.constants.len())
            .map_err(|_| self.error(ErrorKind::Limit, "too many constants"))?;
        self.function.constants.push(k);
        Ok(i)
    }
    fn load(&mut self, dst: u8, k: Constant) -> Result<(), Error> {
        let initial_nil = matches!(k, Constant::Nil)
            && self.function.hook_visible == 0
            && self.function.loops.is_empty()
            && self.function.debug_named[usize::from(dst) / 64] & (1u64 << (dst % 64)) == 0;
        let id = self.constant(k)?;
        self.emit(Op::Load, dst, u16::from(initial_nil), id)?;
        if initial_nil {
            self.debug_line(0);
        }
        Ok(())
    }
    fn local(&self, name: &[u8]) -> Option<u8> {
        self.function
            .locals
            .iter()
            .rev()
            .find(|l| l.name == name)
            .map(|l| l.reg)
    }
    fn bind(&mut self, name: &'a [u8], reg: u8) -> Result<(), Error> {
        if self.function.locals.len() >= 200 {
            return Err(self.error(ErrorKind::Limit, "too many local variables"));
        }
        let debug = self.debug_local(name, reg);
        self.function.locals.push(Local { name, reg, debug });
        self.live_register(u16::from(reg));
        Ok(())
    }
    fn debug_local(&mut self, name: &[u8], reg: u8) -> usize {
        self.function.debug_named[usize::from(reg) / 64] |= 1u64 << (reg % 64);
        let index = self.function.debug_locals.len();
        self.function.debug_locals.push(DebugLocal {
            name: name.into(),
            start: self.function.code.len() as u32,
            end: u32::MAX,
            register: u16::from(reg),
        });
        index
    }
    fn end_locals(&mut self, retained: usize) {
        let end = self.function.code.len() as u32;
        for local in &self.function.locals[retained..] {
            self.function.debug_locals[local.debug].end = end;
            self.function.debug_named[usize::from(local.reg) / 64] &= !(1u64 << (local.reg % 64));
        }
        self.function.locals.truncate(retained);
    }
    fn finish_loop(&mut self) {
        for pc in self
            .function
            .loops
            .pop()
            .expect("compiler loop stack must be balanced")
            .1
        {
            self.patch(pc);
        }
    }
    fn multiple(&self, id: ExprId) -> bool {
        matches!(
            self.ast.exprs[id].0,
            Expr::Call(..) | Expr::Method(..) | Expr::Vararg
        )
    }

    fn record_call(
        &mut self,
        mut function: ExprId,
        method: Option<&[u8]>,
        base: u8,
    ) -> Result<(), Error> {
        let (name, kind) = if let Some(name) = method {
            (name, NameKind::Method)
        } else {
            loop {
                match self.ast.exprs[function].0 {
                    Expr::Paren(inner) => function = inner,
                    Expr::Binary(Kind::Or, left, right, _)
                        if matches!(self.ast.exprs[left].0, Expr::Binary(Kind::And, ..)) =>
                    {
                        function = right
                    }
                    _ => break,
                }
            }
            match &self.ast.exprs[function].0 {
                Expr::Name(name) => {
                    let kind = if self.local(name).is_some() {
                        NameKind::Local
                    } else if self
                        .function
                        .upvalues
                        .iter()
                        .any(|(n, _)| *n == name.as_ref())
                    {
                        NameKind::Upvalue
                    } else {
                        NameKind::Global
                    };
                    (name.as_ref(), kind)
                }
                Expr::Index(_, key) => {
                    let name = if let Expr::String(name) = &self.ast.exprs[*key].0 {
                        name.as_ref()
                    } else {
                        b"?"
                    };
                    (name, NameKind::Field)
                }
                _ => (b"".as_slice(), NameKind::Unknown),
            }
        };
        let start = u32::try_from(self.function.debug_temporaries.len())
            .map_err(|_| self.error(ErrorKind::Limit, "too many debug temporaries"))?;
        for r in 0..base {
            if self.function.live[usize::from(r) / 64]
                & !self.function.debug_hidden[usize::from(r) / 64]
                & !self.function.debug_named[usize::from(r) / 64]
                & (1u64 << (r % 64))
                != 0
            {
                self.function.debug_temporaries.push(r);
            }
        }
        let end = u32::try_from(self.function.debug_temporaries.len())
            .map_err(|_| self.error(ErrorKind::Limit, "too many debug temporaries"))?;
        self.function.debug_calls.push(DebugCall {
            pc: self.function.code.len() as u32,
            name: name.into(),
            kind,
            temporaries: start..end,
        });
        Ok(())
    }

    fn discharge_line(&mut self, mut id: ExprId, line: u32, usage: LineUse) {
        loop {
            match &self.ast.exprs[id].0 {
                Expr::Paren(inner) | Expr::Binary(Kind::And | Kind::Or, _, inner, _) => id = *inner,
                Expr::Name(name) => {
                    if matches!(usage, LineUse::Condition) && self.local(name).is_some() {
                        self.debug_line(0);
                    } else if matches!(usage, LineUse::Copy) || self.local(name).is_none() {
                        self.debug_line(line);
                    }
                    return;
                }
                Expr::Nil | Expr::Boolean(_) | Expr::Number(_) | Expr::String(_) => {
                    if !matches!(usage, LineUse::Operand) {
                        self.debug_line(line);
                    }
                    return;
                }
                Expr::Index(..) => {
                    self.debug_line(line);
                    return;
                }
                _ => return,
            }
        }
    }
}
