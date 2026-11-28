//! Bytecode sharing for source that only constructs a literal table.
use super::*;
use crate::{
    bytecode::Constant,
    lexer::{Kind, Scanner, Token, short_decimal_integer},
    parser::{Ast, Expr, Stmt},
};

const MIN_LITERALS: usize = 64;
const MAX_LITERALS: usize = 1024;
const MAX_TEMPLATES: usize = 4;
const MAX_BYTES: usize = 1024 * 1024;

struct Literal {
    start: usize,
    end: usize,
    slot: usize,
}

struct Template {
    program: ProgramOwner,
    source: Box<[u8]>,
    depth: usize,
    locale: u64,
    literals: Box<[Literal]>,
    replacements: Box<[(usize, f64)]>,
    ordered: Vec<u64>,
}

#[derive(Default)]
pub(super) struct Templates {
    entries: Vec<Template>,
    bytes: usize,
}

impl Template {
    fn bytes(&self) -> usize {
        self.source.len()
            + self.program.heap_bytes()
            + ProgramData::heap_bytes(&self.program)
            + size_of_val(self.literals.as_ref())
            + size_of_val(self.replacements.as_ref())
            + self.ordered.capacity() * size_of::<u64>()
    }

    fn new(source: &[u8], depth: usize, locale: u64, program: &Program, ast: &Ast) -> Option<Self> {
        let &[stmt] = ast.ids(ast.body) else {
            return None;
        };
        let Stmt::Return(values, _) = ast.stmts[stmt].0 else {
            return None;
        };
        let &[expr] = ast.ids(values) else {
            return None;
        };
        if !matches!(ast.exprs[expr].0, Expr::Table(..)) || !source.is_ascii() {
            return None;
        }
        // There is no operator, branch or call whose compilation depends on
        // a numeric value. Table capacity and field order depend on syntax.
        let mut count = 0;
        for (expr, _) in &ast.exprs {
            match expr {
                Expr::Number(_) => count += 1,
                Expr::Table(..) | Expr::String(_) | Expr::Boolean(_) | Expr::Nil => {}
                _ => return None,
            }
        }
        if !(MIN_LITERALS..=MAX_LITERALS).contains(&count) || program.data.prototypes.len() != 1 {
            return None;
        }
        let mut slots = fallible::with_capacity(count).ok()?;
        for (index, value) in program.data.prototypes[0].constants().iter().enumerate() {
            if let Constant::Number(value) = value {
                slots.try_push((value.to_bits(), index)).ok()?;
            }
        }
        slots.sort_unstable();
        let mut literals = fallible::with_capacity(count).ok()?;
        let mut seen = [false; MAX_LITERALS];
        let mut scanner = Scanner::default();
        let mut token = Token::default();
        loop {
            let previous = scanner.position();
            scanner.next_closed(source, &mut token).ok()?;
            if token.kind == Kind::Eof {
                break;
            }
            if token.kind != Kind::Number {
                continue;
            }
            let end = scanner.position();
            let start = previous
                + source[previous..end]
                    .iter()
                    .position(|&byte| !crate::locale::ascii_space(byte))?;
            let value = short_decimal_integer(&source[start..end])?;
            let slot = slots
                .binary_search_by_key(&value.to_bits(), |&(bits, _)| bits)
                .ok()?;
            *seen.get_mut(slot)? = true;
            literals.try_push(Literal { start, end, slot }).ok()?;
        }
        if literals.len() != count || !seen.get(..slots.len())?.iter().all(|&seen| seen) {
            return None;
        }
        let mut replacements = fallible::with_capacity(slots.len()).ok()?;
        replacements.extend(slots.into_iter().map(|(_, index)| (index, f64::NAN)));
        let ordered = fallible::with_capacity(replacements.len()).ok()?;
        Some(Self {
            program: program.data.clone(),
            source: source.try_boxed().ok()?,
            depth,
            locale,
            literals: literals.try_into_boxed_slice().ok()?,
            replacements: replacements.try_into_boxed_slice().ok()?,
            ordered,
        })
    }

