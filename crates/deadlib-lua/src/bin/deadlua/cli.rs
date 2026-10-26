//! Standalone Lua 5.1 command chunks, module loading and script arguments.
use deadlib_lua::{Error, ErrorKind, Value, Vm, compile_named};
use std::{env, io::Write, process::ExitCode};

fn stderr(bytes: &[u8]) {
    let bytes = &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())];
    let mut stderr = std::io::stderr().lock();
    #[cfg(windows)]
    {
        let mut start = 0;
        for (index, &byte) in bytes.iter().enumerate() {
            if byte == b'\n' {
                let _ = stderr.write_all(&bytes[start..index]);
                let _ = stderr.write_all(b"\r\n");
                start = index + 1;
            }
        }
        let _ = stderr.write_all(&bytes[start..]);
    }
    #[cfg(not(windows))]
    let _ = stderr.write_all(bytes);
}

enum Action<'a> {
    Execute(&'a [u8]),
    Require(&'a [u8]),
}
struct Options<'a> {
    actions: Vec<Action<'a>>,
    script: Option<usize>,
    version: bool,
    execute: bool,
}

fn options(args: &[Vec<u8>]) -> Option<Options<'_>> {
    let mut result = Options {
        actions: Vec::new(),
        script: None,
        version: false,
        execute: false,
    };
    let mut index = 1;
    while let Some(arg) = args.get(index) {
        if arg == b"--" {
            result.script = (index + 1 < args.len()).then_some(index + 1);
            break;
        }
        if !arg.starts_with(b"-") || arg == b"-" {
            result.script = Some(index);
            break;
        }
        match arg.get(1) {
            Some(b'v') if arg.len() == 2 => result.version = true,
            Some(flag @ (b'e' | b'l')) => {
                let value = if arg.len() == 2 {
                    index += 1;
                    args.get(index)?.as_slice()
                } else {
                    &arg[2..]
                };
                result.actions.push(if *flag == b'e' {
                    result.execute = true;
                    Action::Execute(value)
                } else {
                    Action::Require(value)
                });
            }
            _ => return None,
        }
        index += 1;
    }
    Some(result)
}

fn error_bytes(vm: &Vm, error: &Error) -> Option<Vec<u8>> {
    match error.value() {
        Some(Value::String(id)) => Some(vm.string_bytes(id).unwrap().to_vec()),
        Some(Value::Number(_)) | None => Some(error.message_bytes().to_vec()),
        Some(Value::Nil) => None,
        _ => Some(b"(error object is not a string)".to_vec()),
    }
}

fn execute(vm: &mut Vm, source: &[u8], name: &[u8]) -> Result<(), Option<Vec<u8>>> {
    let program = compile_named(source, name).map_err(|error| {
        Some(
            if error.line() != 0 && matches!(error.kind(), ErrorKind::Syntax | ErrorKind::Limit) {
                let mut message = name[1..].to_vec();
                message.extend_from_slice(format!(":{}: ", error.line()).as_bytes());
                message.extend_from_slice(error.message_bytes());
                message
            } else {
                error.message_bytes().to_vec()
            },
        )
    })?;
    let function = vm.load_program(&program);
    drop(program);
    vm.call(Value::Function(function), &[])
        .map(|_| ())
        .map_err(|e| error_bytes(vm, &e))
}

fn file(vm: &mut Vm, path: Option<&[u8]>, arguments: &[Value]) -> Result<(), Option<Vec<u8>>> {
    let function = vm.load_file(path).map_err(|e| error_bytes(vm, &e))?;
    vm.call(Value::Function(function), arguments)
        .map(|_| ())
        .map_err(|e| error_bytes(vm, &e))
}

fn run(vm: &mut Vm, args: &[Vec<u8>]) -> Result<(), Option<Vec<u8>>> {
    if let Some(init) = env::var_os("LUA_INIT") {
        let init = init.as_encoded_bytes();
        if let Some(path) = init.strip_prefix(b"@") {
            file(vm, Some(path), &[])?;
        } else {
            execute(vm, init, b"=LUA_INIT")?;
        }
    }
    let Some(options) = options(args) else {
        return Err(Some(b"usage: deadlua [options] [script [args]].\nAvailable options are:\n  -e stat  execute string 'stat'\n  -l name  require library 'name'\n  -v       show version information\n  --       stop handling options\n  -        execute stdin and stop handling options".to_vec()));
    };
    if options.version {
        stderr(format!("DeadLua {} (Lua 5.1)\n", env!("CARGO_PKG_VERSION")).as_bytes());
    }
    for action in options.actions {
        match action {
            Action::Execute(source) => execute(vm, source, b"=(command line)")?,
            Action::Require(name) => {
                let function = vm.global(b"require");
                let name = vm.intern(name);
                vm.call(function, &[name])
                    .map(|_| ())
                    .map_err(|e| error_bytes(vm, &e))?;
            }
        }
    }
    if let Some(script) = options.script {
        let table = vm.new_table();
        let root = vm
            .pin(Value::Table(table))
            .map_err(|e| error_bytes(vm, &e))?;
        let mut arguments = Vec::with_capacity(args.len() - script - 1);
        let loaded = (|| {
            for (index, argument) in args.iter().enumerate() {
                let value = vm.intern(argument);
                vm.raw_table_set(table, Value::Number(index as f64 - script as f64), value)
                    .map_err(|e| error_bytes(vm, &e))?;
                if index > script {
                    arguments.push(value);
                }
            }
            vm.set_global(b"arg", Value::Table(table))
                .map_err(|e| error_bytes(vm, &e))?;
            let path = if args[script] == b"-" && args[script - 1] != b"--" {
                None
            } else {
                Some(args[script].as_slice())
            };
            vm.load_file(path).map_err(|e| error_bytes(vm, &e))
        })();
        // Arguments become call-register roots at entry. Keeping their table
        // pinned during execution would incorrectly retain a replaced global.
        vm.unpin(root).map_err(|e| error_bytes(vm, &e))?;
        let function = loaded?;
        vm.call(Value::Function(function), &arguments)
            .map(|_| ())
            .map_err(|e| error_bytes(vm, &e))?;
    } else if !options.execute && !options.version {
        file(vm, None, &[])?;
    }
    Ok(())
}

pub(super) fn main() -> ExitCode {
    let args: Vec<_> = env::args_os()
        .map(|a| a.as_encoded_bytes().to_vec())
        .collect();
    let mut vm = Vm::new();
    vm.begin_standalone().expect("fresh standalone VM");
    let result = run(&mut vm, &args);
    if let Err(Some(error)) = &result {
        stderr(args.first().map_or(b"deadlua", Vec::as_slice));
        stderr(b": ");
        stderr(error);
        stderr(b"\n");
    }
    let finished = vm.end_standalone();
    if let Err(error) = &finished
        && let Some(error) = error_bytes(&vm, error)
    {
        stderr(args.first().map_or(b"deadlua", Vec::as_slice));
        stderr(b": ");
        stderr(&error);
        stderr(b"\n");
    }
    if result.is_ok() && finished.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
