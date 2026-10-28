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
    ) -> Option<Vec<Value>> {
        let supported = (tables::TAB_FIRST..=tables::TAB_SORT).contains(&code)
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
            return None;
        }
        let mut stack =
            self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].to_vec();
        let results = self.registers[source..source + count].to_vec();
        if self.io_return_stack(code, lines, frame, source, &mut stack, &results) {
            return Some(stack);
        }
        match code {
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
                    let mut text = vec![b'>'];
                    if let Value::String(id) = options {
                        text.extend_from_slice(io::c_bytes(&self.strings.bytes[slot_index(id.0)]));
                    } else {
                        text.extend_from_slice(b"flnSu");
                    }
                    stack.push(self.intern(&text));
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
        Some(stack)
    }
}
