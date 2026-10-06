//! Tests production composition and allocation behavior through private hot paths.
#[path = "../src/actors.rs"]
pub mod actors;
#[path = "../src/anim.rs"]
pub mod anim;
#[path = "../src/font.rs"]
pub mod font;
#[path = "../src/space.rs"]
pub mod space;
#[path = "../src/texture.rs"]
pub mod texture;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default)]
struct HeapStats {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    allocated: u64,
    freed: u64,
}

thread_local! {
    static HEAP: Cell<Option<HeapStats>> = const { Cell::new(None) };
}

struct CountingAlloc;

fn record_heap(change: impl FnOnce(&mut HeapStats)) {
    let _ = HEAP.try_with(|cell| {
        if let Some(mut stats) = cell.get() {
            change(&mut stats);
            cell.set(Some(stats));
        }
    });
}

// SAFETY: every operation delegates the unchanged pointer/layout contract to
// System; counters are thread-local and never allocate or retain pointers.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies a valid allocation layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record_heap(|s| {
                s.allocs += 1;
                s.allocated += layout.size() as u64;
            });
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record_heap(|s| {
            s.frees += 1;
            s.freed += layout.size() as u64;
        });
        // SAFETY: pointer and layout came from this System-backed allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the caller supplies the original allocation and valid size.
        let next = unsafe { System.realloc(ptr, layout, size) };
        if !next.is_null() {
            record_heap(|s| {
                s.reallocs += 1;
                s.allocated += size as u64;
                s.freed += layout.size() as u64;
            });
        }
        next
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

pub mod compose {
    include!("../src/compose.rs");

    mod perf {
        include!("masked_render/cases.rs");
    }
}
