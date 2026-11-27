//! Fuse a guarded quotient/remainder table lookup and scalar state update.
use super::{Arena, FunctionId, StringId, Table, TableId, Value, Vm, math, simple, slot_index};
use crate::bytecode::{Instruction, OPEN, OPEN_ARGS, Op, RK};

const REGISTER_LIMIT: usize = 16;
const NODE_LIMIT: usize = 16;
const MATH_FLOOR: u64 = 33;

#[derive(Clone, Copy, PartialEq)]
enum Input {
    Constant(Value),
    Argument(u8),
    Temporary(u8),
}

const NIL: Input = Input::Constant(Value::Nil);

#[derive(Clone, Copy)]
enum Node {
    Global(StringId),
    Table(Input, Input),
    Arithmetic(Op, Input, Input),
    Call(Input, Input, Option<Input>),
}

struct Graph {
    nodes: [Option<Node>; NODE_LIMIT],
    used: u16,
    len: u8,
}

impl Graph {
    fn add(&mut self, node: Node) -> Option<Input> {
        *self.nodes.get_mut(usize::from(self.len))? = Some(node);
        let input = Input::Temporary(self.len);
        self.len += 1;
        Some(input)
    }

    fn take(&mut self, input: Input) -> Option<Node> {
        let Input::Temporary(index) = input else {
            return None;
        };
        self.used |= 1 << index;
        self.nodes[usize::from(index)]
    }

    fn global(&mut self, key: StringId) -> Option<Input> {
        let prior = self.nodes[..usize::from(self.len)]
            .iter()
            .position(|node| matches!(node, Some(Node::Global(cached)) if *cached == key));
        match prior {
            Some(index) => Some(Input::Temporary(index as u8)),
            None => self.add(Node::Global(key)),
        }
    }
}

struct Plan {
    receiver: usize,
    numerator: usize,
    radix_key: StringId,
    math_key: StringId,
    floor_key: StringId,
    remainder_key: StringId,
    state_key: StringId,
    state_first: bool,
    offset: f64,
    registers: usize,
    line: u32,
}

impl Plan {
    fn build(vm: &Vm, function: FunctionId) -> Option<Self> {
        let closure = &vm.functions[slot_index(function.0)];
        let program = &vm.programs[closure.program];
        let proto = &program.data.prototypes[closure.prototype];
        let registers = usize::from(proto.registers);
        if proto.vararg
            || proto.compat_arg
            || !proto.upvalues.is_empty()
            || proto.code.len() > 32
            || registers > REGISTER_LIMIT
        {
            return None;
        }
        let constants = program.bound_constants(closure.prototype);
        let mut slots = [NIL; REGISTER_LIMIT];
        for (index, slot) in slots[..usize::from(proto.params)].iter_mut().enumerate() {
            *slot = Input::Argument(index as u8);
        }
        let mut graph = Graph {
            nodes: [None; NODE_LIMIT],
            used: 0,
            len: 0,
        };
        let mut write = None;
        for (pc, i) in proto.code.iter().enumerate() {
            let a = usize::from(i.a);
            let b = usize::from(i.b);
            let operand = |index: u32| {
                if index < RK {
                    slots[index as usize]
                } else {
                    Input::Constant(constants[(index - RK) as usize])
                }
            };
            slots[a] = match i.op {
                Op::Move if i.c == 0 => slots[b],
                Op::Load if i.b == 0 => Input::Constant(constants[i.c as usize]),
                Op::Load | Op::Close => continue,
                Op::GetGlobal if write.is_none() => {
                    let Value::String(key) = constants[i.c as usize] else {
                        return None;
                    };
                    graph.global(key)?
                }
                Op::GetTable => {
                    if let Some((_, _, position)) = write
                        && position != graph.len
                    {
                        return None;
                    }
                    graph.add(Node::Table(slots[b], operand(i.c)))?
                }
                op @ (Op::Div | Op::Add | Op::Mul) if write.is_none() => {
                    graph.add(Node::Arithmetic(op, operand(u32::from(i.b)), operand(i.c)))?
                }
                Op::Call if write.is_none() && (1..=2).contains(&i.b) && i.c == 1 => {
                    let infer_second = (i.b == 2).then(|| slots[a + 2]);
                    graph.add(Node::Call(slots[a], slots[a + 1], infer_second))?
                }
                Op::SetGlobal if write.is_none() => {
                    let Value::String(key) = constants[i.c as usize] else {
                        return None;
                    };
                    write = Some((key, slots[a], graph.len));
                    continue;
                }
                Op::Return if i.c == 1 => {
                    let (key, value, position) = write?;
                    if slots[a] != Input::Temporary(position) {
                        return None;
                    }
                    return Self::recognize(
                        graph,
                        slots[a],
                        key,
                        value,
                        registers,
                        proto.instruction_line(pc),
                    );
                }
                _ => return None,
            };
        }
        None
    }

