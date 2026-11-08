#[path = "deadlua/cli.rs"]
mod cli;

fn main() -> std::process::ExitCode {
    // Reentrant C modules retain Rust dispatch frames until the C call returns.
    // Leave enough stack for Lua's 200-call limit even in unoptimized builds;
    // the Windows process entry stack otherwise reserves only one MiB.
    std::thread::Builder::new()
        .name("lua".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(cli::main)
        .expect("create Lua execution thread")
        .join()
        .expect("Lua execution thread panicked")
}
