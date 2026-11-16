use super::*;
use crate::{ErrorKind, Value, Vm};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
mod bindings;
mod finalizers;
mod host;
mod shared;

pub(crate) fn with_vm(test: impl FnOnce(&mut Vm, &Cell<usize>, &Cell<usize>)) {
    with_memory(|memory, attempts, reject| {
        let mut vm = Vm::try_bare_in(memory.clone()).unwrap();
        test(&mut vm, attempts, reject);
    });
}

pub(crate) fn with_memory(test: impl FnOnce(&Memory, &Cell<usize>, &Cell<usize>)) {
    let ledger = Ledger::default();
    let endpoint = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    // SAFETY: callback data remains live until the VM and every owner are dropped.
    let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
    test(&memory, &ledger.attempts, &ledger.reject_at);
    drop(memory);
    assert!(ledger.blocks.borrow().is_empty());
}

#[derive(Default)]
struct Ledger {
    blocks: RefCell<BTreeMap<usize, usize>>,
    calls: RefCell<Vec<(u8, usize, usize)>>,
    attempts: Cell<usize>,
    reject_at: Cell<usize>,
    nested: Cell<*const Memory>,
}
struct Endpoint<'a> {
    ledger: &'a Ledger,
    tag: u8,
}
impl Endpoint<'_> {
    fn allocator(&self) -> Allocator {
        Allocator {
            callback: if self.tag == 1 { first } else { second },
            userdata: (self as *const Self).cast_mut().cast(),
        }
    }
}
unsafe extern "C" fn first(ud: *mut c_void, p: *mut c_void, old: usize, new: usize) -> *mut c_void {
    // SAFETY: tests retain their endpoint and ledger until every block is freed.
    unsafe { allocate(1, ud, p, old, new) }
}
unsafe extern "C" fn second(
    ud: *mut c_void,
    p: *mut c_void,
    old: usize,
    new: usize,
) -> *mut c_void {
    // SAFETY: tests retain their endpoint and ledger until every block is freed.
    unsafe { allocate(2, ud, p, old, new) }
}
unsafe fn allocate(
    tag: u8,
    ud: *mut c_void,
    p: *mut c_void,
    old: usize,
    new: usize,
) -> *mut c_void {
    // SAFETY: the callback's userdata comes from Endpoint::allocator above.
    let endpoint = unsafe { &*ud.cast::<Endpoint<'_>>() };
    assert_eq!(endpoint.tag, tag);
    let ledger = endpoint.ledger;
    ledger.calls.borrow_mut().push((tag, old, new));
    if p.is_null() {
        assert_eq!(old, 0);
    } else {
        assert_eq!(ledger.blocks.borrow().get(&(p as usize)), Some(&old));
    }
    if new != 0 {
        ledger.attempts.set(ledger.attempts.get() + 1);
        if ledger.reject_at.get() != 0 && ledger.attempts.get() >= ledger.reject_at.get() {
            return std::ptr::null_mut();
        }
        if let Some(nested) = NonNull::new(ledger.nested.get().cast_mut()) {
            // SAFETY: the nested domain outlives this callback and is distinct.
            let mut scratch = Buffer::<u8>::new_in(unsafe { nested.as_ref() }.clone());
            scratch.try_reserve_exact(19).unwrap();
            scratch.extend_from_slice(b"nested allocator");
        }
    }
    let layout = |size| Layout::from_size_align(size, ALIGNMENT).unwrap();
    // SAFETY: the independent ledger above verified the original size/identity;
    // all test callback allocations use this same, Lua-compatible alignment.
    let result = unsafe {
        if old == 0 {
            std::alloc::alloc(layout(new))
        } else if new == 0 {
            std::alloc::dealloc(p.cast(), layout(old));
            std::ptr::null_mut()
        } else {
            std::alloc::realloc(p.cast(), layout(old), new)
        }
    };
    if new == 0 || !result.is_null() {
        if old != 0 {
            assert_eq!(ledger.blocks.borrow_mut().remove(&(p as usize)), Some(old));
        }
        if new != 0 {
            assert!(
                ledger
                    .blocks
                    .borrow_mut()
                    .insert(result as usize, new)
                    .is_none()
            );
        }
    }
    result.cast()
}

#[test]
fn replacement_uses_new_callback_and_userdata_for_existing_blocks_and_owner() {
    let ledger = Ledger::default();
    let first = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    let second = Endpoint {
        ledger: &ledger,
        tag: 2,
    };
    // SAFETY: both endpoints remain live, and both accept the same tracked heap.
    let memory = unsafe { Memory::custom(first.allocator()) }.unwrap();
    assert_eq!(
        memory.allocator().unwrap().userdata,
        first.allocator().userdata
    );
    let mut buffer = Buffer::<u64>::new_in(memory.clone());
    buffer.try_reserve_exact(3).unwrap();
    buffer.extend_from_slice(&[11, 22, 33]);
    let before = memory.usage().unwrap();
    assert_eq!(before.blocks, 2);
    // SAFETY: the replacement owns the same heap and original block sizes.
    unsafe {
        memory.replace(second.allocator());
    }
    // Function addresses may differ for the same Rust function, including under
    // Miri. Exercise the returned callback: the endpoint checks its tag and exact
    // sizes, so returning the previous callback with new userdata fails here.
    let current = memory.allocator().unwrap();
    // SAFETY: both endpoints remain live; this separate probe is allocated and
    // released directly through the returned callback, outside domain accounting.
    unsafe {
        let probe = (current.callback)(current.userdata, std::ptr::null_mut(), 0, 13);
        assert!(!probe.is_null());
        assert!((current.callback)(current.userdata, probe, 13, 0).is_null());
    }
    assert_eq!(
        memory.allocator().unwrap().userdata,
        second.allocator().userdata
    );
    buffer.try_reserve_exact(17).unwrap();
    assert_eq!(&*buffer, &[11, 22, 33]);
    assert_eq!(memory.usage().unwrap().bytes, before.bytes + 17 * 8);
    drop(memory); // Buffer keeps the callback owner alive independently.
    assert!(!ledger.blocks.borrow().is_empty());
    drop(buffer);
    assert!(ledger.blocks.borrow().is_empty());
    assert!(
        ledger
            .calls
            .borrow()
            .iter()
            .rev()
            .take(2)
            .all(|&(tag, _, new)| tag == 2 && new == 0)
    );
}

#[test]
fn growth_failure_keeps_initialized_data_capacity_and_exact_ownership() {
    let ledger = Ledger::default();
    let endpoint = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    // SAFETY: endpoint/ledger outlive all allocations in this scope.
    let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
    let mut buffer = Buffer::<u32>::new_in(memory.clone());
    buffer.try_reserve_exact(4).unwrap();
    buffer.extend_from_slice(&[1, 2, 3, 4]);
    let pointer = buffer.as_ptr();
    let usage = memory.usage();
    ledger.reject_at.set(ledger.attempts.get() + 1);
    assert_eq!(buffer.try_reserve(1).unwrap_err().kind(), ErrorKind::Memory);
    assert_eq!(buffer.as_ptr(), pointer);
    assert_eq!(buffer.capacity(), 4);
    assert_eq!(&*buffer, &[1, 2, 3, 4]);
    assert_eq!(memory.usage(), usage);
    ledger.reject_at.set(0);
    buffer.try_reserve(1).unwrap();
    buffer.push(5);
    buffer.retain(|v| v % 2 != 0);
    assert_eq!(&*buffer, &[1, 3, 5]);
    drop(buffer);
    drop(memory);
    assert!(ledger.blocks.borrow().is_empty());
}

#[test]
fn each_custom_bare_bootstrap_failure_releases_every_owned_block() {
    fn run(reject_at: usize) -> usize {
        let ledger = Ledger::default();
        ledger.reject_at.set(reject_at);
        let endpoint = Endpoint {
            ledger: &ledger,
            tag: 1,
        };
        // SAFETY: endpoint/ledger outlive the entire attempt and its cleanup.
        let result = unsafe { Memory::custom(endpoint.allocator()) }.and_then(Vm::try_bare_in);
        if reject_at != 0 {
            assert_eq!(result.as_ref().unwrap_err().kind(), ErrorKind::Memory);
        } else {
            assert!(result.is_ok());
        }
        drop(result);
        assert!(
            ledger.blocks.borrow().is_empty(),
            "bootstrap failure {reject_at}"
        );
        ledger.attempts.get()
    }
    let total = run(0);
    assert!(total > 20);
    for nth in 1..=total {
        run(nth);
    }
    eprintln!("CUSTOM_BOOTSTRAP_REJECTION_POINTS {total}");
}

#[test]
fn vm_objects_and_nested_independent_allocations_keep_their_owner() {
    let other = Ledger::default();
    let other_endpoint = Endpoint {
        ledger: &other,
        tag: 2,
    };
    // SAFETY: both independent domains and userdata are serialized and retained.
    let other_memory = unsafe { Memory::custom(other_endpoint.allocator()) }.unwrap();
    let ledger = Ledger::default();
    ledger.nested.set(&other_memory);
    let endpoint = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    // SAFETY: endpoint remains live through VM and buffer teardown.
    let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
    let mut vm = Vm::try_bare_in(memory.clone()).unwrap();
    let table = vm.try_new_table().unwrap();
    let root = vm.pin(Value::Table(table)).unwrap();
    for i in 0..80 {
        let key = vm.try_intern(format!("owned-key-{i}").as_bytes()).unwrap();
        vm.raw_table_set(table, key, Value::Number(i as f64))
            .unwrap();
        vm.raw_table_set(table, Value::Number((i + 1) as f64), key)
            .unwrap();
    }
    let user = vm.try_new_userdata(73).unwrap();
    vm.raw_table_set(table, Value::Number(100.), user).unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(vm.rooted(root).unwrap(), Value::Table(table));
    assert_eq!(
        memory.usage().unwrap().bytes,
        ledger.blocks.borrow().values().sum()
    );
    drop(vm);
    assert_eq!(memory.usage().unwrap().blocks, 1);
    drop(memory);
    assert!(ledger.blocks.borrow().is_empty());
    assert_eq!(other_memory.usage().unwrap().blocks, 1);
    assert!(other.calls.borrow().len() > 10);
    drop(other_memory);
    assert!(other.blocks.borrow().is_empty());
}

#[test]
fn initialized_elements_and_backing_storage_drop_once_even_on_unwind() {
    struct Item<'a> {
        drops: &'a Cell<usize>,
        fail: bool,
    }
    impl Drop for Item<'_> {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
            assert!(!self.fail, "intentional drop failure");
        }
    }
    let drops = Cell::new(0);
    let ledger = Ledger::default();
    let endpoint = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    // SAFETY: endpoint outlives both normal and unwinding buffer destruction.
    let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
    let mut buffer = Buffer::new_in(memory.clone());
    buffer.try_reserve_exact(3).unwrap();
    for fail in [true, false, false] {
        buffer.push(Item {
            drops: &drops,
            fail,
        });
    }
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(buffer))).is_err());
    assert_eq!(drops.get(), 3);
    assert_eq!(memory.usage().unwrap().blocks, 1);
    drop(memory);
    assert!(ledger.blocks.borrow().is_empty());
}

