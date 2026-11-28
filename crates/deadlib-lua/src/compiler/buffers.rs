//! Keep a bounded amount of retired prototype storage in the VM compiler spare.
use crate::bytecode::{ProgramData, RecycledPrototype};
use crate::memory::fallible::VecExt;
use std::cell::RefCell;

const MIN_INSTRUCTIONS: usize = 1;
const MAX_INSTRUCTIONS: usize = 256;
const MAX_PER_SIZE: usize = 64;
const MAX_RETAINED_BYTES: usize = 1024 * 1024;
type Entries = [Vec<RecycledPrototype>; MAX_INSTRUCTIONS - MIN_INSTRUCTIONS + 1];

thread_local! {
    static ACTIVE: RefCell<Pool> = const { RefCell::new(Pool { entries: None, retained: 0 }) };
}

#[derive(Default)]
pub(super) struct Pool {
    entries: Option<Box<Entries>>,
    retained: usize,
}

struct Scope<'a> {
    pool: &'a mut Pool,
    previous: Pool,
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            *self.pool = active.replace(std::mem::take(&mut self.previous));
        });
    }
}

impl Pool {
    pub(super) fn collect<R>(&mut self, collect: impl FnOnce() -> R) -> R {
        if self.entries.is_none() {
            self.entries = crate::memory::fallible::boxed(std::array::from_fn(|_| Vec::new())).ok();
            if self.entries.is_none() {
                return collect();
            }
        }
        let Ok(previous) = ACTIVE.try_with(|active| active.replace(std::mem::take(self))) else {
            return collect();
        };
        let _scope = Scope {
            pool: self,
            previous,
        };
        collect()
    }

    pub(super) fn take(&mut self, capacity: usize) -> Option<RecycledPrototype> {
        let index = capacity.checked_sub(MIN_INSTRUCTIONS)?;
        let buffers = self.entries.as_mut()?.get_mut(index)?.pop()?;
        self.retained -= buffers.bytes;
        Some(buffers)
    }

    fn recycle(&mut self, data: &mut ProgramData) {
        let Some(entries) = &mut self.entries else {
            return;
        };
        let [prototype] = &*data.prototypes else {
            return;
        };
        let capacity = prototype.code.len();
        if !(MIN_INSTRUCTIONS..=MAX_INSTRUCTIONS).contains(&capacity)
            || prototype.metadata.len() != capacity
        {
            return;
        }
        let bucket = &mut entries[capacity - MIN_INSTRUCTIONS];
        if bucket.len() == MAX_PER_SIZE {
            return;
        }
        let Some(recycled) = data.take_reusable_prototype(MAX_RETAINED_BYTES - self.retained)
        else {
            return;
        };
        let bytes = recycled.bytes;
        if bucket.try_push(recycled).is_ok() {
            self.retained += bytes;
        }
    }
}

pub(crate) fn recycle_retired_code(data: &mut ProgramData) {
    // Programs may outlive the VM or be dropped while the thread shuts down.
    // Only the synchronous program sweep installs a recycling destination.
    let _ = ACTIVE.try_with(|active| active.borrow_mut().recycle(data));
}

#[cfg(test)]
#[test]
fn recycles_buffers_only_after_the_last_program_owner_drops() {
    let program = crate::compile(&b"total = 1;".repeat(24)).unwrap();
    let prototype = &program.data.prototypes[0];
    let count = prototype.code.len();
    let code_address = prototype.code.as_ptr();
    let metadata_address = prototype.metadata.as_ptr();
    let constants_address = prototype.original_constants.as_ptr();
    assert!((32..=256).contains(&count));
    let retained = program.clone();

    let mut pool = Pool::default();
    pool.collect(|| drop(program));
    assert!(pool.take(count).is_none());
    pool.collect(|| drop(retained));

    let (slot, body, _) = pool.take(count).unwrap().into_parts();
    assert!(slot.is_empty());
    assert_eq!(slot.capacity(), 1);
    assert_eq!(body.code.as_ptr(), code_address);
    assert_eq!(body.metadata.as_ptr(), metadata_address);
    assert_eq!(body.original_constants.as_ptr(), constants_address);
}

#[cfg(test)]
#[test]
fn ordinary_vm_compilation_reuses_retired_short_code() {
    let mut vm = crate::Vm::new();
    let first = vm.compile_source(b"return 11", b"=first").unwrap();
    let prototype = &first.data.prototypes[0];
    assert!(prototype.code.len() < 32);
    let code = prototype.code.as_ptr();
    let metadata = prototype.metadata.as_ptr();
    assert_eq!(vm.execute(&first).unwrap(), &[crate::Value::Number(11.0)]);
    drop(first);
    vm.collect_garbage().unwrap();

    let second = vm.compile_source(b"return 47", b"=other").unwrap();
    assert_eq!(second.data.name(), b"=other");
    let prototype = &second.data.prototypes[0];
    assert_eq!(prototype.code.as_ptr(), code);
    assert_eq!(prototype.metadata.as_ptr(), metadata);
    assert_eq!(vm.execute(&second).unwrap(), &[crate::Value::Number(47.0)]);
}
