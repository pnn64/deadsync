//! Library-specific stack effects that precede a native return hook.
use super::*;

impl Vm {
    pub(super) fn library_return_stack(
        &mut self,
        code: u64,
        lines: bool,
        frame: Frame,
        source: usize,
        count: usize,
    ) -> Result<Option<Buffer<Value>>, Error> {
        let supported = (tables::TAB_FIRST..=tables::TAB_SORT).contains(&code)
            || matches!(
                code,
                threads::CO_CREATE | threads::CO_RUNNING | threads::CO_STATUS | threads::CO_WRAP
            )
            || (os::FIRST..=os::LAST).contains(&code)
            || (io::FIRST..=io::LAST).contains(&code)
            || (debug::GET_INFO..=hooks::SET_HOOK).contains(&code)
            || matches!(
                code,
                DEBUG_GET_META
                    | DEBUG_SET_META
                    | DEBUG_GET_ENV
                    | DEBUG_SET_ENV
                    | debug::GET_REGISTRY
            )
            || lines;
        if !supported {
            return Ok(None);
        }
        // I/O close can retain six values, including its implicit file argument.
        let mut stack = self.return_scratch(
            &self.registers[frame.vararg_start..frame.vararg_start + frame.argc()],
            count + 6,
        )?;
        let results = self.return_scratch(&self.registers[source..source + count], 0)?;
        if self.io_return_stack(code, lines, frame, source, &mut stack, &results)? {
            return Ok(Some(stack));
        }
        match code {
            threads::CO_RUNNING if self.thread == self.main => {
                stack.push(Value::Thread(self.main));
                stack.extend_from_slice(&results);
            }
            tables::TAB_INSERT => {
                stack.pop();
            }
            tables::TAB_SORT => stack.resize(2, Value::Nil),
            tables::TAB_FOREACH if count != 0 => {
                stack.extend_from_slice(&self.registers[frame.base + 1..frame.base + 3]);
                stack.extend_from_slice(&results);
            }
            DEBUG_SET_ENV => {
                stack.truncate(1);
            }
            DEBUG_SET_META => {
                stack.truncate(1);
                stack.extend_from_slice(&results);
            }
            debug::GET_INFO => {
                let arg = usize::from(matches!(stack.first(), Some(Value::Thread(_))));
                if matches!(stack.get(arg), Some(Value::Function(_) | Value::Native(_))) {
                    let options = stack.get(arg + 1).copied().unwrap_or(Value::Nil);
                    let options = if let Value::String(id) = options {
                        io::c_bytes(&self.strings.bytes[slot_index(id.0)])
                    } else {
                        b"flnSu"
                    };
                    let mut text = Buffer::new_in(self.registers.memory().clone());
                    text.try_reserve_exact(options.len() + 1)
                        .map_err(|_| self.memory_failure())?;
                    text.push(b'>');
                    text.extend_from_slice(options);
                    stack.push(self.try_intern(&text)?);
                }
                stack.extend_from_slice(&results);
            }
            debug::GET_LOCAL => {
                if count == 2 {
                    stack.push(results[1]);
                }
                stack.extend_from_slice(&results);
            }
            debug::SET_LOCAL => {
                let arg = frame.pc as usize;
                stack.truncate(arg + 2);
                stack.extend_from_slice(&results);
            }
            debug::SET_UPVALUE if count != 0 => {
                stack.pop();
                stack.extend_from_slice(&results);
            }
            hooks::SET_HOOK => {
                let arg = usize::from(matches!(stack.first(), Some(Value::Thread(_))));
                if stack.get(arg).is_none_or(|v| *v == Value::Nil) {
                    stack.resize(arg + 1, Value::Nil);
                }
            }
            // Traceback already maintains its complete, concatenated C window.
            debug::TRACEBACK => {}
            _ => stack.extend_from_slice(&results),
        }
        Ok(Some(stack))
    }
}