#[test]
fn resizing_preserves_the_initialized_tail_when_clone_panics() {
    struct Item<'a> {
        clones: &'a Cell<usize>,
        drops: &'a Cell<usize>,
    }
    impl Clone for Item<'_> {
        fn clone(&self) -> Self {
            let count = self.clones.get() + 1;
            self.clones.set(count);
            assert!(count != 3, "intentional clone failure");
            Self {
                clones: self.clones,
                drops: self.drops,
            }
        }
    }
    impl Drop for Item<'_> {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    let clones = Cell::new(0);
    let drops = Cell::new(0);
    let mut buffer = Buffer::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        buffer.resize(
            5,
            Item {
                clones: &clones,
                drops: &drops,
            },
        );
    }));
    assert!(result.is_err());
    assert_eq!(buffer.len(), 2);
    assert_eq!(clones.get(), 3);
    assert_eq!(drops.get(), 1);
    drop(buffer);
    assert_eq!(drops.get(), 3);
}

#[test]
fn empty_and_zero_sized_buffers_do_not_call_the_allocator_and_overflow_is_recoverable() {
    let ledger = Ledger::default();
    let endpoint = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    // SAFETY: endpoint is retained until every buffer and owner is destroyed.
    let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
    let before = ledger.attempts.get();
    let mut zero = Buffer::<()>::new_in(memory.clone());
    for _ in 0..8 {
        zero.push(());
    }
    assert_eq!(zero.pop(), Some(()));
    assert_eq!(
        zero.try_reserve(usize::MAX).unwrap_err().kind(),
        ErrorKind::Memory
    );
    assert_eq!(zero.len(), 7);
    zero.resize(12, ());
    assert_eq!(zero.len(), 12);
    let mut bytes = Buffer::<u64>::new_in(memory.clone());
    assert_eq!(
        bytes
            .try_reserve_exact(usize::MAX / 8 + 1)
            .unwrap_err()
            .kind(),
        ErrorKind::Memory
    );
    assert_eq!(ledger.attempts.get(), before);
    drop(bytes);
    drop(zero);
    drop(memory);
    assert!(ledger.blocks.borrow().is_empty());
}

