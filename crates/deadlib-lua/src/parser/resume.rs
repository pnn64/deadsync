//! Flat grammar continuations for fixed and reader-supplied source.
use super::bindings::Bindings;
use super::resources::{Context, Resources};
use super::*;
use crate::lexer::Scanner;
use crate::memory::fallible::{self, SliceExt, VecExt};

/// Grammar continuations. List-building tasks hold a mark into one of the
/// parser's scratch stacks; finished lists are published to the AST pools.
#[derive(Debug)]
enum Task {
    RootEnd,
    Expect(Kind),
    Block,
    BlockKeepLocals,
    BlockNext(usize),
    BlockAfter(usize, bool),
    BlockEnd(usize),
    RestoreLocals(u16),
    Statement,
    Local(u32),
    LocalName(u32, usize),
    LocalNames(u32, usize),
    LocalEnd(u32, usize),
    LocalFunctionName(u32),
    LocalFunctionBody(u32, Text),
    LocalFunctionEnd(u32, Text),
    FunctionName(u32),
    FunctionTarget(u32, ExprId),
    FunctionField(u32, ExprId, bool),
    FunctionAssign(u32, ExprId),
    Return(u32),
    ReturnEnd(u32),
    Break(u32),
    DoEnd(u32),
    WhileCond(u32),
    WhileEnd(u32, ExprId),
    RepeatBody(u32),
    RepeatEnd(u32, Block),
    IfCond(u32, usize),
    IfArm(u32, usize, ExprId),
    IfElse(u32, usize),
    ForName(u32),
    ForChoice(u32, Text),
    ForStart(u32, Text),
    ForLimit(u32, Text, ExprId),
    ForStep(u32, Text, ExprId, ExprId),
    ForEnd(u32, Text, ExprId, ExprId, Option<ExprId>, u32),
    ForNames(u32, usize),
    ForMoreName(u32, usize),
    ForBegin(u32, usize),
    ForValues(u32, usize, u32),
    ForInEnd(u32, Span, Span, [u32; 2]),
    ExpressionStatement(u32),
    AssignTargets(u32, usize),
    AssignTarget(u32, usize),
    AssignEnd(u32, usize),
    List,
    ListAfter(usize),
    Function(u32, bool),
    Params(u32, usize, bool),
    ParamAfter(u32, usize),
    ParamsEnd(u32, usize, bool),
    FunctionEnd(u32, Span, bool, u16, bool, u16, u32),
    FunctionAfterEnd(u32, Span, bool, u16, bool, u16, u32, u32),
    Expr(u8),
    ExprLoop(u8, u32),
    Unary(Kind, u32),
    Binary(u8, u32, ExprId, Kind, u32),
    Prefix,
    ResolveName(ExprId),
    Paren(u32),
    ParenEnd(u32),
    Suffix(u32, ExprId),
    Field(u32, ExprId),
    FieldName(u32, ExprId, Text, Option<bool>),
    Index(u32, ExprId),
    IndexEnd(u32, ExprId, ExprId),
    FunctionFieldEnd(u32, ExprId, ExprId, bool),
    ResourceNext(ExprId),
    ResourceInfix(ExprId, Kind),
    ResourceKey(ExprId),
    MethodName(u32, ExprId),
    MethodArgs(u32, ExprId, Text),
    MethodEnd(u32, ExprId, Text, u32),
    CallEnd(u32, ExprId, u32),
    Args,
    ArgsParen(u32),
    ArgsEnd(u32),
    ArgsTable,
    Table,
    TableNext([u32; 2], usize),
    TableAssign([u32; 2], usize, ExprId),
    TableKey([u32; 2], usize),
    TableValue([u32; 2], usize, Option<ExprId>),
    TableSeparator([u32; 2], usize),
    TableEnd([u32; 2], usize),
}

/// Open lists, innermost last. Each kind is a LIFO stack because grammar
/// lists of one kind finish before an enclosing list of that kind continues.
#[derive(Debug, Default)]
struct Scratch {
    ids: Vec<usize>,
    names: Vec<Text>,
    fields: Vec<(Option<ExprId>, ExprId)>,
    arms: Vec<(ExprId, Block)>,
}
impl Scratch {
    fn heap_bytes(&self) -> usize {
        self.ids.capacity() * size_of::<usize>()
            + self.names.capacity() * size_of::<Text>()
            + self.names.iter().map(|n| n.heap_bytes()).sum::<usize>()
            + self.fields.capacity() * size_of::<(Option<ExprId>, ExprId)>()
            + self.arms.capacity() * size_of::<(ExprId, Block)>()
    }
}

/// A parser that suspends at a lexical input request. AST edges and grammar
/// continuations are indices/flat vectors, with no recursive parser invocation.
#[derive(Debug)]
pub(crate) struct Parser {
    scanner: Scanner,
    token: Option<Token>,
    lookahead: Option<Token>,
    previous_line: u32,
    depth: u16,
    base_depth: u16,
    loops: u16,
    vararg: bool,
    locals: u16,
    function_line: u32,
    bindings: Bindings,
    resources: Resources,
    tasks: Vec<Task>,
    ast: Ast,
    scratch: Scratch,
    expr: ExprId,
    stmt: StmtId,
    list: Span,
    body: Block,
    finished: bool,
    native_events: Option<Vec<NativeRootEvent>>,
}

impl Parser {
    pub(crate) fn try_new() -> Result<Self, Error> {
        Ok(Self {
            scanner: Scanner::default(),
            token: None,
            lookahead: None,
            previous_line: 1,
            depth: 0,
            base_depth: 0,
            loops: 0,
            vararg: true,
            locals: 0,
            function_line: 0,
            bindings: Bindings::try_new()?,
            resources: Resources::default(),
            tasks: fallible::vec([Task::RootEnd, Task::Block])?,
            ast: Ast::default(),
            scratch: Scratch::default(),
            expr: 0,
            stmt: 0,
            list: Span::default(),
            body: Span::default(),
            finished: false,
            native_events: None,
        })
    }
}

fn precedence(kind: Kind) -> Option<(u8, u8)> {
    Some(match kind {
        Kind::Or => (1, 1),
        Kind::And => (2, 2),
        Kind::Eq | Kind::Ne | Kind::Lt | Kind::Le | Kind::Gt | Kind::Ge => (3, 3),
        Kind::Concat => (5, 4),
        Kind::Plus | Kind::Minus => (6, 6),
        Kind::Star | Kind::Slash | Kind::Percent => (7, 7),
        Kind::Caret => (10, 9),
        _ => return None,
    })
}

