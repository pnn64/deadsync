use std::{cell::RefCell, collections::HashMap};

#[cfg(feature = "bench-support")]
use std::collections::hash_map::Entry as HashEntry;

use crate::anim::{Step, TweenSeq, TweenState};

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

type TweenIndex = HashMap<u64, usize, rustc_hash::FxBuildHasher>;

struct Entry {
    id: u64,
    seq: TweenSeq,
    last_seen_frame: u64,
}

#[derive(Default)]
struct Registry {
    indices: TweenIndex,
    entries: Vec<Entry>,
    frame: u64,
    materialize_cursor: usize,
}

impl Registry {
    /// Return a cached state and move an out-of-order entry into this frame's
    /// traversal order. Stable actor trees hit the first branch without hashing.
    #[inline(always)]
    fn cached_state(&mut self, id: u64) -> Option<TweenState> {
        let cursor = self.materialize_cursor;
        if let Some(entry) = self.entries.get_mut(cursor)
            && entry.id == id
        {
            debug_assert_ne!(entry.last_seen_frame, self.frame);
            entry.last_seen_frame = self.frame;
            let state = *entry.seq.state();
            self.materialize_cursor = cursor + 1;
            return Some(state);
        }

        let index = *self.indices.get(&id)?;
        if self.entries[index].last_seen_frame == self.frame {
            return Some(*self.entries[index].seq.state());
        }

        debug_assert!(cursor < self.entries.len());
        if index != cursor {
            self.entries.swap(index, cursor);
            let displaced_id = self.entries[index].id;
            *self
                .indices
                .get_mut(&id)
                .expect("cached tween must have an index") = cursor;
            *self
                .indices
                .get_mut(&displaced_id)
                .expect("displaced tween must have an index") = index;
        }

        let entry = &mut self.entries[cursor];
        entry.last_seen_frame = self.frame;
        let state = *entry.seq.state();
        self.materialize_cursor = cursor + 1;
        Some(state)
    }

