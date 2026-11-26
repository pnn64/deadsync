//! Reuse verified code for table constructors with simple method callbacks.
//! Source spelling is fixed outside literal slots, and each function keeps
//! its original constant equality classes.
use super::*;
use crate::{
    bytecode::{Constant, Prototype},
    lexer::{Kind, Lexeme, Scanner, Token, short_decimal_integer},
    memory::Shared,
    parser::{Ast, Expr, ExprId, Stmt},
};

const MAX_PROTOTYPES: usize = 128;
const MAX_CONSTANTS: usize = 2048;
const MAX_TEXT: usize = 96;
const MAX_TEMPLATES: usize = 4;
const MAX_BYTES: usize = 1024 * 1024;
const RECENT_SHAPES: usize = 16;

struct Callback {
    start: usize,
    end: usize,
    line: u32,
    prototype: usize,
}

fn method_callback(ast: &Ast, id: ExprId) -> Option<Callback> {
    let (expr, span) = &ast.exprs[id];
    let Expr::Function {
        params,
        vararg: false,
        body,
        end_line,
        template_source: [start, end],
        template_line: 0,
    } = *expr
    else {
        return None;
    };
    if params.len() != 1 || end_line != span.start || start == end {
        return None;
    }
    let &[statement] = ast.ids(body) else {
        return None;
    };
    let Stmt::Call(mut id) = ast.stmts[statement].0 else {
        return None;
    };
    for _ in 0..8 {
        let Expr::Method(receiver, _, args, _) = ast.exprs[id].0 else {
            return None;
        };
        for &arg in ast.ids(args) {
            let number = match ast.exprs[arg].0 {
                Expr::Number(number) => number,
                Expr::Unary(Kind::Minus, id) => {
                    let Expr::Number(number) = ast.exprs[id].0 else {
                        return None;
                    };
                    -number
                }
                _ => return None,
            };
            if !number.is_finite() {
                return None;
            }
        }
        let (target, span) = &ast.exprs[receiver];
        if matches!(target, Expr::Name(_)) {
            return (span.binding.local_index() == Some(0)).then_some(Callback {
                start: start as usize,
                end: end as usize,
                line: end_line,
                prototype: 0,
            });
        }
        id = receiver;
    }
    None
}

fn named_constructor(ast: &Ast, mut id: ExprId) -> bool {
    loop {
        match ast.exprs[id].0 {
            Expr::Name(_) => return true,
            Expr::Index(parent, key) if matches!(ast.exprs[key].0, Expr::String(_)) => id = parent,
            _ => return false,
        }
    }
}

fn visit_constructor(
    ast: &Ast,
    id: ExprId,
    callbacks: &mut Vec<Callback>,
    depth: usize,
) -> Option<()> {
    if depth == 64 {
        return None;
    }
    match ast.exprs[id].0 {
        Expr::Table(fields, _) => {
            for &(key, value) in ast.fields(fields) {
                if key
                    .is_some_and(|id| !matches!(ast.exprs[id].0, Expr::String(_) | Expr::Number(_)))
                {
                    return None;
                }
                visit_constructor(ast, value, callbacks, depth + 1)?;
            }
        }
        Expr::Call(target, args, _) if named_constructor(ast, target) => {
            let &[arg] = ast.ids(args) else { return None };
            if !matches!(ast.exprs[arg].0, Expr::Table(..)) {
                return None;
            }
            visit_constructor(ast, arg, callbacks, depth + 1)?;
        }
        Expr::Function { .. } => callbacks.try_push(method_callback(ast, id)?).ok()?,
        Expr::Number(_) | Expr::String(_) | Expr::Nil | Expr::Boolean(_) => {}
        _ => return None,
    }
    Some(())
}

fn plain_text(bytes: &[u8]) -> bool {
    bytes.len() <= MAX_TEXT
        && bytes
            .iter()
            .all(|&byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'_' | b'-' | b'.'))
}

#[derive(Clone, Copy)]
enum LiteralKind {
    Text(u8),
    Number { negative: bool },
}

#[derive(Clone, Copy)]
struct Literal {
    start: usize,
    end: usize,
    slot: usize,
    kind: LiteralKind,
}

#[derive(Clone, Copy, Default)]
enum Replacement {
    #[default]
    Original,
    Number(f64),
    Text {
        start: usize,
        end: usize,
    },
}

impl Replacement {
    fn equal(self, other: Self, source: &[u8]) -> bool {
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Text { start, end }, Self::Text { start: b, end: e }) => {
                source[start..end] == source[b..e]
            }
            _ => false,
        }
    }
}