impl Parser {
    pub(crate) fn enable_native_roots(&mut self) -> Result<(), Error> {
        self.native_events = Some(fallible::vec([NativeRootEvent::Open])?);
        Ok(())
    }
    pub(crate) fn take_native_events(&mut self) -> Vec<NativeRootEvent> {
        self.native_events
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }
    pub(crate) fn with_native_depth(depth: usize) -> Result<Self, Error> {
        Ok(Self {
            base_depth: depth as u16,
            ..Self::try_new()?
        })
    }
    pub(crate) fn nesting(&self) -> usize {
        usize::from(self.depth)
    }
    pub(crate) fn shift_native_depth(&mut self, change: u16) {
        self.base_depth = self.base_depth.wrapping_add(change);
    }
    pub(crate) fn arena_bytes(&self) -> usize {
        self.tasks.capacity() * std::mem::size_of::<Task>()
            + self.ast.exprs.capacity() * std::mem::size_of::<(Expr, SourceSpan)>()
            + self.ast.stmts.capacity() * std::mem::size_of::<(Stmt, u32)>()
            + self.ast.list_bytes()
            + self.scratch.heap_bytes()
            + self.scanner.heap_bytes()
            + native_root_bytes(&self.native_events)
            + self.bindings.heap_bytes()
            + self.resources.heap_bytes()
            + self.ast.zero_signs.capacity() * size_of::<(ExprId, bool)>()
            + self.ast.stacks.capacity() * size_of::<(ExprId, u16)>()
            + self.ast.constant_operands.capacity() * size_of::<(ExprId, ConstantOperand)>()
    }
    pub(crate) fn heap_bytes(&self) -> usize {
        let mut bytes = self.arena_bytes();
        bytes += self.token.as_ref().map_or(0, |t| t.text.heap_bytes())
            + self.lookahead.as_ref().map_or(0, |t| t.text.heap_bytes());
        for (expr, _) in &self.ast.exprs {
            bytes += match expr {
                Expr::String(s) | Expr::Name(s) | Expr::Method(_, s, ..) => s.heap_bytes(),
                _ => 0,
            };
        }
        for (stmt, _) in &self.ast.stmts {
            bytes += match stmt {
                Stmt::LocalFunction(n, _) | Stmt::For { name: n, .. } => n.heap_bytes(),
                _ => 0,
            };
        }
        for task in &self.tasks {
            bytes += match task {
                Task::LocalFunctionEnd(_, n)
                | Task::LocalFunctionBody(_, n)
                | Task::ForChoice(_, n)
                | Task::ForStart(_, n)
                | Task::ForLimit(_, n, _)
                | Task::ForStep(_, n, ..)
                | Task::ForEnd(_, n, ..)
                | Task::MethodEnd(_, _, n, _)
                | Task::MethodArgs(_, _, n)
                | Task::FieldName(_, _, n, _) => n.heap_bytes(),
                _ => 0,
            };
        }
        bytes
    }
    /// Size the node arenas for a complete source. Lua source averages about
    /// one expression per 11 bytes, so most sources never grow these arenas.
    fn presize(&mut self, source: usize) -> Result<(), Error> {
        let expressions = source / 8 + 8;
        fallible::reserve(&mut self.ast.exprs, expressions)?;
        self.resources.reserve_values(expressions)?;
        fallible::reserve(&mut self.ast.stmts, source / 32 + 4)?;
        fallible::reserve(&mut self.ast.ids, expressions)?;
        fallible::reserve(&mut self.tasks, 64)?;
        Ok(())
    }
    /// Publish a one-element identifier list directly; no list is being moved.
    fn one(&mut self, id: usize) -> Result<Span, Error> {
        let mark = self.scratch.ids.len();
        self.scratch.ids.try_push(id)?;
        publish(&mut self.ast.ids, &mut self.scratch.ids, mark)
    }
    fn publish_ids(&mut self, mark: usize) -> Result<Span, Error> {
        publish(&mut self.ast.ids, &mut self.scratch.ids, mark)
    }
    fn publish_names(&mut self, mark: usize) -> Result<Span, Error> {
        publish(&mut self.ast.names, &mut self.scratch.names, mark)
    }
    /// The most recent field of the innermost open table constructor.
    fn last_field(&self, mark: usize) -> Option<(Option<ExprId>, ExprId)> {
        self.scratch.fields[mark..].last().copied()
    }
    /// Declare published local names in order.
    fn declare_names(&mut self, input: &[u8], names: Span) -> Result<(), Error> {
        for name in self.ast.names(names) {
            self.bindings.declare(input, name)?;
        }
        Ok(())
    }
    fn current(&self) -> &Token {
        self.token
            .as_ref()
            .expect("grammar dispatch has a current token")
    }
    fn kind(&self) -> Kind {
        self.current().kind
    }
    fn line(&self) -> u32 {
        self.current().line
    }
    fn error(&self, text: &'static str) -> Error {
        let mut error = Error::new_static(ErrorKind::Syntax, self.scanner.line(), text);
        error.near = true;
        error
    }
    fn formatted_error(&self, arguments: std::fmt::Arguments<'_>) -> Error {
        let mut error = fallible::diagnostic(ErrorKind::Syntax, self.scanner.line(), arguments);
        error.near = error.kind() != ErrorKind::Memory;
        error
    }
    fn account<T>(
        &mut self,
        input: &[u8],
        action: impl FnOnce(&mut Resources, &Context<'_>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let cx = Context {
            ast: &self.ast,
            input,
            bindings: &self.bindings,
        };
        let before = self.resources.constant_count();
        let result = action(&mut self.resources, &cx);
        if let Some(events) = &mut self.native_events {
            events.try_extend(
                self.resources
                    .native_constants(before, &cx)?
                    .into_iter()
                    .map(NativeRootEvent::Constant),
            )?;
        }
        result.map_err(|error| {
            let mut error = error.at_line(self.scanner.line());
            error.near = error.kind() != ErrorKind::Memory;
            error
        })
    }
    fn enter(&mut self) -> Result<(), Error> {
        if self.base_depth.wrapping_add(self.depth).wrapping_add(1) > 200 {
            return Err(Error::new_static(
                ErrorKind::Limit,
                self.scanner.line(),
                "chunk has too many syntax levels",
            ));
        }
        self.depth += 1;
        Ok(())
    }
    fn limit_error(&self, limit: impl std::fmt::Display, what: &str, line: u32) -> Error {
        if line == 0 {
            fallible::diagnostic(
                ErrorKind::Limit,
                self.scanner.line(),
                format_args!("main function has more than {limit} {what}"),
            )
        } else {
            fallible::diagnostic(
                ErrorKind::Limit,
                self.scanner.line(),
                format_args!("function at line {line} has more than {limit} {what}"),
            )
        }
    }
    fn check_local_count_at(&self, total: usize, line: u32) -> Result<(), Error> {
        if total > 200 {
            return Err(self.limit_error(200, "local variables", line));
        }
        Ok(())
    }
    fn check_local_count(&self, total: usize) -> Result<(), Error> {
        self.check_local_count_at(total, self.function_line)
    }
    fn take(&mut self) -> Token {
        let token = self
            .token
            .take()
            .expect("grammar consumes its current token");
        self.previous_line = token.end_line;
        self.token = self.lookahead.take();
        token
    }
    fn expect(&mut self, kind: Kind) -> Result<Token, Error> {
        if self.kind() != kind {
            let name = String::from_utf8(kind.try_label()?).expect("expected token is ASCII");
            return Err(self.formatted_error(format_args!("'{name}' expected")));
        }
        Ok(self.take())
    }
    fn close(&mut self, kind: Kind, opener: Kind, line: u32) -> Result<Token, Error> {
        if self.kind() != kind && self.scanner.line() != line {
            let kind = String::from_utf8(kind.try_label()?).expect("closing token");
            let opener = String::from_utf8(opener.try_label()?).expect("opening token");
            return Err(self.formatted_error(format_args!(
                "'{kind}' expected (to close '{opener}' at line {line})"
            )));
        }
        self.expect(kind)
    }
    fn end_block(&self) -> bool {
        matches!(
            self.kind(),
            Kind::Eof | Kind::End | Kind::Else | Kind::ElseIf | Kind::Until
        )
    }
    fn expression(&mut self, input: &[u8], expr: Expr, line: u32) -> Result<ExprId, Error> {
        let id = self.ast.exprs.len();
        let scalar_cost = match &expr {
            Expr::Number(_) | Expr::Name(_) => 1,
            Expr::Paren(inner) | Expr::Unary(_, inner) => {
                let cost = self.ast.exprs[*inner].1.scalar_cost;
                if cost != 0 && cost < 32 { cost + 1 } else { 0 }
            }
            Expr::Binary(
                Kind::Plus | Kind::Minus | Kind::Star | Kind::Slash | Kind::Percent | Kind::Caret,
                left,
                right,
                _,
            ) => {
                let a = self.ast.exprs[*left].1.scalar_cost;
                let b = self.ast.exprs[*right].1.scalar_cost;
                let cost = a + b + 1;
                if a != 0 && b != 0 && cost <= 32 {
                    cost
                } else {
                    0
                }
            }
            _ => 0,
        };
        self.ast.exprs.try_push((
            expr,
            SourceSpan {
                start: line,
                end: self.previous_line,
                discharge: self.previous_line,
                scalar_cost,
            },
        ))?;
        self.account(input, |r, cx| r.expression(id, cx))?;
        self.expr = id;
        Ok(id)
    }
    fn statement(&mut self, stmt: Stmt, line: u32) -> Result<(), Error> {
        self.stmt = self.ast.stmts.len();
        self.ast.stmts.try_push((stmt, line))?;
        Ok(())
    }
    fn start_function_body(
        &mut self,
        input: &[u8],
        line: u32,
        params: usize,
        vararg: bool,
    ) -> Result<(), Error> {
        let count = self.scratch.names.len() - params + usize::from(vararg);
        self.check_local_count_at(count, line)?;
        self.expect(Kind::RParen)?;
        let params = self.publish_names(params)?;
        self.bindings
            .enter(input, line, self.ast.names(params), vararg)?;
        self.tasks.try_push(Task::FunctionEnd(
            line,
            params,
            vararg,
            self.loops,
            self.vararg,
            self.locals,
            self.function_line,
        ))?;
        self.function_line = line;
        self.loops = 0;
        self.vararg = vararg;
        self.locals = count as u16;
        self.resources.enter(self.locals)?;
        self.tasks.try_push(Task::Block)?;
        Ok(())
    }
    fn for_body(
        &mut self,
        input: &[u8],
        line: u32,
        name: Text,
        start: ExprId,
        limit: ExprId,
        step: Option<ExprId>,
    ) -> Result<(), Error> {
        let do_line = self.expect(Kind::Do)?.end_line;
        self.loops += 1;
        self.locals += 4;
        self.resources.locals(self.locals);
        self.bindings.hidden(3)?;
        self.bindings.declare(input, &name)?;
        self.tasks
            .try_push(Task::ForEnd(line, name, start, limit, step, do_line))?;
        self.tasks.try_push(Task::Block)?;
        Ok(())
    }

