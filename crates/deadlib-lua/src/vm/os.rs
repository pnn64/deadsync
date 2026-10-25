//! Lua 5.1 environment, files, time differences and process operations.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
mod process;

pub(super) const FIRST: u64 = 159;
const DIFFTIME: u64 = FIRST;
const GETENV: u64 = FIRST + 1;
const REMOVE: u64 = FIRST + 2;
const TMPNAME: u64 = FIRST + 3;
const EXECUTE: u64 = FIRST + 4;
const EXIT: u64 = FIRST + 5;
pub(super) const LAST: u64 = EXIT;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn temporary_name() -> std::io::Result<std::path::PathBuf> {
    let root = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for _ in 0..64 {
        let nonce = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!(
            "deadsync-lua-{}-{stamp}-{nonce}.tmp",
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
        ] {
            let name = self.intern(name);
            self.raw_set(table, name, Value::Native(id)).unwrap();
        }
        self.set_global(b"os", Value::Table(table)).unwrap();
    }

    pub(super) fn os_native(&mut self, id: u64, base: usize, count: usize) -> Result<usize, Error> {
        match id {
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
                    value => io::integer(
                        self.number(value)
                            .ok_or_else(|| self.argument_type_error(base, count, 0, "number"))?,
                    ) as i32,
                };
                // Process termination does not close the Lua state or run its
                // finalizers. Flush owned streams before the OS closes handles.
                for stream in self.io.files.iter_mut() {
                    let _ = stream.flush();
                }
                use std::io::Write;
                let _ = io::stdout_flush();
                let _ = std::io::stderr().flush();
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
                let (end, start) = (io::integer(end), io::integer(start));
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
