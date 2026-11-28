//! Bulk initialization for runs of constant fields in table constructors.

use super::{Buffer, Error, FunctionId, Memory, Op, Table, Value, Vm, slot_index};
use crate::bytecode::{Instruction, Prototype, RK};

const MIN_FIELDS: usize = 64;
// Lua's floating-byte encoding of the minimum array size.
pub(super) const MIN_ARRAY_HINT: u16 = 32;
const MAX_FIELDS: usize = 4096;
pub(super) const MIN_WIDE_HINT: u16 = 80; // 4096 entries in floating-byte encoding.
const FIELDS_PER_BATCH: usize = 50;

#[derive(Default)]
pub(super) struct Cache {
    small: Option<(FunctionId, usize, Option<Template>)>,
    wide: Option<(FunctionId, usize, Option<Box<WideTemplate>>)>,
}

struct WideTemplate {
    table: Table,
    registers: [Option<Value>; 256],
    end: usize,
}

struct Template {
    values: Buffer<Value>,
    // Last write to each scratch register indexes the constructor values.
    scratch: [u16; FIELDS_PER_BATCH],
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

fn scan_batch<const STRINGS: bool>(
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
        if !(matches!(value, Value::Nil | Value::Boolean(_) | Value::Number(_))
            || STRINGS && matches!(value, Value::String(_)))
        {
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
    #[cold]
    #[inline(never)]
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
        let mut scratch = [0; FIELDS_PER_BATCH];
        let mut batch = [Value::Nil; FIELDS_PER_BATCH];
        let mut scratch_count = 0;
        values.try_reserve_exact(capacity).ok()?;
        while let Some(count) =
            scan_batch::<false>(&code[pc..], constants, table, values.len(), &mut batch)
        {
            if values.len() + count > MAX_FIELDS {
                break;
            }
            let start = values.len();
            values.try_extend_from_slice(&batch[..count]).ok()?;
            for (offset, target) in scratch[..count].iter_mut().enumerate() {
                *target = (start + offset) as u16;
            }
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

struct RecordField {
    instructions: usize,
    key: Value,
    value: Value,
    loads: [Option<(u8, Value)>; 2],
}

// Both an RK operand and a register loaded immediately before the store
// can refer to constants. Stop before a store with a dynamic input.
fn record_field(code: &[Instruction], constants: &[Value], table: u8) -> Option<RecordField> {
    let mut loads = [None; 2];
    let mut n = 0;
    for load in &mut loads {
        let i = *code.get(n)?;
        if i.op != Op::Load || i.b != 0 || i.a == table {
            break;
        }
        *load = Some((i.a, *constants.get(i.c as usize)?));
        n += 1;
    }

    let store = *code.get(n)?;
    if store.op != Op::SetTable || store.a != table {
        return None;
    }
    let operand = |arg: u32| {
        if arg >= RK {
            constants.get((arg - RK) as usize).copied()
        } else {
            loads
                .iter()
                .rev()
                .flatten()
                .find_map(|&(r, value)| (u32::from(r) == arg).then_some(value))
        }
    };
    let key = operand(u32::from(store.b))?;
    let value = operand(store.c)?;
    if !matches!(key, Value::String(_))
        || !matches!(
            value,
            Value::Nil | Value::Boolean(_) | Value::Number(_) | Value::String(_)
        )
    {
        return None;
    }
    Some(RecordField {
        instructions: n + 1,
        key,
        value,
        loads,
    })
}

impl Vm {
    #[cold]
    #[inline(never)]
    pub(super) fn build_wide_table_literal(
        &mut self,
        cache: &mut Cache,
        function: FunctionId,
        prototype: &Prototype,
        base: usize,
        pc: usize,
    ) -> Result<Option<usize>, Error> {
        if !self.sort_fast_context()
            || !self.open_upvalues.is_empty()
            || self.gc_may_call_out()
            || !self.tables.memory().uses_global_allocator()
        {
            return Ok(None);
        }

        let new = prototype.code[pc - 1];
        // The active function keeps its bound strings alive. Slot generations
        // disambiguate constructors even if a previous function was collected.
        let seen = if let Some((owner, entry, snapshot)) = &mut cache.wide
            && *owner == function
            && *entry == pc
        {
            if let Some(template) = snapshot {
                if let Ok(table) = template.table.copy_literal() {
                    self.publish_wide_table(base, new.a, table, &template.registers)?;
                    return Ok(Some(template.end));
                }
                *snapshot = None;
                false
            } else {
                true
            }
        } else {
            cache.wide = Some((function, pc, None));
            false
        };
        let code = &prototype.code;
        let closure = &self.functions[slot_index(function.0)];
        let constants = self.programs[closure.program].bound_constants(closure.prototype);
        let Ok(capacity) = allocation_hint(if new.c == 0 { u32::from(new.b) } else { new.c })
        else {
            return Ok(None);
        };
        let memory = self.tables.memory().clone();
        let native = self.lua51_table_order;
        let mut end = pc;
        let mut register_values = [None; 256];
        let table = if new.c == 0 {
            let mut batch = [Value::Nil; FIELDS_PER_BATCH];
            let Some(count) = scan_batch::<true>(&code[end..], constants, new.a, 0, &mut batch)
            else {
                return Ok(None);
            };
            let mut values = Buffer::new_in(memory);
            if native {
                values.try_reserve_exact(capacity)?;
            }
            let start = usize::from(new.a) + 1;
            let mut count = count;
            loop {
                values.try_reserve(count)?;
                values.extend_from_slice(&batch[..count]);
                for (register, &value) in register_values[start..start + count]
                    .iter_mut()
                    .zip(&batch[..count])
                {
                    *register = Some(value);
                }
                end += count + 1;
                let Some(next) =
                    scan_batch::<true>(&code[end..], constants, new.a, values.len(), &mut batch)
                else {
                    break;
                };
                if values.len() + next > capacity {
                    break;
                }
                count = next;
            }
            if native {
                values.resize(capacity, Value::Nil);
            }
            Table::from_literal_array(values, native)
        } else {
            let Some(mut field) = record_field(&code[end..], constants, new.a) else {
                return Ok(None);
            };

            let mut table = if native {
                Table::with_native51_capacity(0, capacity, memory)?
            } else {
                Table::new_in(memory)
            };
            for _ in 0..capacity {
                table.set_with_native_hash(field.key, field.value, |id| {
                    Table::native_string_hash(&self.strings.bytes[slot_index(id.0)])
                })?;
                for (register, value) in field.loads.into_iter().flatten() {
                    register_values[usize::from(register)] = Some(value);
                }
                end += field.instructions;
                let Some(next) = record_field(&code[end..], constants, new.a) else {
                    break;
                };
                field = next;
            }
            table
        };
        // Keep a single layout and only snapshot a constructor after reuse.
        const MAX_CACHED_BYTES: usize = 8 * 1024 * 1024;
        if seen
            && table.heap_bytes() <= MAX_CACHED_BYTES
            && let Ok(snapshot) = table.copy_literal()
            && let Ok(template) = crate::memory::fallible::boxed(WideTemplate {
                table: snapshot,
                registers: register_values,
                end,
            })
        {
            cache.wide = Some((function, pc, Some(template)));
        }
        self.publish_wide_table(base, new.a, table, &register_values)?;
        Ok(Some(end))
    }

    fn publish_wide_table(
        &mut self,
        base: usize,
        target: u8,
        table: Table,
        registers: &[Option<Value>; 256],
    ) -> Result<(), Error> {
        // The bound constants keep installed string identities rooted.
        let id = self.try_insert_table(table, true)?;
        self.registers[base + usize::from(target)] = Value::Table(id);
        for (register, &value) in registers.iter().enumerate() {
            if let Some(value) = value {
                self.registers[base + register] = value;
            }
        }
        Ok(())
    }

    #[inline(never)]
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
            .small
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
            cache.small = Some((function, pc, template));
        }
        let Some((_, _, Some(template))) = &cache.small else {
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
        let target = &mut self.registers[slot + 1..slot + 1 + template.scratch_count];
        for (register, &source) in target.iter_mut().zip(&template.scratch) {
            *register = template.values[usize::from(source)];
        }
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
