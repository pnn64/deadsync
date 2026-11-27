//! Direct bytecode emission for small closed source chunks.
use super::*;
use crate::lexer::{DirectScanner, DirectToken};

const MAX_DEPTH: u8 = 48;
// Leave headroom below the Lua 5.1 logical register limit. The direct emitter
// reuses registers sooner than the full parser's resource model.
const MAX_DIRECT_REGISTERS: u16 = 96;
// Comparisons use literal operands without reproducing spills from large pools.
const MAX_DIRECT_CONSTANTS: usize = 256;
// These two flags are private to emission and are cleared before publication.
const BOOLEAN_EDGE: u16 = 4;
pub(super) const HOOK_JUMP: u16 = 5;

/// Emit a closed chunk while parsing simple statements and precedence expressions.
/// The ordinary parser handles syntax and resource checks this path cannot model.
pub(crate) fn compile_direct(
    source: &[u8],
    name: &[u8],
    spare: &mut Spare,
) -> Result<Option<Program>, Error> {
    if source.len() > u32::MAX as usize {
        return Ok(None);
    }
    let mut scanner = DirectScanner::default();
    let mut token = DirectToken::default();
    match scanner.next(source, &mut token) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::Memory => return Err(error),
        Err(_) => return Ok(None),
    }
    if !matches!(
        token.kind,
        Kind::Local | Kind::Name | Kind::If | Kind::While | Kind::Repeat | Kind::Do | Kind::Eof
    ) {
        return Ok(None);
    }
    let ast = Ast::default();
    let mut function = match spare.functions.pop() {
        Some(function) => function,
        None => fallible::boxed(Function::default())?,
    };
    function.vararg = true;
    let mut names = std::mem::take(&mut spare.names);
    names.clear();
    let mut parser = Frontend {
        compiler: Compiler {
            ast: &ast,
            source,
            function,
            parents: Vec::new(),
            spare: Vec::new(),
            names,
            prototypes: Vec::new(),
            templates: templates::Templates::default(),
            numeric_templates: numeric_templates::NumericTemplates::default(),
            template_locale: None,
            folded: Vec::new(),
            tasks: Vec::new(),
            assignment_targets: Vec::new(),
            result: 0,
            boolean: boolean::Boolean::default(),
            line: 1,
        },
        scanner,
        token,
        previous_line: 1,
        last_label: None,
        nil_run: None,
    };
    let entry = match parser.compile() {
        Ok(entry) => Some(entry),
        Err(error) if error.kind() == ErrorKind::Memory => return Err(error),
        Err(_) => None,
    };
    let mut compiler = parser.compiler;
    if entry.is_none() {
        compiler.function.reset();
    }
    spare.functions.try_push(compiler.function)?;
    spare.names = compiler.names;
    let Some(entry) = entry else {
        return Ok(None);
    };
    let program = Program::try_new(compiler.prototypes, entry, name)?;
    match program.verify() {
        Ok(()) => Ok(Some(program)),
        Err(error) if error.kind() != ErrorKind::Memory => Ok(None),
        Err(error) => Err(error),
    }
}

#[derive(Clone, Copy)]
enum Value {
    Nil(u32),
    Constant(u32, bool),
    Local(u8),
    Global(u32),
    Register(u8),
    Call(u8, usize),
    Comparison(u8, bool),
    Not(u8, u32),
}

#[derive(Clone, Copy)]
struct Expression {
    value: Value,
    yes: boolean::List,
    no: boolean::List,
}

impl From<Value> for Expression {
    fn from(value: Value) -> Self {
        Self {
            value,
            yes: boolean::List::default(),
            no: boolean::List::default(),
        }
    }
}

struct Frontend<'a> {
    compiler: Compiler<'a>,
    scanner: DirectScanner,
    token: DirectToken,
    previous_line: u32,
    last_label: Option<usize>,
    nil_run: Option<(usize, u8, u8)>,
}