    fn recognize(
        mut graph: Graph,
        result: Input,
        state_key: StringId,
        replacement: Input,
        registers: usize,
        line: u32,
    ) -> Option<Self> {
        let Node::Table(receiver @ Input::Argument(table), index) = graph.take(result)? else {
            return None;
        };
        let Node::Arithmetic(Op::Add, remainder, Input::Constant(Value::Number(offset))) =
            graph.take(index)?
        else {
            return None;
        };
        let Node::Call(remainder_fn, rounded, Some(radix)) = graph.take(remainder)? else {
            return None;
        };
        let Node::Table(radix_table, Input::Constant(Value::String(radix_key))) =
            graph.take(radix)?
        else {
            return None;
        };
        let Node::Call(floor_fn, quotient, None) = graph.take(rounded)? else {
            return None;
        };
        let Node::Arithmetic(Op::Div, Input::Argument(numerator), state) = graph.take(quotient)?
        else {
            return None;
        };
        let Node::Global(state_input) = graph.take(state)? else {
            return None;
        };
        let Node::Arithmetic(Op::Mul, first, second) = graph.take(replacement)? else {
            return None;
        };
        let Node::Table(math, Input::Constant(Value::String(floor_key))) = graph.take(floor_fn)?
        else {
            return None;
        };
        let Node::Table(other_math, Input::Constant(Value::String(remainder_key))) =
            graph.take(remainder_fn)?
        else {
            return None;
        };
        let Node::Global(math_key) = graph.take(math)? else {
            return None;
        };
        let state_first = first == state && second == radix;
        if radix_table != receiver
            || state_input != state_key
            || other_math != math
            || (!state_first && (first != radix || second != state))
            || graph.used != ((1u32 << graph.len) - 1) as u16
        {
            return None;
        }
        Some(Self {
            receiver: usize::from(table),
            numerator: usize::from(numerator),
            radix_key,
            math_key,
            floor_key,
            remainder_key,
            state_key,
            state_first,
            offset,
            registers,
            line,
        })
    }
}

struct Cached {
    plan: Plan,
    slots: [u32; 5],
}

type CacheEntry = (FunctionId, Option<Cached>);

#[derive(Default)]
pub(super) struct Cache([Option<CacheEntry>; 4]);

impl Cache {
    #[inline]
    fn get(&mut self, vm: &Vm, function: FunctionId) -> Option<&mut Cached> {
        let slot = &mut self.0[slot_index(function.0) % 4];
        if slot.as_ref().is_none_or(|(id, _)| *id != function) {
            *slot = Some((
                function,
                Plan::build(vm, function).map(|plan| Cached {
                    plan,
                    slots: [0; 5],
                }),
            ));
        }
        slot.as_mut()?.1.as_mut()
    }
}

pub(super) struct Window<'a> {
    pub code: &'a [Instruction],
    pub hints: &'a mut [[u32; 2]; simple::FIELD_HINTS],
    pub caller: FunctionId,
    pub env: TableId,
    pub base: usize,
    pub next: usize,
}

impl Plan {
    #[inline]
    fn select(
        &self,
        tables: &Arena<Table>,
        args: &[Value],
        env: TableId,
        state: f64,
        radix_hint: &mut u32,
    ) -> Option<(Value, f64)> {
        let (&Value::Table(receiver), &Value::Number(numerator)) =
            (args.get(self.receiver)?, args.get(self.numerator)?)
        else {
            return None;
        };
        let table = &tables[slot_index(receiver.0)];
        let radix = if receiver == env && self.radix_key == self.state_key {
            state
        } else {
            let Value::Number(radix) = table.get_string_cached(self.radix_key, radix_hint) else {
                return None;
            };
            radix
        };
        let index = (numerator / state).floor() % radix + self.offset;
        let result = table.get(Value::Number(index));
        if result == Value::Nil && table.metatable().is_some() {
            return None;
        }
        let number = if self.state_first {
            state * radix
        } else {
            radix * state
        };
        Some((result, number))
    }
}

