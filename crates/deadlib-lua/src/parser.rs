//! A bounded Pratt parser with flat expression, statement and list arenas.
use crate::memory::text::Text;

use crate::{
    Error, ErrorKind,
    lexer::{Kind, Token},
};

pub(crate) type ExprId = usize;
pub(crate) type StmtId = usize;
/// Statement identifiers stored in [`Ast::ids`].
pub(crate) type Block = Span;

/// A contiguous run of one of the AST's flat list pools.
///
/// Lists are completed in nesting order, so a node's list never moves after
/// it is published and every node shares the pool allocations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Span {
    start: u32,
    len: u32,
}
impl Span {
    pub(crate) fn len(self) -> usize {
        self.len as usize
    }
    fn range(self) -> std::ops::Range<usize> {
        self.start as usize..self.start as usize + self.len as usize
    }
}

/// Observable collector roots created by the incremental reference grammar.
/// Events are enabled only for native readers; ordinary compilation owns its AST.
#[derive(Debug)]
pub(crate) enum NativeRootEvent {
    Open,
    Close,
    Anchor(Text),
    Constant(crate::bytecode::Constant),
    Source(Box<[u8]>),
    DebugString(Text),
}
pub(crate) fn native_root_bytes(events: &Option<Vec<NativeRootEvent>>) -> usize {
    events.as_ref().map_or(0, |events| {
        events.capacity() * size_of::<NativeRootEvent>()
            + events
                .iter()
                .map(|event| match event {
                    NativeRootEvent::Anchor(s) | NativeRootEvent::DebugString(s) => s.heap_bytes(),
                    NativeRootEvent::Source(s)
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
    String(Text),
    Name(Text),
    Unary(Kind, ExprId),
    Binary(Kind, ExprId, ExprId, u32),
    Paren(ExprId),
    Index(ExprId, ExprId),
    /// Fields in [`Ast::fields`] and the constructor's load line.
    Table(Span, u32),
    /// Arguments in [`Ast::ids`].
    Call(ExprId, Span, u32),
    Method(ExprId, Text, Span, u32),
    Function {
        /// Parameter names in [`Ast::names`].
        params: Span,
        vararg: bool,
        body: Block,
        end_line: u32,
    },
    Vararg,
}

/// List-valued fields are [`Span`]s: names select [`Ast::names`], arms select
/// [`Ast::arms`], and expression lists select [`Ast::ids`].
#[derive(Debug)]
pub(crate) enum Stmt {
    Local(Span, Span),
    LocalFunction(Text, ExprId),
    Assign(Span, Span),
    FunctionDefinition(Span, Span),
    Call(ExprId),
    Return(Span, u32),
    Break,
    Do(Block),
    While(ExprId, Block),
    Repeat(Block, ExprId),
    If(Span, Option<Block>),
    For {
        name: Text,
        start: ExprId,
        limit: ExprId,
        step: Option<ExprId>,
        do_line: u32,
        body: Block,
    },
    ForIn(Span, Span, Block, [u32; 2]),
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
    /// Statement and expression identifier lists.
    pub ids: Vec<usize>,
    pub names: Vec<Text>,
    pub fields: Vec<(Option<ExprId>, ExprId)>,
    pub arms: Vec<(ExprId, Block)>,
    pub body: Block,
    pub end_line: u32,
    pub constant_operands: Vec<(ExprId, ConstantOperand)>,
    pub zero_signs: Vec<(ExprId, bool)>,
    pub root_zero_sign: Option<bool>,
    pub stacks: Vec<(ExprId, u16)>,
    pub root_stack: u16,
}

impl Ast {
    pub(crate) fn ids(&self, span: Span) -> &[usize] {
        &self.ids[span.range()]
    }
    pub(crate) fn names(&self, span: Span) -> &[Text] {
        &self.names[span.range()]
    }
    pub(crate) fn fields(&self, span: Span) -> &[(Option<ExprId>, ExprId)] {
        &self.fields[span.range()]
    }
    pub(crate) fn arms(&self, span: Span) -> &[(ExprId, Block)] {
        &self.arms[span.range()]
    }
    /// Pool storage, including owned parameter and local name text.
    pub(crate) fn list_bytes(&self) -> usize {
        self.ids.capacity() * size_of::<usize>()
            + self.names.capacity() * size_of::<Text>()
            + self.names.iter().map(|n| n.heap_bytes()).sum::<usize>()
            + self.fields.capacity() * size_of::<(Option<ExprId>, ExprId)>()
            + self.arms.capacity() * size_of::<(ExprId, Block)>()
    }
}

/// Move `scratch[mark..]` to the end of `pool`, returning its published span.
/// Lists nest, so the scratch tail always belongs to the innermost open list.
pub(crate) fn publish<T>(
    pool: &mut Vec<T>,
    scratch: &mut Vec<T>,
    mark: usize,
) -> Result<Span, Error> {
    let len = scratch.len() - mark;
    let start = u32::try_from(pool.len()).map_err(|_| Error::memory())?;
    let end = pool.len().checked_add(len).ok_or_else(Error::memory)?;
    u32::try_from(end).map_err(|_| Error::memory())?;
    crate::memory::fallible::reserve(pool, len)?;
    pool.extend(scratch.drain(mark..));
    Ok(Span {
        start,
        len: len as u32,
    })
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
    Parser::try_new()?
        .resume(source, true)
        .map(|ast| ast.expect("closed source cannot request additional input"))
}

#[cfg(test)]
mod allocation_tests;
