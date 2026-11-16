//! Lua 5.1 environment, files, time differences and process operations.
use super::*;
use crate::memory::fallible::SliceExt;
mod calendar;
#[allow(unsafe_code)]
pub(in crate::vm) mod environment;
#[allow(unsafe_code)]
mod files;
mod platform;
mod process;
#[allow(unsafe_code)]
mod temporary;

pub(super) const FIRST: u64 = 159;
const DIFFTIME: u64 = FIRST;
const GETENV: u64 = FIRST + 1;
const REMOVE: u64 = FIRST + 2;
const TMPNAME: u64 = FIRST + 3;
const EXECUTE: u64 = FIRST + 4;
const EXIT: u64 = FIRST + 5;
const CLOCK: u64 = FIRST + 6;
const RENAME: u64 = FIRST + 7;
pub(super) const TIME: u64 = FIRST + 8;
const DATE: u64 = FIRST + 9;
const SETLOCALE: u64 = FIRST + 10;
pub(super) const LAST: u64 = SETLOCALE;
impl Vm {
    pub(super) fn open_os(&mut self) -> Result<(), Error> {
        let table = self.try_new_table()?;
        for (name, id) in [
            (b"difftime".as_slice(), DIFFTIME),
            (b"getenv", GETENV),
            (b"remove", REMOVE),
            (b"tmpname", TMPNAME),
            (b"execute", EXECUTE),
            (b"exit", EXIT),
            (b"clock", CLOCK),
            (b"rename", RENAME),
            (b"time", TIME),
            (b"date", DATE),
            (b"setlocale", SETLOCALE),
        ] {
            let name = self.try_intern(name)?;
            self.raw_set(table, name, Value::Native(id))?;
        }
        self.set_global(b"os", Value::Table(table))?;
        Ok(())
    }

    pub(super) fn os_native(&mut self, id: u64, base: usize, count: usize) -> Result<usize, Error> {
        match id {
            SETLOCALE => {
                let name = if self.argument(base, count, 0) == Value::Nil {
                    None
                } else {
                    Some(self.string_argument(base, count, 0)?)
                };
                let category = if self.argument(base, count, 1) == Value::Nil {
                    0
                } else {
                    let id = self.string_argument(base, count, 1)?;
                    let bytes = &self.strings.bytes[slot_index(id.0)];
                    let bytes = &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())];
                    [
                        b"all".as_slice(),
                        b"collate",
                        b"ctype",
                        b"monetary",
                        b"numeric",
                        b"time",
                    ]
                    .iter()
                    .position(|&s| s == bytes)
                    .ok_or_else(|| self.argument_option_error(1, bytes))?
                };
                let value = crate::locale::set(
                    name.map(|id| self.strings.bytes[slot_index(id.0)].as_ref()),
                    category,
                )?;
                self.registers[base] = match value {
                    Some(name) => self.try_intern(&name)?,
                    None => Value::Nil,
                };
                return Ok(1);
            }
            DATE => return self.date_native(base, count),
            CLOCK => {
                #[cfg(any(
                    windows,
                    target_os = "linux",
                    target_os = "android",
                    target_vendor = "apple"
                ))]
                {
                    self.registers[base] = Value::Number(platform::clock());
                }
                #[cfg(not(any(
                    windows,
                    target_os = "linux",
                    target_os = "android",
                    target_vendor = "apple"
                )))]
                {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        0,
                        "os.clock is unsupported on this platform",
                    ));
                }
            }
            RENAME => {
                let from = self.string_argument(base, count, 0)?;
                let to = self.string_argument(base, count, 1)?;
                let old = &self.strings.bytes[slot_index(from.0)];
                let new = &self.strings.bytes[slot_index(to.0)];
                if let Err(code) = files::rename(io::c_bytes(old), io::c_bytes(new))? {
                    let name = io::c_bytes(old).try_to_vec()?;
                    return self.io_failure(base, code, Some(&name));
                }
                self.registers[base] = Value::Boolean(true);
            }
            EXECUTE => {
                let name = if self.argument(base, count, 0) == Value::Nil {
                    None
                } else {
                    Some(self.string_argument(base, count, 0)?)
                };
                let command = name.map(|name| io::c_bytes(&self.strings.bytes[slot_index(name.0)]));
                let status = process::execute(command)?.unwrap_or_else(|code| {
                    io::set_errno(code);
                    -1
                });
                self.registers[base] = Value::Number(f64::from(status));
            }
            EXIT => {
                let code = match self.argument(base, count, 0) {
                    Value::Nil => 0,
                    value => lua_integer(
                        self.number(value)
                            .ok_or_else(|| self.argument_type_error(base, count, 0, "number"))?,
                    ) as i32,
                };
                self.flush_process_streams();
                std::process::exit(code);
            }
            DIFFTIME => {
                // The pinned MSVC Lua build evaluates the second difftime
                // argument first, including its optional-number validation.
                let start = match self.argument(base, count, 1) {
                    Value::Nil => 0.0,
                    value => self
                        .number(value)
                        .ok_or_else(|| self.argument_type_error(base, count, 1, "number"))?,
                };
                let end = self
                    .number(self.argument(base, count, 0))
                    .ok_or_else(|| self.argument_type_error(base, count, 0, "number"))?;
                let (end, start) = (platform::timestamp(end), platform::timestamp(start));
                // The pinned MSVC CRT rejects negative time_t values, but accepts
                // zero. Subtract integers before rounding the difference to f64.
                #[cfg(windows)]
                let difference = if end < 0 || start < 0 {
                    io::set_errno(22);
                    0.0
                } else {
                    (end - start) as f64
                };
                #[cfg(not(windows))]
                let difference = (i128::from(end) - i128::from(start)) as f64;
                self.registers[base] = Value::Number(difference);
            }
            GETENV => {
                let key = self.string_argument(base, count, 0)?;
                let bytes = io::c_bytes(&self.strings.bytes[slot_index(key.0)]);
                // `=` is not part of a CRT environment-variable name.
                let value = if bytes.contains(&b'=') {
                    None
                } else {
                    environment::value(bytes)?
                };
                self.registers[base] = match value {
                    Some(value) => self.try_intern(&value)?,
                    None => Value::Nil,
                };
            }
            REMOVE => {
                let name = self.string_argument(base, count, 0)?;
                let bytes = &self.strings.bytes[slot_index(name.0)];
                if let Err(code) = files::remove(io::c_bytes(bytes))? {
                    // Keep the argument in its original slot until the error
                    // text has been built; io_failure may grow the string arena.
                    let name = io::c_bytes(bytes).try_to_vec()?;
                    return self.io_failure(base, code, Some(&name));
                }
                self.registers[base] = Value::Boolean(true);
            }
            TMPNAME => {
                let name = temporary::create()?;
                self.registers[base] = self.try_intern(name.bytes())?;
                name.publish();
            }
            _ => unreachable!(),
        }
        Ok(1)
    }
}
