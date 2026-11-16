use deadlib_lua::{ErrorKind, Value, Vm, compile_named};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{begin, reject_from_now, statistics};

#[derive(Clone, Copy)]
enum Expected {
    Runtime,
    NilError,
    Success,
}
#[derive(Debug)]
enum Outcome {
    Error(ErrorKind, Option<Value>),
    Returned(Value, Value),
}

fn run_case(setup: &[u8], source: &[u8], expected: Expected, nth: usize, filename: &[u8]) -> usize {
    let guard = begin(0);
    let attempts;
    {
        let mut vm = Vm::try_new().unwrap();
        let filename = vm.try_intern(filename).unwrap();
        vm.set_global(b"filename", filename).unwrap();
        let setup = compile_named(setup, b"=io-setup").unwrap();
        let source = compile_named(source, b"@io-errors\xff.lua").unwrap();
        let retry = compile_named(b"return 42", b"=io-retry").unwrap();
        vm.execute(&setup).unwrap();
        vm.try_load_program(&source).unwrap();
        vm.try_load_program(&retry).unwrap();
        let start = statistics().attempts;
        reject_from_now(nth);
        let result = match vm.execute(&source) {
            Ok(values) => Outcome::Returned(
                values.first().copied().unwrap_or(Value::Nil),
                values.get(1).copied().unwrap_or(Value::Nil),
            ),
            Err(error) => Outcome::Error(error.kind(), error.value()),
        };
        attempts = statistics().attempts - start;
        reject_from_now(0);
        if nth == 0 {
            match (expected, result) {
                (Expected::Runtime, Outcome::Error(ErrorKind::Runtime, _))
                | (Expected::NilError, Outcome::Returned(Value::Nil, Value::String(_)))
                | (Expected::Success, Outcome::Returned(Value::Number(42.), _)) => {}
                (_, result) => panic!("unexpected healthy result: {result:?}"),
            }
        } else {
            assert!(statistics().rejected > 0);
            let value = match result {
                Outcome::Error(ErrorKind::Memory, Some(value)) => value,
                Outcome::Returned(Value::Nil, value) if matches!(expected, Expected::NilError) => {
                    value
                }
                _ => panic!("unexpected rejected result at {nth}: {result:?}"),
            };
            let Value::String(id) = value else {
                panic!("memory error must be a rooted string")
            };
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            vm.collect_garbage().unwrap();
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
        }
        assert_eq!(vm.execute(&retry).unwrap(), &[Value::Number(42.)]);
    }
    let allocations = statistics();
    drop(guard);
    assert_eq!(
        allocations.allocated_bytes, allocations.freed_bytes,
        "I/O storage leaked at rejection {nth}"
    );
    attempts
}

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/lua-io-allocation-fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(directory.join(format!("{}-{time}.lua", std::process::id())))
    }
    fn reset(&self) {
        std::fs::write(&self.0, b"42\nline\n").unwrap();
    }
    fn assert_closed(&self) {
        #[cfg(target_os = "linux")]
        {
            let expected = self.0.canonicalize().unwrap();
            for descriptor in std::fs::read_dir("/proc/self/fd").unwrap() {
                if let Ok(path) = std::fs::read_link(descriptor.unwrap().path()) {
                    assert_ne!(path, expected, "file handle survived VM destruction");
                }
            }
        }
        // Windows file readers deny delete sharing, so a retained handle also
        // prevents this rename after VM destruction.
        let renamed = self.0.with_extension("closed-check");
        std::fs::rename(&self.0, &renamed).unwrap();
        std::fs::rename(renamed, &self.0).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn named_io_errors_and_file_operations_recover_from_allocation_failures() {
    use Expected::{NilError, Runtime, Success};
    let cases: &[(&[u8], &[u8], Expected)] = &[
        (b"", b"return io.open(nil)", Runtime),
        (b"", b"return io.close(false)", Runtime),
        (b"", b"return io.input(false)", Runtime),
        (b"", b"return io.lines({})", Runtime),
        (b"", b"return io.open(filename,'invalid')", NilError),
        (b"", b"return io.open(filename..'.missing')", NilError),
        (b"", b"return io.input(filename..'.missing')", Runtime),
        (b"", b"return io.lines(filename..'.missing')", Runtime),
        (b"", b"return loadfile(filename..'.missing')", NilError),
        (b"", b"local f=assert(io.open(filename));f:close();return 42", Success),
        (b"", b"local f=assert(io.open(filename,'r+'));f:close();return 42", Success),
        (b"", b"local f=assert(io.open(filename,'wb'));f:close();return 42", Success),
        (b"", b"local f=assert(io.open(filename,'w+'));f:write('yes');f:seek('set');local text=f:read('*a');f:close();return text=='yes' and 42", Success),
        (b"", b"local f=assert(io.open(filename,'a+'));f:seek('set');f:write('end');f:seek('set');local text=f:read('*a');f:close();return text=='42\\nline\\nend' and 42", Success),
        (b"f=assert(io.open(filename));f:close()", b"return f:read()", Runtime),
        (b"f=assert(io.open(filename));f:close()", b"return tostring(f)=='file (closed)' and 42", Success),
        (b"f=assert(io.open(filename))", b"return f:read('*z')", Runtime),
        (b"f=assert(io.open(filename))", b"return f:write(false)", Runtime),
        (b"f=assert(io.open(filename))", b"return f:setvbuf('bad')", Runtime),
        (b"f=assert(io.open(filename))", b"return getmetatable(f).__gc(false)", Runtime),
        (b"f=assert(io.open(filename))", b"return io.type(f)=='file' and tostring(f):sub(1,6)=='file (' and 42", Success),
        (b"f=assert(io.open(filename))", b"return f:read('*a')=='42\\nline\\n' and 42", Success),
        (b"f=assert(io.open(filename))", b"return f:read('*l')=='42' and 42", Success),
        (b"f=assert(io.open(filename))", b"return f:read('*n')", Success),
        (b"f=assert(io.open(filename))", b"local n,s=f:read('*n','*l');return n==42 and s=='' and 42", Success),
        (b"f=assert(io.open(filename))", b"return f:read(2)=='42' and 42", Success),
        (b"", b"local reader=io.lines(filename);local a,b,c=reader(),reader(),reader();return a=='42' and b=='line' and c==nil and 42", Success),
        (b"f=assert(io.open(filename));reader=f:lines()", b"local a,b,c=reader(),reader(),reader();return a=='42' and b=='line' and c==nil and 42", Success),
        (b"f=assert(io.open(filename,'w+'))", b"f:write('text\\n',42);f:flush();f:seek('set');return f:read('*a')=='text\\n42' and 42", Success),
        (b"", b"io.input(filename);return io.read('*n')", Success),
        (b"", b"io.output(filename);io.write('text');io.flush();return 42", Success),
        (b"f=assert(io.open(filename))", b"f:setvbuf('full',8192);return f:read('*n')", Success),
        (b"f=assert(io.open(filename,'w+'));f:write(string.rep('x',20000)..'\\n42');f:seek('set')", b"return #f:read('*l')==20000 and f:read('*n')", Success),
        (b"f=assert(io.open(filename,'w+'));f:write(string.rep('x',20000)..'\\n42');f:seek('set')", b"return #f:read('*a')==20003 and 42", Success),
        (b"f=assert(io.open(filename,'w+'));f:write(string.rep('0',2048)..'42');f:seek('set')", b"return f:read('*n')", Success),
        (b"f=assert(io.open(filename,'w+'));payload=string.rep('x\\n',10000)", b"f:write(payload);f:flush();f:seek('set');return f:read('*a')==payload and 42", Success),
    ];
    allocation_probe::probe(0, || drop(Vm::try_new().unwrap()));
    let fixture = Fixture::new();
    let filename = fixture.0.to_string_lossy().replace('\\', "/");
    let mut total = 0;
    for (index, &(setup, source, expected)) in cases.iter().enumerate() {
        fixture.reset();
        let count = run_case(setup, source, expected, 0, filename.as_bytes());
        fixture.assert_closed();
        for nth in 1..=count {
            eprintln!("I/O allocation case={index} rejection={nth}");
            fixture.reset();
            run_case(setup, source, expected, nth, filename.as_bytes());
            fixture.assert_closed();
        }
        total += count;
    }
    eprintln!("named I/O recovered {total} allocation failures");
}

#[test]
fn temporary_files_recover_from_allocation_failures_and_release_handles() {
    allocation_probe::probe(0, || drop(Vm::try_new().unwrap()));
    let prefix = format!("deadsync-lua-{}-", std::process::id());
    // Process IDs can be reused while an older crashed process's files remain.
    // Only files created by this run can indicate a leaked VM resource.
    let preexisting: std::collections::HashSet<_> = std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path())
        .collect();
    let assert_closed = || {
        for entry in std::fs::read_dir(std::env::temp_dir()).unwrap() {
            let entry = entry.unwrap();
            assert!(
                !entry.file_name().to_string_lossy().starts_with(&prefix)
                    || preexisting.contains(&entry.path()),
                "temporary file survived VM destruction: {}",
                entry.path().display()
            );
        }
        #[cfg(target_os = "linux")]
        for descriptor in std::fs::read_dir("/proc/self/fd").unwrap() {
            if let Ok(path) = std::fs::read_link(descriptor.unwrap().path()) {
                assert!(
                    !path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with(&prefix)),
                    "unlinked temporary file descriptor survived VM destruction"
                );
            }
        }
    };
    let cases: &[&[u8]] = &[
        b"local f=assert(io.tmpfile());f:close();return 42",
        b"local f=assert(io.tmpfile());f:write('42\\nline');f:seek('set');return f:read('*n')",
        b"local f=assert(io.tmpfile());f:write(string.rep('data',10000));return 42",
    ];
    let mut total = 0;
    for (index, &source) in cases.iter().enumerate() {
        let count = run_case(b"", source, Expected::Success, 0, b"");
        assert_closed();
        for nth in 1..=count {
            eprintln!("temporary I/O case={index} rejection={nth}");
            run_case(b"", source, Expected::Success, nth, b"");
            assert_closed();
        }
        total += count;
    }
    eprintln!("temporary files recovered {total} allocation failures");
}

#[test]
fn missing_file_errors_recover_with_cold_locale_state() {
    const VARIABLE: &str = "DEADLIB_IO_COLD_REJECTION";
    if let Ok(nth) = std::env::var(VARIABLE) {
        allocation_probe::probe(0, || drop(Vm::try_new().unwrap()));
        let count = run_case(
            b"",
            b"return loadfile('__deadlib_no_such_directory__/missing.lua')",
            Expected::NilError,
            nth.parse().unwrap(),
            b"",
        );
        println!("IO_COLD_ATTEMPTS={count}");
        return;
    }
    let run = |nth: usize| {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("missing_file_errors_recover_with_cold_locale_state")
            .arg("--exact")
            .arg("--nocapture")
            .env(VARIABLE, nth.to_string())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "cold rejection {nth}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("IO_COLD_ATTEMPTS="))
            .unwrap()
            .parse::<usize>()
            .unwrap()
    };
    let count = run(0);
    for nth in 1..=count {
        run(nth);
    }
    eprintln!("cold file errors recovered {count} allocation failures");
}
