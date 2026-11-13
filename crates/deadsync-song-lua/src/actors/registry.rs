//! Dense actor storage indexed by the reusable Lua table arena slot.
//!
//! Most tables have only one live generation per slot. During retirement,
//! however, the VM can reuse a collected table's slot before its actor state
//! is swept. Keep such older generations in a small fallback index until they
//! are removed, so a slot reuse never aliases another actor's metadata.

use deadlib_lua::TableId;
use rustc_hash::FxHashMap;
use std::ops::{Index, IndexMut};

const VACANT: u32 = u32::MAX;

pub(super) struct Registry<T> {
    slots: Vec<u32>,
    entries: Vec<(TableId, T)>,
    older: FxHashMap<TableId, u32>,
}

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            entries: Vec::new(),
            older: FxHashMap::default(),
        }
    }
}

impl<T> Registry<T> {
    #[inline]
    fn position(&self, id: &TableId) -> Option<usize> {
        let slot = id.slot() as usize;
        let direct = self.slots.get(slot).copied().unwrap_or(VACANT);
        if direct != VACANT && self.entries[direct as usize].0 == *id {
            Some(direct as usize)
        } else {
            self.older.get(id).copied().map(|index| index as usize)
        }
    }

    #[inline]
    pub(super) fn get(&self, id: &TableId) -> Option<&T> {
        self.entries.get(self.position(id)?).map(|(_, value)| value)
    }

    #[inline]
    pub(super) fn get_mut(&mut self, id: &TableId) -> Option<&mut T> {
        let index = self.position(id)?;
        Some(&mut self.entries[index].1)
    }

    #[inline]
    pub(super) fn contains_key(&self, id: &TableId) -> bool {
        self.position(id).is_some()
    }

    pub(super) fn insert(&mut self, id: TableId, value: T) -> Option<T> {
        if let Some(index) = self.position(&id) {
            return Some(std::mem::replace(&mut self.entries[index].1, value));
        }
        let slot = id.slot() as usize;
        if slot >= self.slots.len() {
            self.slots.resize(slot + 1, VACANT);
        }
        let direct = self.slots[slot];
        if direct != VACANT {
            // A different generation still has metadata pending retirement.
            self.older.insert(self.entries[direct as usize].0, direct);
        }
        let index = u32::try_from(self.entries.len()).expect("actor index fits u32");
        self.entries.push((id, value));
        self.slots[slot] = index;
        None
    }

    pub(super) fn remove(&mut self, id: &TableId) -> Option<T> {
        let index = self.position(id)?;
        let slot = id.slot() as usize;
        if self.slots[slot] == index as u32 {
            self.slots[slot] = VACANT;
        } else {
            self.older.remove(id);
        }
        let last = self.entries.len() - 1;
        if index != last {
            let moved = self.entries[last].0;
            let moved_slot = moved.slot() as usize;
            if self.slots[moved_slot] == last as u32 {
                self.slots[moved_slot] = index as u32;
            } else {
                *self.older.get_mut(&moved).expect("older actor index") = index as u32;
            }
        }
        Some(self.entries.swap_remove(index).1)
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&TableId, &T)> {
        self.entries.iter().map(|(id, value)| (id, value))
    }

    pub(super) fn values(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().map(|(_, value)| value)
    }

    pub(super) fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.entries.iter_mut().map(|(_, value)| value)
    }
}

impl<T> Index<&TableId> for Registry<T> {
    type Output = T;

    fn index(&self, id: &TableId) -> &Self::Output {
        self.get(id).expect("actor exists in registry")
    }
}

impl<T> IndexMut<&TableId> for Registry<T> {
    fn index_mut(&mut self, id: &TableId) -> &mut Self::Output {
        self.get_mut(id).expect("actor exists in registry")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_lua::Vm;

    #[test]
    fn retired_generations_do_not_alias_reused_slots() {
        let mut vm = Vm::new_bare();
        let old = vm.new_table();
        vm.collect_garbage().unwrap();
        let current = vm.new_table();
        assert_ne!(old, current);
        assert_eq!(old.slot(), current.slot());

        let mut actors = Registry::default();
        assert_eq!(actors.insert(old, "old"), None);
        assert_eq!(actors.insert(current, "current"), None);
        assert_eq!(actors.len(), 2);
        assert_eq!(actors.get(&old), Some(&"old"));
        assert_eq!(actors.get(&current), Some(&"current"));
        assert_eq!(actors.remove(&current), Some("current"));
        assert_eq!(actors.get(&old), Some(&"old"));
        assert_eq!(actors.insert(current, "new"), None);
        assert_eq!(actors.remove(&old), Some("old"));
        assert_eq!(actors.get(&current), Some(&"new"));
    }

    #[test]
    fn swapping_out_a_middle_actor_repairs_slot_indexes() {
        let mut vm = Vm::new_bare();
        let ids = [vm.new_table(), vm.new_table(), vm.new_table()];
        let mut actors = Registry::default();
        for (i, id) in ids.into_iter().enumerate() {
            actors.insert(id, i);
        }
        assert_eq!(actors.remove(&ids[1]), Some(1));
        assert_eq!(actors.get(&ids[0]), Some(&0));
        assert_eq!(actors.get(&ids[2]), Some(&2));
        for value in actors.values_mut() {
            *value += 10;
        }
        assert_eq!(actors.get(&ids[0]), Some(&10));
        assert_eq!(actors.get(&ids[2]), Some(&12));
    }

    #[test]
    fn retiring_an_older_generation_repairs_the_moved_current_index() {
        let mut vm = Vm::new_bare();
        let first = vm.new_table();
        vm.collect_garbage().unwrap();
        let second = vm.new_table();
        vm.collect_garbage().unwrap();
        let third = vm.new_table();
        assert_eq!(first.slot(), second.slot());
        assert_eq!(second.slot(), third.slot());

        let mut actors = Registry::default();
        actors.insert(first, 1);
        actors.insert(second, 2);
        actors.insert(third, 3);
        assert_eq!(actors.remove(&second), Some(2));
        assert_eq!(actors.get(&first), Some(&1));
        assert_eq!(actors.get(&third), Some(&3));
        assert_eq!(actors.remove(&first), Some(1));
        assert_eq!(actors.get(&third), Some(&3));
        assert_eq!(actors.remove(&third), Some(3));
        assert_eq!(actors.len(), 0);
    }

    #[test]
    fn randomized_replacements_and_removals_match_a_hash_map() {
        let mut vm = Vm::new_bare();
        let mut ids = Vec::new();
        // Repeated GC makes many different generations share one table slot.
        for _ in 0..32 {
            ids.push(vm.new_table());
            vm.collect_garbage().unwrap();
        }
        // Leave a second group live, with distinct slots.
        ids.extend((0..32).map(|_| vm.new_table()));

        let mut actors = Registry::default();
        let mut expected = FxHashMap::default();
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        for step in 0..5_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let id = ids[(seed as usize) % ids.len()];
            if seed & 3 == 0 {
                assert_eq!(actors.remove(&id), expected.remove(&id));
            } else {
                assert_eq!(actors.insert(id, step), expected.insert(id, step));
            }
            assert_eq!(actors.len(), expected.len());
            if step % 17 == 0 {
                for id in &ids {
                    assert_eq!(actors.get(id), expected.get(id), "step={step}, id={id:?}");
                }
            }
        }
    }
}
