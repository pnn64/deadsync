//! Flat grammar continuations for fixed and reader-supplied source.
use super::*;
use crate::lexer::Scanner;

#[derive(Debug)]
enum Task {
    RootEnd,
    Expect(Kind),
    Block,
    BlockNext(Vec<StmtId>),
    BlockAfter(Vec<StmtId>, bool),
    BlockEnd(Vec<StmtId>),
    RestoreLocals(u16),
    Statement,
    Local(u32),
    LocalName(u32, Vec<Box<[u8]>>),
    LocalNames(u32, Vec<Box<[u8]>>),
    LocalEnd(u32, Vec<Box<[u8]>>),
    LocalFunctionName(u32),
    LocalFunctionBody(u32, Box<[u8]>),
    LocalFunctionEnd(u32, Box<[u8]>),
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
    IfCond(u32, Vec<(ExprId, Block)>),
    IfArm(u32, Vec<(ExprId, Block)>, ExprId),
    IfElse(u32, Vec<(ExprId, Block)>),
    ForName(u32),
    ForChoice(u32, Box<[u8]>),
    ForStart(u32, Box<[u8]>),
    ForLimit(u32, Box<[u8]>, ExprId),
    ForStep(u32, Box<[u8]>, ExprId, ExprId),
    ForEnd(u32, Box<[u8]>, ExprId, ExprId, Option<ExprId>),
    ForNames(u32, Vec<Box<[u8]>>),
    ForMoreName(u32, Vec<Box<[u8]>>),
    ForValues(u32, Vec<Box<[u8]>>),
    ForInEnd(u32, Vec<Box<[u8]>>, Vec<ExprId>),
    ExpressionStatement(u32),
    AssignTargets(u32, Vec<ExprId>),
    AssignTarget(u32, Vec<ExprId>),
    AssignEnd(u32, Vec<ExprId>),
    List,
    ListAfter(Vec<ExprId>),
    Function(u32, bool),
    Params(u32, Vec<Box<[u8]>>, bool),
    ParamAfter(u32, Vec<Box<[u8]>>),
    ParamsEnd(u32, Vec<Box<[u8]>>, bool),
    FunctionEnd(u32, Vec<Box<[u8]>>, bool, u16, bool, u16),
    Expr(u8),
    ExprLoop(u8, u32),
    Unary(Kind, u32),
    Binary(u8, u32, ExprId, Kind),
    Prefix,
    Paren(u32),
    Suffix(u32, ExprId),
    Field(u32, ExprId),
    Index(u32, ExprId),
    MethodName(u32, ExprId),
    MethodEnd(u32, ExprId, Box<[u8]>),
    CallEnd(u32, ExprId),
    Args,
    ArgsParen,
    ArgsEnd,
    ArgsTable,
    Table,
    TableNext(u32, Vec<(Option<ExprId>, ExprId)>),
    TableAssign(u32, Vec<(Option<ExprId>, ExprId)>, ExprId),
    TableKey(u32, Vec<(Option<ExprId>, ExprId)>),
    TableValue(u32, Vec<(Option<ExprId>, ExprId)>, Option<ExprId>),
    TableSeparator(u32, Vec<(Option<ExprId>, ExprId)>),
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
    loops: u16,
    vararg: bool,
    locals: u16,
    tasks: Vec<Task>,
    ast: Ast,
    expr: ExprId,
    stmt: StmtId,
    list: Vec<ExprId>,
    body: Block,
    finished: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            scanner: Scanner::default(),
            token: None,
            lookahead: None,
            previous_line: 1,
            depth: 0,
            loops: 0,
            vararg: true,
            locals: 0,
            tasks: vec![Task::RootEnd, Task::Block],
            ast: Ast::default(),
            expr: 0,
            stmt: 0,
            list: Vec::new(),
            body: Box::default(),
            finished: false,
        }
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
    pub(crate) fn nesting(&self) -> usize {
        usize::from(self.depth)
    }
    pub(crate) fn arena_bytes(&self) -> usize {
        self.tasks.capacity() * std::mem::size_of::<Task>()
            + self.ast.exprs.capacity() * std::mem::size_of::<(Expr, u32)>()
            + self.ast.stmts.capacity() * std::mem::size_of::<(Stmt, u32)>()
            + self.scanner.heap_bytes()
    }
    pub(crate) fn heap_bytes(&self) -> usize {
        fn names(v: &[Box<[u8]>]) -> usize {
            std::mem::size_of_val(v) + v.iter().map(|n| n.len()).sum::<usize>()
        }
        fn block(v: &[StmtId]) -> usize {
            std::mem::size_of_val(v)
        }
        let mut bytes = self.arena_bytes()
            + self.list.capacity() * std::mem::size_of::<ExprId>()
            + block(&self.body);
        bytes += self.token.as_ref().map_or(0, |t| t.text.len())
            + self.lookahead.as_ref().map_or(0, |t| t.text.len());
        for (expr, _) in &self.ast.exprs {
            bytes += match expr {
                Expr::String(s) | Expr::Name(s) => s.len(),
                Expr::Table(v) => std::mem::size_of_val(v.as_ref()),
                Expr::Call(_, v) => std::mem::size_of_val(v.as_ref()),
                Expr::Method(_, n, v) => n.len() + std::mem::size_of_val(v.as_ref()),
                Expr::Function { params, body, .. } => names(params) + block(body),
                _ => 0,
            };
        }
        for (stmt, _) in &self.ast.stmts {
            bytes += match stmt {
                Stmt::Local(n, v) => names(n) + std::mem::size_of_val(v.as_ref()),
                Stmt::LocalFunction(n, _) => n.len(),
                Stmt::Assign(a, b) => {
                    std::mem::size_of_val(a.as_ref()) + std::mem::size_of_val(b.as_ref())
                }
                Stmt::Return(v) => std::mem::size_of_val(v.as_ref()),
                Stmt::Do(b) | Stmt::While(_, b) | Stmt::Repeat(b, _) => block(b),
                Stmt::If(a, b) => {
                    std::mem::size_of_val(a.as_ref())
                        + a.iter().map(|(_, b)| block(b)).sum::<usize>()
                        + block(b)
                }
                Stmt::For { name, body, .. } => name.len() + block(body),
                Stmt::ForIn(n, v, b) => names(n) + std::mem::size_of_val(v.as_ref()) + block(b),
                _ => 0,
            };
        }
        for task in &self.tasks {
            bytes += match task {
                Task::BlockNext(v)
                | Task::BlockAfter(v, _)
                | Task::BlockEnd(v)
                | Task::AssignTargets(_, v)
                | Task::AssignTarget(_, v)
                | Task::AssignEnd(_, v)
                | Task::ListAfter(v) => v.capacity() * std::mem::size_of::<usize>(),
                Task::LocalName(_, v)
                | Task::LocalNames(_, v)
                | Task::LocalEnd(_, v)
                | Task::ForNames(_, v)
                | Task::ForMoreName(_, v)
                | Task::ForValues(_, v)
                | Task::Params(_, v, _)
                | Task::ParamAfter(_, v)
                | Task::ParamsEnd(_, v, _)
                | Task::FunctionEnd(_, v, ..) => {
                    names(v) + (v.capacity() - v.len()) * std::mem::size_of::<Box<[u8]>>()
                }
                Task::LocalFunctionEnd(_, n)
                | Task::LocalFunctionBody(_, n)
                | Task::ForChoice(_, n)
                | Task::ForStart(_, n)
                | Task::ForLimit(_, n, _)
                | Task::ForStep(_, n, ..)
                | Task::ForEnd(_, n, ..)
                | Task::MethodEnd(_, _, n) => n.len(),
                Task::RepeatEnd(_, b) => block(b),
                Task::IfCond(_, v) | Task::IfArm(_, v, _) | Task::IfElse(_, v) => {
                    v.capacity() * std::mem::size_of::<(ExprId, Block)>()
                        + v.iter().map(|(_, b)| block(b)).sum::<usize>()
                }
                Task::ForInEnd(_, n, v) => {
                    names(n)
                        + (n.capacity() - n.len()) * std::mem::size_of::<Box<[u8]>>()
                        + v.capacity() * std::mem::size_of::<ExprId>()
                }
                Task::TableNext(_, v)
                | Task::TableAssign(_, v, _)
                | Task::TableKey(_, v)
                | Task::TableValue(_, v, _)
                | Task::TableSeparator(_, v) => {
                    v.capacity() * std::mem::size_of::<(Option<ExprId>, ExprId)>()
                }
                _ => 0,
            };
        }
        bytes
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
    fn error(&self, text: &str) -> Error {
        Error::new(ErrorKind::Syntax, self.line(), text)
    }
    fn enter(&mut self) -> Result<(), Error> {
        if self.depth >= 120 {
            return Err(Error::new(
                ErrorKind::Limit,
                self.line(),
                "syntax nesting limit exceeded",
            ));
        }
        self.depth += 1;
        Ok(())
    }
    fn check_local_count(&self, total: usize) -> Result<(), Error> {
        if total > 200 {
            return Err(Error::new(
                ErrorKind::Limit,
                self.line(),
                "too many local variables",
            ));
        }
        Ok(())
    }
    fn take(&mut self) -> Token {
        let token = self
            .token
            .take()
            .expect("grammar consumes its current token");
        self.previous_line = token.line;
        self.token = self.lookahead.take();
        token
    }
    fn expect(&mut self, kind: Kind) -> Result<Token, Error> {
        if self.kind() != kind {
            return Err(self.error(&format!("expected {kind:?}, found {:?}", self.kind())));
        }
        Ok(self.take())
    }
    fn end_block(&self) -> bool {
        matches!(
            self.kind(),
            Kind::Eof | Kind::End | Kind::Else | Kind::ElseIf | Kind::Until
        )
    }
    fn expression(&mut self, expr: Expr, line: u32) -> ExprId {
        let id = self.ast.exprs.len();
        self.ast.exprs.push((expr, line));
        self.expr = id;
        id
    }
    fn statement(&mut self, stmt: Stmt, line: u32) {
        self.stmt = self.ast.stmts.len();
        self.ast.stmts.push((stmt, line));
    }
    fn start_function_body(
        &mut self,
        line: u32,
        params: Vec<Box<[u8]>>,
        vararg: bool,
    ) -> Result<(), Error> {
        let count = params.len() + usize::from(vararg);
        self.check_local_count(count)?;
        self.expect(Kind::RParen)?;
        self.tasks.push(Task::FunctionEnd(
            line,
            params,
            vararg,
            self.loops,
            self.vararg,
            self.locals,
        ));
        self.loops = 0;
        self.vararg = vararg;
        self.locals = count as u16;
        self.tasks.push(Task::Block);
        Ok(())
    }
    fn for_body(
        &mut self,
        line: u32,
        name: Box<[u8]>,
        start: ExprId,
        limit: ExprId,
        step: Option<ExprId>,
    ) -> Result<(), Error> {
        self.expect(Kind::Do)?;
        self.loops += 1;
        self.locals += 4;
        self.tasks
            .push(Task::ForEnd(line, name, start, limit, step));
        self.tasks.push(Task::Block);
        Ok(())
    }

    /// Supply the entire retained prefix and whether the reader has reached EOF.
    /// `None` requests more source; `Some` contains the finished flat AST.
    pub(crate) fn resume(&mut self, input: &[u8], closed: bool) -> Result<Option<Ast>, Error> {
        assert!(!self.finished, "a completed parser cannot be resumed");
        loop {
            if self.tasks.is_empty() {
                self.finished = true;
                return Ok(Some(std::mem::take(&mut self.ast)));
            }
            // Advance fetches the next token before continuing grammar work.
            // Additional lookahead is requested only for table record fields.
            if self.token.is_none() {
                self.token = self.scanner.next(input, closed)?;
                if self.token.is_none() {
                    return Ok(None);
                }
            }
            if matches!(self.tasks.last(), Some(Task::TableNext(..)))
                && self.kind() == Kind::Name
                && self.lookahead.is_none()
            {
                self.lookahead = self.scanner.next(input, closed)?;
                if self.lookahead.is_none() {
                    return Ok(None);
                }
            }
            let task = self.tasks.pop().expect("grammar continuation exists");
            match task {
                Task::RootEnd => {
                    self.expect(Kind::Eof)?;
                    self.ast.body = std::mem::take(&mut self.body);
                }
                Task::Expect(kind) => {
                    self.expect(kind)?;
                }
                Task::Block => {
                    self.enter()?;
                    self.tasks.push(Task::RestoreLocals(self.locals));
                    self.tasks.push(Task::BlockNext(Vec::new()));
                }
                Task::BlockNext(stmts) => {
                    if self.end_block() {
                        self.tasks.push(Task::BlockEnd(stmts));
                    } else {
                        let terminal = matches!(self.kind(), Kind::Break | Kind::Return);
                        self.tasks.push(Task::BlockAfter(stmts, terminal));
                        self.tasks.push(Task::Statement);
                    }
                }
                Task::BlockAfter(mut stmts, terminal) => {
                    stmts.push(self.stmt);
                    if self.kind() == Kind::Semi {
                        self.take();
                    }
                    self.tasks.push(if terminal {
                        Task::BlockEnd(stmts)
                    } else {
                        Task::BlockNext(stmts)
                    });
                }
                Task::BlockEnd(stmts) => {
                    self.depth -= 1;
                    self.body = stmts.into_boxed_slice();
                }
                Task::RestoreLocals(count) => self.locals = count,
                Task::Statement => {
                    let line = self.line();
                    match self.kind() {
                        Kind::Local => {
                            self.take();
                            self.tasks.push(Task::Local(line));
                        }
                        Kind::Function => {
                            self.take();
                            self.tasks.push(Task::FunctionName(line));
                        }
                        Kind::Return => {
                            self.take();
                            self.tasks.push(Task::Return(line));
                        }
                        Kind::Break => {
                            self.take();
                            self.tasks.push(Task::Break(line));
                        }
                        Kind::Do => {
                            self.take();
                            self.tasks.push(Task::DoEnd(line));
                            self.tasks.push(Task::Block);
                        }
                        Kind::While => {
                            self.take();
                            self.tasks.push(Task::WhileCond(line));
                            self.tasks.push(Task::Expr(0));
                        }
                        Kind::Repeat => {
                            self.take();
                            self.loops += 1;
                            self.tasks.push(Task::RepeatBody(line));
                            self.tasks.push(Task::Block);
                        }
                        Kind::If => {
                            self.take();
                            self.tasks.push(Task::IfCond(line, Vec::new()));
                            self.tasks.push(Task::Expr(0));
                        }
                        Kind::For => {
                            self.take();
                            self.tasks.push(Task::ForName(line));
                        }
                        _ => {
                            self.tasks.push(Task::ExpressionStatement(line));
                            self.tasks.push(Task::Prefix);
                        }
                    }
                }
                Task::Local(line) => {
                    if self.kind() == Kind::Function {
                        self.take();
                        self.tasks.push(Task::LocalFunctionName(line));
                    } else {
                        let name = self.expect(Kind::Name)?.text;
                        self.tasks.push(Task::LocalNames(line, vec![name]));
                    }
                }
                Task::LocalName(line, mut names) => {
                    names.push(self.expect(Kind::Name)?.text);
                    self.tasks.push(Task::LocalNames(line, names));
                }
                Task::LocalNames(line, names) => {
                    self.check_local_count(usize::from(self.locals) + names.len())?;
                    if self.kind() == Kind::Comma {
                        self.take();
                        self.tasks.push(Task::LocalName(line, names));
                    } else if self.kind() == Kind::Assign {
                        self.take();
                        self.tasks.push(Task::LocalEnd(line, names));
                        self.tasks.push(Task::List);
                    } else {
                        self.locals += names.len() as u16;
                        self.statement(Stmt::Local(names.into_boxed_slice(), Box::default()), line);
                    }
                }
                Task::LocalEnd(line, names) => {
                    self.locals += names.len() as u16;
                    let values = std::mem::take(&mut self.list).into_boxed_slice();
                    self.statement(Stmt::Local(names.into_boxed_slice(), values), line);
                }
                Task::LocalFunctionName(line) => {
                    let name = self.expect(Kind::Name)?.text;
                    self.tasks.push(Task::LocalFunctionBody(line, name));
                }
                Task::LocalFunctionBody(line, name) => {
                    self.check_local_count(usize::from(self.locals) + 1)?;
                    self.locals += 1;
                    self.tasks.push(Task::LocalFunctionEnd(line, name));
                    self.tasks.push(Task::Function(line, false));
                }
                Task::LocalFunctionEnd(line, name) => {
                    self.statement(Stmt::LocalFunction(name, self.expr), line)
                }
                Task::FunctionName(line) => {
                    let name = self.expect(Kind::Name)?.text;
                    let id = self.expression(Expr::Name(name), line);
                    self.tasks.push(Task::FunctionTarget(line, id));
                }
                Task::FunctionTarget(line, id) => {
                    if matches!(self.kind(), Kind::Dot | Kind::Colon) {
                        let method = self.take().kind == Kind::Colon;
                        self.tasks.push(Task::FunctionField(line, id, method));
                    } else {
                        self.tasks.push(Task::FunctionAssign(line, id));
                        self.tasks.push(Task::Function(line, false));
                    }
                }
                Task::FunctionField(line, id, method) => {
                    let name = self.expect(Kind::Name)?.text;
                    let key = self.expression(Expr::String(name), line);
                    let target = self.expression(Expr::Index(id, key), line);
                    if method {
                        self.tasks.push(Task::FunctionAssign(line, target));
                        self.tasks.push(Task::Function(line, true));
                    } else {
                        self.tasks.push(Task::FunctionTarget(line, target));
                    }
                }
                Task::FunctionAssign(line, target) => self.statement(
                    Stmt::Assign(
                        vec![target].into_boxed_slice(),
                        vec![self.expr].into_boxed_slice(),
                    ),
                    line,
                ),
                Task::Return(line) => {
                    if self.end_block() || self.kind() == Kind::Semi {
                        self.statement(Stmt::Return(Box::default()), line);
                    } else {
                        self.tasks.push(Task::ReturnEnd(line));
                        self.tasks.push(Task::List);
                    }
                }
                Task::ReturnEnd(line) => {
                    let values = std::mem::take(&mut self.list).into_boxed_slice();
                    self.statement(Stmt::Return(values), line);
                }
                Task::Break(line) => {
                    if self.loops == 0 {
                        return Err(self.error("no loop to break"));
                    }
                    self.statement(Stmt::Break, line);
                }
                Task::DoEnd(line) => {
                    self.expect(Kind::End)?;
                    let body = std::mem::take(&mut self.body);
                    self.statement(Stmt::Do(body), line);
                }
                Task::WhileCond(line) => {
                    let cond = self.expr;
                    self.expect(Kind::Do)?;
                    self.loops += 1;
                    self.tasks.push(Task::WhileEnd(line, cond));
                    self.tasks.push(Task::Block);
                }
                Task::WhileEnd(line, cond) => {
                    self.expect(Kind::End)?;
                    self.loops -= 1;
                    let body = std::mem::take(&mut self.body);
                    self.statement(Stmt::While(cond, body), line);
                }
                Task::RepeatBody(line) => {
                    self.expect(Kind::Until)?;
                    let body = std::mem::take(&mut self.body);
                    self.tasks.push(Task::RepeatEnd(line, body));
                    self.tasks.push(Task::Expr(0));
                }
                Task::RepeatEnd(line, body) => {
                    self.loops -= 1;
                    self.statement(Stmt::Repeat(body, self.expr), line);
                }
                Task::IfCond(line, arms) => {
                    self.expect(Kind::Then)?;
                    self.tasks.push(Task::IfArm(line, arms, self.expr));
                    self.tasks.push(Task::Block);
                }
                Task::IfArm(line, mut arms, cond) => {
                    arms.push((cond, std::mem::take(&mut self.body)));
                    if self.kind() == Kind::ElseIf {
                        self.take();
                        self.tasks.push(Task::IfCond(line, arms));
                        self.tasks.push(Task::Expr(0));
                    } else if self.kind() == Kind::Else {
                        self.take();
                        self.tasks.push(Task::IfElse(line, arms));
                        self.tasks.push(Task::Block);
                    } else {
                        self.expect(Kind::End)?;
                        self.statement(Stmt::If(arms.into_boxed_slice(), Box::default()), line);
                    }
                }
                Task::IfElse(line, arms) => {
                    self.expect(Kind::End)?;
                    let body = std::mem::take(&mut self.body);
                    self.statement(Stmt::If(arms.into_boxed_slice(), body), line);
                }
                Task::ForName(line) => {
                    let name = self.expect(Kind::Name)?.text;
                    self.tasks.push(Task::ForChoice(line, name));
                }
                Task::ForChoice(line, name) => {
                    if self.kind() == Kind::Assign {
                        self.check_local_count(usize::from(self.locals) + 4)?;
                        self.take();
                        self.tasks.push(Task::ForStart(line, name));
                        self.tasks.push(Task::Expr(0));
                    } else if matches!(self.kind(), Kind::Comma | Kind::In) {
                        self.tasks.push(Task::ForNames(line, vec![name]));
                    } else {
                        return Err(self.error("'=' or 'in' expected"));
                    }
                }
                Task::ForStart(line, name) => {
                    self.expect(Kind::Comma)?;
                    self.tasks.push(Task::ForLimit(line, name, self.expr));
                    self.tasks.push(Task::Expr(0));
                }
                Task::ForLimit(line, name, start) => {
                    let limit = self.expr;
                    if self.kind() == Kind::Comma {
                        self.take();
                        self.tasks.push(Task::ForStep(line, name, start, limit));
                        self.tasks.push(Task::Expr(0));
                    } else {
                        self.for_body(line, name, start, limit, None)?;
                    }
                }
                Task::ForStep(line, name, start, limit) => {
                    self.for_body(line, name, start, limit, Some(self.expr))?
                }
                Task::ForEnd(line, name, start, limit, step) => {
                    self.expect(Kind::End)?;
                    self.loops -= 1;
                    self.locals -= 4;
                    let body = std::mem::take(&mut self.body);
                    self.statement(
                        Stmt::For {
                            name,
                            start,
                            limit,
                            step,
                            body,
                        },
                        line,
                    );
                }
                Task::ForNames(line, names) => {
                    self.check_local_count(usize::from(self.locals) + 3 + names.len())?;
                    if self.kind() == Kind::Comma {
                        self.take();
                        self.tasks.push(Task::ForMoreName(line, names));
                    } else {
                        self.expect(Kind::In)?;
                        self.tasks.push(Task::ForValues(line, names));
                        self.tasks.push(Task::List);
                    }
                }
                Task::ForMoreName(line, mut names) => {
                    names.push(self.expect(Kind::Name)?.text);
                    self.tasks.push(Task::ForNames(line, names));
                }
                Task::ForValues(line, names) => {
                    self.expect(Kind::Do)?;
                    let values = std::mem::take(&mut self.list);
                    self.loops += 1;
                    self.locals += names.len() as u16 + 3;
                    self.tasks.push(Task::ForInEnd(line, names, values));
                    self.tasks.push(Task::Block);
                }
                Task::ForInEnd(line, names, values) => {
                    self.expect(Kind::End)?;
                    self.loops -= 1;
                    self.locals -= names.len() as u16 + 3;
                    let body = std::mem::take(&mut self.body);
                    self.statement(
                        Stmt::ForIn(names.into_boxed_slice(), values.into_boxed_slice(), body),
                        line,
                    );
                }
                Task::ExpressionStatement(line) => {
                    let first = self.expr;
                    if matches!(self.kind(), Kind::Assign | Kind::Comma) {
                        self.tasks.push(Task::AssignTargets(line, vec![first]));
                    } else if matches!(self.ast.exprs[first].0, Expr::Call(..) | Expr::Method(..)) {
                        self.statement(Stmt::Call(first), line);
                    } else {
                        return Err(self.error("expected assignment or function call"));
                    }
                }
                Task::AssignTargets(line, targets) => {
                    if self.kind() == Kind::Comma {
                        self.take();
                        self.tasks.push(Task::AssignTarget(line, targets));
                        self.tasks.push(Task::Prefix);
                    } else {
                        if targets.iter().any(|&i| {
                            !matches!(self.ast.exprs[i].0, Expr::Name(_) | Expr::Index(..))
                        }) {
                            return Err(self.error("assignment target must be a variable"));
                        }
                        self.expect(Kind::Assign)?;
                        self.tasks.push(Task::AssignEnd(line, targets));
                        self.tasks.push(Task::List);
                    }
                }
                Task::AssignTarget(line, mut targets) => {
                    targets.push(self.expr);
                    self.tasks.push(Task::AssignTargets(line, targets));
                }
                Task::AssignEnd(line, targets) => {
                    let values = std::mem::take(&mut self.list).into_boxed_slice();
                    self.statement(Stmt::Assign(targets.into_boxed_slice(), values), line);
                }
                Task::List => {
                    self.tasks.push(Task::ListAfter(Vec::new()));
                    self.tasks.push(Task::Expr(0));
                }
                Task::ListAfter(mut values) => {
                    values.push(self.expr);
                    if self.kind() == Kind::Comma {
                        self.take();
                        self.tasks.push(Task::ListAfter(values));
                        self.tasks.push(Task::Expr(0));
                    } else {
                        self.list = values;
                    }
                }
                Task::Function(line, method) => {
                    self.expect(Kind::LParen)?;
                    let params = if method {
                        vec![Box::from(&b"self"[..])]
                    } else {
                        Vec::new()
                    };
                    self.tasks.push(Task::Params(line, params, true));
                }
                Task::Params(line, mut params, allow_end) => {
                    if self.kind() == Kind::Dots {
                        self.take();
                        self.tasks.push(Task::ParamsEnd(line, params, true));
                    } else if self.kind() == Kind::RParen && allow_end {
                        self.start_function_body(line, params, false)?;
                    } else {
                        params.push(self.expect(Kind::Name)?.text);
                        self.tasks.push(Task::ParamAfter(line, params));
                    }
                }
                Task::ParamAfter(line, params) => {
                    self.check_local_count(params.len())?;
                    if self.kind() == Kind::Comma {
                        self.take();
                        self.tasks.push(Task::Params(line, params, false));
                    } else {
                        self.start_function_body(line, params, false)?;
                    }
                }
                Task::ParamsEnd(line, params, vararg) => {
                    self.start_function_body(line, params, vararg)?
                }
                Task::FunctionEnd(line, params, vararg, loops, parent_vararg, locals) => {
                    self.expect(Kind::End)?;
                    self.loops = loops;
                    self.vararg = parent_vararg;
                    self.locals = locals;
                    let body = std::mem::take(&mut self.body);
                    self.expression(
                        Expr::Function {
                            params: params.into_boxed_slice(),
                            vararg,
                            body,
                        },
                        line,
                    );
                }
                Task::Expr(min) => {
                    self.enter()?;
                    let line = self.line();
                    self.tasks.push(Task::ExprLoop(min, line));
                    match self.kind() {
                        Kind::Not | Kind::Minus | Kind::Hash => {
                            let op = self.take().kind;
                            self.tasks.push(Task::Unary(op, line));
                            self.tasks.push(Task::Expr(7));
                        }
                        Kind::Nil => {
                            self.take();
                            self.expression(Expr::Nil, line);
                        }
                        Kind::True | Kind::False => {
                            let b = self.take().kind == Kind::True;
                            self.expression(Expr::Boolean(b), line);
                        }
                        Kind::Number => {
                            let n = self.take().number;
                            self.expression(Expr::Number(n), line);
                        }
                        Kind::String => {
                            let s = self.take().text;
                            self.expression(Expr::String(s), line);
                        }
                        Kind::Dots => {
                            if !self.vararg {
                                return Err(
                                    self.error("cannot use '...' outside a vararg function")
                                );
                            }
                            self.take();
                            self.expression(Expr::Vararg, line);
                        }
                        Kind::Function => {
                            self.take();
                            self.tasks.push(Task::Function(line, false));
                        }
                        Kind::LBrace => self.tasks.push(Task::Table),
                        _ => self.tasks.push(Task::Prefix),
                    }
                }
                Task::ExprLoop(min, line) => {
                    if let Some((left, right)) = precedence(self.kind())
                        && left > min
                    {
                        let op = self.take().kind;
                        self.tasks.push(Task::Binary(min, line, self.expr, op));
                        self.tasks.push(Task::Expr(right));
                    } else {
                        self.depth -= 1;
                    }
                }
                Task::Unary(op, line) => {
                    self.expression(Expr::Unary(op, self.expr), line);
                }
                Task::Binary(min, line, left, op) => {
                    self.expression(Expr::Binary(op, left, self.expr), line);
                    self.tasks.push(Task::ExprLoop(min, line));
                }
                Task::Prefix => {
                    let line = self.line();
                    if self.kind() == Kind::Name {
                        let name = self.take().text;
                        let id = self.expression(Expr::Name(name), line);
                        self.tasks.push(Task::Suffix(line, id));
                    } else if self.kind() == Kind::LParen {
                        self.take();
                        self.tasks.push(Task::Paren(line));
                        self.tasks.push(Task::Expr(0));
                    } else {
                        return Err(self.error("expected expression"));
                    }
                }
                Task::Paren(line) => {
                    self.expect(Kind::RParen)?;
                    let id = self.expression(Expr::Paren(self.expr), line);
                    self.tasks.push(Task::Suffix(line, id));
                }
                Task::Suffix(line, id) => match self.kind() {
                    Kind::Dot => {
                        self.take();
                        self.tasks.push(Task::Field(line, id));
                    }
                    Kind::LBracket => {
                        self.take();
                        self.tasks.push(Task::Index(line, id));
                        self.tasks.push(Task::Expr(0));
                    }
                    Kind::Colon => {
                        self.take();
                        self.tasks.push(Task::MethodName(line, id));
                    }
                    Kind::LParen | Kind::LBrace | Kind::String => {
                        self.tasks.push(Task::CallEnd(line, id));
                        self.tasks.push(Task::Args);
                    }
                    _ => self.expr = id,
                },
                Task::Field(line, id) => {
                    let name = self.expect(Kind::Name)?.text;
                    let key = self.expression(Expr::String(name), line);
                    let field = self.expression(Expr::Index(id, key), line);
                    self.tasks.push(Task::Suffix(line, field));
                }
                Task::Index(line, id) => {
                    self.expect(Kind::RBracket)?;
                    let field = self.expression(Expr::Index(id, self.expr), line);
                    self.tasks.push(Task::Suffix(line, field));
                }
                Task::MethodName(line, id) => {
                    let name = self.expect(Kind::Name)?.text;
                    self.tasks.push(Task::MethodEnd(line, id, name));
                    self.tasks.push(Task::Args);
                }
                Task::MethodEnd(line, id, name) => {
                    let args = std::mem::take(&mut self.list).into_boxed_slice();
                    let call = self.expression(Expr::Method(id, name, args), line);
                    self.tasks.push(Task::Suffix(line, call));
                }
                Task::CallEnd(line, id) => {
                    let args = std::mem::take(&mut self.list).into_boxed_slice();
                    let call = self.expression(Expr::Call(id, args), line);
                    self.tasks.push(Task::Suffix(line, call));
                }
                Task::Args => match self.kind() {
                    Kind::LParen => {
                        if self.line() != self.previous_line {
                            return Err(
                                self.error("ambiguous syntax (function call on a new line)")
                            );
                        }
                        self.take();
                        self.tasks.push(Task::ArgsParen);
                    }
                    Kind::LBrace => {
                        self.tasks.push(Task::ArgsTable);
                        self.tasks.push(Task::Table);
                    }
                    _ => {
                        let token = self.expect(Kind::String)?;
                        let id = self.expression(Expr::String(token.text), token.line);
                        self.list = vec![id];
                    }
                },
                Task::ArgsParen => {
                    if self.kind() == Kind::RParen {
                        self.take();
                        self.list = Vec::new();
                    } else {
                        self.tasks.push(Task::ArgsEnd);
                        self.tasks.push(Task::List);
                    }
                }
                Task::ArgsEnd => {
                    self.expect(Kind::RParen)?;
                }
                Task::ArgsTable => self.list = vec![self.expr],
                Task::Table => {
                    let line = self.expect(Kind::LBrace)?.line;
                    self.tasks.push(Task::TableNext(line, Vec::new()));
                }
                Task::TableNext(line, fields) => {
                    if self.kind() == Kind::RBrace {
                        self.take();
                        self.expression(Expr::Table(fields.into_boxed_slice()), line);
                    } else if self.kind() == Kind::Name
                        && self
                            .lookahead
                            .as_ref()
                            .is_some_and(|t| t.kind == Kind::Assign)
                    {
                        let token = self.take();
                        let key = self.expression(Expr::String(token.text), token.line);
                        self.tasks.push(Task::TableAssign(line, fields, key));
                    } else if self.kind() == Kind::LBracket {
                        self.take();
                        self.tasks.push(Task::TableKey(line, fields));
                        self.tasks.push(Task::Expr(0));
                    } else {
                        self.tasks.push(Task::TableValue(line, fields, None));
                        self.tasks.push(Task::Expr(0));
                    }
                }
                Task::TableKey(line, fields) => {
                    let key = self.expr;
                    self.tasks.push(Task::TableAssign(line, fields, key));
                    self.tasks.push(Task::Expect(Kind::RBracket));
                }
                Task::TableAssign(line, fields, key) => {
                    self.expect(Kind::Assign)?;
                    self.tasks.push(Task::TableValue(line, fields, Some(key)));
                    self.tasks.push(Task::Expr(0));
                }
                Task::TableValue(line, mut fields, key) => {
                    fields.push((key, self.expr));
                    self.tasks.push(Task::TableSeparator(line, fields));
                }
                Task::TableSeparator(line, fields) => {
                    if matches!(self.kind(), Kind::Comma | Kind::Semi) {
                        self.take();
                        self.tasks.push(Task::TableNext(line, fields));
                    } else {
                        self.expect(Kind::RBrace)?;
                        self.expression(Expr::Table(fields.into_boxed_slice()), line);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type Outcome = Result<String, (ErrorKind, u32, String)>;
    fn streamed(source: &[u8], ends: impl IntoIterator<Item = usize>) -> Outcome {
        let mut parser = Parser::default();
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
        let lua = lua_reference::Lua::new();
        for source in [
            b") ".as_slice(),
            b"local 1 ",
            b"break; ",
            b"return function()return ... ",
        ] {
            let mut parser = Parser::default();
            assert!(parser.resume(source, false).is_err(), "{source:?}");
            let count = lua
                .load(
                    br#"
                local first=...;local n=0
                pcall(load,function()n=n+1;if n==1 then return first end;error('must not read')end)
                return n
            "#
                    .as_slice(),
                )
                .call::<usize>(lua.create_string(source).unwrap())
                .unwrap();
            assert_eq!(count, 1, "{source:?}");
        }
    }
}
