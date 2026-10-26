//! Protected native calls use the shared error handler and child-call driver.
use super::*;

/// Lua 5.1 execution statuses returned by [`NativeStack::protected_call`].
/// Instruction/resource limits still escape as Rust errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NativeCallStatus {
    /// Call completed normally.
    Ok = 0,
    /// Call failed; the error value is on the stack.
    RuntimeError = 2,
    /// The error handler failed; its diagnostic is on the stack.
    ErrorHandler = 5,
}

impl NativeStack<'_, '_> {
    /// Invoke the function below the last `arguments` values, as Lua 5.1 pcall.
    /// Results replace function and arguments. `None` keeps all results. An
    /// error always leaves exactly one error value, regardless of `results`.
    ///
    /// An optional ordinary stack index selects the error function before the
    /// call. It runs while faulting frames and open locals are still present.
    /// The handler is ignored on success; on error it must be a function.
    ///
    /// # Errors
    /// Rejects invalid stack/count inputs, pending unprotected errors or fatal
    /// instruction/resource limits. Runtime failures instead return a status
    /// and leave the native callback usable.
    pub fn protected_call(
        &mut self,
        arguments: usize,
        results: Option<usize>,
        error_function: Option<isize>,
    ) -> Result<NativeCallStatus, Error> {
        self.check_error()?;
        let prefix = self
            .top()
            .checked_sub(
                arguments
                    .checked_add(1)
                    .ok_or_else(|| self.error(b"invalid native call count"))?,
            )
            .ok_or_else(|| self.error(b"native call stack underflow"))?;
        if results.is_some_and(|n| n > limits::MAX_CSTACK - prefix) {
            return Err(self.error(b"native stack overflow"));
        }
        let handler = error_function
            .map(|index| {
                self.position(index)
                    .ok_or_else(|| self.error(b"invalid native error function index"))
            })
            .transpose()?;
        let function = self.call.vm.registers[self.base + prefix];
        let values =
            self.call.vm.registers[self.base + prefix + 1..self.base + self.top()].to_vec();
        self.call.results = prefix;
        self.sync();
        let outcome = self
            .call
            .run_action(Action::ProtectedCall(function, &values, handler), true);
        match outcome {
            Ok(range) => {
                let status = match self.call.vm.registers[range.start] {
                    Value::Number(0.) => NativeCallStatus::Ok,
                    Value::Number(2.) => NativeCallStatus::RuntimeError,
                    Value::Number(5.) => NativeCallStatus::ErrorHandler,
                    _ => unreachable!("protected native status"),
                };
                let actual = range.len() - 1;
                let count = if status == NativeCallStatus::Ok {
                    results.unwrap_or(actual)
                } else {
                    1
                };
                if count > limits::MAX_CSTACK - prefix {
                    self.call.vm.registers.truncate(self.base + prefix);
                    return Err(self.error(b"native stack overflow"));
                }
                self.call.vm.relocate_native_error_functions(
                    range.start + 1..range.end,
                    self.base + prefix,
                );
                self.call
                    .vm
                    .registers
                    .copy_within(range.start + 1..range.end, self.base + prefix);
                self.call.vm.resize_registers(self.base + prefix + count);
                if count > actual {
                    self.call.vm.registers[self.base + prefix + actual..self.base + prefix + count]
                        .fill(Value::Nil);
                }
                self.call.results = prefix + count;
                self.sync();
                Ok(status)
            }
            Err(error) => {
                self.call.results = self.call.vm.registers.len() - self.base;
                self.sync();
                Err(error)
            }
        }
    }
}
