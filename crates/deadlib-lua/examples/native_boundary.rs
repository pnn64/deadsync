//! Development proof of a C-only error jump around the Rust host-call API.
//! Not the production Lua C API: states follow Lua threads and
//! this small fixture supports only the operations in tests/native-boundary.
//! Temporary values use the VM-owned mutable native stack.
#[cfg(windows)]
mod windows {
    use deadlib_lua::{
        Error, HostCall, HostThreadState, NativeDebugFrame, NativeStack, ThreadId, Value, Vm,
        compile_named,
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
    type Dispatch =
        unsafe extern "C" fn(*mut c_void, c_int, c_int, c_int, *mut BoundaryValue) -> c_int;
    type CFunction = unsafe extern "C" fn(*mut c_void) -> c_int;
    type Invoke = unsafe extern "C" fn(*mut c_void, *mut c_void, Dispatch, CFunction) -> c_int;
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
    type NewState = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
    type FreeState = unsafe extern "C" fn(*mut c_void) -> c_int;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> c_int;
    }
    struct Library(usize);
    impl Library {
        fn open(path: &str) -> Self {
            let path = CString::new(path).unwrap();
            // SAFETY: the development runner supplies the two trusted fixture DLLs.
            let module = unsafe { LoadLibraryA(path.as_ptr()) };
            assert!(
                !module.is_null(),
                "load fixture: {}",
                std::io::Error::last_os_error()
            );
            Self(module as usize)
        }
        fn symbol(&self, name: &str) -> usize {
            let name = CString::new(name).unwrap();
            // SAFETY: the module is live and the symbol name is terminated.
            let pointer = unsafe { GetProcAddress(self.0 as *mut c_void, name.as_ptr()) };
            assert!(!pointer.is_null(), "missing fixture symbol {name:?}");
            pointer as usize
        }
    }
    impl Drop for Library {
        fn drop(&mut self) {
            // SAFETY: this owner holds exactly one LoadLibrary reference.
            unsafe {
                FreeLibrary(self.0 as *mut c_void);
            }
        }
    }

