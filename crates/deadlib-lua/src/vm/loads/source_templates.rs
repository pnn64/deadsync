//! Reuse verified programs when only a leading local initialization or its
//! constant-returning function changes. All other source bytes must match.
use super::*;
use crate::parser::{Ast, ConstantOperand, Expr, Stmt};
use std::ops::Range;

const ENTRIES: usize = 32;
const BYTE_LIMIT: usize = 4 * 1024 * 1024;
const MIN_SOURCE: usize = 256;

pub(super) struct Preamble {
    digits: Range<usize>,
    value: f64,
    function: bool,
}

pub(super) struct Candidate {
    preamble: Preamble,
    parser_numbers: Vec<u64>,
}

struct Prefix<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl Prefix<'_> {
    fn space(&mut self) {
        while let Some(&byte) = self.bytes.get(self.pos) {
            if !crate::locale::ascii_space(byte) {
                break;
            }
            self.pos += 1;
        }
    }

    fn eat(&mut self, token: &[u8]) -> Option<()> {
        if !self.bytes[self.pos..].starts_with(token) {
            return None;
        }
        self.pos += token.len();
        Some(())
    }

    fn word(&mut self, token: &[u8]) -> Option<()> {
        self.eat(token)?;
        self.bytes
            .get(self.pos)
            .copied()
            .filter(|&b| crate::locale::ascii_space(b))?;
        self.space();
        Some(())
    }

    fn identifier(&mut self) -> Option<()> {
        let first = *self.bytes.get(self.pos)?;
        if !first.is_ascii_alphabetic() && first != b'_' {
            return None;
        }
        self.pos += 1;
        while let Some(&byte) = self.bytes.get(self.pos) {
            if !byte.is_ascii_alphanumeric() && byte != b'_' {
                break;
            }
            self.pos += 1;
        }
        self.space();
        Some(())
    }
}

impl Preamble {
    pub(super) fn scan(source: &[u8]) -> Option<Self> {
        if source.len() < MIN_SOURCE {
            return None;
        }
        let mut prefix = Prefix {
            bytes: source,
            pos: 0,
        };
        prefix.space();
        prefix.word(b"local")?;
        let name_start = prefix.pos;
        let function = prefix.word(b"function").is_some();
        if !function {
            prefix.pos = name_start;
        }
        prefix.identifier()?;
        if function {
            prefix.eat(b"(")?;
            prefix.space();
            prefix.eat(b")")?;
            prefix.space();
            prefix.word(b"return")?;
        } else {
            prefix.eat(b"=")?;
            prefix.space();
        }
        let start = prefix.pos;
        let end = source[start..]
            .iter()
            .position(|byte| !byte.is_ascii_digit())
            .map(|offset| start + offset)?;
        let value = crate::lexer::short_decimal_integer(&source[start..end])?;
        if value == 0.0 || (!function && value == 1.0) {
            return None;
        }
        prefix.pos = end;
        if function {
            if !crate::locale::ascii_space(source[end]) {
                return None;
            }
            prefix.space();
            prefix.eat(b"end")?;
            if source
                .get(prefix.pos)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
            {
                return None;
            }
        } else {
            let boundary = source[end..]
                .iter()
                .copied()
                .find(|&b| b != b' ' && b != b'\t');
            if !matches!(boundary, Some(b'\n' | b'\r' | b';')) {
                return None;
            }
        }
        if !source.is_ascii() {
            return None;
        }
        Some(Self {
            digits: start..end,
            value,
            function,
        })
    }

    pub(super) fn check_resources(self, parser: &crate::parser::Parser) -> Option<Candidate> {
        let mut parser_numbers = Vec::new();
        if !self.function {
            for value in parser.initializer_numbers(self.value)? {
                parser_numbers.try_push(value.to_bits()).ok()?;
            }
        }
        Some(Candidate {
            preamble: self,
            parser_numbers,
        })
    }

