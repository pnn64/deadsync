use deadlib_lua::{
    Error, ErrorKind, HostCall, HostFunction, NativeLoadStatus, Value, Vm, compile_named,
};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{begin, reject_from_now, statistics};

#[derive(Clone, Copy)]
enum Expected {
    Error,
    ProtectedError,
    Success,
    LoadError,
    LoadSuccess,
    DoFileError,
    DoFileSuccess,
}

#[derive(Debug)]
enum Outcome {
    Error(ErrorKind, Option<Value>),
    Returned(Value, Value),
}

fn run_case(setup: &[u8], source: &[u8], expected: Expected, nth: usize) -> usize {
    run_case_with_file(setup, source, expected, nth, None)
}

fn run_case_with_file(
    setup: &[u8],
    source: &[u8],
    expected: Expected,
    nth: usize,
    file: Option<&std::path::Path>,
) -> usize {
    run_case_options(setup, source, expected, nth, file, None)
}

fn run_case_options(
    setup: &[u8],
    source: &[u8],
    expected: Expected,
    nth: usize,
    file: Option<&std::path::Path>,
    host: Option<HostFunction>,
) -> usize {
    let guard = begin(0);
    let attempts;
    {
        let mut vm = Vm::try_new().unwrap();
        if let Some(host) = host {
            let function = vm.new_host_function(host, &[]).unwrap();
            vm.set_global(b"host", function).unwrap();
        }
        vm.execute(
            &compile_named(
                br#"
            saved_package = package
            saved_loaders = package.loaders
            saved_preload = package.preload
        "#,
                b"=save-package",
            )
            .unwrap(),
        )
        .unwrap();
        let setup = compile_named(setup, b"=setup").unwrap();
        let source = compile_named(source, b"@package-errors\xff.lua").unwrap();
        let retry = compile_named(
            br#"
            package = saved_package
            package.loaders = saved_loaders
            package.preload = saved_preload
            package.preload.__allocation_retry = function() return 42 end
            package.loaded.__allocation_retry = nil
            return require('__allocation_retry')
        "#,
            b"=retry-module",
        )
        .unwrap();
        vm.execute(&setup).unwrap();
        vm.try_load_program(&source).unwrap();
        vm.try_load_program(&retry).unwrap();
        let start = statistics().attempts;
        reject_from_now(nth);
        let outcome = match vm.execute(&source) {
            Ok(values) => Outcome::Returned(
                values.first().copied().unwrap_or(Value::Nil),
                values.get(1).copied().unwrap_or(Value::Nil),
            ),
            Err(error) => Outcome::Error(error.kind(), error.value()),
        };
        attempts = statistics().attempts - start;
        reject_from_now(0);
        // Verify closure before collection, retry or VM destruction could hide
        // a reader retained after the completed/failed operation.
        if let Some(file) = file {
            #[cfg(target_os = "linux")]
            {
                let expected = file.canonicalize().unwrap();
                for descriptor in std::fs::read_dir("/proc/self/fd").unwrap() {
                    let descriptor = descriptor.unwrap();
                    if let Ok(target) = std::fs::read_link(descriptor.path()) {
                        assert_ne!(target, expected, "module reader retained an open file");
                    }
                }
            }
            // Windows read handles deny delete sharing.
            let renamed = file.with_extension("closed-check");
            std::fs::rename(file, &renamed).unwrap();
            std::fs::rename(&renamed, file).unwrap();
        }
        if nth == 0 {
            match (expected, outcome) {
                (
                    Expected::Error | Expected::DoFileError,
                    Outcome::Error(ErrorKind::Runtime, _),
                )
                | (
                    Expected::ProtectedError,
                    Outcome::Returned(Value::Boolean(false), Value::String(_)),
                )
                | (Expected::LoadError, Outcome::Returned(Value::Nil, Value::String(_)))
                | (
                    Expected::Success | Expected::LoadSuccess | Expected::DoFileSuccess,
                    Outcome::Returned(Value::Number(42.), _),
                ) => {}
                (_, outcome) => panic!("unexpected healthy result: {outcome:?}"),
            }
        } else {
            assert!(statistics().rejected > 0);
            let value = match outcome {
                Outcome::Error(ErrorKind::Memory, Some(value)) => value,
                Outcome::Error(ErrorKind::Runtime, Some(value))
                    if matches!(expected, Expected::DoFileError | Expected::DoFileSuccess) =>
                {
                    value
                }
                Outcome::Returned(Value::Boolean(false), value)
                    if matches!(expected, Expected::ProtectedError) =>
                {
                    value
                }
                Outcome::Returned(Value::Nil, value)
                    if matches!(expected, Expected::LoadError | Expected::LoadSuccess) =>
                {
                    value
                }
                _ => panic!("unexpected allocation result: {outcome:?}"),
            };
            let Value::String(id) = value else {
                panic!("expected rooted memory string")
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
        "module state leaked at {nth}"
    );
    attempts
}

#[test]
fn module_errors_search_and_initialization_recover_from_allocation_failures() {
    use Expected::{Error, ProtectedError, Success};
    let cases: &[(&[u8], &[u8], Expected)] = &[
        (b"", b"return require(nil)", Error),
        (b"", b"return require()", Error),
        (b"", b"return pcall(require,nil)", ProtectedError),
        (b"", b"local object={require=require}; return object:require()", Error),
        (b"", b"return module(nil)", Error),
        (b"", b"return package.seeall(false)", Error),
        (b"package.loaders=false", b"return require('missing')", Error),
        (b"package.preload=false", b"return require('missing')", Error),
        (b"package.path=false", b"return require('missing')", Error),
        (b"package.path=''; package.cpath=''", b"return require('missing')", Error),
        (b"package.path=''; package.cpath=''", b"return pcall(require,'missing')", ProtectedError),
        (b"package.loaders={function() return 'one\\255', 'ignored' end, function() return 'two' end}", b"return require('missing')", Error),
        (b"package.preload.failure=function() error('loader failure') end", b"return require('failure')", Error),
        (b"package.preload.recursive=function() return require('recursive') end", b"return pcall(require,'recursive')", ProtectedError),
        (b"package.loaded.cached=42", b"return require('cached')", Success),
        (b"package.preload.value=function(name) return {name=name,answer=42} end", b"local t=require('value'); return t.name=='value' and t.answer", Success),
        (b"package.preload.empty=function() end", b"return require('empty')==true and 42", Success),
        (b"package.preload.retry=function() return false end", b"local a=require('retry');local b=require('retry');return a==false and b==false and 42", Success),
        (b"package.preload.nul=function() return 42 end", b"return require('nul\\0tail')", Success),
        (b"", b"module('alpha.beta',package.seeall); return _NAME=='alpha.beta' and _M==alpha.beta and 42", Success),
        (b"alpha=17", b"module('alpha.beta')", Error),
        (b"", b"module('alpha',function() error('option failure') end)", Error),
        (b"", b"local t={}; package.seeall(t); return getmetatable(t).__index==_G and 42", Success),
        (b"package.path='__deadlib_no_such_directory__/?.lua'; package.cpath=''", b"return require('one.two')", Error),
        (b"package.path=string.rep('__missing_subdirectory__/',40)..'?.lua'; package.cpath=''", b"return require('one.two')", Error),
        (b"package.path='__missing\\255/?.lua'; package.cpath=''", b"return require('one.two')", Error),
    ];
    allocation_probe::probe(0, || drop(Vm::try_new().unwrap()));
    let mut total = 0;
    for (index, &(setup, source, expected)) in cases.iter().enumerate() {
        let count = run_case(setup, source, expected, 0);
        for nth in 1..=count {
            eprintln!("module allocation case={index} rejection={nth}");
            run_case(setup, source, expected, nth);
        }
        total += count;
    }
    eprintln!("module operations recovered {total} allocation failures");
}

#[test]
fn reader_and_string_loading_recover_from_allocation_failures() {
    use Expected::{Error, LoadError, LoadSuccess};
    let cases: &[(&[u8], &[u8], Expected)] = &[
        (b"", b"local f,e=loadstring('return 42'); if not f then return nil,e end; return f()", LoadSuccess),
        (b"", b"return loadstring('return +','@broken\\255.lua')", LoadError),
        (b"", b"local done=false; local f,e=load(function() if not done then done=true;return 'return 42' end end); if not f then return nil,e end;return f()", LoadSuccess),
        (b"", b"local done=false;return load(function() if not done then done=true;return 'return +' end end)", LoadError),
        (b"", b"return load(function() return false end)", LoadError),
        (b"", b"return load(function() error('reader failure') end)", LoadError),
        (b"", b"return load(nil)", Error),
        (b"", b"local i=0;local f,e=load(function() i=i+1;if i==1 then return 'return ' elseif i==2 then return 42 end end,12345);if not f then return nil,e end;return f()", LoadSuccess),
        (b"", b"local i=0;local f,e=load(function() i=i+1;if i==2 then return 'return 42' end end);if not f then return nil,e end;return f()", LoadSuccess),
        (b"payload=string.dump(function() return 42 end)", b"local done=false;local f,e=load(function() if not done then done=true;return payload end end);if not f then return nil,e end;return f()", LoadSuccess),
        (b"payload=string.dump(function() return 42 end);payload=string.sub(payload,1,#payload-1)", b"local done=false;return load(function() if not done then done=true;return payload end end)", LoadError),
        (b"", b"local parts={'local function outer(a) ','local b=2; return function() return a+b end end; ','return outer(40)()'};local i=0;local f,e=load(function() i=i+1;collectgarbage();return parts[i] end,'@nested\\255.lua');if not f then return nil,e end;collectgarbage();return f()", LoadSuccess),
    ];
    allocation_probe::probe(0, || drop(Vm::try_new().unwrap()));
    let mut total = 0;
    for (index, &(setup, source, expected)) in cases.iter().enumerate() {
        let count = run_case(setup, source, expected, 0);
        for nth in 1..=count {
            eprintln!("reader allocation case={index} rejection={nth}");
            run_case(setup, source, expected, nth);
        }
        total += count;
    }
    eprintln!("readers recovered {total} allocation failures");
}

#[test]
fn rust_host_buffer_loading_recovers_and_preserves_parser_status() {
    fn host(call: &mut HostCall<'_>) -> Result<(), Error> {
        let invalid = call.arguments().first() == Some(&Value::Boolean(false));
        let mut stack = call.native_stack()?;
        let source = if invalid {
            b"return +".as_slice()
        } else {
            b"return 42".as_slice()
        };
        match stack.load_buffer(source, Some(b"@rust-reader\xff.lua"))? {
            NativeLoadStatus::Ok => {
                assert!(!invalid);
                stack.call_unprotected(0, Some(1))?;
                stack.finish(1)
            }
            status => {
                assert!(
                    status == NativeLoadStatus::MemoryError
                        || (invalid && status == NativeLoadStatus::SyntaxError)
                );
                stack.push(Value::Nil)?;
                stack.insert(-2)?;
                stack.finish(2)
            }
        }
    }
    allocation_probe::probe(0, || drop(Vm::try_new().unwrap()));
    let mut total = 0;
    for (source, expected) in [
        (b"return host(true)".as_slice(), Expected::LoadSuccess),
        (b"return host(false)".as_slice(), Expected::LoadError),
    ] {
        let count = run_case_options(b"", source, expected, 0, None, Some(host));
        for nth in 1..=count {
            eprintln!("Rust host loader rejection={nth}");
            run_case_options(b"", source, expected, nth, None, Some(host));
        }
        total += count;
    }
    eprintln!("Rust host loaders recovered {total} allocation failures");
}

#[test]
fn source_file_modules_release_resources_when_loading_is_rejected() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/lua-module-allocation-fixtures");
    std::fs::create_dir_all(&base).unwrap();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = base.join(format!("{}-{unique}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    struct Files {
        paths: Vec<std::path::PathBuf>,
        directories: Vec<std::path::PathBuf>,
    }
    impl Drop for Files {
        fn drop(&mut self) {
            for file in &self.paths {
                let _ = std::fs::remove_file(file);
            }
            for directory in self.directories.iter().rev() {
                let _ = std::fs::remove_dir(directory);
            }
        }
    }
    let mut files = Files {
        paths: Vec::new(),
        directories: vec![directory.clone()],
    };
    let mut long = directory.clone();
    for index in 0..4 {
        long = long.join(format!("directory_{index}_{}", "x".repeat(65)));
        std::fs::create_dir(&long).unwrap();
        files.directories.push(long.clone());
    }

    let binary = reference_test::native_chunk(
        "tests/package_allocation_failures.rs::source_file_modules_release_resources_when_loading_is_rejected::chunk0",
        &("return 42"),
    );
    let mut executable = b"#!/usr/bin/env lua\n".to_vec();
    executable.extend_from_slice(&binary);
    let cases = [
        (
            directory.join("healthy.lua"),
            b"return 42".as_slice(),
            Expected::Success,
        ),
        (
            directory.join("syntax.lua"),
            b"return +".as_slice(),
            Expected::Error,
        ),
        (
            directory.join("header.lua"),
            b"#!/usr/bin/env lua\r\nreturn 42".as_slice(),
            Expected::Success,
        ),
        (
            directory.join("binary.lua"),
            binary.as_slice(),
            Expected::Success,
        ),
        (
            directory.join("executable.lua"),
            executable.as_slice(),
            Expected::Success,
        ),
        (
            long.join("unicode-\u{e9}\u{1f980}.lua"),
            b"return 42".as_slice(),
            Expected::Success,
        ),
    ];
    allocation_probe::probe(0, || drop(Vm::try_new().unwrap()));
    let mut total = 0;
    for (index, (file, bytes, expected)) in cases.into_iter().enumerate() {
        files.paths.push(file.clone());
        std::fs::write(&file, bytes).unwrap();
        let setup = format!(
            "filename={:?};package.path=filename;package.cpath=''",
            file.to_string_lossy().replace('\\', "/")
        );
        let invalid = matches!(expected, Expected::Error);
        for (mode, (source, expected)) in [
            (b"return require('fixture')".as_slice(), expected),
            (
                b"local f,e=loadfile(filename);if not f then return nil,e end;return f()"
                    .as_slice(),
                if invalid {
                    Expected::LoadError
                } else {
                    Expected::LoadSuccess
                },
            ),
            (
                b"return dofile(filename)".as_slice(),
                if invalid {
                    Expected::DoFileError
                } else {
                    Expected::DoFileSuccess
                },
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let count = run_case_with_file(setup.as_bytes(), source, expected, 0, Some(&file));
            for nth in 1..=count {
                eprintln!("module file case={index} mode={mode} rejection={nth}");
                run_case_with_file(setup.as_bytes(), source, expected, nth, Some(&file));
            }
            total += count;
        }
    }
    eprintln!("module files recovered {total} allocation failures");
}

#[allow(dead_code)]
#[path = "../src/reference_test.rs"]
mod reference_test;
