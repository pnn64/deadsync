//! Package routines retain their Lua C stack independently of scheduler slots.
use super::*;

impl Vm {
    /// Map live C temporaries while a package operation awaits a callback.
    pub(in crate::vm) fn package_local_slot(
        &self,
        code: u64,
        frame: Frame,
        n: usize,
    ) -> Option<Option<usize>> {
        let arguments = frame.argc();
        let extra = match code {
            MODULE => match frame.pc {
                2 => 1,
                7 => 2,
                8..=10 | 12 => 3,
                _ => return None,
            },
            PRELOAD if frame.pc == 2 => 1,
            LUA | C | CROOT if matches!(frame.pc, 1..=3) => {
                let root = usize::from(code == CROOT);
                let size = match frame.pc {
                    1 => 1,
                    2 => 3,
                    _ => 4,
                };
                return Some(if n < arguments {
                    Some(frame.vararg_start + n)
                } else if n < arguments + root + size {
                    let at = n - arguments;
                    Some(if root != 0 && at == 0 {
                        frame.base + 10
                    } else {
                        frame.base
                            + match at - root {
                                0 => 1,
                                1 => 2,
                                2 => 3,
                                _ => 5,
                            }
                    })
                } else {
                    None
                });
            }
            _ => return None,
        };
        Some(if n < arguments {
            Some(frame.vararg_start + n)
        } else if n < arguments + extra {
            Some(if code == MODULE {
                frame.base + n
            } else {
                frame.base + 1
            })
        } else {
            None
        })
    }

    pub(in crate::vm) fn package_return_stack(
        &self,
        code: u64,
        frame: Frame,
        source: usize,
        count: usize,
    ) -> Option<Vec<Value>> {
        if frame.pc >= 100 && matches!(code, LOADLIB | C | CROOT) {
            return Some(
                self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].to_vec(),
            );
        }
        if code == LIB_GC {
            let stack =
                self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].to_vec();
            return Some(stack);
        }
        if !(REQUIRE..=CROOT).contains(&code) {
            return None;
        }
        let base = frame.base;
        let mut stack =
            self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].to_vec();
        let results = &self.registers[source..source + count];
        match code {
            REQUIRE | SEEALL => {} // The driver maintains the complete live C window.
            MODULE => {
                stack.extend_from_slice(
                    &self.registers[base + frame.argc()..base + frame.argc() + 3],
                );
            }
            PRELOAD => {
                stack.push(self.registers[base + 1]);
                if self.registers[base + 3] == Value::Nil {
                    stack.push(Value::Nil);
                }
                stack.extend_from_slice(results);
            }
            LUA | C | CROOT if count != 0 => {
                if code == CROOT {
                    stack.push(self.registers[base + 10]);
                }
                stack.extend_from_slice(&self.registers[base + 1..base + 4]);
                if frame.pc == 3 {
                    stack.push(self.registers[base + 5]);
                    stack.extend_from_slice(results);
                }
            }
            _ => {}
        }
        Some(stack)
    }
}
