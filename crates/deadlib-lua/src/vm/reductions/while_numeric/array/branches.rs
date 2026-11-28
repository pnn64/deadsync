//! Replay side-effect-free branches and numeric calls over dense arrays.
use super::super::super::{Reduction, integer};
use super::exact::{ArrayReduction, Exact, MIN_LENGTH, repair_blocks};
use crate::arena::Arena;
use crate::bytecode::{COMPARE_BRANCH, Instruction, Op, Prototype, RK};
use crate::table::{ARRAY_WATCH_BLOCKS, ArrayChange, Table, array_index};
use crate::vm::{
    Cell, FrameKind, FunctionId, TableId, Value, Vm,
    forwarding::{MAX_VALUES, numeric},
    slot_index,
};
use std::ops::Range;

const MAX_REGISTERS: usize = 32;
const MAX_INSTRUCTIONS: usize = 48;
const MAX_CAPTURES: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Capture {
    Table(TableId),
    Function(FunctionId),
}

#[derive(Debug, Eq, PartialEq)]
struct Key {
    caller: FunctionId,
    head: usize,
    bounds: (i64, i64),
    ascending: bool,
    captures: [Option<(u32, Capture)>; MAX_CAPTURES],
    tail_targets: [Option<FunctionId>; MAX_VALUES + 1],
}

#[derive(Debug)]
pub(super) struct CachedBranch {
    key: Key,
    exact: Exact,
}

struct Plan<'a> {
    key: Key,
    code: &'a [Instruction],
    tables: [TableId; MAX_CAPTURES],
    table_count: usize,
    callee: Option<FunctionId>,
    calls: [Option<numeric::Plan>; MAX_VALUES + 1],
    control: usize,
    sum: usize,
}

struct Inputs<'a> {
    arrays: [&'a [Value]; MAX_CAPTURES],
    constants: &'a [Value],
}

#[derive(Clone, Copy)]
enum Slot {
    Empty,
    Index,
    Scalar(Value),
    Sum(f64, usize),
}

impl Slot {
    fn number(self) -> Option<f64> {
        let Self::Scalar(Value::Number(number)) = self else {
            return None;
        };
        Some(number)
    }

    fn value(self) -> Option<Value> {
        let Self::Scalar(value) = self else {
            return None;
        };
        Some(value)
    }
}

