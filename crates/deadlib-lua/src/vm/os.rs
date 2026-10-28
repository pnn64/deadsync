//! Lua 5.1 environment, files, time differences and process operations.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
mod calendar;
mod platform;
mod process;

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
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn temporary_name() -> std::io::Result<std::path::PathBuf> {
    #[cfg(windows)]
    let root = std::env::temp_dir();
    // Lua 5.1's POSIX template uses /tmp and an extensionless basename.
    // In particular, require(os.tmpname()) with LUA_PATH="?" must not turn
    // an added filename extension into a module directory separator.
    #[cfg(not(windows))]
    let root = std::path::Path::new("/tmp");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for _ in 0..64 {
        let nonce = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let suffix = if cfg!(windows) { ".tmp" } else { "" };
        let path = root.join(format!(
            "deadsync-lua-{}-{stamp}-{nonce}{suffix}",
            std::process::id()
        ));
        #[cfg(windows)]
        if !path.try_exists()? {
            // Like the reference's tmpnam, this returns an unused name without
            // creating a file. The caller must open it before another process can.
            return Ok(path);
        }
        #[cfg(not(windows))]
        {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => {
                    drop(file);
                    return Ok(path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }
    Err(std::io::ErrorKind::AlreadyExists.into())
}

impl Vm {
    pub(super) fn open_os(&mut self) {
        let table = self.new_table();
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
            let name = self.intern(name);
            self.raw_set(table, name, Value::Native(id)).unwrap();
        }
        self.set_global(b"os", Value::Table(table)).unwrap();
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
                self.registers[base] = value.map_or(Value::Nil, |s| self.intern(&s));
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
                if let Err(code) = platform::rename(&io::path(old), &io::path(new)) {
                    let name = io::c_bytes(old).to_vec();
                    return Ok(self.io_failure(base, code, Some(&name)));
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
                let status = process::execute(command).unwrap_or_else(|code| {
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
                    #[cfg(unix)]
                    let key = {
                        use std::os::unix::ffi::OsStrExt;
                        std::ffi::OsStr::from_bytes(bytes)
                    };
                    #[cfg(not(unix))]
                    let key = String::from_utf8_lossy(bytes);
                    #[cfg(not(unix))]
                    let key = std::ffi::OsStr::new(key.as_ref());
                    std::env::var_os(key)
                };
                self.registers[base] = match value {
                    Some(value) => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::ffi::OsStrExt;
                            self.intern(value.as_bytes())
                        }
                        #[cfg(not(unix))]
                        {
                            self.intern(value.to_string_lossy().as_bytes())
                        }
                    }
                    None => Value::Nil,
                };
            }
            REMOVE => {
                let name = self.string_argument(base, count, 0)?;
                let bytes = &self.strings.bytes[slot_index(name.0)];
                let path = io::path(bytes);
                let result = std::fs::remove_file(&path);
                #[cfg(unix)]
                let result = match result {
                    Err(error) if error.kind() == std::io::ErrorKind::IsADirectory => {
                        std::fs::remove_dir(path)
                    }
                    result => result,
                };
                if let Err(error) = result {
                    let code = io::errno(error);
                    // Keep the argument in its original slot until the error
                    // text has been built; io_failure may grow the string arena.
                    let name = io::c_bytes(bytes).to_vec();
                    return Ok(self.io_failure(base, code, Some(&name)));
                }
                self.registers[base] = Value::Boolean(true);
            }
            TMPNAME => {
                let path = temporary_name().map_err(|_| {
                    Error::new(
                        ErrorKind::Runtime,
                        0,
                        "unable to generate a unique filename",
                    )
                })?;
                #[cfg(unix)]
                {
                    use std::os::unix::ffi::OsStrExt;
                    self.registers[base] = self.intern(path.as_os_str().as_bytes());
                }
                #[cfg(not(unix))]
                {
                    self.registers[base] = self.intern(path.to_string_lossy().as_bytes());
                }
            }
            _ => unreachable!(),
        }
        Ok(1)
    }
}