    fn function_target(&self, base: &ProgramData) -> Option<(usize, usize)> {
        let entry = &base.prototypes[base.entry as usize];
        let first = entry.code.first()?;
        if first.op != Op::Closure || first.b != 0 {
            return None;
        }
        let id = first.c as usize;
        let child = base.prototypes.get(id)?;
        let [load, ret, end] = child.code.as_ref() else {
            return None;
        };
        if load.op != Op::Load
            || load.b != 0
            || ret.op != Op::Return
            || ret.a != load.a
            || ret.c != 1
            || end.op != Op::Return
            || end.c != 0
            || child.params != 0
            || child.vararg
            || !child.upvalues.is_empty()
            || child
                .shared_owner()
                .is_some_and(|owner| owner as usize != id)
            || base
                .prototypes
                .iter()
                .enumerate()
                .any(|(index, p)| index != id && p.shared_owner() == Some(id as u32))
        {
            return None;
        }
        let [Constant::Number(number)] = child.constants() else {
            return None;
        };
        (load.c == 0 && *number == self.value).then_some((id, 0))
    }

    fn initializer_target(
        &self,
        base: &ProgramData,
        ast: &Ast,
        folded: &[Option<f64>],
        mut excluded: Vec<u64>,
    ) -> Option<((usize, usize), Box<[u64]>)> {
        // Lua can continue an expression after a newline. The grammar must
        // confirm that the entire initializer is the scanned literal.
        let first = *ast.ids(ast.body).first()?;
        let Stmt::Local(names, values) = ast.stmts[first].0 else {
            return None;
        };
        let [literal] = ast.ids(values) else {
            return None;
        };
        if names.len() != 1 || !matches!(ast.exprs[*literal].0, Expr::Number(n) if n == self.value)
        {
            return None;
        }
        let id = base.entry as usize;
        let entry = &base.prototypes[id];
        let load = entry.code.first()?;
        let constant = load.c as usize;
        if load.op != Op::Load
            || load.a != 0
            || load.b != 0
            || !matches!(entry.constants().get(constant), Some(Constant::Number(n)) if *n == self.value)
        {
            return None;
        }

        // Changing constant equality can change slot numbers and register
        // allocation. Include folded values and parser-selected operands
        // even when the final instructions do not use them.
        fallible::reserve(
            &mut excluded,
            folded.len() + ast.constant_operands.len() + entry.constants().len(),
        )
        .ok()?;
        excluded.extend(folded.iter().enumerate().filter_map(|(index, value)| {
            (index != *literal)
                .then_some(*value)
                .flatten()
                .map(f64::to_bits)
        }));
        excluded.extend(ast.constant_operands.iter().filter_map(
            |(index, operand)| match operand {
                ConstantOperand::Number(value, _) if *index != *literal => Some(value.to_bits()),
                _ => None,
            },
        ));
        excluded.extend(entry.constants().iter().enumerate().filter_map(
            |(index, value)| match value {
                Constant::Number(value) if index != constant => Some(value.to_bits()),
                _ => None,
            },
        ));
        excluded.sort_unstable();
        excluded.dedup();
        if excluded.binary_search(&self.value.to_bits()).is_ok() {
            return None;
        }
        Some(((id, constant), excluded.try_into_boxed_slice().ok()?))
    }
}

struct Template {
    source: Box<[u8]>,
    digits: Range<usize>,
    depth: usize,
    locale: u64,
    base: crate::memory::Shared<ProgramData>,
    target: (usize, usize),
    excluded: Box<[u64]>,
    bytes: usize,
}

#[derive(Debug)]
pub(super) struct Variant {
    base: crate::memory::Shared<ProgramData>,
    target: (usize, usize),
    value: f64,
}

impl Variant {
    pub(super) fn publish(&self) -> Result<Program, Error> {
        let (prototype, constant) = self.target;
        Program::with_number(&self.base, prototype, constant, self.value)
    }
}

