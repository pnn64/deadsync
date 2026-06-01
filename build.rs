use fs_extra::dir::{CopyOptions, copy};
use shaderc::{Compiler, ShaderKind};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> Result<(), Box<dyn Error>> {
    // Rerun on shader or asset changes
    println!("cargo:rerun-if-changed=src/engine/gfx/shaders");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_PIPEWIRE_AUDIO");
    println!("cargo:rustc-check-cfg=cfg(has_jack_audio)");
    println!("cargo:rustc-check-cfg=cfg(has_pipewire_audio)");
    println!("cargo:rustc-check-cfg=cfg(has_pulse_audio)");

    detect_jack_audio();
    detect_pipewire_audio();
    detect_pulse_audio();
    configure_windows_stack();
    emit_build_info();

    embed_windows_icon()?;

    if has_vulkan_backend() {
        let mut compiler = Compiler::new()?;
        // OUT_DIR used by include_bytes! in Vulkan source.
        let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
        compile_vulkan_shaders(&mut compiler, &out_dir)?;
    }

    // Copy assets into target/<profile>
    let target_dir = compute_target_dir()?;
    copy_assets(&target_dir)?;

    Ok(())
}

fn detect_jack_audio() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "linux" {
        println!("cargo:rustc-cfg=has_jack_audio");
    }
}

fn detect_pipewire_audio() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "linux" {
        return;
    }
    if std::env::var_os("CARGO_FEATURE_PIPEWIRE_AUDIO").is_none() {
        return;
    }
    println!("cargo:rustc-cfg=has_pipewire_audio");
}

fn detect_pulse_audio() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "linux" {
        println!("cargo:rustc-cfg=has_pulse_audio");
    }
}

fn has_vulkan_backend() -> bool {
    std::env::var("CARGO_CFG_TARGET_POINTER_WIDTH").as_deref() != Ok("32")
        && std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() != Ok("win7")
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

fn compile_vulkan_shaders(compiler: &mut Compiler, out_dir: &Path) -> Result<(), Box<dyn Error>> {
    use std::{fmt::Write as _, time::SystemTime};

    fn kind_for_ext(ext: &str) -> Option<ShaderKind> {
        match ext {
            "vert" => Some(ShaderKind::Vertex),
            "frag" => Some(ShaderKind::Fragment),
            "comp" => Some(ShaderKind::Compute),
            "geom" => Some(ShaderKind::Geometry),
            "tesc" => Some(ShaderKind::TessControl),
            "tese" => Some(ShaderKind::TessEvaluation),
            _ => None,
        }
    }

    let mut opts = shaderc::CompileOptions::new()?;
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    if profile == "release" {
        opts.set_optimization_level(shaderc::OptimizationLevel::Performance);
    } else {
        opts.set_optimization_level(shaderc::OptimizationLevel::Zero);
        opts.set_generate_debug_info();
    }

    // Gather candidates deterministically
    let mut paths: Vec<_> = glob::glob("src/engine/gfx/shaders/vulkan_*.*")?
        .filter_map(Result::ok)
        .collect();
    paths.sort();

    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());

        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let Some(kind) = kind_for_ext(ext) else {
            continue;
        };

        let src_meta = fs::metadata(&path)?;
        let src_mtime = src_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let dest_path = out_dir.join(format!("{file_name}.spv"));

        // Timestamp short-circuit (fast path)
        if let Ok(dest_meta) = fs::metadata(&dest_path)
            && let Ok(dest_mtime) = dest_meta.modified()
            && dest_mtime >= src_mtime
        {
            // .spv is up-to-date for this profile/options — skip
            continue;
        }

        let source = fs::read_to_string(&path)?;
        let src_name = path.to_string_lossy();

        let spirv = match compiler.compile_into_spirv(&source, kind, &src_name, "main", Some(&opts))
        {
            Ok(ok) => ok,
            Err(e) => {
                // Pretty error with annotated source
                let mut msg = String::new();
                writeln!(&mut msg, "Shader compile failed: {src_name}")?;
                for (i, line) in source.lines().enumerate() {
                    writeln!(&mut msg, "{:4} | {}", i + 1, line)?;
                }
                writeln!(&mut msg, "\nError: {e}")?;
                return Err(msg.into());
            }
        };

        // Byte-compare to avoid touching mtime if unchanged
        let new_bytes = spirv.as_binary_u8();
        let needs_write = match fs::read(&dest_path) {
            Ok(old_bytes) => old_bytes != new_bytes,
            Err(_) => true,
        };
        if needs_write {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest_path, new_bytes)?;
        }
    }
    Ok(())
}

fn compute_target_dir() -> Result<PathBuf, Box<dyn Error>> {
    // Derive the profile output directory (where the final binary lives, e.g.
    // `target/local`) from OUT_DIR rather than the PROFILE env var.
    //
    // Cargo only ever sets PROFILE to "debug" or "release" — it does NOT reflect
    // custom profiles. A profile like `[profile.local] inherits = "release"`
    // builds into `target/local/` but reports PROFILE=release, so the old logic
    // copied assets to `target/release/` and left the running `target/local`
    // binary with stale assets (e.g. missing language keys).
    //
    // OUT_DIR is always `<target>/<profile>/build/<crate>-<hash>/out`, so its
    // third ancestor is the real profile output directory for any profile.
    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        if let Some(profile_dir) = PathBuf::from(out_dir).ancestors().nth(3) {
            return Ok(profile_dir.to_path_buf());
        }
    }

    // Fallback: best-effort reconstruction from PROFILE (only correct for the
    // built-in debug/release profiles).
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
        // The default behavior (copy_inside=false) is correct.
        // It copies the `assets` directory itself into `target_dir`.
        copy("assets", target_dir, &options)?;
    }
    Ok(())
}

fn emit_build_info() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
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
