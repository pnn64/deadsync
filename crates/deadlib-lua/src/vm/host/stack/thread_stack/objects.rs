//! Objects allocated or inspected through another thread's public C stack.
use super::*;

impl NativeThreadStack<'_> {
    /// Replace the last captures with a host closure in this thread's calling environment.
    /// Binding data is private; retained Lua values must occur in the captures.
    ///
    /// # Errors
    /// Rejects capture underflow, more than 255 captures or a full stack.
    pub fn push_closure<T: std::any::Any + Send + Sync>(
        &mut self,
        callback: HostFunction,
        captures: usize,
        data: T,
    ) -> Result<(), Error> {
        let prefix = self
            .top
            .checked_sub(captures)
            .ok_or_else(|| self.error(b"native capture stack underflow"))?;
        if prefix >= limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        let values = self.vm.threads[slot_index(self.thread.0)].registers
            [self.base + prefix..self.base + self.top]
            .to_vec();
        let env = self.vm.calling_environment(self.thread);
        let function = self
            .vm
            .new_host_function_with_data(callback, &values, data)?;
        self.vm.set_closure_environment(function, env);
        self.set_top(prefix)?;
        self.push(function)
    }
    /// Allocate and push aligned userdata in this thread's calling environment.
    ///
    /// # Errors
    /// Rejects payload allocation failure or a full stack.
    pub fn new_userdata(&mut self, size: usize) -> Result<(), Error> {
        if self.top == limits::MAX_CSTACK {
            return Err(self.error(b"native stack overflow"));
        }
        let env = self.vm.calling_environment(self.thread);
        let id = self.vm.try_allocate_userdata(size, env)?;
        self.push(Value::Userdata(id))
    }
    /// Push a function, userdata or thread environment, or nil for other types.
    ///
    /// # Errors
    /// Rejects a full stack.
    pub fn get_environment(&mut self, index: isize) -> Result<(), Error> {
        let env = match self.value(index) {
            Some(Value::Function(id)) => Some(self.vm.functions[slot_index(id.0)].env),
            Some(Value::Native(id)) => Some(self.vm.native_environment(id)),
            Some(Value::Userdata(id)) => Some(self.vm.userdata[slot_index(id.0)].env),
            Some(Value::Thread(id)) => Some(self.vm.thread_environment(id)),
            _ => None,
        };
        self.push(env.map_or(Value::Nil, Value::Table))
    }
    /// Pop a table and replace a function, userdata or thread environment.
    /// Other target types return false while still consuming the table.
    ///
    /// # Errors
    /// Rejects an absent target or a top value that is not a table.
    pub fn set_environment(&mut self, index: isize) -> Result<bool, Error> {
        let target = self
            .value(index)
            .ok_or_else(|| self.error(b"invalid native stack index"))?;
        let Some(Value::Table(env)) = self.value(-1) else {
            return Err(self.error(b"native environment must be a table"));
        };
        let supported = match target {
            Value::Function(_) | Value::Native(_) => {
                self.vm.set_closure_environment(target, env);
                true
            }
            Value::Userdata(id) => {
                self.vm.userdata[slot_index(id.0)].env = env;
                self.vm.gc_barrier(target, Value::Table(env));
                true
            }
            Value::Thread(id) => {
                self.vm.set_thread_environment(id, env);
                true
            }
            _ => false,
        };
        self.set_top(self.top - 1)?;
        Ok(supported)
    }
}