impl<'a> Plan<'a> {
    fn decode(
        vm: &Vm,
        prototype: &'a Prototype,
        head: usize,
        edge: usize,
        control: usize,
        bounds: (i64, i64),
        ascending: bool,
    ) -> Option<Self> {
        let FrameKind::Lua(caller) = vm.frames.last()?.function else {
            return None;
        };
        let closure = &vm.functions[slot_index(caller.0)];
        let code = prototype.code.get(head..edge)?;
        let update = code.iter().rfind(|i| i.op != Op::Close)?;
        if update.op != Op::Add || usize::from(prototype.registers) > MAX_REGISTERS {
            return None;
        }
        let sum = usize::from(update.a);
        let live = prototype.gc_maps[prototype.metadata.get(edge + 1)?.gc_live as u16 as usize][0];
        let mut captures = [None; MAX_CAPTURES];
        let mut tables = [TableId(0); MAX_CAPTURES];
        let mut table_count = 0;
        let mut arities = 0u16;
        let mut scratch = 0u64;
        for (offset, instruction) in code.iter().enumerate() {
            let a = usize::from(instruction.a);
            let b = instruction.b;
            let c = instruction.c;
            let width = match instruction.op {
                Op::Close => continue,
                Op::Jump | Op::JumpFalse | Op::JumpTrue => {
                    let valid_mode = b == 0
                        || instruction.op != Op::Jump
                            && b == COMPARE_BRANCH
                            && offset.checked_sub(1).is_some_and(|previous| {
                                let previous = code[previous];
                                matches!(previous.op, Op::Eq | Op::Ne)
                                    && previous.a == instruction.a
                            });
                    if !valid_mode || !((head + offset + 1)..=edge).contains(&(c as usize)) {
                        return None;
                    }
                    continue;
                }
                Op::GetUpvalue => {
                    let cell = closure.upvalues[c as usize] as usize;
                    let capture = match vm.upvalues[cell] {
                        Cell::Closed(Value::Table(id)) => {
                            if !tables[..table_count].contains(&id) {
                                if vm.tables[slot_index(id.0)].metatable().is_some() {
                                    return None;
                                }
                                *tables.get_mut(table_count)? = id;
                                table_count += 1;
                            }
                            Capture::Table(id)
                        }
                        Cell::Closed(Value::Function(id)) => Capture::Function(id),
                        _ => return None,
                    };
                    let binding = captures.iter_mut().find(|binding| {
                        binding.as_ref().is_none_or(|&(capture, _)| capture == c)
                    })?;
                    *binding = Some((c, capture));
                    1
                }
                Op::Call if usize::from(b) <= MAX_VALUES && (1..=2).contains(&c) => {
                    arities |= 1 << b;
                    c as usize
                }
                Op::Load if b == 0 => 1,
                Op::Move if c == 0 => 1,
                Op::GetTable | Op::Add | Op::Sub | Op::Mul | Op::Neg | Op::Eq | Op::Ne => 1,
                _ => return None,
            };
            if a < control + 4 && a + width > control {
                return None;
            }
            scratch |= ((1 << width) - 1) << a;
        }
        // Skipped branches may leave different temporaries. Only the sum can
        // survive past the final iteration. Native chunks mark all stack
        // slots live and use ordinary dispatch if those writes are visible.
        scratch &= !(1 << sum);
        if scratch & live != 0
            || prototype.debug.locals().iter().any(|local| {
                local.start as usize <= edge + 1
                    && edge + 1 < local.end as usize
                    && scratch
                        .checked_shr(u32::from(local.register))
                        .is_some_and(|bits| bits & 1 != 0)
            })
            || table_count == 0
        {
            return None;
        }
        let callee = if arities == 0 {
            None
        } else {
            let mut functions =
                captures
                    .iter()
                    .flatten()
                    .filter_map(|(_, capture)| match *capture {
                        Capture::Function(id) => Some(id),
                        Capture::Table(_) => None,
                    });
            let callee = functions.next()?;
            if functions.any(|id| id != callee) {
                return None;
            }
            Some(callee)
        };
        let mut calls = std::array::from_fn(|_| None);
        let mut tail_targets = [None; MAX_VALUES + 1];
        for (arity, plan) in calls.iter_mut().enumerate() {
            if arities & (1 << arity) != 0 {
                let decoded = numeric::Plan::build(vm, callee?, arity)?;
                tail_targets[arity] = decoded.tail_target();
                *plan = Some(decoded);
            }
        }
        Some(Self {
            key: Key {
                caller,
                head,
                bounds,
                ascending,
                captures,
                tail_targets,
            },
            code,
            tables,
            table_count,
            callee,
            calls,
            control,
            sum,
        })
    }

