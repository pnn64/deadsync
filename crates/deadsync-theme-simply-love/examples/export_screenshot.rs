//! Run both Simply Love screenshot exporters in sequence.
#[allow(dead_code)]
#[path = "export_support.rs"]
mod export_support;

use export_support::{PRESETS, Result};
use std::process::Command;

const HELP: &str = "\
Export both Simply Love menu and gameplay screenshots.

cargo run -p deadsync-theme-simply-love --example export_screenshot -- [OPTIONS]

  --preset NAME   Select a preset; repeat to select several (default: all)
  --reverse       Also export reverse-scroll gameplay images
  --help          Show this help

Available presets: vga, svga, xga, 5-4, square, 16-10, 720p, ultrawide,
uwqhd, portrait-9-16, portrait-10-16.";

fn main() -> Result<()> {
    let mut menu_args = Vec::new();
    let mut gameplay_args = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(());
            }
            "--preset" => {
                let name = args.next().ok_or("--preset requires a name")?;
                if !PRESETS.iter().any(|preset| preset.0 == name) {
                    return Err(format!("unknown preset '{name}'; use --help").into());
                }
                menu_args.extend(["--preset".to_owned(), name.clone()]);
                gameplay_args.extend(["--preset".to_owned(), name]);
            }
            "--reverse" => gameplay_args.push(arg),
            _ => return Err(format!("unknown option '{arg}'; use --help").into()),
        }
    }

    run_export("export_menus", &menu_args)?;
    run_export("export_gameplay", &gameplay_args)
}

fn run_export(example: &str, args: &[String]) -> Result<()> {
    let status = Command::new("cargo")
        .current_dir(export_support::workspace_root())
        .args([
            "run",
            "-p",
            "deadsync-theme-simply-love",
            "--example",
            example,
            "--",
        ])
        .args(args)
        .status()?;
    if !status.success() {
        return Err(format!("{example} exited with {status}").into());
    }
    Ok(())
}