    #[cold]
    fn insert_or_get(&mut self, id: u64, tween: TweenSeq) -> TweenState {
        // The source-program builder runs without a registry borrow and may
        // recursively materialize this same id.
        if let Some(state) = self.cached_state(id) {
            return state;
        }

        let cursor = self.materialize_cursor;
        let new_index = self.entries.len();
        let state = *tween.state();
        self.entries.push(Entry {
            id,
            seq: tween,
            last_seen_frame: self.frame,
        });
        self.indices.insert(id, new_index);

        // New actors can appear before retained-but-unseen actors. Keep the
        // observed order contiguous so the next stable frame is hash-free.
        if cursor != new_index {
            self.entries.swap(cursor, new_index);
            let displaced_id = self.entries[new_index].id;
            *self
                .indices
                .get_mut(&id)
                .expect("inserted tween must have an index") = cursor;
            *self
                .indices
                .get_mut(&displaced_id)
                .expect("displaced tween must have an index") = new_index;
        }
        self.materialize_cursor = cursor + 1;
        state
    }
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

#[inline(always)]
const fn seen_recently(last_seen_frame: u64, frame: u64) -> bool {
    frame.wrapping_sub(last_seen_frame) <= 1
}

/// Advance all tweens once per frame and GC unseen actors from the previous frame.
pub fn tick(dt: f32) {
    REG.with(|r| {
        let mut r = r.borrow_mut();
        let frame = r.frame.wrapping_add(1);
        r.frame = frame;
        r.materialize_cursor = 0;

        // Tween programs are already stored densely in their observed render
        // order, so the common path advances them without a hash lookup.
        let mut index = 0;
        while index < r.entries.len() {
            if seen_recently(r.entries[index].last_seen_frame, frame) {
                r.entries[index].seq.update(dt);
                index += 1;
                continue;
            }

            let removed = r.entries.swap_remove(index);
            r.indices.remove(&removed.id);
            if let Some(moved_id) = r.entries.get(index).map(|entry| entry.id) {
                *r.indices
                    .get_mut(&moved_id)
                    .expect("moved tween must have an index") = index;
            }
        }
    });
}

/// Get/create a tween at this callsite and return its current state.
/// `steps` are only enqueued on first sight of this site id.
pub fn materialize(id: u64, initial: TweenState, steps: &[Step]) -> TweenState {
    materialize_lazy(id, initial, || steps.iter().cloned())
}

/// Get/create a tween, constructing its source program only for a vacant entry.
pub fn materialize_lazy<I>(
    id: u64,
    initial: TweenState,
    build_steps: impl FnOnce() -> I,
) -> TweenState
where
    I: IntoIterator<Item = Step>,
{
    let cached = REG.with(|r| r.borrow_mut().cached_state(id));
    if let Some(state) = cached {
        return state;
    }

    // Build outside the registry borrow so source expressions may safely
    // materialize other actors, matching the eager program's reentrancy.
    let mut tween = TweenSeq::new(initial);
    for step in build_steps() {
        tween.push_step(step);
    }

    REG.with(|r| r.borrow_mut().insert_or_get(id, tween))
}

/// Stable-ish hash for a macro callsite before any per-instance salt is mixed in.
pub const fn site_base(file: &'static str, line: u32, col: u32) -> u64 {
    let bytes = file.as_bytes();
    let mut h = FNV_OFFSET;
    let mut i = 0;
    while i < bytes.len() {
        h ^= bytes[i] as u64;
        h = h.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    h ^= ((line as u64) << 32) ^ (col as u64);
    h.wrapping_mul(FNV_PRIME)
}

/// Stable-ish id for a macro callsite, with an optional per-instance discriminator.
#[inline(always)]
pub const fn site_id(site_base: u64, extra: u64) -> u64 {
    site_base ^ extra
}

// Optional manual clear (e.g., on screen swaps if desired).
pub fn clear_all() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

#[cfg(feature = "bench-support")]
mod legacy_benchmark {
    use super::{HashEntry, HashMap, RefCell, TweenState, seen_recently};
    use crate::anim::{Step, TweenSeq};

    type LegacyTweenMap = HashMap<u64, LegacyEntry, rustc_hash::FxBuildHasher>;

    struct LegacyEntry {
        seq: TweenSeq,
        last_seen_frame: u64,
    }

    #[derive(Default)]
    struct LegacyRegistry {
        map: LegacyTweenMap,
        frame: u64,
        active_ids: Vec<u64>,
    }

    thread_local! {
        static LEGACY_REG: RefCell<LegacyRegistry> = RefCell::new(LegacyRegistry::default());
    }

    pub(super) fn tick(dt: f32) {
        LEGACY_REG.with(|r| {
            let mut r = r.borrow_mut();
            let frame = r.frame.wrapping_add(1);
            r.frame = frame;
            let LegacyRegistry {
                map, active_ids, ..
            } = &mut *r;
            active_ids.retain(|&id| match map.entry(id) {
                HashEntry::Occupied(mut occupied)
                    if seen_recently(occupied.get().last_seen_frame, frame) =>
                {
                    occupied.get_mut().seq.update(dt);
                    true
                }
                HashEntry::Occupied(occupied) => {
                    occupied.remove();
                    false
                }
                HashEntry::Vacant(_) => false,
            });
        });
    }

    pub(super) fn materialize_lazy<I>(
        id: u64,
        initial: TweenState,
        build_steps: impl FnOnce() -> I,
    ) -> TweenState
    where
        I: IntoIterator<Item = Step>,
    {
        let cached = LEGACY_REG.with(|r| {
            let mut r = r.borrow_mut();
            let frame = r.frame;
            r.map.get_mut(&id).map(|entry| {
                if entry.last_seen_frame != frame {
                    entry.last_seen_frame = frame;
                }
                *entry.seq.state()
            })
        });
        if let Some(state) = cached {
            return state;
        }

        let mut tween = TweenSeq::new(initial);
        for step in build_steps() {
            tween.push_step(step);
        }

        LEGACY_REG.with(|r| {
            let mut r = r.borrow_mut();
            let frame = r.frame;
            let mut activate = false;
            let state = match r.map.entry(id) {
                HashEntry::Occupied(mut occupied) => {
                    let entry = occupied.get_mut();
                    entry.last_seen_frame = frame;
                    *entry.seq.state()
                }
                HashEntry::Vacant(vacant) => {
                    let state = *tween.state();
                    vacant.insert(LegacyEntry {
                        seq: tween,
                        last_seen_frame: frame,
                    });
                    activate = true;
                    state
                }
            };
            if activate {
                r.active_ids.push(id);
            }
            state
        })
    }

    pub(super) fn clear_all() {
        LEGACY_REG.with(|r| *r.borrow_mut() = LegacyRegistry::default());
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn __benchmark_legacy_tick(dt: f32) {
    legacy_benchmark::tick(dt);
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn __benchmark_legacy_materialize_lazy<I>(
    id: u64,
    initial: TweenState,
    build_steps: impl FnOnce() -> I,
) -> TweenState
where
    I: IntoIterator<Item = Step>,
{
    legacy_benchmark::materialize_lazy(id, initial, build_steps)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn __benchmark_legacy_clear_all() {
    legacy_benchmark::clear_all();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim;

    fn reset_registry(frame: u64) {
        REG.with(|r| {
            let mut r = r.borrow_mut();
            *r = Registry::default();
            r.frame = frame;
        });
    }

    fn registry_len() -> usize {
        REG.with(|r| r.borrow().entries.len())
    }

    fn active_id_len() -> usize {
        registry_len()
    }

    fn assert_registry_indices() {
        REG.with(|r| {
            let r = r.borrow();
            assert_eq!(r.indices.len(), r.entries.len());
            for (index, entry) in r.entries.iter().enumerate() {
                assert_eq!(r.indices.get(&entry.id), Some(&index));
            }
        });
    }

    fn legacy_site_id(file: &'static str, line: u32, col: u32, extra: u64) -> u64 {
        let mut h = FNV_OFFSET;
        for &b in file.as_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(FNV_PRIME);
        }
        h ^= (u64::from(line) << 32) ^ u64::from(col);
        h = h.wrapping_mul(FNV_PRIME);
        h ^ extra
    }

    #[test]
    fn tick_updates_live_tweens() {
        reset_registry(0);
        let steps = [anim::linear(1.0).x(10.0).build()];

        let state = materialize(1, TweenState::default(), &steps);
        assert_eq!(state.x, 0.0);

        tick(0.25);

        let state = materialize(1, TweenState::default(), &steps);
        assert!(
            (state.x - 2.5).abs() < 0.0001,
            "expected x ~= 2.5, got {}",
            state.x
        );
    }

    #[test]
    fn sleep_delays_the_following_segment() {
        reset_registry(0);
        let steps = [anim::sleep(0.5), anim::linear(0.5).x(10.0).build()];

        materialize(1, TweenState::default(), &steps);
        tick(0.25);
        assert_eq!(materialize(1, TweenState::default(), &steps).x, 0.0);

        tick(0.25);
        assert_eq!(materialize(1, TweenState::default(), &steps).x, 0.0);

        tick(0.25);
        let state = materialize(1, TweenState::default(), &steps);
        assert!((state.x - 5.0).abs() < 0.0001);
    }

    #[test]
    fn duplicate_materialize_in_frame_updates_once() {
        reset_registry(0);
        let steps = [anim::linear(1.0).x(10.0).build()];

        let _ = materialize(1, TweenState::default(), &steps);
        let _ = materialize(1, TweenState::default(), &steps);
        assert_eq!(active_id_len(), 1);

        tick(0.25);

        let state = materialize(1, TweenState::default(), &steps);
        assert!(
            (state.x - 2.5).abs() < 0.0001,
            "expected x ~= 2.5 after one update, got {}",
            state.x
        );
    }

    #[test]
    fn reordered_materialization_preserves_each_tween_state() {
        reset_registry(0);
        let first = [anim::linear(1.0).x(10.0).build()];
        let second = [anim::linear(1.0).x(20.0).build()];
        materialize(1, TweenState::default(), &first);
        materialize(2, TweenState::default(), &second);

        tick(0.25);
        let second_state = materialize(2, TweenState::default(), &second);
        let first_state = materialize(1, TweenState::default(), &first);
        assert!((second_state.x - 5.0).abs() < 0.0001);
        assert!((first_state.x - 2.5).abs() < 0.0001);
        assert_registry_indices();

        tick(0.25);
        let second_state = materialize(2, TweenState::default(), &second);
        let first_state = materialize(1, TweenState::default(), &first);
        assert!((second_state.x - 10.0).abs() < 0.0001);
        assert!((first_state.x - 5.0).abs() < 0.0001);
        assert_registry_indices();
    }

    #[test]
    fn stale_removal_repairs_swapped_entry_index() {
        reset_registry(0);
        let steps = [anim::linear(10.0).x(30.0).build()];
        materialize(1, TweenState::default(), &steps);
        materialize(2, TweenState::default(), &steps);
        materialize(3, TweenState::default(), &steps);

        tick(1.0);
        let third = materialize(3, TweenState::default(), &steps);
        assert!((third.x - 3.0).abs() < 0.0001);
        assert_registry_indices();

        tick(1.0);
        assert_eq!(registry_len(), 1);
        assert_registry_indices();
        let third = materialize(3, TweenState::default(), &steps);
        assert!((third.x - 6.0).abs() < 0.0001);
    }

    #[test]
    fn new_actor_can_precede_retained_unseen_actors() {
        reset_registry(0);
        let steps = [anim::linear(10.0).x(30.0).build()];
        materialize(1, TweenState::default(), &steps);
        materialize(2, TweenState::default(), &steps);

        tick(1.0);
        assert_eq!(materialize(3, TweenState::default(), &steps).x, 0.0);
        assert!((materialize(1, TweenState::default(), &steps).x - 3.0).abs() < 0.0001);
        assert!((materialize(2, TweenState::default(), &steps).x - 3.0).abs() < 0.0001);
        assert_registry_indices();

        tick(1.0);
        assert!((materialize(3, TweenState::default(), &steps).x - 3.0).abs() < 0.0001);
        assert!((materialize(1, TweenState::default(), &steps).x - 6.0).abs() < 0.0001);
        assert!((materialize(2, TweenState::default(), &steps).x - 6.0).abs() < 0.0001);
        assert_registry_indices();
    }

    #[test]
    fn lazy_materialize_builds_program_only_for_vacant_entry() {
        reset_registry(0);
        let state = materialize_lazy(1, TweenState::default(), || {
            [anim::linear(1.0).x(10.0).build()]
        });
        assert_eq!(state.x, 0.0);

        let state = materialize_lazy(1, TweenState::default(), || -> [Step; 1] {
            panic!("cache hits must not rebuild tween steps")
        });
        assert_eq!(state.x, 0.0);
        assert_eq!(registry_len(), 1);
        assert_eq!(active_id_len(), 1);
    }

    #[test]
    fn lazy_program_build_can_materialize_another_actor() {
        reset_registry(0);
        materialize_lazy(1, TweenState::default(), || {
            materialize_lazy(2, TweenState::default(), || [anim::sleep(1.0)]);
            [anim::sleep(1.0)]
        });

        assert_eq!(registry_len(), 2);
        assert_eq!(active_id_len(), 2);
    }

    #[test]
    fn tick_drops_stale_entries_across_frame_wraparound() {
        reset_registry(u64::MAX - 1);
        let steps = [anim::sleep(1.0)];
        materialize(7, TweenState::default(), &steps);
        assert_eq!(registry_len(), 1);

        tick(0.0);
        assert_eq!(registry_len(), 1);

        tick(0.0);
        assert_eq!(registry_len(), 0);
    }

    #[test]
    fn split_site_hash_matches_legacy_id() {
        const FILE: &str = "deadsync/src/engine/present/dsl.rs";
        const LINE: u32 = 614;
        const COL: u32 = 9;
        const EXTRA: u64 = 0x53434F4C464F524D;
        const BASE: u64 = site_base(FILE, LINE, COL);
        const ID: u64 = site_id(BASE, EXTRA);

        assert_eq!(ID, legacy_site_id(FILE, LINE, COL, EXTRA));
    }
}