#[test]
fn standard_buffers_support_extended_alignment_and_custom_domains_reject_it_safely() {
    #[derive(Clone, Copy)]
    #[repr(align(64))]
    struct Wide(u8);
    let mut buffer = Buffer::default();
    buffer.try_reserve_exact(1).unwrap();
    buffer.push(Wide(42));
    assert_eq!(buffer.as_ptr() as usize % 64, 0);
    assert_eq!(buffer[0].0, 42);
    let ledger = Ledger::default();
    let endpoint = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    // SAFETY: this allocator promises Lua's pointer/double alignment, not 64.
    let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
    let mut custom = Buffer::<Wide>::new_in(memory.clone());
    let before = ledger.attempts.get();
    assert_eq!(
        custom.try_reserve_exact(1).unwrap_err().kind(),
        ErrorKind::Memory
    );
    assert_eq!(ledger.attempts.get(), before);
    drop(custom);
    drop(memory);
    assert!(ledger.blocks.borrow().is_empty());
}

#[test]
fn shared_domains_serialize_callbacks_and_release_the_control_block_after_all_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Vm>();
    let ledger = Ledger::default();
    let endpoint = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    // SAFETY: callback access to the ledger is serialized by Memory. The host
    // reads it only after joining every thread; endpoint remains live throughout.
    let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
    std::thread::scope(|scope| {
        for worker in 0..8 {
            let memory = memory.clone();
            scope.spawn(move || {
                for size in 1..=32 {
                    let mut buffer = Buffer::<u64>::new_in(memory.clone());
                    buffer.try_reserve_exact(size + worker).unwrap();
                    for value in 0..size {
                        buffer.push(value as u64);
                    }
                    assert_eq!(buffer.last(), Some(&((size - 1) as u64)));
                }
            });
        }
    });
    assert_eq!(memory.usage().unwrap().blocks, 1);
    assert_eq!(ledger.blocks.borrow().len(), 1);
    drop(memory);
    assert!(ledger.blocks.borrow().is_empty());
}

