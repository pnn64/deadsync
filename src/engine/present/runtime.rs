use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::Entry as HashEntry},
    hash::BuildHasherDefault,
};

use crate::engine::present::anim::{Step, TweenSeq, TweenState};
use twox_hash::XxHash64;

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

type TweenMap = HashMap<u64, Entry, BuildHasherDefault<XxHash64>>;

struct Entry {
    seq: TweenSeq,
    last_seen_frame: u64,
}

struct Registry {
    map: TweenMap,
    frame: u64,
    seen_ids: Vec<u64>,
    stale_ids: Vec<u64>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            map: TweenMap::default(),
            frame: 0,
            seen_ids: Vec::new(),
            stale_ids: Vec::new(),
        }
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

        // Dense id lists avoid scanning every hash bucket every frame. `seen_ids`
        // contains ids materialized last frame; `stale_ids` contains ids from the
        // frame before that, which now need one last liveness check.
        let mut stale_ids = std::mem::take(&mut r.stale_ids);
        for id in stale_ids.drain(..) {
            let drop = r
                .map
                .get(&id)
                .is_some_and(|entry| !seen_recently(entry.last_seen_frame, frame));
            if drop {
                r.map.remove(&id);
            }
        }

        let mut seen_ids = std::mem::take(&mut r.seen_ids);
        for &id in &seen_ids {
            if let Some(entry) = r.map.get_mut(&id) {
                entry.seq.update(dt);
            }
        }

        std::mem::swap(&mut r.stale_ids, &mut seen_ids);
        r.seen_ids = stale_ids;
    });
}

/// Get/create a tween at this callsite and return its current state.
/// `steps` are only enqueued on first sight of this site id.
pub fn materialize(id: u64, initial: TweenState, steps: &[Step]) -> TweenState {
    REG.with(|r| {
        let mut r = r.borrow_mut();
        let frame = r.frame;
        let mut mark_seen = false;
        let state = match r.map.entry(id) {
            HashEntry::Occupied(mut occupied) => {
                let ent = occupied.get_mut();
                if ent.last_seen_frame != frame {
                    ent.last_seen_frame = frame;
                    mark_seen = true;
                }
                *ent.seq.state()
            }
            HashEntry::Vacant(vacant) => {
                let mut tw = TweenSeq::new(initial);
                for s in steps {
                    tw.push_step(s.clone());
                }
                let state = *tw.state();
                vacant.insert(Entry {
                    seq: tw,
                    last_seen_frame: frame,
                });
                mark_seen = true;
                state
            }
        };
        if mark_seen {
            r.seen_ids.push(id);
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
    use crate::engine::present::anim;

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
    fn duplicate_materialize_in_frame_updates_once() {
        reset_registry(0);
        let steps = [anim::linear(1.0).x(10.0).build()];

        let _ = materialize(1, TweenState::default(), &steps);
        let _ = materialize(1, TweenState::default(), &steps);

        tick(0.25);

        let state = materialize(1, TweenState::default(), &steps);
        assert!(
            (state.x - 2.5).abs() < 0.0001,
            "expected x ~= 2.5 after one update, got {}",
            state.x
        );
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
