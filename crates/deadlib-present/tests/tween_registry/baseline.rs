// Frozen from 6513c22e2; runtime logic is unchanged.
use std::{cell::RefCell, collections::HashMap};

use crate::anim::{Step, TweenSeq, TweenState};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

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

        let stored_index = self.indices.get_mut(&id)?;
        let index = *stored_index;
        if self.entries[index].last_seen_frame == self.frame {
            return Some(*self.entries[index].seq.state());
        }

        debug_assert!(cursor < self.entries.len());
        if index != cursor {
            self.entries.swap(index, cursor);
            let displaced_id = self.entries[index].id;
            *stored_index = cursor;
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
        self.indices.insert(id, cursor);

        // New actors can appear before retained-but-unseen actors. Keep the
        // observed order contiguous so the next stable frame is hash-free.
        if cursor != new_index {
            self.entries.swap(cursor, new_index);
            let displaced_id = self.entries[new_index].id;
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
/// # Panics
///
/// Panics if an internal state invariant is violated.
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
#[must_use]
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
#[must_use]
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
#[must_use]
pub const fn site_id(site_base: u64, extra: u64) -> u64 {
    site_base ^ extra
}

// Optional manual clear (e.g., on screen swaps if desired).
pub fn clear_all() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}
