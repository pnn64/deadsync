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
            fn GetProcessHandleCount(p: *mut std::ffi::c_void, count: *mut u32) -> i32;
        }
        let mut count = 0;
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
fn run(command: &[u8], source: &[u8], expected: f64, runtime: bool, nth: usize) -> usize {
    let handles = resources();
    #[cfg(target_os = "linux")]
    let children = std::fs::read("/proc/thread-self/children").unwrap();
    let guard = begin(0);
    let attempts;
    {
        let mut vm = Vm::try_new().unwrap();
        let command = vm.try_intern(command).unwrap();
        vm.set_global(b"command", command).unwrap();
        let source = compile_named(source, b"=execute-recovery").unwrap();
        let retry = compile_named(b"return 42", b"=execute-retry").unwrap();
        vm.try_load_program(&source).unwrap();
        vm.try_load_program(&retry).unwrap();
        let start = statistics().attempts;
        reject_from_now(nth);
        let result = vm
            .execute(&source)
            .map(|v| v[0])
            .map_err(|e| (e.kind(), e.value()));
        attempts = statistics().attempts - start;
        reject_from_now(0);
        if nth == 0 {
            if runtime {
                assert!(matches!(result, Err((ErrorKind::Runtime, _))));
            } else {
                assert_eq!(result, Ok(Value::Number(expected)));
            }
        } else {
            assert!(statistics().rejected > 0);
            let Err((ErrorKind::Memory, Some(Value::String(id)))) = result else {
                panic!("rejection {nth}: {result:?}");
            };
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            vm.collect_garbage().unwrap();
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            if runtime {
                assert_eq!(vm.execute(&source).unwrap_err().kind(), ErrorKind::Runtime);
            } else {
                assert_eq!(vm.execute(&source).unwrap(), &[Value::Number(expected)]);
            }
        }
        assert_eq!(vm.execute(&retry).unwrap(), &[Value::Number(42.)]);
    }
    let allocations = statistics();
    drop(guard);
    assert_eq!(
        allocations.allocated_bytes, allocations.freed_bytes,
        "execute storage leaked at {nth}"
    );
    assert_eq!(resources(), handles, "execute handles leaked at {nth}");
    #[cfg(target_os = "linux")]
    assert_eq!(
        std::fs::read("/proc/thread-self/children").unwrap(),
        children,
        "execute child not reaped at {nth}"
    );
    attempts
}
fn sweep(command: &[u8], source: &[u8], expected: f64, runtime: bool) -> usize {
    let count = run(command, source, expected, runtime, 0);
    for nth in 1..=count {
        eprintln!("execute rejection={nth}");
        run(command, source, expected, runtime, nth);
    }
    count
}

#[test]
fn shell_execution_and_queries_recover_without_leaking_storage_or_children() {
    let exit = if cfg!(windows) {
        b"exit /b 0".as_slice()
    } else {
        b"exit 0"
    };
    let status = if cfg!(windows) {
        b"exit /b 7".as_slice()
    } else {
        b"exit 7"
    };
    // Native process startup initializes caches outside Rust's allocator. The
    // separate cold probes retain rejection coverage before this warm-up.
    let mut warm = Vm::new();
    let value = warm.try_intern(exit).unwrap();
    warm.set_global(b"command", value).unwrap();
    warm.execute(&compile_named(b"return os.execute(command)", b"=warm-process").unwrap())
        .unwrap();
    drop(warm);
    let mut long = exit.to_vec();
    long.extend_from_slice(if cfg!(windows) { b"&rem " } else { b" # " });
    long.resize(6000, b'x');
    let mut nul = exit.to_vec();
    nul.extend_from_slice(b"\0ignored suffix");
    let cases: &[(&[u8], &[u8], f64, bool)] = &[
        (exit, b"return os.execute(command)", 0., false),
        (
            status,
            b"return os.execute(command)",
            if cfg!(windows) { 7. } else { 1792. },
            false,
        ),
        (exit, b"return os.execute()", 1., false),
        (exit, b"return os.execute(nil)", 1., false),
        (exit, b"return select('#',os.execute(nil,false))", 1., false),
        (b"", b"return os.execute(command)", 0., false),
        (&long, b"return os.execute(command)", 0., false),
        (&nul, b"return os.execute(command)", 0., false),
        (exit, b"return os.execute(false)", 0., true),
        (exit, b"return os.execute({})", 0., true),
    ];
    let mut total = 0;
    for &(command, source, expected, runtime) in cases {
        total += sweep(command, source, expected, runtime);
    }
    #[cfg(windows)]
    {
        total += windows_paths(exit);
    }
    eprintln!("execute recovered {total} allocation failures");
    // Explicitly retain a rejection-armed query; all attempts must either be
    // denied with the rooted error or the query must need no new allocation.
    let mut vm = Vm::new();
    let p = compile_named(b"return os.execute()", b"=query").unwrap();
    vm.try_load_program(&p).unwrap();
    let (result, allocations) =
        allocation_probe::probe(1, || vm.execute(&p).map(|v| v[0]).map_err(|e| e.kind()));
    if allocations.attempts == 0 {
        assert_eq!(result, Ok(Value::Number(1.)));
    } else {
        assert_eq!(result, Err(ErrorKind::Memory));
        assert_eq!(allocations.attempts, allocations.rejected);
    }
    #[cfg(target_os = "linux")]
    verify_signal_restoration();
}

