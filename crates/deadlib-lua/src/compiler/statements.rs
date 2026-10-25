//! Statement scheduling and register allocation.
use super::*;

impl<'a> Compiler<'a> {
    pub(super) fn statement(&mut self, id: StmtId) -> Result<(), Error> {
        let (stmt, line) = &self.ast.stmts[id];
        self.line = *line;
        let saved = self.function.next;
        if !matches!(stmt, Stmt::Local(..) | Stmt::LocalFunction(..)) {
            self.tasks.push(Task::Reset(saved));
        }
        match stmt {
            Stmt::Local(names, values) => {
                self.tasks.push(Task::BindLocal(id));
                self.tasks.push(Task::Values(values, names.len()));
            }
            Stmt::Assign(targets, _) | Stmt::FunctionDefinition(targets, _) => {
                self.tasks
                    .push(Task::AssignNext(id, 0, Vec::with_capacity(targets.len())));
            }
            Stmt::Call(value) => {
                let dst = self.alloc(1)?;
                self.tasks.push(Task::Results(*value, dst, 0));
            }
            Stmt::Return(values, end_line) => {
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
                        self.tasks.push(Task::TailEnd(*end_line));
                        self.tasks.push(Task::Call(values[0], dst, TAIL));
                        return Ok(());
                    }
                }
                let base = self.alloc(1)?;
                self.tasks
                    .push(Task::ReturnNext(id, base, 0, values.len() as u32));
            }
            Stmt::Do(body) => self.tasks.push(Task::Scope(body)),
            Stmt::While(cond, _) => {
                let start = self.function.code.len() as u32;
                let reg = self.alloc(1)?;
                self.tasks.push(Task::WhileCond(id, saved, start, reg));
                self.tasks.push(Task::Expr(*cond, reg));
            }
            Stmt::Repeat(body, _) => {
                let start = self.function.code.len() as u32;
                let locals = self.function.locals.len();
                self.function.loops.push((saved, Vec::new()));
                self.tasks.push(Task::RepeatCond(id, saved, start, locals));
                self.tasks.push(Task::Block(body));
            }
            Stmt::If(arms, _) => {
                self.tasks
                    .push(Task::IfNext(id, 0, saved, Vec::with_capacity(arms.len())))
            }
            Stmt::For { start, .. } => {
                let base = self.alloc(4)?;
                self.tasks.push(Task::ForLimit(id, saved, base));
                self.tasks.push(Task::Expr(*start, base));
            }
            Stmt::Break => {
                let Some((next, _)) = self.function.loops.last() else {
                    return Err(self.error(ErrorKind::Syntax, "no loop to break"));
                };
                self.close(*next)?;
                let pc = self.emit(Op::Jump, 0, 0, 0)?;
                self.function.loops.last_mut().unwrap().1.push(pc);
            }
            Stmt::LocalFunction(name, value) => {
                let reg = self.alloc(1)?;
                self.bind(name, reg)?;
                self.tasks.push(Task::LocalFunctionEnd);
                self.tasks.push(Task::Expr(*value, reg));
            }
            Stmt::ForIn(_, values, _, _) => {
                self.tasks.push(Task::ForInInit(id, saved));
                self.tasks.push(Task::Values(values, 3));
            }
        }
        Ok(())
    }

    pub(super) fn assignment(&self, id: StmtId) -> (&'a [ExprId], &'a [ExprId]) {
        match &self.ast.stmts[id].0 {
            Stmt::Assign(targets, values) | Stmt::FunctionDefinition(targets, values) => {
                (targets, values)
            }
            _ => unreachable!("assignment continuation"),
        }
    }
    pub(super) fn assign_next(
        &mut self,
        id: StmtId,
        mut index: usize,
        mut locations: Vec<Target>,
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
                        Target::Global(self.constant(Constant::String(name.clone()))?)
                    }
                }
                Expr::Index(table, _) => {
                    if let Some(table) = self.local_operand(*table) {
                        self.assign_key(id, index, locations, table)?;
                    } else {
                        let reg = self.alloc(1)?;
                        self.tasks.push(Task::AssignKey(id, index, locations));
                        self.tasks.push(Task::Operand(*table, reg));
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
                for target in &mut locations {
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
            locations.push(target);
            index += 1;
        }
        if let ([Target::Local(dst)], [value]) = (locations.as_slice(), values) {
            let mut expression = *value;
            while let Expr::Paren(inner) = self.ast.exprs[expression].0 {
                expression = inner;
            }
            if !matches!(
                self.ast.exprs[expression].0,
                Expr::Table(..) | Expr::Binary(Kind::And | Kind::Or, ..) | Expr::Vararg
            ) {
                self.tasks.push(Task::Results(*value, *dst, 1));
                return Ok(());
            }
        }
        self.tasks.push(Task::AssignEnd(id, locations));
        self.tasks.push(Task::Values(values, targets.len()));
        Ok(())
    }

    pub(super) fn assign_key(
        &mut self,
        id: StmtId,
        index: usize,
        mut locations: Vec<Target>,
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
                .push(Task::AssignIndex(id, index, locations, table));
            self.tasks.push(Task::Operand(key, reg));
            return Ok(());
        };
        locations.push(Target::Index(table, key));
        self.tasks.push(Task::AssignNext(id, index + 1, locations));
        Ok(())
    }

    pub(super) fn for_in(&mut self, id: StmtId, saved: u16) -> Result<(), Error> {
        let Stmt::ForIn(names, _, body, lines) = &self.ast.stmts[id].0 else {
            unreachable!()
        };
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
        self.tasks
            .push(Task::ForInEnd(results, controls, locals, head, done));
        self.tasks.push(Task::Scope(body));
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
