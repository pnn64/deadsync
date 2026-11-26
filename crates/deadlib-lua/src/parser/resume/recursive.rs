//! Recursive descent over complete source.
//!
//! Each grammar action, binding update and resource check runs in the order of
//! the resumable continuations, so both parsers build the same tree. Any
//! syntax, limit or nesting failure abandons the attempt: the caller restarts
//! the resumable parser, which reports the reference diagnostic.
use super::*;

// Larger chunks can amortize the lookup cost across repeated short bodies.
const MIN_TEMPLATE_SOURCE: usize = 2048;
const LEAF_SLOTS: usize = 32;

/// Position data of a consumed token.
#[derive(Clone, Copy)]
struct Taken {
    kind: Kind,
    line: u32,
    end_line: u32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ParsedLeaf {
    id: ExprId,
    offsets: [u32; 2],
    depth: u16,
    stack: u16,
    zero_sign: Option<bool>,
}

fn leaf_slot(input: &[u8], start: usize) -> Option<usize> {
    Some(crate::hash::hash_bytes(input.get(start..start + 24)?) as usize % LEAF_SLOTS)
}

fn abandon() -> Error {
    Error::new_static(ErrorKind::Syntax, 0, "recursive parse abandoned")
}

impl Parser {
    // Complete source restarts through the streaming grammar after a syntax
    // failure and has no native reader roots to publish while parsing.
    fn raccount<T>(
        &mut self,
        input: &[u8],
        action: impl FnOnce(&mut Resources, &Context<'_>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.account_with::<false, T>(input, action)
    }

    /// Parse complete `input` from the parser's initial state.
    pub(super) fn recursive(&mut self, input: &[u8]) -> Result<Ast, Error> {
        self.token = Some(self.rtoken(input)?);
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
        self.parsed_leaves.clear();
        self.token = None;
        self.lookahead = None;
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

    /// Borrowed names keep source offsets until the grammar needs a symbol.
    fn rscan(
        scanner: &mut Scanner,
        bindings: &mut Bindings,
        input: &[u8],
        out: &mut Token,
    ) -> Result<(), Error> {
        scanner.next_closed(input, out)?;
        if out.kind == Kind::Name && matches!(out.text, Lexeme::Owned(_)) {
            let (start, end) = scanner.name_span();
            out.symbol = bindings.intern(input, start, end)?;
        }
        Ok(())
    }

    fn rtoken(&mut self, input: &[u8]) -> Result<Token, Error> {
        let mut token = Token::default();
        Self::rscan(&mut self.scanner, &mut self.bindings, input, &mut token)?;
        Ok(token)
    }

    /// Consume the current token and fetch its successor.
    #[inline]
    fn bump(&mut self, input: &[u8]) -> Result<Taken, Error> {
        let current = self.token.as_mut().expect("current token");
        if current.kind == Kind::Eof {
            return Err(abandon());
        }
        let taken = Taken {
            kind: current.kind,
            line: current.line,
            end_line: current.end_line,
        };
        match self.lookahead.take() {
            Some(next) => *current = next,
            None => Self::rscan(&mut self.scanner, &mut self.bindings, input, current)?,
        }
        self.previous_line = taken.end_line;
        Ok(taken)
    }
    /// Consume the current token and return its text.
    fn bump_text(&mut self, input: &[u8]) -> Result<Lexeme, Error> {
        let text = std::mem::take(&mut self.token.as_mut().expect("current token").text);
        self.bump(input)?;
        Ok(text)
    }
    /// Consume a name token and return its text.
    fn rname(&mut self, input: &[u8]) -> Result<Lexeme, Error> {
        if self.kind() != Kind::Name {
            return Err(abandon());
        }
        self.bump_text(input)
    }
    fn rsymbol(&mut self, input: &[u8]) -> Result<(Lexeme, u32), Error> {
        let name = self.rname(input)?;
        let symbol = self.bindings.intern_lexeme(input, &name)?;
        Ok((name, symbol))
    }
    fn rexpect(&mut self, input: &[u8], kind: Kind) -> Result<Taken, Error> {
        if self.kind() != kind {
            return Err(abandon());
        }
        self.bump(input)
    }
    fn renter(&mut self) -> Result<(), Error> {
        if self.depth >= MAX_RECURSIVE_DEPTH
            || self.base_depth.wrapping_add(self.depth).wrapping_add(1) > MAX_SYNTAX_DEPTH
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
        let id = self.ast.stmts.len();
        self.ast.stmts.try_push((stmt, line))?;
        Ok(id)
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
                    self.raccount(input, |r, cx| r.return_values(cx.ast.ids(values), cx))?;
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
                self.raccount(input, |r, cx| r.condition(cond, cx))?;
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
                self.raccount(input, |r, cx| r.condition(cond, cx))?;
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
            self.raccount(input, |r, cx| r.condition(cond, cx))?;
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
        self.raccount(input, |r, cx| r.next(start, cx))?;
        self.rexpect(input, Kind::Comma)?;
        let limit = self.rexpr(input, 0)?;
        self.raccount(input, |r, cx| r.next(limit, cx))?;
        let step = if self.kind() == Kind::Comma {
            self.bump(input)?;
            let step = self.rexpr(input, 0)?;
            self.raccount(input, |r, cx| r.next(step, cx))?;
            Some(step)
        } else {
            self.raccount(input, |r, cx| r.default_step(cx))?;
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

    fn rfor_in(&mut self, input: &[u8], line: u32, first: Lexeme) -> Result<StmtId, Error> {
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
        self.raccount(input, |r, cx| {
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
            self.raccount(input, |r, _| r.reserve(1))?;
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
            self.raccount(input, |r, cx| r.adjust(count, cx.ast.ids(values), cx))?;
            values
        } else {
            self.raccount(input, |r, cx| r.adjust(count, &[], cx))?;
            Span::default()
        };
        self.locals += count as u16;
        self.resources.locals(self.locals);
        let names = self.publish_names(mark)?;
        self.declare_names(input, names)?;
        self.rstatement_node(Stmt::Local(names, values), line)
    }

    fn rfunction_statement(&mut self, input: &[u8], line: u32) -> Result<StmtId, Error> {
        let mut target = self.rresolved_name(input, line)?;
        let mut method = false;
        while matches!(self.kind(), Kind::Dot | Kind::Colon) {
            self.raccount(input, |r, cx| r.any(target, cx))?;
            method = self.bump(input)?.kind == Kind::Colon;
            let field = self.rname(input)?;
            let key = self.rstring(input, field, line)?;
            target = self.expression_with::<false>(input, Expr::Index(target, key), line)?;
            if method {
                break;
            }
        }
        let value = self.rfunction(input, line, method)?;
        self.raccount(input, |r, cx| r.assignment(&[target], &[value], cx))?;
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
            let conflict = self.raccount(input, |r, _| r.conflict(&open[mark..], target));
            self.scratch.ids = open;
            conflict?;
            let limit =
                i32::from(MAX_SYNTAX_DEPTH) - i32::from(self.base_depth.wrapping_add(self.depth));
            if (self.scratch.ids.len() - mark) as i32 > limit {
                return Err(abandon());
            }
            self.scratch.ids.try_push(target)?;
        }
        self.rexpect(input, Kind::Assign)?;
        let values = self.rlist(input)?;
        let targets = self.publish_ids(mark)?;
        self.raccount(input, |r, cx| {
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
            self.raccount(input, |r, cx| r.next(value, cx))?;
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
                self.expression_with::<false>(input, Expr::Unary(op, inner), line)?
            }
            Kind::Nil => {
                self.bump(input)?;
                self.expression_with::<false>(input, Expr::Nil, line)?
            }
            Kind::True | Kind::False => {
                let value = self.bump(input)?.kind == Kind::True;
                self.expression_with::<false>(input, Expr::Boolean(value), line)?
            }
            Kind::Number => {
                let number = self.current().number;
                self.bump(input)?;
                self.rnumber(input, number, line)?
            }
            Kind::String => {
                let text = self.bump_text(input)?;
                self.rstring(input, text, line)?
            }
            Kind::Dots => {
                if !self.vararg {
                    return Err(abandon());
                }
                self.bump(input)?;
                self.expression_with::<false>(input, Expr::Vararg, line)?
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
            self.raccount(input, |r, cx| r.infix(left, token.kind, cx))?;
            let right = self.rexpr(input, right_power)?;
            left = self.expression_with::<false>(
                input,
                Expr::Binary(token.kind, left, right, token.end_line),
                line,
            )?;
        }
        self.depth -= 1;
        Ok(left)
    }

    fn rresolved_name(&mut self, input: &[u8], line: u32) -> Result<ExprId, Error> {
        let (mut name, symbol) = self.rsymbol(input)?;
        let reference = self.bindings.reference_symbol(symbol)?;
        let binding = reference.binding();
        if let Some(slot) = self.raccount(input, |r, cx| r.push_reference(reference, cx))? {
            name.set_string_slot(slot);
        }
        self.rpush_expr(Expr::Name(name), line, 1, binding)
    }

    fn rnumber(&mut self, input: &[u8], number: f64, line: u32) -> Result<ExprId, Error> {
        self.raccount(input, |r, _| r.push_number(number))?;
        self.rpush_expr(Expr::Number(number), line, 1, NameBinding::default())
    }

    fn rstring(&mut self, input: &[u8], text: Lexeme, line: u32) -> Result<ExprId, Error> {
        let id = self.rpush_expr(Expr::String(text), line, 0, NameBinding::default())?;
        let slot = self.raccount(input, |r, cx| r.push_string(id, cx))?;
        let Expr::String(text) = &mut self.ast.exprs[id].0 else {
            unreachable!("string expression")
        };
        text.set_string_slot(slot);
        Ok(id)
    }

    fn rpush_expr(
        &mut self,
        expr: Expr,
        line: u32,
        scalar_cost: u8,
        binding: NameBinding,
    ) -> Result<ExprId, Error> {
        let id = self.ast.exprs.len();
        self.ast.exprs.try_push((
            expr,
            SourceSpan {
                start: line,
                end: self.previous_line,
                discharge: self.previous_line,
                scalar_cost,
                operand: 0,
                binding,
            },
        ))?;
        Ok(id)
    }

    fn rprefix(&mut self, input: &[u8]) -> Result<ExprId, Error> {
        let line = self.line();
        let mut id = match self.kind() {
            Kind::Name => self.rresolved_name(input, line)?,
            Kind::LParen => {
                self.bump(input)?;
                let inner = self.rexpr(input, 0)?;
                self.rexpect(input, Kind::RParen)?;
                self.expression_with::<false>(input, Expr::Paren(inner), line)?
            }
            _ => return Err(abandon()),
        };
        loop {
            match self.kind() {
                Kind::Dot => {
                    self.raccount(input, |r, cx| r.any(id, cx))?;
                    self.bump(input)?;
                    let name = self.rname(input)?;
                    let key = self.rstring(input, name, line)?;
                    id = self.expression_with::<false>(input, Expr::Index(id, key), line)?;
                }
                Kind::LBracket => {
                    self.raccount(input, |r, cx| r.any(id, cx))?;
                    self.bump(input)?;
                    let key = self.rexpr(input, 0)?;
                    self.raccount(input, |r, cx| r.val(key, cx))?;
                    self.rexpect(input, Kind::RBracket)?;
                    id = self.expression_with::<false>(input, Expr::Index(id, key), line)?;
                }
                Kind::Colon => {
                    self.bump(input)?;
                    let (mut name, symbol) = self.rsymbol(input)?;
                    let slot = self.raccount(input, |r, cx| r.method(id, symbol, cx))?;
                    name.set_string_slot(slot);
                    let call_line = self.current().end_line;
                    let args = self.rargs(input)?;
                    id = self.expression_with::<false>(input, Expr::Method(id, name, args, call_line), line)?;
                }
                Kind::LParen | Kind::LBrace | Kind::String => {
                    self.raccount(input, |r, cx| r.next(id, cx))?;
                    let call_line = self.current().end_line;
                    let args = self.rargs(input)?;
                    id = self.expression_with::<false>(input, Expr::Call(id, args, call_line), line)?;
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
                self.raccount(input, |r, cx| r.arguments(cx.ast.ids(values)))?;
                self.rexpect(input, Kind::RParen)?;
                Ok(values)
            }
            Kind::LBrace => {
                let table = self.rtable(input)?;
                self.one(table)
            }
            Kind::String => {
                let line = self.line();
                let text = self.bump_text(input)?;
                let id = self.rstring(input, text, line)?;
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
        self.raccount(input, |r, _| r.begin_table())?;
        let previous = self.previous_line;
        let line = self.rexpect(input, Kind::LBrace)?.line;
        let mark = self.scratch.fields.len();
        loop {
            if self.kind() != Kind::RBrace {
                self.raccount(input, |r, cx| r.prepare_field(cx))?;
            }
            if self.kind() == Kind::Name && self.lookahead.is_none() {
                self.lookahead = Some(self.rtoken(input)?);
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
                let line = self.line();
                let text = self.bump_text(input)?;
                let key = self.rstring(input, text, line)?;
                self.rexpect(input, Kind::Assign)?;
                self.raccount(input, |r, cx| r.record_key(key, cx))?;
                Some(key)
            } else if self.kind() == Kind::LBracket {
                self.bump(input)?;
                let key = self.rexpr(input, 0)?;
                self.raccount(input, |r, cx| r.val(key, cx))?;
                self.rexpect(input, Kind::RBracket)?;
                self.rexpect(input, Kind::Assign)?;
                self.raccount(input, |r, cx| r.record_key(key, cx))?;
                Some(key)
            } else {
                None
            };
            let value = self.rexpr(input, 0)?;
            self.raccount(input, |r, cx| r.field(key, value, cx))?;
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
        self.expression_with::<false>(input, Expr::Table(fields, previous), line)
    }

    fn repeated_leaf(
        &mut self,
        input: &[u8],
        line: u32,
        start: usize,
        old: ParsedLeaf,
    ) -> Result<Option<ExprId>, Error> {
        if self.depth > old.depth {
            return Ok(None);
        }
        let old_line = self.ast.exprs[old.id].1.start;
        let [old_start, old_end] = old.offsets.map(|offset| offset as usize);
        let end = start + old_end - old_start;
        if input.get(start..end) != Some(&input[old_start..old_end])
            || input
                .get(end)
                .is_some_and(|&byte| byte == b'_' || crate::locale::is_alnum(byte))
        {
            return Ok(None);
        }
        let Expr::Function {
            params,
            vararg,
            body,
            end_line,
            ..
        } = self.ast.exprs[old.id].0
        else {
            unreachable!("parsed leaf function");
        };
        let Some(new_end) = line.checked_add(end_line - old_line) else {
            return Ok(None);
        };
        // The first parse checked this body at an equal or greater nesting
        // depth. Every name binds within the leaf.
        self.scanner.skip_closed_bytes(end, new_end);
        self.bump(input)?;
        self.previous_line = new_end;
        self.ast
            .stacks
            .try_push((self.ast.exprs.len(), old.stack))?;
        if let Some(sign) = old.zero_sign {
            self.ast.zero_signs.try_push((self.ast.exprs.len(), sign))?;
        }
        self.expression_with::<false>(
            input,
            Expr::Function {
                params,
                vararg,
                body,
                end_line: new_end,
                template_source: old.offsets,
                template_line: old_line,
            },
            line,
        )
        .map(Some)
    }

    /// Parse a parameter list and body after `function`.
    fn rfunction(&mut self, input: &[u8], line: u32, method: bool) -> Result<ExprId, Error> {
        let source_start = self.scanner.position();
        let source_candidate = input.len() >= MIN_TEMPLATE_SOURCE
            && !method
            && self.lookahead.is_none()
            && self.kind() == Kind::LParen
            && self.line() == line;
        let slot = source_candidate
            .then(|| leaf_slot(input, source_start))
            .flatten();
        if let Some(slot) = slot
            && let Some(&Some(old)) = self.parsed_leaves.get(slot)
            && let Some(id) = self.repeated_leaf(input, line, source_start, old)?
        {
            return Ok(id);
        }
        let first_expr = self.ast.exprs.len();
        let children = self.ast.stacks.len();
        self.rexpect(input, Kind::LParen)?;
        let mark = self.scratch.names.len();
        if method {
            self.scratch
                .names
                .try_push(Lexeme::Owned(Text::new(b"self")?))?;
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
        let source_end = self.scanner.position();
        let leaf = source_candidate
            && self.lookahead.is_none()
            && self.ast.stacks.len() == children
            && !self.bindings.has_captures();
        let end_line = self.rexpect(input, Kind::End)?.end_line;
        let template_source = if leaf
            && source_end - source_start <= MAX_TEMPLATE_BODY as usize
            && let Ok(end) = u32::try_from(source_end)
        {
            [source_start as u32, end]
        } else {
            [0; 2]
        };
        (self.loops, self.vararg, self.locals, self.function_line) = parent;
        self.bindings.leave();
        let (sign, stack) = self.resources.leave();
        let id = self.ast.exprs.len();
        if template_source[0] != template_source[1]
            && let Some(slot) = slot
            && self.ast.exprs[first_expr..].iter().all(|(expr, span)| {
                !matches!(expr, Expr::Name(_)) || span.binding.local_index().is_some()
            })
        {
            if self.parsed_leaves.is_empty() {
                self.parsed_leaves.try_resize(LEAF_SLOTS, None)?;
            }
            self.parsed_leaves[slot] = Some(ParsedLeaf {
                id,
                offsets: template_source,
                depth: self.depth,
                stack,
                zero_sign: sign,
            });
        }
        self.ast.stacks.try_push((id, stack))?;
        if let Some(sign) = sign {
            self.ast.zero_signs.try_push((self.ast.exprs.len(), sign))?;
        }
        self.expression_with::<false>(
            input,
            Expr::Function {
                params,
                vararg,
                body,
                end_line,
                template_source,
                template_line: 0,
            },
            line,
        )
    }
}