    fn row(
        &self,
        inputs: &Inputs<'_>,
        index: usize,
        sum: f64,
        exact: &mut Option<Exact>,
        term_count: &mut usize,
    ) -> Option<f64> {
        let mut slots = [Slot::Empty; MAX_REGISTERS];
        slots[self.control + 3] = Slot::Index;
        slots[self.sum] = Slot::Sum(sum, 0);
        let mut additions = 0;
        let mut called = false;
        let mut pc = 0;
        while let Some(&Instruction { op, a, b, c }) = self.code.get(pc) {
            pc += 1;
            let a = usize::from(a);
            let operand = |key| {
                if key < RK {
                    slots[key as usize]
                } else {
                    Slot::Scalar(inputs.constants[(key - RK) as usize])
                }
            };
            let value = match op {
                Op::Close => continue,
                Op::Jump => {
                    pc = c as usize - self.key.head;
                    continue;
                }
                Op::JumpFalse | Op::JumpTrue => {
                    if slots[a].value()?.is_truthy() == (op == Op::JumpTrue) {
                        pc = c as usize - self.key.head;
                    }
                    continue;
                }
                Op::GetUpvalue => {
                    let &(_, capture) = self
                        .key
                        .captures
                        .iter()
                        .flatten()
                        .find(|(id, _)| *id == c)?;
                    Slot::Scalar(match capture {
                        Capture::Table(id) => Value::Table(id),
                        Capture::Function(id) => Value::Function(id),
                    })
                }
                Op::GetTable => {
                    let Value::Table(id) = operand(u32::from(b)).value()? else {
                        return None;
                    };
                    let array = self.tables[..self.table_count]
                        .iter()
                        .position(|&table| table == id)?;
                    let offset = match operand(c) {
                        Slot::Index => index,
                        key => array_index(key.value()?)?,
                    };
                    Slot::Scalar(*inputs.arrays[array].get(offset)?)
                }
                Op::Load => Slot::Scalar(inputs.constants[c as usize]),
                Op::Move => operand(u32::from(b)),
                Op::Eq | Op::Ne => {
                    let left = operand(u32::from(b)).value()?;
                    let right = operand(c).value()?;
                    let equal = left == right;
                    if !equal
                        && matches!(
                            (left, right),
                            (Value::Table(_), Value::Table(_))
                                | (Value::Userdata(_), Value::Userdata(_))
                        )
                    {
                        // Distinct table and userdata values may share __eq.
                        return None;
                    }
                    Slot::Scalar(Value::Boolean(equal == (op == Op::Eq)))
                }
                Op::Call => {
                    let count = usize::from(b);
                    if called || slots[a].value()? != Value::Function(self.callee?) {
                        return None;
                    }
                    let mut arguments = [0.0; MAX_VALUES];
                    for (output, slot) in arguments.iter_mut().zip(&slots[a + 1..a + 1 + count]) {
                        *output = slot.number()?;
                    }
                    let results = self.calls[count].as_ref()?.evaluate(&arguments);
                    for (output, result) in slots[a..a + c as usize].iter_mut().zip(results) {
                        *output = Slot::Scalar(result.map_or(Value::Nil, Value::Number));
                    }
                    called = true;
                    continue;
                }
                Op::Add | Op::Sub | Op::Mul => {
                    let left = operand(u32::from(b));
                    let right = operand(c).number()?;
                    if op == Op::Add
                        && let Slot::Sum(number, version) = left
                        && version == additions
                    {
                        if let Some(proof) = exact
                            && proof.push(right).is_none()
                        {
                            *exact = None;
                        }
                        additions += 1;
                        Slot::Sum(number + right, additions)
                    } else {
                        Slot::Scalar(Value::Number(Vm::number_op(op, left.number()?, right)))
                    }
                }
                Op::Neg => Slot::Scalar(Value::Number(-operand(u32::from(b)).number()?)),
                _ => unreachable!("validated branched loop opcode"),
            };
            slots[a] = value;
        }
        let Slot::Sum(sum, version) = slots[self.sum] else {
            return None;
        };
        if (self.callee.is_some() && !called) || version == 0 || version != additions {
            return None;
        }
        *term_count = term_count.checked_add(additions)?;
        Some(sum)
    }

    fn inputs<'vm>(&self, tables: &'vm Arena<Table>, constants: &'vm [Value]) -> Inputs<'vm> {
        let mut arrays = [&[][..]; MAX_CAPTURES];
        for (array, id) in arrays.iter_mut().zip(&self.tables[..self.table_count]) {
            *array = tables[slot_index(id.0)].array_values();
        }
        Inputs { arrays, constants }
    }

