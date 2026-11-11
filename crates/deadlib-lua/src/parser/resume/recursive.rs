//! Recursive descent over complete source.
//!
//! Each grammar action, binding update and resource check runs in the order of
//! the resumable continuations, so both parsers build the same tree. Any
//! syntax, limit or nesting failure abandons the attempt: the caller restarts
//! the resumable parser, which reports the reference diagnostic.
use super::*;

/// Deeper syntax nesting uses the resumable parser, which needs no native stack.
const MAX_DEPTH: u16 = 48;

/// Position data of a consumed token; its payload stays in the staged slot.
#[derive(Clone, Copy)]
struct Taken {
    kind: Kind,
    line: u32,
    end_line: u32,
}

fn abandon() -> Error {
    Error::new_static(ErrorKind::Syntax, 0, "recursive parse abandoned")
}

impl Parser {
    /// Parse complete `input` from the parser's initial state.
    pub(super) fn recursive(&mut self, input: &[u8]) -> Result<Ast, Error> {
        self.rscan(input)?;
        self.token = Some(std::mem::take(&mut self.staged));
        let body = self.rblock(input)?;
        self.ast.end_line = self.previous_line;
        if self.kind() != Kind::Eof {
            return Err(abandon());
        }
        self.ast.body = body;
        self.resources.finish(&mut self.ast);
        Ok(std::mem::take(&mut self.ast))
    }

    /// Return to the initial state after an abandoned attempt.
    pub(super) fn restart(&mut self) -> Result<(), Error> {
        self.scanner = Scanner::default();
        self.token = None;
        self.lookahead = None;
        self.staged = Token::default();
        self.previous_line = 1;
        self.depth = 0;
        self.loops = 0;
        self.vararg = true;
        self.locals = 0;
        self.function_line = 0;
        self.bindings.reset()?;
        self.resources.clear();
        self.tasks.clear();
        self.tasks.try_push(Task::RootEnd)?;
        self.tasks.try_push(Task::Block)?;
        self.ast.clear();
        self.scratch.ids.clear();
        self.scratch.names.clear();
        self.scratch.fields.clear();
        self.scratch.arms.clear();
        self.expr = 0;
        self.stmt = 0;
        self.list = Span::default();
        self.body = Span::default();
        Ok(())
    }

    /// Scan one token into the staged slot, interning identifiers as the
    /// resumable scan does.
    fn rscan(&mut self, input: &[u8]) -> Result<(), Error> {
        self.scanner.next_closed(input, &mut self.staged)?;
        if self.staged.kind == Kind::Name {
            let (start, end) = self.scanner.name_span();
            self.bindings.intern(input, start, end)?;
        }
        Ok(())
    }
    /// Consume the current token and fetch its successor.
    #[inline]
    fn bump(&mut self, input: &[u8]) -> Result<Taken, Error> {
        if self.kind() == Kind::Eof {
            return Err(abandon());
        }
        match self.lookahead.take() {
            Some(next) => self.staged = next,
            None => self.rscan(input)?,
        }
        let current = self.token.as_mut().expect("current token");
        std::mem::swap(current, &mut self.staged);
        let taken = Taken {
            kind: self.staged.kind,
            line: self.staged.line,
            end_line: self.staged.end_line,
        };
        self.previous_line = taken.end_line;
        Ok(taken)
    }
    /// The text of the token `bump` just consumed.
    fn taken_text(&mut self) -> Text {
        std::mem::take(&mut self.staged.text)
    }
    /// Consume the current token and return its text.
    fn bump_text(&mut self, input: &[u8]) -> Result<Text, Error> {
        self.bump(input)?;
        Ok(self.taken_text())
    }
    /// Consume a name token and return its text.
    fn rname(&mut self, input: &[u8]) -> Result<Text, Error> {
        self.rexpect(input, Kind::Name)?;
        Ok(self.taken_text())
    }
    fn rexpect(&mut self, input: &[u8], kind: Kind) -> Result<Taken, Error> {
        if self.kind() != kind {
            return Err(abandon());
        }
        self.bump(input)
    }
    fn renter(&mut self) -> Result<(), Error> {
        if self.depth >= MAX_DEPTH || self.base_depth.wrapping_add(self.depth).wrapping_add(1) > 200
        {
            return Err(abandon());
        }
        self.depth += 1;
        Ok(())
    }
    fn rlocals(total: usize) -> Result<(), Error> {
        if total > 200 { Err(abandon()) } else { Ok(()) }
    }
    fn restore_locals(&mut self, count: u16) {
        self.locals = count;
        self.resources.locals(count);
        self.bindings.truncate(count);
    }
    fn rstatement_node(&mut self, stmt: Stmt, line: u32) -> Result<StmtId, Error> {
        self.statement(stmt, line)?;
        Ok(self.stmt)
    }

