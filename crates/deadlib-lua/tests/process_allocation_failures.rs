use deadlib_lua::{ErrorKind, Value, Vm, compile_named};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{begin, reject_from_now, statistics};

fn resources() -> usize {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn GetProcessHandleCount(process: *mut std::ffi::c_void, count: *mut u32) -> i32;
        }
        let mut count = 0;
        // SAFETY: the process pseudo-handle is borrowed; count is writable.
        assert_ne!(
            unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) },
            0
        );
        count as usize
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir("/proc/self/fd").unwrap().count()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        0
    }
}

fn run_case(command: &[u8], source: &[u8], runtime_error: bool, nth: usize) -> usize {
    let handles = resources();
    #[cfg(target_os = "linux")]
    let children = std::fs::read("/proc/thread-self/children").unwrap();
    let guard = begin(0);
    let attempts;
    {
        let mut vm = Vm::try_new().unwrap();
        let command = vm.try_intern(command).unwrap();
        vm.set_global(b"command", command).unwrap();
        let source = compile_named(source, b"=process-recovery").unwrap();
        let retry = compile_named(b"return 42", b"=process-retry").unwrap();
        vm.try_load_program(&source).unwrap();
        vm.try_load_program(&retry).unwrap();
        let start = statistics().attempts;
        reject_from_now(nth);
        let result = vm
            .execute(&source)
            .map(|values| values.first().copied().unwrap_or(Value::Nil))
            .map_err(|error| (error.kind(), error.value()));
        attempts = statistics().attempts - start;
        reject_from_now(0);
        if nth == 0 {
            if runtime_error {
                assert!(matches!(result, Err((ErrorKind::Runtime, _))));
            } else {
                assert_eq!(result, Ok(Value::Number(42.)));
            }
        } else {
            assert!(statistics().rejected > 0);
            let Err((ErrorKind::Memory, Some(Value::String(id)))) = result else {
                panic!("rejection {nth}: {result:?}");
            };
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            vm.collect_garbage().unwrap();
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            if runtime_error {
                assert_eq!(vm.execute(&source).unwrap_err().kind(), ErrorKind::Runtime);
            } else {
                assert_eq!(vm.execute(&source).unwrap(), &[Value::Number(42.)]);
            }
        }
        assert_eq!(vm.execute(&retry).unwrap(), &[Value::Number(42.)]);
    }
    let allocations = statistics();
    drop(guard);
    assert_eq!(
        allocations.allocated_bytes, allocations.freed_bytes,
        "storage leaked at {nth}"
    );
    assert_eq!(resources(), handles, "pipe/process handle leaked at {nth}");
    #[cfg(target_os = "linux")]
    assert_eq!(
        std::fs::read("/proc/thread-self/children").unwrap(),
        children,
        "unreaped child at {nth}"
    );
    attempts
}

#[test]
fn pipes_recover_at_every_allocation_and_release_storage_and_native_handles() {
    #[cfg(windows)]
    {
        use std::{
            io::Read,
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        // Process startup and CreatePipe initialize separate native handle
        // caches (see the recorded controls). Initialize them before exact
        // per-operation leak accounting; neither grows on subsequent calls.
        // Separate fresh-process probes cover cold allocation rejection.
        let mut child = Command::new("cmd.exe")
            .args(["/c", "echo payload"])
            .creation_flags(0x08000000)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        child.wait().unwrap();
        unsafe extern "system" {
            fn CreatePipe(
                read: *mut *mut std::ffi::c_void,
                write: *mut *mut std::ffi::c_void,
                attributes: *const std::ffi::c_void,
                size: u32,
            ) -> i32;
        }
        use std::os::windows::io::{FromRawHandle, OwnedHandle};
        let (mut read, mut write) = (std::ptr::null_mut(), std::ptr::null_mut());
        // SAFETY: outputs hold two handles, immediately placed under RAII.
        assert_ne!(
            unsafe { CreatePipe(&mut read, &mut write, std::ptr::null(), 0) },
            0
        );
        drop(unsafe { OwnedHandle::from_raw_handle(read) });
        drop(unsafe { OwnedHandle::from_raw_handle(write) });
    }
    let read = if cfg!(windows) {
        b"echo payload".as_slice()
    } else {
        b"printf 'payload\\n'"
    };
    let write = if cfg!(windows) {
        b"more > nul".as_slice()
    } else {
        b"cat >/dev/null"
    };
    let exit = if cfg!(windows) {
        b"exit /b 0".as_slice()
    } else {
        b"exit 0"
    };
    let mut long = read.to_vec();
    long.extend_from_slice(if cfg!(windows) { b"&rem " } else { b" # " });
    long.resize(6000, b'x');
    let mut nul = read.to_vec();
    nul.extend_from_slice(b"\0ignored command suffix");
    let cases: &[(&[u8], &[u8], bool)] = &[
        (read, b"local f=assert(io.popen(command));assert(f:read('*l')=='payload');f:close();return 42", false),
        (read, b"local f=assert(io.popen(command,'r'));assert(#f:read('*a')>0);f:close();return 42", false),
        (write, b"local f=assert(io.popen(command,'w'));assert(f:write('first\\nsecond\\n'));f:close();return 42", false),
        (write, b"local f=assert(io.popen(command,'w'));f:setvbuf('no');assert(f:write('payload\\n'));f:close();return 42", false),
        (exit, b"local f=assert(io.popen(command));f:close();return 42", false),
        (exit, b"local f=assert(io.popen(command));return 42", false),
        (write, b"local f=assert(io.popen(command,'w'));f:write('buffered');return 42", false),
        (&long, b"local f=assert(io.popen(command));assert(f:read('*l')=='payload');f:close();return 42", false),
        (&nul, b"local f=assert(io.popen(command));assert(f:read('*l')=='payload');f:close();return 42", false),
        (exit, b"assert(io.popen(command,'invalid'))", true),
        (exit, b"assert(io.popen(command,''))", true),
    ];
    let mut total = 0;
    for (index, &(command, source, runtime)) in cases.iter().enumerate() {
        let count = run_case(command, source, runtime, 0);
        for nth in 1..=count {
            eprintln!("pipe case={index} rejection={nth}");
            run_case(command, source, runtime, nth);
        }
        total += count;
    }
    eprintln!(
        "pipes recovered {total} allocation failures across {} cases",
        cases.len()
    );
    // VM finalization may try fallible allocations to call __gc. Native pipe
    // cleanup must still close, flush and reap when all of those attempts fail.
    for (command, mode) in [(exit, "r"), (write, "w")] {
        let handles = resources();
        let mut vm = Vm::try_new().unwrap();
        let command = vm.try_intern(command).unwrap();
        vm.set_global(b"command", command).unwrap();
        let source = format!(
            "f=assert(io.popen(command,'{mode}'));{}",
            if mode == "w" {
                "f:write('buffered')"
            } else {
                ""
            }
        );
        let source = compile_named(source.as_bytes(), b"=pipe-drop").unwrap();
        vm.execute(&source).unwrap();
        let ((), allocations) = allocation_probe::probe(1, || drop(vm));
        assert_eq!(
            allocations.attempts, allocations.rejected,
            "teardown allocation escaped rejection"
        );
        eprintln!(
            "pipe VM teardown mode={mode} rejected={}",
            allocations.rejected
        );
        assert_eq!(resources(), handles, "pipe teardown leaked handles");
    }
}
