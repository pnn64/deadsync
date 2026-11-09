use deadlib_lua::{ErrorKind, Value, Vm, compile_named};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{probe, statistics};

struct Case {
    name: &'static str,
    source: &'static [u8],
    capture: bool,
    runtime_error: bool,
}
const fn case(name: &'static str, source: &'static [u8]) -> Case {
    Case {
        name,
        source,
        capture: false,
        runtime_error: false,
    }
}
const CASES: &[Case] = &[
    case("stdin-number", b"return io.read('*n')"),
    case("stdin-all", b"return io.read('*a')=='42\\nline\\n' and 42"),
    case("stdin-lines", b"local a,b=io.read('*l','*l');return a=='42' and b=='line' and 42"),
    case("stdin-iterator", b"local r=io.lines();return r()=='42' and r()=='line' and r()==nil and 42"),
    case("stdout", b"io.write('output');io.flush();return 42"),
    case("stderr", b"io.stderr:write('error');io.stderr:flush();return 42"),
    case("stdout-configure", b"io.stdout:setvbuf('full',10000);return 42"),
    case("print", b"print('output');return 42"),
    case("print-types", b"print(12.5,true,nil,{},function()end,newproxy(),coroutine.create(function()end));return 42"),
    case("print-proxy-metatable", b"local p=newproxy(true);local q=newproxy(p);assert(getmetatable(p)==getmetatable(q));print(p,q);return 42"),
    Case { runtime_error: true, ..case("print-invalid-proxy", b"print(newproxy({}))") },
    case("print-number-result", b"tostring=function()return 12.5 end;print('ignored');return 42"),
    Case { runtime_error: true, ..case("print-invalid-result", b"tostring=function()return false end;print('ignored')") },
    Case { capture: true, ..case("print-capture", b"print('output',12.5,true,nil);return 42") },
    Case { capture: true, ..case("print-capture-large", b"print(string.rep('abcdef',10000));return 42") },
];

#[derive(Debug)]
enum Outcome {
    Returned(Value),
    Error(ErrorKind, Option<Value>),
}

fn child(index: usize, rejection: usize) -> usize {
    let case = &CASES[index];
    // Do not print, create/drop another VM, or access Rust's standard streams
    // before the measured execution. There is deliberately no libtest harness.
    let mut vm = Vm::try_new().unwrap();
    vm.set_output_capture(case.capture);
    let program = compile_named(case.source, b"=stdio-recovery").unwrap();
    let retry = compile_named(b"return 42", b"=stdio-retry").unwrap();
    vm.try_load_program(&program).unwrap();
    vm.try_load_program(&retry).unwrap();
    let (outcome, allocations) = probe(rejection, || match vm.execute(&program) {
        Ok(values) => Outcome::Returned(values.first().copied().unwrap_or(Value::Nil)),
        Err(error) => Outcome::Error(error.kind(), error.value()),
    });
    if rejection == 0 {
        match outcome {
            Outcome::Returned(Value::Number(42.)) if !case.runtime_error => {}
            Outcome::Error(ErrorKind::Runtime, _) if case.runtime_error => {}
            _ => panic!("{} healthy result: {outcome:?}", case.name),
        }
        if case.name == "print-capture" {
            assert_eq!(vm.output(), Some(b"output\t12.5\ttrue\tnil\n".as_slice()));
        } else if case.name == "print-capture-large" {
            let bytes = vm.output().unwrap();
            assert_eq!(bytes.len(), 60001);
            assert!(
                bytes[..60000]
                    .as_chunks::<6>()
                    .0
                    .iter()
                    .all(|chunk| chunk == b"abcdef")
            );
            assert_eq!(bytes[60000], b'\n');
        }
    } else {
        assert!(allocations.rejected > 0);
        let Outcome::Error(ErrorKind::Memory, Some(Value::String(id))) = outcome else {
            panic!("{} rejection {rejection}: {outcome:?}", case.name);
        };
        assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
        vm.collect_garbage().unwrap();
        assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
    }
    assert_eq!(vm.execute(&retry).unwrap(), &[Value::Number(42.)]);
    // Lua stdout is a process resource: its shared buffer outlives this VM,
    // just as the former std LineWriter did. This probe checks error delivery
    // and reuse, not a claim that destroying a VM frees process-global buffers.
    drop(vm);
    assert!(!statistics().enabled);
    allocations.attempts
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.get(1).is_some_and(|arg| arg == "--stdio-child") {
        let attempts = child(arguments[2].parse().unwrap(), arguments[3].parse().unwrap());
        println!("\nSTDIO_ATTEMPTS={attempts}");
        return;
    }
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let executable = std::env::current_exe().unwrap();
    let run = |index: usize, rejection: usize| {
        let mut process = Command::new(&executable)
            .args(["--stdio-child", &index.to_string(), &rejection.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = process.stdin.take().unwrap();
        if let Err(error) = stdin.write_all(b"42\nline\n") {
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        }
        drop(stdin);
        let output = process.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{} rejection {rejection}: {}\n{}",
            CASES[index].name,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("STDIO_ATTEMPTS="))
            .unwrap()
            .parse::<usize>()
            .unwrap()
    };
    let mut total = 0;
    for (index, case) in CASES.iter().enumerate() {
        let count = run(index, 0);
        for rejection in 1..=count {
            eprintln!("stdio case={} rejection={rejection}", case.name);
            run(index, rejection);
        }
        total += count;
    }
    println!(
        "cold stdio recovered {total} allocation failures across {} cases",
        CASES.len()
    );
}
