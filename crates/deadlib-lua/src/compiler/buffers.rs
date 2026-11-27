//! Keep a bounded number of retired bytecode buffers in the VM compiler spare.
use crate::bytecode::{Instruction, InstructionMetadata, ProgramData};
use crate::memory::fallible::VecExt;
use std::cell::RefCell;

const MIN_INSTRUCTIONS: usize = 32;
const MAX_INSTRUCTIONS: usize = 256;
const MAX_PER_SIZE: usize = 64;
const MAX_RETAINED_BYTES: usize = 1024 * 1024;
const BYTES_PER_INSTRUCTION: usize = size_of::<Instruction>() + size_of::<InstructionMetadata>();

type Buffers = (Vec<Instruction>, Vec<InstructionMetadata>);
type Entries = [Vec<Buffers>; MAX_INSTRUCTIONS - MIN_INSTRUCTIONS + 1];

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

    pub(super) fn take(&mut self, capacity: usize) -> Option<Buffers> {
        let index = capacity.checked_sub(MIN_INSTRUCTIONS)?;
        let buffers = self.entries.as_mut()?.get_mut(index)?.pop()?;
        self.retained -= capacity * BYTES_PER_INSTRUCTION;
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
        let bytes = capacity * BYTES_PER_INSTRUCTION;
        let bucket = &mut entries[capacity - MIN_INSTRUCTIONS];
        if self.retained + bytes > MAX_RETAINED_BYTES || bucket.len() == MAX_PER_SIZE {
            return;
        }
        let Some((code, metadata)) = data.take_reusable_code() else {
            return;
        };
        let mut code = code.into_vec();
        let mut metadata = metadata.into_vec();
        code.clear();
        metadata.clear();
        if bucket.try_push((code, metadata)).is_ok() {
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
    assert!((32..=256).contains(&count));
    let retained = program.clone();

    let mut pool = Pool::default();
    pool.collect(|| drop(program));
    assert!(pool.take(count).is_none());
    pool.collect(|| drop(retained));

    let (code, metadata) = pool.take(count).unwrap();
    assert!(code.is_empty() && metadata.is_empty());
    assert_eq!(code.as_ptr(), code_address);
    assert_eq!(metadata.as_ptr(), metadata_address);
}
