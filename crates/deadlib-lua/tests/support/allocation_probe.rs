//! Allocator rejection for tests; no production allocator is replaced.
#![allow(unsafe_code)]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Allocations {
    pub(crate) attempts: usize,
    pub(crate) rejected: usize,
    pub(crate) deny_from: usize,
    pub(crate) enabled: bool,
}
thread_local! {
    static ALLOCATIONS: Cell<Allocations> = const { Cell::new(Allocations {
        attempts: 0, rejected: 0, deny_from: 0, enabled: false,
    }) };
}
struct RejectingAllocator;
fn reject() -> bool {
    ALLOCATIONS
        .try_with(|cell| {
            let mut state = cell.get();
            if !state.enabled {
                return false;
            }
            state.attempts += 1;
            let denied = state.deny_from != 0 && state.attempts >= state.deny_from;
            state.rejected += usize::from(denied);
            cell.set(state);
            denied
        })
        .unwrap_or(false)
}
// SAFETY: accepted requests and every deallocation go to System unchanged.
// A rejected allocation returns NULL; a rejected reallocation retains the old
// block. The hook neither allocates nor unwinds, and its TLS needs no allocation.
unsafe impl GlobalAlloc for RejectingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if reject() {
            std::ptr::null_mut()
        } else {
            unsafe { System.alloc(layout) }
        }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if reject() {
            std::ptr::null_mut()
        } else {
            unsafe { System.alloc_zeroed(layout) }
        }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if reject() {
            std::ptr::null_mut()
        } else {
            unsafe { System.realloc(pointer, layout, size) }
        }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: RejectingAllocator = RejectingAllocator;
struct Disarm;
impl Drop for Disarm {
    fn drop(&mut self) {
        ALLOCATIONS.with(|cell| {
            let mut state = cell.get();
            state.enabled = false;
            cell.set(state)
        });
    }
}
pub(crate) fn probe<T>(deny_from: usize, action: impl FnOnce() -> T) -> (T, Allocations) {
    ALLOCATIONS.with(|cell| {
        assert!(!cell.get().enabled);
        cell.set(Allocations {
            deny_from,
            enabled: true,
            ..Allocations::default()
        });
    });
    let guard = Disarm;
    let result = action();
    let state = ALLOCATIONS.with(Cell::get);
    drop(guard);
    (result, state)
}