impl Vm {
    #[inline(never)]
    pub(super) fn try_radix_selection(
        &mut self,
        cache: &mut Cache,
        function: FunctionId,
        window: Window<'_>,
        error_line: &mut u32,
    ) -> Option<usize> {
        let Cached {
            plan,
            slots: [radix_hint, math_hint, floor_hint, rem_hint, state_hint],
        } = cache.get(self, function)?;
        let first = window.code[window.next - 1];
        let call = window.base + usize::from(first.a);
        let argc = usize::from(first.b);
        let frame_count = self.frames.len() + 1;
        if argc <= plan.receiver.max(plan.numerator)
            || frame_count >= self.frames.capacity()
            || frame_count + 1 >= usize::from(self.call_slots)
            || plan.registers > self.registers.capacity() - self.registers.len()
            || !self.sort_fast_context()
            || !self.sort_gc_ready()
        {
            return None;
        }
        let env = self.functions[slot_index(function.0)].env;
        let globals = &self.tables[slot_index(env.0)];
        let (Value::Number(state), Value::Table(math)) = (
            globals.get_string_cached(plan.state_key, state_hint),
            globals.get_string_cached(plan.math_key, math_hint),
        ) else {
            return None;
        };
        let math = &self.tables[slot_index(math.0)];
        if math.get_string_cached(plan.floor_key, floor_hint) != Value::Native(MATH_FLOOR)
            || math.get_string_cached(plan.remainder_key, rem_hint)
                != Value::Native(math::MATH_FMOD)
        {
            return None;
        }
        let (result, mut state) = plan.select(
            &self.tables,
            &self.registers[call + 1..call + 1 + argc],
            env,
            state,
            radix_hint,
        )?;
        if !self.tables[slot_index(env.0)].set_string_scalar_cached(
            plan.state_key,
            Value::Number(state),
            state_hint,
        ) {
            return None;
        }
        if first.c != 0 {
            self.registers[call] = result;
        }
        self.top = call + first.c as usize;
        *error_line = plan.line;

        let caller = &self.functions[slot_index(window.caller.0)];
        let constants = self.programs[caller.program].bound_constants(caller.prototype);
        let mut pc = window.next;
        let mut writes = 0u32;
        // The window never allocates, calls user code, or writes a table.
        // Keep the one scalar update local and publish it before leaving.
        loop {
            let i = window.code[pc];
            let slot = window.base + usize::from(i.a);
            match i.op {
                Op::Move if i.c != OPEN => {
                    self.registers[slot] = self.registers[window.base + usize::from(i.b)];
                }
                Op::Load if i.b == 0 => self.registers[slot] = constants[i.c as usize],
                Op::Load => {}
                Op::GetGlobal => {
                    let Value::String(key) = constants[i.c as usize] else {
                        break;
                    };
                    let value = if window.env == env && key == plan.state_key {
                        Value::Number(state)
                    } else {
                        self.tables[slot_index(window.env.0)]
                            .get_string_cached(key, &mut window.hints[pc % simple::FIELD_HINTS][0])
                    };
                    if value == Value::Nil {
                        break;
                    }
                    self.registers[slot] = value;
                }
                Op::Call
                    if i.c <= 1
                        && i.b != OPEN_ARGS
                        && self.registers[slot] == Value::Function(function) =>
                {
                    let args = &self.registers[slot + 1..slot + 1 + usize::from(i.b)];
                    let Some((value, next)) =
                        plan.select(&self.tables, args, env, state, radix_hint)
                    else {
                        break;
                    };
                    if i.c != 0 {
                        self.registers[slot] = value;
                    }
                    self.top = slot + i.c as usize;
                    state = next;
                    writes += 1;
                }
                _ => break,
            }
            pc += 1;
        }
        if writes != 0 {
            let globals = &mut self.tables[slot_index(env.0)];
            assert!(globals.set_string_scalar_cached(
                plan.state_key,
                Value::Number(state),
                state_hint
            ));
            globals.advance_fields_version(writes - 1);
        }
        Some(pc)
    }
}
