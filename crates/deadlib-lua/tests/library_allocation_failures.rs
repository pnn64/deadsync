use deadlib_lua::{ErrorKind, Value, Vm, compile_named};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{begin, reject_from_now, statistics};

#[derive(Clone, Copy)]
enum Expected {
    Runtime,
    Success,
}
#[derive(Debug)]
enum Outcome {
    Error(ErrorKind, Option<Value>),
    Returned(Value),
}

fn run_case(setup: &[u8], source: &[u8], expected: Expected, nth: usize) -> usize {
    run_bound_case(setup, source, expected, nth, &[], true)
}

fn run_bound_case(
    setup: &[u8],
    source: &[u8],
    expected: Expected,
    nth: usize,
    bindings: &[(&[u8], &[u8])],
    repeat: bool,
) -> usize {
    let guard = begin(0);
    let attempts;
    {
        let mut vm = Vm::try_new().unwrap();
        for &(name, bytes) in bindings {
            let value = vm.try_intern(bytes).unwrap();
            vm.set_global(name, value).unwrap();
        }
        let setup = compile_named(setup, b"=os-setup").unwrap();
        let source = compile_named(source, b"@os-errors\xff.lua").unwrap();
        let retry = compile_named(b"return 42", b"=os-retry").unwrap();
        vm.execute(&setup).unwrap();
        vm.try_load_program(&source).unwrap();
        vm.try_load_program(&retry).unwrap();
        let start = statistics().attempts;
        reject_from_now(nth);
        let result = match vm.execute(&source) {
            Ok(values) => Outcome::Returned(values.first().copied().unwrap_or(Value::Nil)),
            Err(error) => Outcome::Error(error.kind(), error.value()),
        };
        attempts = statistics().attempts - start;
        reject_from_now(0);
        if nth == 0 {
            match (expected, result) {
                (Expected::Runtime, Outcome::Error(ErrorKind::Runtime, _))
                | (Expected::Success, Outcome::Returned(Value::Number(42.))) => {}
                (_, result) => panic!("unexpected healthy result: {result:?}"),
            }
        } else {
            assert!(statistics().rejected > 0);
            let Outcome::Error(ErrorKind::Memory, Some(Value::String(id))) = result else {
                panic!("unexpected rejected result at {nth}: {result:?}");
            };
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            vm.collect_garbage().unwrap();
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            if repeat {
                match expected {
                    Expected::Runtime => {
                        assert_eq!(vm.execute(&source).unwrap_err().kind(), ErrorKind::Runtime)
                    }
                    Expected::Success => {
                        assert_eq!(vm.execute(&source).unwrap(), &[Value::Number(42.)])
                    }
                }
            }
        }
        assert_eq!(vm.execute(&retry).unwrap(), &[Value::Number(42.)]);
    }
    let allocations = statistics();
    drop(guard);
    assert_eq!(
        allocations.allocated_bytes, allocations.freed_bytes,
        "OS storage leaked at rejection {nth}"
    );
    attempts
}