    fn rblock(&mut self, input: &[u8]) -> Result<Block, Error> {
        self.renter()?;
        let locals = self.locals;
        let body = self.rstatements(input)?;
        self.restore_locals(locals);
        Ok(body)
    }
    /// Statements up to a block terminator; the caller has entered a level.
    fn rstatements(&mut self, input: &[u8]) -> Result<Block, Error> {
        let mark = self.scratch.ids.len();
        while !self.end_block() {
            let terminal = matches!(self.kind(), Kind::Break | Kind::Return);
            let stmt = self.rstatement(input)?;
            self.resources.reset();
            self.scratch.ids.try_push(stmt)?;
            if self.kind() == Kind::Semi {
                self.bump(input)?;
            }
            if terminal {
                break;
            }
        }
        self.depth -= 1;
        self.publish_ids(mark)
    }

    fn rstatement(&mut self, input: &[u8]) -> Result<StmtId, Error> {
        let line = self.line();
        match self.kind() {
            Kind::Local => {
                self.bump(input)?;
                self.rlocal(input, line)
            }
            Kind::Function => {
                self.bump(input)?;
                self.rfunction_statement(input, line)
            }
            Kind::Return => {
                self.bump(input)?;
                let values = if self.end_block() || self.kind() == Kind::Semi {
                    Span::default()
                } else {
                    let values = self.rlist(input)?;
                    self.account(input, |r, cx| r.return_values(cx.ast.ids(values), cx))?;
                    values
                };
                self.rstatement_node(Stmt::Return(values, self.previous_line), line)
            }
            Kind::Break => {
                self.bump(input)?;
                if self.loops == 0 {
                    return Err(abandon());
                }
                self.rstatement_node(Stmt::Break, line)
            }
            Kind::Do => {
                self.bump(input)?;
                let body = self.rblock(input)?;
                self.rexpect(input, Kind::End)?;
                self.rstatement_node(Stmt::Do(body), line)
            }
            Kind::While => {
                self.bump(input)?;
                let cond = self.rexpr(input, 0)?;
                self.account(input, |r, cx| r.condition(cond, cx))?;
                self.rexpect(input, Kind::Do)?;
                self.loops += 1;
                let body = self.rblock(input)?;
                self.rexpect(input, Kind::End)?;
                self.loops -= 1;
                self.rstatement_node(Stmt::While(cond, body), line)
            }
            Kind::Repeat => {
                self.bump(input)?;
                self.loops += 1;
                let locals = self.locals;
                self.renter()?;
                let body = self.rstatements(input)?;
                self.rexpect(input, Kind::Until)?;
                let cond = self.rexpr(input, 0)?;
                self.account(input, |r, cx| r.condition(cond, cx))?;
                self.loops -= 1;
                let stmt = self.rstatement_node(Stmt::Repeat(body, cond), line)?;
                self.restore_locals(locals);
                Ok(stmt)
            }
            Kind::If => {
                self.bump(input)?;
                self.rif(input, line)
            }
            Kind::For => {
                self.bump(input)?;
                self.rfor(input, line)
            }
            _ => self.rexpression_statement(input, line),
        }
    }

    fn rif(&mut self, input: &[u8], line: u32) -> Result<StmtId, Error> {
        let mark = self.scratch.arms.len();
        loop {
            let cond = self.rexpr(input, 0)?;
            self.account(input, |r, cx| r.condition(cond, cx))?;
            self.rexpect(input, Kind::Then)?;
            let body = self.rblock(input)?;
            self.scratch.arms.try_push((cond, body))?;
            match self.kind() {
                Kind::ElseIf => {
                    self.bump(input)?;
                }
                Kind::Else => {
                    self.bump(input)?;
                    let other = self.rblock(input)?;
                    self.rexpect(input, Kind::End)?;
                    let arms = publish(&mut self.ast.arms, &mut self.scratch.arms, mark)?;
                    return self.rstatement_node(Stmt::If(arms, Some(other)), line);
                }
                _ => {
                    self.rexpect(input, Kind::End)?;
                    let arms = publish(&mut self.ast.arms, &mut self.scratch.arms, mark)?;
                    return self.rstatement_node(Stmt::If(arms, None), line);
                }
            }
        }
    }

