//! Single-file official suite probe; run by scripts/check_rust_lua_suite.py.
use deadlib_lua::{ProgramCache, Value, Vm, compile, compile_named};
use std::{env, fs, path::Path, process::ExitCode, time::Instant};

fn cycles() -> Option<u64> {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn QueryProcessCycleTime(handle: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
        }
        let mut value = 0;
        // SAFETY: The pseudo-handle is always valid; the output points to live
        // writable storage for this synchronous call.
        (unsafe { QueryProcessCycleTime(GetCurrentProcess(), &mut value) } != 0).then_some(value)
    }
    #[cfg(not(windows))]
    None
}

fn benchmark(path: &str) -> Result<(), deadlib_lua::Error> {
    let mut vm = Vm::try_new()?;
    vm.execute(&compile(b"_U=true; T=nil; function Message(s) print(s) end; math.randomseed(0); assert(os.setlocale('C'))")?)?;
    let file = Path::new(path).file_name().unwrap().to_str().unwrap();
    let driver = format!("return dofile('{file}')");
    let program = compile_named(driver.as_bytes(), b"=official benchmark")?;
    let function = vm.try_load_program(&program)?;
    let before = cycles();
    let started = Instant::now();
    let values = vm.call(Value::Function(function), &[])?;
    let elapsed = started.elapsed().as_nanos();
    let used = before.zip(cycles()).map(|(a, b)| b.saturating_sub(a));
    let returned = match values.first() {
        Some(Value::Number(n)) => format!("{:016x}", n.to_bits()),
        _ => "none".to_owned(),
    };
    eprintln!(
        "BENCH\t{file}\t{elapsed}\t{}\t{returned}",
        used.map_or_else(|| "unavailable".to_owned(), |n| n.to_string())
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    let [engine, path] = args.as_slice() else {
        eprintln!("usage: official_suite rust|rust-cache|rust-bench path.lua");
        return ExitCode::FAILURE;
    };
    if engine == "rust-bench" {
        return match benchmark(path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("FAIL\t{:?}\t{}\t{}", e.kind(), e.line(), e.message());
                ExitCode::FAILURE
            }
        };
    }
    let source = fs::read(path).expect("read test source");
    let name = format!(
        "@{}",
        Path::new(path).file_name().unwrap().to_string_lossy()
    );
    let bootstrap = b"_U=true; function Message(s) print(s) end";
    if engine == "rust" || engine == "rust-cache" {
        let mut vm = Vm::new();
        vm.set_output_capture(true);
        vm.set_instruction_limit(Some(100_000_000));
        vm.execute(&compile(bootstrap).unwrap()).unwrap();
        let program = if engine == "rust-cache" {
            let directory = env::var_os("DEADLIB_LUA_TEST_CACHE").expect("test cache directory");
            ProgramCache::new(directory)
                .compile_named(&source, name.as_bytes())
                .map(|cached| {
                    eprintln!("CACHE\t{:?}", cached.status);
                    cached.program
                })
        } else {
            compile_named(&source, name.as_bytes())
        };
        let outcome = program.and_then(|p| vm.execute(&p).map(|v| v.to_vec()));
        use std::io::Write;
        std::io::stdout()
            .write_all(vm.output().unwrap())
            .expect("write probe output");
        match outcome {
            Ok(values) => {
                eprintln!("PASS\t{} return values", values.len());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("FAIL\t{:?}\t{}\t{}", e.kind(), e.line(), e.message());
                if let Some(Value::String(id)) = e.value() {
                    eprintln!(
                        "error object: {}",
                        String::from_utf8_lossy(vm.string_bytes(id).unwrap())
                    );
                }
                ExitCode::FAILURE
            }
        }
    } else {
        eprintln!("unknown engine: {engine}");
        ExitCode::FAILURE
    }
}
