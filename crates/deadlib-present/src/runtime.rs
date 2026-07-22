use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::Entry as HashEntry},
};

use crate::anim::{Step, TweenSeq, TweenState};

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

type TweenMap = HashMap<u64, Entry, rustc_hash::FxBuildHasher>;

struct Entry {
    seq: TweenSeq,
    last_seen_frame: u64,
}

#[derive(Default)]
struct Registry {
    map: TweenMap,
    frame: u64,
    active_ids: Vec<u64>,
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

        // Traverse only active ids instead of every hash bucket. Each live tween
        // is checked and advanced through one entry lookup; unseen tweens drop
        // after one absent materialization frame.
        let Registry {
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
    let cached = REG.with(|r| {
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

    // Build outside the registry borrow so source expressions may safely
    // materialize other actors, matching the eager program's reentrancy.
    let mut tween = TweenSeq::new(initial);
    for step in build_steps() {
        tween.push_step(step);
    }

    REG.with(|r| {
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
                vacant.insert(Entry {
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
        REG.with(|r| r.borrow().map.len())
    }

    fn active_id_len() -> usize {
        REG.with(|r| r.borrow().active_ids.len())
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
