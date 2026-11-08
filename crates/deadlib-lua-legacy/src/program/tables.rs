//! Explicit byte-keyed Boolean state without a retained Lua table.
use super::*;
use rustc_hash::{FxHashMap, FxHashSet};

/// A host-owned raw table whose values remain Boolean or nil.
///
/// The host must establish object identity, exclude metatables and outside
/// observers, and snapshot every present key. Nil deletes an entry. Keys are
/// arbitrary bytes; a first write of a new key may allocate native storage.
/// Released keys can retain a bounded inactive cache; those slots remain nil.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BooleanTable {
    /// Identity used by `Binding::Object`; equal contents do not merge identities.
    pub object: u32,
    /// Every present byte key, including entries whose value is false.
    pub entries: Vec<(Box<[u8]>, bool)>,
}

/// An immutable, dense, one-based array of declared Boolean-table identities.
/// The host must exclude metatables, extra keys and writes to this array.
#[derive(Clone, Debug, Default)]
pub struct BooleanTableArray {
    /// Identity used by `Binding::Object`.
    pub object: u32,
    /// Table identities in their authored numeric order, including aliases.
    pub tables: Vec<u32>,
}

#[derive(PartialEq, Eq, Hash)]
struct Array {
    object: u32,
    tables: Box<[usize]>,
}

#[derive(PartialEq, Eq, Hash)]
pub(super) struct Layout {
    maps: Box<[BooleanTable]>,
    arrays: Box<[Array]>,
}

impl Layout {
    pub(super) fn new(bindings: &Bindings) -> Result<Option<Arc<Self>>, CompileError> {
        if bindings.boolean_tables.is_empty() && bindings.boolean_table_arrays.is_empty() {
            return Ok(None);
        }
        let mut objects = FxHashSet::default();
        for map in &bindings.boolean_tables {
            if !objects.insert(map.object)
                || bindings.fields.iter().any(|f| f.object == map.object)
                || bindings.environment == Some(map.object)
            {
                return Err(CompileError::InvalidBindings("Boolean table identity"));
            }
            let mut keys = FxHashSet::default();
            if map.entries.iter().any(|(key, _)| !keys.insert(key.as_ref())) {
                return Err(CompileError::InvalidBindings("duplicate Boolean table key"));
            }
        }
        let mut arrays = Vec::with_capacity(bindings.boolean_table_arrays.len());
        for array in &bindings.boolean_table_arrays {
            if !objects.insert(array.object)
                || bindings.fields.iter().any(|f| f.object == array.object)
                || bindings.environment == Some(array.object)
            {
                return Err(CompileError::InvalidBindings("Boolean table array identity"));
            }
            let tables = array
                .tables
                .iter()
                .map(|object| {
                    bindings
                        .boolean_tables
                        .iter()
                        .position(|map| map.object == *object)
                        .ok_or(CompileError::InvalidBindings("Boolean table array reference"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            arrays.push(Array {
                object: array.object,
                tables: tables.into_boxed_slice(),
            });
        }
        Ok(Some(Arc::new(Self {
            maps: bindings.boolean_tables.clone().into_boxed_slice(),
            arrays: arrays.into_boxed_slice(),
        })))
    }

    pub(super) fn index(&self, object: u32) -> Option<usize> {
        self.maps.iter().position(|m| m.object == object)
    }

    pub(super) fn object(&self, index: usize) -> u32 {
        self.maps[index].object
    }

    pub(super) fn objects(&self) -> impl Iterator<Item = u32> + '_ {
        self.maps.iter().map(|map| map.object)
    }

    pub(super) fn array_index(&self, object: u32) -> Option<usize> {
        self.arrays.iter().position(|array| array.object == object)
    }

    pub(super) fn array_object(&self, index: usize) -> u32 {
        self.arrays[index].object
    }
    pub(super) fn array(&self, index: usize) -> &[usize] {
        &self.arrays[index].tables
    }

    pub(super) fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + size_of_val(self.arrays.as_ref())
            + self
                .arrays
                .iter()
                .map(|array| size_of_val(array.tables.as_ref()))
                .sum::<usize>()
            + size_of_val(self.maps.as_ref())
            + self
                .maps
                .iter()
                .map(|map| {
                    map.entries.capacity() * size_of::<(Box<[u8]>, bool)>()
                        + map.entries.iter().map(|(key, _)| key.len()).sum::<usize>()
                })
                .sum::<usize>()
    }

    pub(super) fn state(&self) -> Box<State> {
        Box::new(State {
            maps: self
                .maps
                .iter()
                .map(|map| Slots {
                    values: map
                        .entries
                        .iter()
                        .map(|(key, value)| (key.clone(), Some(*value)))
                        .collect(),
                    releases: 0,
                })
                .collect(),
        })
    }
}

pub(super) struct State {
    maps: Box<[Slots]>,
}

struct Slots {
    values: FxHashMap<Box<[u8]>, Option<bool>>,
    releases: u8,
}

impl State {
    pub(super) fn read(&self, table: usize, key: &[u8]) -> Value {
        self.maps[table]
            .values
            .get(key)
            .copied()
            .flatten()
            .map_or(Value::Nil, Value::Boolean)
    }

    pub(super) fn write(&mut self, table: usize, key: &[u8], value: Value) {
        let map = &mut self.maps[table];
        match value {
            Value::Nil => {
                if let Some(previous) = map.values.get_mut(key) {
                    *previous = None;
                }
                // Small input sets retain their slots without accounting on
                // presses. Larger sets clear nil slots every 64 releases;
                // at most 127 inactive slots survive between cleanups.
                if map.values.len() > 64 {
                    map.releases += 1;
                    if map.releases == 64 {
                        map.values.retain(|_, value| value.is_some());
                        map.releases = 0;
                    }
                }
            }
            Value::Boolean(value) => {
                if let Some(previous) = map.values.get_mut(key) {
                    *previous = Some(value);
                } else {
                    map.values.insert(key.into(), Some(value));
                }
            }
            _ => unreachable!("verified Boolean table value"),
        }
    }

    pub(super) fn get(&self, table: usize, key: &[u8]) -> Option<bool> {
        self.maps[table].values.get(key).copied().flatten()
    }

    pub(super) fn entries(&self, table: usize) -> impl Iterator<Item = (&[u8], bool)> {
        self.maps[table]
            .values
            .iter()
            .filter_map(|(key, value)| value.map(|value| (key.as_ref(), value)))
    }

    // Hash control/allocator bookkeeping is excluded from this payload estimate.
    pub(super) fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + size_of_val(self.maps.as_ref())
            + self
                .maps
                .iter()
                .map(|map| {
                    map.values.capacity() * size_of::<(Box<[u8]>, Option<bool>)>()
                        + map.values.keys().map(|key| key.len()).sum::<usize>()
                })
                .sum::<usize>()
    }
}
