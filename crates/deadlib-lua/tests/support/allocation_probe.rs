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
    pub(crate) allocated_bytes: usize,
    pub(crate) freed_bytes: usize,
}
thread_local! {
    static ALLOCATIONS: Cell<Allocations> = const { Cell::new(Allocations {
        attempts: 0, rejected: 0, deny_from: 0, enabled: false,
        allocated_bytes: 0, freed_bytes: 0,
    }) };
}
struct RejectingAllocator;
fn record_bytes(allocated: usize, freed: usize) {
    let _ = ALLOCATIONS.try_with(|cell| {
        let mut state = cell.get();
        if state.enabled {
            state.allocated_bytes += allocated;
            state.freed_bytes += freed;
            cell.set(state);
        }
    });
}
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
            let pointer = unsafe { System.alloc(layout) };
            if !pointer.is_null() {
                record_bytes(layout.size(), 0);
            }
            pointer
        }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if reject() {
            std::ptr::null_mut()
        } else {
            let pointer = unsafe { System.alloc_zeroed(layout) };
            if !pointer.is_null() {
                record_bytes(layout.size(), 0);
            }
            pointer
        }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if reject() {
            std::ptr::null_mut()
        } else {
            let pointer = unsafe { System.realloc(pointer, layout, size) };
            if !pointer.is_null() {
                record_bytes(size, layout.size());
            }
            pointer
        }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record_bytes(0, layout.size());
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: RejectingAllocator = RejectingAllocator;
pub(crate) struct Disarm;
impl Drop for Disarm {
    fn drop(&mut self) {
        ALLOCATIONS.with(|cell| {
            let mut state = cell.get();
            state.enabled = false;
            cell.set(state)
        });
    }
}
pub(crate) fn begin(deny_from: usize) -> Disarm {
    ALLOCATIONS.with(|cell| {
        assert!(!cell.get().enabled);
        cell.set(Allocations {
            deny_from,
            enabled: true,
            ..Allocations::default()
        });
    });
    Disarm
}
pub(crate) fn statistics() -> Allocations {
    ALLOCATIONS.with(Cell::get)
}
/// Change denial without resetting ownership accounting. Zero allows allocations.
#[allow(dead_code)] // Only matrices which include setup/teardown need this hook.
pub(crate) fn reject_from_now(nth: usize) {
    ALLOCATIONS.with(|cell| {
        let mut state = cell.get();
        assert!(state.enabled);
        state.deny_from = if nth == 0 { 0 } else { state.attempts + nth };
        cell.set(state);
    });
}
pub(crate) fn probe<T>(deny_from: usize, action: impl FnOnce() -> T) -> (T, Allocations) {
    let guard = begin(deny_from);
    let result = action();
    let state = statistics();
    drop(guard);
    (result, state)
}