struct Template {
    program: Shared<ProgramData>,
    source: Box<[u8]>,
    literals: Box<[Literal]>,
    offsets: Box<[usize]>,
    replacements: Box<[Replacement]>,
    depth: usize,
    locale: u64,
}

#[derive(Default)]
pub(super) struct Templates {
    entries: Vec<Template>,
    bytes: usize,
    recent: [Option<u64>; RECENT_SHAPES],
    next_recent: usize,
}

fn return_value(ast: &Ast) -> Option<ExprId> {
    let &[statement] = ast.ids(ast.body) else {
        return None;
    };
    let Stmt::Return(values, _) = ast.stmts[statement].0 else {
        return None;
    };
    let &[root] = ast.ids(values) else {
        return None;
    };
    Some(root)
}

// A repeated hint justifies scanning for literal slots. The exact matcher
// still checks every literal and the source bytes between them.
fn syntax_hint(ast: &Ast, source: &[u8], name: &[u8], depth: usize, locale: u64) -> u64 {
    use crate::hash::hash_bytes;
    let mut hint =
        hash_bytes(name) ^ locale ^ (depth as u64).rotate_left(17) ^ ast.exprs.len() as u64;
    for (expr, _) in &ast.exprs {
        let text = match expr {
            Expr::Name(name) | Expr::Method(_, name, _, _) => name,
            Expr::Index(_, key) => {
                let Expr::String(key) = &ast.exprs[*key].0 else {
                    continue;
                };
                key
            }
            Expr::String(name) => {
                if let Lexeme::Source { start, .. } = *name
                    && start != 0
                    && matches!(source[start as usize - 1], b'\'' | b'"')
                {
                    continue;
                }
                name
            }
            _ => continue,
        };
        hint = hint.rotate_left(5) ^ hash_bytes(text.bytes(source));
    }
    hint
}

impl Template {
    fn bytes(&self) -> usize {
        self.program.heap_bytes()
            + ProgramData::heap_bytes(&self.program)
            + self.source.len()
            + size_of_val(self.literals.as_ref())
            + size_of_val(self.offsets.as_ref())
            + size_of_val(self.replacements.as_ref())
    }

    fn build(
        source: &[u8],
        depth: usize,
        locale: u64,
        program: &Program,
        ast: &Ast,
        root: ExprId,
    ) -> Option<Self> {
        let base = &program.data;
        if !source.is_ascii() {
            return None;
        }
        let mut callbacks = fallible::with_capacity(base.prototypes.len() - 1).ok()?;
        visit_constructor(ast, root, &mut callbacks, 0)?;
        if callbacks.len() + 1 != base.prototypes.len()
            || callbacks.windows(2).any(|pair| pair[0].end > pair[1].start)
        {
            return None;
        }
        let entry = base.entry as usize;
        let root = &base.prototypes[entry];
        let mut closures = root.code.iter().filter(|i| i.op == Op::Closure);
        for callback in &mut callbacks {
            let closure = closures.next()?;
            let prototype = &base.prototypes[closure.c as usize];
            if prototype.defined != [callback.line; 2]
                || !prototype.upvalues.is_empty()
                || prototype.params != 1
                || prototype.vararg
            {
                return None;
            }
            callback.prototype = closure.c as usize;
        }
        if closures.next().is_some() {
            return None;
        }
        let mut offsets = fallible::with_capacity(base.prototypes.len()).ok()?;
        let mut total = 0;
        for p in &base.prototypes {
            offsets.push(total);
            total += p.constants().len();
        }
        if total > MAX_CONSTANTS {
            return None;
        }
        let mut fixed = fallible::repeat(false, total).ok()?;
        // A lookup key can also name a call in the debug metadata.
        for (expr, _) in &ast.exprs {
            if let Expr::Index(_, id) = *expr
                && let Expr::String(key) = &ast.exprs[id].0
                && let Some(slot) = root.constants().iter().position(|value| {
                    value.string(&root.constant_strings) == Some(key.bytes(source))
                })
            {
                fixed[offsets[entry] + slot] = true;
            }
        }
        let mut literals = Vec::new();
        let mut scanner = Scanner::default();
        let mut token = Token::default();
        let mut previous = Kind::Eof;
        let mut next_callback = 0;
        loop {
            let before = scanner.position();
            scanner.next_closed(source, &mut token).ok()?;
            if token.kind == Kind::Eof {
                break;
            }
            let end = scanner.position();
            while callbacks
                .get(next_callback)
                .is_some_and(|callback| end > callback.end)
            {
                next_callback += 1;
            }
            let scope = callbacks
                .get(next_callback)
                .filter(|callback| end > callback.start);
            match (scope, token.kind) {
                (None, Kind::Name | Kind::String) => {
                    let bytes = token.text.bytes(source);
                    let key = root
                        .constants()
                        .iter()
                        .position(|value| value.string(&root.constant_strings) == Some(bytes));
                    if let Some(key) = key {
                        let slot = offsets[entry] + key;
                        if token.kind == Kind::String
                            && let Lexeme::Source { start, end, .. } = token.text
                            && let Some(quote @ (b'\'' | b'"')) = start
                                .checked_sub(1)
                                .and_then(|pos| source.get(pos as usize))
                                .copied()
                            && source.get(end as usize) == Some(&quote)
                            && plain_text(bytes)
                        {
                            literals
                                .try_push(Literal {
                                    start: start as usize,
                                    end: end as usize,
                                    slot,
                                    kind: LiteralKind::Text(quote),
                                })
                                .ok()?;
                        } else {
                            fixed[slot] = true;
                        }
                    }
                }
                (Some(callback), Kind::Number) => {
                    let negative = previous == Kind::Minus;
                    let number = if negative {
                        -token.number
                    } else {
                        token.number
                    };
                    let p = &base.prototypes[callback.prototype];
                    let key = p
                        .constants()
                        .iter()
                        .position(|value| matches!(value, Constant::Number(n) if *n == number))?;
                    let slot = offsets[callback.prototype] + key;
                    let start = before
                        + source[before..end]
                            .iter()
                            .position(|&byte| !crate::locale::ascii_space(byte))?;
                    if number != 0.0 && short_decimal_integer(&source[start..end]).is_some() {
                        literals
                            .try_push(Literal {
                                start,
                                end,
                                slot,
                                kind: LiteralKind::Number { negative },
                            })
                            .ok()?;
                    } else {
                        fixed[slot] = true;
                    }
                }
                _ => {}
            }
            previous = token.kind;
        }
        literals.retain(|literal| !fixed[literal.slot]);
        if literals.is_empty() {
            return None;
        }
        Some(Self {
            program: base.clone(),
            source: source.try_boxed().ok()?,
            literals: literals.try_into_boxed_slice().ok()?,
            offsets: offsets.try_into_boxed_slice().ok()?,
            replacements: fallible::repeat(Replacement::Original, total)
                .ok()?
                .try_into_boxed_slice()
                .ok()?,
            depth,
            locale,
        })
    }

