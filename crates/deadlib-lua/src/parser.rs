//! A bounded Pratt parser with flat expression and statement arenas.

use crate::{
    Error, ErrorKind,
    lexer::{Kind, Token},
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

#[derive(Debug, Default)]
pub(crate) struct Ast {
    pub exprs: Vec<(Expr, u32)>,
    pub stmts: Vec<(Stmt, u32)>,
    pub body: Block,
}

mod resume;
pub(crate) use resume::Parser;

pub(crate) fn parse(source: &[u8]) -> Result<Ast, Error> {
    Parser::default()
        .resume(source, true)
        .map(|ast| ast.expect("closed source cannot request additional input"))
}
