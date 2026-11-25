//! Append-ordered constants with a temporary index for larger function pools.
use crate::{
    Error,
    bytecode::Constant,
    lexer::Lexeme,
    memory::fallible::{SliceExt, VecExt},
};

#[derive(Default)]
pub(super) struct Constants {
    values: Vec<Constant>,
    strings: Vec<u8>,
    slots: Vec<u32>,
    /// Maps parser constant slots to one-based IDs assigned during emission.
    source_ids: Vec<u32>,
    /// A string without a parser slot requires lookup by content for this pool.
    lookup_strings: bool,
}

pub(super) struct PackedConstants {
    pub values: Box<[Constant]>,
    pub strings: Box<[u8]>,
}
#[derive(Clone, Copy)]
enum Key<'a> {
    Nil,
    Boolean(bool),
    Number(f64),
    String(&'a [u8]),
}
enum Entry {
    Present(u32),
    Vacant(usize),
}
const LINEAR: usize = usize::MAX;
const INDEX_AT: usize = 16;

impl<'a> Key<'a> {
    fn from_constant(value: &'a Constant, strings: &'a [u8]) -> Self {
        match value {
            Constant::Nil => Key::Nil,
            Constant::Boolean(v) => Key::Boolean(*v),
            Constant::Number(v) => Key::Number(*v),
            Constant::String(_) | Constant::PooledString { .. } => {
                Key::String(value.string(strings).expect("constant string"))
            }
        }
    }
    fn matches(self, value: &Constant, strings: &[u8]) -> bool {
        match (self, value) {
            (Self::Nil, Constant::Nil) => true,
            (Self::Boolean(a), Constant::Boolean(b)) => a == *b,
            (Self::Number(a), Constant::Number(b)) => a == *b,
            (Self::String(a), b) => b.string(strings).is_some_and(|b| a == b),
            _ => false,
        }
    }
    fn hash(self) -> Option<usize> {
        let mut hash = match self {
            Self::Nil => 0x9e3779b97f4a7c15,
            Self::Boolean(v) => 0xc2b2ae3d27d4eb4f ^ u64::from(v),
            Self::Number(v) if v.is_nan() => return None,
            // Numeric equality merges both zero signs; the stored first value
            // and the compiler's existing zero-sign selection are unchanged.
            Self::Number(v) => 0x165667b19e3779f9 ^ if v == 0.0 { 0 } else { v.to_bits() },
            Self::String(bytes) => return Some(crate::hash::hash_bytes(bytes) as usize),
        };
        hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
        Some((hash ^ (hash >> 31)) as usize)
    }
}

