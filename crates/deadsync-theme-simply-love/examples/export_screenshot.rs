//! Single-executable dispatcher for Simply Love screenshot exports.
#[path = "gameplay/export_gameplay.rs"]
mod export_gameplay;
#[path = "menus/export_menus.rs"]
mod export_menus;
#[allow(dead_code)]
#[path = "export_support.rs"]
mod export_support;

use export_support::{PRESETS, Result};
use std::{collections::HashSet, path::Path, process::Command, time::Instant};

const HELP: &str = "\
Export Simply Love screenshots from one executable.

cargo run -p deadsync-theme-simply-love --example export_screenshot -- [MODE] [OPTIONS]

    MODE              all (default), menus/export_menus, or gameplay/export_gameplay

  --preset NAME   Select a preset; repeat to select several (default: all)
    --reverse       Export reverse-scroll gameplay images
  --help          Show this help

Available presets: vga, svga, xga, 5-4, square, 16-10, 720p, ultrawide,
uwqhd, portrait-9-16, portrait-10-16.";
const SCREENSHOT_WITHOUT_SURFACE: &str = "DEADSYNC_SCREENSHOT_WITHOUT_SURFACE";

fn main() -> Result<()> {
    // SAFETY: this CLI sets the variable before initializing libraries or spawning worker threads.
    unsafe { std::env::set_var(SCREENSHOT_WITHOUT_SURFACE, "1") };
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let mode = match args.first().map(String::as_str) {
        Some("menus" | "export_menus" | "gameplay" | "export_gameplay" | "all") => args.remove(0),
        Some("-h" | "--help") => {
            println!("{HELP}");
            return Ok(());
        }
        Some(argument) if !argument.starts_with('-') => {
            return Err(format!("unknown mode '{argument}'; use --help").into());
        }
        _ => "all".to_owned(),
    };
    match mode.as_str() {
        "menus" | "export_menus" => export_menus::run(args),
        "gameplay" | "export_gameplay" => export_gameplay::run(args),
        "all" => run_all(args),
        _ => unreachable!(),
    }
}

fn run_all(args: Vec<String>) -> Result<()> {
    let mut presets = Vec::new();
    let mut seen_sizes = HashSet::new();
    let mut reverse = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(());
            }
            "--preset" => {
                let name = args.next().ok_or("--preset requires a name")?;
                let preset = PRESETS
                    .iter()
                    .find(|preset| preset.0 == name)
                    .ok_or_else(|| format!("unknown preset '{name}'; use --help"))?;
                if seen_sizes.insert((preset.1, preset.2)) {
                    presets.push(preset.0);
                }
            }
            "--reverse" => reverse = true,
            _ => return Err(format!("unknown option '{arg}'; use --help").into()),
        }
    }

    if presets.is_empty() {
        for &(name, width, height) in PRESETS {
            if seen_sizes.insert((width, height)) {
                presets.push(name);
            }
        }
    }

    let started = Instant::now();
    let executable = std::env::current_exe()?;

    run_exporters(&executable, &presets, reverse)?;

    build_index(&executable, "menus", &presets, false)?;
    build_index(&executable, "gameplay", &presets, reverse)?;
    println!(
        "Screenshot export completed in {:.1}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn run_exporters(executable: &Path, presets: &[&str], reverse: bool) -> Result<()> {
    let mut menu_command = Command::new(executable);
    menu_command.arg("menus").arg("--images-only");
    for preset in presets {
        menu_command.arg("--preset").arg(*preset);
    }

    let mut gameplay_command = Command::new(executable);
    gameplay_command.arg("gameplay").arg("--images-only");
    for preset in presets {
        gameplay_command.arg("--preset").arg(*preset);
    }
    if reverse {
        gameplay_command.arg("--reverse");
    }

    let menu_status = menu_command.status()?;
    if !menu_status.success() {
        return Err(format!("menu exporter exited with {menu_status}").into());
    }
    let gameplay_status = gameplay_command.status()?;
    if !gameplay_status.success() {
        return Err(format!("gameplay exporter exited with {gameplay_status}").into());
    }
    Ok(())
}

fn build_index(executable: &Path, mode: &str, presets: &[&str], reverse: bool) -> Result<()> {
    let mut command = Command::new(executable);
    command.arg(mode).arg("--index-only");
    for preset in presets {
        command.arg("--preset").arg(*preset);
    }
    if reverse {
        command.arg("--reverse");
    }
    let status = command.status()?;
    if !status.success() {
        return Err(format!(
            "building index with {} exited with {status}",
            executable.display()
        )
        .into());
    }
    Ok(())
}
