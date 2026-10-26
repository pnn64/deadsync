//! Development proof of a C-only error jump around the Rust host-call API.
//! Not the production Lua C API: states are borrowed for one invocation and
//! this small fixture supports only the operations in tests/native-boundary.
//! Temporary values use the VM-owned mutable native stack.
#[cfg(windows)]
mod windows {
    use deadlib_lua::{Error, HostCall, NativeStack, Value, Vm, compile_named};
    use std::{
        ffi::{CString, c_char, c_int, c_void},
        sync::atomic::{AtomicUsize, Ordering},
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
    type Invoke = unsafe extern "C" fn(*mut c_void, Dispatch, CFunction) -> c_int;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> c_int;
    }
    struct Library(*mut c_void);
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
            Self(module)
        }
        fn symbol(&self, name: &str) -> usize {
            let name = CString::new(name).unwrap();
            // SAFETY: the module is live and the symbol name is terminated.
            let pointer = unsafe { GetProcAddress(self.0, name.as_ptr()) };
            assert!(!pointer.is_null(), "missing fixture symbol {name:?}");
            pointer as usize
        }
    }
    impl Drop for Library {
        fn drop(&mut self) {
            // SAFETY: this owner holds exactly one LoadLibrary reference.
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }

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
                    self.stack.call(count, (b >= 0).then_some(b as usize))?;
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
        let [Value::LightUserdata(function), Value::LightUserdata(invoke)] = call.captures()[..2]
        else {
            unreachable!()
        };
        // SAFETY: this pointer is the typed boundary_invoke symbol from boundary.c.
        let invoke: Invoke = unsafe { std::mem::transmute(invoke as usize) };
        // SAFETY: every registered symbol has lua_CFunction's C signature.
        let function: CFunction = unsafe { std::mem::transmute(function as usize) };
        let mut context = Context {
            stack: call.native_stack()?,
            error: None,
        };
        // SAFETY: trusted C functions borrow the Context only until return.
        // setjmp/longjmp and every skipped frame are within the C DLLs.
        let count = unsafe {
            invoke(
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
        let bridge = Library::open(bridge);
        let module = Library::open(module);
        let invoke = bridge.symbol("boundary_invoke");
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
        ] {
            let mut captures = [Value::Nil; 2];
            captures[0] = Value::LightUserdata(module.symbol(name) as u64);
            captures[1] = Value::LightUserdata(invoke as u64);
            let function = vm.new_host_function(native, &captures).unwrap();
            vm.set_global(name.as_bytes(), function).unwrap();
        }
        vm.execute(&compile_named(&source, b"@boundary.lua").unwrap())
            .unwrap();
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
        // VM destruction precedes unloading either DLL, including finalizers.
    }
}
fn main() {
    #[cfg(windows)]
    windows::run();
    #[cfg(not(windows))]
    panic!("Windows/MSVC development-only boundary probe");
}
