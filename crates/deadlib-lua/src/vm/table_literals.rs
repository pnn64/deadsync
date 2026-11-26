//! Decode a run of scalar list fields once per constructor and copy it in bulk.

use super::{Buffer, Error, FunctionId, Memory, Op, Table, Value, Vm, slot_index};
use crate::bytecode::{Instruction, Prototype};

const MIN_FIELDS: usize = 64;
// Lua's floating-byte encoding of the minimum array size.
pub(super) const MIN_ARRAY_HINT: u16 = 32;
const MAX_FIELDS: usize = 4096;
const FIELDS_PER_BATCH: usize = 50;

#[derive(Default)]
pub(super) struct Cache(Option<(FunctionId, usize, Option<Template>)>);

struct Template {
    values: Buffer<Value>,
    scratch: [Value; FIELDS_PER_BATCH],
    scratch_count: usize,
    end: usize,
    overwrite: Option<Overwrite>,
}

struct Overwrite {
    control: usize,
    index: Value,
    step: Value,
    end: usize,
}

pub(super) fn allocation_hint(hint: u32) -> Result<usize, Error> {
    if hint < 8 {
        return Ok(hint as usize);
    }
    1usize
        .checked_shl((hint >> 3) - 1)
        .and_then(|scale| scale.checked_mul((hint & 7) as usize + 8))
        .filter(|&size| size <= 1 << 26)
        .ok_or_else(|| Error::runtime_static("table overflow"))
}

fn scan_batch(
    code: &[Instruction],
    constants: &[Value],
    table: u8,
    offset: usize,
    values: &mut [Value; FIELDS_PER_BATCH],
) -> Option<usize> {
    for count in 0..=values.len() {
        let instruction = *code.get(count)?;
        if instruction.op == Op::SetList {
            return (count != 0
                && instruction.a == table
                && usize::from(instruction.b) == count
                && instruction.c as usize == offset + 1)
                .then_some(count);
        }
        if usize::from(instruction.a) != usize::from(table) + 1 + count {
            return None;
        }
        let value = match instruction.op {
            Op::Load if instruction.b == 0 => *constants.get(instruction.c as usize)?,
            _ => return None,
        };
        if !matches!(value, Value::Nil | Value::Boolean(_) | Value::Number(_)) {
            return None;
        }
        *values.get_mut(count)? = value;
    }
    None
}

impl Overwrite {
    fn decode(
        code: &[Instruction],
        constants: &[Value],
        pc: usize,
        table: u8,
        len: usize,
    ) -> Option<Self> {
        let [first, limit, step, prep, ..] = code.get(pc..)? else {
            return None;
        };
        let control = usize::from(prep.a);
        if prep.op != Op::ForPrep {
            return None;
        }
        let end = prep.c as usize;
        let backedge = *code.get(end)?;
        let start = pc + 4;
        if backedge.op != Op::ForLoop || backedge.a != prep.a || backedge.c as usize != start {
            return None;
        }
        let store = super::reductions::dense_loop_store(control, code.get(start..end)?)?;
        if store.a != table || u32::from(store.b) != store.c {
            return None;
        }
        // Only constant bounds run between the last literal batch and the stores.
        let mut bounds = [0.0; 3];
        for (position, load) in [first, limit, step].into_iter().enumerate() {
            if load.op != Op::Load || load.b != 0 || usize::from(load.a) != control + position {
                return None;
            }
            let Value::Number(value) = *constants.get(load.c as usize)? else {
                return None;
            };
            bounds[position] = value;
        }
        let [_, limit, step] = bounds;
        let last = len as f64;
        if bounds != [1.0, last, 1.0] && bounds != [last, 1.0, -1.0] {
            return None;
        }
        Some(Self {
            control,
            index: Value::Number(limit),
            step: Value::Number(step),
            end: end + 1,
        })
    }
}

