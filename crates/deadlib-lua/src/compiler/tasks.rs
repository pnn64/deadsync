//! Flat code-generation continuations. No task recursively enters the compiler.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct CallState {
    pub id: ExprId,
    pub wanted: u32,
    pub saved: u16,
    pub index: u16,
    pub base: u8,
    pub dst: u8,
}
#[derive(Clone, Copy)]
pub(super) struct TableState {
    pub id: ExprId,
    pub index: usize,
    pub flushed: u32,
    pub pending: u16,
    pub table: u8,
    pub dst: u8,
}

pub(super) enum Task<'a> {
    Block(&'a [StmtId]),
    Statement(StmtId),
    Scope(&'a [StmtId]),
    ScopeEnd(usize, u16),
    Reset(u16),
    Values(&'a [ExprId], usize),
    ValuesNext(&'a [ExprId], u16, u8, u16),
    BindLocal(StmtId),
    LocalFunctionEnd,
    AssignNext(StmtId, usize, Vec<Target>),
    AssignKey(StmtId, usize, Vec<Target>, u8),
    AssignIndex(StmtId, usize, Vec<Target>, u8),
    AssignEnd(StmtId, Vec<Target>),
    ReturnNext(StmtId, u8, usize, u32),
    TailEnd(u32),
    WhileCond(StmtId, u16, u32, u8),
    WhileEnd(u32, usize),
    RepeatCond(StmtId, u16, u32, usize),
    RepeatEnd(u16, u32, usize, u8),
    IfNext(StmtId, usize, u16, Vec<usize>),
    IfCond(StmtId, usize, u16, Vec<usize>, u8),
    IfBody(StmtId, usize, u16, Vec<usize>, usize),
    IfEnd(Vec<usize>),
    ForLimit(StmtId, u16, u8),
    ForStep(StmtId, u16, u8),
    ForInit(StmtId, u16, u8),
    ForEnd(StmtId, u8, usize, usize, usize, u32),
    ForInInit(StmtId, u16),
    ForInEnd(u8, usize, usize, u32, usize),
    Expr(ExprId, u8),
    ExprEnd(u16, u8),
    Results(ExprId, u8, u32),
    FillNil(u8, u32),
    Operand(ExprId, u8),
    OperandEnd(ExprId, u8),
    Paren(ExprId, u32),
    Unary(Kind, u8, u8, u32),
    LogicalLeft(ExprId, u8),
    LogicalEnd(ExprId, u32, usize),
    BinaryLeft(ExprId, u8, u8),
    BinaryRight(ExprId, u8, u8),
    IndexTable(ExprId, u8, u8),
    IndexKey(ExprId, u8, u8),
    TableNext(TableState),
    TableKey(TableState, u8),
    TableValue(TableState, u8, u8),
    TableConstant(TableState, u8, u32),
    TableList(TableState, bool),
    Call(ExprId, u8, u32),
    CallCallee(CallState),
    CallArgs(CallState),
    ChildEnd(u32, u8),
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<Task<'_>>() <= 64);

impl<'a> Compiler<'a> {
    pub(super) fn run(&mut self) -> Result<(), Error> {
        while let Some(task) = self.tasks.pop() {
            match task {
                Task::Block(block) => {
                    if let Some((&id, rest)) = block.split_first() {
                        self.tasks.push(Task::Block(rest));
                        self.tasks.push(Task::Statement(id));
                    }
                }
                Task::Statement(id) => self.statement(id)?,
                Task::Scope(body) => {
                    self.tasks.push(Task::ScopeEnd(
                        self.function.locals.len(),
                        self.function.next,
                    ));
                    self.tasks.push(Task::Block(body));
                }
                Task::ScopeEnd(locals, next) => {
                    self.close(next)?;
                    self.end_locals(locals);
                    self.reset(next);
                }
                Task::Reset(next) => self.reset(next),
                Task::Values(values, wanted) => {
                    let count = u16::try_from(wanted.max(values.len()))
                        .map_err(|_| self.error(ErrorKind::Limit, "too many expression values"))?;
                    let start = self.alloc(count.max(1))?;
                    self.tasks
                        .push(Task::ValuesNext(values, wanted as u16, start, 0));
                }
                Task::ValuesNext(values, wanted, start, index) => {
                    if let Some(&value) = values.get(usize::from(index)) {
                        let returns = if usize::from(index) + 1 == values.len() {
                            u32::from(wanted.saturating_sub(index))
                        } else {
                            1
                        };
                        self.tasks
                            .push(Task::ValuesNext(values, wanted, start, index + 1));
                        self.tasks
                            .push(Task::Results(value, start + index as u8, returns));
                    } else {
                        if !values.last().is_some_and(|id| self.multiple(*id)) {
                            for index in values.len()..usize::from(wanted) {
                                self.load((usize::from(start) + index) as u8, Constant::Nil)?;
                            }
                        }
                        self.result = start;
                    }
                }
                Task::BindLocal(id) => {
                    let Stmt::Local(names, _) = &self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    let base = self.result;
                    self.reset(u16::from(base) + names.len() as u16);
                    for (i, name) in names.iter().enumerate() {
                        self.bind(name, (usize::from(base) + i) as u8)?;
                    }
                }
                Task::LocalFunctionEnd => {
                    let index = self
                        .function
                        .locals
                        .last()
                        .expect("bound local function")
                        .debug;
                    self.function.debug_locals[index].start = self.function.code.len() as u32;
                }
                Task::AssignNext(id, index, locations) => self.assign_next(id, index, locations)?,
                Task::AssignKey(id, index, locations, regs) => {
                    let table = self.result;
                    let (targets, _) = self.assignment(id);
                    let Expr::Index(_, key) = self.ast.exprs[targets[index]].0 else {
                        unreachable!()
                    };
                    self.tasks
                        .push(Task::AssignIndex(id, index, locations, table));
                    self.tasks.push(Task::Operand(key, regs + 1));
                }
                Task::AssignIndex(id, index, mut locations, table) => {
                    locations.push(Target::Index(table, self.result));
                    self.tasks.push(Task::AssignNext(id, index + 1, locations));
                }
                Task::AssignEnd(id, locations) => {
                    if matches!(self.ast.stmts[id].0, Stmt::FunctionDefinition(..)) {
                        self.line = self.ast.stmts[id].1;
                    }
                    let base = self.result;
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
                Task::ReturnNext(id, base, index, count) => {
                    let Stmt::Return(values, end_line) = &self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    if let Some(&value) = values.get(index) {
                        let all = index + 1 == values.len() && self.multiple(value);
                        self.tasks.push(Task::ReturnNext(
                            id,
                            base,
                            index + 1,
                            if all { OPEN } else { count },
                        ));
                        self.tasks.push(Task::Results(
                            value,
                            (usize::from(base) + index) as u8,
                            if all { OPEN } else { 1 },
                        ));
                    } else {
                        self.line = *end_line;
                        self.emit(Op::Return, base, 0, count)?;
                    }
                }
                Task::TailEnd(line) => {
                    *self
                        .function
                        .lines
                        .last_mut()
                        .expect("tail call emits return") = line;
                }
                Task::WhileCond(id, saved, start, reg) => {
                    let Stmt::While(_, body) = &self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    let done = self.emit(Op::JumpFalse, reg, 0, 0)?;
                    self.reset(saved);
                    self.function.loops.push((saved, Vec::new()));
                    self.tasks.push(Task::WhileEnd(start, done));
                    self.tasks.push(Task::Scope(body));
                }
                Task::WhileEnd(start, done) => {
                    self.emit(Op::Jump, 0, 0, start)?;
                    self.patch(done);
                    self.finish_loop();
                }
                Task::RepeatCond(id, saved, start, locals) => {
                    let Stmt::Repeat(_, cond) = self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    let reg = self.alloc(1)?;
                    self.tasks.push(Task::RepeatEnd(saved, start, locals, reg));
                    self.tasks.push(Task::Expr(cond, reg));
                }
                Task::RepeatEnd(saved, start, locals, reg) => {
                    self.close(saved)?;
                    self.emit(Op::JumpFalse, reg, 0, start)?;
                    self.end_locals(locals);
                    self.finish_loop();
                }
                Task::IfNext(id, index, saved, exits) => {
                    let Stmt::If(arms, other) = &self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    if let Some((cond, _)) = arms.get(index) {
                        let reg = self.alloc(1)?;
                        self.tasks.push(Task::IfCond(id, index, saved, exits, reg));
                        self.tasks.push(Task::Expr(*cond, reg));
                    } else {
                        self.tasks.push(Task::IfEnd(exits));
                        self.tasks.push(Task::Scope(other));
                    }
                }
                Task::IfCond(id, index, saved, exits, reg) => {
                    let Stmt::If(arms, _) = &self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    let skip = self.emit(Op::JumpFalse, reg, 0, 0)?;
                    self.reset(saved);
                    self.tasks.push(Task::IfBody(id, index, saved, exits, skip));
                    self.tasks.push(Task::Scope(&arms[index].1));
                }
                Task::IfBody(id, index, saved, mut exits, skip) => {
                    exits.push(self.emit(Op::Jump, 0, 0, 0)?);
                    self.patch(skip);
                    self.tasks.push(Task::IfNext(id, index + 1, saved, exits));
                }
                Task::IfEnd(exits) => {
                    for pc in exits {
                        self.patch(pc);
                    }
                }
                Task::ForLimit(id, saved, base) => {
                    let Stmt::For { limit, .. } = self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    self.tasks.push(Task::ForStep(id, saved, base));
                    self.tasks.push(Task::Expr(limit, base + 1));
                }
                Task::ForStep(id, saved, base) => {
                    let Stmt::For { step, .. } = self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    self.tasks.push(Task::ForInit(id, saved, base));
                    if let Some(step) = step {
                        self.tasks.push(Task::Expr(step, base + 2));
                    } else {
                        self.load(base + 2, Constant::Number(1.0))?;
                    }
                }
                Task::ForInit(id, saved, base) => {
                    let Stmt::For {
                        name,
                        body,
                        do_line,
                        ..
                    } = &self.ast.stmts[id].0
                    else {
                        unreachable!()
                    };
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
                    self.tasks
                        .push(Task::ForEnd(id, base, controls, locals, prep, head));
                    self.tasks.push(Task::Scope(body));
                }
                Task::ForEnd(id, base, controls, locals, prep, head) => {
                    self.end_locals(locals);
                    self.close(u16::from(base) + 3)?;
                    self.patch(prep);
                    self.line = self.ast.stmts[id].1;
                    self.emit(Op::ForLoop, base, 0, head)?;
                    self.end_controls(controls);
                    self.finish_loop();
                }
                Task::ForInInit(id, saved) => self.for_in(id, saved)?,
                Task::ForInEnd(results, controls, locals, head, done) => {
                    self.close(u16::from(results))?;
                    self.end_locals(locals);
                    self.emit(Op::Jump, 0, 0, head)?;
                    self.patch(done);
                    self.end_controls(controls);
                    self.finish_loop();
                }
                Task::Expr(id, dst) => self.expr(id, dst)?,
                Task::ExprEnd(saved, dst) => {
                    self.reset(saved);
                    self.result = dst;
                }
                Task::Results(id, dst, wanted) => self.expr_results(id, dst, wanted)?,
                Task::FillNil(dst, wanted) => {
                    if wanted != OPEN {
                        for i in 1..wanted {
                            self.load((u32::from(dst) + i) as u8, Constant::Nil)?;
                        }
                    }
                    self.result = dst;
                }
                Task::Operand(id, dst) => self.operand(id, dst),
                Task::OperandEnd(reference, dst) => {
                    if self.folded[reference].is_some()
                        || matches!(
                            &self.ast.exprs[reference].0,
                            Expr::Nil | Expr::Boolean(_) | Expr::String(_)
                        )
                    {
                        self.function.debug_hidden[usize::from(dst) / 64] |= 1u64 << (dst % 64);
                        self.debug_line(0);
                    }
                    self.result = dst;
                }
                Task::Paren(inner, line) => self.discharge_line(inner, line, LineUse::Copy),
                Task::Unary(op, dst, work, line) => {
                    self.line = line;
                    let op = match op {
                        Kind::Minus => Op::Neg,
                        Kind::Not => Op::Not,
                        Kind::Hash => Op::Len,
                        _ => unreachable!(),
                    };
                    self.emit(op, dst, u16::from(work), 0)?;
                }
                Task::LogicalLeft(id, dst) => self.logical_left(id, dst)?,
                Task::LogicalEnd(right, line, done) => {
                    self.discharge_line(right, line, LineUse::Copy);
                    self.patch(done);
                }
                Task::BinaryLeft(id, dst, work) => {
                    let Expr::Binary(op, left, right, line) = self.ast.exprs[id].0 else {
                        unreachable!()
                    };
                    let left_reg = self.result;
                    self.discharge_line(
                        left,
                        line,
                        if op == Kind::Concat {
                            LineUse::Copy
                        } else {
                            LineUse::Operand
                        },
                    );
                    let right_reg =
                        self.right_register(right, left_reg, work, op == Kind::Concat)?;
                    self.tasks.push(Task::BinaryRight(id, dst, left_reg));
                    self.tasks.push(if op == Kind::Concat {
                        Task::Expr(right, right_reg)
                    } else {
                        Task::Operand(right, right_reg)
                    });
                }
                Task::BinaryRight(id, dst, left) => self.binary_right(id, dst, left)?,
                Task::IndexTable(id, dst, work) => {
                    let Expr::Index(_, key) = self.ast.exprs[id].0 else {
                        unreachable!()
                    };
                    let table = self.result;
                    let key_reg = self.right_register(key, table, work, false)?;
                    self.tasks.push(Task::IndexKey(id, dst, table));
                    self.tasks.push(Task::Operand(key, key_reg));
                }
                Task::IndexKey(id, dst, table) => {
                    self.line = self.ast.exprs[id].1.discharge;
                    self.emit(Op::GetTable, dst, u16::from(table), u32::from(self.result))?;
                }
                Task::TableNext(state) => self.table_next(state)?,
                Task::TableKey(state, regs) => {
                    let Expr::Table(fields, _) = &self.ast.exprs[state.id].0 else {
                        unreachable!()
                    };
                    let key = self.result;
                    let value = fields[state.index].1;
                    let value_reg = self.right_register(value, key, regs, false)?;
                    self.tasks.push(Task::TableValue(state, regs, key));
                    self.tasks.push(Task::Operand(value, value_reg));
                }
                Task::TableValue(mut state, regs, key) => {
                    self.emit(
                        Op::SetTable,
                        state.table,
                        u16::from(key),
                        u32::from(self.result),
                    )?;
                    self.reset(u16::from(regs));
                    state.index += 1;
                    self.tasks.push(Task::TableNext(state));
                }
                Task::TableConstant(mut state, scratch, constant) => {
                    let value = self.result;
                    let key = if value == scratch {
                        self.alloc(1)?
                    } else {
                        scratch
                    };
                    let line = self.line;
                    let Expr::Table(fields, _) = &self.ast.exprs[state.id].0 else {
                        unreachable!()
                    };
                    self.line = self.ast.exprs[fields[state.index].0.expect("constant key")]
                        .1
                        .discharge;
                    self.emit(Op::Load, key, 0, constant)?;
                    self.function.debug_hidden[usize::from(key) / 64] |= 1u64 << (key % 64);
                    self.debug_line(0);
                    self.line = line;
                    self.emit(Op::SetTable, state.table, u16::from(key), u32::from(value))?;
                    self.reset(u16::from(scratch));
                    state.index += 1;
                    self.tasks.push(Task::TableNext(state));
                }
                Task::TableList(mut state, open) => {
                    state.pending += 1;
                    if open || state.pending == 50 {
                        self.emit(
                            Op::SetList,
                            state.table,
                            if open { OPEN_ARGS } else { state.pending },
                            state.flushed + 1,
                        )?;
                        state.flushed += u32::from(state.pending);
                        state.pending = 0;
                        self.reset(u16::from(state.table) + 1);
                    }
                    state.index += 1;
                    self.tasks.push(Task::TableNext(state));
                }
                Task::Call(id, dst, wanted) => self.call(id, dst, wanted)?,
                Task::CallCallee(state) => {
                    let (_, method, _, _) = self.call_parts(state.id);
                    if let Some(method) = method {
                        let receiver = self.alloc(1)?;
                        debug_assert_eq!(u16::from(receiver), u16::from(state.base) + 1);
                        self.emit(Op::Move, receiver, u16::from(state.base), 0)?;
                        let key = self.alloc(1)?;
                        self.load(key, Constant::String(method.into()))?;
                        self.function.debug_hidden[usize::from(key) / 64] |= 1u64 << (key % 64);
                        self.emit(
                            Op::GetTable,
                            state.base,
                            u16::from(receiver),
                            u32::from(key),
                        )?;
                        self.reset(u16::from(key));
                    }
                    self.tasks.push(Task::CallArgs(state));
                }
                Task::CallArgs(mut state) => {
                    let (_, method, args, _) = self.call_parts(state.id);
                    if let Some(&id) = args.get(usize::from(state.index)) {
                        let all = usize::from(state.index) + 1 == args.len() && self.multiple(id);
                        let dst = self.alloc(1)?;
                        debug_assert_eq!(
                            usize::from(dst),
                            usize::from(state.base)
                                + 1
                                + usize::from(method.is_some())
                                + usize::from(state.index)
                        );
                        state.index += 1;
                        self.tasks.push(Task::CallArgs(state));
                        self.tasks
                            .push(Task::Results(id, dst, if all { OPEN } else { 1 }));
                    } else {
                        self.call_end(state)?;
                    }
                }
                Task::ChildEnd(line, dst) => {
                    self.line = line;
                    self.emit(Op::Return, 0, 0, 0)?;
                    let child = self.finish();
                    self.function = self.parents.pop().expect("child has a parent");
                    self.emit(Op::Closure, dst, 0, child)?;
                }
            }
        }
        Ok(())
    }
}
