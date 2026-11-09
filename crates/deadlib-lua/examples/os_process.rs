//! Isolated Rust process runner for scripts which may call os.exit.
use std::{env, fs, io::Write, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    let [engine, path] = args.as_slice() else {
        eprintln!("usage: os_process rust|rust-cache|dump script.lua");
        return ExitCode::FAILURE;
    };
    let source = fs::read(path).expect("read fixed process probe");
    if engine == "dump" {
        let program = deadlib_lua::compile_named(&source, b"=(native process)")
            .expect("compile fixed binary fixture");
        let mut vm = deadlib_lua::Vm::new();
        let function = vm.load_program(&program);
        vm.set_global(b"fixture", deadlib_lua::Value::Function(function))
            .unwrap();
        let dump = deadlib_lua::compile(b"return string.dump(fixture)").unwrap();
        let deadlib_lua::Value::String(bytes) = vm.execute(&dump).unwrap()[0] else {
            panic!("string.dump must return a string");
        };
        std::io::stdout()
            .write_all(vm.string_bytes(bytes).unwrap())
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
