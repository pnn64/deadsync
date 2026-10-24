use deadlib_lua::{Vm, compile};
use std::{
    env, fs,
    io::{self, Read},
    process::ExitCode,
};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    let source = match args.as_slice() {
        [flag, text] if flag == "-e" => text.as_bytes().to_vec(),
        [path] => fs::read(path)?,
        [] => {
            let mut bytes = Vec::new();
            io::stdin().read_to_end(&mut bytes)?;
            bytes
        }
        _ => return Err("usage: deadlua [-e source | script.lua] (or read stdin)".into()),
    };
    let program = compile(&source)?;
    let mut vm = Vm::new();
    vm.execute(&program)?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