    fn rfor(&mut self, input: &[u8], line: u32) -> Result<StmtId, Error> {
        let name = self.rname(input)?;
        if matches!(self.kind(), Kind::Comma | Kind::In) {
            return self.rfor_in(input, line, name);
        }
        if self.kind() != Kind::Assign {
            return Err(abandon());
        }
        Self::rlocals(usize::from(self.locals) + 4)?;
        self.bump(input)?;
        let start = self.rexpr(input, 0)?;
        self.account(input, |r, cx| r.next(start, cx))?;
        self.rexpect(input, Kind::Comma)?;
        let limit = self.rexpr(input, 0)?;
        self.account(input, |r, cx| r.next(limit, cx))?;
        let step = if self.kind() == Kind::Comma {
            self.bump(input)?;
            let step = self.rexpr(input, 0)?;
            self.account(input, |r, cx| r.next(step, cx))?;
            Some(step)
        } else {
            self.account(input, |r, cx| r.default_step(cx))?;
            None
        };
        let do_line = self.rexpect(input, Kind::Do)?.end_line;
        self.loops += 1;
        self.locals += 4;
        self.resources.locals(self.locals);
        self.bindings.hidden(3)?;
        self.bindings.declare(input, &name)?;
        let body = self.rblock(input)?;
        self.rexpect(input, Kind::End)?;
        self.loops -= 1;
        self.locals -= 4;
        self.resources.locals(self.locals);
        self.bindings.truncate(self.locals);
        self.rstatement_node(
            Stmt::For {
                name,
                start,
                limit,
                step,
                do_line,
                body,
            },
            line,
        )
    }

    fn rfor_in(&mut self, input: &[u8], line: u32, first: Text) -> Result<StmtId, Error> {
        let mark = self.scratch.names.len();
        self.scratch.names.try_push(first)?;
        loop {
            let count = self.scratch.names.len() - mark;
            Self::rlocals(usize::from(self.locals) + 3 + count)?;
            if self.kind() != Kind::Comma {
                self.rexpect(input, Kind::In)?;
                break;
            }
            self.bump(input)?;
            let name = self.rname(input)?;
            self.scratch.names.try_push(name)?;
        }
        let iter_line = self.current().end_line;
        let values = self.rlist(input)?;
        self.account(input, |r, cx| {
            r.adjust(3, cx.ast.ids(values), cx)?;
            r.check(3)
        })?;
        let do_line = self.rexpect(input, Kind::Do)?.end_line;
        self.loops += 1;
        let names = self.publish_names(mark)?;
        self.locals += names.len() as u16 + 3;
        self.resources.locals(self.locals);
        self.bindings.hidden(3)?;
        self.declare_names(input, names)?;
        let body = self.rblock(input)?;
        self.rexpect(input, Kind::End)?;
        self.loops -= 1;
        self.locals -= names.len() as u16 + 3;
        self.resources.locals(self.locals);
        self.bindings.truncate(self.locals);
        self.rstatement_node(Stmt::ForIn(names, values, body, [do_line, iter_line]), line)
    }

    fn rlocal(&mut self, input: &[u8], line: u32) -> Result<StmtId, Error> {
        if self.kind() == Kind::Function {
            self.bump(input)?;
            let name = self.rname(input)?;
            Self::rlocals(usize::from(self.locals) + 1)?;
            self.account(input, |r, _| r.reserve(1))?;
            self.locals += 1;
            self.resources.locals(self.locals);
            self.bindings.declare(input, &name)?;
            let function_line = self.current().end_line;
            let value = self.rfunction(input, function_line, false)?;
            return self.rstatement_node(Stmt::LocalFunction(name, value), line);
        }
        let mark = self.scratch.names.len();
        let name = self.rname(input)?;
        self.scratch.names.try_push(name)?;
        loop {
            let count = self.scratch.names.len() - mark;
            Self::rlocals(usize::from(self.locals) + count)?;
            if self.kind() != Kind::Comma {
                break;
            }
            self.bump(input)?;
            let name = self.rname(input)?;
            self.scratch.names.try_push(name)?;
        }
        let count = self.scratch.names.len() - mark;
        let values = if self.kind() == Kind::Assign {
            self.bump(input)?;
            let values = self.rlist(input)?;
            self.account(input, |r, cx| r.adjust(count, cx.ast.ids(values), cx))?;
            values
        } else {
            self.account(input, |r, cx| r.adjust(count, &[], cx))?;
            Span::default()
        };
        self.locals += count as u16;
        self.resources.locals(self.locals);
        let names = self.publish_names(mark)?;
        self.declare_names(input, names)?;
        self.rstatement_node(Stmt::Local(names, values), line)
    }