#[test]
fn calendar_calls_recover_and_release_storage_after_allocation_failures() {
    use Expected::{Runtime, Success};
    let cases: &[(&[u8], &[u8], Expected)] = &[
        (b"", b"return os.date('!%Y-%m-%d %H:%M:%S',946684800)=='2000-01-01 00:00:00' and 42", Success),
        (b"", b"local d=os.date('!*t',946684800);return d.year==2000 and d.month==1 and d.day==1 and d.wday==7 and d.yday==1 and 42", Success),
        (b"", b"return os.date('!',946684800)=='' and 42", Success),
        (b"", b"return os.date('!abc\\0ignored',946684800)=='abc' and 42", Success),
        (b"", b"return os.date(123,946684800)=='123' and 42", Success),
        (b"format='!'..string.rep('year=%Y;',2000)", b"return os.date(format,946684800)==string.rep('year=2000;',2000) and 42", Success),
        (b"", b"return type(os.date())=='string' and 42", Success),
        (b"", b"return type(os.date('*t'))=='table' and 42", Success),
        (b"", b"os.date(false)", Runtime),
        (b"", b"os.date('%Y',{})", Runtime),
        (b"", b"return type(os.time())=='number' and 42", Success),
        (b"", b"return type(os.time(nil))=='number' and 42", Success),
        (b"", b"return type(os.time{year=2000,month=1,day=1})=='number' and 42", Success),
        (b"", b"return os.time{year=2000,month=1,day=1}==os.time{year=2000,month=1,day=1,hour=12,min=0,sec=0} and 42", Success),
        (b"", b"return type(os.time{year='2000',month='1',day='1',hour='0',min='1',sec='2',isdst=false})=='number' and 42", Success),
        (b"", b"os.time(false)", Runtime),
        (b"", b"os.time({})", Runtime),
        (b"", b"os.time{day=1}", Runtime),
        (b"", b"os.time{day=1,month=1}", Runtime),
        (b"", b"local fields={sec=0,min=0,hour=12,day=1,month=1,year=2000,isdst=false};local calls={};local t=setmetatable({},{__index=function(_,k)calls[#calls+1]=k;return fields[k]end});os.time(t);return table.concat(calls,',')=='sec,min,hour,day,month,year,isdst' and 42", Success),
        (b"", b"os.time(setmetatable({},{__index=function()error('field failure')end}))", Runtime),
        (b"", b"local t=setmetatable({},{__index=function(_,k)return k=='year' and 2000 or 1 end});return type(os.time(t))=='number' and 42", Success),
    ];
    let mut total = 0;
    for (index, &(setup, source, expected)) in cases.iter().enumerate() {
        let count = run_case(setup, source, expected, 0);
        for nth in 1..=count {
            eprintln!("calendar case={index} rejection={nth}");
            run_case(setup, source, expected, nth);
        }
        total += count;
    }
    eprintln!(
        "calendar recovered {total} allocation failures across {} cases",
        cases.len()
    );
}

#[test]
fn table_concatenation_and_static_library_errors_recover_after_allocation_failures() {
    use Expected::{Runtime, Success};
    let cases: &[(&[u8], &[u8], Expected)] = &[
        (b"", b"return table.concat({})=='' and 42", Success),
        (
            b"",
            b"return table.concat({'abc','def'})=='abcdef' and 42",
            Success,
        ),
        (
            b"",
            b"return table.concat({'a\\0b','c'},'\\0')=='a\\0b\\0c' and 42",
            Success,
        ),
        (
            b"",
            b"return table.concat({1,12.5,-42},':')=='1:12.5:-42' and 42",
            Success,
        ),
        (
            b"",
            b"return table.concat({'a','b'},12)=='a12b' and 42",
            Success,
        ),
        (
            b"",
            b"return table.concat({1,2,3},',',2,3)=='2,3' and 42",
            Success,
        ),
        (
            b"",
            b"return table.concat({1,2,3},',',3,2)=='' and 42",
            Success,
        ),
        (b"", b"table.concat({true})", Runtime),
        (b"", b"table.concat({[1]='a',[3]='c'},'',1,3)", Runtime),
        (b"", b"table.concat({},false)", Runtime),
        (b"", b"table.setn({})", Runtime),
        (b"", b"table.insert({})", Runtime),
        (
            b"t={};for i=1,1000 do t[i]=string.rep('x',100)end",
            b"return #table.concat(t,'|')==100999 and #t[1]==100 and 42",
            Success,
        ),
    ];
    let mut total = 0;
    for (index, &(setup, source, expected)) in cases.iter().enumerate() {
        let count = run_case(setup, source, expected, 0);
        for nth in 1..=count {
            eprintln!("table library case={index} rejection={nth}");
            run_case(setup, source, expected, nth);
        }
        total += count;
    }
    eprintln!(
        "table library recovered {total} allocation failures across {} cases",
        cases.len()
    );
}

