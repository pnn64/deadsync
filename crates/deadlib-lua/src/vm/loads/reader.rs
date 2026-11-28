//! Check that a Lua reader cannot reach a callback that inspects its caller.
//! The VM still invokes the reader after every chunk. The proof covers both
//! sides of a branch and uses stable upvalue types across all invocations.
use super::*;
use crate::bytecode::RK;

const REGISTERS: usize = 16;
const STEPS: usize = 72;

pub(super) fn captured_source(vm: &Vm, reader: Value, fragment: &[u8]) -> Option<StringId> {
    let Value::Function(id) = reader else {
        return None;
    };
    let closure = &vm.functions[slot_index(id.0)];
    closure.upvalues.iter().find_map(|&cell| {
        let Value::String(source) = vm.cell_value(cell as usize) else {
            return None;
        };
        let bytes = &vm.strings.bytes[slot_index(source.0)];
        // ASCII source keeps the same lexer classifications across locale changes.
        (bytes.len() > fragment.len()
            && bytes.len() <= SPARE_SOURCE_LIMIT
            && bytes.starts_with(fragment)
            && bytes.is_ascii())
        .then_some(source)
    })
}

#[derive(Clone, Copy)]
enum ReaderValue {
    Known(Value),
    Number,
    String,
    Unknown,
}
impl ReaderValue {
    fn from_value(value: Value) -> Self {
        if matches!(value, Value::Number(_)) {
            Self::Number
        } else {
            Self::Known(value)
        }
    }

    fn is_string(self) -> bool {
        matches!(self, Self::String | Self::Known(Value::String(_)))
    }
}

struct Reader<'a> {
    vm: &'a Vm,
    closure: &'a Closure,
    code: &'a [crate::bytecode::Instruction],
    constants: &'a [Value],
}

#[cold]
#[inline(never)]
pub(super) fn cannot_inspect_stack(vm: &Vm, reader: Value) -> bool {
    let Value::Function(id) = reader else {
        return false;
    };
    let closure = &vm.functions[slot_index(id.0)];
    let program = &vm.programs[closure.program];
    let prototype = &program.data.prototypes[closure.prototype];
    if prototype.compat_arg
        || usize::from(prototype.registers) > REGISTERS
        || prototype.code().len() > STEPS
    {
        return false;
    }
    let mut remaining = STEPS;
    Reader {
        vm,
        closure,
        code: prototype.code(),
        constants: program.bound_constants(closure.prototype),
    }
    .check(
        0,
        [ReaderValue::Known(Value::Nil); REGISTERS],
        None,
        &mut remaining,
    )
}

impl Reader<'_> {
    fn call_result(
        values: &[ReaderValue; REGISTERS],
        a: usize,
        count: usize,
    ) -> Option<ReaderValue> {
        use ReaderValue::{Known, Number, String, Unknown};
        let args = values.get(a + 1..=a.checked_add(count)?)?;
        match (values[a], args) {
            (Known(Value::Native(0)), &[result]) => Some(result),
            (Known(Value::Native(debug::GET_INFO)), [target, options @ ..])
                if matches!(
                    target,
                    Number | Known(Value::Function(_) | Value::Native(_))
                ) && (options.is_empty()
                    || matches!(options, [Known(Value::Nil)])
                    || matches!(options, [value] if value.is_string())) =>
            {
                // getinfo reads frame and prototype metadata, independent of
                // the parser's temporary stack values. It returns a table or nil.
                Some(Unknown)
            }
            (
                Known(Value::Native(strings::STR_SUB)),
                [string, Number] | [string, Number, Number],
            ) if string.is_string() => Some(String),
            _ => None,
        }
    }

    fn check(
        &self,
        mut pc: usize,
        mut values: [ReaderValue; REGISTERS],
        mut open_top: Option<usize>,
        remaining: &mut usize,
    ) -> bool {
        use ReaderValue::{Known, Number, Unknown};
        while *remaining != 0 {
            *remaining -= 1;
            let Some(i) = self.code.get(pc).copied() else {
                return false;
            };
            let a = usize::from(i.a);
            let b = usize::from(i.b);
            let c = i.c as usize;
            pc += 1;
            let operand = |op: usize| {
                if op < RK as usize {
                    values[op]
                } else {
                    ReaderValue::from_value(self.constants[op - RK as usize])
                }
            };
            match i.op {
                Op::Load if b == 0 => values[a] = ReaderValue::from_value(self.constants[c]),
                // No eligible instruction creates a closure in the current frame.
                Op::Load | Op::Close => {}
                Op::Move if i.c != OPEN => values[a] = values[b],
                Op::Not => values[a] = Unknown,
                Op::Bool => values[a] = if c == 0 { values[b] } else { Unknown },
                Op::GetUpvalue => {
                    values[a] = ReaderValue::from_value(
                        self.vm.cell_value(self.closure.upvalues[c] as usize),
                    )
                }
                Op::SetUpvalue
                    if matches!(values[a], Number)
                        && matches!(
                            self.vm.cell_value(self.closure.upvalues[c] as usize),
                            Value::Number(_)
                        ) => {}
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow
                    if matches!((operand(b), operand(c)), (Number, Number)) =>
                {
                    values[a] = Number;
                }
                Op::Neg if matches!(values[b], Number) => values[a] = Number,
                Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge
                    if matches!((operand(b), operand(c)), (Number, Number)) =>
                {
                    values[a] = Unknown;
                }
                Op::Len if values[b].is_string() || matches!(values[b], Known(Value::Table(_))) => {
                    values[a] = Number;
                }
                Op::GetGlobal => {
                    let env = &self.vm.tables[slot_index(self.closure.env.0)];
                    if env.metatable().is_some() {
                        return false;
                    }
                    values[a] = ReaderValue::from_value(env.get(self.constants[c]));
                }
                Op::GetTable => {
                    let key = operand(c);
                    values[a] = match values[b] {
                        Known(Value::Table(id)) => {
                            let table = &self.vm.tables[slot_index(id.0)];
                            if table.metatable().is_some() {
                                return false;
                            }
                            if let Known(key) = key {
                                ReaderValue::from_value(table.get(key))
                            } else {
                                Unknown
                            }
                        }
                        Known(string @ Value::String(_)) => {
                            let (Known(key), Value::Table(methods)) =
                                (key, self.vm.metafield(string, Meta::Index))
                            else {
                                return false;
                            };
                            let Value::Native(strings::STR_SUB) =
                                self.vm.tables[slot_index(methods.0)].get(key)
                            else {
                                return false;
                            };
                            Known(Value::Native(strings::STR_SUB))
                        }
                        _ => return false,
                    };
                }
                Op::Call | Op::TailCall if i.op == Op::TailCall || i.c <= 1 || i.c == OPEN => {
                    let count = if i.b == OPEN_ARGS {
                        open_top.and_then(|top| top.checked_sub(a + 1))
                    } else {
                        Some(b)
                    };
                    let Some(result) = count.and_then(|count| Self::call_result(&values, a, count))
                    else {
                        return false;
                    };
                    if i.op == Op::TailCall {
                        return true;
                    }
                    values[a..].fill(Unknown);
                    if i.c != 0 {
                        values[a] = result;
                    }
                    // All the allowed calls produce one result on success.
                    open_top = (i.c == OPEN).then_some(a + 1);
                }
                Op::Jump if b == 0 && c >= pc => pc = c,
                Op::JumpFalse | Op::JumpTrue if c >= pc => {
                    if !self.check(c, values, open_top, remaining) {
                        return false;
                    }
                }
                Op::Return if i.c != OPEN => return true,
                _ => return false,
            }
        }
        false
    }
}