    struct Bridge {
        module: Library,
        _library: Library,
        invoke: Invoke,
        read: Read,
        write: Write,
        new_state: NewState,
        free_state: FreeState,
        state_bytes: usize,
        states: Mutex<std::collections::HashMap<usize, ThreadId>>,
    }
    struct StateOwner {
        state: usize,
        bridge: Arc<Bridge>,
    }
    struct Binding {
        function: usize,
        bridge: Arc<Bridge>,
    }
    impl HostThreadState for StateOwner {
        fn heap_bytes(&self) -> usize {
            self.bridge.state_bytes
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
            assert_eq!(freed, 1, "C state was still executing during destruction");
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
        let state = unsafe { (bridge.new_state)(identity as *mut c_void) } as usize;
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
        stack.initialize_thread_state(
            thread,
            StateOwner {
                state,
                bridge: bridge.clone(),
            },
        )?;
        Ok(state)
    }
    static CREATED: AtomicUsize = AtomicUsize::new(0);
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
                let count =
                    usize::try_from(a).map_err(|_| self.stack.error(b"invalid move count"))?;
                return self.stack.move_thread_values(from, to, count);
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
                    let state = self.state;
                    value.integer = self.stack.dump(|stack, bytes| {
                        let writer = writer.ok_or_else(|| stack.error(b"missing C writer"))?;
                        let mut context = Context {
                            stack,
                            error: None,
                            yielded: None,
                            bridge: bridge.clone(),
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
                49 => {
                    // SAFETY: luaL_loadfile lends an optional C-string path;
                    // copy it before loading can reenter Lua and mutate slots.
                    let path = if value.bytes.is_null() {
                        None
                    } else {
                        Some(
                            unsafe { std::ffi::CStr::from_ptr(value.bytes) }
                                .to_bytes()
                                .to_vec(),
                        )
                    };
                    value.integer = self.stack.load_file(path.as_deref())? as c_int;
                }
                48 => {
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
                    let state = self.state;
                    let status = self.stack.load_reader(name.as_deref(), |stack| {
                        let mut context = Context {
                            stack,
                            error: None,
                            yielded: None,
                            bridge: bridge.clone(),
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
                        let bytes = unsafe {
                            std::slice::from_raw_parts(output.bytes.cast(), output.length)
                        };
                        Ok(Some(bytes.to_vec()))
                    })?;
                    value.integer = status as c_int;
                }
                0 => value.integer = self.stack.top() as c_int,
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
                        self.stack.top() as isize + a as isize + 1
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
                    let count = usize::try_from(a)
                        .map_err(|_| self.stack.error(b"invalid C capture count"))?;
                    self.stack.push_closure(
                        native,
                        count,
                        Binding {
                            function: value.bytes as usize,
                            bridge: self.bridge.clone(),
                        },
                    )?;
                }
                15 => {
                    let array = usize::try_from(a)
                        .map_err(|_| self.stack.error(b"invalid array capacity"))?;
                    let records = usize::try_from(b)
                        .map_err(|_| self.stack.error(b"invalid record capacity"))?;
                    self.stack.create_table(array, records)?;
                }
                16 => self.stack.get_table(a as isize, b != 0)?,
                17 => self.stack.set_table(a as isize, b != 0)?,
                18 | 19 => {
                    // SAFETY: getfield/setfield lend a string for this dispatch.
                    let name =
                        unsafe { std::slice::from_raw_parts(value.bytes.cast(), value.length) };
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
                    value.integer = self.stack.protected_call(
                        arguments,
                        (b >= 0).then_some(b as usize),
                        handler,
                    )? as c_int;
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
                    let count = usize::try_from(a)
                        .map_err(|_| self.stack.error(b"invalid concat count"))?;
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
                    let bytes =
                        unsafe { std::slice::from_raw_parts(value.bytes.cast(), value.length) };
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
                47 => {
                    use deadlib_lua::GcOperation;
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
                    value.integer = self.stack.gc(operation)?;
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
                0 => value.integer = stack.top() as c_int,
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
                        stack.top() as isize + a as isize + 1
                    };
                    let top =
                        usize::try_from(top).map_err(|_| stack.error(b"invalid stack top"))?;
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
                    value.integer =
                        c_int::from(stack.value(a as isize).is_some_and(Value::is_truthy))
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
        match context.operation(op, a, b, value) {
            Ok(()) => 0,
            Err(error) => {
                context.error = Some(error);
                1
            }
        }
    }
    fn native(call: &mut HostCall<'_>) -> Result<(), Error> {
        let binding = call.function_data::<Binding>().expect("C function binding");
        let function = binding.function;
        let bridge = binding.bridge.clone();
        // SAFETY: bindings come from trusted symbols or lua_pushcclosure,
        // both of which provide lua_CFunction's C signature.
        let function: CFunction = unsafe { std::mem::transmute(function) };
        let invoke = bridge.invoke;
        let mut stack = call.native_stack()?;
        let thread = stack.thread_id();
        let state = state_for(&mut stack, &bridge, thread)? as *mut c_void;
        let mut context = Context {
            stack: &mut stack,
            error: None,
            yielded: None,
            bridge,
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
        let count = usize::try_from(count)
            .map_err(|_| stack.error(b"invalid boundary-probe result count"))?;
        stack.finish(count)
    }
    pub fn run() {
        let args: Vec<_> = std::env::args().skip(1).collect();
        let [bridge, module, source] = args.as_slice() else {
            panic!("native_boundary bridge.dll module.dll source.lua")
        };
        let library = Library::open(bridge);
        let module = Library::open(module);
        // SAFETY: all four fixture symbols match the signatures in api.h.
        let bridge = unsafe {
            Arc::new(Bridge {
                invoke: std::mem::transmute::<usize, Invoke>(library.symbol("boundary_invoke")),
                read: std::mem::transmute::<usize, Read>(library.symbol("boundary_read")),
                write: std::mem::transmute::<usize, Write>(library.symbol("boundary_write")),
                new_state: std::mem::transmute::<usize, NewState>(
                    library.symbol("boundary_state_new"),
                ),
                free_state: std::mem::transmute::<usize, FreeState>(
                    library.symbol("boundary_state_free"),
                ),
                state_bytes: std::mem::transmute::<usize, unsafe extern "C" fn() -> usize>(
                    library.symbol("boundary_state_size"),
                )(),
                states: Mutex::new(std::collections::HashMap::new()),
                module,
                _library: library,
            })
        };
        let mut vm = Vm::new();
        vm.set_instruction_limit(Some(10_000_000));
        let source = std::fs::read(source).unwrap();
        for name in [
            "boundary_sum",
            "boundary_identity",
            "boundary_raise",
            "boundary_binary",
            "boundary_call",
            "boundary_stack",
            "boundary_mutate",
            "boundary_state",
            "boundary_state_reenter",
            "boundary_cached_top",
            "boundary_make_counter",
            "boundary_snapshot",
            "boundary_set_environment",
            "boundary_set_globals",
            "boundary_registry_roundtrip",
            "boundary_make_many",
            "boundary_table",
            "boundary_get",
            "boundary_set",
            "boundary_field",
            "boundary_raw",
            "boundary_metatable",
            "boundary_count",
            "boundary_shuffle",
            "boundary_userdata",
            "boundary_userdata_read",
            "boundary_userdata_finalize",
            "boundary_bytes",
            "boundary_environment",
            "boundary_capture_bytes",
            "boundary_length",
            "boundary_userdata_empty",
            "boundary_protected",
            "boundary_protected_negative",
            "boundary_protected_argument",
            "boundary_replace_handler",
            "boundary_scalar",
            "boundary_scalar_null",
            "boundary_scalar_capture",
            "boundary_integers",
            "boundary_compare",
            "boundary_concat",
            "boundary_concat_prefix",
            "boundary_format",
            "boundary_format_gc",
            "boundary_auxcheck",
            "boundary_auxwhere",
            "boundary_auxerror",
            "boundary_auxuserdata",
            "boundary_auxget",
            "boundary_auxnewmetatable",
            "boundary_auxmetafield",
            "boundary_auxopen",
            "boundary_auxfind",
            "boundary_auxrefs",
            "boundary_auxbuffer_info",
            "boundary_auxbuffer",
            "boundary_auxgsub",
            "boundary_upget",
            "boundary_upset",
            "boundary_upbind",
            "boundary_gc",
            "boundary_gc_counts",
            "boundary_load",
            "boundary_loadbuffer",
            "boundary_loadtop",
            "boundary_loadroots",
            "boundary_loadfile",
            "boundary_dump",
            "boundary_dump_null",
            "boundary_thread",
            "boundary_thread_status",
            "boundary_thread_pointer",
            "boundary_thread_capture",
            "boundary_pointer",
            "boundary_newthread",
            "boundary_thread_values",
            "boundary_thread_push",
            "boundary_thread_move",
            "boundary_thread_primitives",
            "boundary_thread_selfmove",
            "boundary_thread_resize",
            "boundary_thread_hold",
            "boundary_resume",
            "boundary_yield",
            "boundary_level",
            "boundary_setlevel",
            "boundary_level_reader",
            "boundary_cpcall",
            "boundary_debug_info",
            "boundary_debug_local",
            "boundary_debug_hold",
        ] {
            let function = vm
                .new_host_function_with_data(
                    native,
                    &[],
                    Binding {
                        function: bridge.module.symbol(name),
                        bridge: bridge.clone(),
                    },
                )
                .unwrap();
            vm.set_global(name.as_bytes(), function).unwrap();
        }
        let library_owner = Arc::downgrade(&bridge);
        drop(bridge);
        let state_function = vm.global(b"boundary_state");
        let before = vm.call(state_function, &[]).unwrap()[0];
        // Moving Vm and growing its thread arena must not move native states.
        let mut vm = Box::new(vm);
        assert_eq!(vm.call(state_function, &[]).unwrap()[0], before);
        vm.begin_standalone().unwrap();
        vm.execute(&compile_named(&source, b"@boundary.lua").unwrap())
            .unwrap();
        vm.end_standalone().unwrap();
        vm.collect_garbage().unwrap();
        assert!(
            DESTROYED.load(Ordering::Relaxed) > 0,
            "unreachable coroutine states must be freed before VM close"
        );
        let collected = DESTROYED.load(Ordering::Relaxed);
        drop(vm);
        assert!(library_owner.upgrade().is_none(), "all DLL owners released");
        let created = CREATED.load(Ordering::Relaxed);
        let destroyed = DESTROYED.load(Ordering::Relaxed);
        assert_eq!(
            created, destroyed,
            "one native-state destruction per allocation"
        );
        let operations = OPERATIONS.load(Ordering::Relaxed);
        let drops = DROPS.load(Ordering::Relaxed);
        assert_eq!(
            operations, drops,
            "every Rust dispatcher must return before a C jump"
        );
        eprintln!(
            "BOUNDARY operations={operations} drops={drops} panic_abort={}",
            cfg!(panic = "abort")
        );
        eprintln!("STATES created={created} collected={collected} destroyed={destroyed}");
        // VM destruction and all states precede unloading either DLL.
    }
}
fn main() {
    #[cfg(windows)]
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(windows::run)
        .unwrap()
        .join()
        .unwrap();
    #[cfg(not(windows))]
    panic!("Windows/MSVC development-only boundary probe");
}