    fn match_source(&mut self, source: &[u8]) -> Option<()> {
        let mut previous = self.literals[0].start;
        let mut tail = source.strip_prefix(&self.source[..previous])?;
        self.replacements.fill(Replacement::Original);
        for &Literal {
            start,
            end,
            slot,
            kind,
        } in &self.literals
        {
            tail = tail.strip_prefix(&self.source[previous..start])?;
            let at = source.len() - tail.len();
            let (length, replacement) = match kind {
                LiteralKind::Number { negative } => {
                    let length = tail
                        .iter()
                        .take(16)
                        .take_while(|byte| byte.is_ascii_digit())
                        .count();
                    let value = short_decimal_integer(&tail[..length])?;
                    if value == 0.0 {
                        return None;
                    }
                    (
                        length,
                        Replacement::Number(if negative { -value } else { value }),
                    )
                }
                LiteralKind::Text(quote) => {
                    let length = tail
                        .iter()
                        .take(MAX_TEXT + 1)
                        .position(|&byte| byte == quote)?;
                    if !plain_text(&tail[..length]) {
                        return None;
                    }
                    (
                        length,
                        Replacement::Text {
                            start: at,
                            end: at + length,
                        },
                    )
                }
            };
            let old = &mut self.replacements[slot];
            if matches!(old, Replacement::Original) {
                *old = replacement;
            } else if !old.equal(replacement, source) {
                return None;
            }
            tail = &tail[length..];
            previous = end;
        }
        (tail == &self.source[previous..]).then_some(())?;
        self.distinct_constants(source).then_some(())
    }