    fn rfunction_statement(&mut self, input: &[u8], line: u32) -> Result<StmtId, Error> {
        let name = self.rname(input)?;
        let mut target = self.expression(input, Expr::Name(name), line)?;
        self.rresolve(input, target)?;
        let mut method = false;
        while matches!(self.kind(), Kind::Dot | Kind::Colon) {
            self.account(input, |r, cx| r.any(target, cx))?;
            method = self.bump(input)?.kind == Kind::Colon;
            let field = self.rname(input)?;
            let key = self.expression(input, Expr::String(field), line)?;
            target = self.expression(input, Expr::Index(target, key), line)?;
            if method {
                break;
            }
        }
        let value = self.rfunction(input, line, method)?;
        self.account(input, |r, cx| r.assignment(&[target], &[value], cx))?;
        let targets = self.one(target)?;
        let values = self.one(value)?;
        self.rstatement_node(Stmt::FunctionDefinition(targets, values), line)
    }

    fn rexpression_statement(&mut self, input: &[u8], line: u32) -> Result<StmtId, Error> {
        let first = self.rprefix(input)?;
        if matches!(self.ast.exprs[first].0, Expr::Call(..) | Expr::Method(..)) {
            return self.rstatement_node(Stmt::Call(first), line);
        }
        let mark = self.scratch.ids.len();
        self.scratch.ids.try_push(first)?;
        loop {
            let last = *self.scratch.ids.last().expect("assignment target");
            if !matches!(self.ast.exprs[last].0, Expr::Name(_) | Expr::Index(..)) {
                return Err(abandon());
            }
            if self.kind() != Kind::Comma {
                break;
            }
            self.bump(input)?;
            let target = self.rprefix(input)?;
            // Taking the scratch stack moves no elements and lets the
            // accounting closure borrow the open target list.
            let open = std::mem::take(&mut self.scratch.ids);
            let conflict = self.account(input, |r, _| r.conflict(&open[mark..], target));
            self.scratch.ids = open;
            conflict?;
            let limit = 200 - i32::from(self.base_depth.wrapping_add(self.depth));
            if (self.scratch.ids.len() - mark) as i32 > limit {
                return Err(abandon());
            }
            self.scratch.ids.try_push(target)?;
        }
        self.rexpect(input, Kind::Assign)?;
        let values = self.rlist(input)?;
        let targets = self.publish_ids(mark)?;
        self.account(input, |r, cx| {
            r.assignment(cx.ast.ids(targets), cx.ast.ids(values), cx)
        })?;
        self.rstatement_node(Stmt::Assign(targets, values), line)
    }

    fn rlist(&mut self, input: &[u8]) -> Result<Span, Error> {
        let mark = self.scratch.ids.len();
        loop {
            let value = self.rexpr(input, 0)?;
            self.scratch.ids.try_push(value)?;
            if self.kind() != Kind::Comma {
                break;
            }
            let separator = self.bump(input)?;
            self.ast.exprs[value].1.discharge = separator.end_line;
            self.account(input, |r, cx| r.next(value, cx))?;
        }
        self.publish_ids(mark)
    }