#[derive(Default)]
pub(super) struct Templates {
    entries: Vec<Template>,
    bytes: usize,
}
impl Templates {
    pub(super) fn find(
        &mut self,
        source: &[u8],
        name: &[u8],
        depth: usize,
        locale: u64,
        preamble: &Preamble,
    ) -> Option<Variant> {
        let index = self.entries.iter().rposition(|entry| {
            entry.depth == depth
                && entry.locale == locale
                && entry.base.name.as_ref() == name
                && entry.digits.start == preamble.digits.start
                && entry.source[entry.digits.end..] == source[preamble.digits.end..]
                && entry.source[..entry.digits.start] == source[..preamble.digits.start]
                && entry
                    .excluded
                    .binary_search(&preamble.value.to_bits())
                    .is_err()
        })?;
        if index + 1 != self.entries.len() {
            let entry = self.entries.remove(index);
            self.entries.push(entry);
        }
        let entry = self.entries.last().expect("matched template");
        Some(Variant {
            base: entry.base.clone(),
            target: entry.target,
            value: preamble.value,
        })
    }

    pub(super) fn remember(
        &mut self,
        source: &[u8],
        depth: usize,
        locale: u64,
        candidate: Candidate,
        program: &Program,
        ast: &Ast,
        folded: &[Option<f64>],
    ) {
        if locale != crate::locale::revision() {
            return;
        }
        let Candidate {
            preamble,
            parser_numbers,
        } = candidate;
        let Some((target, excluded)) = (if preamble.function {
            preamble
                .function_target(&program.data)
                .map(|target| (target, Box::default()))
        } else {
            preamble.initializer_target(&program.data, ast, folded, parser_numbers)
        }) else {
            return;
        };
        let bytes = source.len()
            + program.data.heap_bytes()
            + ProgramData::heap_bytes(&program.data)
            + std::mem::size_of_val(excluded.as_ref());
        if bytes > BYTE_LIMIT {
            return;
        }
        let Ok(source) = source.try_boxed() else {
            return;
        };
        while self.entries.len() == ENTRIES || self.bytes + bytes > BYTE_LIMIT {
            self.bytes -= self.entries.remove(0).bytes;
        }
        if self.entries.try_reserve(1).is_err() {
            return;
        }
        self.entries.push(Template {
            source,
            digits: preamble.digits,
            depth,
            locale,
            base: program.data.clone(),
            target,
            excluded,
            bytes,
        });
        self.bytes += bytes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(vm: &mut Vm, source: &str, expected: &[Value]) -> Program {
        let source = source.as_bytes();
        let name = b"@numeric-source-template.lua";
        let reference = crate::compile_named(source, name).unwrap();
        let program = vm.compile_source(source, name).unwrap();
        assert_eq!(
            crate::chunk51::dump(&program.data, program.data.entry as usize).unwrap(),
            crate::chunk51::dump(&reference.data, reference.data.entry as usize).unwrap(),
        );
        assert_eq!(vm.execute(&program).unwrap(), expected);
        program
    }

    #[test]
    fn complete_source_templates_keep_constants_independent_and_check_collisions() {
        let mut vm = Vm::new();
        let padding = "p".repeat(2300);
        let mut previous = None::<Program>;
        for value in [47, 473, 1700] {
            let program = compile(
                &mut vm,
                &format!(
                    "local function version() return {value} end\n--{padding}\nreturn version()"
                ),
                &[Value::Number(f64::from(value))],
            );
            if let Some(previous) = &previous {
                assert!(std::ptr::eq(
                    previous.data.prototypes[0].code.as_ref(),
                    program.data.prototypes[0].code.as_ref()
                ));
            }
            previous = Some(program);
        }
        let initializers = [47, 302, 9, 706].map(|value| {
            compile(
                &mut vm,
                &format!("local version = {value}\n--{padding}\nreturn version, 4+5"),
                &[Value::Number(f64::from(value)), Value::Number(9.0)],
            )
        });
        let code = initializers
            .each_ref()
            .map(|p| p.data.prototypes[0].code.as_ptr());
        assert_eq!(code[0], code[1]);
        assert_ne!(code[0], code[2]);
        assert_eq!(code[0], code[3]);
        assert_eq!(
            vm.execute(&initializers[0]).unwrap(),
            &[Value::Number(47.0), Value::Number(9.0)]
        );
    }
}
