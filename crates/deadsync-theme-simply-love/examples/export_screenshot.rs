//! Run both Simply Love screenshot exporters in sequence.
#[allow(dead_code)]
#[path = "export_support.rs"]
mod export_support;

use export_support::{PRESETS, Result};
use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::Instant,
};

const HELP: &str = "\
Export both Simply Love menu and gameplay screenshots.

cargo run -p deadsync-theme-simply-love --example export_screenshot -- [OPTIONS]

  --preset NAME   Select a preset; repeat to select several (default: all)
  --reverse       Also export reverse-scroll gameplay images
    --jobs COUNT    Run 1-5 exporters concurrently (default: available CPU parallelism, max 16)
  --help          Show this help

Available presets: vga, svga, xga, 5-4, square, 16-10, 720p, ultrawide,
uwqhd, portrait-9-16, portrait-10-16.";

struct ExportTask {
    executable: PathBuf,
    preset: &'static str,
}

fn main() -> Result<()> {
    let mut presets = Vec::new();
    let mut seen_sizes = HashSet::new();
    let mut jobs = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(12);
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
            "--jobs" => {
                let count = args.next().ok_or("--jobs requires a count")?;
                jobs = count.parse()?;
                if !(1..=12).contains(&jobs) {
                    return Err("--jobs must be between 1 and 12".into());
                }
            }
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

    let mut tasks = VecDeque::new();
    for &preset in &presets {
        tasks.push_back(ExportTask {
            executable: menu_exe.clone(),
            preset,
        });
        tasks.push_back(ExportTask {
            executable: gameplay_exe.clone(),
            preset,
        });
    }
    run_tasks(tasks, jobs)?;

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

fn run_tasks(tasks: VecDeque<ExportTask>, jobs: usize) -> Result<()> {
    let tasks = Mutex::new(tasks);
    let failures = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| {
                loop {
                    let task = tasks.lock().expect("task queue poisoned").pop_front();
                    let Some(task) = task else {
                        break;
                    };
                    if let Err(error) = run_task(&task) {
                        failures
                            .lock()
                            .expect("failure list poisoned")
                            .push(error.to_string());
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().expect("failure list poisoned");
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n").into())
    }
}

fn run_task(task: &ExportTask) -> Result<()> {
    let status = Command::new(&task.executable)
        .args(["--images-only", "--preset", task.preset])
        .status()?;
    if !status.success() {
        return Err(format!(
            "{} for preset '{}' exited with {status}",
            task.executable.display(),
            task.preset
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
