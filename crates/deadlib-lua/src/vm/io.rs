//! Lua 5.1 file userdata, shared I/O environments and native stream operations.
use super::*;
use crate::memory::fallible::{self, SliceExt, VecExt};
use std::{cell::Cell as ErrnoCell, num::NonZeroU64};
#[allow(unsafe_code)]
pub(in crate::vm) mod input;
mod number;
mod output;
#[allow(unsafe_code)]
mod process;
mod returns;
#[allow(unsafe_code)]
mod stdio;
mod stream;
#[cfg(test)]
mod write_tests;
use number::read_number;
pub(super) fn stdout_write(bytes: &[u8]) -> std::io::Result<()> {
    output::write(bytes)
}
pub(super) use output::flush as stdout_flush;
pub(super) fn stderr_write(bytes: &[u8]) {
    if cfg!(windows) {
        for part in bytes.split_inclusive(|&b| b == b'\n') {
            if let Some(line) = part.strip_suffix(b"\n") {
                let _ = stdio::write_all(2, line);
                let _ = stdio::write_all(2, b"\r\n");
            } else {
                let _ = stdio::write_all(2, part);
            }
        }
    } else {
        let _ = stdio::write_all(2, bytes);
    }
}
pub(super) use stream::LINE_BUFFER;
use stream::Resource;
pub(super) use stream::Stream;
pub(super) use stream::errno;

pub(super) const FIRST: u64 = 128;
const CLOSE: u64 = 128;
const FLUSH: u64 = 129;
const INPUT: u64 = 130;
const LINES: u64 = 131;
const OPEN_FILE: u64 = 132;
const OUTPUT: u64 = 133;
const POPEN: u64 = 134;
const READ: u64 = 135;
pub(super) const TMPFILE: u64 = 136;
const TYPE: u64 = 137;
const WRITE: u64 = 138;
const FCLOSE: u64 = 139;
const FFLUSH: u64 = 140;
const FLINES: u64 = 141;
const FREAD: u64 = 142;
const SEEK: u64 = 143;
const SETVBUF: u64 = 144;
const FWRITE: u64 = 145;
const GC: u64 = 146;
const TO_STRING: u64 = 147;
const REAL_CLOSE: u64 = 148;
const NO_CLOSE: u64 = 149;
const PIPE_CLOSE: u64 = 150;
pub(super) const LAST: u64 = PIPE_CLOSE;
pub(super) const STACK_READY: u32 = 2;
pub(super) fn callback(id: u64) -> Option<HostFunction> {
    match id {
        CLOSE | FCLOSE => Some(host::stack::io_callbacks::close),
        GC => Some(host::stack::io_callbacks::finalize),
        host::stack::body::LINES => Some(host::stack::io_callbacks::readline),
        _ => None,
    }
}
thread_local! { static ERRNO:ErrnoCell<i32>=const {ErrnoCell::new(0)}; }