    fn match_source(&mut self, mut source: &[u8]) -> Option<()> {
        for (_, value) in &mut self.replacements {
            *value = f64::NAN;
        }
        let mut previous = 0;
        for &Literal { start, end, slot } in &self.literals {
            source = source.strip_prefix(&self.source[previous..start])?;
            let digits = source
                .iter()
                .take(16)
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            let value = short_decimal_integer(&source[..digits])?;
            let replacement = &mut self.replacements[slot].1;
            if replacement.is_nan() {
                *replacement = value;
            } else if *replacement != value {
                return None;
            }
            source = &source[digits..];
            previous = end;
        }
        if source != &self.source[previous..] {
            return None;
        }
        // Every original equality class maps to exactly one new numeric
        // value. Keep both parsers' constant deduplication and load slots.
        self.ordered.clear();
        self.ordered
            .extend(self.replacements.iter().map(|&(_, value)| value.to_bits()));
        self.ordered.sort_unstable();
        (!self.ordered.windows(2).any(|pair| pair[0] == pair[1])).then_some(())
    }
}

impl Templates {
    pub(super) fn find(
        &mut self,
        source: &[u8],
        name: &[u8],
        depth: usize,
        locale: u64,
    ) -> Option<Result<Program, Error>> {
        let index = self.entries.iter_mut().rposition(|template| {
            depth == template.depth
                && locale == template.locale
                && name == template.program.name()
                && template.match_source(source).is_some()
        })?;
        self.entries[index..].rotate_left(1);
        let template = self.entries.last().expect("matched literal table template");
        Some(Program::with_numbers(
            &template.program,
            0,
            &template.replacements,
        ))
    }

    pub(super) fn remember(
        &mut self,
        source: &[u8],
        depth: usize,
        locale: u64,
        program: &Program,
        ast: &Ast,
    ) {
        if ast.exprs.len() < MIN_LITERALS
            || crate::memory::compilation::active()
            || locale != crate::locale::revision()
        {
            return;
        }
        let Some(template) = Template::new(source, depth, locale, program, ast) else {
            return;
        };
        let bytes = template.bytes();
        if bytes > MAX_BYTES {
            return;
        }
        while self.entries.len() == MAX_TEMPLATES || self.bytes + bytes > MAX_BYTES {
            self.bytes -= self.entries.remove(0).bytes();
        }
        if self.entries.try_reserve(1).is_ok() {
            self.entries.push(template);
            self.bytes += bytes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(first: u32, repeated: u32) -> String {
        let literals = (first..first + 72)
            .map(|number| number.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!("return {{ {literals}, Nested = {{ {repeated}, false }} }}")
    }

    fn compare(vm: &mut Vm, source: &str) -> Program {
        let name = b"@mesh.lua";
        let program = vm.compile_source(source.as_bytes(), name).unwrap();
        let expected = crate::compile_named(source.as_bytes(), name).unwrap();
        assert_eq!(
            crate::chunk51::dump(&program.data, 0).unwrap(),
            crate::chunk51::dump(&expected.data, 0).unwrap(),
        );
        program
    }

    #[test]
    fn templates_share_code_when_numbers_keep_their_equality_classes() {
        let mut vm = Vm::new();
        let original = compare(&mut vm, &source(0, 0));
        let matched = compare(&mut vm, &source(1000, 1000));
        assert_eq!(
            original.data.prototypes[0].code().as_ptr(),
            matched.data.prototypes[0].code().as_ptr(),
        );

        let split = compare(&mut vm, &source(100, 200));
        let merged = compare(&mut vm, &source(150, 151));
        assert_ne!(
            original.data.prototypes[0].code().as_ptr(),
            split.data.prototypes[0].code().as_ptr(),
        );
        assert_ne!(
            split.data.prototypes[0].code().as_ptr(),
            merged.data.prototypes[0].code().as_ptr(),
        );
        assert!(matches!(
            original.data.prototypes[0].constants()[0],
            Constant::Number(value) if value.to_bits() == 0
        ));
    }
}
