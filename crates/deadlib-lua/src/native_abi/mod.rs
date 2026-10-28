//! Lua 5.1 C ABI calls with C-only error jumps and VM-owned native stacks.
#![allow(unsafe_code)]
use crate::{
    Error, HostCall, HostThreadState, NativeDebugFrame, NativeHook, NativeLibrary,
    NativeLibraryLoader, NativeStack, NativeSymbol, ThreadId, Value,
};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

#[repr(C)]
#[derive(Default)]
struct BoundaryValue {
    integer: c_int,
    number: f64,
    bytes: *const c_char,
    length: usize,
    wide_integer: isize,
    reader: Option<Reader>,
    writer: Option<Writer>,
    userdata: *mut c_void,
    state: *mut c_void,
}
#[repr(C)]
struct DebugRecord {
    event: c_int,
    name: *const c_char,
    namewhat: *const c_char,
    what: *const c_char,
    source: *const c_char,
    currentline: c_int,
    nups: c_int,
    linedefined: c_int,
    lastlinedefined: c_int,
    short_src: [u8; 60],
    i_ci: c_int,
}
const _: () = assert!(
    std::mem::size_of::<DebugRecord>()
        == if cfg!(target_pointer_width = "64") {
            120
        } else {
            100
        }
);
type Dispatch = unsafe extern "C" fn(*mut c_void, c_int, c_int, c_int, *mut BoundaryValue) -> c_int;
type CFunction = unsafe extern "C" fn(*mut c_void) -> c_int;
type Invoke = unsafe extern "C" fn(*mut c_void, *mut c_void, Dispatch, CFunction) -> c_int;
type Hook = unsafe extern "C" fn(*mut c_void, *mut DebugRecord);
type InvokeHook =
    unsafe extern "C" fn(*mut c_void, *mut c_void, Dispatch, Hook, *mut DebugRecord) -> c_int;
type Reader = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut usize) -> *const c_char;
type Writer = unsafe extern "C" fn(*mut c_void, *const c_void, usize, *mut c_void) -> c_int;
type Write = unsafe extern "C" fn(
    *mut c_void,
    *mut c_void,
    Dispatch,
    Writer,
    *mut c_void,
    *const c_void,
    usize,
    *mut BoundaryValue,
) -> c_int;
type Read = unsafe extern "C" fn(
    *mut c_void,
    *mut c_void,
    Dispatch,
    Reader,
    *mut c_void,
    *mut BoundaryValue,
) -> c_int;
type NewState = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
type FreeState = unsafe extern "C" fn(*mut c_void) -> c_int;
mod library;
use library::Library;
mod bundle;

fn matching_version(pointer: usize) -> bool {
    // SAFETY: only a symbol from the Lua C ABI image is passed here.
    let version: unsafe extern "C" fn() -> u64 = unsafe { std::mem::transmute(pointer) };
    (unsafe { version() })
        == u64::from_str_radix(env!("DEADLIB_LUA_NATIVE_ABI_VERSION"), 16)
            .expect("build ABI version")
}

/// Platform adapter for the implemented Lua 5.1 C API. The bundled C image is
/// opened lazily; normal/build Cargo dependencies remain empty.
impl NativeCAbiLoader {
    /// Resolve an initializer from this VM build's C API image.
    /// Supported names are `base`, `math`, `string`, `table`, `os` and `debug`.
    /// Bind the result with [`crate::Vm::new_native_function`] and call it with
    /// the ordinary VM API. Initialization uses the current thread's globals,
    /// registry and function environment; repeated calls create fresh closures.
    pub fn standard_library(&self, name: &[u8]) -> Result<NativeSymbol, Box<[u8]>> {
        let symbol: &[u8] = match name {
            b"base" => b"luaopen_base",
            b"math" => b"luaopen_math",
            b"string" => b"luaopen_string",
            b"table" => b"luaopen_table",
            b"os" => b"luaopen_os",
            b"debug" => b"luaopen_debug",
            _ => return Err(b"unsupported standard library".as_slice().into()),
        };
        let bridge = self
            .bridge
            .get_or_init(Bridge::bundled)
            .as_ref()
            .map_err(Clone::clone)?
            .clone();
        Ok(NativeSymbol::with_data(
            native,
            Binding {
                function: bridge._library.try_symbol(symbol)?,
                bridge,
                owner: None,
            },
        ))
    }
    /// Use an explicitly supplied jump-boundary image.
    ///
    /// # Safety
    /// The image must implement this build's `native_abi/api.h` protocol and
    /// matching C signatures. All C functions used by a VM must share that image.
    pub unsafe fn from_library(path: &[u8]) -> Result<Self, Box<[u8]>> {
        let image = Arc::new(Library::try_open(path)?);
        let bridge = unsafe { Bridge::from_image(image) }?;
        Ok(Self {
            bridge: std::sync::OnceLock::from(Ok(bridge)),
        })
    }
    /// Snapshot process-wide counters for native boundary cleanup diagnostics.
    pub fn statistics() -> NativeCAbiStats {
        NativeCAbiStats {
            states_created: CREATED.load(Ordering::Relaxed),
            states_destroyed: DESTROYED.load(Ordering::Relaxed),
            groups_created: CREATED_GROUPS.load(Ordering::Relaxed),
            groups_destroyed: DESTROYED_GROUPS.load(Ordering::Relaxed),
            operations: OPERATIONS.load(Ordering::Relaxed),
            returned: DROPS.load(Ordering::Relaxed),
            libraries_opened: LIBRARIES_OPENED.load(Ordering::Relaxed),
            libraries_closed: LIBRARIES_CLOSED.load(Ordering::Relaxed),
        }
    }
}
/// Process-wide native adapter counters; snapshots are not atomic as a group.
#[derive(Clone, Copy, Debug)]
pub struct NativeCAbiStats {
    pub states_created: usize,
    pub states_destroyed: usize,
    pub groups_created: usize,
    pub groups_destroyed: usize,
    pub operations: usize,
    pub returned: usize,
    pub libraries_opened: usize,
    pub libraries_closed: usize,
}
impl Bridge {
    fn bundled() -> Result<Arc<Self>, Box<[u8]>> {
        let image = bundle::image()?;
        // SAFETY: image bytes and protocol version are produced by this build.
        unsafe { Self::from_image(image) }
    }
    unsafe fn from_image(image: Arc<Library>) -> Result<Arc<Self>, Box<[u8]>> {
        // SAFETY: the constructor's caller guarantees these protocol signatures.
        unsafe {
            Ok(Arc::new(Self {
                hook: std::mem::transmute::<usize, InvokeHook>(image.try_symbol(b"boundary_hook")?),
                invoke: std::mem::transmute::<usize, Invoke>(image.try_symbol(b"boundary_invoke")?),
                read: std::mem::transmute::<usize, Read>(image.try_symbol(b"boundary_read")?),
                write: std::mem::transmute::<usize, Write>(image.try_symbol(b"boundary_write")?),
                new_state: std::mem::transmute::<usize, NewState>(
                    image.try_symbol(b"boundary_state_new")?,
                ),
                free_state: std::mem::transmute::<usize, FreeState>(
                    image.try_symbol(b"boundary_state_free")?,
                ),
                state_bytes: std::mem::transmute::<usize, unsafe extern "C" fn(*mut c_void) -> usize>(
                    image.try_symbol(b"boundary_state_size")?,
                ),
                states: Mutex::new(std::collections::HashMap::new()),
                _library: image,
            }))
        }
    }
}

