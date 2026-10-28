//! I/O callbacks that invoke a C body in their existing activation.
use super::*;

fn stack<'call, 'vm>(call: &'call mut HostCall<'vm>) -> Result<NativeStack<'call, 'vm>, Error> {
    let stack = call.native_stack()?;
    stack.call.vm.frames[stack.frame].pc = crate::vm::io::STACK_READY;
    Ok(stack)
}
fn close_handler(stack: &mut NativeStack<'_, '_>) -> Result<Option<usize>, Error> {
    stack.get_environment(1)?;
    stack.get_field(-1, b"__close")?;
    stack.call_close_body(stack.value(-1).unwrap_or(Value::Nil))
}
pub(in crate::vm) fn close(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = stack(call)?;
    if stack.top() == 0 {
        let env = stack.context_environment();
        let value = stack.call.vm.tables[slot_index(env.0)].get(Value::Number(2.));
        stack.push(value)?;
    }
    stack
        .call
        .vm
        .checked_file(stack.value(1).unwrap_or(Value::Nil), true)?;
    match close_handler(&mut stack)? {
        Some(count) => stack.finish(count),
        None => Ok(()),
    }
}
pub(in crate::vm) fn finalize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = stack(call)?;
    let value = stack.value(1).unwrap_or(Value::Nil);
    let key = stack.call.vm.file_key(value, true).ok_or_else(|| {
        stack.argument_error(
            1,
            format!(
                "FILE* expected, got {}",
                if stack.top() == 0 {
                    "no value"
                } else {
                    value.type_name()
                }
            )
            .as_bytes(),
        )
    })?;
    if stack.call.vm.io.files[slot_index(key)].resource.is_some()
        && close_handler(&mut stack)?.is_none()
    {
        return Ok(());
    }
    stack.finish(0)
}
pub(in crate::vm) fn readline(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = stack(call)?;
    stack.call.vm.ensure_file_backend()?;
    let file = stack.value(-10003).unwrap_or(Value::Nil);
    let close = stack.value(-10004).unwrap_or(Value::Nil).is_truthy();
    let key = stack
        .call
        .vm
        .file_key(file, false)
        .ok_or_else(|| stack.error(b"file is already closed"))?;
    let slot = slot_index(key);
    if stack.call.vm.io.files[slot].resource.is_none() {
        return Err(stack.error(b"file is already closed"));
    }
    let vm = &mut stack.call.vm;
    let mut bytes = vm.take_buffer(128);
    let before = vm.io.files[slot].heap_bytes();
    let result = vm.io.files[slot].read_line(&mut bytes);
    vm.io
        .files
        .note_alloc(vm.io.files[slot].heap_bytes().saturating_sub(before));
    let success = match result {
        Ok(success) => success,
        Err(code) => {
            vm.buffers.push(bytes);
            crate::vm::io::set_errno(code);
            return Err(Error::from_bytes(
                ErrorKind::Runtime,
                0,
                crate::vm::io::message(code).into_owned(),
            ));
        }
    };
    if success {
        let line = stack.call.vm.finish_buffer(bytes);
        stack.push(line)?;
        return stack.finish(1);
    }
    stack.call.vm.buffers.push(bytes);
    if close {
        stack.set_top(0)?;
        stack.push(file)?;
        if close_handler(&mut stack)?.is_none() {
            return Ok(());
        }
    } else {
        let empty = stack.intern(b"");
        stack.push(empty)?;
    }
    stack.finish(0)
}