    fn prefix_and_last(&self) -> (Range<usize>, usize) {
        // The plan is only built for positive u32 bounds.
        let first = self.key.bounds.0 as usize - 1;
        let last = self.key.bounds.1 as usize - 1;
        if self.key.ascending {
            (first..last, last)
        } else {
            (first + 1..last + 1, first)
        }
    }

    fn scan_rows(
        &self,
        inputs: &Inputs<'_>,
        range: Range<usize>,
        mut sum: f64,
        proof: &mut Option<Exact>,
        terms: &mut usize,
    ) -> Option<f64> {
        for offset in 0..range.len() {
            let index = if self.key.ascending {
                range.start + offset
            } else {
                range.end - 1 - offset
            };
            sum = self.row(inputs, index, sum, proof, terms)?;
        }
        Some(sum)
    }

    fn scan(&self, inputs: &Inputs<'_>, sum: f64, remember: bool) -> Option<(f64, Option<Exact>)> {
        let (prefix, last) = self.prefix_and_last();
        let mut proof = remember.then_some(Exact::ZERO);
        let mut terms = 0;
        let sum = self.scan_rows(inputs, prefix, sum, &mut proof, &mut terms)?;
        self.row(inputs, last, sum, &mut None, &mut 0)?;
        Some((sum, proof.and_then(|proof| proof.check_count(terms, 1))))
    }

    fn constant_read_tables(&self) -> Option<u8> {
        let mut destinations = 0u64;
        for instruction in self.code {
            if matches!(instruction.op, Op::Jump | Op::JumpTrue | Op::JumpFalse) {
                destinations |= 1 << (instruction.c as usize - self.key.head);
            }
        }
        let mut tables = 0;
        for (position, read) in self.code.iter().enumerate() {
            if read.op != Op::GetTable || read.c as usize == self.control + 3 {
                continue;
            }
            // A fixed lookup can depend on a row outside the changed block.
            // Prove its table is loaded unconditionally immediately before it.
            if read.c < RK || destinations & (1 << position) != 0 {
                return None;
            }
            let load = self.code.get(position.checked_sub(1)?)?;
            if load.op != Op::GetUpvalue || u16::from(load.a) != read.b {
                return None;
            }
            let &(_, Capture::Table(id)) = self
                .key
                .captures
                .iter()
                .flatten()
                .find(|(capture, _)| *capture == load.c)?
            else {
                return None;
            };
            tables |= 1
                << self.tables[..self.table_count]
                    .iter()
                    .position(|&table| table == id)?;
        }
        Some(tables)
    }

    fn repair(
        &self,
        tables: &Arena<Table>,
        inputs: &Inputs<'_>,
        cache: &mut ArrayReduction,
        initial: f64,
    ) -> Option<f64> {
        let constant_reads = self.constant_read_tables()?;
        let mut dirty = [0; MAX_CAPTURES * ARRAY_WATCH_BLOCKS];
        let mut count = 0;
        for (index, id) in self.tables[..self.table_count].iter().enumerate() {
            match tables[slot_index(id.0)].array_change() {
                ArrayChange::Clean => {}
                ArrayChange::Dirty { blocks, len } if constant_reads & (1 << index) == 0 => {
                    for &block in &blocks[..len] {
                        if !dirty[..count].contains(&block) {
                            dirty[count] = block;
                            count += 1;
                        }
                    }
                }
                _ => return None,
            }
        }
        if count == 0 {
            return None;
        }
        let (prefix, last) = self.prefix_and_last();
        let (cached, blocks) = cache.branch_blocks()?;
        // Each instruction can contribute at most one term per row.
        let exact = repair_blocks(
            blocks,
            prefix.start,
            prefix.len(),
            &dirty[..count],
            self.code.len(),
            |range| {
                let mut proof = Some(Exact::ZERO);
                let start = prefix.start;
                self.scan_rows(
                    inputs,
                    start + range.start..start + range.end,
                    0.0,
                    &mut proof,
                    &mut 0,
                )?;
                proof
            },
        );
        let result = exact.and_then(|exact| {
            let sum = exact.apply(initial)?;
            self.row(inputs, last, sum, &mut None, &mut 0)?;
            cached.exact = exact;
            Some(sum)
        });
        if result.is_none() {
            blocks.clear();
        }
        result
    }
}