    fn rexpr(&mut self, input: &[u8], min: u8) -> Result<ExprId, Error> {
        self.renter()?;
        let line = self.line();
        let mut left = match self.kind() {
            Kind::Not | Kind::Minus | Kind::Hash => {
                let op = self.bump(input)?.kind;
                let inner = self.rexpr(input, 7)?;
                self.expression(input, Expr::Unary(op, inner), line)?
            }
            Kind::Nil => {
                self.bump(input)?;
                self.expression(input, Expr::Nil, line)?
            }
            Kind::True | Kind::False => {
                let value = self.bump(input)?.kind == Kind::True;
                self.expression(input, Expr::Boolean(value), line)?
            }
            Kind::Number => {
                self.bump(input)?;
                let number = self.staged.number;
                self.expression(input, Expr::Number(number), line)?
            }
            Kind::String => {
                let text = self.bump_text(input)?;
                self.expression(input, Expr::String(text), line)?
            }
            Kind::Dots => {
                if !self.vararg {
                    return Err(abandon());
                }
                self.bump(input)?;
                self.expression(input, Expr::Vararg, line)?
            }
            Kind::Function => {
                self.bump(input)?;
                self.rfunction(input, line, false)?
            }
            Kind::LBrace => self.rtable(input)?,
            _ => self.rprefix(input)?,
        };
        while let Some((left_power, right_power)) = precedence(self.kind())
            && left_power > min
        {
            let token = self.bump(input)?;
            self.account(input, |r, cx| r.infix(left, token.kind, cx))?;
            let right = self.rexpr(input, right_power)?;
            left = self.expression(
                input,
                Expr::Binary(token.kind, left, right, token.end_line),
                line,
            )?;
        }
        self.depth -= 1;
        Ok(left)
    }

    fn rresolve(&mut self, input: &[u8], id: ExprId) -> Result<(), Error> {
        let Expr::Name(name) = &self.ast.exprs[id].0 else {
            unreachable!("resolved name")
        };
        let reference = self.bindings.reference(input, name)?;
        self.account(input, |r, cx| r.resolve(id, reference, cx))
    }

    fn rprefix(&mut self, input: &[u8]) -> Result<ExprId, Error> {
        let line = self.line();
        let mut id = match self.kind() {
            Kind::Name => {
                let name = self.bump_text(input)?;
                let id = self.expression(input, Expr::Name(name), line)?;
                self.rresolve(input, id)?;
                id
            }
            Kind::LParen => {
                self.bump(input)?;
                let inner = self.rexpr(input, 0)?;
                self.rexpect(input, Kind::RParen)?;
                self.expression(input, Expr::Paren(inner), line)?
            }
            _ => return Err(abandon()),
        };
        loop {
            match self.kind() {
                Kind::Dot => {
                    self.account(input, |r, cx| r.any(id, cx))?;
                    self.bump(input)?;
                    let name = self.rname(input)?;
                    let key = self.expression(input, Expr::String(name), line)?;
                    id = self.expression(input, Expr::Index(id, key), line)?;
                }
                Kind::LBracket => {
                    self.account(input, |r, cx| r.any(id, cx))?;
                    self.bump(input)?;
                    let key = self.rexpr(input, 0)?;
                    self.account(input, |r, cx| r.val(key, cx))?;
                    self.rexpect(input, Kind::RBracket)?;
                    id = self.expression(input, Expr::Index(id, key), line)?;
                }
                Kind::Colon => {
                    self.bump(input)?;
                    let name = self.rname(input)?;
                    self.account(input, |r, cx| r.method(id, &name, cx))?;
                    let call_line = self.current().end_line;
                    let args = self.rargs(input)?;
                    id = self.expression(input, Expr::Method(id, name, args, call_line), line)?;
                }
                Kind::LParen | Kind::LBrace | Kind::String => {
                    self.account(input, |r, cx| r.next(id, cx))?;
                    let call_line = self.current().end_line;
                    let args = self.rargs(input)?;
                    id = self.expression(input, Expr::Call(id, args, call_line), line)?;
                }
                _ => return Ok(id),
            }
        }
    }

    fn rargs(&mut self, input: &[u8]) -> Result<Span, Error> {
        match self.kind() {
            Kind::LParen => {
                if self.line() != self.previous_line {
                    return Err(abandon());
                }
                self.bump(input)?;
                if self.kind() == Kind::RParen {
                    self.bump(input)?;
                    return Ok(Span::default());
                }
                let values = self.rlist(input)?;
                self.account(input, |r, cx| r.arguments(cx.ast.ids(values)))?;
                self.rexpect(input, Kind::RParen)?;
                Ok(values)
            }
            Kind::LBrace => {
                let table = self.rtable(input)?;
                self.one(table)
            }
            Kind::String => {
                let token = self.bump(input)?;
                let text = self.taken_text();
                let id = self.expression(input, Expr::String(text), token.line)?;
                self.one(id)
            }
            _ => Err(abandon()),
        }
    }

    fn discharge_last_field(&mut self, mark: usize, line: u32) {
        if let Some((None, value)) = self.last_field(mark) {
            self.ast.exprs[value].1.discharge = line;
        }
    }