impl Frontend<'_> {
    fn compile(&mut self) -> Result<u32, Error> {
        self.block(0)?;
        let function = &self.compiler.function;
        if self.token.kind != Kind::Eof
            || function.max > MAX_DIRECT_REGISTERS
            || function.constants.len() > MAX_DIRECT_CONSTANTS
        {
            return Err(self.unsupported());
        }
        self.compiler.line = self.previous_line;
        self.compiler.function.debug_stack = self.compiler.function.max;
        self.compiler.emit(Op::Return, 0, 0, 0)?;
        self.compiler.prototypes = fallible::with_capacity(1)?;
        self.compiler.finish()
    }

    fn unsupported(&self) -> Error {
        Error::new_static(
            ErrorKind::Unsupported,
            self.token.line,
            "direct frontend fallback",
        )
    }

    fn advance(&mut self) -> Result<(), Error> {
        self.previous_line = self.token.end_line;
        self.scanner.next(self.compiler.source, &mut self.token)
    }

    fn take(&mut self, kind: Kind) -> Result<bool, Error> {
        if self.token.kind != kind {
            return Ok(false);
        }
        self.advance()?;
        Ok(true)
    }

    fn expect(&mut self, kind: Kind) -> Result<(), Error> {
        if !self.take(kind)? {
            return Err(self.unsupported());
        }
        Ok(())
    }

    fn name(&mut self) -> Result<Lexeme, Error> {
        if self.token.kind != Kind::Name {
            return Err(self.unsupported());
        }
        let name = self.scanner.take_text(self.token);
        self.advance()?;
        Ok(name)
    }

    fn lookup(&mut self, spelling: &Lexeme) -> Result<Value, Error> {
        let c = &mut self.compiler;
        let name = spelling.bytes(c.source);
        let head = name_head(name);
        if let Some(local) = c
            .function
            .locals
            .iter()
            .rev()
            .find(|local| local.name.equals(&c.names, name, head))
        {
            return Ok(Value::Local(local.reg));
        }
        c.string_constant(spelling).map(Value::Global)
    }

    fn write_value(&mut self, value: Value, dst: u8) -> Result<(), Error> {
        let c = &mut self.compiler;
        match value {
            Value::Nil(_) => self.load_nil(dst)?,
            Value::Constant(index, _) => {
                c.emit(Op::Load, dst, 0, index)?;
            }
            Value::Global(index) => {
                c.emit(Op::GetGlobal, dst, 0, index)?;
            }
            Value::Comparison(reg, negate) => {
                c.line = self.previous_line;
                c.emit(Op::Bool, dst, u16::from(reg), u32::from(negate))?;
            }
            Value::Not(reg, line) => {
                c.line = line;
                c.emit(Op::Not, dst, u16::from(reg), 0)?;
            }
            Value::Local(reg) | Value::Register(reg) | Value::Call(reg, _) if reg != dst => {
                c.emit(Op::Move, dst, u16::from(reg), 0)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn load_nil(&mut self, dst: u8) -> Result<(), Error> {
        let c = &mut self.compiler;
        let pc = c.function.code.len();
        // Lua merges neighboring LOADNIL ranges when there is no branch
        // entry between them. Keep the extra physical write invisible.
        let previous = self.nil_run.filter(|&(end, first, last)| {
            end == pc
                && self.last_label != Some(pc)
                && first <= dst
                && dst <= last.saturating_add(1)
        });
        if self.last_label == Some(pc) {
            let id = c.constant(Constant::Nil)?;
            c.emit(Op::Load, dst, 0, id)?;
        } else {
            c.load(dst, Constant::Nil)?;
        }
        if let Some((_, first, last)) = previous {
            c.debug_line(0);
            self.nil_run = Some((pc + 1, first, last.max(dst)));
        } else if c.function.code[pc].hook != 0 {
            self.nil_run = Some((pc + 1, dst, dst));
        } else {
            self.nil_run = None;
        }
        Ok(())
    }

    fn operand(&mut self, expression: Expression, dst: u8) -> Result<u16, Error> {
        if !expression.yes.is_empty() || !expression.no.is_empty() {
            self.write(expression, dst)?;
            return Ok(u16::from(dst));
        }
        Ok(match expression.value {
            Value::Nil(index) | Value::Constant(index, _) => (RK + index) as u16,
            Value::Local(reg) | Value::Register(reg) | Value::Call(reg, _) => u16::from(reg),
            Value::Global(_) | Value::Comparison(..) | Value::Not(..) => {
                self.write_value(expression.value, dst)?;
                u16::from(dst)
            }
        })
    }

    /// Selects one arm while letting the opposite arm fall through.
    fn branch(
        &mut self,
        mut expression: Expression,
        work: u8,
        taken_when: bool,
        line: u32,
    ) -> Result<boolean::List, Error> {
        let (reg, invert, flags) = match expression.value {
            Value::Comparison(reg, invert) => (reg, invert, COMPARE_BRANCH),
            Value::Not(reg, _) => (reg, true, BOOLEAN_EDGE),
            Value::Local(reg) | Value::Register(reg) | Value::Call(reg, _) => (reg, false, 0),
            Value::Nil(_) | Value::Constant(_, false) if taken_when => {
                let (taken, fallthrough) = edges(expression, taken_when);
                self.patch_edges(fallthrough, self.compiler.function.code.len());
                return Ok(taken);
            }
            Value::Constant(_, true) if !taken_when => {
                let (taken, fallthrough) = edges(expression, taken_when);
                self.patch_edges(fallthrough, self.compiler.function.code.len());
                return Ok(taken);
            }
            value => {
                self.compiler.line = line;
                self.write_value(value, work)?;
                (work, false, 0)
            }
        };
        let op = if taken_when ^ invert {
            Op::JumpTrue
        } else {
            Op::JumpFalse
        };
        let c = &mut self.compiler;
        c.line = line;
        let pc = c.emit(op, reg, flags, 0)?;
        if flags == COMPARE_BRANCH {
            c.debug_line(0);
        }
        c.list_push(
            if taken_when {
                &mut expression.yes
            } else {
                &mut expression.no
            },
            pc,
        );
        let (taken, fallthrough) = edges(expression, taken_when);
        self.patch_edges(fallthrough, self.compiler.function.code.len());
        Ok(taken)
    }

    fn patch_edges(&mut self, list: boolean::List, target: usize) {
        self.last_label = self.last_label.max(Some(target));
        let f = &mut *self.compiler.function;
        let at_next_instruction = target == f.code.len();
        let mut edges = list;
        while let Some(pc) = edges.take(&f.code) {
            let jump = &mut f.code[pc];
            if jump.b == BOOLEAN_EDGE {
                jump.b = 0;
            }
            if !at_next_instruction {
                jump.c = target as u32;
            }
        }
        if at_next_instruction {
            f.pending_jumps.join(&mut f.code, list);
        }
    }

    fn source_jump(&mut self) -> Result<boolean::List, Error> {
        let c = &mut self.compiler;
        let mut exits = std::mem::take(&mut c.function.pending_jumps);
        let pc = c.emit(Op::Jump, 0, HOOK_JUMP, 0)?;
        c.list_push(&mut exits, pc);
        Ok(exits)
    }

    fn finish_loop(&mut self) {
        let f = &mut *self.compiler.function;
        let (_, exits) = f.loops.pop().expect("a direct loop is open");
        self.patch_edges(exits, self.compiler.function.code.len());
    }

    fn require_edge_values(&self, mut list: boolean::List, dst: u8) -> (bool, bool) {
        let code = &self.compiler.function.code;
        let (mut boolean, mut copy) = (false, false);
        while let Some(pc) = list.take(code) {
            if code[pc].b != 0 {
                boolean = true;
            } else if code[pc].a != dst {
                copy = true;
            }
        }
        (boolean, copy)
    }

    fn write(&mut self, mut expression: Expression, dst: u8) -> Result<(), Error> {
        if expression.yes.is_empty() && expression.no.is_empty() {
            return self.write_value(expression.value, dst);
        }
        let comparison = matches!(expression.value, Value::Comparison(..));
        self.compiler.line = self.previous_line;
        if let Value::Comparison(reg, invert) = expression.value {
            let op = if invert { Op::JumpFalse } else { Op::JumpTrue };
            let c = &mut self.compiler;
            let pc = c.emit(op, reg, COMPARE_BRANCH, 0)?;
            c.debug_line(0);
            c.list_push(&mut expression.yes, pc);
        } else {
            self.write_value(expression.value, dst)?;
        }
        let (bool_no, copy_no) = self.require_edge_values(expression.no, dst);
        let (bool_yes, copy_yes) = self.require_edge_values(expression.yes, dst);
        let materialize = bool_no || bool_yes || comparison;
        let copy = copy_no || copy_yes;
        let skip = if !comparison && materialize {
            self.source_jump()?
        } else if !comparison && copy {
            let mut skip = boolean::List::default();
            let pc = self.compiler.emit(Op::Jump, 0, 0, 0)?;
            self.compiler.debug_line(0);
            self.compiler.list_push(&mut skip, pc);
            skip
        } else {
            boolean::List::default()
        };
        let mut exits = boolean::List::default();
        let (no, yes) = if materialize {
            let no = self.compiler.function.code.len();
            self.compiler.load(dst, Constant::Boolean(false))?;
            let over_true = self.compiler.emit(Op::Jump, 0, BOOLEAN_SKIP, 0)?;
            self.compiler.debug_line(0);
            let yes = self.compiler.function.code.len();
            self.compiler.load(dst, Constant::Boolean(true))?;
            self.compiler.patch(over_true);
            if copy {
                let pc = self.compiler.emit(Op::Jump, 0, 0, 0)?;
                self.compiler.debug_line(0);
                self.compiler.list_push(&mut exits, pc);
            }
            (no, yes)
        } else {
            (0, 0)
        };
        for (mut list, label) in [(expression.no, no), (expression.yes, yes)] {
            while let Some(pc) = list.take(&self.compiler.function.code) {
                let i = self.compiler.function.code[pc].instruction;
                if i.b != 0 {
                    let jump = &mut self.compiler.function.code[pc];
                    if jump.b == BOOLEAN_EDGE {
                        jump.b = 0;
                    }
                    jump.c = label as u32;
                } else if i.a == dst {
                    self.compiler.list_push(&mut exits, pc);
                } else {
                    self.compiler.patch(pc);
                    self.compiler.emit(Op::Move, dst, u16::from(i.a), 0)?;
                    self.compiler.debug_line(0);
                    let exit = self.compiler.emit(Op::Jump, 0, 0, 0)?;
                    self.compiler.debug_line(0);
                    self.compiler.list_push(&mut exits, exit);
                }
            }
        }
        let end = self.compiler.function.code.len();
        self.compiler.list_join(&mut exits, skip);
        self.patch_edges(exits, end);
        Ok(())
    }

    fn call(
        &mut self,
        name: &Lexeme,
        value: Value,
        dst: u8,
        wanted: u32,
        depth: u8,
    ) -> Result<Value, Error> {
        let call_line = self.token.line;
        if call_line != self.previous_line {
            return Err(self.unsupported());
        }
        self.write_value(value, dst)?;
        self.expect(Kind::LParen)?;
        let saved = self.compiler.function.next;
        let mut argc = 0;
        if !self.take(Kind::RParen)? {
            loop {
                let arg = self.compiler.alloc(1)?;
                let value = self.expression(arg, 0, depth + 1)?;
                argc += 1;
                if self.take(Kind::Comma)? {
                    self.compiler.line = self.previous_line;
                    self.write(value, arg)?;
                    continue;
                }
                self.expect(Kind::RParen)?;
                if let Value::Call(_, pc) = value.value {
                    self.compiler.function.code[pc].c = OPEN;
                    argc = OPEN_ARGS;
                } else {
                    self.compiler.line = self.previous_line;
                    self.write(value, arg)?;
                }
                break;
            }
        }
        let c = &mut self.compiler;
        c.line = call_line;
        let (name, kind) = match value {
            Value::Global(index) => (CallName::Constant(index), NameKind::Global),
            Value::Local(_) => (CallName::Text(name.bytes(c.source)), NameKind::Local),
            _ => unreachable!("direct call name"),
        };
        c.record_call_site(name, kind, dst)?;
        let pc = c.emit(Op::Call, dst, argc, wanted)?;
        c.reset(saved);
        Ok(Value::Call(dst, pc))
    }

    fn primary(&mut self, dst: u8, depth: u8) -> Result<Expression, Error> {
        let line = self.token.line;
        let value = match self.token.kind {
            Kind::Name => {
                let name = self.name()?;
                let value = self.lookup(&name)?;
                self.compiler.line = line;
                return if self.token.kind == Kind::LParen {
                    self.call(&name, value, dst, 1, depth).map(Into::into)
                } else {
                    Ok(value.into())
                };
            }
            Kind::LParen => {
                self.advance()?;
                let mut expression = self.expression(dst, 0, depth + 1)?;
                self.expect(Kind::RParen)?;
                if let Value::Call(reg, _) = expression.value {
                    expression.value = Value::Register(reg);
                }
                return Ok(expression);
            }
            Kind::Not => {
                self.advance()?;
                let mut expression = self.expression(dst, 7, depth + 1)?;
                expression.value = match expression.value {
                    Value::Comparison(reg, negate) => Value::Comparison(reg, !negate),
                    Value::Nil(_) | Value::Constant(_, false) => {
                        let id = self.compiler.constant(Constant::Boolean(true))?;
                        Value::Constant(id, true)
                    }
                    Value::Constant(_, true) => {
                        let id = self.compiler.constant(Constant::Boolean(false))?;
                        Value::Constant(id, false)
                    }
                    Value::Local(reg) | Value::Call(reg, _) | Value::Register(reg) => {
                        Value::Not(reg, self.previous_line)
                    }
                    value => {
                        self.write_value(value, dst)?;
                        Value::Not(dst, self.previous_line)
                    }
                };
                let code = &mut self.compiler.function.code;
                for mut list in [expression.no, expression.yes] {
                    while let Some(pc) = list.take(code) {
                        if code[pc].b == 0 {
                            code[pc].b = BOOLEAN_EDGE;
                        }
                    }
                }
                std::mem::swap(&mut expression.no, &mut expression.yes);
                return Ok(expression);
            }
            Kind::Nil => self.compiler.constant(Constant::Nil)?,
            Kind::True => self.compiler.constant(Constant::Boolean(true))?,
            Kind::False => self.compiler.constant(Constant::Boolean(false))?,
            Kind::Number if self.token.number() != 0.0 => self
                .compiler
                .constant(Constant::Number(self.token.number()))?,
            Kind::String => {
                let text = self.scanner.take_text(self.token);
                self.compiler.string_constant(&text)?
            }
            _ => return Err(self.unsupported()),
        };
        let literal = match self.token.kind {
            Kind::Nil => Value::Nil(value),
            Kind::False => Value::Constant(value, false),
            _ => Value::Constant(value, true),
        };
        self.advance()?;
        self.compiler.line = self.previous_line;
        Ok(literal.into())
    }

    fn expression(&mut self, dst: u8, limit: u8, depth: u8) -> Result<Expression, Error> {
        if depth >= MAX_DEPTH {
            return Err(self.unsupported());
        }
        let mut left = self.primary(dst, depth)?;
        while let Some((precedence, op)) = binary(self.token.kind) {
            if precedence <= limit {
                break;
            }
            self.advance()?;
            if matches!(op, Op::JumpFalse | Op::JumpTrue) {
                let taken_when = op == Op::JumpTrue;
                let pending = self.branch(left, dst, taken_when, self.previous_line)?;
                let mut right = self.expression(dst, precedence, depth + 1)?;
                self.compiler.list_join(
                    if taken_when {
                        &mut right.yes
                    } else {
                        &mut right.no
                    },
                    pending,
                );
                if let Value::Call(reg, _) = right.value {
                    right.value = Value::Register(reg);
                }
                left = right;
            } else {
                let lhs = self.operand(left, dst)?;
                let saved = self.compiler.function.next;
                let temp = if lhs == u16::from(dst) {
                    self.compiler.alloc(1)?
                } else {
                    dst
                };
                let right = self.expression(temp, precedence, depth + 1)?;
                let rhs = self.operand(right, temp)?;
                let c = &mut self.compiler;
                c.line = self.previous_line;
                c.emit(op, dst, lhs, u32::from(rhs))?;
                c.reset(saved);
                left = Value::Comparison(dst, false).into();
            }
        }
        Ok(left)
    }

    fn block(&mut self, depth: u8) -> Result<(), Error> {
        if depth >= MAX_DEPTH {
            return Err(self.unsupported());
        }
        while !matches!(
            self.token.kind,
            Kind::Eof | Kind::End | Kind::Else | Kind::ElseIf | Kind::Until
        ) {
            self.compiler.line = self.token.line;
            let terminal = match self.token.kind {
                Kind::Local => {
                    self.local(depth)?;
                    false
                }
                Kind::Name => {
                    self.assignment_or_call(depth)?;
                    false
                }
                Kind::If => {
                    self.if_statement(depth)?;
                    false
                }
                Kind::While => {
                    self.while_statement(depth)?;
                    false
                }
                Kind::Repeat => {
                    self.repeat_statement(depth)?;
                    false
                }
                Kind::Do => {
                    self.advance()?;
                    self.scoped_block(depth)?;
                    self.expect(Kind::End)?;
                    false
                }
                Kind::Break => {
                    if self.compiler.function.loops.is_empty() {
                        return Err(self.unsupported());
                    }
                    self.advance()?;
                    self.compiler.line = self.previous_line;
                    let exits = self.source_jump()?;
                    let f = &mut *self.compiler.function;
                    f.loops.last_mut().unwrap().1.join(&mut f.code, exits);
                    true
                }
                _ => return Err(self.unsupported()),
            };
            self.take(Kind::Semi)?;
            if terminal {
                break;
            }
        }
        Ok(())
    }

    fn local(&mut self, depth: u8) -> Result<(), Error> {
        self.advance()?;
        let name = self.name()?;
        let dst = self.compiler.alloc(1)?;
        if self.take(Kind::Assign)? {
            let value = self.expression(dst, 0, depth)?;
            self.write(value, dst)?;
        } else {
            self.load_nil(dst)?;
        }
        self.compiler.bind(name.bytes(self.compiler.source), dst)
    }

    fn assignment_or_call(&mut self, depth: u8) -> Result<(), Error> {
        let name = self.name()?;
        let target = self.lookup(&name)?;
        let saved = self.compiler.function.next;
        let temp = self.compiler.alloc(1)?;
        if self.take(Kind::Assign)? {
            let value = self.expression(temp, 0, depth)?;
            if let Value::Local(dst) = target {
                self.write(value, dst)?;
            } else if let Value::Global(index) = target {
                let src = if let Value::Local(reg) = value.value
                    && value.yes.is_empty()
                    && value.no.is_empty()
                {
                    reg
                } else {
                    self.write(value, temp)?;
                    temp
                };
                self.compiler.emit(Op::SetGlobal, src, 0, index)?;
            }
        } else if self.token.kind == Kind::LParen {
            self.call(&name, target, temp, 0, depth)?;
        } else {
            return Err(self.unsupported());
        }
        self.compiler.reset(saved);
        Ok(())
    }

    fn scoped_block(&mut self, depth: u8) -> Result<(), Error> {
        let saved = self.compiler.function.next;
        let locals = self.compiler.function.locals.len();
        self.block(depth + 1)?;
        self.compiler.end_locals(locals);
        self.compiler.reset(saved);
        Ok(())
    }

    fn condition(&mut self, depth: u8) -> Result<boolean::List, Error> {
        let saved = self.compiler.function.next;
        let temp = self.compiler.alloc(1)?;
        let mut value = self.expression(temp, 0, depth)?;
        if matches!(value.value, Value::Nil(_)) {
            let id = self.compiler.constant(Constant::Boolean(false))?;
            value.value = Value::Constant(id, false);
        }
        let pc = self.branch(value, temp, false, self.previous_line)?;
        self.compiler.reset(saved);
        Ok(pc)
    }

    fn if_statement(&mut self, depth: u8) -> Result<(), Error> {
        self.advance()?;
        let mut exits = boolean::List::default();
        loop {
            let no = self.condition(depth)?;
            self.expect(Kind::Then)?;
            self.scoped_block(depth)?;
            if !matches!(self.token.kind, Kind::Else | Kind::ElseIf) {
                self.patch_edges(no, self.compiler.function.code.len());
                break;
            }
            self.compiler.line = self.previous_line;
            let exit = self.source_jump()?;
            self.compiler.list_join(&mut exits, exit);
            self.patch_edges(no, self.compiler.function.code.len());
            if self.take(Kind::Else)? {
                self.scoped_block(depth)?;
                break;
            }
            self.expect(Kind::ElseIf)?;
        }
        self.expect(Kind::End)?;
        let pc = self.compiler.function.code.len();
        self.patch_edges(exits, pc);
        Ok(())
    }

    fn while_statement(&mut self, depth: u8) -> Result<(), Error> {
        self.advance()?;
        let head = self.compiler.function.code.len() as u32;
        self.last_label = Some(head as usize);
        let no = self.condition(depth)?;
        self.expect(Kind::Do)?;
        let f = &mut *self.compiler.function;
        f.loops.try_push((f.next, boolean::List::default()))?;
        self.scoped_block(depth)?;
        self.compiler.line = self.previous_line;
        let again = self.source_jump()?;
        self.patch_edges(again, head as usize);
        self.expect(Kind::End)?;
        self.patch_edges(no, self.compiler.function.code.len());
        self.finish_loop();
        Ok(())
    }

    fn repeat_statement(&mut self, depth: u8) -> Result<(), Error> {
        self.advance()?;
        let f = &mut *self.compiler.function;
        let head = f.code.len() as u32;
        self.last_label = Some(head as usize);
        let saved = f.next;
        let locals = f.locals.len();
        f.loops.try_push((saved, boolean::List::default()))?;
        self.block(depth + 1)?;
        self.expect(Kind::Until)?;
        let no = self.condition(depth)?;
        self.patch_edges(no, head as usize);
        self.compiler.end_locals(locals);
        self.compiler.reset(saved);
        self.finish_loop();
        Ok(())
    }
}

fn edges(expression: Expression, truth: bool) -> (boolean::List, boolean::List) {
    if truth {
        (expression.yes, expression.no)
    } else {
        (expression.no, expression.yes)
    }
}

fn binary(kind: Kind) -> Option<(u8, Op)> {
    Some(match kind {
        Kind::Or => (1, Op::JumpTrue),
        Kind::And => (2, Op::JumpFalse),
        Kind::Eq => (3, Op::Eq),
        Kind::Ne => (3, Op::Ne),
        Kind::Lt => (3, Op::Lt),
        Kind::Le => (3, Op::Le),
        Kind::Gt => (3, Op::Gt),
        Kind::Ge => (3, Op::Ge),
        _ => return None,
    })
}
