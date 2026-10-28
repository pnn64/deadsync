//! Host ABI binding and Lua-owned dynamic library handles.
use super::*;
use std::any::Any;

/// A resolved foreign function, with private binding data and no Lua upvalues.
pub struct NativeSymbol {
    pub(super) callback: HostFunction,
    pub(super) data: Option<Box<dyn Any + Send + Sync>>,
}
impl NativeSymbol {
    /// Bind a function implemented by a host ABI adapter.
    pub fn new(callback: HostFunction) -> Self {
        Self {
            callback,
            data: None,
        }
    }
    /// Bind private data. Lua values in this data are not collector roots.
    pub fn with_data<T: Any + Send + Sync>(callback: HostFunction, data: T) -> Self {
        Self {
            callback,
            data: Some(Box::new(data)),
        }
    }
}

/// One opened library. Methods must not reenter Lua or panic.
///
/// The owning userdata calls `close` from its finalizer. Implementations must
/// also release an unclosed resource on destruction, and make repeated closes
/// harmless. Binding data may retain the owner, but must not keep it open after
/// an explicit close. Calling functions from a closed library is unsupported.
pub trait NativeLibrary: std::fmt::Debug + Send + Sync {
    /// Native loader handle, stored in the Lua userdata's pointer-sized payload.
    fn handle(&self) -> usize;
    /// Resolve a NUL-free symbol. Error bytes follow the platform loader.
    fn symbol(self: Arc<Self>, name: &[u8]) -> Result<NativeSymbol, Box<[u8]>>;
    /// Release the native loader reference and mark the handle closed.
    fn close(&self);
}

/// Platform/ABI adapter used by `package.loadlib` and the native searchers.
/// Opening and resolving never execute Lua. Returned functions run through the
/// ordinary host boundary, with their actual Lua closure environment.
pub trait NativeLibraryLoader: std::fmt::Debug + Send + Sync {
    /// Open a NUL-free path. Errors contain the platform loader's message bytes.
    fn open(&self, path: &[u8]) -> Result<Arc<dyn NativeLibrary>, Box<[u8]>>;
}

#[derive(Debug)]
pub(in crate::vm) struct LoadingLibrary {
    thread: ThreadId,
    owner: usize,
    code: u64,
    path: Vec<u8>,
    symbol: Vec<u8>,
    name: Vec<u8>,
    file: usize,
}
impl LoadingLibrary {
    pub(in crate::vm) fn heap_bytes(&self) -> usize {
        self.path.capacity() + self.symbol.capacity() + self.name.capacity()
    }
}

impl Vm {
    /// Install a native-library ABI adapter, or disable future opens.
    /// Existing library userdata and resolved functions retain their owners.
    /// The default `native-c-api` feature installs the bundled adapter on
    /// Windows and Linux; embedders can replace it or disable future opens.
    pub fn set_native_library_loader(&mut self, loader: Option<Arc<dyn NativeLibraryLoader>>) {
        self.native_library_support |= loader.is_some();
        self.native_library_loader = loader;
    }

    pub(in crate::vm) fn trim_library_loads(&mut self) {
        let mut jobs = std::mem::take(&mut self.library_loads);
        jobs.retain(|job| {
            let frames = if job.thread == self.thread {
                &self.frames
            } else if let Some(thread) = self.threads.get(job.thread.0) {
                &thread.frames
            } else {
                return false;
            };
            frames.get(job.owner).is_some_and(|frame| {
                let code = self
                    .body_code(job.thread, job.owner)
                    .map_or(frame.function, FrameKind::Native);
                matches!(code, FrameKind::Native(id) if id == job.code) && frame.pc >= 100
            })
        });
        self.library_loads = jobs;
    }

