use crate::MusicMapSeg;
use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// Pre-roll input frames and ring capacity.
pub const PREROLL_IN_FRAMES: u64 = 8;
pub const RING_CAP_SAMPLES: usize = 1 << 16;
pub const MUSIC_SEG_RING_CAP: usize = 1 << 11;

pub struct SpscRingI16 {
    buf: UnsafeCell<Box<[i16]>>,
    mask: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

// SAFETY: the ring is intentionally single-producer/single-consumer. Interior
// mutability is synchronized by the `head`/`tail` atomics, and callers only
// access the buffer through the ring API.
unsafe impl Send for SpscRingI16 {}
// SAFETY: shared references are safe because producer and consumer operate on
// disjoint logical regions and publish ownership with atomic ordering.
unsafe impl Sync for SpscRingI16 {}

pub fn ring_new(cap_pow2: usize) -> Arc<SpscRingI16> {
    assert!(cap_pow2.is_power_of_two());
    Arc::new(SpscRingI16 {
        buf: UnsafeCell::new(vec![0i16; cap_pow2].into_boxed_slice()),
        mask: cap_pow2 - 1,
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
    })
}

#[inline(always)]
fn ring_cap(r: &SpscRingI16) -> usize {
    // SAFETY: the boxed slice is allocated once at construction time and never
    // moved out of `buf`; taking a shared view to read its length is safe.
    unsafe { (&*r.buf.get()).len() }
}

#[inline(always)]
pub fn ring_free_samples(r: &SpscRingI16) -> usize {
    let cap = ring_cap(r);
    let h = r.head.load(Ordering::Relaxed);
    let t = r.tail.load(Ordering::Acquire);
    cap.saturating_sub(h.wrapping_sub(t))
}

pub fn ring_push(r: &SpscRingI16, data: &[i16]) -> usize {
    let cap = ring_cap(r);
    let mask = r.mask;
    let h = r.head.load(Ordering::Relaxed);
    let t = r.tail.load(Ordering::Acquire);
    let free = cap - h.wrapping_sub(t);
    let n = data.len().min(free);
    if n == 0 {
        return 0;
    }
    let idx = h & mask;
    // SAFETY: this is the single producer. The free-space check above ensures
    // the consumer cannot be reading the slots being written, and publication
    // happens only after the copies complete via the Release store to `head`.
    unsafe {
        let buf = &mut *r.buf.get();
        let first = (cap - idx).min(n);
        buf[idx..idx + first].copy_from_slice(&data[..first]);
        if n > first {
            buf[0..(n - first)].copy_from_slice(&data[first..n]);
        }
    }
    r.head.store(h.wrapping_add(n), Ordering::Release);
    n
}

pub fn ring_pop(r: &SpscRingI16, out: &mut [i16]) -> usize {
    let cap = ring_cap(r);
    let mask = r.mask;
    let h = r.head.load(Ordering::Acquire);
    let t = r.tail.load(Ordering::Relaxed);
    let avail = h.wrapping_sub(t);
    let n = out.len().min(avail);
    if n == 0 {
        return 0;
    }
    let idx = t & mask;
    // SAFETY: this is the single consumer. The Acquire load of `head`
    // guarantees the producer finished writing the visible region before we
    // copy from it, and these slots are not mutated again until `tail` advances.
    unsafe {
        let buf = &*r.buf.get();
        let first = (cap - idx).min(n);
        out[..first].copy_from_slice(&buf[idx..idx + first]);
        if n > first {
            out[first..n].copy_from_slice(&buf[0..(n - first)]);
        }
    }
    r.tail.store(t.wrapping_add(n), Ordering::Release);
    n
}

pub fn ring_clear(r: &SpscRingI16) {
    // This is called from the manager thread when the producer is stopped. It
    // makes the buffer appear empty to the consumer callback.
    let tail_pos = r.tail.load(Ordering::Relaxed);
    r.head.store(tail_pos, Ordering::Release);
}

/// Fill `dst` from the ring buffer, returning the number of interleaved samples
/// actually popped from the ring. Any remaining slots are zeroed.
pub fn callback_fill_from_ring_i16(ring: &SpscRingI16, dst: &mut [i16]) -> usize {
    let mut filled = 0;
    while filled < dst.len() {
        let got = ring_pop(ring, &mut dst[filled..]);
        if got == 0 {
            for d in &mut dst[filled..] {
                *d = 0;
            }
            break;
        }
        filled += got;
    }
    filled
}

pub struct SpscRingMusicSeg {
    buf: UnsafeCell<Box<[MusicMapSeg]>>,
    mask: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

// SAFETY: this ring follows the same SPSC discipline as `SpscRingI16`; the only
// interior mutation is coordinated through the atomic indices.
unsafe impl Send for SpscRingMusicSeg {}
// SAFETY: shared references are safe because producer and consumer operate on
// disjoint logical regions and publish ownership with atomic ordering.
unsafe impl Sync for SpscRingMusicSeg {}

pub fn music_seg_ring_new(cap_pow2: usize) -> Arc<SpscRingMusicSeg> {
    assert!(cap_pow2.is_power_of_two());
    Arc::new(SpscRingMusicSeg {
        buf: UnsafeCell::new(vec![MusicMapSeg::default(); cap_pow2].into_boxed_slice()),
        mask: cap_pow2 - 1,
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
    })
}

#[inline(always)]
fn music_seg_ring_cap(r: &SpscRingMusicSeg) -> usize {
    // SAFETY: the boxed slice is allocated once at construction time and never
    // moved out of `buf`; taking a shared view to read its length is safe.
    unsafe { (&*r.buf.get()).len() }
}

#[inline(always)]
pub fn music_seg_ring_has_space(r: &SpscRingMusicSeg) -> bool {
    let cap = music_seg_ring_cap(r);
    let h = r.head.load(Ordering::Relaxed);
    let t = r.tail.load(Ordering::Acquire);
    h.wrapping_sub(t) < cap
}

pub fn music_seg_ring_push(r: &SpscRingMusicSeg, seg: MusicMapSeg) -> bool {
    let cap = music_seg_ring_cap(r);
    let h = r.head.load(Ordering::Relaxed);
    let t = r.tail.load(Ordering::Acquire);
    if h.wrapping_sub(t) >= cap {
        return false;
    }
    let idx = h & r.mask;
    // SAFETY: this is the single producer. The capacity check guarantees the
    // consumer is not reading this slot, and the Release store to `head`
    // publishes the initialized segment only after the write completes.
    unsafe {
        (&mut *r.buf.get())[idx] = seg;
    }
    r.head.store(h.wrapping_add(1), Ordering::Release);
    true
}

pub fn music_seg_ring_pop(r: &SpscRingMusicSeg) -> Option<MusicMapSeg> {
    let h = r.head.load(Ordering::Acquire);
    let t = r.tail.load(Ordering::Relaxed);
    if h == t {
        return None;
    }
    let idx = t & r.mask;
    // SAFETY: this is the single consumer. The Acquire load of `head` guarantees
    // the producer has finished writing this slot before we copy the value out.
    let seg = unsafe { (&*r.buf.get())[idx] };
    r.tail.store(t.wrapping_add(1), Ordering::Release);
    Some(seg)
}

pub fn music_seg_ring_clear(r: &SpscRingMusicSeg) {
    let tail_pos = r.tail.load(Ordering::Relaxed);
    r.head.store(tail_pos, Ordering::Release);
}
