//! Statement scheduling and register allocation.
use super::*;

impl<'a> Compiler<'a> {
    pub(super) fn statement(&mut self, id: StmtId) -> Result<(), Error> {
        let (stmt, line) = &self.ast.stmts[id];
        self.line = *line;
        let saved = self.function.next;
        if !matches!(stmt, Stmt::Local(..) | Stmt::LocalFunction(..)) {
            self.tasks.try_push(Task::Reset(saved))?;
        }
        match stmt {
            Stmt::Local(names, values) => {
                self.tasks.try_push(Task::BindLocal(id))?;
                self.tasks
                    .try_push(Task::Values(self.ast.ids(*values), names.len()))?;
            }
            Stmt::Assign(..) | Stmt::FunctionDefinition(..) => {
                self.tasks.try_push(Task::AssignNext(
                    id,
                    0,
                    tasks::AssignmentTargets::default(),
                ))?;
            }
            Stmt::Call(value) => {
                let dst = self.alloc(1)?;
                self.tasks.try_push(Task::Results(*value, dst, 0))?;
            }
            Stmt::Return(values, end_line) => {
                let values = self.ast.ids(*values);
                if values.is_empty() {
                    self.line = *end_line;
                    self.emit(Op::Return, 0, 0, 0)?;
                    return Ok(());
                }
                if values.len() == 1 {
                    let mut value = values[0];
                    while let Expr::Paren(inner) = self.ast.exprs[value].0 {
                        value = inner;
                    }
                    if let Expr::Name(name) = &self.ast.exprs[value].0
                        && let Some(register) = self.local(name)
                    {
                        self.line = *end_line;
                        self.emit(Op::Return, register, 0, 1)?;
                        return Ok(());
                    }
                    if matches!(
                        self.ast.exprs[values[0]].0,
                        Expr::Call(..) | Expr::Method(..)
                    ) {
                        let dst = self.alloc(1)?;
                        self.tasks.try_push(Task::TailEnd(*end_line))?;
                        self.tasks.try_push(Task::Call(values[0], dst, TAIL))?;
                        return Ok(());
                    }
                }
                let base = self.alloc(1)?;
                self.tasks
                    .try_push(Task::ReturnNext(id, base, 0, values.len() as u32))?;
            }
            Stmt::Do(body) => self.tasks.try_push(Task::Scope(self.ast.ids(*body)))?,
            Stmt::While(cond, _) => {
                let start = self.function.code.len() as u32;
                let reg = self.alloc(1)?;
                self.tasks
                    .try_push(Task::WhileCond(id, saved, start, reg))?;
                self.tasks.try_push(Task::BooleanNode(*cond, reg))?;
            }
            Stmt::Repeat(body, _) => {
                let start = self.function.code.len() as u32;
                let locals = self.function.locals.len();
                self.function.loops.try_push((saved, Vec::new()))?;
                self.tasks
                    .try_push(Task::RepeatCond(id, saved, start, locals))?;
                self.tasks.try_push(Task::Block(self.ast.ids(*body)))?;
            }
            Stmt::If(arms, _) => self.tasks.try_push(Task::IfNext(
                id,
                0,
                saved,
                fallible::with_capacity(arms.len())?,
            ))?,
            Stmt::For { start, .. } => {
                let base = self.alloc(4)?;
                self.tasks.try_push(Task::ForLimit(id, saved, base))?;
                self.tasks.try_push(Task::Expr(*start, base))?;
            }
            Stmt::Break => {
                let Some((next, _)) = self.function.loops.last() else {
                    return Err(self.error(ErrorKind::Syntax, "no loop to break"));
                };
                let next = *next;
                if self
                    .function
                    .locals
                    .iter()
                    .any(|l| u16::from(l.reg) >= next && l.captured)
                {
                    self.close(next)?;
                    self.debug_line(self.line);
                }
                let pc = self.emit(Op::Jump, 0, 0, 0)?;
                self.function.loops.last_mut().unwrap().1.try_push(pc)?;
            }
            Stmt::LocalFunction(name, value) => {
                let reg = self.alloc(1)?;
                self.bind(name, reg)?;
                self.tasks.try_push(Task::LocalFunctionEnd)?;
                self.tasks.try_push(Task::Expr(*value, reg))?;
            }
            Stmt::ForIn(_, values, _, _) => {
                self.tasks.try_push(Task::ForInInit(id, saved))?;
                self.tasks
                    .try_push(Task::Values(self.ast.ids(*values), 3))?;
            }
        }
        Ok(())
    }

