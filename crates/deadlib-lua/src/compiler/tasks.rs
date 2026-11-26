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
    Scope(&'a [StmtId]),
    ScopeEnd(usize, u16),
    Reset(u16),
    Values(&'a [ExprId], usize),
    ValuesNext(&'a [ExprId], u16, u8, u16),
    BindLocal(StmtId),
    LocalFunctionEnd,
    AssignNext(StmtId, usize),
    AssignKey(StmtId, usize),
    AssignIndex(StmtId, usize, u8),
    AssignEnd(StmtId, usize),
    ReturnNext(StmtId, u8, usize, u32),
    TailEnd(u32),
    WhileCond(StmtId, u16, u32, u8),
    WhileEnd(u32, boolean::List),
    RepeatCond(StmtId, u16, u32, usize),
    RepeatEnd(ExprId, u16, u32, usize, u8),
    IfNext(StmtId, usize, u16, boolean::List),
    IfCond(StmtId, usize, u16, boolean::List, u8),
    IfBody(StmtId, usize, u16, boolean::List, boolean::List),
    IfEnd(boolean::List),
    ForLimit(StmtId, u16, u8),
    ForStep(StmtId, u16, u8),
    ForInit(StmtId, u16, u8),
    ForEnd(StmtId, u8, usize, usize, usize, u32),
    ForInInit(StmtId, u16),
    ForInEnd(u8, usize, usize, u32, usize),
    Expr(ExprId, u8),
    ExprEnd(u16, u8),
    FillNil(u8, u32),
    Operand(ExprId, u8),
    Paren(ExprId, u32),
    Unary(ExprId, u8, u8),
    BooleanNode(ExprId, u8),
    BooleanLeaf(ExprId, u8),
    BooleanNot(ExprId, u8),
    BooleanLeft(ExprId, u8),
    BooleanRight(Kind, boolean::List),
    BooleanFinish(ExprId, u8),
    LogicalLeft(ExprId, u8),
    LogicalEnd(ExprId, u32, usize),
    BinaryLeft(ExprId, u8, u8),
    BinaryRight(ExprId, u8, u8),
    BinaryConstant(ExprId, u8, u8, u32),
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
    ChildEnd(u32, u8, Option<templates::Source>, u32),
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<Task<'_>>() <= 48);

impl<'a> Compiler<'a> {
    pub(super) fn run(&mut self) -> Result<(), Error> {
        while let Some(task) = self.tasks.pop() {
            match task {
                Task::Block(block) => {
                    if let Some((&id, rest)) = block.split_first() {
                        // The statement is the next continuation: start it directly.
                        self.tasks.try_push(Task::Block(rest))?;
                        self.statement(id)?;
                    }
                }
                Task::Scope(body) => {
                    self.tasks.try_push(Task::ScopeEnd(
                        self.function.locals.len(),
                        self.function.next,
                    ))?;
                    self.tasks.try_push(Task::Block(body))?;
                }
                Task::ScopeEnd(locals, next) => {
                    let captured = self.function.locals[locals..].iter().any(|l| l.captured);
                    self.end_locals(locals);
                    if captured {
                        self.close(next)?;
                        self.debug_line(self.line);
                    }
                    self.reset(next);
                }
                Task::Reset(next) => self.reset(next),
                Task::Values(values, wanted) => {
                    let start = self.alloc(1)?;
                    self.tasks
                        .try_push(Task::ValuesNext(values, wanted as u16, start, 0))?;
                }
                Task::ValuesNext(values, wanted, start, index) => {
                    if let Some(&value) = values.get(usize::from(index)) {
                        let end = u16::from(start) + index + 1;
                        if end > self.function.next {
                            self.alloc(end - self.function.next)?;
                        }
                        let returns = if usize::from(index) + 1 == values.len() {
                            u32::from(wanted.saturating_sub(index))
                        } else {
                            1
                        };
                        self.tasks
                            .try_push(Task::ValuesNext(values, wanted, start, index + 1))?;
                        self.expr_results(value, start + index as u8, returns)?;
                    } else {
                        let end = u16::from(start) + wanted.max(values.len() as u16).max(1);
                        if end > self.function.next {
                            self.alloc(end - self.function.next)?;
                        }
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
                    let names = self.ast.names(*names);
                    let base = self.result;
                    self.reset(u16::from(base) + names.len() as u16);
                    for (i, name) in names.iter().enumerate() {
                        self.bind(name.bytes(self.source), (usize::from(base) + i) as u8)?;
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
                Task::AssignNext(id, start) => self.assign_next(id, start)?,
                Task::AssignKey(id, start) => self.assign_key(id, start, self.result)?,
                Task::AssignIndex(id, start, table) => {
                    self.assignment_targets
                        .try_push(Target::Index(table, TargetKey::Register(self.result)))?;
                    self.tasks.try_push(Task::AssignNext(id, start))?;
                }
                Task::AssignEnd(id, start) => {
                    if matches!(self.ast.stmts[id].0, Stmt::FunctionDefinition(..)) {
                        self.line = self.ast.stmts[id].1;
                    } else {
                        let (_, values) = self.assignment(id);
                        self.line = self.ast.exprs[*values.last().expect("assignment value")]
                            .1
                            .discharge;
                    }
                    let base = self.result;
                    for at in (start..self.assignment_targets.len()).rev() {
                        let src = (usize::from(base) + at - start) as u8;
                        match self.assignment_targets[at] {
                            Target::Local(dst) => {
                                self.emit(Op::Move, dst, u16::from(src), 0)?;
                            }
                            Target::Upvalue(up) => {
                                self.emit(Op::SetUpvalue, src, 0, u32::from(up))?;
                            }
                            Target::Global(k) => {
                                self.emit(Op::SetGlobal, src, 0, k)?;
                            }
                            Target::Index(table, key) => {
                                let saved = self.function.next;
                                let reg = match key {
                                    TargetKey::Register(reg) => reg,
                                    TargetKey::Constant(constant) => {
                                        let reg = self.alloc(1)?;
                                        self.emit(
                                            Op::Load,
                                            reg,
                                            if constant <= u32::from(u16::MAX) - RK {
                                                RK_LOAD
                                            } else {
                                                0
                                            },
                                            constant,
                                        )?;
                                        self.function.debug_hidden[usize::from(reg) / 64] |=
                                            1u64 << (reg % 64);
                                        self.debug_line(0);
                                        reg
                                    }
                                };
                                self.emit(Op::SetTable, table, u16::from(reg), u32::from(src))?;
                                self.reset(saved);
                            }
                        }
                    }
                    self.assignment_targets.truncate(start);
                }
                Task::ReturnNext(id, base, index, count) => {
                    let Stmt::Return(values, end_line) = &self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    let values = self.ast.ids(*values);
                    if let Some(&value) = values.get(index) {
                        let end = u16::from(base) + index as u16 + 1;
                        if end > self.function.next {
                            self.alloc(end - self.function.next)?;
                        }
                        let all = index + 1 == values.len() && self.multiple(value);
                        self.tasks.try_push(Task::ReturnNext(
                            id,
                            base,
                            index + 1,
                            if all { OPEN } else { count },
                        ))?;
                        self.expr_results(
                            value,
                            (usize::from(base) + index) as u8,
                            if all { OPEN } else { 1 },
                        )?;
                    } else {
                        self.line = *end_line;
                        self.emit(Op::Return, base, 0, count)?;
                    }
                }
                Task::TailEnd(line) => {
                    self.function
                        .code
                        .last_mut()
                        .expect("tail call emits return")
                        .line = line;
                }
                Task::WhileCond(id, saved, start, reg) => {
                    let Stmt::While(cond, body) = &self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    let done = self.predicate_finish(*cond, reg)?;
                    self.reset(saved);
                    self.function.loops.try_push((saved, Vec::new()))?;
                    self.tasks.try_push(Task::WhileEnd(start, done))?;
                    self.tasks.try_push(Task::Scope(self.ast.ids(*body)))?;
                }
                Task::WhileEnd(start, done) => {
                    self.emit(Op::Jump, 0, 0, start)?;
                    self.list_patch(done, self.function.code.len());
                    self.finish_loop();
                }
                Task::RepeatCond(id, saved, start, locals) => {
                    let Stmt::Repeat(_, cond) = self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    let reg = self.alloc(1)?;
                    self.tasks
                        .try_push(Task::RepeatEnd(cond, saved, start, locals, reg))?;
                    self.tasks.try_push(Task::BooleanNode(cond, reg))?;
                }
                Task::RepeatEnd(cond, saved, start, locals, reg) => {
                    let again = self.predicate_finish(cond, reg)?;
                    if self.function.locals[locals..].iter().any(|l| l.captured) {
                        self.close(saved)?;
                        self.debug_line(self.line);
                        let done = self.emit(Op::Jump, 0, 0, 0)?;
                        self.list_patch(again, self.function.code.len());
                        self.end_locals(locals);
                        self.close(saved)?;
                        self.debug_line(self.line);
                        self.emit(Op::Jump, 0, 0, start)?;
                        self.patch(done);
                    } else {
                        self.end_locals(locals);
                        self.list_patch(again, start as usize);
                    }
                    self.finish_loop();
                }
                Task::IfNext(id, index, saved, exits) => {
                    let Stmt::If(arms, other) = &self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    let arms = self.ast.arms(*arms);
                    if let Some((cond, _)) = arms.get(index) {
                        let reg = self.alloc(1)?;
                        self.tasks
                            .try_push(Task::IfCond(id, index, saved, exits, reg))?;
                        self.tasks.try_push(Task::BooleanNode(*cond, reg))?;
                    } else {
                        self.tasks.try_push(Task::IfEnd(exits))?;
                        if let Some(other) = other {
                            self.tasks.try_push(Task::Scope(self.ast.ids(*other)))?;
                        }
                    }
                }
                Task::IfCond(id, index, saved, exits, reg) => {
                    let Stmt::If(arms, _) = &self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    let arms = self.ast.arms(*arms);
                    let skip = self.predicate_finish(arms[index].0, reg)?;
                    self.reset(saved);
                    self.tasks
                        .try_push(Task::IfBody(id, index, saved, exits, skip))?;
                    self.tasks
                        .try_push(Task::Scope(self.ast.ids(arms[index].1)))?;
                }
                Task::IfBody(id, index, saved, mut exits, skip) => {
                    let Stmt::If(arms, other) = &self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    if index + 1 < arms.len() || other.is_some() {
                        let exit = self.emit(Op::Jump, 0, 0, 0)?;
                        self.list_push(&mut exits, exit);
                    }
                    self.list_patch(skip, self.function.code.len());
                    self.tasks
                        .try_push(Task::IfNext(id, index + 1, saved, exits))?;
                }
                Task::IfEnd(exits) => {
                    self.list_patch(exits, self.function.code.len());
                }
                Task::ForLimit(id, saved, base) => {
                    let Stmt::For { limit, .. } = self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    self.tasks.try_push(Task::ForStep(id, saved, base))?;
                    self.tasks.try_push(Task::Expr(limit, base + 1))?;
                }
                Task::ForStep(id, saved, base) => {
                    let Stmt::For { step, .. } = self.ast.stmts[id].0 else {
                        unreachable!()
                    };
                    self.tasks.try_push(Task::ForInit(id, saved, base))?;
                    if let Some(step) = step {
                        self.tasks.try_push(Task::Expr(step, base + 2))?;
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
                        self.debug_local(name, base + i as u8)?;
                    }
                    self.line = *do_line;
                    let prep = self.emit(Op::ForPrep, base, 0, 0)?;
                    let head = self.function.code.len() as u32;
                    let locals = self.function.locals.len();
                    self.bind(name.bytes(self.source), base + 3)?;
                    self.function.loops.try_push((saved, Vec::new()))?;
                    self.tasks
                        .try_push(Task::ForEnd(id, base, controls, locals, prep, head))?;
                    self.tasks.try_push(Task::Scope(self.ast.ids(*body)))?;
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
                Task::FillNil(dst, wanted) => {
                    if wanted != OPEN {
                        let end = u16::from(dst) + wanted as u16;
                        if end > self.function.next {
                            self.alloc(end - self.function.next)?;
                        }
                        for i in 1..wanted {
                            self.load((u32::from(dst) + i) as u8, Constant::Nil)?;
                        }
                    }
                    self.result = dst;
                }
                Task::Operand(id, dst) => self.operand(id, dst)?,
                Task::Paren(inner, line) => self.discharge_line(inner, line, LineUse::Copy),
                Task::Unary(id, dst, work) => self.unary(id, dst, work)?,
                Task::BooleanNode(id, work) => self.boolean_node(id, work)?,
                Task::BooleanLeaf(id, work) => self.boolean_leaf(id, work),
                Task::BooleanNot(id, work) => self.boolean_not(id, work)?,
                Task::BooleanLeft(id, work) => self.boolean_left(id, work)?,
                Task::BooleanRight(op, list) => self.boolean_right(op, list),
                Task::BooleanFinish(id, dst) => self.boolean_finish(id, dst)?,
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
                    if op == Kind::Concat
                        && let Some(line) = self
                            .function
                            .code
                            .last()
                            .map(|i| i.hook)
                            .filter(|&l| l != 0)
                    {
                        // A copied concat operand is discharged at the operator;
                        // callbacks and hook inspection must see that location.
                        self.function.code.last_mut().expect("concat operand").line = line;
                    }
                    let right_reg =
                        self.right_register(right, left_reg, work, op == Kind::Concat)?;
                    self.tasks.try_push(Task::BinaryRight(id, dst, left_reg))?;
                    self.tasks.try_push(if op == Kind::Concat {
                        Task::Expr(right, right_reg)
                    } else {
                        Task::Operand(right, right_reg)
                    })?;
                }
                Task::BinaryRight(id, dst, left) => self.binary_right(id, dst, u16::from(left))?,
                Task::BinaryConstant(id, dst, work, constant) => {
                    self.binary_constant(id, dst, work, constant)?
                }
                Task::IndexTable(id, dst, work) => self.index_table(id, dst, work)?,
                Task::IndexKey(id, dst, table) => self.index_key(id, dst, table)?,
                Task::TableNext(state) => self.table_next(state)?,
                Task::TableKey(state, regs) => {
                    let Expr::Table(fields, _) = &self.ast.exprs[state.id].0 else {
                        unreachable!()
                    };
                    let fields = self.ast.fields(*fields);
                    let key = self.result;
                    let value = fields[state.index].1;
                    let value_reg = self.right_register(value, key, regs, false)?;
                    self.tasks.try_push(Task::TableValue(state, regs, key))?;
                    self.operand(value, value_reg)?;
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
                    self.table_next(state)?;
                }
                Task::TableConstant(mut state, scratch, constant) => {
                    let value = self.result;
                    let key = if value == scratch {
                        self.alloc(1)?
                    } else {
                        scratch
                    };
                    let key = if constant <= u32::from(u16::MAX) - RK {
                        // SetTable can encode this key directly. A temporary
                        // load would be removed by emit's operand folding.
                        (RK + constant) as u16
                    } else {
                        let line = self.line;
                        let Expr::Table(fields, _) = &self.ast.exprs[state.id].0 else {
                            unreachable!()
                        };
                        let fields = self.ast.fields(*fields);
                        self.line = self.ast.exprs[fields[state.index].0.expect("constant key")]
                            .1
                            .discharge;
                        self.emit(Op::Load, key, 0, constant)?;
                        self.function.debug_hidden[usize::from(key) / 64] |= 1u64 << (key % 64);
                        self.debug_line(0);
                        self.line = line;
                        u16::from(key)
                    };
                    self.emit(Op::SetTable, state.table, key, u32::from(value))?;
                    self.reset(u16::from(scratch));
                    state.index += 1;
                    self.table_next(state)?;
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
                    self.table_next(state)?;
                }
                Task::Call(id, dst, wanted) => self.call(id, dst, wanted)?,
                Task::CallCallee(state) => self.call_callee(state)?,
                Task::CallArgs(state) => self.call_args(state)?,
                Task::ChildEnd(line, dst, template, original_line) => {
                    self.line = if original_line == 0 {
                        line
                    } else {
                        self.function.defined[1]
                    };
                    self.emit(Op::Return, 0, 0, 0)?;
                    let child = self.finish()?;
                    if original_line != 0 {
                        self.prototypes[child as usize].relocate(line);
                    }
                    self.line = line;
                    if let Some(source) = template {
                        self.templates.remember(
                            source,
                            child,
                            &mut self.prototypes[child as usize],
                            self.source,
                            self.template_locale,
                        )?;
                    }
                    let parent = self.parents.pop().expect("child has a parent");
                    self.spare
                        .try_push(std::mem::replace(&mut self.function, parent))?;
                    self.emit(Op::Closure, dst, 0, child)?;
                }
            }
        }
        Ok(())
    }
}