    fn rtable(&mut self, input: &[u8]) -> Result<ExprId, Error> {
        self.account(input, |r, _| r.begin_table())?;
        let previous = self.previous_line;
        let line = self.rexpect(input, Kind::LBrace)?.line;
        let mark = self.scratch.fields.len();
        loop {
            if self.kind() != Kind::RBrace {
                self.account(input, |r, cx| r.prepare_field(cx))?;
            }
            if self.kind() == Kind::Name && self.lookahead.is_none() {
                self.rscan(input)?;
                self.lookahead = Some(std::mem::take(&mut self.staged));
            }
            if self.kind() == Kind::RBrace {
                let end_line = self.current().end_line;
                self.discharge_last_field(mark, end_line);
                self.bump(input)?;
                break;
            }
            let key = if self.kind() == Kind::Name
                && self
                    .lookahead
                    .as_ref()
                    .is_some_and(|t| t.kind == Kind::Assign)
            {
                let token = self.bump(input)?;
                let text = self.taken_text();
                let key = self.expression(input, Expr::String(text), token.line)?;
                self.rexpect(input, Kind::Assign)?;
                self.account(input, |r, cx| r.record_key(key, cx))?;
                Some(key)
            } else if self.kind() == Kind::LBracket {
                self.bump(input)?;
                let key = self.rexpr(input, 0)?;
                self.account(input, |r, cx| r.val(key, cx))?;
                self.rexpect(input, Kind::RBracket)?;
                self.rexpect(input, Kind::Assign)?;
                self.account(input, |r, cx| r.record_key(key, cx))?;
                Some(key)
            } else {
                None
            };
            let value = self.rexpr(input, 0)?;
            self.account(input, |r, cx| r.field(key, value, cx))?;
            self.scratch.fields.try_push((key, value))?;
            if matches!(self.kind(), Kind::Comma | Kind::Semi) {
                let separator = self.bump(input)?;
                self.discharge_last_field(mark, separator.end_line);
            } else {
                let end_line = self.current().end_line;
                self.discharge_last_field(mark, end_line);
                self.rexpect(input, Kind::RBrace)?;
                break;
            }
        }
        let fields = publish(&mut self.ast.fields, &mut self.scratch.fields, mark)?;
        self.expression(input, Expr::Table(fields, previous), line)
    }

    /// Parse a parameter list and body after `function`.
    fn rfunction(&mut self, input: &[u8], line: u32, method: bool) -> Result<ExprId, Error> {
        self.rexpect(input, Kind::LParen)?;
        let mark = self.scratch.names.len();
        if method {
            self.scratch.names.try_push(Text::new(b"self")?)?;
        }
        let mut allow_end = true;
        let vararg = loop {
            if self.kind() == Kind::Dots {
                self.bump(input)?;
                break true;
            }
            if self.kind() == Kind::RParen && allow_end {
                break false;
            }
            if self.kind() != Kind::Name {
                return Err(abandon());
            }
            let name = self.bump_text(input)?;
            self.scratch.names.try_push(name)?;
            Self::rlocals(self.scratch.names.len() - mark)?;
            if self.kind() != Kind::Comma {
                break false;
            }
            self.bump(input)?;
            allow_end = false;
        };
        let count = self.scratch.names.len() - mark + usize::from(vararg);
        Self::rlocals(count)?;
        self.rexpect(input, Kind::RParen)?;
        let params = self.publish_names(mark)?;
        self.bindings
            .enter(input, line, self.ast.names(params), vararg)?;
        let parent = (self.loops, self.vararg, self.locals, self.function_line);
        self.function_line = line;
        self.loops = 0;
        self.vararg = vararg;
        self.locals = count as u16;
        self.resources.enter(self.locals)?;
        let body = self.rblock(input)?;
        let end_line = self.rexpect(input, Kind::End)?.end_line;
        (self.loops, self.vararg, self.locals, self.function_line) = parent;
        self.bindings.leave();
        let (sign, stack) = self.resources.leave();
        self.ast.stacks.try_push((self.ast.exprs.len(), stack))?;
        if let Some(sign) = sign {
            self.ast.zero_signs.try_push((self.ast.exprs.len(), sign))?;
        }
        self.expression(
            input,
            Expr::Function {
                params,
                vararg,
                body,
                end_line,
            },
            line,
        )
    }
}