    pub(super) fn assignment(&self, id: StmtId) -> (&'a [ExprId], &'a [ExprId]) {
        let ast = self.ast;
        match &ast.stmts[id].0 {
            Stmt::Assign(targets, values) | Stmt::FunctionDefinition(targets, values) => {
                (ast.ids(*targets), ast.ids(*values))
            }
            _ => unreachable!("assignment continuation"),
        }
    }
    pub(super) fn assign_next(
        &mut self,
        id: StmtId,
        mut index: usize,
        mut locations: tasks::AssignmentTargets,
    ) -> Result<(), Error> {
        let (targets, values) = self.assignment(id);
        while let Some(&target) = targets.get(index) {
            let target = match &self.ast.exprs[target].0 {
                Expr::Name(name) => {
                    if let Some(reg) = self.local(name) {
                        Target::Local(reg)
                    } else if let Some(up) = self.upvalue(name)? {
                        Target::Upvalue(up)
                    } else {
                        Target::Global(self.string_constant(name)?)
                    }
                }
                Expr::Index(table, _) => {
                    if let Some(table) = self.local_operand(*table) {
                        self.assign_key(id, index, locations, table)?;
                    } else {
                        let reg = self.alloc(1)?;
                        self.tasks.try_push(Task::AssignKey(id, index, locations))?;
                        self.tasks.try_push(Task::Operand(*table, reg))?;
                    }
                    return Ok(());
                }
                _ => return Err(self.error(ErrorKind::Syntax, "invalid assignment target")),
            };
            if let Target::Local(reg) = target
                && locations
                    .iter()
                    .any(|t| matches!(t, Target::Index(table, key) if *table == reg || matches!(key, TargetKey::Register(key) if *key == reg)))
            {
                let saved = self.alloc(1)?;
                self.emit(Op::Move, saved, u16::from(reg), 0)?;
                for target in locations.iter_mut() {
                    if let Target::Index(table, key) = target {
                        if *table == reg {
                            *table = saved;
                        }
                        if matches!(key, TargetKey::Register(key) if *key == reg) {
                            *key = TargetKey::Register(saved);
                        }
                    }
                }
            }
            locations.try_push(target, targets.len())?;
            index += 1;
        }
        if let ([Target::Local(dst)], [value]) = (&locations[..], values) {
            let mut expression = *value;
            while let Expr::Paren(inner) = self.ast.exprs[expression].0 {
                expression = inner;
            }
            if !matches!(
                self.ast.exprs[expression].0,
                Expr::Table(..) | Expr::Binary(Kind::And | Kind::Or, ..) | Expr::Vararg
            ) {
                self.tasks.try_push(Task::Results(*value, *dst, 1))?;
                return Ok(());
            }
        }
        if locations.len() == 1
            && !matches!(locations[0], Target::Local(_))
            && let [value] = values
            && let Some(register) = self.local_operand(*value)
        {
            self.result = register;
            self.tasks.try_push(Task::AssignEnd(id, locations))?;
            return Ok(());
        }
        self.tasks.try_push(Task::AssignEnd(id, locations))?;
        self.tasks.try_push(Task::Values(values, targets.len()))?;
        Ok(())
    }

    pub(super) fn assign_key(
        &mut self,
        id: StmtId,
        index: usize,
        mut locations: tasks::AssignmentTargets,
        table: u8,
    ) -> Result<(), Error> {
        let (targets, _) = self.assignment(id);
        let Expr::Index(_, key) = self.ast.exprs[targets[index]].0 else {
            unreachable!("indexed assignment key")
        };
        let key = if let Some(constant) = self.constant_operand(key)? {
            TargetKey::Constant(constant)
        } else if let Some(reg) = self.local_operand(key) {
            TargetKey::Register(reg)
        } else {
            let reg = self.alloc(1)?;
            self.tasks
                .try_push(Task::AssignIndex(id, index, locations, table))?;
            self.tasks.try_push(Task::Operand(key, reg))?;
            return Ok(());
        };
        locations.try_push(Target::Index(table, key), targets.len())?;
        self.tasks
            .try_push(Task::AssignNext(id, index + 1, locations))?;
        Ok(())
    }

    pub(super) fn for_in(&mut self, id: StmtId, saved: u16) -> Result<(), Error> {
        let ast = self.ast;
        let Stmt::ForIn(names, _, body, lines) = &ast.stmts[id].0 else {
            unreachable!()
        };
        let names = ast.names(*names);
        let base = self.result;
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
            self.debug_local(name, base + i as u8)?;
        }
        let results = self.alloc(names.len().max(3) as u16)?;
        let test = self.alloc(1)?;
        self.function.debug_lines.try_push(lines[0])?;
        self.line = lines[1];
        let head = self.function.code.len() as u32;
        for i in 0..3 {
            self.emit(Op::Move, results + i, u16::from(base + i), 0)?;
        }
        self.function.debug_calls.try_push(DebugCall {
            pc: self.function.code.len() as u32,
            name: b"(for generator)".try_boxed()?,
            kind: NameKind::Local,
            temporaries: 0..0,
        })?;
        self.emit(Op::CallIter, results, 2, names.len() as u32)?;
        self.load(test, Constant::Nil)?;
        self.emit(Op::Eq, test, u16::from(results), u32::from(test))?;
        self.function.debug_hidden[usize::from(test) / 64] |= 1u64 << (test % 64);
        let done = self.emit(Op::JumpTrue, test, 0, 0)?;
        self.emit(Op::Move, base + 2, u16::from(results), 0)?;
        let locals = self.function.locals.len();
        for (i, name) in names.iter().enumerate() {
            self.bind(name, results + i as u8)?;
        }
        self.function.loops.try_push((saved, Vec::new()))?;
        self.tasks
            .try_push(Task::ForInEnd(results, controls, locals, head, done))?;
        self.tasks.try_push(Task::Scope(ast.ids(*body)))?;
        Ok(())
    }
    pub(super) fn end_controls(&mut self, controls: usize) {
        let end = self.function.code.len() as u32;
        for local in &mut self.function.debug_locals[controls..controls + 3] {
            local.end = end;
            self.function.debug_named[usize::from(local.register) / 64] &=
                !(1u64 << (local.register % 64));
        }
    }
}
