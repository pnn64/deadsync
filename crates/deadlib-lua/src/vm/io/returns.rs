//! Actual C-library temporaries retained at return.
use super::*;

impl Vm {
    pub(in crate::vm) fn io_return_stack(
        &mut self,
        code: u64,
        lines: bool,
        frame: Frame,
        source: usize,
        stack: &mut Vec<Value>,
        results: &[Value],
    ) -> bool {
        if !(FIRST..=LAST).contains(&code) && !lines {
            return false;
        }
        if frame.pc == STACK_READY || frame.pc == threads::HOST_YIELDED {
            return true;
        }
        if lines {
            if results.is_empty() {
                let captures =
                    &self.native_closures[slot_index(self.builtin_closure(code))].captures;
                let file = captures[0];
                if captures[1].is_truthy() {
                    stack.clear();
                    stack.push(file);
                    let closer = self.io_close_temporaries(file, stack);
                    let count = if self.registers[source] != Value::Nil {
                        1
                    } else if closer == Value::Native(NO_CLOSE) {
                        2
                    } else {
                        3
                    };
                    stack.extend_from_slice(&self.registers[source..source + count]);
                } else {
                    stack.push(self.intern(b""));
                }
            } else {
                stack.extend_from_slice(results);
            }
            return true;
        }
        match code {
            TYPE => {
                let expected = self.file_metatable().map_or(Value::Nil, Value::Table);
                stack.push(expected);
                let pointer = matches!(stack.first(), Some(Value::Userdata(_)))
                    || matches!(stack.first(), Some(Value::LightUserdata(p)) if *p != 0);
                if pointer && let Some(mt) = self.metatable(stack[0]) {
                    stack.push(Value::Table(mt));
                }
            }
            OPEN_FILE | POPEN | TMPFILE if results.len() == 3 => {
                stack.push(self.registers[source + frame.argc().max(3) + 1]);
            }
            READ | WRITE | FLUSH => {
                let file = self
                    .default_file(code, code != READ)
                    .expect("validated default file");
                stack.push(file);
            }
            LINES => {
                let Value::Native(id) = results[0] else {
                    unreachable!("line iterator")
                };
                stack.push(self.native_closures[slot_index(id)].captures[0]);
            }
            CLOSE | FCLOSE => {
                if stack.is_empty() {
                    stack.push(
                        self.tables[slot_index(self.builtin_environment(code).0)]
                            .get(Value::Number(2.)),
                    );
                }
                self.io_close_temporaries(stack[0], stack);
            }
            GC if matches!(self.registers[source], Value::Boolean(_) | Value::Nil) => {
                let closer = self.io_close_temporaries(stack[0], stack);
                let count = if self.registers[source] != Value::Nil {
                    1
                } else if closer == Value::Native(NO_CLOSE) {
                    2
                } else {
                    3
                };
                stack.extend_from_slice(&self.registers[source..source + count]);
            }
            _ => {}
        }
        stack.extend_from_slice(results);
        true
    }
    fn io_close_temporaries(&self, file: Value, stack: &mut Vec<Value>) -> Value {
        let Value::Userdata(id) = file else {
            unreachable!("validated file")
        };
        let env = self.userdata[slot_index(id.0)].env;
        let closer = self.tables[slot_index(env.0)].get(Value::String(
            self.strings.find(b"__close").expect("file closer name"),
        ));
        stack.extend_from_slice(&[Value::Table(env), closer]);
        closer
    }
}
