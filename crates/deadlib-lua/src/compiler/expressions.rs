//! Expression scheduling with register results passed through continuations.
use super::{
    tasks::{CallState, TableState},
    *,
};

impl<'a> Compiler<'a> {
    pub(super) fn expr(&mut self, id: ExprId, dst: u8) -> Result<(), Error> {
        let saved = self.function.next;
        let (expr, span) = &self.ast.exprs[id];
        self.line = span.discharge;
        if let Some(n) = self.folded[id] {
            self.load(dst, Constant::Number(n))?;
            self.result = dst;
            return Ok(());
        }
        self.tasks.push(Task::ExprEnd(saved, dst));
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
                self.tasks.push(Task::Paren(*inner, span.discharge));
                self.tasks.push(Task::Expr(*inner, dst));
            }
            Expr::Unary(op, inner) => {
                let work = self.work_register(dst)?;
                self.tasks.push(Task::Unary(*op, dst, work, span.end));
                self.tasks.push(Task::Expr(*inner, work));
            }
            Expr::Binary(Kind::And | Kind::Or, left, _, _) => {
                self.tasks.push(Task::LogicalLeft(id, dst));
                self.tasks.push(Task::Expr(*left, dst));
            }
            Expr::Binary(op, left, right, _) => {
                let work = self.work_register(dst)?;
                let constant = if matches!(
                    op,
                    Kind::Plus
                        | Kind::Minus
                        | Kind::Star
                        | Kind::Slash
                        | Kind::Percent
                        | Kind::Caret
                ) && let Some(number) = self.folded[*left]
                {
                    Some(self.constant(Constant::Number(number))?)
                } else if *op != Kind::Concat {
                    self.constant_operand(*left)?
                } else {
                    None
                };
                if let Some(constant) = constant
                    && !self.eager_numeric_tree(id)
                {
                    self.tasks
                        .push(Task::BinaryConstant(id, dst, work, constant));
                    self.tasks.push(Task::Operand(*right, work));
                    return Ok(());
                }
                self.tasks.push(Task::BinaryLeft(id, dst, work));
                self.tasks.push(if *op == Kind::Concat {
                    Task::Expr(*left, work)
                } else {
                    Task::Operand(*left, work)
                });
            }
            Expr::Index(table, _) => {
                let work = self.work_register(dst)?;
                self.tasks.push(Task::IndexTable(id, dst, work));
                self.tasks.push(Task::Operand(*table, work));
            }
            Expr::Table(_, load_line) => {
                let table = if self.top_temporary(dst) {
                    dst
                } else {
                    self.alloc(1)?
                };
                self.line = *load_line;
                self.emit(Op::NewTable, table, 0, 0)?;
                self.tasks.push(Task::TableNext(TableState {
                    id,
                    index: 0,
                    flushed: 0,
                    pending: 0,
                    table,
                    dst,
                }));
            }
            Expr::Call(..) | Expr::Method(..) => self.tasks.push(Task::Call(id, dst, 1)),
            Expr::Function {
                params,
                vararg,
                body,
                end_line,
            } => {
                let parent = std::mem::replace(
                    &mut self.function,
                    Function {
                        vararg: *vararg,
                        compat_arg: *vararg,
                        defined: [span.start, *end_line],
                        zero_sign: self
                            .ast
                            .zero_signs
                            .binary_search_by_key(&id, |&(id, _)| id)
                            .ok()
                            .map(|at| self.ast.zero_signs[at].1),
                        ..Function::default()
                    },
                );
                self.parents.push(parent);
                self.function.params = params.len() as u16;
                for name in params {
                    let reg = self.alloc(1)?;
                    self.bind(name, reg)?;
                }
                if *vararg {
                    let reg = self.alloc(1)?;
                    self.bind(b"arg", reg)?;
                }
                self.tasks.push(Task::ChildEnd(*end_line, dst));
                self.tasks.push(Task::Block(body));
            }
            Expr::Vararg => self.vararg(dst, 1)?,
        }
        Ok(())
    }
    pub(super) fn expr_results(&mut self, id: ExprId, dst: u8, wanted: u32) -> Result<(), Error> {
        self.line = self.ast.exprs[id].1.discharge;
        match &self.ast.exprs[id].0 {
            Expr::Call(..) | Expr::Method(..) => self.tasks.push(Task::Call(id, dst, wanted)),
            Expr::Vararg => self.vararg(dst, wanted)?,
            _ => {
                self.tasks.push(Task::FillNil(dst, wanted));
                self.tasks.push(Task::Expr(id, dst));
            }
        }
        Ok(())
    }
    fn vararg(&mut self, dst: u8, wanted: u32) -> Result<(), Error> {
        if !self.function.vararg {
            return Err(self.error(
                ErrorKind::Syntax,
                "cannot use '...' outside a vararg function",
            ));
        }
        self.function.compat_arg = false;
        let end = u16::from(dst) + if wanted == OPEN { 1 } else { wanted as u16 };
        if end > self.function.next {
            self.alloc(end - self.function.next)?;
        }
        self.emit(Op::Vararg, dst, 0, wanted)?;
        self.result = dst;
        Ok(())
    }
    pub(super) fn operand(&mut self, id: ExprId, dst: u8) {
        let reference = self.unparen(id);
        if let Some(reg) = self.local_operand(reference) {
            self.line = self.ast.exprs[id].1.discharge;
            self.result = reg;
            return;
        }
        self.tasks.push(Task::OperandEnd(reference, dst));
        self.tasks.push(Task::Expr(id, dst));
    }
    fn unparen(&self, mut id: ExprId) -> ExprId {
        while let Expr::Paren(inner) = self.ast.exprs[id].0 {
            id = inner;
        }
        id
    }
    pub(super) fn local_operand(&self, id: ExprId) -> Option<u8> {
        if let Expr::Name(name) = &self.ast.exprs[self.unparen(id)].0 {
            self.local(name)
        } else {
            None
        }
    }
    fn work_register(&mut self, dst: u8) -> Result<u8, Error> {
        // A named destination must keep its previous value until the complete
        // expression finishes: callbacks and metamethods can inspect it.
        if self.function.debug_named[usize::from(dst) / 64] & (1u64 << (dst % 64)) != 0 {
            self.alloc(1)
        } else {
            Ok(dst)
        }
    }
    fn top_temporary(&self, dst: u8) -> bool {
        u16::from(dst) + 1 == self.function.next
            && self.function.debug_named[usize::from(dst) / 64] & (1u64 << (dst % 64)) == 0
    }
    fn eager_numeric_tree(&self, id: ExprId) -> bool {
        let cost = u16::from(self.ast.exprs[id].1.scalar_cost);
        cost != 0 && cost <= 256 - self.function.next
    }
    pub(super) fn right_register(
        &mut self,
        right: ExprId,
        left: u8,
        work: u8,
        copy: bool,
    ) -> Result<u8, Error> {
        if left == work && (copy || self.local_operand(right).is_none()) {
            self.alloc(1)
        } else {
            Ok(work)
        }
    }
    pub(super) fn logical_left(&mut self, id: ExprId, dst: u8) -> Result<(), Error> {
        let Expr::Binary(op, left, right, op_line) = self.ast.exprs[id].0 else {
            unreachable!()
        };
        self.discharge_line(left, op_line, LineUse::Condition);
        self.line = op_line;
        let and = op == Kind::And;
        let mut first = left;
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
            self.debug_line(0);
        }
        let done = self.emit(if and { Op::JumpFalse } else { Op::JumpTrue }, dst, 0, 0)?;
        if omitted
            || matches!(
                self.ast.exprs[first].0,
                Expr::Binary(
                    Kind::Eq | Kind::Ne | Kind::Lt | Kind::Le | Kind::Gt | Kind::Ge,
                    ..
                )
            )
        {
            self.debug_line(0);
        }
        self.function.debug_hidden[usize::from(dst) / 64] |= 1u64 << (dst % 64);
        self.tasks.push(Task::LogicalEnd(
            right,
            self.ast.exprs[id].1.discharge,
            done,
        ));
        self.tasks.push(Task::Expr(right, dst));
        Ok(())
    }
    pub(super) fn binary_right(&mut self, id: ExprId, dst: u8, left: u8) -> Result<(), Error> {
        let Expr::Binary(op, ..) = self.ast.exprs[id].0 else {
            unreachable!()
        };
        let right = self.result;
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
            _ => unreachable!("binary operator"),
        };
        if matches!(op, Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge) {
            let boolean = self.alloc(1)?;
            self.emit(op, boolean, u16::from(left), u32::from(right))?;
            self.function.live[usize::from(boolean) / 64] &= !(1u64 << (boolean % 64));
            self.line = self.ast.exprs[id].1.discharge;
            self.emit(Op::Bool, dst, u16::from(boolean), 0)?;
        } else {
            self.emit(op, dst, u16::from(left), u32::from(right))?;
        }
        Ok(())
    }
    pub(super) fn binary_constant(
        &mut self,
        id: ExprId,
        dst: u8,
        work: u8,
        constant: u32,
    ) -> Result<(), Error> {
        let Expr::Binary(_, left, _, _) = self.ast.exprs[id].0 else {
            unreachable!("constant left operand")
        };
        let right = self.result;
        let reg = if right == work { self.alloc(1)? } else { work };
        self.line = self.ast.exprs[left].1.discharge;
        self.emit(Op::Load, reg, 0, constant)?;
        self.function.debug_hidden[usize::from(reg) / 64] |= 1u64 << (reg % 64);
        self.debug_line(0);
        self.binary_right(id, dst, reg)
    }
    pub(super) fn table_next(&mut self, state: TableState) -> Result<(), Error> {
        let Expr::Table(fields, _) = &self.ast.exprs[state.id].0 else {
            unreachable!()
        };
        if let Some(&(key, value)) = fields.get(state.index) {
            if let Some(key) = key {
                if let Some(constant) = self.constant_operand(key)? {
                    let reg = self.alloc(1)?;
                    self.tasks.push(Task::TableConstant(state, reg, constant));
                    self.tasks.push(Task::Operand(value, reg));
                    return Ok(());
                }
                let regs = self.alloc(1)?;
                self.tasks.push(Task::TableKey(state, regs));
                self.tasks.push(Task::Operand(key, regs));
            } else {
                let reg = self.alloc(1)?;
                let open = state.index + 1 == fields.len() && self.multiple(value);
                self.tasks.push(Task::TableList(state, open));
                self.tasks
                    .push(Task::Results(value, reg, if open { OPEN } else { 1 }));
            }
        } else {
            if state.pending != 0 {
                self.line = self.ast.exprs[state.id].1.end;
                self.emit(Op::SetList, state.table, state.pending, state.flushed + 1)?;
            }
            if state.dst != state.table {
                self.emit(Op::Move, state.dst, u16::from(state.table), 0)?;
            }
        }
        Ok(())
    }
    pub(super) fn constant_operand(&mut self, id: ExprId) -> Result<Option<u32>, Error> {
        let Ok(index) = self
            .ast
            .constant_operands
            .binary_search_by_key(&id, |&(id, _)| id)
        else {
            return Ok(None);
        };
        let constant = match self.ast.constant_operands[index].1 {
            crate::parser::ConstantOperand::Nil => Constant::Nil,
            crate::parser::ConstantOperand::Boolean(value) => Constant::Boolean(value),
            crate::parser::ConstantOperand::Number(value) => Constant::Number(value),
            crate::parser::ConstantOperand::String(source) => {
                let Expr::String(bytes) = &self.ast.exprs[source].0 else {
                    unreachable!("constant key literal")
                };
                Constant::String(bytes.clone())
            }
        };
        self.constant(constant).map(Some)
    }
    pub(super) fn call_parts(&self, id: ExprId) -> (ExprId, Option<&'a [u8]>, &'a [ExprId], u32) {
        match &self.ast.exprs[id].0 {
            Expr::Call(function, args, line) => (*function, None, args, *line),
            Expr::Method(receiver, method, args, line) => (*receiver, Some(method), args, *line),
            _ => unreachable!("call continuation"),
        }
    }
    pub(super) fn call(&mut self, id: ExprId, dst: u8, wanted: u32) -> Result<(), Error> {
        let (function, _, _, line) = self.call_parts(id);
        self.line = line;
        let saved = self.function.next;
        let base = if self.top_temporary(dst) {
            dst
        } else {
            self.alloc(1)?
        };
        self.tasks.push(Task::CallCallee(CallState {
            id,
            wanted,
            saved,
            index: 0,
            base,
            dst,
        }));
        self.tasks.push(Task::Expr(function, base));
        Ok(())
    }
    pub(super) fn call_end(&mut self, state: CallState) -> Result<(), Error> {
        let CallState {
            id,
            wanted,
            saved,
            base,
            dst,
            ..
        } = state;
        let (function, method, args, line) = self.call_parts(id);
        self.line = line;
        let argc = if args.last().is_some_and(|id| self.multiple(*id)) {
            OPEN_ARGS
        } else {
            (args.len() + usize::from(method.is_some())) as u16
        };
        if wanted == TAIL {
            self.record_call(function, method, base)?;
            self.emit(Op::TailCall, base, argc, 0)?;
            self.emit(Op::Return, base, 0, OPEN)?;
        } else if wanted == OPEN {
            self.record_call(function, method, base)?;
            self.emit(Op::Call, base, argc, OPEN)?;
            if dst != base {
                self.emit(Op::Move, dst, u16::from(base), OPEN)?;
            }
        } else {
            let needed = u16::from(base) + wanted as u16;
            if needed > self.function.next {
                self.alloc(needed - self.function.next)?;
            }
            self.record_call(function, method, base)?;
            self.emit(Op::Call, base, argc, wanted)?;
            for i in 0..if dst != base { wanted } else { 0 } {
                self.emit(
                    Op::Move,
                    (u32::from(dst) + i) as u8,
                    u16::from(base) + i as u16,
                    0,
                )?;
            }
        }
        self.reset(if wanted < TAIL {
            saved.max(u16::from(dst) + wanted as u16)
        } else {
            saved
        });
        self.result = dst;
        Ok(())
    }
}