impl Vm {
    pub(in crate::vm) fn reduce_branched_array_loop(
        &mut self,
        base: usize,
        control: usize,
        head: usize,
        edge: usize,
        prototype: &Prototype,
    ) -> Option<Reduction> {
        let body = prototype.code.get(head..edge)?;
        if body.len() > MAX_INSTRUCTIONS
            || !body
                .iter()
                .any(|i| i.op == Op::JumpFalse || i.op == Op::JumpTrue)
            || !body.iter().any(|i| i.op == Op::GetTable)
            || !self.sort_fast_context()
            || !self.open_upvalues.is_empty()
        {
            return None;
        }
        let start = base + control;
        let index = integer(self.registers[start])?;
        let limit = integer(self.registers[start + 1])?;
        let step @ (-1 | 1) = integer(self.registers[start + 2])? else {
            return None;
        };
        let first = index + step;
        let (lower, upper) = if step > 0 {
            (first, limit)
        } else {
            (limit, first)
        };
        if lower < 1 || upper > i128::from(u32::MAX) || lower >= upper {
            return None;
        }
        let plan = Plan::decode(
            self,
            prototype,
            head,
            edge,
            control,
            (lower as i64, upper as i64),
            step > 0,
        )?;
        let Value::Number(initial) = self.registers[base + plan.sum] else {
            return None;
        };
        if !self.gc_checkpoint_idle()
            || plan
                .calls
                .iter()
                .flatten()
                .any(|call| !call.has_stack_space(self))
        {
            return Some(Reduction::Retry);
        }
        let watched = &plan.tables[..plan.table_count];
        let cached = self
            .array_reduction
            .cached_branch()
            .filter(|cached| cached.key == plan.key);
        let clean_sum = cached.and_then(|entry| {
            watched
                .iter()
                .all(|id| self.tables[slot_index(id.0)].array_change() == ArrayChange::Clean)
                .then(|| entry.exact.apply(initial))?
        });
        let (sum, exact, repaired) = if let Some(sum) = clean_sum {
            (sum, None, false)
        } else {
            let closure = &self.functions[slot_index(plan.key.caller.0)];
            let constants = self.programs[closure.program].bound_constants(closure.prototype);
            let inputs = plan.inputs(&self.tables, constants);
            let repaired = cached
                .is_some()
                .then(|| plan.repair(&self.tables, &inputs, &mut self.array_reduction, initial))
                .flatten();
            if let Some(sum) = repaired {
                (sum, None, true)
            } else {
                let (sum, exact) = plan.scan(
                    &inputs,
                    initial,
                    upper - lower + 1 >= i128::from(MIN_LENGTH),
                )?;
                (sum, exact, false)
            }
        };
        let sum_slot = plan.sum;
        let tables = plan.tables;
        let table_count = plan.table_count;
        let watch = exact.is_some() || repaired;
        if let Some(exact) = exact {
            self.array_reduction.cache_branch(CachedBranch {
                key: plan.key,
                exact,
            });
        }
        if watch {
            for id in &tables[..table_count] {
                self.tables[slot_index(id.0)].watch_array();
            }
        }
        self.gc.pending = false;
        // Publish only the proven accumulator and index. Execute the final
        // iteration to restore its visible registers and call result width.
        self.registers[start] = Value::Number((limit - step) as f64);
        self.registers[base + sum_slot] = Value::Number(sum);
        Some(Reduction::Applied(
            (upper - lower) as u64 * (body.len() as u64 + 1),
        ))
    }
}
