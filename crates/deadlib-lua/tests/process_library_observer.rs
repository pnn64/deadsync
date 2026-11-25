//! The process-library observer reports exactly the standard-library calls
//! that reach outside the VM. One test, because the observer is a plain
//! function and its counter is therefore process-wide.
use deadlib_lua::{Vm, compile_named};
use std::sync::atomic::{AtomicUsize, Ordering};

static CALLS: AtomicUsize = AtomicUsize::new(0);

fn observed(vm: &mut Vm, source: &str) -> usize {
    let before = CALLS.load(Ordering::Relaxed);
    let program = compile_named(source.as_bytes(), b"=observer").unwrap();
    vm.execute(&program).unwrap();
    CALLS.load(Ordering::Relaxed) - before
}

#[test]
fn reports_only_calls_that_reach_outside_the_vm() {
    let directory = std::env::temp_dir().join(format!(
        "deadlib-lua-process-observer-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let chunk = directory.join("chunk.lua");
    std::fs::write(&chunk, "return 1").unwrap();
    let chunk = chunk.to_string_lossy().replace('\\', "/");

    {
        let mut vm = Vm::new();
        vm.set_process_library_observer(Some(|| {
            CALLS.fetch_add(1, Ordering::Relaxed);
        }));

        for inside in [
            "local t = {} for i = 1, 10 do t[i] = math.sqrt(i) end return #t",
            "return string.format('%d', 3) .. tostring(io) .. type(os.time)",
            "return os.difftime(10, 4)",
            "return io.type(io.stdout), tostring(io.stdout)",
            "return loadstring('return 2')()",
        ] {
            assert_eq!(observed(&mut vm, inside), 0, "{inside}");
        }

        for outside in [
            "return os.time()".to_owned(),
            "return os.time{year=2026, month=11, day=25}".to_owned(),
            "return os.clock()".to_owned(),
            "return os.date('%Y')".to_owned(),
            "return os.getenv('PATH')".to_owned(),
            format!("return loadfile('{chunk}')"),
            format!("return dofile('{chunk}')"),
            format!("local f = io.open('{chunk}') f:close()"),
            format!("for line in io.lines('{chunk}') do end"),
        ] {
            assert!(observed(&mut vm, &outside) >= 1, "{outside}");
        }

        // Dropping the VM closes its standard streams without any script.
        let before = CALLS.load(Ordering::Relaxed);
        drop(vm);
        assert_eq!(CALLS.load(Ordering::Relaxed), before, "VM teardown");
    }

    // Without an observer the same calls run unreported.
    let mut vm = Vm::new();
    assert_eq!(observed(&mut vm, "return os.time(), os.clock()"), 0);
    let _ = std::fs::remove_dir_all(directory);
}
