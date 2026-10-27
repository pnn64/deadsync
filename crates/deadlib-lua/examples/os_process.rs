//! Isolated process oracle for scripts which may call os.exit.
use std::{env, fs, io::Write, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    let [engine, path] = args.as_slice() else {
        eprintln!("usage: os_process rust|rust-cache|reference script.lua");
        return ExitCode::FAILURE;
    };
    let source = fs::read(path).expect("read fixed process probe");
    if engine == "dump" {
        let lua = lua_reference::Lua::new();
        let function = lua
            .load(source)
            .set_name("=(native process)")
            .into_function()
            .expect("compile fixed binary fixture");
        std::io::stdout()
            .write_all(&function.dump(false))
            .expect("write native fixture");
        return ExitCode::SUCCESS;
    }
    let name = format!(
        "@{}",
        std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
    );
    let result = if engine == "rust" || engine == "rust-cache" {
        let mut vm = deadlib_lua::Vm::new();
        vm.set_instruction_limit(Some(1_000_000));
        let program = if engine == "rust-cache" {
            let directory = env::var_os("DEADLIB_LUA_TEST_CACHE").expect("test cache directory");
            deadlib_lua::ProgramCache::new(directory)
                .compile_named(&source, name.as_bytes())
                .map(|cached| {
                    eprintln!("CACHE\t{:?}", cached.status);
                    cached.program
                })
        } else {
            deadlib_lua::compile_named(&source, name.as_bytes())
        };
        program
            .and_then(|program| vm.execute(&program).map(|_| ()))
            .map_err(|error| error.to_string())
    } else if engine == "reference" {
        // SAFETY: the differential runner executes fixed local process probes
        // in an isolated child with no third-party native modules.
        let lua = unsafe { lua_reference::Lua::unsafe_new() };
        lua.load(source)
            .set_name(name)
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
