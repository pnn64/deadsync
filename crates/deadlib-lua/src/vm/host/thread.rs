//! Resources whose lifetime follows one Lua thread, rather than one callback.
use super::*;
use std::any::Any;

/// Host-owned state attached once to one Lua thread.
///
/// Each thread, including the main thread, has one independent slot per type.
/// State is not inherited by new coroutines. Retained Lua values must be listed
/// in [`Self::roots`]; returned roots must stay unchanged after initialization.
/// Resources drop when their thread is collected, or after Lua shutdown
/// finalizers when the VM is destroyed. Drop must not reenter Lua or panic.
pub trait HostThreadState: Any + Send + Sync {
    /// Lua values strongly retained by this state.
    fn roots(&self) -> &[Value] {
        &[]
    }
    /// Owned allocation bytes beyond the state object's own size.
    fn heap_bytes(&self) -> usize {
        0
    }
}

pub(in crate::vm) struct ThreadData(Box<dyn HostThreadState>);
impl std::fmt::Debug for ThreadData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostThreadState").finish_non_exhaustive()
    }
}
impl ThreadData {
    pub(in crate::vm) fn roots(&self) -> &[Value] {
        self.0.roots()
    }
    pub(in crate::vm) fn heap_bytes(&self) -> usize {
        std::mem::size_of_val(self.0.as_ref()).saturating_add(self.0.heap_bytes())
    }
    pub(super) fn get<T: HostThreadState>(&self) -> Option<&T> {
        let state: &dyn Any = self.0.as_ref();
        state.downcast_ref()
    }
}

impl Vm {
    pub(in crate::vm) fn initialize_thread_state<T: HostThreadState>(
        &mut self,
        state: T,
    ) -> Result<(), Error> {
        self.initialize_thread_state_for(self.thread, state)
    }
    pub(in crate::vm) fn initialize_thread_state_for<T: HostThreadState>(
        &mut self,
        thread: ThreadId,
        state: T,
    ) -> Result<(), Error> {
        if !self.threads.contains(thread.0) {
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                "invalid or collected Lua thread",
            ));
        }
        let index = slot_index(thread.0);
        if self.threads[index]
            .host_states
            .iter()
            .any(|s| s.get::<T>().is_some())
        {
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                "host thread state is already initialized",
            ));
        }
        for &value in state.roots() {
            self.validate_value(value)?;
        }
        // Barrier each edge even if this thread has already been traversed.
        for &value in state.roots() {
            self.gc_new_value(value);
        }
        let data = ThreadData(Box::new(state));
        let bytes = data.heap_bytes();
        let states = &mut self.threads[index].host_states;
        let before = states.capacity();
        states.push(data);
        let bytes =
            bytes.saturating_add((states.capacity() - before) * std::mem::size_of::<ThreadData>());
        self.gc_note_control(bytes);
        Ok(())
    }
}

impl HostCall<'_> {
    /// Identity of the currently executing Lua thread, including the main one.
    pub fn thread_id(&self) -> ThreadId {
        self.vm.thread
    }
    /// Borrow this thread's host state when its concrete type matches `T`.
    /// The borrow ends before a nested Lua call can change the active thread.
    pub fn thread_state<T: HostThreadState>(&self) -> Option<&T> {
        self.vm.threads[slot_index(self.vm.thread.0)]
            .host_states
            .iter()
            .find_map(ThreadData::get)
    }
    /// Initialize this thread's state, validating and tracing its Lua roots.
    ///
    /// # Errors
    /// Rejects an occupied type slot or invalid/collected root values. Rejected state
    /// is dropped normally; existing state is never replaced.
    pub fn initialize_thread_state<T: HostThreadState>(&mut self, state: T) -> Result<(), Error> {
        self.vm.initialize_thread_state(state)
    }
}
