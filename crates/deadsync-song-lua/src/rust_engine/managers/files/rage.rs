//! RageFile handles retain Rust buffers and a VM-local simulated write store.
//! Writes become visible to other handles on close; they never reach disk.
use super::super::fields::{object, set};
use super::*;
use crate::rage_file::{FileWrites, c_string, set_file_path};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct Factory {
    song_dir: PathBuf,
    writes: Arc<Mutex<FileWrites>>,
}

#[derive(Default)]
struct File {
    path: PathBuf,
    bytes: Vec<u8>,
    pos: usize,
    mode: i32,
    open: bool,
    eof: bool,
    destroyed: bool,
    error: String,
}

impl File {
    fn check(&self, mode: i32, call: &HostCall<'_>) -> Result<(), Error> {
        let reason = if self.destroyed {
            "has been destroyed"
        } else if !self.open {
            "is not open"
        } else if mode != 0 && self.mode & mode == 0 {
            if mode == 1 {
                "is not open for reading"
            } else {
                "is not open for writing"
            }
        } else {
            return Ok(());
        };
        Err(call.error(format!("File '{}' {reason}.", self.path.display()).as_bytes()))
    }

    fn close(&mut self, factory: &Factory) {
        if self.open && self.mode & 2 != 0 {
            factory
                .writes
                .lock()
                .expect("file write store")
                .insert(&self.path, std::mem::take(&mut self.bytes));
        }
        self.open = false;
    }

    fn read(&mut self, count: i32, call: &HostCall<'_>) -> Result<&[u8], Error> {
        self.check(1, call)?;
        if count < -1 {
            return Err(call.error(b"ReadBytes: invalid byte count"));
        }
        let start = self.pos.min(self.bytes.len());
        let available = self.bytes.len() - start;
        let wanted = if count == -1 {
            available
        } else {
            count as usize
        };
        let size = wanted.min(available);
        self.pos += size;
        if count == -1 || wanted > available {
            self.eof = true;
        }
        Ok(c_string(&self.bytes[start..start + size]))
    }

    fn get_line(&mut self, call: &HostCall<'_>) -> Result<&[u8], Error> {
        self.check(1, call)?;
        let start = self.pos.min(self.bytes.len());
        let rest = &self.bytes[start..];
        if rest.is_empty() {
            self.eof = true;
            return Ok(&[]);
        }
        let size = memchr::memchr(b'\n', rest);
        let mut end = size.unwrap_or(rest.len());
        if size.is_some() && end > 0 && rest[end - 1] == b'\r' {
            end -= 1;
        }
        self.pos += size.map_or(rest.len(), |size| size + 1);
        Ok(c_string(&rest[..end]))
    }
}

#[derive(Clone, Copy)]
enum Method {
    Open,
    Write,
    PutLine,
    Read,
    GetLine,
    ReadBytes,
    Seek,
    Tell,
    AtEof,
    GetError,
    Flush,
    Close,
    ClearError,
    Destroy,
}

#[derive(Clone)]
struct Binding {
    factory: Factory,
    file: Arc<Mutex<File>>,
    method: Method,
}

pub(super) fn install(vm: &mut Vm, song_dir: &Path) -> Result<(), Error> {
    let globals = vm.get_global(b"_G")?;
    let factory = Factory {
        song_dir: song_dir.to_owned(),
        writes: Arc::new(Mutex::new(FileWrites::default())),
    };
    let function = vm.new_host_function_with_data(initialize, &[globals], factory)?;
    vm.call(function, &[]).map(|_| ())
}

fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = table(call, call.captures()[0])?;
    let factory = call
        .function_data::<Factory>()
        .expect("file factory")
        .clone();
    let util = object(call, |call, owner| {
        let function =
            call.new_host_function_with_data(create, &[Value::Table(globals)], factory)?;
        set(owner, "CreateRageFile", function, call)
    })?;
    set(globals, "RageFileUtil", util, call)
}

fn create(call: &mut HostCall<'_>) -> Result<(), Error> {
    let factory = call
        .function_data::<Factory>()
        .expect("file factory")
        .clone();
    let file = Arc::new(Mutex::new(File::default()));
    let globals = call.captures()[0];
    let owner = object(call, |call, owner| {
        for (name, method) in [
            ("Open", Method::Open),
            ("Write", Method::Write),
            ("PutLine", Method::PutLine),
            ("Read", Method::Read),
            ("GetLine", Method::GetLine),
            ("ReadBytes", Method::ReadBytes),
            ("Seek", Method::Seek),
            ("Tell", Method::Tell),
            ("AtEOF", Method::AtEof),
            ("GetError", Method::GetError),
            ("Flush", Method::Flush),
            ("Close", Method::Close),
            ("ClearError", Method::ClearError),
            ("destroy", Method::Destroy),
        ] {
            let function = call.new_host_function_with_data(
                dispatch,
                &[globals],
                Binding {
                    factory: factory.clone(),
                    file: file.clone(),
                    method,
                },
            )?;
            set(owner, name, function, call)?;
        }
        Ok(())
    })?;
    call.push(Value::Table(owner))
}

fn bytes(call: &mut HostCall<'_>, index: usize) -> Result<Vec<u8>, Error> {
    let value = call.arguments().get(index).copied().unwrap_or(Value::Nil);
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    Ok(c_string(call.string_bytes(id).expect("live string")).to_vec())
}

