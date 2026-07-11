use fs_extra::dir::{CopyOptions, copy};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=assets");

    configure_windows_stack();
    emit_build_info();

    embed_windows_icon()?;

    let target_dir = compute_target_dir()?;
    copy_assets(&target_dir)?;

    Ok(())
}

fn configure_windows_stack() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    const STACK_RESERVE_BYTES: usize = 32 * 1024 * 1024;

    // Gameplay/notefield construction currently uses a large main-thread stack
    // frame, especially in debug builds. Windows binaries default to a 1 MiB
    // stack reserve, which is smaller than the reserve we effectively get on
    // Unix and can overflow while entering gameplay through a window callback.
    match target_env.as_str() {
        "msvc" => println!("cargo:rustc-link-arg-bins=/STACK:{STACK_RESERVE_BYTES}"),
        "gnu" => println!("cargo:rustc-link-arg-bins=-Wl,--stack,{STACK_RESERVE_BYTES}"),
        _ => println!("cargo:rustc-link-arg-bins=/STACK:{STACK_RESERVE_BYTES}"),
    }
}

#[cfg(windows)]
fn embed_windows_icon() -> Result<(), Box<dyn Error>> {
    const WINDOWS_ICON_PATH: &str = "assets/graphics/icon/icon.ico";
    println!("cargo:rerun-if-changed={WINDOWS_ICON_PATH}");
    if fs::metadata(WINDOWS_ICON_PATH).is_err() {
        return Err(format!("missing Windows icon file: {WINDOWS_ICON_PATH}").into());
    }
    let mut res = winres::WindowsResource::new();
    res.set_icon(WINDOWS_ICON_PATH);
    res.compile()?;
    Ok(())
}

#[cfg(not(windows))]
fn embed_windows_icon() -> Result<(), Box<dyn Error>> {
    Ok(())
}

fn compute_target_dir() -> Result<PathBuf, Box<dyn Error>> {
    // Derive the profile output directory (where the final binary lives, e.g.
    // `target/local`) from OUT_DIR rather than the PROFILE env var.
    //
    // Cargo only ever sets PROFILE to "debug" or "release"; it does not reflect
    // custom profiles. A profile like `[profile.local] inherits = "release"`
    // builds into `target/local/` but reports PROFILE=release, so deriving from
    // OUT_DIR keeps copied assets with the binary that will run.
    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        if let Some(profile_dir) = PathBuf::from(out_dir).ancestors().nth(3) {
            return Ok(profile_dir.to_path_buf());
        }
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let profile = std::env::var("PROFILE")?;
    let base = std::env::var("CARGO_TARGET_DIR")
        .map_or_else(|_| manifest_dir.join("target"), PathBuf::from);
    Ok(base.join(profile))
}

fn copy_assets(target_dir: &Path) -> Result<(), Box<dyn Error>> {
    if fs::metadata("assets").is_ok() {
        let mut options = CopyOptions::new();
        options.overwrite = true;
        copy("assets", target_dir, &options)?;
    }
    Ok(())
}

fn emit_build_info() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    // Re-run when the checked-out commit moves, so an otherwise cached build
    // can't keep reporting a stale hash/stamp in the startup banner.
    emit_git_rerun_paths(&manifest_dir);
    let hash = git_output(&manifest_dir, &["rev-parse", "--short=10", "HEAD"])
        .unwrap_or_else(|| "unknown".to_string());
    let stamp = git_output(
        &manifest_dir,
        &[
            "log",
            "-1",
            "--date=format-local:%Y%m%d @ %H:%M:%S",
            "--format=%cd",
            "HEAD",
        ],
    )
    .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=DEADSYNC_BUILD_HASH={hash}");
    println!("cargo:rustc-env=DEADSYNC_BUILD_STAMP={stamp}");
}

/// Tell cargo to watch the git files that change when HEAD moves: `.git/HEAD`
/// (checkouts), `.git/logs/HEAD` (the reflog, appended on every commit and
/// checkout — this is what keeps the hash fresh even when the branch ref is
/// packed or, in a worktree, lives in the common git dir), the ref file HEAD
/// points at, and `packed-refs`. Only existing paths are emitted; a missing
/// path would make cargo re-run the script every build.
fn emit_git_rerun_paths(manifest_dir: &Path) {
    let git_dir = git_output(manifest_dir, &["rev-parse", "--git-dir"])
        .map(|d| manifest_dir.join(d))
        .unwrap_or_else(|| manifest_dir.join(".git"));
    let mut watch = vec![
        git_dir.join("HEAD"),
        git_dir.join("logs/HEAD"),
        git_dir.join("packed-refs"),
    ];
    if let Ok(head) = fs::read_to_string(git_dir.join("HEAD"))
        && let Some(reference) = head.trim().strip_prefix("ref: ")
    {
        watch.push(git_dir.join(reference));
    }
    for path in watch {
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn git_output(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