    fn begin_library_load(&mut self, code: u64, mut frame: Frame) -> Result<(), Error> {
        let owner = self.frames.len() - 1;
        let path;
        let symbol;
        let name;
        let file;
        let mut stack =
            self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].to_vec();
        if code == LOADLIB {
            for n in 0..2 {
                if !matches!(stack.get(n), Some(Value::String(_) | Value::Number(_))) {
                    return Err(self.package_bad(frame, n, b"string"));
                }
                let id = self.string_argument(frame.vararg_start - 1, frame.argc(), n)?;
                stack[n] = Value::String(id);
            }
            path = self.concat_bytes(stack[0])?;
            symbol = self.concat_bytes(stack[1])?;
            name = Vec::new();
            file = 0;
        } else {
            let base = frame.base;
            path = self.concat_bytes(self.registers[base + 5])?;
            name = self.concat_bytes(self.registers[base])?;
            let modname = io::c_bytes(&name);
            let modname = modname
                .iter()
                .position(|&b| b == b'-')
                .map_or(modname, |i| &modname[i + 1..]);
            symbol = [b"luaopen_".as_slice(), &replace(modname, b'.', b"_")].concat();
            if code == CROOT {
                stack.push(self.registers[base + 10]);
            }
            stack.extend_from_slice(&self.registers[base + 1..base + 4]);
            file = frame.base + stack.len();
            stack.push(self.registers[base + 5]);
            stack.push(self.intern(&symbol));
        }
        let old = frame.vararg_start..frame.vararg_start + frame.argc();
        let scratch_end = self.registers.len();
        self.resize_registers(scratch_end.max(frame.base + stack.len()));
        self.registers[old.clone()].fill(Value::Nil);
        self.registers[frame.base..].fill(Value::Nil);
        self.registers[frame.base..frame.base + stack.len()].copy_from_slice(&stack);
        self.relocate_native_error_functions(old, frame.base);
        frame.vararg_start = frame.base;
        frame.vararg_count = stack.len() as u32;
        frame.pc = 100;
        self.frames[owner] = frame;
        let path = io::c_bytes(&path).to_vec();
        let symbol = io::c_bytes(&symbol).to_vec();
        let before = self.library_loads.capacity();
        self.library_loads.push(LoadingLibrary {
            thread: self.thread,
            owner,
            code,
            path,
            symbol,
            name,
            file,
        });
        self.gc_note_control(
            (self.library_loads.capacity() - before) * size_of::<LoadingLibrary>()
                + self.library_loads.last().unwrap().heap_bytes(),
        );
        Ok(())
    }

    fn library_failure(
        &mut self,
        mut frame: Frame,
        job: LoadingLibrary,
        init: bool,
        message: &[u8],
    ) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        let end = frame.base + frame.argc();
        self.resize_registers(self.registers.len().max(end + 3));
        if job.code == LOADLIB {
            self.registers[end] = Value::Nil;
            self.registers[end + 1] = self.intern(message);
            self.registers[end + 2] = self.intern(if init { b"init" } else { b"open" });
            frame.vararg_count += 3;
            self.frames[owner] = frame;
            return Ok(self.return_frame(frame, end, 3));
        }
        if job.code == CROOT && init {
            let text = [
                b"\n\tno module '".as_slice(),
                io::c_bytes(&job.name),
                b"' in file '",
                &job.path,
                b"'",
            ]
            .concat();
            // The failed symbol's message remains below the C-root diagnostic.
            self.registers[end] = self.intern(message);
            self.registers[end + 1] = self.intern(&text);
            frame.vararg_count += 2;
            self.frames[owner] = frame;
            return Ok(self.return_frame(frame, end + 1, 1));
        }
        let error = self.intern(message);
        self.package_load_error(frame, job.file, error)
    }

    pub(super) fn library_step(
        &mut self,
        code: u64,
        mut frame: Frame,
    ) -> Result<Option<usize>, Error> {
        if frame.pc < 100 {
            self.begin_library_load(code, frame)?;
            return Ok(None);
        }
        let owner = self.frames.len() - 1;
        let job_index = self
            .library_loads
            .iter()
            .rposition(|j| j.thread == self.thread && j.owner == owner)
            .expect("library operation");
        let end = frame.base + frame.argc();
        self.resize_registers(self.registers.len().max(end + 4));
        match frame.pc {
            100 => {
                let key = [b"LOADLIB: ".as_slice(), &self.library_loads[job_index].path].concat();
                let key = self.intern(&key);
                self.registers[end] = key;
                frame.vararg_count += 1;
                frame.pc = 101;
                self.frames[owner] = frame;
                self.get_table(Value::Table(self.registry), key, end)?;
            }
            101 => {
                if self.registers[end - 1] != Value::Nil {
                    self.frames[owner].pc = 104;
                } else {
                    let env = self.calling_environment(self.thread);
                    let id = self.try_allocate_userdata(size_of::<usize>(), env)?;
                    self.registers[end - 1] = Value::Userdata(id);
                    let key = self.intern(b"_LOADLIB");
                    frame.pc = 102;
                    self.frames[owner] = frame;
                    self.get_table(Value::Table(self.registry), key, end)?;
                }
            }
            102 => {
                let Value::Userdata(id) = self.registers[end - 1] else {
                    unreachable!("library userdata")
                };
                let mt = match self.registers[end] {
                    Value::Nil => None,
                    Value::Table(mt) => Some(mt),
                    _ => return Err(runtime("invalid library metatable")),
                };
                self.set_metatable(Value::Userdata(id), mt);
                self.registers[end] = self.intern(
                    &[b"LOADLIB: ".as_slice(), &self.library_loads[job_index].path].concat(),
                );
                self.registers[end + 1] = Value::Userdata(id);
                let key = self.registers[end];
                frame.vararg_count += 2;
                frame.pc = 103;
                self.frames[owner] = frame;
                self.set_table(Value::Table(self.registry), key, Value::Userdata(id))?;
            }
            103 => {
                self.registers[end - 2..end].fill(Value::Nil);
                frame.vararg_count -= 2;
                frame.pc = 104;
                self.frames[owner] = frame;
            }
            104 => {
                let job = self.library_loads.remove(job_index);
                let Value::Userdata(id) = self.registers[end - 1] else {
                    return Err(runtime("invalid native library registry entry"));
                };
                if self.userdata[slot_index(id.0)].library.is_none() {
                    let Some(loader) = self.native_library_loader.clone() else {
                        return self.library_failure(
                            frame,
                            job,
                            false,
                            b"dynamic library loader is disabled",
                        );
                    };
                    let library = match loader.open(&job.path) {
                        Ok(library) => library,
                        Err(message) => return self.library_failure(frame, job, false, &message),
                    };
                    let data = &mut self.userdata[slot_index(id.0)];
                    if data.bytes.len() != size_of::<usize>() {
                        return Err(runtime("invalid native library payload"));
                    }
                    data.bytes
                        .as_mut()
                        .copy_from_slice(&library.handle().to_ne_bytes());
                    data.library = Some(library);
                }
                let library = self.userdata[slot_index(id.0)]
                    .library
                    .as_ref()
                    .unwrap()
                    .clone();
                let symbol = match library.symbol(&job.symbol) {
                    Ok(symbol) => symbol,
                    Err(message) => return self.library_failure(frame, job, true, &message),
                };
                self.registers[end] = self.host_function(symbol.callback, &[], symbol.data)?;
                frame.vararg_count += 1;
                self.frames[owner] = frame;
                return Ok(self.return_frame(frame, end, 1));
            }
            _ => unreachable!("native library phase"),
        }
        Ok(None)
    }

    pub(super) fn library_gc(&mut self, mut frame: Frame) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        if frame.pc == 0 {
            let value = self.argument(frame.vararg_start - 1, frame.argc(), 0);
            let Value::Userdata(id) = value else {
                return Err(self.package_bad(frame, 0, b"_LOADLIB"));
            };
            let Some(mt) = self.metatable(value) else {
                return Err(self.package_bad(frame, 0, b"_LOADLIB"));
            };
            if self.userdata[slot_index(id.0)].bytes.len() != size_of::<usize>() {
                return Err(self.package_bad(frame, 0, b"_LOADLIB"));
            }
            let arguments = frame.vararg_start..frame.vararg_start + frame.argc();
            self.resize_registers(self.registers.len().max(frame.base + frame.argc() + 4));
            self.registers.copy_within(arguments.clone(), frame.base);
            self.relocate_native_error_functions(arguments.clone(), frame.base);
            self.registers[arguments].fill(Value::Nil);
            self.registers[frame.base + frame.argc()] = Value::Table(mt);
            self.registers[frame.base + frame.argc() + 3] = value;
            frame.vararg_start = frame.base;
            frame.vararg_count += 1;
            frame.pc = 1;
            self.frames[owner] = frame;
            let key = self.intern(b"_LOADLIB");
            self.get_table(Value::Table(self.registry), key, frame.base + frame.argc())?;
            return Ok(None);
        }
        let end = frame.base + frame.argc();
        if self.registers[end - 1] != self.registers[end] {
            return Err(self.package_bad(frame, 0, b"_LOADLIB"));
        }
        let Value::Userdata(id) = self.registers[end + 2] else {
            unreachable!("library close receiver")
        };
        self.registers[end - 1..end + 3].fill(Value::Nil);
        frame.vararg_count -= 1;
        self.frames[owner] = frame;
        if let Some(library) = self.userdata[slot_index(id.0)].library.take() {
            library.close();
        }
        self.userdata[slot_index(id.0)].bytes.as_mut().fill(0);
        Ok(self.return_frame(frame, frame.base, 0))
    }
}