fn integer(call: &HostCall<'_>, index: usize) -> Result<i32, Error> {
    let value = call.number(index)?.trunc();
    if !(i32::MIN as f64..=i32::MAX as f64).contains(&value) {
        return Err(call.error(b"integer out of range"));
    }
    Ok(value as i32)
}

fn dispatch(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call
        .function_data::<Binding>()
        .expect("file method")
        .clone();
    let owner = call.arguments().first().copied().unwrap_or(Value::Nil);
    let Value::Table(_) = owner else {
        return Err(call.error(b"RageFile self expected"));
    };
    let text = if matches!(
        binding.method,
        Method::Open | Method::Write | Method::PutLine
    ) {
        Some(bytes(call, 1)?)
    } else {
        None
    };
    let number = if matches!(binding.method, Method::Open) {
        Some(integer(call, 2)?)
    } else if matches!(binding.method, Method::ReadBytes | Method::Seek) {
        Some(integer(call, 1)?)
    } else {
        None
    };
    // A callback may reenter through the global side-effect counter's metamethods.
    // Match the native userdata's failed borrow instead of waiting on this mutex.
    let mut state = binding
        .file
        .try_lock()
        .map_err(|_| call.error(b"file already borrowed"))?;
    let result = match binding.method {
        Method::Open => {
            if state.destroyed {
                state.check(0, call)?;
            }
            super::super::note_side_effect(call, call.captures()[0])?;
            state.close(&binding.factory);
            set_file_path(
                &mut state.path,
                &binding.factory.song_dir,
                &String::from_utf8_lossy(text.as_ref().unwrap()),
            );
            let mode = number.unwrap();
            state.mode = mode;
            state.pos = 0;
            state.eof = false;
            state.bytes.clear();
            let opened = if mode & 3 == 3 {
                state.error = "Reading and writing are mutually exclusive".into();
                false
            } else if mode & 3 == 0 {
                state.error = "Neither reading nor writing specified".into();
                false
            } else if mode & 1 != 0 {
                let loaded = if let Some(bytes) = binding
                    .factory
                    .writes
                    .lock()
                    .expect("file write store")
                    .get(&state.path)
                {
                    Ok(bytes.clone())
                } else {
                    std::fs::read(&state.path)
                };
                match loaded {
                    Ok(bytes) => {
                        state.bytes = bytes;
                        true
                    }
                    Err(error) => {
                        state.error = match error.kind() {
                            std::io::ErrorKind::NotFound => "No such file or directory".into(),
                            std::io::ErrorKind::PermissionDenied => "Permission denied".into(),
                            _ => error.to_string(),
                        };
                        false
                    }
                }
            } else if state.path.is_dir() || state.path.ancestors().skip(1).any(Path::is_file) {
                state.error = "Not a writable file path".into();
                false
            } else {
                true
            };
            state.open = opened;
            Value::Boolean(opened)
        }
        Method::Write | Method::PutLine => {
            state.check(2, call)?;
            super::super::note_side_effect(call, call.captures()[0])?;
            let bytes = text.as_ref().unwrap();
            state.bytes.extend_from_slice(bytes);
            let count = if matches!(binding.method, Method::PutLine) {
                state.bytes.extend_from_slice(b"\r\n");
                2
            } else {
                bytes.len()
            };
            Value::Number(count as f64)
        }
        Method::Read | Method::GetLine | Method::ReadBytes => {
            let bytes = if matches!(binding.method, Method::GetLine) {
                state.get_line(call)?
            } else {
                state.read(number.unwrap_or(-1), call)?
            };
            call.try_intern(bytes)?
        }
        Method::Seek => {
            state.check(1, call)?;
            let pos = number.unwrap();
            if pos >= 0 && state.pos == pos as usize {
                Value::Number(pos as f64)
            } else {
                state.eof = false;
                if pos < 0 {
                    Value::Number(-1.)
                } else {
                    state.pos = pos as usize;
                    Value::Number(pos as f64)
                }
            }
        }
        Method::Tell => {
            state.check(1, call)?;
            Value::Number(state.pos as f64)
        }
        Method::AtEof => {
            state.check(1, call)?;
            Value::Boolean(state.eof)
        }
        Method::GetError => {
            if state.destroyed {
                state.check(0, call)?;
            }
            call.try_intern(state.error.as_bytes())?
        }
        Method::Flush => {
            if state.destroyed {
                state.check(0, call)?;
            }
            if !state.open {
                state.error = "Not open".into();
            }
            super::super::note_side_effect(call, call.captures()[0])?;
            owner
        }
        Method::Close | Method::ClearError | Method::Destroy => {
            if state.destroyed {
                state.check(0, call)?;
            }
            super::super::note_side_effect(call, call.captures()[0])?;
            if matches!(binding.method, Method::ClearError) {
                state.error.clear();
            } else {
                state.close(&binding.factory);
            }
            if matches!(binding.method, Method::Destroy) {
                state.destroyed = true;
            }
            if call.arguments().len() > 1 {
                *call.arguments().last().unwrap()
            } else {
                let name = match binding.method {
                    Method::Close => "Close",
                    Method::ClearError => "ClearError",
                    _ => "destroy",
                };
                let key = call.try_intern(name.as_bytes())?;
                let method = call.table_get(owner, key)?;
                if !matches!(method, Value::Function(_) | Value::Native(_)) {
                    return Err(call.error(b"RageFile method function expected"));
                }
                method
            }
        }
    };
    call.push(result)
}