impl Template {
    fn decode(
        code: &[Instruction],
        constants: &[Value],
        mut pc: usize,
        capacity: usize,
        native51: bool,
        memory: Memory,
    ) -> Option<Self> {
        let table = code[pc - 1].a;
        let mut values = Buffer::new_in(memory);
        let mut scratch = [Value::Nil; FIELDS_PER_BATCH];
        let mut batch = scratch;
        let mut scratch_count = 0;
        values.try_reserve_exact(capacity).ok()?;
        while let Some(count) = scan_batch(&code[pc..], constants, table, values.len(), &mut batch)
        {
            if values.len() + count > MAX_FIELDS {
                break;
            }
            values.try_extend_from_slice(&batch[..count]).ok()?;
            scratch[..count].copy_from_slice(&batch[..count]);
            scratch_count = scratch_count.max(count);
            pc += count + 1;
        }
        if values.len() < MIN_FIELDS {
            return None;
        }
        let len = if native51 {
            capacity.max(values.len())
        } else {
            values.len()
        };
        let overwrite = Overwrite::decode(code, constants, pc, table, len);
        Some(Self {
            values,
            scratch,
            scratch_count,
            end: pc,
            overwrite,
        })
    }
}

impl Vm {
    pub(super) fn copy_table_literal(
        &mut self,
        cache: &mut Cache,
        function: FunctionId,
        prototype: &Prototype,
        base: usize,
        pc: usize,
    ) -> Result<Option<usize>, Error> {
        // The skipped batches only install scalars. A hook, captured scratch
        // register or userdata finalizer could still observe the partial table.
        if !self.sort_fast_context()
            || !self.open_upvalues.is_empty()
            || self.gc_may_call_out()
            || !self.tables.memory().uses_global_allocator()
        {
            return Ok(None);
        }
        let instruction = prototype.code[pc - 1];
        let Ok(capacity) = allocation_hint(u32::from(instruction.b)) else {
            return Ok(None);
        };
        if capacity > MAX_FIELDS {
            return Ok(None);
        }
        if !cache
            .0
            .as_ref()
            .is_some_and(|key| key.0 == function && key.1 == pc)
        {
            let closure = &self.functions[slot_index(function.0)];
            let constants = self.programs[closure.program].bound_constants(closure.prototype);
            let template = Template::decode(
                &prototype.code,
                constants,
                pc,
                capacity,
                self.lua51_table_order,
                self.tables.memory().clone(),
            );
            cache.0 = Some((function, pc, template));
        }
        let Some((_, _, Some(template))) = &cache.0 else {
            return Ok(None);
        };
        let array = if self.lua51_table_order { capacity } else { 0 };
        let memory = self.tables.memory().clone();
        // A complete overwrite leaves the same array extent. Its values can
        // come from the same immutable backing as consecutive dense appends.
        let shared = template.overwrite.as_ref().and_then(|overwrite| {
            Table::with_dense_sequence(
                array.max(template.values.len()),
                self.lua51_table_order,
                memory.clone(),
            )
            .map(|table| (table, overwrite))
        });
        let (table, overwrite) = if let Some((table, overwrite)) = shared {
            (table, Some(overwrite))
        } else {
            (
                Table::with_scalar_array(&template.values, array, self.lua51_table_order, memory)
                    .map_err(|_| self.memory_failure())?,
                None,
            )
        };
        let id = self.try_insert_table(table, false)?;
        let slot = base + usize::from(instruction.a);
        self.registers[slot] = Value::Table(id);
        self.registers[slot + 1..slot + 1 + template.scratch_count]
            .copy_from_slice(&template.scratch[..template.scratch_count]);
        if let Some(overwrite) = overwrite {
            let control = base + overwrite.control;
            self.registers[control..control + 4].copy_from_slice(&[
                overwrite.index,
                overwrite.index,
                overwrite.step,
                overwrite.index,
            ]);
            return Ok(Some(overwrite.end));
        }
        Ok(Some(template.end))
    }
}