#[cfg(target_os = "linux")]
fn verify_signal_restoration() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    unsafe extern "C" fn handler(_: i32) {
        CALLS.fetch_add(1, Ordering::Relaxed);
    }
    unsafe extern "C" {
        fn signal(number: i32, handler: usize) -> usize;
        fn raise(number: i32) -> i32;
    }
    struct Restore(i32, usize);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                signal(self.0, self.1);
            }
        }
    }
    for (number, name) in [(2, "INT"), (3, "QUIT")] {
        // SAFETY: the signal handler only increments a lock-free atomic. An
        // integer ABI representation also permits SIG_DFL/SIG_IGN sentinels.
        let previous = unsafe { signal(number, handler as *const () as usize) };
        assert_ne!(previous, usize::MAX);
        let restore = Restore(number, previous);
        CALLS.store(0, Ordering::Relaxed);
        let mut vm = Vm::new();
        let source = format!("return os.execute('kill -{name} $PPID; exit 0')");
        let program = compile_named(source.as_bytes(), b"=execute-signals").unwrap();
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Number(0.)]);
        assert_eq!(
            CALLS.load(Ordering::Relaxed),
            0,
            "system did not ignore the parent's signal"
        );
        assert_eq!(unsafe { raise(number) }, 0);
        assert_eq!(
            CALLS.load(Ordering::Relaxed),
            1,
            "system did not restore the host handler"
        );
        drop(restore);
    }
    eprintln!("POSIX execute restored 2 signal handlers");
}

#[cfg(windows)]
fn windows_paths(command: &[u8]) -> usize {
    use std::{ffi::OsString, path::PathBuf};
    struct Environment {
        comspec: Option<OsString>,
        path: Option<OsString>,
    }
    impl Drop for Environment {
        fn drop(&mut self) {
            unsafe {
                if let Some(v) = &self.comspec {
                    std::env::set_var("COMSPEC", v)
                } else {
                    std::env::remove_var("COMSPEC")
                }
                if let Some(v) = &self.path {
                    std::env::set_var("PATH", v)
                } else {
                    std::env::remove_var("PATH")
                }
            }
        }
    }
    let environment = Environment {
        comspec: std::env::var_os("COMSPEC"),
        path: std::env::var_os("PATH"),
    };
    let shell = PathBuf::from(environment.comspec.as_ref().unwrap());
    let root = std::env::temp_dir().join(format!("deadlua-execute-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let spaced = root.join("space; shell");
    std::fs::create_dir(&spaced).unwrap();
    std::fs::copy(&shell, spaced.join("cmd.exe")).unwrap();
    std::fs::copy(&shell, root.join("local-shell.exe")).unwrap();
    std::fs::write(root.join("local-shell"), b"not executable").unwrap();
    let path = OsString::from(format!("nonexistent;;\"{}\"", spaced.display()));
    let mut total = 0;
    unsafe {
        std::env::set_var("PATH", &path);
    }
    for (value, query) in [
        (None, 0.),
        (Some(root.join("missing.exe").into_os_string()), 0.),
        (Some(root.join("local-shell").into_os_string()), 1.),
        (Some(spaced.join("cmd.exe").into_os_string()), 1.),
        (Some(OsString::from("x".repeat(6000))), 0.),
    ] {
        // Windows environment APIs serialize updates; this binary has one test
        // and each child observes the complete prepared environment.
        unsafe {
            if let Some(v) = value {
                std::env::set_var("COMSPEC", v)
            } else {
                std::env::remove_var("COMSPEC")
            }
        }
        total += sweep(command, b"return os.execute()", query, false);
        total += sweep(command, b"return os.execute(command)", 0., false);
    }
    unsafe {
        std::env::remove_var("COMSPEC");
        std::env::set_var("PATH", "");
    }
    total += sweep(command, b"return os.execute(command)", -1., false);
    drop(environment);
    for file in [
        spaced.join("cmd.exe"),
        root.join("local-shell.exe"),
        root.join("local-shell"),
    ] {
        std::fs::remove_file(file).unwrap();
    }
    std::fs::remove_dir(spaced).unwrap();
    std::fs::remove_dir(root).unwrap();
    total
}
