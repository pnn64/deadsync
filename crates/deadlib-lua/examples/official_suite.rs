//! Single-file official suite probe; run by scripts/check_rust_lua_suite.py.
use deadlib_lua::{Value, Vm, compile, compile_named};
use std::{env, fs, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    let [engine, path] = args.as_slice() else {
        eprintln!("usage: official_suite rust|reference path.lua");
        return ExitCode::FAILURE;
    };
    let source = fs::read(path).expect("read test source");
    let name = format!(
        "@{}",
        Path::new(path).file_name().unwrap().to_string_lossy()
    );
    let bootstrap = b"_U=true; function Message(s) print(s) end";
    if engine == "rust" {
        let mut vm = Vm::new();
        vm.set_output_capture(true);
        vm.set_instruction_limit(Some(100_000_000));
        vm.execute(&compile(bootstrap).unwrap()).unwrap();
        let outcome = compile_named(&source, name.as_bytes())
            .and_then(|p| vm.execute(&p).map(|v| v.to_vec()));
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
    } else if engine == "reference" {
        // SAFETY: this developer-only runner executes pinned official Lua tests in
        // a dedicated test directory; no untrusted native modules are supplied.
        let lua = unsafe { lua_reference::Lua::unsafe_new() };
        lua.load(bootstrap.as_slice()).exec().unwrap();
        match lua
            .load(source)
            .set_name(name)
            .eval::<lua_reference::MultiValue>()
        {
            Ok(values) => {
                eprintln!("PASS\t{} return values", values.len());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("FAIL\t{e}");
                ExitCode::FAILURE
            }
        }
    } else {
        eprintln!("unknown engine: {engine}");
        ExitCode::FAILURE
    }
}
