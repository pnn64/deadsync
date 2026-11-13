//! A scoped budget for the compiler's ordinary Rust allocations.
//!
//! Allocation ownership remains with Vec/Box. Workspace charges are retained
//! until the compilation job ends, including across reader suspensions. This
//! conservatively bounds peak workspace without replacing the global allocator.
//! Scope activation is limited to synchronous compiler work, never Lua readers
//! or other host callbacks. Nested activations restore the previous budget.
use super::{Memory, Reservation, Shared};
use crate::Error;
use std::{cell::RefCell, sync::Mutex};

type Account = Shared<Mutex<Reservation>>;
thread_local! {
    static ACTIVE: RefCell<Option<Account>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Workspace(Option<Account>);

impl Workspace {
    pub(crate) fn new(memory: &Memory) -> Result<Self, Error> {
        if memory.budget_usage().is_none() {
            return Ok(Self::default());
        }
        let reservation = memory.reserve_external(|| 0)?;
        Ok(Self(Some(Shared::try_new_in(
            Mutex::new(reservation),
            memory.clone(),
        )?)))
    }

    pub(crate) fn run<T>(&self, work: impl FnOnce() -> T) -> T {
        struct Restore(Option<Account>);
        impl Drop for Restore {
            fn drop(&mut self) {
                let active = ACTIVE.with(|slot| slot.replace(self.0.take()));
                drop(active);
            }
        }
        let _restore = Restore(ACTIVE.with(|slot| slot.replace(self.0.clone())));
        work()
    }
}

pub(crate) fn active() -> bool {
    ACTIVE.with(|slot| slot.borrow().is_some())
}

pub(super) fn charge(bytes: usize) -> Result<(), Error> {
    if bytes == 0 {
        return Ok(());
    }
    ACTIVE.with(|slot| {
        if let Some(account) = slot.borrow().as_ref() {
            account
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .grow(bytes)?;
        }
        Ok(())
    })
}
