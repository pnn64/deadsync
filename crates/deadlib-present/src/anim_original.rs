// Update control flow frozen from 076196e8ffecd95eb826151af441d788e17f6d5e.
// Preparation, easing, property formulas and endpoint writes are unchanged and
// shared with production. Keep this generic so its mul_add calls remain the
// baseline when the production active-property loop gains FMA specialization.
use super::*;

#[inline(never)]
pub(super) fn runtime_update(segment: &mut RuntimeSegment, s: &mut TweenState, dt: f32) -> bool {
    // returns true if finished
    if segment.dur == 0.0 {
        return true;
    }

    segment.elapsed = (segment.elapsed + dt).min(segment.dur);

    // The caller finalizes completed segments, so avoid applying the same
    // endpoint twice here.
    if segment.elapsed >= segment.dur {
        return true;
    }

    // Sleeps still consume time, but have no properties to interpolate.
    if segment.prepared.is_empty() {
        return false;
    }

    let a = ease_apply(segment.ease, segment.elapsed / segment.dur);

    for p in &segment.prepared {
        p.apply_lerp(s, a);
    }

    false
}

pub(super) fn update(seq: &mut TweenSeq, mut dt: f32) {
    while dt > 0.0 {
        // pull a step if needed
        if seq.current.is_none() {
            let Some(step) = seq.queue.pop_front() else {
                break;
            };
            seq.current = Some(RuntimeSegment::new(&step.0));
            seq.current
                .as_mut()
                .unwrap()
                .prepare(step.0.build_ops, &seq.state);
        }

        // drive current step
        let seg = seq.current.as_mut().unwrap();
        let before = seg.elapsed;
        if !runtime_update(seg, &mut seq.state, dt) {
            break;
        }
        // Only completed steps pass unused time to the next queued step.
        dt -= (seg.elapsed - before).max(0.0);
        // Snap to exact targets. Sleeps have no prepared operations.
        for p in &seg.prepared {
            p.apply_final(&mut seq.state);
        }
        seq.current = None;
    }
}