impl Constants {
    /// Publish the exact-size pools, keeping their work buffers for reuse.
    pub fn finish(&mut self) -> Result<PackedConstants, Error> {
        let strings = self.strings.try_boxed()?;
        let mut exact = crate::memory::fallible::with_capacity(self.values.len())?;
        exact.append(&mut self.values);
        self.clear();
        Ok(PackedConstants {
            values: exact.try_into_boxed_slice()?,
            strings,
        })
    }
    pub fn clear(&mut self) {
        self.values.clear();
        self.strings.clear();
        self.slots.clear();
        self.source_ids.clear();
        self.lookup_strings = false;
    }
    fn rebuild(&mut self) -> Result<(), Error> {
        self.slots
            .try_resize((self.values.len() * 2).next_power_of_two(), 0)?;
        self.slots.fill(0);
        let mask = self.slots.len() - 1;
        for (id, value) in self.values.iter().enumerate() {
            if !self.lookup_strings
                && matches!(value, Constant::String(_) | Constant::PooledString { .. })
            {
                continue;
            }
            let Some(hash) = Key::from_constant(value, &self.strings).hash() else {
                continue;
            };
            let mut slot = hash & mask;
            while self.slots[slot] != 0 {
                slot = (slot + 1) & mask;
            }
            self.slots[slot] = id as u32 + 1;
        }
        Ok(())
    }
    #[inline]
    fn lookup(&mut self, key: Key<'_>) -> Result<Entry, Error> {
        // A zero slot reserves one encoding. Preserve the original complete
        // u32 ID range by using the linear path at its last representable ID.
        if self.values.len() < INDEX_AT || self.values.len() >= u32::MAX as usize {
            return Ok(self
                .values
                .iter()
                .position(|v| key.matches(v, &self.strings))
                .map_or(Entry::Vacant(LINEAR), |id| Entry::Present(id as u32)));
        }
        if self.slots.is_empty() {
            self.rebuild()?;
        }
        let Some(hash) = key.hash() else {
            return Ok(Entry::Vacant(LINEAR));
        };
        let mask = self.slots.len() - 1;
        let mut slot = hash & mask;
        loop {
            let id = self.slots[slot];
            if id == 0 {
                return Ok(Entry::Vacant(slot));
            }
            if key.matches(&self.values[id as usize - 1], &self.strings) {
                return Ok(Entry::Present(id - 1));
            }
            slot = (slot + 1) & mask;
        }
    }
    fn append(&mut self, value: Constant, slot: usize) -> Result<Option<u32>, Error> {
        let Ok(id) = u32::try_from(self.values.len()) else {
            return Ok(None);
        };
        self.values.try_push(value)?;
        if !self.slots.is_empty() && id != u32::MAX && slot != LINEAR {
            if self.values.len() * 4 >= self.slots.len() * 3 {
                self.rebuild()?;
            } else {
                self.slots[slot] = id + 1;
            }
        }
        Ok(Some(id))
    }
    pub fn intern(&mut self, value: Constant) -> Result<Option<u32>, Error> {
        let key = Key::from_constant(&value, &[]);
        if matches!(key, Key::String(_)) {
            self.enable_string_lookup();
        }
        match self.lookup(key)? {
            Entry::Present(id) => Ok(Some(id)),
            Entry::Vacant(slot) => self.append(value, slot),
        }
    }
    pub fn string(&mut self, bytes: &[u8]) -> Result<Option<u32>, Error> {
        self.enable_string_lookup();
        match self.lookup(Key::String(bytes))? {
            Entry::Present(id) => Ok(Some(id)),
            Entry::Vacant(slot) => self.append_string(bytes, slot),
        }
    }

    fn enable_string_lookup(&mut self) {
        if !self.lookup_strings {
            self.lookup_strings = true;
            // A later lookup must include previously emitted strings.
            self.slots.clear();
        }
    }

    fn append_string(&mut self, bytes: &[u8], slot: usize) -> Result<Option<u32>, Error> {
        let start = self.strings.len();
        let end = start
            .checked_add(bytes.len())
            .and_then(|end| u32::try_from(end).ok());
        let value = if let Some(end) = end {
            self.strings.try_extend_from_slice(bytes)?;
            Constant::PooledString {
                start: start as u32,
                end,
            }
        } else {
            Constant::String(bytes.try_boxed()?)
        };
        self.append(value, slot)
    }

    fn source_id(&self, slot: usize) -> Option<u32> {
        self.source_ids.get(slot).and_then(|id| id.checked_sub(1))
    }

    fn remember_source(&mut self, slot: usize, id: Option<u32>) -> Result<(), Error> {
        if let Some(encoded) = id.and_then(|id| id.checked_add(1)) {
            if slot >= self.source_ids.len() {
                self.source_ids.try_resize(slot + 1, 0)?;
            }
            self.source_ids[slot] = encoded;
        }
        Ok(())
    }

    pub fn source_number(&mut self, slot: usize, number: f64) -> Result<Option<u32>, Error> {
        if let Some(id) = self.source_id(slot) {
            debug_assert!(
                matches!(&self.values[id as usize], Constant::Number(value) if *value == number)
            );
            return Ok(Some(id));
        }
        let id = self.intern(Constant::Number(number))?;
        self.remember_source(slot, id)?;
        Ok(id)
    }

    pub fn source_string(
        &mut self,
        spelling: &Lexeme,
        source: &[u8],
    ) -> Result<Option<u32>, Error> {
        let Some(slot) = spelling.string_slot() else {
            return self.string(spelling.bytes(source));
        };
        if let Some(id) = self.source_id(slot) {
            debug_assert_eq!(
                self.values[id as usize].string(&self.strings),
                Some(spelling.bytes(source))
            );
            return Ok(Some(id));
        }
        let bytes = spelling.bytes(source);
        let id = if self.lookup_strings {
            self.string(bytes)?
        } else {
            // The parser already established that different slots denote
            // different strings. Keep IDs in first-emission order.
            self.append_string(bytes, LINEAR)?
        };
        self.remember_source(slot, id)?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests;
