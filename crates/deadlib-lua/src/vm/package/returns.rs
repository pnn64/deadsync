//! Package routines retain Lua-visible callback values independently of scheduler slots.
use super::*;

impl Vm {
    /// Map Lua-visible temporaries while a package operation awaits a callback.
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
            LUA if matches!(frame.pc, 1..=3) => {
                let size = match frame.pc {
                    1 => 1,
                    2 => 3,
                    _ => 4,
                };
                return Some(if n < arguments {
                    Some(frame.vararg_start + n)
                } else if n < arguments + size {
                    let at = n - arguments;
                    Some(
                        frame.base
                            + match at {
                                0 => 1,
                                1 => 2,
                                2 => 3,
                                _ => 5,
                            },
                    )
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
    ) -> Result<Option<Buffer<Value>>, Error> {
        if !(REQUIRE..=LUA).contains(&code) {
            return Ok(None);
        }
        let base = frame.base;
        // Reserve the largest retained package window plus its result suffix.
        let mut stack = self.return_scratch(
            &self.registers[frame.vararg_start..frame.vararg_start + frame.argc()],
            count + 5,
        )?;
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
            LUA if count != 0 => {
                stack.extend_from_slice(&self.registers[base + 1..base + 4]);
                if frame.pc == 3 {
                    stack.push(self.registers[base + 5]);
                    stack.extend_from_slice(results);
                }
            }
            _ => {}
        }
        Ok(Some(stack))
    }
}