    /// Supply the entire retained prefix and whether the reader has reached EOF.
    /// `None` requests more source; `Some` contains the finished flat AST.
    pub(crate) fn resume(&mut self, input: &[u8], closed: bool) -> Result<Option<Ast>, Error> {
        if closed && self.ast.exprs.capacity() == 0 && !crate::memory::compilation::active() {
            self.presize(input.len())?;
        }
        self.resume_inner(input, closed).map_err(|mut error| {
            if !error.near {
                return error;
            }
            error.near = false;
            if self.kind() == Kind::Byte(0) {
                return error;
            }
            let build = || -> Result<Error, Error> {
                let kind = self.kind();
                let near = if matches!(kind, Kind::Name | Kind::String | Kind::Number) {
                    self.scanner.near_buffer(input)?
                } else {
                    kind.try_label()?
                };
                let mut bytes = error.message_bytes().try_to_vec()?;
                bytes.try_extend_from_slice(b" near '")?;
                bytes.try_extend_from_slice(
                    &near[..near.iter().position(|&b| b == 0).unwrap_or(near.len())],
                )?;
                bytes.try_push(b'\'')?;
                Ok(Error::try_from_bytes(error.kind(), error.line(), bytes))
            };
            build().unwrap_or_else(|error| error)
        })
    }
    fn scan(&mut self, input: &[u8], closed: bool) -> Result<Option<Token>, Error> {
        let token = self.scanner.next(input, closed)?;
        if let Some(token) = &token
            && let Some(events) = &mut self.native_events
        {
            let text = match token.kind {
                Kind::Name | Kind::String => Some(token.text.try_clone()?),
                Kind::And
                | Kind::Break
                | Kind::Do
                | Kind::Else
                | Kind::ElseIf
                | Kind::End
                | Kind::False
                | Kind::For
                | Kind::Function
                | Kind::If
                | Kind::In
                | Kind::Local
                | Kind::Nil
                | Kind::Not
                | Kind::Or
                | Kind::Repeat
                | Kind::Return
                | Kind::Then
                | Kind::True
                | Kind::Until
                | Kind::While => Some(Text::new(
                    token.kind.static_label().expect("keyword label"),
                )?),
                _ => None,
            };
            if let Some(text) = text {
                events.try_push(NativeRootEvent::Anchor(text))?;
            }
        }
        if token.as_ref().is_some_and(|t| t.kind == Kind::Name) {
            let (start, end) = self.scanner.name_span();
            self.bindings.intern(input, start, end)?;
        }
        Ok(token)
    }
    fn resume_inner(&mut self, input: &[u8], closed: bool) -> Result<Option<Ast>, Error> {
        assert!(!self.finished, "a completed parser cannot be resumed");
        loop {
            if self.tasks.is_empty() {
                self.finished = true;
                self.resources.finish(&mut self.ast);
                self.resources = Resources::default();
                return Ok(Some(std::mem::take(&mut self.ast)));
            }
            // Advance fetches the next token before continuing grammar work.
            // Additional lookahead is requested only for table record fields.
            if self.token.is_none() {
                self.token = self.scan(input, closed)?;
                if self.token.is_none() {
                    return Ok(None);
                }
            }
            if matches!(self.tasks.last(), Some(Task::TableNext(..))) && self.kind() != Kind::RBrace
            {
                self.account(input, |r, cx| r.prepare_field(cx))?;
            }
            if matches!(self.tasks.last(), Some(Task::TableNext(..)))
                && self.kind() == Kind::Name
                && self.lookahead.is_none()
            {
                self.lookahead = self.scan(input, closed)?;
                if self.lookahead.is_none() {
                    return Ok(None);
                }
            }
            let task = self.tasks.pop().expect("grammar continuation exists");
            match task {
                Task::RootEnd => {
                    self.ast.end_line = self.previous_line;
                    self.expect(Kind::Eof)?;
                    self.ast.body = std::mem::take(&mut self.body);
                    if let Some(events) = &mut self.native_events {
                        events.try_push(NativeRootEvent::Close)?;
                    }
                }
                Task::Expect(kind) => {
                    self.expect(kind)?;
                }
                Task::Block => {
                    self.enter()?;
                    self.tasks.try_push(Task::RestoreLocals(self.locals))?;
                    self.tasks
                        .try_push(Task::BlockNext(self.scratch.ids.len()))?;
                }
                Task::BlockKeepLocals => {
                    self.enter()?;
                    self.tasks
                        .try_push(Task::BlockNext(self.scratch.ids.len()))?;
                }
                Task::BlockNext(stmts) => {
                    if self.end_block() {
                        self.tasks.try_push(Task::BlockEnd(stmts))?;
                    } else {
                        let terminal = matches!(self.kind(), Kind::Break | Kind::Return);
                        self.tasks.try_push(Task::BlockAfter(stmts, terminal))?;
                        self.tasks.try_push(Task::Statement)?;
                    }
                }
                Task::BlockAfter(stmts, terminal) => {
                    self.resources.reset();
                    self.scratch.ids.try_push(self.stmt)?;
                    if self.kind() == Kind::Semi {
                        self.take();
                    }
                    self.tasks.try_push(if terminal {
                        Task::BlockEnd(stmts)
                    } else {
                        Task::BlockNext(stmts)
                    })?;
                }
                Task::BlockEnd(stmts) => {
                    self.depth -= 1;
                    self.body = self.publish_ids(stmts)?;
                }
                Task::RestoreLocals(count) => {
                    self.locals = count;
                    self.resources.locals(count);
                    self.bindings.truncate(count);
                }
                Task::Statement => {
                    let line = self.line();
                    match self.kind() {
                        Kind::Local => {
                            self.take();
                            self.tasks.try_push(Task::Local(line))?;
                        }
                        Kind::Function => {
                            self.take();
                            self.tasks.try_push(Task::FunctionName(line))?;
                        }
                        Kind::Return => {
                            self.take();
                            self.tasks.try_push(Task::Return(line))?;
                        }
                        Kind::Break => {
                            self.take();
                            self.tasks.try_push(Task::Break(line))?;
                        }
                        Kind::Do => {
                            self.take();
                            self.tasks.try_push(Task::DoEnd(line))?;
                            self.tasks.try_push(Task::Block)?;
                        }
                        Kind::While => {
                            self.take();
                            self.tasks.try_push(Task::WhileCond(line))?;
                            self.tasks.try_push(Task::Expr(0))?;
                        }
                        Kind::Repeat => {
                            self.take();
                            self.loops += 1;
                            self.tasks.try_push(Task::RestoreLocals(self.locals))?;
                            self.tasks.try_push(Task::RepeatBody(line))?;
                            self.tasks.try_push(Task::BlockKeepLocals)?;
                        }
                        Kind::If => {
                            self.take();
                            self.tasks
                                .try_push(Task::IfCond(line, self.scratch.arms.len()))?;
                            self.tasks.try_push(Task::Expr(0))?;
                        }
                        Kind::For => {
                            self.take();
                            self.tasks.try_push(Task::ForName(line))?;
                        }
                        _ => {
                            self.tasks.try_push(Task::ExpressionStatement(line))?;
                            self.tasks.try_push(Task::Prefix)?;
                        }
                    }
                }
                Task::Local(line) => {
                    if self.kind() == Kind::Function {
                        self.take();
                        self.tasks.try_push(Task::LocalFunctionName(line))?;
                    } else {
                        let name = self.expect(Kind::Name)?.text;
                        let names = self.scratch.names.len();
                        self.scratch.names.try_push(name)?;
                        self.tasks.try_push(Task::LocalNames(line, names))?;
                    }
                }
                Task::LocalName(line, names) => {
                    let name = self.expect(Kind::Name)?.text;
                    self.scratch.names.try_push(name)?;
                    self.tasks.try_push(Task::LocalNames(line, names))?;
                }
                Task::LocalNames(line, names) => {
                    let count = self.scratch.names.len() - names;
                    self.check_local_count(usize::from(self.locals) + count)?;
                    if self.kind() == Kind::Comma {
                        self.take();
                        self.tasks.try_push(Task::LocalName(line, names))?;
                    } else if self.kind() == Kind::Assign {
                        self.take();
                        self.tasks.try_push(Task::LocalEnd(line, names))?;
                        self.tasks.try_push(Task::List)?;
                    } else {
                        self.account(input, |r, cx| r.adjust(count, &[], cx))?;
                        self.locals += count as u16;
                        self.resources.locals(self.locals);
                        let names = self.publish_names(names)?;
                        self.declare_names(input, names)?;
                        self.statement(Stmt::Local(names, Span::default()), line)?;
                    }
                }
                Task::LocalEnd(line, names) => {
                    let values = std::mem::take(&mut self.list);
                    let count = self.scratch.names.len() - names;
                    self.account(input, |r, cx| r.adjust(count, cx.ast.ids(values), cx))?;
                    self.locals += count as u16;
                    self.resources.locals(self.locals);
                    let names = self.publish_names(names)?;
                    self.declare_names(input, names)?;
                    self.statement(Stmt::Local(names, values), line)?;
                }
                Task::LocalFunctionName(line) => {
                    let name = self.expect(Kind::Name)?.text;
                    self.tasks.try_push(Task::LocalFunctionBody(line, name))?;
                }
                Task::LocalFunctionBody(line, name) => {
                    self.check_local_count(usize::from(self.locals) + 1)?;
                    self.account(input, |r, _| r.reserve(1))?;
                    self.locals += 1;
                    self.resources.locals(self.locals);
                    self.bindings.declare(input, &name)?;
                    self.tasks.try_push(Task::LocalFunctionEnd(line, name))?;
                    self.tasks
                        .try_push(Task::Function(self.current().end_line, false))?;
                }
                Task::LocalFunctionEnd(line, name) => {
                    self.statement(Stmt::LocalFunction(name, self.expr), line)?
                }
                Task::FunctionName(line) => {
                    let name = self.expect(Kind::Name)?.text;
                    let id = self.expression(input, Expr::Name(name), line)?;
                    self.tasks.try_push(Task::FunctionTarget(line, id))?;
                    self.tasks.try_push(Task::ResolveName(id))?;
                }
                Task::FunctionTarget(line, id) => {
                    if matches!(self.kind(), Kind::Dot | Kind::Colon) {
                        self.account(input, |r, cx| r.any(id, cx))?;
                        let method = self.take().kind == Kind::Colon;
                        self.tasks.try_push(Task::FunctionField(line, id, method))?;
                    } else {
                        self.tasks.try_push(Task::FunctionAssign(line, id))?;
                        self.tasks.try_push(Task::Function(line, false))?;
                    }
                }
                Task::FunctionField(line, id, method) => {
                    let name = self.expect(Kind::Name)?.text;
                    self.tasks
                        .try_push(Task::FieldName(line, id, name, Some(method)))?;
                }
                Task::FunctionFieldEnd(line, id, key, method) => {
                    let target = self.expression(input, Expr::Index(id, key), line)?;
                    if method {
                        self.tasks.try_push(Task::FunctionAssign(line, target))?;
                        self.tasks.try_push(Task::Function(line, true))?;
                    } else {
                        self.tasks.try_push(Task::FunctionTarget(line, target))?;
                    }
                }
                Task::FunctionAssign(line, target) => {
                    let value = self.expr;
                    self.account(input, |r, cx| r.assignment(&[target], &[value], cx))?;
                    let targets = self.one(target)?;
                    let values = self.one(value)?;
                    self.statement(Stmt::FunctionDefinition(targets, values), line)?;
                }
                Task::Return(line) => {
                    if self.end_block() || self.kind() == Kind::Semi {
                        self.statement(Stmt::Return(Span::default(), self.previous_line), line)?;
                    } else {
                        self.tasks.try_push(Task::ReturnEnd(line))?;
                        self.tasks.try_push(Task::List)?;
                    }
                }
                Task::ReturnEnd(line) => {
                    let values = std::mem::take(&mut self.list);
                    self.account(input, |r, cx| r.return_values(cx.ast.ids(values), cx))?;
                    self.statement(Stmt::Return(values, self.previous_line), line)?;
                }
                Task::Break(line) => {
                    if self.loops == 0 {
                        return Err(self.error("no loop to break"));
                    }
                    self.statement(Stmt::Break, line)?;
                }
                Task::DoEnd(line) => {
                    self.close(Kind::End, Kind::Do, line)?;
                    let body = std::mem::take(&mut self.body);
                    self.statement(Stmt::Do(body), line)?;
                }
                Task::WhileCond(line) => {
                    let cond = self.expr;
                    self.account(input, |r, cx| r.condition(cond, cx))?;
                    self.expect(Kind::Do)?;
                    self.loops += 1;
                    self.tasks.try_push(Task::WhileEnd(line, cond))?;
                    self.tasks.try_push(Task::Block)?;
                }
                Task::WhileEnd(line, cond) => {
                    self.close(Kind::End, Kind::While, line)?;
                    self.loops -= 1;
                    let body = std::mem::take(&mut self.body);
                    self.statement(Stmt::While(cond, body), line)?;
                }
                Task::RepeatBody(line) => {
                    self.close(Kind::Until, Kind::Repeat, line)?;
                    let body = std::mem::take(&mut self.body);
                    self.tasks.try_push(Task::RepeatEnd(line, body))?;
                    self.tasks.try_push(Task::Expr(0))?;
                }
                Task::RepeatEnd(line, body) => {
                    let cond = self.expr;
                    self.account(input, |r, cx| r.condition(cond, cx))?;
                    self.loops -= 1;
                    self.statement(Stmt::Repeat(body, self.expr), line)?;
                }
                Task::IfCond(line, arms) => {
                    let cond = self.expr;
                    self.account(input, |r, cx| r.condition(cond, cx))?;
                    self.expect(Kind::Then)?;
                    self.tasks.try_push(Task::IfArm(line, arms, self.expr))?;
                    self.tasks.try_push(Task::Block)?;
                }
                Task::IfArm(line, arms, cond) => {
                    let body = std::mem::take(&mut self.body);
                    self.scratch.arms.try_push((cond, body))?;
                    if self.kind() == Kind::ElseIf {
                        self.take();
                        self.tasks.try_push(Task::IfCond(line, arms))?;
                        self.tasks.try_push(Task::Expr(0))?;
                    } else if self.kind() == Kind::Else {
                        self.take();
                        self.tasks.try_push(Task::IfElse(line, arms))?;
                        self.tasks.try_push(Task::Block)?;
                    } else {
                        self.close(Kind::End, Kind::If, line)?;
                        let arms = publish(&mut self.ast.arms, &mut self.scratch.arms, arms)?;
                        self.statement(Stmt::If(arms, None), line)?;
                    }
                }
                Task::IfElse(line, arms) => {
                    self.close(Kind::End, Kind::If, line)?;
                    let body = std::mem::take(&mut self.body);
                    let arms = publish(&mut self.ast.arms, &mut self.scratch.arms, arms)?;
                    self.statement(Stmt::If(arms, Some(body)), line)?;
                }
                Task::ForName(line) => {
                    let name = self.expect(Kind::Name)?.text;
                    self.tasks.try_push(Task::ForChoice(line, name))?;
                }
                Task::ForChoice(line, name) => {
                    if self.kind() == Kind::Assign {
                        if let Some(events) = &mut self.native_events {
                            for name in [b"(for index)".as_slice(), b"(for limit)", b"(for step)"] {
                                events.try_push(NativeRootEvent::Anchor(Text::new(name)?))?;
                            }
                        }
                        self.check_local_count(usize::from(self.locals) + 4)?;
                        self.take();
                        self.tasks.try_push(Task::ForStart(line, name))?;
                        self.tasks.try_push(Task::Expr(0))?;
                    } else if matches!(self.kind(), Kind::Comma | Kind::In) {
                        if let Some(events) = &mut self.native_events {
                            for name in [
                                b"(for generator)".as_slice(),
                                b"(for state)",
                                b"(for control)",
                            ] {
                                events.try_push(NativeRootEvent::Anchor(Text::new(name)?))?;
                            }
                        }
                        let names = self.scratch.names.len();
                        self.scratch.names.try_push(name)?;
                        self.tasks.try_push(Task::ForNames(line, names))?;
                    } else {
                        return Err(self.error("'=' or 'in' expected"));
                    }
                }
                Task::ForStart(line, name) => {
                    let start = self.expr;
                    self.account(input, |r, cx| r.next(start, cx))?;
                    self.expect(Kind::Comma)?;
                    self.tasks.try_push(Task::ForLimit(line, name, self.expr))?;
                    self.tasks.try_push(Task::Expr(0))?;
                }
                Task::ForLimit(line, name, start) => {
                    let limit = self.expr;
                    self.account(input, |r, cx| r.next(limit, cx))?;
                    if self.kind() == Kind::Comma {
                        self.take();
                        self.tasks
                            .try_push(Task::ForStep(line, name, start, limit))?;
                        self.tasks.try_push(Task::Expr(0))?;
                    } else {
                        self.account(input, |r, cx| r.default_step(cx))?;
                        self.for_body(input, line, name, start, limit, None)?;
                    }
                }
                Task::ForStep(line, name, start, limit) => {
                    let step = self.expr;
                    self.account(input, |r, cx| r.next(step, cx))?;
                    self.for_body(input, line, name, start, limit, Some(step))?
                }
                Task::ForEnd(line, name, start, limit, step, do_line) => {
                    self.close(Kind::End, Kind::For, line)?;
                    self.loops -= 1;
                    self.locals -= 4;
                    self.resources.locals(self.locals);
                    self.bindings.truncate(self.locals);
                    let body = std::mem::take(&mut self.body);
                    self.statement(
                        Stmt::For {
                            name,
                            start,
                            limit,
                            step,
                            do_line,
                            body,
                        },
                        line,
                    )?;
                }
                Task::ForNames(line, names) => {
                    let count = self.scratch.names.len() - names;
                    self.check_local_count(usize::from(self.locals) + 3 + count)?;
                    if self.kind() == Kind::Comma {
                        self.take();
                        self.tasks.try_push(Task::ForMoreName(line, names))?;
                    } else {
                        self.expect(Kind::In)?;
                        self.tasks.try_push(Task::ForBegin(line, names))?;
                    }
                }
                Task::ForMoreName(line, names) => {
                    let name = self.expect(Kind::Name)?.text;
                    self.scratch.names.try_push(name)?;
                    self.tasks.try_push(Task::ForNames(line, names))?;
                }
                Task::ForBegin(line, names) => {
                    self.tasks
                        .try_push(Task::ForValues(line, names, self.current().end_line))?;
                    self.tasks.try_push(Task::List)?;
                }
                Task::ForValues(line, names, iter_line) => {
                    let values = std::mem::take(&mut self.list);
                    self.account(input, |r, cx| {
                        r.adjust(3, cx.ast.ids(values), cx)?;
                        r.check(3)
                    })?;
                    let do_line = self.expect(Kind::Do)?.end_line;
                    self.loops += 1;
                    let names = self.publish_names(names)?;
                    self.locals += names.len() as u16 + 3;
                    self.resources.locals(self.locals);
                    self.bindings.hidden(3)?;
                    self.declare_names(input, names)?;
                    self.tasks.try_push(Task::ForInEnd(
                        line,
                        names,
                        values,
                        [do_line, iter_line],
                    ))?;
                    self.tasks.try_push(Task::Block)?;
                }
                Task::ForInEnd(line, names, values, lines) => {
                    self.close(Kind::End, Kind::For, line)?;
                    self.loops -= 1;
                    self.locals -= names.len() as u16 + 3;
                    self.resources.locals(self.locals);
                    self.bindings.truncate(self.locals);
                    let body = std::mem::take(&mut self.body);
                    self.statement(Stmt::ForIn(names, values, body, lines), line)?;
                }
                Task::ExpressionStatement(line) => {
                    let first = self.expr;
                    if matches!(self.ast.exprs[first].0, Expr::Call(..) | Expr::Method(..)) {
                        self.statement(Stmt::Call(first), line)?;
                    } else {
                        let targets = self.scratch.ids.len();
                        self.scratch.ids.try_push(first)?;
                        self.tasks.try_push(Task::AssignTargets(line, targets))?;
                    }
                }
                Task::AssignTargets(line, targets) => {
                    let last = *self.scratch.ids[targets..]
                        .last()
                        .expect("assignment target");
                    if !matches!(self.ast.exprs[last].0, Expr::Name(_) | Expr::Index(..)) {
                        return Err(self.error("syntax error"));
                    }
                    if self.kind() == Kind::Comma {
                        self.take();
                        self.tasks.try_push(Task::AssignTarget(line, targets))?;
                        self.tasks.try_push(Task::Prefix)?;
                    } else {
                        if self.scratch.ids[targets..].iter().any(|&i| {
                            !matches!(self.ast.exprs[i].0, Expr::Name(_) | Expr::Index(..))
                        }) {
                            return Err(self.error("syntax error"));
                        }
                        self.expect(Kind::Assign)?;
                        self.tasks.try_push(Task::AssignEnd(line, targets))?;
                        self.tasks.try_push(Task::List)?;
                    }
                }
                Task::AssignTarget(line, targets) => {
                    let target = self.expr;
                    // Taking the scratch stack moves no elements and lets the
                    // accounting closure borrow the open target list.
                    let open = std::mem::take(&mut self.scratch.ids);
                    let conflict = self.account(input, |r, _| r.conflict(&open[targets..], target));
                    self.scratch.ids = open;
                    conflict?;
                    let limit = 200 - i32::from(self.base_depth.wrapping_add(self.depth));
                    if (self.scratch.ids.len() - targets) as i32 > limit {
                        return Err(self.limit_error(
                            limit,
                            "variables in assignment",
                            self.function_line,
                        ));
                    }
                    self.scratch.ids.try_push(self.expr)?;
                    self.tasks.try_push(Task::AssignTargets(line, targets))?;
                }
                Task::AssignEnd(line, targets) => {
                    let values = std::mem::take(&mut self.list);
                    let targets = self.publish_ids(targets)?;
                    self.account(input, |r, cx| {
                        r.assignment(cx.ast.ids(targets), cx.ast.ids(values), cx)
                    })?;
                    self.statement(Stmt::Assign(targets, values), line)?;
                }
                Task::List => {
                    self.tasks
                        .try_push(Task::ListAfter(self.scratch.ids.len()))?;
                    self.tasks.try_push(Task::Expr(0))?;
                }
                Task::ListAfter(values) => {
                    self.scratch.ids.try_push(self.expr)?;
                    if self.kind() == Kind::Comma {
                        let separator = self.take();
                        self.ast.exprs[self.expr].1.discharge = separator.end_line;
                        self.tasks.try_push(Task::ListAfter(values))?;
                        self.tasks.try_push(Task::Expr(0))?;
                        self.tasks.try_push(Task::ResourceNext(self.expr))?;
                    } else {
                        self.list = self.publish_ids(values)?;
                    }
                }
                Task::Function(line, method) => {
                    if let Some(events) = &mut self.native_events {
                        events.try_push(NativeRootEvent::Open)?;
                    }
                    self.expect(Kind::LParen)?;
                    let params = self.scratch.names.len();
                    if method {
                        self.scratch.names.try_push(Text::new(b"self")?)?;
                    }
                    self.tasks.try_push(Task::Params(line, params, true))?;
                }
                Task::Params(line, params, allow_end) => {
                    if allow_end
                        && self.scratch.names.len() > params
                        && let Some(events) = &mut self.native_events
                    {
                        events.try_push(NativeRootEvent::Anchor(Text::new(b"self")?))?;
                    }
                    if self.kind() == Kind::Dots {
                        self.take();
                        self.tasks.try_push(Task::ParamsEnd(line, params, true))?;
                    } else if self.kind() == Kind::RParen && allow_end {
                        self.start_function_body(input, line, params, false)?;
                    } else {
                        if self.kind() != Kind::Name {
                            return Err(self.error("<name> or '...' expected"));
                        }
                        let name = self.take().text;
                        self.scratch.names.try_push(name)?;
                        self.tasks.try_push(Task::ParamAfter(line, params))?;
                    }
                }
                Task::ParamAfter(line, params) => {
                    self.check_local_count_at(self.scratch.names.len() - params, line)?;
                    if self.kind() == Kind::Comma {
                        self.take();
                        self.tasks.try_push(Task::Params(line, params, false))?;
                    } else {
                        self.start_function_body(input, line, params, false)?;
                    }
                }
                Task::ParamsEnd(line, params, vararg) => {
                    if vararg && let Some(events) = &mut self.native_events {
                        events.try_push(NativeRootEvent::Anchor(Text::new(b"arg")?))?;
                    }
                    self.start_function_body(input, line, params, vararg)?
                }
                Task::FunctionEnd(
                    line,
                    params,
                    vararg,
                    loops,
                    parent_vararg,
                    locals,
                    parent_line,
                ) => {
                    let end_line = self.close(Kind::End, Kind::Function, line)?.end_line;
                    self.tasks.try_push(Task::FunctionAfterEnd(
                        line,
                        params,
                        vararg,
                        loops,
                        parent_vararg,
                        locals,
                        parent_line,
                        end_line,
                    ))?;
                }
                Task::FunctionAfterEnd(
                    line,
                    params,
                    vararg,
                    loops,
                    parent_vararg,
                    locals,
                    parent_line,
                    end_line,
                ) => {
                    if let Some(events) = &mut self.native_events {
                        events.try_push(NativeRootEvent::Close)?;
                        if let Some(token) = &self.token
                            && matches!(token.kind, Kind::Name | Kind::String)
                        {
                            events.try_push(NativeRootEvent::Anchor(token.text.try_clone()?))?;
                        }
                    }
                    self.loops = loops;
                    self.vararg = parent_vararg;
                    self.locals = locals;
                    self.function_line = parent_line;
                    self.bindings.leave();
                    let (sign, stack) = self.resources.leave();
                    self.ast.stacks.try_push((self.ast.exprs.len(), stack))?;
                    if let Some(sign) = sign {
                        self.ast.zero_signs.try_push((self.ast.exprs.len(), sign))?;
                    }
                    let body = std::mem::take(&mut self.body);
                    self.expression(
                        input,
                        Expr::Function {
                            params,
                            vararg,
                            body,
                            end_line,
                        },
                        line,
                    )?;
                }
                Task::Expr(min) => {
                    self.enter()?;
                    let line = self.line();
                    self.tasks.try_push(Task::ExprLoop(min, line))?;
                    match self.kind() {
                        Kind::Not | Kind::Minus | Kind::Hash => {
                            let op = self.take().kind;
                            self.tasks.try_push(Task::Unary(op, line))?;
                            self.tasks.try_push(Task::Expr(7))?;
                        }
                        Kind::Nil => {
                            self.take();
                            self.expression(input, Expr::Nil, line)?;
                        }
                        Kind::True | Kind::False => {
                            let b = self.take().kind == Kind::True;
                            self.expression(input, Expr::Boolean(b), line)?;
                        }
                        Kind::Number => {
                            let n = self.take().number;
                            self.expression(input, Expr::Number(n), line)?;
                        }
                        Kind::String => {
                            let s = self.take().text;
                            self.expression(input, Expr::String(s), line)?;
                        }
                        Kind::Dots => {
                            if !self.vararg {
                                return Err(
                                    self.error("cannot use '...' outside a vararg function")
                                );
                            }
                            self.take();
                            self.expression(input, Expr::Vararg, line)?;
                        }
                        Kind::Function => {
                            self.take();
                            self.tasks.try_push(Task::Function(line, false))?;
                        }
                        Kind::LBrace => self.tasks.try_push(Task::Table)?,
                        _ => self.tasks.try_push(Task::Prefix)?,
                    }
                }
                Task::ExprLoop(min, line) => {
                    if let Some((left, right)) = precedence(self.kind())
                        && left > min
                    {
                        let token = self.take();
                        self.tasks.try_push(Task::Binary(
                            min,
                            line,
                            self.expr,
                            token.kind,
                            token.end_line,
                        ))?;
                        self.tasks.try_push(Task::Expr(right))?;
                        self.tasks
                            .try_push(Task::ResourceInfix(self.expr, token.kind))?;
                    } else {
                        self.depth -= 1;
                    }
                }
                Task::Unary(op, line) => {
                    self.expression(input, Expr::Unary(op, self.expr), line)?;
                }
                Task::Binary(min, line, left, op, op_line) => {
                    self.expression(input, Expr::Binary(op, left, self.expr, op_line), line)?;
                    self.tasks.try_push(Task::ExprLoop(min, line))?;
                }
                Task::Prefix => {
                    let line = self.line();
                    if self.kind() == Kind::Name {
                        let name = self.take().text;
                        let id = self.expression(input, Expr::Name(name), line)?;
                        self.tasks.try_push(Task::Suffix(line, id))?;
                        self.tasks.try_push(Task::ResolveName(id))?;
                    } else if self.kind() == Kind::LParen {
                        self.take();
                        self.tasks.try_push(Task::Paren(line))?;
                        self.tasks.try_push(Task::Expr(0))?;
                    } else {
                        return Err(self.error("unexpected symbol"));
                    }
                }
                Task::ResolveName(id) => {
                    let Expr::Name(name) = &self.ast.exprs[id].0 else {
                        unreachable!()
                    };
                    let reference = self.bindings.reference(input, name).map_err(|error| {
                        if error.kind() == ErrorKind::Memory {
                            error
                        } else {
                            self.limit_error(60, "upvalues", error.line())
                        }
                    })?;
                    self.account(input, |r, cx| r.resolve(id, reference, cx))?;
                }
                Task::Paren(line) => {
                    self.close(Kind::RParen, Kind::LParen, line)?;
                    self.tasks.try_push(Task::ParenEnd(line))?;
                }
                Task::ParenEnd(line) => {
                    let id = self.expression(input, Expr::Paren(self.expr), line)?;
                    self.tasks.try_push(Task::Suffix(line, id))?;
                }
                Task::Suffix(line, id) => match self.kind() {
                    Kind::Dot => {
                        self.account(input, |r, cx| r.any(id, cx))?;
                        self.take();
                        self.tasks.try_push(Task::Field(line, id))?;
                    }
                    Kind::LBracket => {
                        self.account(input, |r, cx| r.any(id, cx))?;
                        self.take();
                        self.tasks.try_push(Task::Index(line, id))?;
                        self.tasks.try_push(Task::Expr(0))?;
                    }
                    Kind::Colon => {
                        self.take();
                        self.tasks.try_push(Task::MethodName(line, id))?;
                    }
                    Kind::LParen | Kind::LBrace | Kind::String => {
                        self.account(input, |r, cx| r.next(id, cx))?;
                        self.tasks
                            .try_push(Task::CallEnd(line, id, self.current().end_line))?;
                        self.tasks.try_push(Task::Args)?;
                    }
                    _ => self.expr = id,
                },
                Task::Field(line, id) => {
                    let name = self.expect(Kind::Name)?.text;
                    self.tasks.try_push(Task::FieldName(line, id, name, None))?;
                }
                Task::FieldName(line, id, name, method) => {
                    let key = self.expression(input, Expr::String(name), line)?;
                    self.tasks.try_push(match method {
                        Some(method) => Task::FunctionFieldEnd(line, id, key, method),
                        None => Task::IndexEnd(line, id, key),
                    })?;
                }
                Task::Index(line, id) => {
                    let key = self.expr;
                    self.account(input, |r, cx| r.val(key, cx))?;
                    self.expect(Kind::RBracket)?;
                    self.tasks.try_push(Task::IndexEnd(line, id, key))?;
                }
                Task::IndexEnd(line, id, key) => {
                    let field = self.expression(input, Expr::Index(id, key), line)?;
                    self.tasks.try_push(Task::Suffix(line, field))?;
                }
                Task::MethodName(line, id) => {
                    let name = self.expect(Kind::Name)?.text;
                    self.tasks.try_push(Task::MethodArgs(line, id, name))?;
                }
                Task::MethodArgs(line, id, name) => {
                    self.account(input, |r, cx| r.method(id, &name, cx))?;
                    self.tasks.try_push(Task::MethodEnd(
                        line,
                        id,
                        name,
                        self.current().end_line,
                    ))?;
                    self.tasks.try_push(Task::Args)?;
                }
                Task::MethodEnd(line, id, name, call_line) => {
                    let args = std::mem::take(&mut self.list);
                    let call =
                        self.expression(input, Expr::Method(id, name, args, call_line), line)?;
                    self.tasks.try_push(Task::Suffix(line, call))?;
                }
                Task::CallEnd(line, id, call_line) => {
                    let args = std::mem::take(&mut self.list);
                    let call = self.expression(input, Expr::Call(id, args, call_line), line)?;
                    self.tasks.try_push(Task::Suffix(line, call))?;
                }
                Task::Args => match self.kind() {
                    Kind::LParen => {
                        if self.line() != self.previous_line {
                            return Err(
                                self.error("ambiguous syntax (function call x new statement)")
                            );
                        }
                        let line = self.take().end_line;
                        self.tasks.try_push(Task::ArgsParen(line))?;
                    }
                    Kind::LBrace => {
                        self.tasks.try_push(Task::ArgsTable)?;
                        self.tasks.try_push(Task::Table)?;
                    }
                    Kind::String => {
                        let token = self.take();
                        let id = self.expression(input, Expr::String(token.text), token.line)?;
                        self.list = self.one(id)?;
                    }
                    _ => return Err(self.error("function arguments expected")),
                },
                Task::ArgsParen(line) => {
                    if self.kind() == Kind::RParen {
                        self.take();
                        self.list = Span::default();
                    } else {
                        self.tasks.try_push(Task::ArgsEnd(line))?;
                        self.tasks.try_push(Task::List)?;
                    }
                }
                Task::ArgsEnd(line) => {
                    let values = self.list;
                    self.account(input, |r, cx| r.arguments(cx.ast.ids(values)))?;
                    self.close(Kind::RParen, Kind::LParen, line)?;
                }
                Task::ArgsTable => self.list = self.one(self.expr)?,
                Task::Table => {
                    self.account(input, |r, _| r.begin_table())?;
                    let previous = self.previous_line;
                    let line = self.expect(Kind::LBrace)?.line;
                    self.tasks
                        .try_push(Task::TableNext([line, previous], self.scratch.fields.len()))?;
                }
                Task::TableNext(line, fields) => {
                    if self.kind() == Kind::RBrace {
                        if let Some((None, value)) = self.last_field(fields) {
                            self.ast.exprs[value].1.discharge = self.current().end_line;
                        }
                        self.take();
                        self.tasks.try_push(Task::TableEnd(line, fields))?;
                    } else if self.kind() == Kind::Name
                        && self
                            .lookahead
                            .as_ref()
                            .is_some_and(|t| t.kind == Kind::Assign)
                    {
                        let token = self.take();
                        let key = self.expression(input, Expr::String(token.text), token.line)?;
                        self.tasks.try_push(Task::TableAssign(line, fields, key))?;
                    } else if self.kind() == Kind::LBracket {
                        self.take();
                        self.tasks.try_push(Task::TableKey(line, fields))?;
                        self.tasks.try_push(Task::Expr(0))?;
                    } else {
                        self.tasks.try_push(Task::TableValue(line, fields, None))?;
                        self.tasks.try_push(Task::Expr(0))?;
                    }
                }
                Task::TableKey(line, fields) => {
                    let key = self.expr;
                    self.account(input, |r, cx| r.val(key, cx))?;
                    self.tasks.try_push(Task::TableAssign(line, fields, key))?;
                    self.tasks.try_push(Task::Expect(Kind::RBracket))?;
                }
                Task::TableAssign(line, fields, key) => {
                    self.expect(Kind::Assign)?;
                    self.tasks
                        .try_push(Task::TableValue(line, fields, Some(key)))?;
                    self.tasks.try_push(Task::Expr(0))?;
                    self.tasks.try_push(Task::ResourceKey(key))?;
                }
                Task::TableValue(line, fields, key) => {
                    let value = self.expr;
                    self.account(input, |r, cx| r.field(key, value, cx))?;
                    self.scratch.fields.try_push((key, self.expr))?;
                    self.tasks.try_push(Task::TableSeparator(line, fields))?;
                }
                Task::TableSeparator(line, fields) => {
                    if matches!(self.kind(), Kind::Comma | Kind::Semi) {
                        let separator = self.take();
                        if let Some((None, value)) = self.last_field(fields) {
                            self.ast.exprs[value].1.discharge = separator.end_line;
                        }
                        self.tasks.try_push(Task::TableNext(line, fields))?;
                    } else {
                        if let Some((None, value)) = self.last_field(fields) {
                            self.ast.exprs[value].1.discharge = self.current().end_line;
                        }
                        self.close(Kind::RBrace, Kind::LBrace, line[0])?;
                        self.tasks.try_push(Task::TableEnd(line, fields))?;
                    }
                }
                Task::TableEnd(line, fields) => {
                    let fields = publish(&mut self.ast.fields, &mut self.scratch.fields, fields)?;
                    self.expression(input, Expr::Table(fields, line[1]), line[0])?;
                }
                Task::ResourceNext(id) => self.account(input, |r, cx| r.next(id, cx))?,
                Task::ResourceInfix(id, op) => self.account(input, |r, cx| r.infix(id, op, cx))?,
                Task::ResourceKey(id) => self.account(input, |r, cx| r.record_key(id, cx))?,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type Outcome = Result<String, (ErrorKind, u32, String)>;
    fn streamed(source: &[u8], ends: impl IntoIterator<Item = usize>) -> Outcome {
        let mut parser = Parser::try_new().unwrap();
        for end in ends {
            match parser.resume(&source[..end], end == source.len()) {
                Ok(Some(ast)) => return Ok(format!("{ast:?}")),
                Ok(None) => {}
                Err(e) => return Err((e.kind(), e.line(), e.message().into())),
            }
        }
        panic!("input never closed")
    }
    #[test]
    fn grammar_resumes_across_all_split_positions() {
        let corpus:&[&[u8]]=&[
            b"",b"-- eof",b"local a,b=1,2;a,b=b,a;do local x={} end;return a,b",
            b"local function f(a,...)local x={...};return function()return a,x end end;return f(1,2,3)",
            b"local a={b={}};function a.b:f(x,...) return self,x,... end;return a.b:f 'hello'",
            b"local s=0;for i=1,10,2 do if i<2 then s=s+i elseif i==3 then break else s=s-1 end end;while s<1 do s=s+1 end;repeat s=s-1 until s<0;return s",
            b"for a,b,c in pairs({x=1}),nil,nil do do local d=a..b end end;return {x=1,[2]=3,4;f(5),}",
            b"return -2^2+3*4,not false and 1 or 2,('abc'):sub(1,2),f{1,2}.x[g()].y:method(a,b)",
            b"return {a,b=2,c(),[f()]=g(),h:i(),[1]=5};",b"return function(a,...)return ... end",
            b"local 1",b"(a)=2",b"break",b"while true do local f=function()break end end",
            b"return function()return ... end",b"f\n()",b"return {x=}",b"local function f(a,)end",b"return (1+)",
        ];
        for &source in corpus {
            let expected = streamed(source, [source.len()]);
            for split in 0..=source.len() {
                assert_eq!(
                    streamed(source, [split, source.len()]),
                    expected,
                    "split={split}: {source:?}"
                );
            }
            assert_eq!(
                streamed(source, 0..=source.len()),
                expected,
                "one-byte delivery: {source:?}"
            );
        }
    }
    #[test]
    fn generated_bytes_and_large_lists_do_not_require_parser_replay() {
        let mut random = 0x19f1_53eau32;
        for case in 0..2000 {
            let mut source = Vec::new();
            for _ in 0..case % 65 {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                source.push((random >> 24) as u8);
            }
            assert_eq!(
                streamed(&source, 0..=source.len()),
                streamed(&source, [source.len()])
            );
        }
        let source = format!("local t={{{}}};return #t", "1,".repeat(10_000));
        assert_eq!(
            streamed(source.as_bytes(), 0..=source.len()),
            streamed(source.as_bytes(), [source.len()])
        );
    }
    #[test]
    fn syntax_errors_stop_before_requesting_unused_input() {
        let mut vm = crate::Vm::new();
        let program = crate::compile(
            br#"
                local first=...;local n=0
                pcall(load,function()n=n+1;if n==1 then return first end;error('must not read')end)
                return n
            "#,
        )
        .unwrap();
        for source in [
            b") ".as_slice(),
            b"local 1 ",
            b"break; ",
            b"return function()return ... ",
        ] {
            let mut parser = Parser::try_new().unwrap();
            assert!(parser.resume(source, false).is_err(), "{source:?}");
            let first = vm.intern(source);
            let count = vm.execute_with_args(&program, &[first]).unwrap();
            assert_eq!(count, &[crate::Value::Number(1.)], "{source:?}");
        }
    }
}
