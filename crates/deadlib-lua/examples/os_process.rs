//! Isolated process oracle for scripts which may call os.exit.
use std::{env, fs, io::Write, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    let [engine, path] = args.as_slice() else {
        eprintln!("usage: os_process rust|reference script.lua");
        return ExitCode::FAILURE;
    };
    let source = fs::read(path).expect("read fixed process probe");
    let name = b"=(deadlib-lua)";
    let result = if engine == "rust" {
        let mut vm = deadlib_lua::Vm::new();
        vm.set_instruction_limit(Some(1_000_000));
        deadlib_lua::compile_named(&source, name)
            .and_then(|program| vm.execute(&program).map(|_| ()))
            .map_err(|error| error.to_string())
    } else if engine == "reference" {
        // SAFETY: the differential runner executes fixed local process probes
        // in an isolated child with no third-party native modules.
        let lua = unsafe { lua_reference::Lua::unsafe_new() };
        lua.load(source)
            .set_name("=(deadlib-lua)")
            .exec()
            .map_err(|error| error.to_string())
    } else {
        panic!("unknown engine");
    };
    let _ = std::io::stdout().flush();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
