//! Run both Simply Love screenshot exporters in sequence.
#[allow(dead_code)]
#[path = "export_support.rs"]
mod export_support;

use export_support::{PRESETS, Result};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

const HELP: &str = "\
Export both Simply Love menu and gameplay screenshots.

cargo run -p deadsync-theme-simply-love --example export_screenshot -- [OPTIONS]

  --preset NAME   Select a preset; repeat to select several (default: all)
  --reverse       Also export reverse-scroll gameplay images
  --help          Show this help

Available presets: vga, svga, xga, 5-4, square, 16-10, 720p, ultrawide,
uwqhd, portrait-9-16, portrait-10-16.";

fn main() -> Result<()> {
    let mut presets = Vec::new();
    let mut seen_sizes = HashSet::new();
    let mut reverse = false;
    let mut args = std::env::args().skip(1);
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
    build_exporters()?;
    let examples_dir = std::env::current_exe()?
        .parent()
        .ok_or("could not find exporter examples directory")?
        .to_path_buf();
    let menu_exe = exporter_path(&examples_dir, "export_menus")?;
    let gameplay_exe = exporter_path(&examples_dir, "export_gameplay")?;

    run_exporters(&menu_exe, &gameplay_exe, &presets, reverse)?;

    build_index(&menu_exe, &presets, false)?;
    build_index(&gameplay_exe, &presets, reverse)?;
    println!(
        "Screenshot export completed in {:.1}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn build_exporters() -> Result<()> {
    let status = Command::new("cargo")
        .current_dir(export_support::workspace_root())
        .args([
            "build",
            "-p",
            "deadsync-theme-simply-love",
            "--example",
            "export_menus",
            "--example",
            "export_gameplay",
        ])
        .status()?;
    if !status.success() {
        return Err(format!("building screenshot exporters exited with {status}").into());
    }
    Ok(())
}

fn exporter_path(examples_dir: &Path, name: &str) -> Result<PathBuf> {
    let path = examples_dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    if !path.is_file() {
        return Err(format!("exporter executable not found: {}", path.display()).into());
    }
    Ok(path)
}

fn run_exporters(
    menu_exe: &Path,
    gameplay_exe: &Path,
    presets: &[&str],
    reverse: bool,
) -> Result<()> {
    let mut menu_command = Command::new(menu_exe);
    menu_command.arg("--images-only");
    for preset in presets {
        menu_command.arg("--preset").arg(*preset);
    }

    let mut gameplay_command = Command::new(gameplay_exe);
    gameplay_command.arg("--images-only");
    for preset in presets {
        gameplay_command.arg("--preset").arg(*preset);
    }
    if reverse {
        gameplay_command.arg("--reverse");
    }

    let mut menu_child = menu_command.spawn()?;
    let mut gameplay_child = match gameplay_command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = menu_child.wait();
            return Err(error.into());
        }
    };
    let menu_status = menu_child.wait()?;
    let gameplay_status = gameplay_child.wait()?;
    if !menu_status.success() || !gameplay_status.success() {
        return Err(format!(
            "exporter failure (menus: {menu_status}, gameplay: {gameplay_status})"
        )
        .into());
    }
    Ok(())
}

fn build_index(executable: &Path, presets: &[&str], reverse: bool) -> Result<()> {
    let mut command = Command::new(executable);
    command.arg("--index-only");
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