#[derive(Debug)]
struct Bridge {
    hook: InvokeHook,
    _library: Arc<Library>,
    invoke: Invoke,
    read: Read,
    write: Write,
    new_state: NewState,
    free_state: FreeState,
    state_bytes: unsafe extern "C" fn(*mut c_void) -> usize,
    states: Mutex<std::collections::HashMap<usize, ThreadId>>,
}
struct StateOwner {
    state: usize,
    bytes: usize,
    bridge: Arc<Bridge>,
}
struct Binding {
    function: usize,
    bridge: Arc<Bridge>,
    owner: Option<Arc<LoadedLibrary>>,
}
#[derive(Debug, Default)]
pub struct NativeCAbiLoader {
    bridge: std::sync::OnceLock<Result<Arc<Bridge>, Box<[u8]>>>,
}
#[derive(Debug)]
struct LoadedLibrary {
    library: Mutex<Option<Library>>,
    bridge: Arc<Bridge>,
}
impl NativeLibraryLoader for NativeCAbiLoader {
    fn open(&self, path: &[u8]) -> Result<Arc<dyn NativeLibrary>, Box<[u8]>> {
        let bridge = self
            .bridge
            .get_or_init(Bridge::bundled)
            .as_ref()
            .map_err(Clone::clone)?
            .clone();
        let library = Library::try_open(path)?;
        LIBRARIES_OPENED.fetch_add(1, Ordering::Relaxed);
        Ok(Arc::new(LoadedLibrary {
            library: Mutex::new(Some(library)),
            bridge,
        }))
    }
}
impl NativeLibrary for LoadedLibrary {
    fn handle(&self) -> usize {
        self.library.lock().unwrap().as_ref().map_or(0, |lib| lib.0)
    }
    fn close(&self) {
        if self.library.lock().unwrap().take().is_some() {
            LIBRARIES_CLOSED.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn symbol(self: Arc<Self>, name: &[u8]) -> Result<NativeSymbol, Box<[u8]>> {
        let function = self
            .library
            .lock()
            .unwrap()
            .as_ref()
            .expect("open library")
            .try_symbol(name)?;
        Ok(NativeSymbol::with_data(
            native,
            Binding {
                function,
                bridge: self.bridge.clone(),
                owner: Some(self),
            },
        ))
    }
}
impl Drop for LoadedLibrary {
    fn drop(&mut self) {
        self.close();
    }
}
impl HostThreadState for StateOwner {
    fn heap_bytes(&self) -> usize {
        self.bytes
    }
}
impl Drop for StateOwner {
    fn drop(&mut self) {
        assert!(
            self.bridge
                .states
                .lock()
                .unwrap()
                .remove(&self.state)
                .is_some()
        );
        // SAFETY: the VM-owned opaque token remains live through this drop;
        // the boundary verifies that no C frame still borrows the handle.
        let freed = unsafe { (self.bridge.free_state)(self.state as *mut c_void) };
        assert!(
            matches!(freed, 1 | 2),
            "C state was still executing during destruction"
        );
        if freed == 2 {
            DESTROYED_GROUPS.fetch_add(1, Ordering::Relaxed);
        }
        DESTROYED.fetch_add(1, Ordering::Relaxed);
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        assert!(self.states.lock().unwrap().is_empty());
    }
}
fn state_for(
    stack: &mut NativeStack<'_, '_>,
    bridge: &Arc<Bridge>,
    thread: ThreadId,
) -> Result<usize, Error> {
    if let Some(owner) = stack.thread_state::<StateOwner>(thread) {
        assert!(Arc::ptr_eq(&owner.bridge, bridge));
        return Ok(owner.state);
    }
    let identity = stack
        .object_pointer(Value::Thread(thread))
        .ok_or_else(|| stack.error(b"invalid Lua thread"))?;
    // SAFETY: this aligned VM-owned identity outlives the thread resource.
    // C treats it as an opaque key, never as a dereferenceable state layout.
    let main = stack.main_thread_id();
    let parent = if thread == main {
        0
    } else {
        state_for(stack, bridge, main)?
    };
    let state =
        unsafe { (bridge.new_state)(identity as *mut c_void, parent as *mut c_void) } as usize;
    if state == 0 {
        return Err(stack.error(b"cannot allocate boundary state"));
    }
    assert!(
        bridge
            .states
            .lock()
            .unwrap()
            .insert(state, thread)
            .is_none()
    );
    CREATED.fetch_add(1, Ordering::Relaxed);
    if parent == 0 {
        CREATED_GROUPS.fetch_add(1, Ordering::Relaxed);
    }
    stack.initialize_thread_state(
        thread,
        StateOwner {
            state,
            // SAFETY: this token was just created and is retained below.
            bytes: unsafe { (bridge.state_bytes)(state as *mut c_void) },
            bridge: bridge.clone(),
        },
    )?;
    Ok(state)
}
fn bridge_for(
    stack: &mut NativeStack<'_, '_>,
    requested: Arc<Bridge>,
) -> Result<Arc<Bridge>, Error> {
    let main = stack.main_thread_id();
    if let Some(owner) = stack.thread_state::<StateOwner>(main) {
        if owner.bridge._library.0 != requested._library.0 {
            return Err(stack.error(b"one Lua state cannot mix C API images"));
        }
        Ok(owner.bridge.clone())
    } else {
        state_for(stack, &requested, main)?;
        Ok(requested)
    }
}
static CREATED: AtomicUsize = AtomicUsize::new(0);
static CREATED_GROUPS: AtomicUsize = AtomicUsize::new(0);
static DESTROYED_GROUPS: AtomicUsize = AtomicUsize::new(0);
static LIBRARIES_OPENED: AtomicUsize = AtomicUsize::new(0);
static LIBRARIES_CLOSED: AtomicUsize = AtomicUsize::new(0);
static DESTROYED: AtomicUsize = AtomicUsize::new(0);
static OPERATIONS: AtomicUsize = AtomicUsize::new(0);
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Returned;
impl Drop for Returned {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}
struct Context<'stack, 'a, 'vm> {
    stack: &'stack mut NativeStack<'a, 'vm>,
    error: Option<Error>,
    yielded: Option<usize>,
    bridge: Arc<Bridge>,
    owner: Option<Arc<LoadedLibrary>>,
    state: usize,
}
impl Context<'_, '_, '_> {
    fn operation(
        &mut self,
        op: c_int,
        a: c_int,
        b: c_int,
        value: &mut BoundaryValue,
    ) -> Result<(), Error> {
        if op == 74 {
            self.stack.flush_process_streams();
            return Ok(());
        }
        if op == 47 {
            use crate::GcOperation;
            let operation = match a {
                0 => GcOperation::Stop,
                1 => GcOperation::Restart,
                2 => GcOperation::Collect,
                3 => GcOperation::CountKilobytes,
                4 => GcOperation::CountRemainder,
                5 => GcOperation::Step(b),
                6 => GcOperation::SetPause(b),
                7 => GcOperation::SetStepMultiplier(b),
                _ => {
                    value.integer = -1;
                    return Ok(());
                }
            };
            let thread = self
                .bridge
                .states
                .lock()
                .unwrap()
                .get(&(value.state as usize))
                .copied()
                .ok_or_else(|| self.stack.error(b"unknown collector thread"))?;
            value.integer = self.stack.gc_thread(thread, operation)?;
            return Ok(());
        }
        if matches!(op, 40 | 44 | 55) && value.state as usize != self.state {
            let thread = self
                .bridge
                .states
                .lock()
                .unwrap()
                .get(&(value.state as usize))
                .copied()
                .ok_or_else(|| self.stack.error(b"unknown execution thread"))?;
            if op == 55 {
                let child = self.stack.new_thread_on(thread)?;
                value.userdata = state_for(self.stack, &self.bridge, child)? as *mut c_void;
            } else {
                let count = usize::try_from(a)
                    .map_err(|_| self.stack.error(b"invalid concatenation count"))?;
                if op == 40 {
                    self.stack.concatenate_thread(thread, count)?;
                } else {
                    self.stack.concatenate_primitives_thread(thread, count)?;
                }
            }
            return Ok(());
        }
        if op == 73 {
            let thread = self
                .bridge
                .states
                .lock()
                .unwrap()
                .get(&(value.state as usize))
                .copied()
                .ok_or_else(|| self.stack.error(b"unknown collector thread"))?;
            return self.stack.check_thread_gc(thread);
        }
        if op == 70 {
            let thread = self
                .bridge
                .states
                .lock()
                .unwrap()
                .get(&(value.state as usize))
                .copied()
                .ok_or_else(|| self.stack.error(b"unknown function thread"))?;
            let function = if thread == self.stack.thread_id() {
                self.stack.value(a as isize)
            } else {
                self.stack.thread_stack(thread)?.value(a as isize)
            };
            value.integer = -1;
            value.userdata = std::ptr::null_mut();
            if let Some(function) = function {
                if let Some(binding) = self.stack.host_function_data::<Binding>(function) {
                    value.userdata = binding.function as *mut c_void;
                } else if let Some(body) = self.stack.builtin_body_of(function) {
                    value.integer = body.implementation_code() as c_int;
                }
            }
            return Ok(());
        }
        if (65..=68).contains(&op) {
            let thread = self
                .bridge
                .states
                .lock()
                .unwrap()
                .get(&(value.state as usize))
                .copied()
                .ok_or_else(|| self.stack.error(b"unknown hook thread"))?;
            if op == 65 {
                let function = if value.userdata.is_null() || a == 0 {
                    NativeHook::None
                } else if value.integer != 0 {
                    NativeHook::Lua
                } else {
                    self.stack.push_closure(
                        native_hook,
                        0,
                        Binding {
                            function: value.userdata as usize,
                            bridge: self.bridge.clone(),
                            owner: self.owner.clone(),
                        },
                    )?;
                    NativeHook::Host(self.stack.value(-1).unwrap())
                };
                self.stack.set_hook(thread, function, a, b)?;
                if matches!(function, NativeHook::Host(_)) {
                    self.stack.set_top(self.stack.top() - 1)?;
                }
            } else {
                let info = self.stack.hook(thread)?;
                if op == 66 {
                    value.integer = c_int::from(info.function == NativeHook::Lua);
                    value.userdata = match info.function {
                        NativeHook::Host(f) => {
                            self.stack
                                .host_function_data::<Binding>(f)
                                .expect("C hook binding")
                                .function as *mut c_void
                        }
                        _ => std::ptr::null_mut(),
                    };
                } else {
                    value.integer = if op == 67 {
                        info.mask as c_int
                    } else {
                        info.count
                    };
                }
            }
            return Ok(());
        }
        if op == 69 {
            value.integer = c_int::from(self.stack.push_debug_hook()?);
            return Ok(());
        }
        if (61..=64).contains(&op) {
            let thread = self
                .bridge
                .states
                .lock()
                .unwrap()
                .get(&(value.state as usize))
                .copied()
                .ok_or_else(|| self.stack.error(b"unknown debug thread"))?;
            // SAFETY: these fixture operations lend a live lua_Debug with
            // the matching repr(C) layout for this synchronous dispatch.
            let ar = unsafe { &mut *value.userdata.cast::<DebugRecord>() };
            if op == 61 {
                let frame = self.stack.get_stack(thread, a)?;
                value.integer = c_int::from(frame.is_some());
                if let Some(frame) = frame {
                    ar.i_ci = frame.index();
                }
            } else {
                let frame = NativeDebugFrame::from_index(thread, ar.i_ci);
                if op == 62 {
                    // SAFETY: lua_getinfo lends a NUL-terminated option string.
                    let options = unsafe { CStr::from_ptr(value.bytes) }.to_bytes();
                    let info = self.stack.get_info(frame, options)?;
                    value.integer = c_int::from(info.valid);
                    if let Some(source) = info.source {
                        ar.source = self.stack.debug_bytes(source).unwrap().as_ptr().cast();
                    }
                    if let Some(what) = info.what {
                        ar.what = what.as_ptr().cast();
                    }
                    if let Some([first, last]) = info.defined {
                        ar.linedefined = first;
                        ar.lastlinedefined = last;
                    }
                    if let Some(short) = info.short_source {
                        ar.short_src[..short.len()].copy_from_slice(&short);
                    }
                    if let Some(line) = info.current_line {
                        ar.currentline = line;
                    }
                    if let Some(n) = info.upvalues {
                        ar.nups = n as c_int;
                    }
                    if let Some(name) = info.name {
                        ar.name = name
                            .and_then(|n| self.stack.debug_bytes(n))
                            .map_or(std::ptr::null(), |b| b.as_ptr().cast());
                    }
                    if let Some(kind) = info.name_what {
                        ar.namewhat = kind.as_ptr().cast();
                    }
                } else {
                    let name = if op == 63 {
                        self.stack.get_local(frame, a)?
                    } else {
                        self.stack.set_local(frame, a)?
                    };
                    value.bytes = name
                        .and_then(|n| self.stack.debug_bytes(n))
                        .map_or(std::ptr::null(), |b| b.as_ptr().cast());
                }
            }
            return Ok(());
        }
        if op == 60 {
            let thread = self
                .bridge
                .states
                .lock()
                .unwrap()
                .get(&(value.state as usize))
                .copied()
                .ok_or_else(|| self.stack.error(b"unknown level thread"))?;
            value.integer = self
                .stack
                .thread_call_level(thread)
                .ok_or_else(|| self.stack.error(b"unknown level thread"))?
                as c_int;
            return Ok(());
        }
        if op == 57 {
            let thread = self
                .bridge
                .states
                .lock()
                .unwrap()
                .get(&(value.state as usize))
                .copied()
                .ok_or_else(|| self.stack.error(b"unknown target thread"))?;
            let count = usize::try_from(a)
                .map_err(|_| self.stack.error(b"invalid resume argument count"))?;
            value.integer = self.stack.resume_thread(thread, count)? as c_int;
            return Ok(());
        }
        if op == 56 || op == 59 {
            let states = self.bridge.states.lock().unwrap();
            let from = *states
                .get(&(value.state as usize))
                .ok_or_else(|| self.stack.error(b"unknown source thread"))?;
            let to = *states
                .get(&(value.userdata as usize))
                .ok_or_else(|| self.stack.error(b"unknown destination thread"))?;
            drop(states);
            if op == 59 {
                return self.stack.copy_thread_call_level(from, to);
            }
            if from == to {
                return Ok(());
            }
            let count = usize::try_from(a).map_err(|_| self.stack.error(b"invalid move count"))?;
            return self.stack.move_thread_values(from, to, count);
        }
        if op == 49 {
            // SAFETY: luaL_loadfile lends an optional C-string path; copy it
            // before selecting the target or running collector callbacks.
            let path = if value.bytes.is_null() {
                None
            } else {
                Some(unsafe { CStr::from_ptr(value.bytes) }.to_bytes().to_vec())
            };
            let thread = self
                .bridge
                .states
                .lock()
                .unwrap()
                .get(&(value.state as usize))
                .copied()
                .ok_or_else(|| self.stack.error(b"unknown file-loading thread"))?;
            value.integer = self.stack.load_file_thread(thread, path.as_deref())? as c_int;
            return Ok(());
        }
        if op == 48 {
            let reader = value
                .reader
                .ok_or_else(|| self.stack.error(b"missing C reader"))?;
            // SAFETY: lua_load supplies an optional terminated name for this call.
            let name = if value.bytes.is_null() {
                None
            } else {
                Some(
                    unsafe { std::ffi::CStr::from_ptr(value.bytes) }
                        .to_bytes()
                        .to_vec(),
                )
            };
            let data = value.userdata;
            let bridge = self.bridge.clone();
            let owner = self.owner.clone();
            let state = value.state as usize;
            let thread = self
                .bridge
                .states
                .lock()
                .unwrap()
                .get(&state)
                .copied()
                .ok_or_else(|| self.stack.error(b"unknown reader thread"))?;
            let status = self
                .stack
                .load_reader_thread(thread, name.as_deref(), |stack| {
                    let mut context = Context {
                        stack,
                        error: None,
                        yielded: None,
                        bridge: bridge.clone(),
                        owner: owner.clone(),
                        state,
                    };
                    let mut output = BoundaryValue::default();
                    // SAFETY: the C-only reader boundary borrows this reborrowed
                    // stack until it returns, and jumps only after dispatch exits.
                    let result = unsafe {
                        (bridge.read)(
                            state as *mut c_void,
                            (&mut context as *mut Context<'_, '_, '_>).cast(),
                            dispatch,
                            reader,
                            data,
                            &mut output,
                        )
                    };
                    if let Some(error) = context.error.take() {
                        return Err(error);
                    }
                    if result != 0 {
                        return Err(context.stack.error(b"C reader failed without an error"));
                    }
                    if output.bytes.is_null() || output.length == 0 {
                        return Ok(None);
                    }
                    // SAFETY: the reader contract keeps this block live until
                    // its next invocation; copy it before any Lua reentry.
                    let bytes =
                        unsafe { std::slice::from_raw_parts(output.bytes.cast(), output.length) };
                    Ok(Some(bytes.to_vec()))
                })?;
            value.integer = status as c_int;
            return Ok(());
        }
        if value.state as usize != self.state {
            let thread = self
                .bridge
                .states
                .lock()
                .unwrap()
                .get(&(value.state as usize))
                .copied()
                .ok_or_else(|| self.stack.error(b"unknown target thread"))?;
            return self.inactive_operation(thread, op, a, b, value);
        }
        match op {
            71 => {
                let body = crate::NativeBuiltinBody::from_implementation_code(a as u64)
                    .ok_or_else(|| self.stack.error(b"invalid builtin implementation"))?;
                value.integer = self.stack.call_builtin_body(body)? as c_int;
                if value.integer < 0 {
                    self.yielded = Some(self.stack.top());
                }
            }
            58 => {
                let count = usize::try_from(a)
                    .map_err(|_| self.stack.error(b"invalid yield result count"))?;
                if let Err(error) = self.stack.check_yield(count) {
                    let value = self.stack.intern(error.message_bytes());
                    return Err(self.stack.raise(value));
                }
                self.yielded = Some(count);
            }
            55 => {
                let thread = self.stack.new_thread()?;
                value.userdata = state_for(self.stack, &self.bridge, thread)? as *mut c_void;
            }
            54 => {
                value.userdata = self
                    .stack
                    .to_pointer(a as isize)
                    .map_or(std::ptr::null_mut(), |p| p as *mut c_void)
            }
            51 => {
                value.userdata = match self.stack.to_thread(a as isize) {
                    Some(thread) => state_for(self.stack, &self.bridge, thread)? as *mut c_void,
                    None => std::ptr::null_mut(),
                };
            }
            52 => value.integer = c_int::from(self.stack.push_thread()?),
            53 => {
                let thread = self
                    .bridge
                    .states
                    .lock()
                    .unwrap()
                    .get(&(value.userdata as usize))
                    .copied();
                let status = thread.and_then(|thread| self.stack.thread_status(thread));
                value.integer =
                    status.ok_or_else(|| self.stack.error(b"unknown C state"))? as c_int;
            }
            50 => {
                let writer = value.writer;
                let data = value.userdata;
                let bridge = self.bridge.clone();
                let owner = self.owner.clone();
                let state = self.state;
                value.integer = self.stack.dump(|stack, bytes| {
                    let writer = writer.ok_or_else(|| stack.error(b"missing C writer"))?;
                    let mut context = Context {
                        stack,
                        error: None,
                        yielded: None,
                        bridge: bridge.clone(),
                        owner: owner.clone(),
                        state,
                    };
                    let mut output = BoundaryValue::default();
                    // Empty native vectors have NULL storage in PUC Lua.
                    let pointer = if bytes.is_empty() {
                        std::ptr::null()
                    } else {
                        bytes.as_ptr().cast()
                    };
                    // SAFETY: bytes and the reborrowed Context remain valid
                    // until this C-only boundary returns. Dispatch exits
                    // before any reader/writer error jumps within C.
                    let result = unsafe {
                        (bridge.write)(
                            state as *mut c_void,
                            (&mut context as *mut Context<'_, '_, '_>).cast(),
                            dispatch,
                            writer,
                            data,
                            pointer,
                            bytes.len(),
                            &mut output,
                        )
                    };
                    if let Some(error) = context.error.take() {
                        return Err(error);
                    }
                    if result != 0 {
                        return Err(context.stack.error(b"C writer failed without an error"));
                    }
                    Ok(output.integer)
                })?;
            }
            0 => value.integer = self.stack.top_offset() as c_int,
            1 => self.stack.push(Value::Number(value.number))?,
            2 => self.stack.push(Value::Boolean(value.integer != 0))?,
            3 => self.stack.push(Value::Nil)?,
            4 => {
                // SAFETY: lua_pushlstring lends this buffer until dispatch returns.
                let bytes = if value.length == 0 {
                    &[]
                } else {
                    unsafe { std::slice::from_raw_parts(value.bytes.cast(), value.length) }
                };
                let string = self.stack.intern(bytes);
                self.stack.push(string)?;
            }
            5 => self
                .stack
                .push(self.stack.value(a as isize).unwrap_or(Value::Nil))?,
            6 => value.number = self.stack.number(a as isize)?,
            7 => {
                let value = self.stack.value(-1).unwrap_or(Value::Nil);
                if self.stack.top() != 0 {
                    self.stack.set_top(self.stack.top() - 1)?;
                }
                // Lua's handler call replaces the top error slot with its
                // function. That slot is not a local of the failing C frame.
                return Err(self.stack.raise(value));
            }
            8 => {
                let count = usize::try_from(a)
                    .map_err(|_| self.stack.error(b"invalid probe call count"))?;
                self.stack
                    .call_unprotected(count, (b >= 0).then_some(b as usize))?;
            }
            9 => value.integer = c_int::from(a < 0 || self.stack.check_stack(a as usize)),
            10 => {
                let top = if a >= 0 {
                    a as isize
                } else {
                    self.stack.top_offset() + a as isize + 1
                };
                let top =
                    usize::try_from(top).map_err(|_| self.stack.error(b"invalid stack top"))?;
                self.stack.set_top(top)?;
            }
            11 => {
                let value = self
                    .stack
                    .value(-1)
                    .ok_or_else(|| self.stack.error(b"empty replacement stack"))?;
                self.stack.set(a as isize, value)?;
                self.stack.set_top(self.stack.top() - 1)?;
            }
            12 => self
                .stack
                .push(Value::LightUserdata(value.bytes as usize as u64))?,
            13 => {
                value.bytes = match self.stack.value(a as isize) {
                    Some(Value::LightUserdata(pointer)) => pointer as usize as *const c_char,
                    Some(Value::Userdata(_)) => self
                        .stack
                        .userdata_bytes_mut(a as isize)?
                        .unwrap()
                        .as_mut_ptr()
                        .cast(),
                    _ => std::ptr::null(),
                }
            }
            14 => {
                let count =
                    usize::try_from(a).map_err(|_| self.stack.error(b"invalid C capture count"))?;
                self.stack.push_closure(
                    native,
                    count,
                    Binding {
                        function: value.bytes as usize,
                        bridge: self.bridge.clone(),
                        owner: self.owner.clone(),
                    },
                )?;
            }
            15 => {
                let array =
                    usize::try_from(a).map_err(|_| self.stack.error(b"invalid array capacity"))?;
                let records =
                    usize::try_from(b).map_err(|_| self.stack.error(b"invalid record capacity"))?;
                self.stack.create_table(array, records)?;
            }
            16 => self.stack.get_table(a as isize, b != 0)?,
            17 => self.stack.set_table(a as isize, b != 0)?,
            18 | 19 => {
                // SAFETY: getfield/setfield lend a string for this dispatch.
                let name = unsafe { std::slice::from_raw_parts(value.bytes.cast(), value.length) };
                if op == 18 {
                    self.stack.get_field(a as isize, name)?;
                } else {
                    self.stack.set_field(a as isize, name)?;
                }
            }
            20 => self.stack.raw_get_integer(a as isize, b)?,
            21 => self.stack.raw_set_integer(a as isize, b)?,
            22 => value.integer = c_int::from(self.stack.get_metatable(a as isize)?),
            23 => self.stack.set_metatable(a as isize)?,
            24 => value.integer = c_int::from(self.stack.next_table(a as isize)?),
            29 => self.stack.new_userdata(value.length)?,
            30 => value.length = self.stack.object_len(a as isize)?,
            31 => match self.stack.to_c_bytes(a as isize)? {
                Some(bytes) => {
                    value.bytes = bytes.as_ptr().cast();
                    value.length = bytes.len() - 1;
                }
                None => {
                    value.bytes = std::ptr::null();
                    value.length = 0;
                }
            },
            32 => self.stack.get_environment(a as isize)?,
            33 => value.integer = c_int::from(self.stack.set_environment(a as isize)?),
            34 => {
                let arguments = usize::try_from(a)
                    .map_err(|_| self.stack.error(b"invalid protected call count"))?;
                let handler = (value.integer != 0).then_some(value.integer as isize);
                value.integer =
                    self.stack
                        .protected_call(arguments, (b >= 0).then_some(b as usize), handler)?
                        as c_int;
            }
            35 => {
                let number = self.stack.to_number(a as isize);
                value.integer = c_int::from(number.is_some());
                value.number = number.unwrap_or(0.);
            }
            36 => value.wide_integer = self.stack.to_integer(a as isize).unwrap_or(0),
            37 => value.integer = c_int::from(self.stack.to_boolean(a as isize)),
            38 => self.stack.push_integer(value.wide_integer)?,
            39 => {
                value.integer = c_int::from(if value.integer == 0 {
                    self.stack.equal(a as isize, b as isize)?
                } else {
                    self.stack.less_than(a as isize, b as isize)?
                });
            }
            40 => {
                let count =
                    usize::try_from(a).map_err(|_| self.stack.error(b"invalid concat count"))?;
                self.stack.concatenate(count)?;
            }
            41 => {
                value.integer = c_int::from(matches!(
                    self.stack.value(a as isize),
                    Some(Value::Native(_))
                ))
            }
            42 => {
                // SAFETY: the C auxiliary helper lends this message until return.
                let bytes = unsafe { std::slice::from_raw_parts(value.bytes.cast(), value.length) };
                let error = self.stack.argument_error(a as isize, bytes);
                let message = self.stack.intern(error.message_bytes());
                self.stack.push(message)?;
            }
            43 => self.stack.push_where(a as isize)?,
            44 => {
                let count = usize::try_from(a)
                    .map_err(|_| self.stack.error(b"invalid format-part count"))?;
                self.stack.concatenate_primitives(count)?;
            }
            45 | 46 => {
                let number = usize::try_from(b).unwrap_or(0);
                let name = if op == 45 {
                    self.stack.get_upvalue(a as isize, number)?
                } else {
                    self.stack.set_upvalue(a as isize, number)?
                };
                value.bytes = name.map_or(std::ptr::null(), |bytes| bytes.as_ptr().cast());
            }
            25 => {
                value.integer = match self.stack.value(a as isize) {
                    None => -1,
                    Some(Value::Nil) => 0,
                    Some(Value::Boolean(_)) => 1,
                    Some(Value::LightUserdata(_)) => 2,
                    Some(Value::Number(_)) => 3,
                    Some(Value::String(_)) => 4,
                    Some(Value::Table(_)) => 5,
                    Some(Value::Function(_) | Value::Native(_)) => 6,
                    Some(Value::Userdata(_)) => 7,
                    Some(Value::Thread(_)) => 8,
                    Some(Value::ParserPrototype(_)) => 9,
                };
            }
            26 => {
                value.integer = c_int::from(matches!(
                    (self.stack.value(a as isize), self.stack.value(b as isize)),
                    (Some(first), Some(second)) if first == second
                ));
            }
            27 => self.stack.remove(a as isize)?,
            28 => self.stack.insert(a as isize)?,
            _ => return Err(self.stack.error(b"unknown boundary-probe operation")),
        }
        Ok(())
    }
    fn inactive_operation(
        &mut self,
        thread: ThreadId,
        op: c_int,
        a: c_int,
        b: c_int,
        value: &mut BoundaryValue,
    ) -> Result<(), Error> {
        if matches!(op, 18 | 19 | 39) || (matches!(op, 16 | 17) && b == 0) {
            match op {
                16 => self.stack.get_table_thread(thread, a as isize, b != 0)?,
                17 => self.stack.set_table_thread(thread, a as isize, b != 0)?,
                18 | 19 => {
                    // SAFETY: field APIs lend these bytes until dispatch returns.
                    let name =
                        unsafe { std::slice::from_raw_parts(value.bytes.cast(), value.length) };
                    if op == 18 {
                        self.stack.get_field_thread(thread, a as isize, name)?;
                    } else {
                        self.stack.set_field_thread(thread, a as isize, name)?;
                    }
                }
                39 => {
                    value.integer = c_int::from(if value.integer == 0 {
                        self.stack.equal_thread(thread, a as isize, b as isize)?
                    } else {
                        self.stack
                            .less_than_thread(thread, a as isize, b as isize)?
                    });
                }
                _ => unreachable!(),
            }
            return Ok(());
        }
        if op == 8 {
            let arguments =
                usize::try_from(a).map_err(|_| self.stack.error(b"invalid native call count"))?;
            self.stack.call_unprotected_thread(
                thread,
                arguments,
                (b >= 0).then_some(b as usize),
            )?;
            return Ok(());
        }
        if op == 34 {
            let arguments = usize::try_from(a)
                .map_err(|_| self.stack.error(b"invalid protected call count"))?;
            let handler = (value.integer != 0).then_some(value.integer as isize);
            value.integer = self.stack.protected_call_thread(
                thread,
                arguments,
                (b >= 0).then_some(b as usize),
                handler,
            )? as c_int;
            return Ok(());
        }
        if op == 51 {
            let target = self.stack.thread_stack(thread)?.to_thread(a as isize);
            value.userdata = match target {
                Some(id) => state_for(self.stack, &self.bridge, id)? as *mut c_void,
                None => std::ptr::null_mut(),
            };
            return Ok(());
        }
        let mut stack = self.stack.thread_stack(thread)?;
        match op {
            0 => value.integer = stack.top_offset() as c_int,
            1 => stack.push(Value::Number(value.number))?,
            2 => stack.push(Value::Boolean(value.integer != 0))?,
            3 => stack.push(Value::Nil)?,
            4 => {
                // SAFETY: the C primitive lends these bytes until dispatch returns.
                let bytes = if value.length == 0 {
                    &[]
                } else {
                    unsafe { std::slice::from_raw_parts(value.bytes.cast(), value.length) }
                };
                let v = stack.intern(bytes);
                stack.push(v)?;
            }
            5 => {
                let v = stack.value(a as isize).unwrap_or(Value::Nil);
                stack.push(v)?;
            }
            6 => {
                value.number = stack
                    .to_number(a as isize)
                    .ok_or_else(|| stack.error(b"number expected"))?
            }
            9 => value.integer = c_int::from(a < 0 || stack.check_stack(a as usize)),
            10 => {
                let top = if a >= 0 {
                    a as isize
                } else {
                    stack.top_offset() + a as isize + 1
                };
                let top = usize::try_from(top).map_err(|_| stack.error(b"invalid stack top"))?;
                stack.set_top(top)?;
            }
            11 => {
                let v = stack
                    .value(-1)
                    .ok_or_else(|| stack.error(b"empty replacement stack"))?;
                stack.set(a as isize, v)?;
                stack.set_top(stack.top() - 1)?;
            }
            12 => stack.push(Value::LightUserdata(value.bytes as usize as u64))?,
            13 => {
                value.bytes = stack
                    .userdata_pointer(a as isize)
                    .map_or(std::ptr::null(), |p| p as *const c_char)
            }
            14 => {
                let count =
                    usize::try_from(a).map_err(|_| stack.error(b"invalid C capture count"))?;
                stack.push_closure(
                    native,
                    count,
                    Binding {
                        function: value.bytes as usize,
                        bridge: self.bridge.clone(),
                        owner: self.owner.clone(),
                    },
                )?;
            }
            15 => {
                let array =
                    usize::try_from(a).map_err(|_| stack.error(b"invalid array capacity"))?;
                let records =
                    usize::try_from(b).map_err(|_| stack.error(b"invalid record capacity"))?;
                stack.create_table(array, records)?;
            }
            16 if b != 0 => stack.raw_get(a as isize)?,
            17 if b != 0 => stack.raw_set(a as isize)?,
            20 => stack.raw_get_integer(a as isize, b)?,
            21 => stack.raw_set_integer(a as isize, b)?,
            22 => value.integer = c_int::from(stack.get_metatable(a as isize)?),
            23 => stack.set_metatable(a as isize)?,
            24 => value.integer = c_int::from(stack.next_table(a as isize)?),
            29 => stack.new_userdata(value.length)?,
            32 => stack.get_environment(a as isize)?,
            33 => value.integer = c_int::from(stack.set_environment(a as isize)?),
            45 | 46 => {
                let number = usize::try_from(b).unwrap_or(0);
                let name = if op == 45 {
                    stack.get_upvalue(a as isize, number)?
                } else {
                    stack.set_upvalue(a as isize, number)?
                };
                value.bytes = name.map_or(std::ptr::null(), |bytes| bytes.as_ptr().cast());
            }
            25 => {
                value.integer = match stack.value(a as isize) {
                    None => -1,
                    Some(Value::Nil) => 0,
                    Some(Value::Boolean(_)) => 1,
                    Some(Value::LightUserdata(_)) => 2,
                    Some(Value::Number(_)) => 3,
                    Some(Value::String(_)) => 4,
                    Some(Value::Table(_)) => 5,
                    Some(Value::Function(_) | Value::Native(_)) => 6,
                    Some(Value::Userdata(_)) => 7,
                    Some(Value::Thread(_)) => 8,
                    Some(Value::ParserPrototype(_)) => 9,
                }
            }
            26 => {
                value.integer = c_int::from(
                    matches!((stack.value(a as isize),stack.value(b as isize)),(Some(x),Some(y))if x==y),
                )
            }
            27 => stack.remove(a as isize)?,
            28 => stack.insert(a as isize)?,
            30 => value.length = stack.object_len(a as isize)?,
            31 => match stack.to_c_bytes(a as isize)? {
                Some(bytes) => {
                    value.bytes = bytes.as_ptr().cast();
                    value.length = bytes.len() - 1;
                }
                None => {
                    value.bytes = std::ptr::null();
                    value.length = 0;
                }
            },
            35 => {
                let n = stack.to_number(a as isize);
                value.integer = c_int::from(n.is_some());
                value.number = n.unwrap_or(0.);
            }
            36 => value.wide_integer = stack.to_integer(a as isize).unwrap_or(0),
            37 => {
                value.integer = c_int::from(stack.value(a as isize).is_some_and(Value::is_truthy))
            }
            38 => stack.push(Value::Number(value.wide_integer as f64))?,
            41 => {
                value.integer =
                    c_int::from(matches!(stack.value(a as isize), Some(Value::Native(_))))
            }
            52 => value.integer = c_int::from(stack.push_thread()?),
            54 => {
                value.userdata = stack
                    .to_pointer(a as isize)
                    .map_or(std::ptr::null_mut(), |p| p as *mut c_void)
            }
            _ => return Err(stack.error(b"inactive operation requires execution support")),
        }
        Ok(())
    }
}
unsafe extern "C" fn dispatch(
    raw: *mut c_void,
    op: c_int,
    a: c_int,
    b: c_int,
    value: *mut BoundaryValue,
) -> c_int {
    OPERATIONS.fetch_add(1, Ordering::Relaxed);
    let _returned = Returned;
    // SAFETY: boundary_invoke borrows this unique Context for this call.
    // C jumps only after this function returns, including its Drop guard.
    let context = unsafe { &mut *raw.cast::<Context<'_, '_, '_>>() };
    let value = unsafe { &mut *value };
    if matches!(op, 7 | 72) && value.state as usize != context.state {
        let thread = context
            .bridge
            .states
            .lock()
            .unwrap()
            .get(&(value.state as usize))
            .copied();
        let outcome = thread
            .ok_or_else(|| context.stack.error(b"unknown error thread"))
            .and_then(|thread| {
                if op == 7
                    && let Some(error) = context.stack.waiting_thread_error(thread)?
                {
                    return Err(error);
                }
                context
                    .stack
                    .prepare_thread_panic(thread, value.integer != 0)
            });
        return match outcome {
            Ok(()) => 2,
            Err(error) => {
                context.error = Some(error);
                1
            }
        };
    }
    match context.operation(op, a, b, value) {
        Ok(()) => 0,
        Err(error) => {
            if error.target_thread.is_none()
                && matches!(op, 8 | 16..=19 | 39 | 40 | 44 | 47 | 49 | 55 | 73)
                && value.state as usize != context.state
                && error.kind() == crate::ErrorKind::Runtime
            {
                let thread = context
                    .bridge
                    .states
                    .lock()
                    .unwrap()
                    .get(&(value.state as usize))
                    .copied();
                if let Some(thread) = thread {
                    if let Some(error) = context.stack.route_waiting_error(thread, &error) {
                        context.error = Some(error);
                        return 1;
                    }
                    match context.stack.push_thread_panic_error(thread, &error) {
                        // C rereads lua_atpanic after any finalizer callback,
                        // then dispatches the stack reset before calling it.
                        Ok(()) => return 3,
                        Err(error) => {
                            context.error = Some(error);
                            return 1;
                        }
                    }
                }
            }
            context.error = Some(error);
            1
        }
    }
}
fn native(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call.function_data::<Binding>().expect("C function binding");
    let function = binding.function;
    let bridge = binding.bridge.clone();
    let owner = binding.owner.clone();
    if owner.as_ref().is_some_and(|owner| owner.handle() == 0) {
        return Err(call.error(b"native library is closed"));
    }
    // SAFETY: bindings come from trusted symbols or lua_pushcclosure,
    // both of which provide lua_CFunction's C signature.
    let function: CFunction = unsafe { std::mem::transmute(function) };
    let invoke = bridge.invoke;
    let mut stack = call.native_stack()?;
    let bridge = bridge_for(&mut stack, bridge)?;
    let thread = stack.thread_id();
    let state = state_for(&mut stack, &bridge, thread)? as *mut c_void;
    let mut context = Context {
        stack: &mut stack,
        error: None,
        yielded: None,
        bridge,
        owner,
        state: state as usize,
    };
    // SAFETY: trusted C functions borrow the Context only until return.
    // setjmp/longjmp and every skipped frame are within the C DLLs.
    let count = unsafe {
        invoke(
            state,
            (&mut context as *mut Context<'_, '_, '_>).cast(),
            dispatch,
            function,
        )
    };
    if let Some(error) = context.error.take() {
        return Err(error);
    }
    let yielded = context.yielded;
    drop(context);
    if let Some(count) = yielded {
        return stack.yield_results(count);
    }
    let count =
        usize::try_from(count).map_err(|_| stack.error(b"invalid boundary-probe result count"))?;
    stack.finish(count)
}
fn native_hook(call: &mut HostCall<'_>) -> Result<(), Error> {
    let event = call.hook_event().expect("native hook event");
    let binding = call.function_data::<Binding>().expect("C hook binding");
    let function = binding.function;
    let bridge = binding.bridge.clone();
    let owner = binding.owner.clone();
    let invoke = bridge.hook;
    // SAFETY: only the trusted lua_sethook fixture supplies this binding.
    let function: Hook = unsafe { std::mem::transmute(function) };
    let mut stack = call.native_stack()?;
    let bridge = bridge_for(&mut stack, bridge)?;
    let thread = stack.thread_id();
    let state = state_for(&mut stack, &bridge, thread)? as *mut c_void;
    let mut record = DebugRecord {
        event: event.event,
        currentline: event.current_line,
        i_ci: event.frame.index(),
        name: std::ptr::null(),
        namewhat: std::ptr::null(),
        what: std::ptr::null(),
        source: std::ptr::null(),
        nups: 0,
        linedefined: 0,
        lastlinedefined: 0,
        short_src: [0; 60],
    };
    let mut context = Context {
        stack: &mut stack,
        error: None,
        yielded: None,
        bridge,
        owner,
        state: state as usize,
    };
    // SAFETY: the C-only jump boundary returns after all Rust dispatchers.
    unsafe {
        invoke(
            state,
            (&mut context as *mut Context<'_, '_, '_>).cast(),
            dispatch,
            function,
            &mut record,
        )
    };
    if let Some(error) = context.error.take() {
        return Err(error);
    }
    let yielded = context.yielded;
    drop(context);
    if let Some(count) = yielded {
        return stack.yield_results(count);
    }
    Ok(())
}
