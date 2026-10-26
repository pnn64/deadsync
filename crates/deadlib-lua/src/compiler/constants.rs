//! Append-ordered constants with a temporary index for larger function pools.
use crate::bytecode::Constant;

#[derive(Default)]
pub(super) struct Constants {
    values: Vec<Constant>,
    slots: Vec<u32>,
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
const INDEX_AT: usize = 64;

impl Key<'_> {
    fn from_constant(value: &Constant) -> Key<'_> {
        match value {
            Constant::Nil => Key::Nil,
            Constant::Boolean(v) => Key::Boolean(*v),
            Constant::Number(v) => Key::Number(*v),
            Constant::String(v) => Key::String(v),
        }
    }
    fn matches(self, value: &Constant) -> bool {
        match (self, value) {
            (Self::Nil, Constant::Nil) => true,
            (Self::Boolean(a), Constant::Boolean(b)) => a == *b,
            (Self::Number(a), Constant::Number(b)) => a == *b,
            (Self::String(a), Constant::String(b)) => a == b.as_ref(),
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
            Self::String(bytes) => {
                let mut hash = 0x85ebca77c2b2ae63 ^ bytes.len() as u64;
                let (words, tail) = bytes.as_chunks::<8>();
                for word in words {
                    hash = (hash ^ u64::from_le_bytes(*word))
                        .rotate_left(27)
                        .wrapping_mul(0x9e3779b185ebca87);
                }
                for &byte in tail {
                    hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
                }
                hash
            }
        };
        hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
        Some((hash ^ (hash >> 31)) as usize)
    }
}

impl Constants {
    pub fn finish(self) -> Box<[Constant]> {
        self.values.into_boxed_slice()
    }
    fn rebuild(&mut self) {
        self.slots
            .resize((self.values.len() * 2).next_power_of_two(), 0);
        self.slots.fill(0);
        let mask = self.slots.len() - 1;
        for (id, value) in self.values.iter().enumerate() {
            let Some(hash) = Key::from_constant(value).hash() else {
                continue;
            };
            let mut slot = hash & mask;
            while self.slots[slot] != 0 {
                slot = (slot + 1) & mask;
            }
            self.slots[slot] = id as u32 + 1;
        }
    }
    #[inline]
    fn lookup(&mut self, key: Key<'_>) -> Entry {
        // A zero slot reserves one encoding. Preserve the original complete
        // u32 ID range by using the linear path at its last representable ID.
        if self.values.len() < INDEX_AT || self.values.len() >= u32::MAX as usize {
            return self
                .values
                .iter()
                .position(|v| key.matches(v))
                .map_or(Entry::Vacant(LINEAR), |id| Entry::Present(id as u32));
        }
        if self.slots.is_empty() {
            self.rebuild();
        }
        let Some(hash) = key.hash() else {
            return Entry::Vacant(LINEAR);
        };
        let mask = self.slots.len() - 1;
        let mut slot = hash & mask;
        loop {
            let id = self.slots[slot];
            if id == 0 {
                return Entry::Vacant(slot);
            }
            if key.matches(&self.values[id as usize - 1]) {
                return Entry::Present(id - 1);
            }
            slot = (slot + 1) & mask;
        }
    }
    fn append(&mut self, value: Constant, slot: usize) -> Option<u32> {
        let id = u32::try_from(self.values.len()).ok()?;
        self.values.push(value);
        if !self.slots.is_empty() && id != u32::MAX {
            if self.values.len() * 4 >= self.slots.len() * 3 {
                self.rebuild();
            } else if slot != LINEAR {
                self.slots[slot] = id + 1;
            }
        }
        Some(id)
    }
    pub fn intern(&mut self, value: Constant) -> Option<u32> {
        match self.lookup(Key::from_constant(&value)) {
            Entry::Present(id) => Some(id),
            Entry::Vacant(slot) => self.append(value, slot),
        }
    }
    pub fn string(&mut self, bytes: &[u8]) -> Option<u32> {
        match self.lookup(Key::String(bytes)) {
            Entry::Present(id) => Some(id),
            Entry::Vacant(slot) => self.append(Constant::String(bytes.into()), slot),
        }
    }
}

#[cfg(test)]
mod tests;
