//! Register allocation and Lua 5.1 bytecode emission.

use crate::{
    Error, ErrorKind,
    bytecode::{
        Constant, DebugCall, DebugInfo, DebugLocal, Instruction, NameKind, OPEN, OPEN_ARGS, Op,
        Program, Prototype, Upvalue,
    },
    lexer::Kind,
    parser::{Ast, Block, Expr, ExprId, Stmt},
};
use std::collections::HashMap;

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
}

struct Compiler<'a> {
    ast: &'a Ast,
    function: Function<'a>,
    parents: Vec<Function<'a>>,
    prototypes: Vec<Prototype>,
    folded: Vec<Option<f64>>,
    depth: u16,
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
        depth: 0,
        line: 1,
    };
    c.block(&ast.body)?;
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
            registers: f.max.max(1),
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
                temporaries: f.debug_temporaries.into_boxed_slice(),
            },
        });
        id
    }
    fn upvalue(&mut self, name: &'a [u8]) -> Result<Option<u16>, Error> {
        fn capture<'a>(
            parents: &mut [Function<'a>],
            name: &'a [u8],
        ) -> Result<Option<Upvalue>, ()> {
            let Some((parent, ancestors)) = parents.split_last_mut() else {
                return Ok(None);
            };
            if let Some(local) = parent.locals.iter().rev().find(|l| l.name == name) {
                return Ok(Some(Upvalue::Local(local.reg)));
            }
            if let Some(i) = parent.upvalues.iter().position(|(n, _)| *n == name) {
                return Ok(Some(Upvalue::Outer(i as u16)));
            }
            if let Some(up) = capture(ancestors, name)? {
                if parent.upvalues.len() >= 60 {
                    return Err(());
                }
                let i = parent.upvalues.len() as u16;
                parent.upvalues.push((name, up));
                return Ok(Some(Upvalue::Outer(i)));
            }
            Ok(None)
        }
        if let Some(i) = self.function.upvalues.iter().position(|(n, _)| *n == name) {
            return Ok(Some(i as u16));
        }
        if let Some(up) = capture(&mut self.parents, name)
            .map_err(|()| self.error(ErrorKind::Limit, "too many upvalues"))?
        {
            if self.function.upvalues.len() >= 60 {
                return Err(self.error(ErrorKind::Limit, "too many upvalues"));
            }
            let i = self.function.upvalues.len() as u16;
            self.function.upvalues.push((name, up));
            return Ok(Some(i));
        }
        Ok(None)
    }
    fn child(
        &mut self,
        params: &'a [Box<[u8]>],
        vararg: bool,
        body: &Block,
        defined: [u32; 2],
    ) -> Result<u32, Error> {
        let parent = std::mem::replace(
            &mut self.function,
            Function {
                vararg,
                compat_arg: vararg,
                defined,
                ..Function::default()
            },
        );
        self.parents.push(parent);
        self.function.params = params.len() as u16;
        for name in params {
            let r = self.alloc(1)?;
            self.bind(name, r)?;
        }
        if vararg {
            let r = self.alloc(1)?;
            self.bind(b"arg", r)?;
        }
        self.block(body)?;
        self.line = defined[1];
        self.emit(Op::Return, 0, 0, 0)?;
        let id = self.finish();
        self.function = self.parents.pop().expect("nested compiler has a parent");
        Ok(id)
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
        self.function.lines.push(self.line);
        self.function
            .debug_lines
            .push(if op == Op::Close { 0 } else { self.line });
        self.function
            .gc_live
            .push(u32::from(before) | (u32::from(after) << 16));
        Ok(pc)
    }
    fn patch(&mut self, pc: usize) {
        self.function.code[pc].c = self.function.code.len() as u32;
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
        let id = self.constant(k)?;
        self.emit(Op::Load, dst, 0, id)?;
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
    fn block(&mut self, block: &Block) -> Result<(), Error> {
        for &id in block.iter() {
            let (stmt, line) = &self.ast.stmts[id];
            self.line = *line;
            self.statement(stmt)?;
        }
        Ok(())
    }
    fn scope(&mut self, body: &Block) -> Result<(), Error> {
        let saved = (self.function.locals.len(), self.function.next);
        self.block(body)?;
        self.close(saved.1)?;
        self.end_locals(saved.0);
        self.reset(saved.1);
        Ok(())
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
    fn values(&mut self, values: &[ExprId], wanted: usize) -> Result<u8, Error> {
        let count = u16::try_from(wanted.max(values.len()))
            .map_err(|_| self.error(ErrorKind::Limit, "too many expression values"))?;
        let start = self.alloc(count.max(1))?;
        for (index, &value) in values.iter().enumerate() {
            let dst = start as u16 + index as u16;
            let returns = if index + 1 == values.len() {
                wanted.saturating_sub(index) as u32
            } else {
                1
            };
            self.expr_results(value, dst as u8, returns)?;
        }
        for index in values.len()..wanted {
            if values.last().is_some_and(|id| self.multiple(*id)) {
                break;
            }
            self.load((usize::from(start) + index) as u8, Constant::Nil)?;
        }
        Ok(start)
    }
    fn multiple(&self, id: ExprId) -> bool {
        matches!(
            self.ast.exprs[id].0,
            Expr::Call(..) | Expr::Method(..) | Expr::Vararg
        )
    }

    fn statement(&mut self, stmt: &'a Stmt) -> Result<(), Error> {
        let saved = self.function.next;
        let statement_line = self.line;
        match stmt {
            Stmt::Local(names, values) => {
                let base = self.values(values, names.len())?;
                self.reset(u16::from(base) + names.len() as u16);
                for (i, name) in names.iter().enumerate() {
                    self.bind(name, (usize::from(base) + i) as u8)?;
                }
                return Ok(());
            }
            Stmt::Assign(targets, values) | Stmt::FunctionDefinition(targets, values) => {
                let mut locations: Vec<Target> = Vec::with_capacity(targets.len());
                for &id in targets.iter() {
                    let target = match &self.ast.exprs[id].0 {
                        Expr::Name(name) => {
                            if let Some(reg) = self.local(name) {
                                Target::Local(reg)
                            } else if let Some(up) = self.upvalue(name)? {
                                Target::Upvalue(up)
                            } else {
                                Target::Global(self.constant(Constant::String(name.clone()))?)
                            }
                        }
                        Expr::Index(table, key) => {
                            let t = self.alloc(2)?;
                            let table = self.operand(*table, t)?;
                            let key = self.operand(*key, t + 1)?;
                            Target::Index(table, key)
                        }
                        _ => return Err(self.error(ErrorKind::Syntax, "invalid assignment target")),
                    };
                    if let Target::Local(reg) = target
                        && locations.iter().any(|t| matches!(t, Target::Index(table, key) if *table == reg || *key == reg)) {
                            let saved = self.alloc(1)?;
                            self.emit(Op::Move, saved, u16::from(reg), 0)?;
                            for target in &mut locations {
                                if let Target::Index(table, key) = target {
                                    if *table == reg { *table = saved; }
                                    if *key == reg { *key = saved; }
                                }
                            }
                    }
                    locations.push(target);
                }
                let base = self.values(values, targets.len())?;
                if matches!(stmt, Stmt::FunctionDefinition(..)) {
                    self.line = statement_line;
                }
                for (i, target) in locations.iter().enumerate().rev() {
                    let src = (usize::from(base) + i) as u8;
                    match target {
                        Target::Local(dst) => {
                            self.emit(Op::Move, *dst, u16::from(src), 0)?;
                        }
                        Target::Upvalue(up) => {
                            self.emit(Op::SetUpvalue, src, 0, u32::from(*up))?;
                        }
                        Target::Global(k) => {
                            self.emit(Op::SetGlobal, src, 0, *k)?;
                        }
                        Target::Index(table, key) => {
                            self.emit(Op::SetTable, *table, u16::from(*key), u32::from(src))?;
                        }
                    }
                }
            }
            Stmt::Call(id) => {
                let dst = self.alloc(1)?;
                self.expr_results(*id, dst, 0)?;
            }
            Stmt::Return(values, end_line) => {
                if values.len() == 1 {
                    let call = match &self.ast.exprs[values[0]].0 {
                        Expr::Call(function, args, line) => {
                            Some((*function, None, args.as_ref(), *line))
                        }
                        Expr::Method(receiver, name, args, line) => {
                            Some((*receiver, Some(name.as_ref()), args.as_ref(), *line))
                        }
                        _ => None,
                    };
                    if let Some((function, method, args, line)) = call {
                        let dst = self.alloc(1)?;
                        self.line = line;
                        self.call(function, method, args, dst, TAIL)?;
                        *self
                            .function
                            .lines
                            .last_mut()
                            .expect("tail call emits return") = *end_line;
                        self.reset(saved);
                        return Ok(());
                    }
                }
                let base = self.alloc(
                    u16::try_from(values.len().max(1))
                        .map_err(|_| self.error(ErrorKind::Limit, "too many return values"))?,
                )?;
                let mut count = values.len() as u32;
                for (i, &id) in values.iter().enumerate() {
                    let all = i + 1 == values.len() && self.multiple(id);
                    self.expr_results(
                        id,
                        (usize::from(base) + i) as u8,
                        if all { OPEN } else { 1 },
                    )?;
                    if all {
                        count = OPEN;
                    }
                }
                self.line = *end_line;
                self.emit(Op::Return, base, 0, count)?;
            }
            Stmt::Do(body) => self.scope(body)?,
            Stmt::While(cond, body) => {
                let start = self.function.code.len() as u32;
                let r = self.alloc(1)?;
                self.expr(*cond, r)?;
                let done = self.emit(Op::JumpFalse, r, 0, 0)?;
                self.reset(saved);
                self.function.loops.push((saved, Vec::new()));
                self.scope(body)?;
                self.emit(Op::Jump, 0, 0, start)?;
                self.patch(done);
                self.finish_loop();
            }
            Stmt::Repeat(body, cond) => {
                let start = self.function.code.len() as u32;
                let locals = self.function.locals.len();
                self.function.loops.push((saved, Vec::new()));
                self.block(body)?;
                let r = self.alloc(1)?;
                self.expr(*cond, r)?;
                self.close(saved)?;
                self.emit(Op::JumpFalse, r, 0, start)?;
                self.end_locals(locals);
                self.finish_loop();
            }
            Stmt::If(arms, other) => {
                let mut exits = Vec::with_capacity(arms.len());
                for (cond, body) in arms.iter() {
                    let r = self.alloc(1)?;
                    self.expr(*cond, r)?;
                    let skip = self.emit(Op::JumpFalse, r, 0, 0)?;
                    self.reset(saved);
                    self.scope(body)?;
                    exits.push(self.emit(Op::Jump, 0, 0, 0)?);
                    self.patch(skip);
                }
                self.scope(other)?;
                for pc in exits {
                    self.patch(pc);
                }
            }
            Stmt::For {
                name,
                start,
                limit,
                step,
                do_line,
                body,
            } => {
                let base = self.alloc(4)?;
                self.expr(*start, base)?;
                self.expr(*limit, base + 1)?;
                if let Some(id) = step {
                    self.expr(*id, base + 2)?;
                } else {
                    self.load(base + 2, Constant::Number(1.0))?;
                }
                let controls = self.function.debug_locals.len();
                for (i, name) in [b"(for index)".as_slice(), b"(for limit)", b"(for step)"]
                    .into_iter()
                    .enumerate()
                {
                    self.debug_local(name, base + i as u8);
                }
                self.line = *do_line;
                let prep = self.emit(Op::ForPrep, base, 0, 0)?;
                let head = self.function.code.len() as u32;
                let locals = self.function.locals.len();
                self.bind(name, base + 3)?;
                self.function.loops.push((saved, Vec::new()));
                self.scope(body)?;
                self.end_locals(locals);
                self.close(u16::from(base) + 3)?;
                self.patch(prep);
                self.line = statement_line;
                self.emit(Op::ForLoop, base, 0, head)?;
                let end = self.function.code.len() as u32;
                for local in &mut self.function.debug_locals[controls..controls + 3] {
                    local.end = end;
                    self.function.debug_named[usize::from(local.register) / 64] &=
                        !(1u64 << (local.register % 64));
                }
                self.finish_loop();
            }
            Stmt::Break => {
                if self.function.loops.is_empty() {
                    return Err(self.error(ErrorKind::Syntax, "no loop to break"));
                }
                self.close(
                    self.function
                        .loops
                        .last()
                        .expect("loop was checked above")
                        .0,
                )?;
                let pc = self.emit(Op::Jump, 0, 0, 0)?;
                self.function
                    .loops
                    .last_mut()
                    .expect("loop was checked above")
                    .1
                    .push(pc);
            }
            Stmt::LocalFunction(name, value) => {
                let reg = self.alloc(1)?;
                self.bind(name, reg)?;
                self.expr(*value, reg)?;
                let index = self
                    .function
                    .locals
                    .last()
                    .expect("local function is bound")
                    .debug;
                self.function.debug_locals[index].start = self.function.code.len() as u32;
                return Ok(());
            }
            Stmt::ForIn(names, values, body, lines) => {
                let base = self.values(values, 3)?;
                self.reset(u16::from(base) + 3);
                let controls = self.function.debug_locals.len();
                for (i, name) in [
                    b"(for generator)".as_slice(),
                    b"(for state)",
                    b"(for control)",
                ]
                .into_iter()
                .enumerate()
                {
                    self.debug_local(name, base + i as u8);
                }
                let results = self.alloc(names.len().max(3) as u16)?;
                let test = self.alloc(1)?;
                self.function.debug_lines.push(lines[0]);
                self.line = lines[1];
                let head = self.function.code.len() as u32;
                for i in 0..3 {
                    self.emit(Op::Move, results + i, u16::from(base + i), 0)?;
                }
                self.function.debug_calls.push(DebugCall {
                    pc: self.function.code.len() as u32,
                    name: b"(for generator)".as_slice().into(),
                    kind: NameKind::Local,
                    temporaries: 0..0,
                });
                self.emit(Op::Call, results, 2, names.len() as u32)?;
                self.load(test, Constant::Nil)?;
                self.emit(Op::Eq, test, u16::from(results), u32::from(test))?;
                self.function.debug_hidden[usize::from(test) / 64] |= 1u64 << (test % 64);
                let done = self.emit(Op::JumpTrue, test, 0, 0)?;
                self.emit(Op::Move, base + 2, u16::from(results), 0)?;
                let locals = self.function.locals.len();
                for (i, name) in names.iter().enumerate() {
                    self.bind(name, results + i as u8)?;
                }
                self.function.loops.push((saved, Vec::new()));
                self.scope(body)?;
                self.close(u16::from(results))?;
                self.end_locals(locals);
                self.emit(Op::Jump, 0, 0, head)?;
                self.patch(done);
                let end = self.function.code.len() as u32;
                for local in &mut self.function.debug_locals[controls..controls + 3] {
                    local.end = end;
                    self.function.debug_named[usize::from(local.register) / 64] &=
                        !(1u64 << (local.register % 64));
                }
                self.finish_loop();
            }
        }
        self.reset(saved);
        Ok(())
    }

    fn expr_results(&mut self, id: ExprId, dst: u8, wanted: u32) -> Result<(), Error> {
        match &self.ast.exprs[id].0 {
            Expr::Call(function, args, line) => {
                self.line = *line;
                self.call(*function, None, args, dst, wanted)
            }
            Expr::Method(receiver, method, args, line) => {
                self.line = *line;
                self.call(*receiver, Some(method), args, dst, wanted)
            }
            Expr::Vararg => {
                if !self.function.vararg {
                    return Err(self.error(
                        ErrorKind::Syntax,
                        "cannot use '...' outside a vararg function",
                    ));
                }
                self.function.compat_arg = false;
                self.emit(Op::Vararg, dst, 0, wanted)?;
                Ok(())
            }
            _ => {
                self.expr(id, dst)?;
                if wanted != OPEN {
                    for i in 1..wanted {
                        self.load((u32::from(dst) + i) as u8, Constant::Nil)?;
                    }
                }
                Ok(())
            }
        }
    }

    fn call(
        &mut self,
        function: ExprId,
        method: Option<&[u8]>,
        args: &[ExprId],
        dst: u8,
        wanted: u32,
    ) -> Result<(), Error> {
        let call_line = self.line;
        // Build arguments above all live temporaries, then copy the requested results down.
        let saved = self.function.next;
        let count = args.len() + usize::from(method.is_some());
        let base = self.alloc(
            u16::try_from(count + 1)
                .map_err(|_| self.error(ErrorKind::Limit, "too many call arguments"))?,
        )?;
        if let Some(method) = method {
            self.expr(function, base + 1)?;
            let key = self.alloc(1)?;
            self.load(key, Constant::String(method.into()))?;
            self.function.debug_hidden[usize::from(key) / 64] |= 1u64 << (key % 64);
            self.emit(Op::GetTable, base, u16::from(base + 1), u32::from(key))?;
        } else {
            self.expr(function, base)?;
        }
        let mut argc = count as u16;
        for (i, &id) in args.iter().enumerate() {
            let all = i + 1 == args.len() && self.multiple(id);
            self.expr_results(
                id,
                (usize::from(base) + 1 + usize::from(method.is_some()) + i) as u8,
                if all { OPEN } else { 1 },
            )?;
            if all {
                argc = OPEN_ARGS;
            }
        }
        self.line = call_line;
        if wanted == TAIL {
            self.record_call(function, method, base)?;
            self.emit(Op::TailCall, base, argc, 0)?;
            // Lua tail calls replace the frame. A resumable native call returns here.
            self.emit(Op::Return, base, 0, OPEN)?;
        } else if wanted == OPEN {
            self.record_call(function, method, base)?;
            // Open result ranges are relocated by Call using b/c-independent metadata in Move.
            self.emit(Op::Call, base, argc, OPEN)?;
            self.emit(Op::Move, dst, u16::from(base), OPEN)?;
        } else {
            let needed = u16::from(base) + wanted as u16;
            if needed > self.function.next {
                self.alloc(needed - self.function.next)?;
            }
            self.record_call(function, method, base)?;
            self.emit(Op::Call, base, argc, wanted)?;
            for i in 0..wanted {
                self.emit(
                    Op::Move,
                    (u32::from(dst) + i) as u8,
                    u16::from(base) + i as u16,
                    0,
                )?;
            }
        }
        self.reset(saved);
        Ok(())
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

    fn expr(&mut self, id: ExprId, dst: u8) -> Result<(), Error> {
        if self.depth >= 120 {
            return Err(self.error(ErrorKind::Limit, "expression nesting limit exceeded"));
        }
        self.depth += 1;
        let saved = self.function.next;
        let (expr, span) = &self.ast.exprs[id];
        self.line = self.ast.exprs[id].1.discharge;
        if let Some(n) = self.folded[id] {
            self.load(dst, Constant::Number(n))?;
            self.depth -= 1;
            return Ok(());
        }
        match expr {
            Expr::Nil => self.load(dst, Constant::Nil)?,
            Expr::Boolean(b) => self.load(dst, Constant::Boolean(*b))?,
            Expr::Number(n) => self.load(dst, Constant::Number(*n))?,
            Expr::String(s) => self.load(dst, Constant::String(s.clone()))?,
            Expr::Name(name) => {
                if let Some(src) = self.local(name) {
                    self.emit(Op::Move, dst, u16::from(src), 0)?;
                } else if let Some(up) = self.upvalue(name)? {
                    self.emit(Op::GetUpvalue, dst, 0, u32::from(up))?;
                } else {
                    let k = self.constant(Constant::String(name.clone()))?;
                    self.emit(Op::GetGlobal, dst, 0, k)?;
                }
            }
            Expr::Paren(inner) => {
                self.expr(*inner, dst)?;
                self.discharge_line(*inner, self.ast.exprs[id].1.discharge, LineUse::Copy);
            }
            Expr::Unary(op, inner) => {
                self.expr(*inner, dst)?;
                self.line = self.ast.exprs[id].1.end;
                let op = match op {
                    Kind::Minus => Op::Neg,
                    Kind::Not => Op::Not,
                    Kind::Hash => Op::Len,
                    _ => unreachable!("parser produced invalid unary operator"),
                };
                self.emit(op, dst, u16::from(dst), 0)?;
            }
            Expr::Binary(Kind::And | Kind::Or, left, right, op_line) => {
                self.expr(*left, dst)?;
                self.discharge_line(*left, *op_line, LineUse::Condition);
                self.line = *op_line;
                let and = matches!(expr, Expr::Binary(Kind::And, ..));
                let mut first = *left;
                while let Expr::Paren(inner) = self.ast.exprs[first].0 {
                    first = inner;
                }
                let truth = match &self.ast.exprs[first].0 {
                    Expr::Nil | Expr::Boolean(false) => Some(false),
                    Expr::Boolean(true) | Expr::String(_) => Some(true),
                    _ if self.folded[first].is_some() => Some(true),
                    _ => None,
                };
                let omitted = truth == Some(and);
                if omitted {
                    *self
                        .function
                        .debug_lines
                        .last_mut()
                        .expect("constant condition emits a value") = 0;
                }
                let op = if matches!(expr, Expr::Binary(Kind::And, ..)) {
                    Op::JumpFalse
                } else {
                    Op::JumpTrue
                };
                let done = self.emit(op, dst, 0, 0)?;
                if omitted
                    || matches!(
                        self.ast.exprs[first].0,
                        Expr::Binary(
                            Kind::Eq | Kind::Ne | Kind::Lt | Kind::Le | Kind::Gt | Kind::Ge,
                            ..
                        )
                    )
                {
                    *self
                        .function
                        .debug_lines
                        .last_mut()
                        .expect("condition emits a branch") = 0;
                }
                self.function.debug_hidden[usize::from(dst) / 64] |= 1u64 << (dst % 64);
                self.expr(*right, dst)?;
                self.discharge_line(*right, self.ast.exprs[id].1.discharge, LineUse::Copy);
                self.patch(done);
            }
            Expr::Binary(op, left, right, op_line) => {
                let regs = self.alloc(2)?;
                let (left_reg, right_reg) = if *op == Kind::Concat {
                    self.expr(*left, regs)?;
                    self.discharge_line(*left, *op_line, LineUse::Copy);
                    self.expr(*right, regs + 1)?;
                    (regs, regs + 1)
                } else {
                    let left_reg = self.operand(*left, regs)?;
                    self.discharge_line(*left, *op_line, LineUse::Operand);
                    let right_reg = self.operand(*right, regs + 1)?;
                    (left_reg, right_reg)
                };
                self.line = self.ast.exprs[id].1.end;
                let op = match op {
                    Kind::Plus => Op::Add,
                    Kind::Minus => Op::Sub,
                    Kind::Star => Op::Mul,
                    Kind::Slash => Op::Div,
                    Kind::Percent => Op::Mod,
                    Kind::Caret => Op::Pow,
                    Kind::Concat => Op::Concat,
                    Kind::Eq => Op::Eq,
                    Kind::Ne => Op::Ne,
                    Kind::Lt => Op::Lt,
                    Kind::Le => Op::Le,
                    Kind::Gt => Op::Gt,
                    Kind::Ge => Op::Ge,
                    _ => unreachable!("parser produced invalid binary operator"),
                };
                self.emit(op, dst, u16::from(left_reg), u32::from(right_reg))?;
                if matches!(op, Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge) {
                    self.function
                        .debug_lines
                        .push(self.ast.exprs[id].1.discharge);
                }
            }
            Expr::Index(table, key) => {
                let regs = self.alloc(2)?;
                let table = self.operand(*table, regs)?;
                let key = self.operand(*key, regs + 1)?;
                self.line = self.ast.exprs[id].1.discharge;
                self.emit(Op::GetTable, dst, u16::from(table), u32::from(key))?;
            }
            Expr::Table(fields, load_line) => {
                let table = self.alloc(1)?;
                self.line = *load_line;
                self.emit(Op::NewTable, table, 0, 0)?;
                let mut flushed = 0u32;
                let mut pending = 0u16;
                for (i, (key, value)) in fields.iter().enumerate() {
                    if let Some(key) = key {
                        let regs = self.alloc(2)?;
                        let key = self.operand(*key, regs)?;
                        let value = self.operand(*value, regs + 1)?;
                        self.emit(Op::SetTable, table, u16::from(key), u32::from(value))?;
                        self.reset(u16::from(regs));
                    } else {
                        let reg = self.alloc(1)?;
                        let open = i + 1 == fields.len() && self.multiple(*value);
                        self.expr_results(*value, reg, if open { OPEN } else { 1 })?;
                        pending += 1;
                        if open || pending == 50 {
                            self.emit(
                                Op::SetList,
                                table,
                                if open { OPEN_ARGS } else { pending },
                                flushed + 1,
                            )?;
                            flushed += u32::from(pending);
                            pending = 0;
                            self.reset(u16::from(table) + 1);
                        }
                    }
                }
                if pending != 0 {
                    self.line = self.ast.exprs[id].1.end;
                    self.emit(Op::SetList, table, pending, flushed + 1)?;
                }
                self.emit(Op::Move, dst, u16::from(table), 0)?;
            }
            Expr::Call(function, args, line) => {
                self.line = *line;
                self.call(*function, None, args, dst, 1)?;
            }
            Expr::Method(receiver, method, args, line) => {
                self.line = *line;
                self.call(*receiver, Some(method), args, dst, 1)?
            }
            Expr::Function {
                params,
                vararg,
                body,
                end_line,
            } => {
                let child = self.child(params, *vararg, body, [span.start, *end_line])?;
                self.emit(Op::Closure, dst, 0, child)?;
            }
            Expr::Vararg => self.expr_results(id, dst, 1)?,
        }
        self.reset(saved);
        self.depth -= 1;
        Ok(())
    }

    fn operand(&mut self, id: ExprId, dst: u8) -> Result<u8, Error> {
        let mut reference = id;
        while let Expr::Paren(inner) = self.ast.exprs[reference].0 {
            reference = inner;
        }
        if let Expr::Name(name) = &self.ast.exprs[reference].0
            && let Some(reg) = self.local(name)
        {
            self.line = self.ast.exprs[id].1.discharge;
            return Ok(reg);
        }
        self.expr(id, dst)?;
        // RK constants do not occupy a visible temporary in the 5.1 stack.
        let id = reference;
        if self.folded[id].is_some()
            || matches!(
                &self.ast.exprs[id].0,
                Expr::Nil | Expr::Boolean(_) | Expr::String(_)
            )
        {
            self.function.debug_hidden[usize::from(dst) / 64] |= 1u64 << (dst % 64);
            *self
                .function
                .debug_lines
                .last_mut()
                .expect("operand emits a value") = 0;
        }
        Ok(dst)
    }

    fn discharge_line(&mut self, mut id: ExprId, line: u32, usage: LineUse) {
        loop {
            match &self.ast.exprs[id].0 {
                Expr::Paren(inner) | Expr::Binary(Kind::And | Kind::Or, _, inner, _) => id = *inner,
                Expr::Name(name) => {
                    if matches!(usage, LineUse::Condition) && self.local(name).is_some() {
                        *self
                            .function
                            .debug_lines
                            .last_mut()
                            .expect("local emits a copy") = 0;
                    } else if matches!(usage, LineUse::Copy) || self.local(name).is_none() {
                        *self
                            .function
                            .debug_lines
                            .last_mut()
                            .expect("name emits a value") = line;
                    }
                    return;
                }
                Expr::Nil | Expr::Boolean(_) | Expr::Number(_) | Expr::String(_) => {
                    if !matches!(usage, LineUse::Operand) {
                        *self
                            .function
                            .debug_lines
                            .last_mut()
                            .expect("literal emits a value") = line;
                    }
                    return;
                }
                Expr::Index(..) => {
                    *self
                        .function
                        .debug_lines
                        .last_mut()
                        .expect("index emits a value") = line;
                    return;
                }
                _ => return,
            }
        }
    }
}
