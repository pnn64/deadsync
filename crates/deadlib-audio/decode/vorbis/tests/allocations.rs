mod support;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::atomic::{AtomicUsize, Ordering},
};
struct Counting;
thread_local! { static TRACK: Cell<bool> = const { Cell::new(false) }; }
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
// Test-only forwarding allocator. Every unsafe call preserves the exact
// allocator contract/layout supplied by Rust; no production unsafe code.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK.try_with(Cell::get).unwrap_or(false) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if TRACK.try_with(Cell::get).unwrap_or(false) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn repeated_packet_decode_and_reset_allocate_nothing() {
    for bytes in [
        include_bytes!("fixtures/stereo.ogg").as_slice(),
        include_bytes!("fixtures/surround.ogg").as_slice(),
        include_bytes!("fixtures/mono.ogg").as_slice(),
    ] {
        let packets = support::packets(bytes);
        let mut decoder = support::decoder(&packets);
        ALLOCATIONS.store(0, Ordering::Relaxed);
        let mut samples = 0;
        TRACK.with(|track| track.set(true));
        for _ in 0..4 {
            decoder.reset();
            for packet in &packets[3..] {
                let block = decoder.decode(packet).unwrap().unwrap();
                samples += block.frames * block.channels;
                for plane in block.planes() {
                    std::hint::black_box(plane);
                }
            }
        }
        TRACK.with(|track| track.set(false));
        assert!(samples > 0);
        assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    }
}
