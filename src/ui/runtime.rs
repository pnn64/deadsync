use std::{cell::RefCell, collections::HashMap};

use crate::ui::anim::{Step, TweenSeq, TweenState};

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

/// Advance all tweens once per frame and GC unseen actors from the previous frame.
pub fn tick(dt: f32) {
    REG.with(|r| {
        let mut r = r.borrow_mut();
        r.frame = r.frame.wrapping_add(1);

        for e in r.map.values_mut() {
            e.seq.update(dt);
        }

        let cur = r.frame;
        // Drop anything not seen last frame (one-frame grace is usually enough).
        r.map.retain(|_, e| e.last_seen_frame + 1 >= cur);
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

/// Stable-ish id for a macro callsite, with an optional per-instance discriminator.
pub fn site_id(file: &'static str, line: u32, col: u32, extra: u64) -> u64 {
    // FNV-1a 64
    let mut h = 0xcbf29ce484222325u64;
    for &b in file.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h ^= (u64::from(line) << 32) ^ u64::from(col);
    h = h.wrapping_mul(0x100000001b3);
    h ^= extra;
    h
}

// Optional manual clear (e.g., on screen swaps if desired).
pub fn clear_all() {
    REG.with(|r| r.borrow_mut().map.clear());
}