    fn distinct_constants(&self, source: &[u8]) -> bool {
        for (id, p) in self.program.prototypes.iter().enumerate() {
            let original = p.constants();
            let pool = &self.replacements[self.offsets[id]..self.offsets[id] + original.len()];
            for (slot, &value) in pool.iter().enumerate() {
                if matches!(value, Replacement::Original) {
                    continue;
                }
                for (other, &replacement) in pool.iter().enumerate() {
                    if slot == other {
                        continue;
                    }
                    let equal = match (value, replacement, &original[other]) {
                        (Replacement::Number(a), Replacement::Original, Constant::Number(b)) => {
                            a == *b
                        }
                        (Replacement::Text { start, end }, Replacement::Original, constant) => {
                            constant.string(&p.constant_strings) == Some(&source[start..end])
                        }
                        _ => value.equal(replacement, source),
                    };
                    if equal {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn program(&self, source: &[u8]) -> Result<Program, Error> {
        Program::with_constant_pools(&self.program, false, |id, p: &Prototype| {
            let pool = &self.replacements[self.offsets[id]..self.offsets[id] + p.constants().len()];
            if pool
                .iter()
                .all(|value| matches!(value, Replacement::Original))
            {
                return Ok(None);
            }
            let mut constants = p.copy_constants()?;
            for (constant, &replacement) in constants.iter_mut().zip(pool) {
                match replacement {
                    Replacement::Original => {}
                    Replacement::Number(value) => *constant = Constant::Number(value),
                    Replacement::Text { start, end } => {
                        let bytes = &source[start..end];
                        if constant.string(&p.constant_strings) != Some(bytes) {
                            *constant = Constant::String(bytes.try_boxed()?);
                        }
                    }
                }
            }
            Ok(Some(constants.try_into_boxed_slice()?))
        })
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
            template.depth == depth
                && template.locale == locale
                && template.program.name.as_ref() == name
                && template.match_source(source).is_some()
        })?;
        self.entries[index..].rotate_left(1);
        let template = self.entries.last().expect("matched constructor template");
        Some(template.program(source))
    }

    pub(super) fn remember(
        &mut self,
        source: &[u8],
        depth: usize,
        locale: u64,
        program: &Program,
        ast: &Ast,
    ) {
        if source.len() > SPARE_SOURCE_LIMIT
            || !(2..=MAX_PROTOTYPES).contains(&program.data.prototypes.len())
            || locale != crate::locale::revision()
            || crate::memory::compilation::active()
        {
            return;
        }
        let Some(root) = return_value(ast) else {
            return;
        };
        let hint = syntax_hint(ast, source, &program.data.name, depth, locale);
        if !self.recent.contains(&Some(hint)) {
            self.recent[self.next_recent] = Some(hint);
            self.next_recent = (self.next_recent + 1) % RECENT_SHAPES;
            return;
        }
        let Some(template) = Template::build(source, depth, locale, program, ast, root) else {
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

    fn source(group: &str, label: &str, first: u32, second: u32) -> String {
        let pad = "x".repeat(2300);
        format!(
            "return Actor['{group}'] {{\n\
             Name='{label}', Caption='{label}',\n\
             On=function(self) self:set({first},{second},4.0) end, After=function(self) self:go(-{second}) end\n\
             }} --{pad}"
        )
    }

    fn compile(vm: &mut Vm, source: &str) -> Program {
        let name = b"@constructor.lua";
        let program = vm.compile_source(source.as_bytes(), name).unwrap();
        let reference = crate::compile_named(source.as_bytes(), name).unwrap();
        let dump = |program: &Program| {
            crate::chunk51::dump(&program.data, program.data.entry as usize).unwrap()
        };
        assert_eq!(dump(&program), dump(&reference));
        program
    }

    fn code(program: &Program) -> *const crate::bytecode::Instruction {
        program.data.prototypes[program.data.entry as usize]
            .code
            .as_ptr()
    }

    #[test]
    fn constructor_variants_reuse_layouts_and_keep_constants_independent() {
        let mut vm = Vm::new();
        compile(&mut vm, &source("One", "seed", 10, 20));
        let first = compile(&mut vm, &source("One", "first", 1, 2));
        compile(&mut vm, &source("Two", "seed", 17, 13));
        let second = compile(&mut vm, &source("Two", "different", 8, 3));
        let variant = compile(&mut vm, &source("One", "longer label", 123, 77));
        assert_ne!(code(&first), code(&second));
        assert_eq!(code(&first), code(&variant));
        for id in 0..2 {
            assert_eq!(
                first.data.prototypes[id].code.as_ptr(),
                variant.data.prototypes[id].code.as_ptr()
            );
        }
        let collision = compile(&mut vm, &source("One", "Name", 4, 77));
        assert_ne!(code(&first), code(&collision));
        let root = &first.data.prototypes[first.data.entry as usize];
        assert!(
            root.constants()
                .iter()
                .any(|constant| constant.string(&root.constant_strings) == Some(b"first"))
        );
        assert!(matches!(
            first.data.prototypes[0]
                .constants()
                .iter()
                .find(|value| matches!(value, Constant::Number(_))),
            Some(Constant::Number(1.0))
        ));
    }
}
