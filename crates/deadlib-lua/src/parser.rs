//! A bounded Pratt parser with flat expression and statement arenas.

use crate::{
    Error, ErrorKind,
    lexer::{Kind, Lexer, Token},
};

pub(crate) type ExprId = usize;
pub(crate) type StmtId = usize;
pub(crate) type Block = Box<[StmtId]>;

#[derive(Debug)]
pub(crate) enum Expr {
    Nil,
    Boolean(bool),
    Number(f64),
    String(Box<[u8]>),
    Name(Box<[u8]>),
    Unary(Kind, ExprId),
    Binary(Kind, ExprId, ExprId),
    Paren(ExprId),
    Index(ExprId, ExprId),
    Table(Box<[(Option<ExprId>, ExprId)]>),
    Call(ExprId, Box<[ExprId]>),
    Method(ExprId, Box<[u8]>, Box<[ExprId]>),
    Function {
        params: Box<[Box<[u8]>]>,
        vararg: bool,
        body: Block,
    },
    Vararg,
}

#[derive(Debug)]
pub(crate) enum Stmt {
    Local(Box<[Box<[u8]>]>, Box<[ExprId]>),
    LocalFunction(Box<[u8]>, ExprId),
    Assign(Box<[ExprId]>, Box<[ExprId]>),
    Call(ExprId),
    Return(Box<[ExprId]>),
    Break,
    Do(Block),
    While(ExprId, Block),
    Repeat(Block, ExprId),
    If(Box<[(ExprId, Block)]>, Block),
    For {
        name: Box<[u8]>,
        start: ExprId,
        limit: ExprId,
        step: Option<ExprId>,
        body: Block,
    },
    ForIn(Box<[Box<[u8]>]>, Box<[ExprId]>, Block),
}

#[derive(Debug)]
pub(crate) struct Ast {
    pub exprs: Vec<(Expr, u32)>,
    pub stmts: Vec<(Stmt, u32)>,
    pub body: Block,
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    token: Token,
    next: Token,
    previous_line: u32,
    depth: u16,
    ast: Ast,
}

pub(crate) fn parse(source: &[u8]) -> Result<Ast, Error> {
    let mut lexer = Lexer::new(source);
    let token = lexer.next()?;
    let next = lexer.next()?;
    let mut p = Parser {
        lexer,
        token,
        next,
        previous_line: 1,
        depth: 0,
        ast: Ast {
            exprs: Vec::new(),
            stmts: Vec::new(),
            body: Box::default(),
        },
    };
    p.ast.body = p.block()?;
    p.expect(Kind::Eof)?;
    Ok(p.ast)
}

