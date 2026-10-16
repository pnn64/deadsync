//! Replay native gameplay observations through the production Lua screen.
#[path = "song_lua_itgmania_semantic_parity/native_gameplay.rs"]
mod native_gameplay;
#[path = "support/paths.rs"]
mod paths;

use std::{path::Path, process::ExitCode};

fn main() -> ExitCode {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    let scope = if args.last().is_some_and(|arg| arg == "--first-query") {
        args.pop();
        native_gameplay::Scope::FirstQuery
    } else if args.last().is_some_and(|arg| arg == "--entry") {
        args.pop();
        native_gameplay::Scope::Entry
    } else {
        native_gameplay::Scope::Full
    };
    if args.is_empty() {
        eprintln!(
            "cargo test --test live_song_lua -- <trace.json> <simfile> <report.json> [--entry|--first-query]"
        );
        return ExitCode::SUCCESS;
    }
    if args.len() != 3 {
        eprintln!("expected native trace, simfile and report path");
        return ExitCode::FAILURE;
    }
    let trace: serde_json::Value = match std::fs::read(&args[0]).map_err(|e| e.to_string())
        .and_then(|bytes| if Path::new(&args[0]).extension().is_some_and(|e|e=="zst") {
            zstd::stream::decode_all(bytes.as_slice()).map_err(|e|e.to_string())
        } else { Ok(bytes) })
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string())) {
        Ok(trace) => trace,
        Err(error) => { eprintln!("cannot read native capture: {error}"); return ExitCode::FAILURE; }
    };
    match native_gameplay::replay(&trace, Path::new(&args[0]), Path::new(&args[1]), None, scope, |used, total| {
        if used % 256 == 0 || used == total { eprintln!("replay {used}/{total}"); }
    }) {
        Ok(report) => {
            match serde_json::to_vec_pretty(&report)
                .map_err(|error| error.to_string())
                .and_then(|bytes| {
                    std::fs::write(&args[2], bytes).map_err(|error| error.to_string())
                }) {
                Ok(()) if report["interactive_checks_passed"] == true => ExitCode::SUCCESS,
                Ok(()) => ExitCode::from(2), // Other diagnostic coverage does not certify an archive.
                Err(error) => {
                    eprintln!("could not write report: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(error) => {
            eprintln!("live replay failed: {error}");
            ExitCode::FAILURE
        }
    }
}
