use std::{cell::RefCell, collections::HashMap};

use crate::ui::anim::{Step, TweenSeq, TweenState};

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

struct Entry {
    seq: TweenSeq,
    last_seen_frame: u64,
}

#[derive(Default)]
struct Registry {
    map: HashMap<u64, Entry>,
    frame: u64,
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

        // Drop anything not seen in the current or previous frame. `wrapping_sub`
        // keeps the one-frame grace semantics correct across `u64` wraparound.
        r.map.retain(|_, e| {
            let keep = seen_recently(e.last_seen_frame, frame);
            if keep {
                e.seq.update(dt);
            }
            keep
        });
    });
}

/// Get/create a tween at this callsite and return its current state.
/// `steps` are only enqueued on first sight of this site id.
pub fn materialize(id: u64, initial: TweenState, steps: &[Step]) -> TweenState {
    REG.with(|r| {
        let mut r = r.borrow_mut();
        let frame = r.frame;

        let ent = r.map.entry(id).or_insert_with(|| {
            let mut tw = TweenSeq::new(initial);
            for s in steps {
                tw.push_step(s.clone());
            }
            Entry {
                seq: tw,
                last_seen_frame: frame,
            }
        });

        ent.last_seen_frame = frame;
        ent.seq.state().clone()
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
    use crate::ui::anim;

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
        const FILE: &str = "deadsync/src/ui/dsl.rs";
        const LINE: u32 = 614;
        const COL: u32 = 9;
        const EXTRA: u64 = 0x53434F4C464F524D;
        const BASE: u64 = site_base(FILE, LINE, COL);
        const ID: u64 = site_id(BASE, EXTRA);

        assert_eq!(ID, legacy_site_id(FILE, LINE, COL, EXTRA));
    }
}
