#[path = "deadlua/cli.rs"]
mod cli;

fn main() -> std::process::ExitCode {
    cli::main()
}
