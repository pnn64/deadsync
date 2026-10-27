//! A bounded Pratt parser with flat expression and statement arenas.

use crate::{
    Error, ErrorKind,
    lexer::{Kind, Token},
};

pub(crate) type ExprId = usize;
pub(crate) type StmtId = usize;
pub(crate) type Block = Box<[StmtId]>;

/// Observable collector roots created by the incremental reference grammar.
/// Events are enabled only for native readers; ordinary compilation owns its AST.
#[derive(Debug)]
pub(crate) enum NativeRootEvent {
    Open,
    Close,
    Anchor(Box<[u8]>),
    Constant(crate::bytecode::Constant),
    Source(Box<[u8]>),
    DebugString(Box<[u8]>),
}
pub(crate) fn native_root_bytes(events: &Option<Vec<NativeRootEvent>>) -> usize {
    events.as_ref().map_or(0, |events| {
        events.capacity() * size_of::<NativeRootEvent>()
            + events
                .iter()
                .map(|event| match event {
                    NativeRootEvent::Anchor(s)
                    | NativeRootEvent::Source(s)
                    | NativeRootEvent::DebugString(s)
                    | NativeRootEvent::Constant(crate::bytecode::Constant::String(s)) => s.len(),
                    _ => 0,
                })
                .sum::<usize>()
    })
}

#[derive(Debug)]
pub(crate) enum Expr {
    Nil,
    Boolean(bool),
    Number(f64),
    String(Box<[u8]>),
    Name(Box<[u8]>),
    Unary(Kind, ExprId),
    Binary(Kind, ExprId, ExprId, u32),
    Paren(ExprId),
    Index(ExprId, ExprId),
    Table(Box<[(Option<ExprId>, ExprId)]>, u32),
    Call(ExprId, Box<[ExprId]>, u32),
    Method(ExprId, Box<[u8]>, Box<[ExprId]>, u32),
    Function {
        params: Box<[Box<[u8]>]>,
        vararg: bool,
        body: Block,
        end_line: u32,
    },
    Vararg,
}

#[derive(Debug)]
pub(crate) enum Stmt {
    Local(Box<[Box<[u8]>]>, Box<[ExprId]>),
    LocalFunction(Box<[u8]>, ExprId),
    Assign(Box<[ExprId]>, Box<[ExprId]>),
    FunctionDefinition(Box<[ExprId]>, Box<[ExprId]>),
    Call(ExprId),
    Return(Box<[ExprId]>, u32),
    Break,
    Do(Block),
    While(ExprId, Block),
    Repeat(Block, ExprId),
    If(Box<[(ExprId, Block)]>, Option<Block>),
    For {
        name: Box<[u8]>,
        start: ExprId,
        limit: ExprId,
        step: Option<ExprId>,
        do_line: u32,
        body: Block,
    },
    ForIn(Box<[Box<[u8]>]>, Box<[ExprId]>, Block, [u32; 2]),
}

#[derive(Debug, Default)]
pub(crate) struct SourceSpan {
    pub start: u32,
    pub end: u32,
    pub discharge: u32,
    /// Conservative scalar-tree size, 1..=32; zero excludes eager grouping.
    pub scalar_cost: u8,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    size_of::<SourceSpan>() == 16 && size_of::<(Expr, SourceSpan)>() == size_of::<Expr>() + 16
);

#[derive(Debug, Default)]
pub(crate) struct Ast {
    pub exprs: Vec<(Expr, SourceSpan)>,
    pub stmts: Vec<(Stmt, u32)>,
    pub body: Block,
    pub end_line: u32,
    pub constant_operands: Vec<(ExprId, ConstantOperand)>,
    pub zero_signs: Vec<(ExprId, bool)>,
    pub root_zero_sign: Option<bool>,
    pub stacks: Vec<(ExprId, u16)>,
    pub root_stack: u16,
}

/// Reference constant operands for deferred loads. String indices borrow
/// existing AST literals, including folded logical expressions.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ConstantOperand {
    Nil,
    Boolean(bool),
    /// The flag records an RK operand; a folded left value can be known even
    /// when the reference must materialize it in a register.
    Number(f64, bool),
    String(ExprId),
}

mod bindings;
mod resources;
mod resume;
pub(crate) use resume::Parser;

pub(crate) fn parse(source: &[u8]) -> Result<Ast, Error> {
    Parser::default()
        .resume(source, true)
        .map(|ast| ast.expect("closed source cannot request additional input"))
}