impl Parser<'_> {
    fn error(&self, message: &str) -> Error {
        Error::new(ErrorKind::Syntax, self.token.line, message)
    }
    fn enter(&mut self) -> Result<(), Error> {
        // Bound recursive parsing independently of source size and native thread-stack size.
        if self.depth >= 120 {
            return Err(Error::new(
                ErrorKind::Limit,
                self.token.line,
                "syntax nesting limit exceeded",
            ));
        }
        self.depth += 1;
        Ok(())
    }
    fn advance(&mut self) -> Result<Token, Error> {
        let next = self.lexer.next()?;
        self.previous_line = self.token.line;
        Ok(std::mem::replace(
            &mut self.token,
            std::mem::replace(&mut self.next, next),
        ))
    }
    fn eat(&mut self, kind: Kind) -> Result<bool, Error> {
        if self.token.kind == kind {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    fn expect(&mut self, kind: Kind) -> Result<Token, Error> {
        if self.token.kind != kind {
            return Err(self.error(&format!("expected {kind:?}, found {:?}", self.token.kind)));
        }
        self.advance()
    }
    fn name(&mut self) -> Result<Box<[u8]>, Error> {
        Ok(self.expect(Kind::Name)?.text)
    }
    fn expr_node(&mut self, expr: Expr, line: u32) -> ExprId {
        let id = self.ast.exprs.len();
        self.ast.exprs.push((expr, line));
        id
    }
    fn stmt_node(&mut self, stmt: Stmt, line: u32) -> StmtId {
        let id = self.ast.stmts.len();
        self.ast.stmts.push((stmt, line));
        id
    }
    fn end_block(&self) -> bool {
        matches!(
            self.token.kind,
            Kind::Eof | Kind::End | Kind::Else | Kind::ElseIf | Kind::Until
        )
    }

    fn block(&mut self) -> Result<Block, Error> {
        self.enter()?;
        let mut stmts = Vec::new();
        while !self.end_block() {
            let terminal = matches!(self.token.kind, Kind::Return | Kind::Break);
            stmts.push(self.statement()?);
            self.eat(Kind::Semi)?;
            if terminal {
                break;
            }
        }
        self.depth -= 1;
        Ok(stmts.into_boxed_slice())
    }

    fn statement(&mut self) -> Result<StmtId, Error> {
        let line = self.token.line;
        let stmt = match self.token.kind {
            Kind::Local => {
                self.advance()?;
                if self.eat(Kind::Function)? {
                    let name = self.name()?;
                    let function = self.function(line, false)?;
                    Stmt::LocalFunction(name, function)
                } else {
                    let mut names = vec![self.name()?];
                    while self.eat(Kind::Comma)? {
                        names.push(self.name()?);
                    }
                    let values = if self.eat(Kind::Assign)? {
                        self.expr_list()?
                    } else {
                        Box::default()
                    };
                    Stmt::Local(names.into_boxed_slice(), values)
                }
            }
            Kind::Function => {
                self.advance()?;
                let name = self.name()?;
                let mut target = self.expr_node(Expr::Name(name), line);
                while self.eat(Kind::Dot)? {
                    let field = self.name()?;
                    let key = self.expr_node(Expr::String(field), line);
                    target = self.expr_node(Expr::Index(target, key), line);
                }
                let method = if self.eat(Kind::Colon)? {
                    let field = self.name()?;
                    let key = self.expr_node(Expr::String(field), line);
                    target = self.expr_node(Expr::Index(target, key), line);
                    true
                } else {
                    false
                };
                let value = self.function(line, method)?;
                Stmt::Assign(
                    vec![target].into_boxed_slice(),
                    vec![value].into_boxed_slice(),
                )
            }
            Kind::Return => {
                self.advance()?;
                Stmt::Return(if self.end_block() || self.token.kind == Kind::Semi {
                    Box::default()
                } else {
                    self.expr_list()?
                })
            }
            Kind::Break => {
                self.advance()?;
                Stmt::Break
            }
            Kind::Do => {
                self.advance()?;
                let body = self.block()?;
                self.expect(Kind::End)?;
                Stmt::Do(body)
            }
            Kind::While => {
                self.advance()?;
                let cond = self.expr(0)?;
                self.expect(Kind::Do)?;
                let body = self.block()?;
                self.expect(Kind::End)?;
                Stmt::While(cond, body)
            }
            Kind::Repeat => {
                self.advance()?;
                let body = self.block()?;
                self.expect(Kind::Until)?;
                Stmt::Repeat(body, self.expr(0)?)
            }
            Kind::If => {
                self.advance()?;
                let mut arms = Vec::new();
                loop {
                    let cond = self.expr(0)?;
                    self.expect(Kind::Then)?;
                    arms.push((cond, self.block()?));
                    if !self.eat(Kind::ElseIf)? {
                        break;
                    }
                }
                let other = if self.eat(Kind::Else)? {
                    self.block()?
                } else {
                    Box::default()
                };
                self.expect(Kind::End)?;
                Stmt::If(arms.into_boxed_slice(), other)
            }
            Kind::For => {
                self.advance()?;
                let name = self.name()?;
                if self.eat(Kind::Assign)? {
                    let start = self.expr(0)?;
                    self.expect(Kind::Comma)?;
                    let limit = self.expr(0)?;
                    let step = if self.eat(Kind::Comma)? {
                        Some(self.expr(0)?)
                    } else {
                        None
                    };
                    self.expect(Kind::Do)?;
                    let body = self.block()?;
                    self.expect(Kind::End)?;
                    Stmt::For {
                        name,
                        start,
                        limit,
                        step,
                        body,
                    }
                } else {
                    let mut names = vec![name];
                    while self.eat(Kind::Comma)? {
                        names.push(self.name()?);
                    }
                    self.expect(Kind::In)?;
                    let values = self.expr_list()?;
                    self.expect(Kind::Do)?;
                    let body = self.block()?;
                    self.expect(Kind::End)?;
                    Stmt::ForIn(names.into_boxed_slice(), values, body)
                }
            }
            _ => {
                let first = self.prefix()?;
                if self.token.kind == Kind::Assign || self.token.kind == Kind::Comma {
                    let mut targets = vec![first];
                    while self.eat(Kind::Comma)? {
                        targets.push(self.prefix()?);
                    }
                    for &target in &targets {
                        if !matches!(self.ast.exprs[target].0, Expr::Name(_) | Expr::Index(..)) {
                            return Err(self.error("assignment target must be a variable"));
                        }
                    }
                    self.expect(Kind::Assign)?;
                    Stmt::Assign(targets.into_boxed_slice(), self.expr_list()?)
                } else if matches!(self.ast.exprs[first].0, Expr::Call(..) | Expr::Method(..)) {
                    Stmt::Call(first)
                } else {
                    return Err(self.error("expected assignment or function call"));
                }
            }
        };
        Ok(self.stmt_node(stmt, line))
    }

    fn expr_list(&mut self) -> Result<Box<[ExprId]>, Error> {
        let mut values = vec![self.expr(0)?];
        while self.eat(Kind::Comma)? {
            values.push(self.expr(0)?);
        }
        Ok(values.into_boxed_slice())
    }

    fn function(&mut self, line: u32, method: bool) -> Result<ExprId, Error> {
        self.expect(Kind::LParen)?;
        let mut params = Vec::new();
        if method {
            params.push(Box::from(&b"self"[..]));
        }
        let mut vararg = false;
        if self.token.kind != Kind::RParen {
            loop {
                if self.eat(Kind::Dots)? {
                    vararg = true;
                    break;
                }
                params.push(self.name()?);
                if !self.eat(Kind::Comma)? {
                    break;
                }
            }
        }
        self.expect(Kind::RParen)?;
        let body = self.block()?;
        self.expect(Kind::End)?;
        Ok(self.expr_node(
            Expr::Function {
                params: params.into_boxed_slice(),
                vararg,
                body,
            },
            line,
        ))
    }

    fn expr(&mut self, minimum: u8) -> Result<ExprId, Error> {
        self.enter()?;
        let line = self.token.line;
        let mut left = if matches!(self.token.kind, Kind::Not | Kind::Minus | Kind::Hash) {
            let op = self.advance()?.kind;
            let value = self.expr(7)?;
            self.expr_node(Expr::Unary(op, value), line)
        } else {
            match self.token.kind {
                Kind::Nil => {
                    self.advance()?;
                    self.expr_node(Expr::Nil, line)
                }
                Kind::True | Kind::False => {
                    let value = self.advance()?.kind == Kind::True;
                    self.expr_node(Expr::Boolean(value), line)
                }
                Kind::Number => {
                    let n = self.advance()?.number;
                    self.expr_node(Expr::Number(n), line)
                }
                Kind::String => {
                    let s = self.advance()?.text;
                    self.expr_node(Expr::String(s), line)
                }
                Kind::Dots => {
                    self.advance()?;
                    self.expr_node(Expr::Vararg, line)
                }
                Kind::Function => {
                    self.advance()?;
                    self.function(line, false)?
                }
                Kind::LBrace => self.table()?,
                _ => self.prefix()?,
            }
        };
        loop {
            let (l, r) = match self.token.kind {
                Kind::Or => (1, 1),
                Kind::And => (2, 2),
                Kind::Eq | Kind::Ne | Kind::Lt | Kind::Le | Kind::Gt | Kind::Ge => (3, 3),
                Kind::Concat => (5, 4),
                Kind::Plus | Kind::Minus => (6, 6),
                Kind::Star | Kind::Slash | Kind::Percent => (7, 7),
                Kind::Caret => (10, 9),
                _ => break,
            };
            if l <= minimum {
                break;
            }
            let op = self.advance()?.kind;
            let right = self.expr(r)?;
            left = self.expr_node(Expr::Binary(op, left, right), line);
        }
        self.depth -= 1;
        Ok(left)
    }

    fn prefix(&mut self) -> Result<ExprId, Error> {
        let line = self.token.line;
        let mut id = if self.token.kind == Kind::Name {
            let name = self.name()?;
            self.expr_node(Expr::Name(name), line)
        } else if self.eat(Kind::LParen)? {
            let inner = self.expr(0)?;
            self.expect(Kind::RParen)?;
            self.expr_node(Expr::Paren(inner), line)
        } else {
            return Err(self.error("expected expression"));
        };
        loop {
            id = match self.token.kind {
                Kind::Dot => {
                    self.advance()?;
                    let field = self.name()?;
                    let key = self.expr_node(Expr::String(field), line);
                    self.expr_node(Expr::Index(id, key), line)
                }
                Kind::LBracket => {
                    self.advance()?;
                    let key = self.expr(0)?;
                    self.expect(Kind::RBracket)?;
                    self.expr_node(Expr::Index(id, key), line)
                }
                Kind::Colon => {
                    self.advance()?;
                    let method = self.name()?;
                    let args = self.args()?;
                    self.expr_node(Expr::Method(id, method, args), line)
                }
                Kind::LParen | Kind::LBrace | Kind::String => {
                    let args = self.args()?;
                    self.expr_node(Expr::Call(id, args), line)
                }
                _ => break,
            };
        }
        Ok(id)
    }

    fn args(&mut self) -> Result<Box<[ExprId]>, Error> {
        if self.token.kind == Kind::LParen {
            if self.token.line != self.previous_line {
                return Err(self.error("ambiguous syntax (function call on a new line)"));
            }
            self.advance()?;
            let args = if self.token.kind == Kind::RParen {
                Box::default()
            } else {
                self.expr_list()?
            };
            self.expect(Kind::RParen)?;
            Ok(args)
        } else if self.token.kind == Kind::LBrace {
            Ok(vec![self.table()?].into_boxed_slice())
        } else {
            let token = self.expect(Kind::String)?;
            let id = self.expr_node(Expr::String(token.text), token.line);
            Ok(vec![id].into_boxed_slice())
        }
    }

    fn table(&mut self) -> Result<ExprId, Error> {
        let line = self.expect(Kind::LBrace)?.line;
        let mut fields = Vec::new();
        while self.token.kind != Kind::RBrace {
            let key = if self.token.kind == Kind::Name && self.next.kind == Kind::Assign {
                let token = self.advance()?;
                self.advance()?;
                Some(self.expr_node(Expr::String(token.text), token.line))
            } else if self.eat(Kind::LBracket)? {
                let key = self.expr(0)?;
                self.expect(Kind::RBracket)?;
                self.expect(Kind::Assign)?;
                Some(key)
            } else {
                None
            };
            fields.push((key, self.expr(0)?));
            if !self.eat(Kind::Comma)? && !self.eat(Kind::Semi)? {
                break;
            }
        }
        self.expect(Kind::RBrace)?;
        Ok(self.expr_node(Expr::Table(fields.into_boxed_slice()), line))
    }
}
