//! Frozen from 56a0f78eb48cb5af19d2b33667585fcdbcb79ffb; only function visibility and formatting differ.

use super::*;

// Separate compact key sets preserve Lua's non-transitive mixed-number
// comparisons without storing a large tagged key for every numeric entry.
struct DeduplicateIndex {
    booleans: u8,
    integers: std::collections::HashSet<i64>,
    numbers: std::collections::HashSet<u64>,
    integer_numbers: std::collections::HashSet<u64>,
    strings: std::collections::HashSet<mlua::LuaString>,
    tables: std::collections::HashSet<usize>,
}

impl DeduplicateIndex {
    fn from_prefix(prefix: &[Value]) -> Self {
        let (mut integers, mut numbers, mut strings, mut tables) = (0, 0, 0, 0);
        for value in prefix {
            match value {
                Value::Integer(_) => integers += 1,
                Value::Number(value) if !value.is_nan() => numbers += 1,
                Value::String(value) if value.to_str().is_ok() => strings += 1,
                Value::Table(_) => tables += 1,
                _ => {}
            }
        }
        let mut index = Self {
            booleans: 0,
            integers: std::collections::HashSet::with_capacity(integers),
            numbers: std::collections::HashSet::with_capacity(numbers),
            integer_numbers: std::collections::HashSet::with_capacity(integers),
            strings: std::collections::HashSet::with_capacity(strings),
            tables: std::collections::HashSet::with_capacity(tables),
        };
        for value in prefix {
            index.insert(value);
        }
        index
    }

    fn insert(&mut self, value: &Value) -> bool {
        fn number_bits(value: f64) -> u64 {
            if value == 0.0 { 0 } else { value.to_bits() }
        }
        match value {
            Value::Boolean(value) => {
                let mask = 1 << u8::from(*value);
                let new = self.booleans & mask == 0;
                self.booleans |= mask;
                new
            }
            Value::Integer(value) => {
                let bits = number_bits(*value as f64);
                if self.numbers.contains(&bits) || !self.integers.insert(*value) {
                    return false;
                }
                self.integer_numbers.insert(bits);
                true
            }
            Value::Number(value) if !value.is_nan() => {
                let bits = number_bits(*value);
                !self.integer_numbers.contains(&bits) && self.numbers.insert(bits)
            }
            Value::String(value) if value.to_str().is_ok() => self.strings.insert(value.clone()),
            Value::Table(value) => self.tables.insert(value.to_pointer() as usize),
            // These values never compare equal under lua_values_equal.
            _ => true,
        }
    }
}

// Keep the larger index frame out of the short-list path. This is called once
// after accepting the 33rd value; the remaining loop needs no mode check.
#[inline(never)]
fn finish_indexed_deduplication(
    out: &Table,
    mut out_index: usize,
    prefix: Vec<Value>,
    first: Value,
    remaining: impl Iterator<Item = mlua::Result<Value>>,
) -> mlua::Result<()> {
    let mut index = DeduplicateIndex::from_prefix(&prefix);
    drop(prefix);
    index.insert(&first);
    out.raw_set(out_index, first)?;
    out_index += 1;
    for value in remaining {
        let value = value?;
        if index.insert(&value) {
            out.raw_set(out_index, value)?;
            out_index += 1;
        }
    }
    Ok(())
}

pub(super) fn deduplicate_lua_table(lua: &Lua, table: &Table) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    let mut seen = Vec::new();
    let mut out_index = 1;
    let mut values = table.sequence_values::<Value>();
    for value in values.by_ref() {
        let value = value?;
        if seen.iter().any(|seen| lua_values_equal(seen, &value)) {
            continue;
        }
        if seen.len() == 32 && table.raw_len() >= 128 {
            finish_indexed_deduplication(&out, out_index, seen, value, values)?;
            return Ok(out);
        }
        seen.push(value.clone());
        out.raw_set(out_index, value)?;
        out_index += 1;
    }
    Ok(out)
}
