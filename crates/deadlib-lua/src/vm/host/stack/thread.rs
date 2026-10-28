//! Thread identities, execution statuses and lazily attached host resources.
use super::*;

impl NativeStack<'_, '_> {
    #[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
    pub(crate) fn main_thread_id(&self) -> ThreadId {
        self.call.vm.main
    }
    /// Identity of the active Lua thread, including the main thread.
    pub fn thread_id(&self) -> ThreadId {
        self.call.vm.thread
    }
    /// Convert an ordinary or pseudo-indexed thread value; other types return None.
    pub fn to_thread(&self, index: isize) -> Option<ThreadId> {
        match self.value(index) {
            Some(Value::Thread(id)) => Some(id),
            _ => None,
        }
    }
    /// Push the active thread and return whether it is the main thread.
    ///
    /// # Errors
    /// Rejects a pending unprotected error or a full stack.
    pub fn push_thread(&mut self) -> Result<bool, Error> {
        let thread = self.call.vm.thread;
        self.push(Value::Thread(thread))?;
        Ok(thread == self.call.vm.main)
    }
    /// Read a live thread's retained execution status. Unlike coroutine.status,
    /// fresh and completed threads both return Ok; only a yielded thread returns
    /// Yielded. Failed children retain RuntimeError or ErrorHandler; quota-aborted
    /// children return RuntimeError.
    /// Collected/invalid identities return None. The main thread remains Ok.
    pub fn thread_status(&self, thread: ThreadId) -> Option<NativeThreadStatus> {
        self.call
            .vm
            .threads
            .get(thread.0)
            .map(|state| state.resume_status)
    }
    /// Borrow typed state belonging to any live thread in this VM.
    /// This resource lookup does not activate or retain the target thread.
    pub fn thread_state<T: HostThreadState>(&self, thread: ThreadId) -> Option<&T> {
        self.call
            .vm
            .threads
            .get(thread.0)?
            .host_states
            .iter()
            .find_map(ThreadData::get)
    }
    /// Attach typed state to a live thread, including a never-resumed coroutine.
    /// Roots are validated and traced; the resource follows that thread's lifetime.
    ///
    /// # Errors
    /// Rejects pending errors, collected threads, occupied type slots or invalid
    /// Lua roots. Rejected state drops normally without replacing existing state.
    pub fn initialize_thread_state<T: HostThreadState>(
        &mut self,
        thread: ThreadId,
        state: T,
    ) -> Result<(), Error> {
        self.check_error()?;
        self.call.vm.initialize_thread_state_for(thread, state)
    }
}