#[test]
fn allocator_replacement_reaches_vm_strings_userdata_and_selected_threads() {
    let ledger = Ledger::default();
    let first = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    let second = Endpoint {
        ledger: &ledger,
        tag: 2,
    };
    // SAFETY: both endpoints operate on the same live allocation ledger.
    let memory = unsafe { Memory::custom(first.allocator()) }.unwrap();
    let mut vm = Vm::try_bare_in(memory.clone()).unwrap();
    let original = vm.try_intern(b"original string").unwrap();
    let root = vm.pin(original).unwrap();
    let function = vm
        .new_host_function(return_capture, &[Value::Number(42.)])
        .unwrap();
    // new_thread may collect before the closure is placed on the child stack.
    let function_root = vm.pin(function).unwrap();
    let changed = ledger.calls.borrow().len();
    // SAFETY: the replacement accepts every earlier block and its exact size.
    unsafe {
        memory.replace(second.allocator());
    }
    vm.try_new_userdata(100).unwrap();
    vm.with_native_stack(|stack| {
        let thread = stack.new_thread()?;
        stack
            .thread_stack(thread)?
            .push_bytes(b"selected-thread string")?;
        stack.thread_stack(thread)?.push(function)?;
        stack.call_unprotected_thread(thread, 0, Some(1))?;
        assert_eq!(
            stack.thread_stack(thread)?.value(-1),
            Some(Value::Number(42.))
        );
        Ok(())
    })
    .unwrap();
    vm.unpin(function_root).unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(vm.rooted(root).unwrap(), original);
    drop(vm);
    drop(memory);
    assert!(
        ledger.calls.borrow()[changed..]
            .iter()
            .all(|&(tag, _, _)| tag == 2)
    );
    assert!(ledger.blocks.borrow().is_empty());
}

