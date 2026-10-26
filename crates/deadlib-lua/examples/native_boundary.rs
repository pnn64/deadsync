//! Development proof of a C-only error jump around the Rust host-call API.
//! Not the production Lua C API: states follow Lua threads and
//! this small fixture supports only the operations in tests/native-boundary.
//! Temporary values use the VM-owned mutable native stack.
#[cfg(windows)]
mod windows {
    use deadlib_lua::{Error, HostCall, HostThreadState, NativeStack, Value, Vm, compile_named};
    use std::{
        ffi::{CString, c_char, c_int, c_void},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    #[repr(C)]
    struct BoundaryValue {
        integer: c_int,
        number: f64,
        bytes: *const c_char,
        length: usize,
    }
    type Dispatch =
        unsafe extern "C" fn(*mut c_void, c_int, c_int, c_int, *mut BoundaryValue) -> c_int;
    type CFunction = unsafe extern "C" fn(*mut c_void) -> c_int;
    type Invoke = unsafe extern "C" fn(*mut c_void, *mut c_void, Dispatch, CFunction) -> c_int;
    type NewState = unsafe extern "C" fn() -> *mut c_void;
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
        new_state: NewState,
        free_state: FreeState,
        state_bytes: usize,
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
            // SAFETY: thread ownership outlives every active invocation; the Arc
            // keeps the allocating DLL loaded until its matching free returns.
            let freed = unsafe { (self.bridge.free_state)(self.state as *mut c_void) };
            assert_eq!(freed, 1, "C state was still executing during destruction");
            DESTROYED.fetch_add(1, Ordering::Relaxed);
        }
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
    struct Context<'a, 'vm> {
        stack: NativeStack<'a, 'vm>,
        error: Option<Error>,
        bridge: Arc<Bridge>,
    }
    impl Context<'_, '_> {
        fn operation(
            &mut self,
            op: c_int,
            a: c_int,
            b: c_int,
            value: &mut BoundaryValue,
        ) -> Result<(), Error> {
            match op {
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
                7 => return Err(self.stack.raise(self.stack.value(-1).unwrap_or(Value::Nil))),
                8 => {
                    let count = usize::try_from(a)
                        .map_err(|_| self.stack.error(b"invalid probe call count"))?;
                    self.stack
                        .call_unprotected(count, (b >= 0).then_some(b as usize))?;
                }
                9 => value.integer = c_int::from(a >= 0 && self.stack.check_stack(a as usize)),
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
        let context = unsafe { &mut *raw.cast::<Context<'_, '_>>() };
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
        if call.thread_state::<StateOwner>().is_none() {
            // SAFETY: typed fixture constructor creates an idle, heap-owned state.
            let state = unsafe { (bridge.new_state)() };
            if state.is_null() {
                return Err(call.error(b"cannot allocate boundary state"));
            }
            CREATED.fetch_add(1, Ordering::Relaxed);
            call.initialize_thread_state(StateOwner {
                state: state as usize,
                bridge: bridge.clone(),
            })?;
        }
        let owner = call.thread_state::<StateOwner>().unwrap();
        assert!(Arc::ptr_eq(&owner.bridge, &bridge));
        let state = owner.state as *mut c_void;
        // SAFETY: bindings come from trusted symbols or lua_pushcclosure,
        // both of which provide lua_CFunction's C signature.
        let function: CFunction = unsafe { std::mem::transmute(function) };
        let invoke = bridge.invoke;
        let mut context = Context {
            stack: call.native_stack()?,
            error: None,
            bridge,
        };
        // SAFETY: trusted C functions borrow the Context only until return.
        // setjmp/longjmp and every skipped frame are within the C DLLs.
        let count = unsafe {
            invoke(
                state,
                (&mut context as *mut Context<'_, '_>).cast(),
                dispatch,
                function,
            )
        };
        if let Some(error) = context.error.take() {
            return Err(error);
        }
        let count = usize::try_from(count)
            .map_err(|_| context.stack.error(b"invalid boundary-probe result count"))?;
        context.stack.finish(count)
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
                new_state: std::mem::transmute::<usize, NewState>(
                    library.symbol("boundary_state_new"),
                ),
                free_state: std::mem::transmute::<usize, FreeState>(
                    library.symbol("boundary_state_free"),
                ),
                state_bytes: std::mem::transmute::<usize, unsafe extern "C" fn() -> usize>(
                    library.symbol("boundary_state_size"),
                )(),
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
