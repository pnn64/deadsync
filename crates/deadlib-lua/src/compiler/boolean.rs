//! Flat boolean lowering with pending short-circuit edges in instruction slots.
use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct List {
    head: u32,
    tail: u32,
}
#[derive(Clone, Copy)]
enum Value {
    Constant(ExprId),
    Boolean(bool),
    Local(u8),
    Register(u8),
    Comparison(u8, bool),
}
#[derive(Clone, Copy)]
pub(super) struct Boolean {
    value: Value,
    origin: ExprId,
    yes: List,
    no: List,
}
impl Default for Boolean {
    fn default() -> Self {
        Self {
            value: Value::Boolean(false),
            origin: 0,
            yes: List::default(),
            no: List::default(),
        }
    }
}
impl Compiler<'_> {
    fn list_push(&mut self, list: &mut List, pc: usize) {
        let link = pc as u32 + 1;
        if list.tail != 0 {
            self.function.code[list.tail as usize - 1].c = link;
        } else {
            list.head = link;
        }
        list.tail = link;
        self.function.code[pc].c = 0;
    }
    fn list_join(&mut self, left: &mut List, right: List) {
        if right.head == 0 {
            return;
        }
        if left.tail != 0 {
            self.function.code[left.tail as usize - 1].c = right.head;
        } else {
            left.head = right.head;
        }
        left.tail = right.tail;
    }
    fn list_patch(&mut self, mut list: List, target: usize) {
        while list.head != 0 {
            let pc = list.head as usize - 1;
            list.head = self.function.code[pc].c;
            self.function.code[pc].c = target as u32;
        }
    }
    fn pop_boolean_code(&mut self) -> Instruction {
        self.function.hook_visible -= usize::from(self.function.hook_lines.pop().unwrap() != 0);
        self.function.lines.pop();
        self.function.debug_lines.pop();
        self.function.gc_live.pop();
        self.function.code.pop().unwrap()
    }
    pub(super) fn boolean_node(&mut self, id: ExprId, work: u8) {
        let inner = self.unparen(id);
        match self.ast.exprs[inner].0 {
            Expr::Binary(Kind::And | Kind::Or, left, ..) => {
                self.tasks.push(Task::BooleanLeft(inner, work));
                self.tasks.push(Task::BooleanNode(left, work));
            }
            Expr::Unary(Kind::Not, child) => {
                self.tasks.push(Task::BooleanNot(inner, work));
                self.tasks.push(Task::BooleanNode(child, work));
            }
            _ if self.constant_truth(inner).is_some() => {
                self.boolean = Boolean {
                    value: Value::Constant(inner),
                    origin: id,
                    ..Boolean::default()
                };
            }
            _ if self.local_operand(inner).is_some() => {
                self.boolean = Boolean {
                    value: Value::Local(self.local_operand(inner).unwrap()),
                    origin: id,
                    ..Boolean::default()
                };
            }
            _ => {
                self.tasks.push(Task::BooleanLeaf(id, work));
                self.tasks.push(Task::Expr(id, work));
            }
        }
    }
    pub(super) fn boolean_leaf(&mut self, id: ExprId, work: u8) {
        let value = if matches!(
            self.ast.exprs[self.unparen(id)].0,
            Expr::Binary(
                Kind::Eq | Kind::Ne | Kind::Lt | Kind::Le | Kind::Gt | Kind::Ge,
                ..
            )
        ) {
            let materializer = self.pop_boolean_code();
            assert!(
                materializer.op == Op::Bool && materializer.a == work,
                "comparison leaf materializer"
            );
            Value::Comparison(materializer.b as u8, materializer.c != 0)
        } else {
            Value::Register(work)
        };
        self.boolean = Boolean {
            value,
            origin: id,
            ..Boolean::default()
        };
    }
    pub(super) fn boolean_not(&mut self, id: ExprId, work: u8) -> Result<(), Error> {
        let mut value = self.boolean;
        std::mem::swap(&mut value.yes, &mut value.no);
        self.line = self.ast.exprs[id].1.end;
        value.value = match value.value {
            Value::Constant(id) => Value::Boolean(!self.constant_truth(id).unwrap()),
            Value::Boolean(v) => Value::Boolean(!v),
            Value::Comparison(reg, invert) => Value::Comparison(reg, !invert),
            Value::Local(reg) | Value::Register(reg) => {
                self.emit(Op::Not, work, u16::from(reg), 0)?;
                Value::Register(work)
            }
        };
        value.origin = id;
        self.boolean = value;
        Ok(())
    }
    fn boolean_condition(
        &mut self,
        value: &mut Boolean,
        wanted: bool,
        work: u8,
        line: u32,
    ) -> Result<(), Error> {
        self.line = line;
        let (reg, invert, comparison) = match value.value {
            Value::Constant(id) => {
                if self.constant_truth(id) == Some(!wanted) {
                    return Ok(());
                }
                match &self.ast.exprs[id].0 {
                    Expr::Nil => self.load(work, Constant::Nil)?,
                    Expr::Boolean(v) => self.load(work, Constant::Boolean(*v))?,
                    Expr::String(v) => self.load_string(work, v)?,
                    _ => self.load(work, Constant::Number(self.folded[id].unwrap()))?,
                }
                (work, false, false)
            }
            Value::Boolean(v) => {
                if v != wanted {
                    return Ok(());
                }
                self.load(work, Constant::Boolean(v))?;
                (work, false, false)
            }
            Value::Local(reg) => (reg, false, false),
            Value::Comparison(reg, invert) => (reg, invert, true),
            Value::Register(reg) => {
                if self
                    .function
                    .code
                    .last()
                    .is_some_and(|i| i.op == Op::Not && i.a == reg)
                {
                    let instruction = self.pop_boolean_code();
                    (instruction.b as u8, true, false)
                } else {
                    self.discharge_line(value.origin, line, LineUse::Condition);
                    (reg, false, false)
                }
            }
        };
        let pc = self.emit(
            if wanted ^ invert {
                Op::JumpTrue
            } else {
                Op::JumpFalse
            },
            reg,
            if comparison { COMPARE_BRANCH } else { 0 },
            0,
        )?;
        if comparison {
            self.debug_line(0);
        }
        self.list_push(
            if wanted {
                &mut value.yes
            } else {
                &mut value.no
            },
            pc,
        );
        Ok(())
    }
    pub(super) fn boolean_left(&mut self, id: ExprId, work: u8) -> Result<(), Error> {
        let Expr::Binary(op, _, right, line) = self.ast.exprs[id].0 else {
            unreachable!("boolean logical node")
        };
        let mut value = self.boolean;
        self.boolean_condition(&mut value, op == Kind::Or, work, line)?;
        let (pending, continuing) = if op == Kind::Or {
            (value.yes, value.no)
        } else {
            (value.no, value.yes)
        };
        self.list_patch(continuing, self.function.code.len());
        self.tasks.push(Task::BooleanRight(op, pending));
        self.tasks.push(Task::BooleanNode(right, work));
        Ok(())
    }
    pub(super) fn boolean_right(&mut self, op: Kind, pending: List) {
        let mut value = self.boolean;
        self.list_join(
            if op == Kind::Or {
                &mut value.yes
            } else {
                &mut value.no
            },
            pending,
        );
        self.boolean = value;
    }
    pub(super) fn boolean_finish(&mut self, id: ExprId, dst: u8) -> Result<(), Error> {
        let mut value = self.boolean;
        self.line = self.ast.exprs[id].1.end;
        let comparison = matches!(value.value, Value::Comparison(..));
        if comparison {
            self.boolean_condition(&mut value, true, dst, self.line)?;
        } else {
            match value.value {
                Value::Boolean(v) => self.load(dst, Constant::Boolean(v))?,
                Value::Register(reg) => {
                    assert!(
                        self.function
                            .code
                            .last()
                            .is_some_and(|i| i.op == Op::Not && i.a == reg),
                        "final boolean negation"
                    );
                    let pc = self.function.code.len() - 1;
                    self.function.code[pc].a = dst;
                    self.live_register(u16::from(dst));
                    let after = self.live_map()?;
                    self.function.gc_live[pc] =
                        (self.function.gc_live[pc] & 0xffff) | (u32::from(after) << 16);
                }
                _ => unreachable!("negated boolean result"),
            }
        }
        if value.yes.head != 0 || value.no.head != 0 {
            let done = if comparison {
                None
            } else {
                Some(self.emit(Op::Jump, 0, 0, 0)?)
            };
            let no = self.function.code.len();
            self.load(dst, Constant::Boolean(false))?;
            let skip = self.emit(Op::Jump, 0, BOOLEAN_SKIP, 0)?;
            self.debug_line(0);
            let yes = self.function.code.len();
            self.load(dst, Constant::Boolean(true))?;
            self.patch(skip);
            if let Some(done) = done {
                self.patch(done);
            }
            self.list_patch(value.no, no);
            self.list_patch(value.yes, yes);
        }
        self.result = dst;
        Ok(())
    }
}
