//! Bounded reuse of successful compilation under one immutable host schema.
use super::*;

struct Entry {
    bytes: Vec<u8>,
    arguments: Vec<Binding>,
    upvalues: Vec<Binding>,
    program: Program,
}

/// A loading-time cache for consecutive identical callback requests.
///
/// Dump bytes and bindings must match exactly, including numeric bits, object
/// identities, cell/input indices and type contracts. Only the last successful
/// request is retained. Programs share immutable code; each machine still gets
/// independent state, or an explicit group supplies its shared cell store.
pub struct BatchCompiler<'compiler, 'schema> {
    compiler: &'compiler Compiler<'schema>,
    last: Option<Entry>,
}

impl<'compiler, 'schema> BatchCompiler<'compiler, 'schema> {
    pub(super) fn new(compiler: &'compiler Compiler<'schema>) -> Self {
        Self { compiler, last: None }
    }

    /// Dump without execution, then compile or reuse the last identical request.
    pub fn compile(
        &mut self,
        function: &Function,
        arguments: &[Binding],
        upvalues: &[Binding],
    ) -> Result<Program, CompileError> {
        self.lower(Cow::Owned(dump(function)?), arguments, upvalues)
    }

    /// Validate/compile bytes, reusing only an exactly matching successful request.
    pub fn compile_bytecode(
        &mut self,
        bytes: &[u8],
        arguments: &[Binding],
        upvalues: &[Binding],
    ) -> Result<Program, CompileError> {
        self.lower(Cow::Borrowed(bytes), arguments, upvalues)
    }

    fn lower(
        &mut self,
        bytes: Cow<'_, [u8]>,
        arguments: &[Binding],
        upvalues: &[Binding],
    ) -> Result<Program, CompileError> {
        if let Some(last) = &self.last
            && last.bytes == bytes.as_ref()
            && same(&last.arguments, arguments)
            && same(&last.upvalues, upvalues)
        {
            return Ok(last.program.clone());
        }
        let program = self.compiler.compile_bytecode(&bytes, arguments, upvalues)?;
        if let Some(last) = &mut self.last {
            last.bytes = bytes.into_owned();
            last.arguments.clear();
            last.arguments.extend_from_slice(arguments);
            last.upvalues.clear();
            last.upvalues.extend_from_slice(upvalues);
            last.program = program.clone();
        } else {
            self.last = Some(Entry {
                bytes: bytes.into_owned(),
                arguments: arguments.to_vec(),
                upvalues: upvalues.to_vec(),
                program: program.clone(),
            });
        }
        Ok(program)
    }
}

fn same(a: &[Binding], b: &[Binding]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| match (a, b) {
            (Binding::Literal(a), Binding::Literal(b)) => match (a, b) {
                (Literal::Nil, Literal::Nil) => true,
                (Literal::Boolean(a), Literal::Boolean(b)) => a == b,
                (Literal::Number(a), Literal::Number(b)) => a.to_bits() == b.to_bits(),
                (Literal::Bytes(a), Literal::Bytes(b)) => a == b,
                _ => false,
            },
            (Binding::Object(a), Binding::Object(b)) => a == b,
            (Binding::Cell(a), Binding::Cell(b)) => a == b,
            (Binding::Input { slot: a, types: at }, Binding::Input { slot: b, types: bt }) => {
                a == b && at == bt
            }
            _ => false,
        })
}