#[derive(Debug, Default)]
pub(super) struct Io {
    pub files: Arena<Stream>,
    pub file_tag: Option<StringId>,
    pub stdin: Option<NonZeroU64>,
    pub stdout: Option<NonZeroU64>,
    pub stderr: Option<NonZeroU64>,
}
impl Io {
    pub fn heap_bytes(&self) -> usize {
        self.files.iter().map(Stream::heap_bytes).sum()
    }
    pub(in crate::vm) fn standard_stream(&mut self, kind: usize) -> NonZeroU64 {
        let (slot, resource, read, write) = match kind {
            0 => (&mut self.stdin, Resource::Input, true, false),
            1 => (&mut self.stdout, Resource::Output, false, true),
            2 => (&mut self.stderr, Resource::Error, false, true),
            _ => unreachable!("standard stream kind"),
        };
        *slot.get_or_insert_with(|| {
            NonZeroU64::new(self.files.insert(Stream::new(resource, read, write, true)))
                .expect("stream arena key")
        })
    }
    pub(in crate::vm) fn write_standard(&mut self, kind: usize, bytes: &[u8]) -> Result<(), i32> {
        if kind == 1 {
            stdout_write(bytes).map_err(errno)
        } else {
            stderr_write(bytes);
            Ok(())
        }
    }
    pub(in crate::vm) fn write_print(&mut self, bytes: &[u8]) -> Result<(), Error> {
        // Lua print ignores ordinary output-device errors. Allocation failure
        // in our Rust buffer must still reach the protected memory boundary.
        match self.write_standard(1, bytes) {
            Err(12) => Err(Error::memory()),
            _ => Ok(()),
        }
    }
    pub(in crate::vm) fn flush_stdout(&mut self) -> Result<(), i32> {
        stdout_flush().map_err(errno)
    }
    fn is_stdout(&self, key: u64) -> bool {
        matches!(self.files[slot_index(key)].resource, Some(Resource::Output))
    }
}
impl Vm {
    fn open_stream(
        &mut self,
        path: Option<&[u8]>,
        mode: &[u8],
        pipe_file: bool,
    ) -> Result<Result<Stream, i32>, Error> {
        match path {
            None => input::temporary().map(|result| {
                result.map(|file| Stream::new(Resource::File(file), true, true, false))
            }),
            Some(path) if pipe_file => pipe(path, mode),
            Some(path) => open(path, mode),
        }
    }
    pub(super) fn standard_stream(&mut self, kind: usize) -> NonZeroU64 {
        self.io.standard_stream(kind)
    }
    pub(super) fn flush_process_streams(&mut self) {
        // Process exit flushes C streams without closing the Lua state or
        // invoking finalizers. Match that for the streams owned by this VM.
        for stream in self.io.files.iter_mut() {
            let _ = stream.flush();
        }
        let _ = self.io.flush_stdout();
    }
}
pub(super) fn c_bytes(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}
pub(super) fn integer(n: f64) -> i64 {
    if !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&n) {
        i64::MIN
    } else {
        n as i64
    }
}
pub(super) fn set_errno(code: i32) {
    ERRNO.set(code);
}
#[cfg(not(any(target_os = "linux", windows)))]
pub(super) fn path(bytes: &[u8]) -> std::borrow::Cow<'_, std::path::Path> {
    let bytes = c_bytes(bytes);
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        std::borrow::Cow::Borrowed(std::path::Path::new(std::ffi::OsStr::from_bytes(bytes)))
    }
    #[cfg(not(unix))]
    {
        match String::from_utf8_lossy(bytes) {
            std::borrow::Cow::Borrowed(text) => {
                std::borrow::Cow::Borrowed(std::path::Path::new(text))
            }
            std::borrow::Cow::Owned(text) => std::borrow::Cow::Owned(text.into()),
        }
    }
}
pub(super) fn message(code: i32) -> Result<std::borrow::Cow<'static, [u8]>, Error> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        crate::locale::posix::error_message(code).map(std::borrow::Cow::Owned)
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    Ok(std::borrow::Cow::Borrowed(match code {
        0 => b"No error",
        2 => b"No such file or directory",
        8 => b"Exec format error",
        9 => b"Bad file descriptor",
        12 => b"Not enough space",
        13 => b"Permission denied",
        17 => b"File exists",
        18 => b"Improper link",
        22 => b"Invalid argument",
        24 => b"Too many open files",
        28 => b"No space left on device",
        29 => b"Illegal seek",
        32 => b"Broken pipe",
        34 => b"Result too large",
        41 => b"Directory not empty",
        _ => b"Input/output error",
    }))
}
fn runtime(text: &'static str) -> Error {
    Error::runtime_static(text)
}
pub(super) fn open(path: &[u8], mode: &[u8]) -> Result<Result<Stream, i32>, Error> {
    // MSVC fopen rejects the empty C-string as EINVAL, before opening a path.
    if cfg!(windows) && c_bytes(path).is_empty() {
        return Ok(Err(22));
    }
    let mode = c_bytes(mode);
    let Some(&first) = mode.first() else {
        return Ok(Err(22));
    };
    if !matches!(first, b'r' | b'w' | b'a')
        || mode[1..].iter().any(|b| !matches!(b, b'+' | b'b' | b't'))
    {
        return Ok(Err(22));
    }
    let update = mode.contains(&b'+');
    let read = first == b'r' || update;
    let write = first != b'r' || update;
    let file = match input::open(c_bytes(path), first, update)? {
        Ok(file) => file,
        Err(code) => return Ok(Err(code)),
    };
    #[cfg(windows)]
    let file = match (|| -> Result<_, i32> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = file;
        if read && write && !mode.contains(&b'b') {
            let length = file.metadata().map_err(stream::errno)?.len();
            if length != 0 {
                file.seek(SeekFrom::End(-1)).map_err(stream::errno)?;
                let mut last = [0];
                file.read_exact(&mut last).map_err(stream::errno)?;
                if last[0] == 26 {
                    file.set_len(length - 1).map_err(stream::errno)?;
                }
            }
            file.seek(SeekFrom::Start(0)).map_err(stream::errno)?;
        }
        Ok(file)
    })() {
        Ok(file) => file,
        Err(code) => return Ok(Err(code)),
    };
    Ok(Ok(Stream::new(
        Resource::File(file),
        read,
        write,
        !mode.contains(&b'b'),
    )))
}
pub(super) fn open_input(path: &[u8], binary: bool) -> Result<Result<Stream, i32>, Error> {
    input::open(c_bytes(path), b'r', false)
        .map(|result| result.map(|file| Stream::new(Resource::File(file), true, false, !binary)))
}
fn pipe(command: &[u8], mode: &[u8]) -> Result<Result<Stream, i32>, Error> {
    let mode = c_bytes(mode);
    let Some(&first) = mode.first() else {
        return Ok(Err(22));
    };
    if !matches!(first, b'r' | b'w') || mode[1..].iter().any(|b| !matches!(b, b'b' | b't')) {
        return Ok(Err(22));
    }
    process::open(c_bytes(command), first == b'r').map(|result| {
        result.map(|pipe| {
            Stream::new(
                Resource::Pipe(pipe),
                first == b'r',
                first == b'w',
                !mode.contains(&b'b'),
            )
        })
    })
}