#[test]
fn environment_reads_recover_with_long_names_empty_unicode_and_binary_values() {
    const CHILD: &str = "DEADLIB_LUA_ENV_RECOVERY_CHILD";
    let key = format!("DEADLIB_LUA_ENV_RECOVERY_{}", "N".repeat(600));
    if std::env::var_os(CHILD).is_some() {
        let value = std::env::var_os(&key).unwrap();
        #[cfg(unix)]
        let value = {
            use std::os::unix::ffi::OsStringExt;
            value.into_vec()
        };
        #[cfg(not(unix))]
        let value = value.to_string_lossy().into_owned().into_bytes();
        let cases: &[(&[u8], Expected)] = &[
            (
                b"return os.getenv(env_key)==env_value and 42",
                Expected::Success,
            ),
            (
                b"return os.getenv(env_key..'\\0ignored')==env_value and 42",
                Expected::Success,
            ),
            (
                b"return os.getenv(env_key..'_missing')==nil and 42",
                Expected::Success,
            ),
            (
                b"return os.getenv('bad=name')==nil and 42",
                Expected::Success,
            ),
            (b"os.getenv(false)", Expected::Runtime),
            (b"return os.getenv('')==nil and 42", Expected::Success),
        ];
        let mut total = 0;
        for &(source, expected) in cases {
            let run = |nth| {
                run_bound_case(
                    b"",
                    source,
                    expected,
                    nth,
                    &[(b"env_key", key.as_bytes()), (b"env_value", &value)],
                    true,
                )
            };
            let count = run(0);
            for nth in 1..=count {
                run(nth);
            }
            total += count;
        }
        eprintln!("environment child recovered {total} allocation failures");
        return;
    }
    let values = [
        std::ffi::OsString::new(),
        "plain value".into(),
        "é音𝄞".repeat(1000).into(),
    ];
    #[cfg(unix)]
    let binary = {
        use std::os::unix::ffi::OsStringExt;
        Some(std::ffi::OsString::from_vec(b"raw\xff\x80bytes".to_vec()))
    };
    #[cfg(not(unix))]
    let binary: Option<std::ffi::OsString> = None;
    let mut total = 0;
    for value in values.into_iter().chain(binary) {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "environment_reads_recover_with_long_names_empty_unicode_and_binary_values",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env(&key, value)
            .env_remove(format!("{key}_missing"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let text = String::from_utf8(result.stderr).unwrap();
        total += text
            .lines()
            .find_map(|line| line.strip_prefix("environment child recovered "))
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<usize>()
            .unwrap();
    }
    eprintln!("environment recovered {total} allocation failures");
}

#[test]
fn file_removal_and_rename_recover_without_retaining_paths_or_handles() {
    use Expected::{Runtime, Success};
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/lua-os-allocation-fixtures");
    std::fs::create_dir_all(&directory).unwrap();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let filename = directory.join(format!(
        "{}-{stamp}-é音𝄞-{}.lua",
        std::process::id(),
        "long".repeat(30)
    ));
    let destination = filename.with_extension("renamed");
    let missing = filename.with_extension("missing");
    let old = filename.to_string_lossy();
    let new = destination.to_string_lossy();
    let absent = missing.to_string_lossy();
    let bindings: &[(&[u8], &[u8])] = &[
        (b"filename", old.as_bytes()),
        (b"destination", new.as_bytes()),
        (b"missing", absent.as_bytes()),
    ];
    let cases: &[(&[u8], Expected)] = &[
        (b"return os.remove(filename) and 42", Success),
        (b"return os.remove(filename..'\\0ignored') and 42", Success),
        (b"return os.rename(filename,destination) and 42", Success),
        (b"return os.rename(filename..'\\0ignored',destination..'\\0ignored') and 42", Success),
        (b"local ok,msg,code=os.remove(missing);return ok==nil and type(msg)=='string' and type(code)=='number' and 42", Success),
        (b"local ok,msg,code=os.rename(missing,destination);return ok==nil and type(msg)=='string' and type(code)=='number' and 42", Success),
        (b"os.remove(false)", Runtime),
        (b"os.rename(filename,false)", Runtime),
        (b"local ok,msg,code=os.rename('',destination);return ok==nil and code==2 and 42", Success),
        (b"local ok,msg,code=os.rename(filename,'');return ok==nil and code==2 and 42", Success),
        (b"local ok,msg,code=os.remove('');return ok==nil and code==2 and 42", Success),
    ];
    let run = |source, expected, nth| {
        std::fs::write(&filename, b"payload").unwrap();
        if destination.exists() {
            std::fs::remove_file(&destination).unwrap();
        }
        // An operation may complete its external side effect before a later
        // Lua return allocation fails. Retry the VM, not an already moved file.
        let count = run_bound_case(b"", source, expected, nth, bindings, false);
        assert!(
            filename.exists() || destination.exists() || source.starts_with(b"return os.remove")
        );
        for path in [&filename, &destination] {
            if path.exists() {
                assert_eq!(std::fs::read(path).unwrap(), b"payload");
                std::fs::remove_file(path).unwrap();
            }
        }
        count
    };
    let mut total = 0;
    for (index, &(source, expected)) in cases.iter().enumerate() {
        let count = run(source, expected, 0);
        for nth in 1..=count {
            eprintln!("OS file case={index} rejection={nth}");
            run(source, expected, nth);
        }
        total += count;
    }
    eprintln!(
        "OS files recovered {total} allocation failures across {} cases",
        cases.len()
    );
}

#[test]
fn temporary_names_clean_up_unpublished_files_on_allocation_failure() {
    fn run(nth: usize) -> usize {
        let mut vm = Vm::try_new().unwrap();
        let program = compile_named(b"return os.tmpname()", b"=temporary-name").unwrap();
        let retry = compile_named(b"return 42", b"=temporary-retry").unwrap();
        vm.try_load_program(&program).unwrap();
        vm.try_load_program(&retry).unwrap();
        let (result, allocations) = allocation_probe::probe(nth, || match vm.execute(&program) {
            Ok(values) => Outcome::Returned(values[0]),
            Err(error) => Outcome::Error(error.kind(), error.value()),
        });
        if nth == 0 {
            let Outcome::Returned(Value::String(id)) = result else {
                panic!("temporary name result: {result:?}");
            };
            let bytes = vm.string_bytes(id).unwrap();
            let path = std::path::Path::new(std::str::from_utf8(bytes).unwrap());
            assert!(
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&format!("deadsync-lua-{}-", std::process::id()))
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(path.parent(), Some(std::path::Path::new("/tmp")));
                let metadata = std::fs::metadata(path).unwrap();
                assert_eq!(metadata.len(), 0);
                assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
                std::fs::remove_file(path).unwrap();
            }
            #[cfg(windows)]
            assert!(!path.exists());
        } else {
            assert!(allocations.rejected > 0);
            let Outcome::Error(ErrorKind::Memory, Some(Value::String(id))) = result else {
                panic!("temporary rejection {nth}: {result:?}");
            };
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            vm.collect_garbage().unwrap();
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
        }
        assert_eq!(vm.execute(&retry).unwrap(), &[Value::Number(42.)]);
        allocations.attempts
    }
    #[cfg(unix)]
    let files = || {
        let prefix = format!("deadsync-lua-{}-", std::process::id());
        let mut files: Vec<_> = std::fs::read_dir("/tmp")
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&prefix)
            })
            .collect();
        files.sort();
        files
    };
    #[cfg(unix)]
    let before = files();
    let count = run(0);
    for nth in 1..=count {
        eprintln!("temporary name rejection={nth}");
        run(nth);
        #[cfg(unix)]
        assert_eq!(
            files(),
            before,
            "unpublished temporary file survived rejection {nth}"
        );
    }
    eprintln!("temporary names recovered {count} allocation failures");
}
