//! A bounded Pratt parser with flat expression, statement and list arenas.
use crate::memory::text::Text;

use crate::{
    Error, ErrorKind,
    lexer::{Kind, Lexeme, Token},
};

pub(crate) const MAX_SYNTAX_DEPTH: u16 = 200;
/// The complete grammar falls back to continuations above this nesting depth.
pub(crate) const MAX_RECURSIVE_DEPTH: u16 = 48;

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

/// Text waiting to be published at a reader boundary. The source prefix
/// remains available while the loader consumes the pending events.
#[derive(Debug)]
pub(crate) enum RootText {
    Source { start: u32, end: u32 },
    Static(&'static [u8]),
    Owned(Text),
}
impl RootText {
    fn from_lexeme(text: &Lexeme) -> Result<Self, Error> {
        Ok(match text {
            Lexeme::Source { start, end, .. } => Self::Source {
                start: *start,
                end: *end,
            },
            Lexeme::Owned(text) => Self::Owned(Text::new(text)?),
        })
    }
    pub(crate) fn bytes<'a>(&'a self, input: &'a [u8]) -> &'a [u8] {
        match self {
            Self::Source { start, end } => &input[*start as usize..*end as usize],
            Self::Static(bytes) => bytes,
            Self::Owned(text) => text,
        }
    }
}

/// Observable collector roots created by the incremental reference grammar.
/// Events are enabled only for native readers; ordinary compilation owns its AST.
#[derive(Debug)]
pub(crate) enum NativeRootEvent {
    Open,
    OpenWithoutTable,
    Close,
    Anchor(RootText),
    Constant(crate::bytecode::Constant),
    ConstantText(RootText),
    Source(Box<[u8]>),
    DebugString(Text),
}
pub(crate) fn native_root_bytes(events: &Option<Vec<NativeRootEvent>>) -> usize {
    events.as_ref().map_or(0, |events| {
        events.capacity() * size_of::<NativeRootEvent>()
            + events
                .iter()
                .map(|event| match event {
                    NativeRootEvent::Anchor(RootText::Owned(s))
                    | NativeRootEvent::ConstantText(RootText::Owned(s))
                    | NativeRootEvent::DebugString(s) => s.heap_bytes(),
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
    String(Lexeme),
    Name(Lexeme),
    Unary(Kind, ExprId),
    Binary(Kind, ExprId, ExprId, u32),
    Paren(ExprId),
    Index(ExprId, ExprId),
    /// Fields in [`Ast::fields`] and the constructor's load line.
    Table(Span, u32),
    /// Arguments in [`Ast::ids`].
    Call(ExprId, Span, u32),
    Method(ExprId, Lexeme, Span, u32),
    Function {
        /// Parameter names in [`Ast::names`].
        params: Span,
        vararg: bool,
        body: Block,
        end_line: u32,
        template_source: [u32; 2],
        /// Starting line of a reused body, or zero when parsed here.
        template_line: u32,
    },
    Vararg,
}

/// List-valued fields are [`Span`]s: names select [`Ast::names`], arms select
/// [`Ast::arms`], and expression lists select [`Ast::ids`].
#[derive(Debug)]
pub(crate) enum Stmt {
    Local(Span, Span),
    LocalFunction(Lexeme, ExprId),
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
        name: Lexeme,
        start: ExprId,
        limit: ExprId,
        step: Option<ExprId>,
        do_line: u32,
        body: Block,
    },
    ForIn(Span, Span, Block, [u32; 2]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NameBinding(u8);

impl NameBinding {
    pub(crate) const UPVALUE: Self = Self(254);
    pub(crate) const GLOBAL: Self = Self(255);

    fn local(index: u8) -> Self {
        debug_assert!(index < 200);
        Self(index)
    }

    pub(crate) fn local_index(self) -> Option<usize> {
        (self.0 < Self::UPVALUE.0).then_some(usize::from(self.0))
    }
}

impl Default for NameBinding {
    fn default() -> Self {
        Self::GLOBAL
    }
}

#[derive(Debug, Default)]
pub(crate) struct SourceSpan {
    pub start: u32,
    pub end: u32,
    pub discharge: u32,
    /// Conservative scalar-tree size, 1..=32; zero excludes eager grouping.
    pub scalar_cost: u8,
    /// One-based index into the resolved constant operands. Zero is absent.
    pub operand: u16,
    pub binding: NameBinding,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    size_of::<SourceSpan>() == 16 && size_of::<(Expr, SourceSpan)>() == size_of::<Expr>() + 16
);

/// A constructor field in the AST. The largest usize cannot index an
/// expression arena and denotes a value with an implicit array index.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TableField {
    key: ExprId,
    pub(crate) value: ExprId,
}

impl TableField {
    pub(crate) fn new(key: Option<ExprId>, value: ExprId) -> Self {
        Self {
            key: key.unwrap_or(usize::MAX),
            value,
        }
    }

    pub(crate) fn key(self) -> Option<ExprId> {
        (self.key != usize::MAX).then_some(self.key)
    }
}

#[derive(Debug, Default)]
pub(crate) struct Ast {
    pub exprs: Vec<(Expr, SourceSpan)>,
    pub stmts: Vec<(Stmt, u32)>,
    /// Statement and expression identifier lists.
    pub ids: Vec<usize>,
    pub names: Vec<Lexeme>,
    pub fields: Vec<TableField>,
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
    /// Remove every node, keeping storage for another parse.
    pub(crate) fn clear(&mut self) {
        self.exprs.clear();
        self.stmts.clear();
        self.ids.clear();
        self.names.clear();
        self.fields.clear();
        self.arms.clear();
        self.body = Span::default();
        self.end_line = 0;
        self.constant_operands.clear();
        self.zero_signs.clear();
        self.root_zero_sign = None;
        self.stacks.clear();
        self.root_stack = 0;
    }
    pub(crate) fn ids(&self, span: Span) -> &[usize] {
        &self.ids[span.range()]
    }
    pub(crate) fn names(&self, span: Span) -> &[Lexeme] {
        &self.names[span.range()]
    }
    pub(crate) fn fields(&self, span: Span) -> &[TableField] {
        &self.fields[span.range()]
    }
    pub(crate) fn arms(&self, span: Span) -> &[(ExprId, Block)] {
        &self.arms[span.range()]
    }
    pub(crate) fn constant_operand(&self, id: ExprId) -> Option<ConstantOperand> {
        let index = match self.exprs[id].1.operand {
            0 => return None,
            u16::MAX => self
                .constant_operands
                .binary_search_by_key(&id, |&(id, _)| id)
                .ok()?,
            index => usize::from(index - 1),
        };
        Some(self.constant_operands[index].1)
    }
    /// Pool storage, including owned parameter and local name text.
    pub(crate) fn list_bytes(&self) -> usize {
        self.ids.capacity() * size_of::<usize>()
            + self.names.capacity() * size_of::<Lexeme>()
            + self.names.iter().map(|n| n.heap_bytes()).sum::<usize>()
            + self.fields.capacity() * size_of::<TableField>()
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
    /// The slot records an RK operand. A folded left value can also be known
    /// when the reference must materialize it in a register.
    Number(f64, Option<u8>),
    String(ExprId),
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<ConstantOperand>() == 16);

mod bindings;
mod resources;
mod resume;
/// Chunks up to this size can reuse a parsed numeric source template.
pub(crate) const SOURCE_REUSE_LIMIT: usize = 64 * 1024;

pub(crate) use resume::{Parser, Spare};

use crate::hash::hash_bytes as hash;

pub(crate) fn parse(source: &[u8]) -> Result<Ast, Error> {
    Parser::try_new()?
        .resume(source, true)
        .map(|ast| ast.expect("closed source cannot request additional input"))
}

#[cfg(test)]
mod allocation_tests;