fn return_capture(call: &mut crate::HostCall<'_>) -> Result<(), Error> {
    call.push(call.captures()[0])
}

#[test]
fn native_capture_and_arena_failures_leave_no_partial_closure_and_allow_retry() {
    fn run(reject_at: usize) -> usize {
        let ledger = Ledger::default();
        let endpoint = Endpoint {
            ledger: &ledger,
            tag: 1,
        };
        // SAFETY: the endpoint and ledger outlive the VM and all owned storage.
        let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
        let mut vm = Vm::try_bare_in(memory.clone()).unwrap();
        let start = ledger.attempts.get();
        ledger
            .reject_at
            .set(if reject_at == 0 { 0 } else { start + reject_at });
        let result = vm.new_host_function(return_capture, &[Value::Number(42.)]);
        let points = ledger.attempts.get() - start;
        if reject_at != 0 {
            let error = result.unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Memory);
            assert_eq!(
                vm.string_bytes(match error.value().unwrap() {
                    Value::String(id) => id,
                    _ => panic!("fixed memory diagnostic"),
                }),
                Some(b"not enough memory".as_slice())
            );
            assert_eq!(vm.heap_stats().native_closures, 0);
        }
        ledger.reject_at.set(0);
        let function = vm
            .new_host_function(return_capture, &[Value::Number(42.)])
            .unwrap();
        assert_eq!(vm.call(function, &[]).unwrap(), &[Value::Number(42.)]);
        drop(vm);
        drop(memory);
        assert!(
            ledger.blocks.borrow().is_empty(),
            "native closure failure {reject_at}"
        );
        points
    }
    let total = run(0);
    assert!(total >= 5);
    for nth in 1..=total {
        run(nth);
    }
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {total}");
}

#[test]
fn native_thread_and_stack_failures_preserve_memory_errors_and_allow_reuse() {
    fn create_threads(vm: &mut Vm) -> Result<(), Error> {
        vm.with_native_stack(|stack| {
            for i in 0..12 {
                let thread = stack.new_thread()?;
                stack.thread_stack(thread)?.push(Value::Number(i as f64))?;
            }
            Ok(())
        })
    }
    fn run(reject_at: usize) -> usize {
        let ledger = Ledger::default();
        let endpoint = Endpoint {
            ledger: &ledger,
            tag: 1,
        };
        // SAFETY: the endpoint and ledger outlive the VM and all owned storage.
        let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
        let mut vm = Vm::try_bare_in(memory.clone()).unwrap();
        let start = ledger.attempts.get();
        ledger
            .reject_at
            .set(if reject_at == 0 { 0 } else { start + reject_at });
        let result = create_threads(&mut vm);
        let points = ledger.attempts.get() - start;
        if reject_at != 0 {
            assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
        } else {
            result.unwrap();
        }
        ledger.reject_at.set(0);
        vm.with_native_stack(|stack| stack.set_top(0)).unwrap();
        vm.collect_garbage().unwrap();
        assert_eq!(vm.heap_stats().threads, 1);
        create_threads(&mut vm).unwrap();
        drop(vm);
        drop(memory);
        assert!(
            ledger.blocks.borrow().is_empty(),
            "thread creation failure {reject_at}"
        );
        points
    }
    let total = run(0);
    assert!(total >= 15);
    for nth in 1..=total {
        run(nth);
    }
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {total}");
}

#[test]
fn collection_does_not_grow_the_string_index_when_the_allocator_rejects_growth() {
    let ledger = Ledger::default();
    let endpoint = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    // SAFETY: the endpoint and ledger outlive all owned storage.
    let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
    let mut vm = Vm::try_bare_in(memory.clone()).unwrap();
    let value = vm.try_intern(b"retained string").unwrap();
    let root = vm.pin(value).unwrap();
    // Prepare the separately owned trace buffers before rejecting growth. The
    // assertion below still forbids every allocation during index compaction.
    vm.collect_garbage().unwrap();
    let before = ledger.attempts.get();
    ledger.reject_at.set(before + 1);
    vm.collect_garbage().unwrap();
    assert_eq!(ledger.attempts.get(), before);
    assert_eq!(vm.try_intern(b"retained string").unwrap(), value);
    assert_eq!(vm.rooted(root).unwrap(), value);
    drop(vm);
    drop(memory);
    assert!(ledger.blocks.borrow().is_empty());
}