impl Vm {
    pub(super) fn open_io(&mut self) -> Result<(), Error> {
        let meta = self.try_new_table()?;
        let Value::String(tag) = self.try_intern(b"FILE*")? else {
            unreachable!()
        };
        self.io.file_tag = Some(tag);
        self.raw_set(self.registry, Value::String(tag), Value::Table(meta))?;
        let index = self.try_intern(b"__index")?;
        self.raw_set(meta, index, Value::Table(meta))?;
        for (name, id) in [
            (b"close".as_slice(), FCLOSE),
            (b"flush", FFLUSH),
            (b"lines", FLINES),
            (b"read", FREAD),
            (b"seek", SEEK),
            (b"setvbuf", SETVBUF),
            (b"write", FWRITE),
            (b"__gc", GC),
            (b"__tostring", TO_STRING),
        ] {
            let key = self.try_intern(name)?;
            self.raw_set(meta, key, Value::Native(id))?;
        }
        let env = self.try_new_table()?;
        let key = self.try_intern(b"__close")?;
        self.raw_set(env, key, Value::Native(REAL_CLOSE))?;
        let standard = self.try_new_table()?;
        self.raw_set(standard, key, Value::Native(NO_CLOSE))?;
        let popen = self.try_new_table()?;
        self.raw_set(popen, key, Value::Native(PIPE_CLOSE))?;
        let library = self.try_new_table()?;
        for (name, id) in [
            (b"close".as_slice(), CLOSE),
            (b"flush", FLUSH),
            (b"input", INPUT),
            (b"lines", LINES),
            (b"open", OPEN_FILE),
            (b"output", OUTPUT),
            (b"popen", POPEN),
            (b"read", READ),
            (b"tmpfile", TMPFILE),
            (b"type", TYPE),
            (b"write", WRITE),
        ] {
            self.set_native_environment(id, if id == POPEN { popen } else { env })?;
            let key = self.try_intern(name)?;
            self.raw_set(library, key, Value::Native(id))?;
        }
        for (index, name, resource, read, write) in [
            (1, b"stdin".as_slice(), Resource::Input, true, false),
            (2, b"stdout", Resource::Output, false, true),
            (0, b"stderr", Resource::Error, false, true),
        ] {
            let value = self.try_file_value(Stream::new(resource, read, write, true), standard)?;
            let key = self.file_key(value, false).and_then(NonZeroU64::new);
            match index {
                1 => self.io.stdin = key,
                2 => self.io.stdout = key,
                _ => self.io.stderr = key,
            }
            let name = self.try_intern(name)?;
            self.raw_set(library, name, value)?;
            if index != 0 {
                self.raw_set(env, Value::Number(index as f64), value)?;
            }
        }
        self.set_global(b"io", Value::Table(library))?;
        Ok(())
    }
    fn try_file_value(&mut self, file: Stream, env: TableId) -> Result<Value, Error> {
        let id = self.try_allocate_userdata(std::mem::size_of::<usize>(), env)?;
        let metatable = self.file_metatable();
        self.reserve_gc_metatable(Value::Userdata(id), metatable)?;
        let file = self
            .io
            .files
            .try_insert(file)
            .map_err(|_| self.memory_failure())?;
        let user = &mut self.userdata[slot_index(id.0)];
        user.file = NonZeroU64::new(file);
        user.metatable = metatable;
        if let Some(table) = metatable {
            self.gc_barrier(Value::Userdata(id), Value::Table(table));
        }

        Ok(Value::Userdata(id))
    }
    pub(in crate::vm) fn file_key(&mut self, value: Value, check_meta: bool) -> Option<u64> {
        let Value::Userdata(id) = value else {
            return None;
        };
        let user = self.userdata.get(id.0)?;
        if check_meta {
            let expected = self.file_metatable()?;
            if user.metatable != Some(expected) {
                return None;
            }
        }
        user.file.map(NonZeroU64::get)
    }
    fn file_metatable(&self) -> Option<TableId> {
        let tag = self.io.file_tag.or_else(|| self.strings.find(b"FILE*"))?;
        match self.tables[slot_index(self.registry.0)].get(Value::String(tag)) {
            Value::Table(id) => Some(id),
            _ => None,
        }
    }
    pub(in crate::vm) fn checked_file(
        &mut self,
        value: Value,
        check_meta: bool,
    ) -> Result<u64, Error> {
        let missing = self
            .active_native
            .is_some_and(|i| self.frames[i].argc() == 0);
        self.checked_file_value(value, check_meta, missing)
    }
    fn checked_file_value(
        &mut self,
        value: Value,
        check_meta: bool,
        missing: bool,
    ) -> Result<u64, Error> {
        let key = self.file_key(value, check_meta).ok_or_else(|| {
            match fallible::format(format_args!(
                "FILE* expected, got {}",
                if missing {
                    "no value"
                } else {
                    value.type_name()
                }
            )) {
                Ok(message) => self.argument_error(0, message.as_bytes()),
                Err(error) => error,
            }
        })?;
        if self.io.files[slot_index(key)].resource.is_none() {
            return Err(runtime("attempt to use a closed file"));
        }
        Ok(key)
    }
    fn default_file(&mut self, id: u64, output: bool) -> Result<Value, Error> {
        let env = self.builtin_environment(id);
        let value =
            self.tables[slot_index(env.0)].get(Value::Number(if output { 2.0 } else { 1.0 }));
        let key = self
            .file_key(value, false)
            .ok_or_else(|| runtime("invalid standard file"))?;
        if self.io.files[slot_index(key)].resource.is_none() {
            return Err(runtime(if output {
                "standard output file is closed"
            } else {
                "standard input file is closed"
            }));
        }
        Ok(value)
    }
    pub(super) fn io_failure(
        &mut self,
        base: usize,
        code: i32,
        path: Option<&[u8]>,
    ) -> Result<usize, Error> {
        set_errno(code);
        if code == 12 {
            return Err(self.memory_failure());
        }
        let mut bytes = self.try_take_buffer(64)?;
        if let Some(path) = path {
            bytes.try_extend_from_slice(c_bytes(path))?;
            bytes.try_extend_from_slice(b": ")?;
        }
        bytes.try_extend_from_slice(&message(code)?)?;
        let text = self.try_finish_buffer(bytes)?;
        self.reserve_registers(base + 3)?;
        self.resize_registers(self.registers.len().max(base + 3));
        self.registers[base] = Value::Nil;
        self.registers[base + 1] = text;
        self.registers[base + 2] = Value::Number(f64::from(code));
        Ok(3)
    }
    #[cold]
    fn file_argument_error(&self, path: &[u8], code: i32) -> Error {
        if code == 12 {
            return self.memory_failure();
        }
        let detail = (|| -> Result<Vec<u8>, Error> {
            let mut detail = c_bytes(path).try_to_vec()?;
            detail.try_extend_from_slice(b": ")?;
            detail.try_extend_from_slice(&message(code)?)?;
            Ok(detail)
        })();
        match detail {
            Ok(detail) => self.argument_error(0, &detail),
            Err(error) => error,
        }
    }
    fn io_result(&mut self, base: usize, result: Result<(), i32>) -> Result<usize, Error> {
        match result {
            Ok(()) => {
                self.registers[base] = Value::Boolean(true);
                Ok(1)
            }
            Err(code) => self.io_failure(base, code, None),
        }
    }
    fn close_stream(&mut self, key: u64, base: usize) -> Result<usize, Error> {
        match self.io.files[slot_index(key)].close() {
            Ok(_) => self.io_result(base, Ok(())),
            Err(code) => self.io_failure(base, code, None),
        }
    }
    pub(super) fn io_native(&mut self, id: u64, base: usize, count: usize) -> Result<usize, Error> {
        debug_assert!(callback(id).is_none());
        let arg = self.argument(base, count, 0);
        if id == NO_CLOSE {
            self.reserve_registers(base + 2)?;
            self.resize_registers(self.registers.len().max(base + 2));
            self.registers[base] = Value::Nil;
            self.registers[base + 1] = self.try_intern(b"cannot close standard file")?;
            return Ok(2);
        }

        if matches!(id, REAL_CLOSE | PIPE_CLOSE) {
            let key = self.checked_file(arg, true)?;
            return self.close_stream(key, base);
        }
        if id == TYPE {
            if count == 0 {
                return Err(self.argument_error(0, b"value expected"));
            }
            self.registers[base] = match self.file_key(arg, true) {
                Some(key) => {
                    self.try_intern(if self.io.files[slot_index(key)].resource.is_some() {
                        b"file"
                    } else {
                        b"closed file"
                    })?
                }
                None => Value::Nil,
            };
            return Ok(1);
        }
        if matches!(id, OPEN_FILE | POPEN | TMPFILE) {
            let path = if id == TMPFILE {
                None
            } else {
                let s = self.string_argument(base, count, 0)?;
                Some(self.strings.bytes[slot_index(s.0)].try_to_vec()?)
            };
            let mode = if id == TMPFILE || self.argument(base, count, 1) == Value::Nil {
                b"r".try_to_vec()?
            } else {
                let s = self.string_argument(base, count, 1)?;
                self.strings.bytes[slot_index(s.0)].try_to_vec()?
            };
            // Lua roots a closed userdata before attempting the OS open.
            let value = self.try_file_value(Stream::closed(), self.builtin_environment(id))?;
            let pending = base + count.max(3) + 1;
            self.reserve_registers(pending + 1)?;
            self.resize_registers(self.registers.len().max(pending + 1));
            self.registers[pending] = value;
            let result = self.open_stream(path.as_deref(), &mode, id == POPEN)?;
            return Ok(match result {
                Ok(file) => {
                    let key = self.file_key(value, false).expect("new file userdata");
                    self.io.files[slot_index(key)] = file;
                    self.registers[base] = value;
                    1
                }
                Err(code) => self.io_failure(base, code, path.as_deref())?,
            });
        }
        if matches!(id, INPUT | OUTPUT) {
            let env = self.builtin_environment(id);
            let index = Value::Number(if id == INPUT { 1.0 } else { 2.0 });
            if arg != Value::Nil {
                let value = if matches!(arg, Value::String(_) | Value::Number(_)) {
                    let s = self.string_argument(base, count, 0)?;
                    let path = self.strings.bytes[slot_index(s.0)].try_to_vec()?;
                    let file = self
                        .open_stream(Some(&path), if id == INPUT { b"r" } else { b"w" }, false)?
                        .map_err(|code| {
                            ERRNO.set(code);
                            self.file_argument_error(&path, code)
                        })?;
                    self.try_file_value(file, env)?
                } else {
                    self.checked_file(arg, true)?;
                    arg
                };
                self.raw_set(env, index, value)?;
            }
            self.registers[base] = self.tables[slot_index(env.0)].get(index);
            return Ok(1);
        }
        if id == LINES && arg != Value::Nil {
            let s = self.string_argument(base, count, 0)?;
            let path = self.strings.bytes[slot_index(s.0)].try_to_vec()?;
            let file = self
                .open_stream(Some(&path), b"r", false)?
                .map_err(|code| {
                    ERRNO.set(code);
                    self.file_argument_error(&path, code)
                })?;
            let Value::Userdata(file) = self.try_file_value(file, self.builtin_environment(id))?
            else {
                unreachable!()
            };
            self.registers[base] = self.new_native(
                threads::NativeKind::Lines,
                &[Value::Userdata(file), Value::Boolean(true)],
            )?;
            return Ok(1);
        }
        let default = matches!(id, READ | WRITE | FLUSH | LINES);
        if id == LINES && arg == Value::Nil && count != 0 {
            // io_lines appends the default file, but f_lines still checks slot 1.
            self.checked_file(arg, true)?;
        }
        let pushed_default = default && id == LINES;
        let value = if pushed_default {
            self.tables[slot_index(self.builtin_environment(id).0)].get(Value::Number(1.))
        } else if default {
            self.default_file(id, matches!(id, WRITE | FLUSH))?
        } else {
            arg
        };
        let key = if id == TO_STRING {
            self.file_key(value, true)
                .ok_or_else(|| self.argument_type_error(base, count, 0, "FILE*"))?
        } else if pushed_default {
            self.checked_file_value(value, true, false)?
        } else {
            self.checked_file(value, !default || id == LINES)?
        };
        let slot = slot_index(key);
        let first = usize::from(!default);
        let before = self.io.files[slot].heap_bytes();
        let result = (|| match id {
            TO_STRING => {
                self.registers[base] = if self.io.files[slot].resource.is_none() {
                    self.try_intern(b"file (closed)")?
                } else {
                    let bytes = fallible::format(format_args!("file ({key:016X})"))?.into_bytes();
                    self.try_intern(&bytes)?
                };
                Ok(1)
            }
            FLUSH | FFLUSH => {
                let result = self.io.files[slot].flush();
                self.io_result(base, result)
            }
            LINES | FLINES => {
                let Value::Userdata(file) = value else {
                    unreachable!()
                };
                self.registers[base] = self.new_native(
                    threads::NativeKind::Lines,
                    &[Value::Userdata(file), Value::Boolean(false)],
                )?;
                Ok(1)
            }
            SEEK => {
                let mode = if self.argument(base, count, 1) == Value::Nil {
                    1
                } else {
                    let s = self.string_argument(base, count, 1)?;
                    match c_bytes(&self.strings.bytes[slot_index(s.0)]) {
                        b"set" => 0,
                        b"cur" => 1,
                        b"end" => 2,
                        _ => {
                            return Err(
                                self.argument_option_error(1, &self.strings.bytes[slot_index(s.0)])
                            );
                        }
                    }
                };
                let offset = match self.argument(base, count, 2) {
                    Value::Nil => 0,
                    value => {
                        let n = self
                            .number(value)
                            .ok_or_else(|| self.argument_type_error(base, count, 2, "number"))?;
                        if cfg!(windows) {
                            i64::from(debug::integer(n))
                        } else {
                            lua_integer(n)
                        }
                    }
                };
                Ok(match self.io.files[slot].seek(mode, offset) {
                    Ok(n) => {
                        self.registers[base] =
                            Value::Number(if cfg!(windows) && n > i64::from(i32::MAX) {
                                -1.0
                            } else {
                                n as f64
                            });
                        1
                    }
                    Err(code) => self.io_failure(base, code, None)?,
                })
            }
            SETVBUF => {
                let s = self.string_argument(base, count, 1)?;
                let mode = match c_bytes(&self.strings.bytes[slot_index(s.0)]) {
                    b"no" => 0,
                    b"full" => 1,
                    b"line" => 2,
                    _ => {
                        return Err(
                            self.argument_option_error(1, &self.strings.bytes[slot_index(s.0)])
                        );
                    }
                };
                let size = match self.argument(base, count, 2) {
                    Value::Nil => stream::LINE_BUFFER,
                    value => lua_integer(
                        self.number(value)
                            .ok_or_else(|| self.argument_type_error(base, count, 2, "number"))?,
                    ) as usize,
                };
                if mode != 0 && !(2..=i32::MAX as usize).contains(&size) {
                    return self.io_failure(base, 22, None);
                }
                let result = self.io.files[slot].set_buffering(mode, size);
                self.io_result(base, result)
            }
            WRITE | FWRITE => {
                let mut status = Ok(());
                for index in first..count {
                    let value = self.argument(base, count, index);
                    let formatted;
                    let bytes = match value {
                        Value::Number(n) => {
                            formatted = native::try_format_number(n)
                                .map_err(|_| self.memory_failure())?;
                            formatted.as_slice()
                        }
                        _ => {
                            let s = self.string_argument(base, count, index)?;
                            self.strings.bytes[slot_index(s.0)].as_ref()
                        }
                    };
                    if status.is_ok() {
                        if self.io.is_stdout(key)
                            && let Some(output) = self.output.as_mut()
                        {
                            output.try_extend_from_slice(bytes)?;
                        } else {
                            status = self.io.files[slot].write(bytes);
                        }
                    }
                }
                self.io_result(base, status)
            }
            READ | FREAD => {
                // Lua 5.1 g_read calls clearerr; its line iterator does not.
                self.io.files[slot].clear_eof();
                let formats = count.saturating_sub(first).max(1);
                let mut results = 0;
                for index in 0..formats {
                    let mut bytes = self.try_take_buffer(128)?;
                    let format = if count == first {
                        Value::Nil
                    } else {
                        self.argument(base, count, first + index)
                    };
                    let result = if format == Value::Nil && count == first {
                        self.io.files[slot]
                            .read_line(&mut bytes)
                            .map(|ok| (ok, None))
                    } else if let Value::Number(n) = format {
                        self.io.files[slot]
                            .read_chars(&mut bytes, lua_integer(n) as usize)
                            .map(|ok| (ok, None))
                    } else {
                        let text = if let Value::String(s) = format {
                            self.strings.bytes[slot_index(s.0)].as_ref()
                        } else {
                            self.recycle_buffer(bytes);
                            return Err(self.argument_error(first + index, b"invalid option"));
                        };
                        if text.first() != Some(&b'*') {
                            self.recycle_buffer(bytes);
                            return Err(self.argument_error(first + index, b"invalid option"));
                        }
                        match text.get(1) {
                            Some(b'l') => self.io.files[slot]
                                .read_line(&mut bytes)
                                .map(|ok| (ok, None)),
                            Some(b'a') => self.io.files[slot]
                                .read_chars(&mut bytes, usize::MAX)
                                .map(|_| (true, None)),
                            Some(b'n') => read_number(&mut self.io.files[slot], &mut bytes)
                                .map(|n| (n.is_some(), n)),
                            _ => {
                                self.recycle_buffer(bytes);
                                return Err(self.argument_error(first + index, b"invalid format"));
                            }
                        }
                    };
                    let (ok, number) = match result {
                        Ok(r) => r,
                        Err(code) => {
                            self.recycle_buffer(bytes);
                            return self.io_failure(base, code, None);
                        }
                    };
                    let value = if !ok {
                        self.recycle_buffer(bytes);
                        Value::Nil
                    } else if let Some(n) = number {
                        self.recycle_buffer(bytes);
                        Value::Number(n)
                    } else {
                        self.try_finish_buffer(bytes)?
                    };
                    self.reserve_registers(base + formats)?;
                    self.resize_registers(self.registers.len().max(base + formats));
                    self.registers[base + index] = value;
                    results += 1;
                    if !ok {
                        break;
                    }
                }
                Ok(results)
            }
            _ => unreachable!("I/O function ID"),
        })();
        self.io
            .files
            .note_alloc(self.io.files[slot].heap_bytes().saturating_sub(before));
        result
    }
}
