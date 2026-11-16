use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn test_mutation_copies_a_projected_static_before_writing() {
    static ORIGINAL: usize = 17;
    let shared = Shared::try_new_in((), Memory::default()).unwrap();
    let mut projected = shared.project(|_| &ORIGINAL);
    let retained = projected.clone();
    assert!(Shared::ptr_eq(&projected, &retained));
    *Shared::make_mut(&mut projected) = 42;
    assert!(!Shared::ptr_eq(&projected, &retained));
    assert_eq!(*projected, 42);
    assert_eq!(*retained, 17);
    assert_eq!(ORIGINAL, 17);
}

struct Item {
    drops: Arc<AtomicUsize>,
    values: [usize; 3],
    panic: bool,
}
impl Drop for Item {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
        assert!(!self.panic, "intentional shared destructor panic");
    }
}
fn item(drops: &Arc<AtomicUsize>, panic: bool) -> Item {
    Item {
        drops: drops.clone(),
        values: [17, 25, 42],
        panic,
    }
}

#[test]
fn projected_shared_views_retain_the_original_across_threads_and_allocator_replacement() {
    let ledger = Ledger::default();
    let first = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    let second = Endpoint {
        ledger: &ledger,
        tag: 2,
    };
    // SAFETY: both endpoints/ledger outlive all owners; Memory serializes callbacks.
    let memory = unsafe { Memory::custom(first.allocator()) }.unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let original = Shared::try_new_in(item(&drops, false), memory.clone()).unwrap();
    let bytes = original.heap_bytes();
    let erased = original
        .clone()
        .project(|v| v as &(dyn std::any::Any + Send + Sync));
    assert_eq!(erased.downcast_ref::<Item>().unwrap().values[0], 17);
    let slice = original.clone().project(|v| &v.values[1..]);
    let attempts = ledger.attempts.get();
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let view = slice.clone();
            scope.spawn(move || {
                let scalar = view.clone().project(|v| &v[1]);
                assert_eq!(*scalar, 42);
                assert_eq!(&*view, &[25, 42]);
            });
        }
    });
    assert_eq!(ledger.attempts.get(), attempts);
    assert_eq!(memory.usage().unwrap().blocks, 2);
    drop(original);
    drop(erased);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    // SAFETY: replacement accepts every outstanding block on the same ledger.
    unsafe { memory.replace(second.allocator()) };
    drop(memory);
    drop(slice); // Last owner is an unsized subobject, not the original Item.
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert!(ledger.blocks.borrow().is_empty());
    let calls = ledger.calls.borrow();
    assert_eq!(calls[calls.len() - 2], (2, bytes, 0));
    assert!(
        calls
            .iter()
            .rev()
            .take(2)
            .all(|&(tag, _, new)| tag == 2 && new == 0)
    );
}

#[test]
fn shared_failure_and_unwinding_release_the_value_and_exact_allocation_once() {
    let ledger = Ledger::default();
    let endpoint = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    // SAFETY: all allocations are dropped before the endpoint/ledger.
    let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    ledger.reject_at.set(ledger.attempts.get() + 1);
    assert_eq!(
        Shared::try_new_in(item(&drops, false), memory.clone())
            .unwrap_err()
            .kind(),
        ErrorKind::Memory
    );
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert_eq!(memory.usage().unwrap().blocks, 1);
    ledger.reject_at.set(0);
    let value = Shared::try_new_in(item(&drops, true), memory.clone())
        .unwrap()
        .project(|v| &v.values[..]);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(value))).is_err());
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    assert_eq!(memory.usage().unwrap().blocks, 1);
    let value = Shared::try_new_in(item(&drops, false), memory.clone()).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            value.project::<usize>(|_| panic!("intentional projection panic"))
        }))
        .is_err()
    );
    assert_eq!(drops.load(Ordering::Relaxed), 3);
    drop(memory);
    assert!(ledger.blocks.borrow().is_empty());
}

#[test]
fn shared_zero_sized_and_overaligned_owners_drop_with_their_original_layout() {
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    struct Zero;
    impl Drop for Zero {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[repr(align(64))]
    struct Wide(Item);
    let value = Shared::try_new_in(Zero, Memory::default()).unwrap();
    let view = value
        .clone()
        .project(|v| v as &(dyn std::any::Any + Send + Sync));
    drop(value);
    assert_eq!(DROPS.load(Ordering::Relaxed), 0);
    drop(view);
    assert_eq!(DROPS.load(Ordering::Relaxed), 1);
    let drops = Arc::new(AtomicUsize::new(0));
    let wide = Shared::try_new_in(Wide(item(&drops, false)), Memory::default()).unwrap();
    assert_eq!((&*wide as *const Wide) as usize % 64, 0);
    assert_eq!(wide.0.values[2], 42);
    drop(wide);
    let ledger = Ledger::default();
    let endpoint = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    // SAFETY: only Lua's pointer/double alignment is promised; wide is rejected.
    let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
    let attempts = ledger.attempts.get();
    assert_eq!(
        Shared::try_new_in(Wide(item(&drops, false)), memory.clone())
            .unwrap_err()
            .kind(),
        ErrorKind::Memory
    );
    assert_eq!(ledger.attempts.get(), attempts);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    drop(memory);
    assert!(ledger.blocks.borrow().is_empty());
}
