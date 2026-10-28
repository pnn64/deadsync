//! Reconstruct the library C stack before a return hook inspects or changes it.
use super::*;
mod libraries;

impl Vm {
    pub(in crate::vm) fn native_return_stack(
        &mut self,
        id: u64,
        frame: &mut Frame,
        source: usize,
        count: usize,
    ) -> usize {
        let kind = self.native_closures.get(id).map(|c| &c.kind);
        let iterator =
            matches!(kind, Some(threads::NativeKind::Match)) || id == host::stack::body::MATCH;
        let code = match kind {
            Some(threads::NativeKind::Builtin(code)) => *code,
            _ => id,
        };
        let lines =
            matches!(kind, Some(threads::NativeKind::Lines)) || id == host::stack::body::LINES;
        if let Some(stack) = self.package_return_stack(code, *frame, source, count) {
            return self.install_return_stack(frame, stack, count);
        }
        if let Some(stack) = self.library_return_stack(code, lines, *frame, source, count) {
            return self.install_return_stack(frame, stack, count);
        }
        let append = iterator
            || (math::MATH_FIRST..=math::MATH_LAST).contains(&code)
            || ((strings::STR_FIRST..=strings::STR_FORMAT).contains(&code)
                && code != strings::STR_GSUB)
            || matches!(
                code,
                2 | 3
                    | 4
                    | 8
                    | PAIRS
                    | IPAIRS
                    | IPAIRS_STEP
                    | TOSTRING
                    | LOAD_STRING
                    | COLLECT_GARBAGE
                    | GC_INFO
                    | LOAD_FILE
                    | DO_FILE
            );
        if !append
            && !matches!(
                code,
                0 | 5
                    | 6
                    | 7
                    | NEXT
                    | PAIRS_NEXT
                    | GET_META
                    | SET_META
                    | GET_ENV
                    | SET_ENV
                    | NEW_PROXY
                    | PRINT
                    | PCALL
                    | XPCALL
            )
        {
            return source;
        }
        let old = frame.vararg_start..frame.vararg_start + frame.argc();
        let mut stack = self.registers[old.clone()].to_vec();
        let results = &self.registers[source..source + count];
        match code {
            0 => {} // assert returns its existing arguments.
            7 if !matches!(stack.first(), Some(Value::String(s)) if self.strings.bytes[slot_index(s.0)].first() == Some(&b'#')) =>
                {}
            5 | NEXT | PAIRS_NEXT | NEW_PROXY => {
                stack.truncate(1);
                stack.resize(1, Value::Nil);
                stack.extend_from_slice(results);
            }
            6 | SET_META | PCALL | XPCALL | strings::STR_GMATCH => {
                stack.clear();
                stack.extend_from_slice(results);
            }
            GET_META => {
                let value = stack[0];
                if self.metafield(value, Meta::Protected) != Value::Nil
                    && let Some(mt) = self.metatable(value)
                {
                    stack.push(Value::Table(mt));
                }
                stack.extend_from_slice(results);
            }
            GET_ENV | SET_ENV => {
                let target = stack.first().copied().unwrap_or(Value::Nil);
                let function = match target {
                    f @ (Value::Function(_) | Value::Native(_)) => f,
                    _ => {
                        let level = if target == Value::Nil {
                            1
                        } else {
                            self.number(target).expect("validated environment level") as usize
                        };
                        if level == 0 {
                            self.builtin_context(id).unwrap_or(Value::Native(id))
                        } else {
                            let previous = self.active_native.replace(self.frames.len() - 1);
                            let function = self.stack_kind(level).expect("validated stack level");
                            self.active_native = previous;
                            match function {
                                FrameKind::Lua(id) => Value::Function(id),
                                FrameKind::Native(id) => Value::Native(id),
                            }
                        }
                    }
                };
                stack.push(function);
                if code == SET_ENV {
                    if count == 0 {
                        stack.push(Value::Thread(self.thread));
                    }
                } else {
                    stack.extend_from_slice(results);
                }
            }
            PRINT => stack.push(self.registers[frame.base + 1]),
            IPAIRS_STEP if count == 0 => {
                let next = (lua_integer(self.number(stack[1]).expect("validated iterator index"))
                    as i32)
                    .wrapping_add(1);
                stack.extend_from_slice(&[Value::Number(f64::from(next)), Value::Nil]);
            }
            _ => stack.extend_from_slice(results),
        }
        self.install_return_stack(frame, stack, count)
    }

    fn install_return_stack(
        &mut self,
        frame: &mut Frame,
        stack: Vec<Value>,
        count: usize,
    ) -> usize {
        let old = frame.vararg_start..frame.vararg_start + frame.argc();
        // The returned suffix must alias the values presented to the hook.
        // Moving the live C window also releases discarded arguments/scratch.
        let window = self.registers.len();
        let result = window + stack.len() - count;
        self.registers.extend_from_slice(&stack);
        self.relocate_native_error_functions(old.clone(), window);
        self.registers[old.start..old.end.min(window)].fill(Value::Nil);
        self.registers[frame.base..window].fill(Value::Nil);
        frame.base = window;
        frame.vararg_start = window;
        frame.vararg_count = stack.len() as u32;
        result
    }
}
