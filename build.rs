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
    println!("cargo:rerun-if-changed=src/core/gfx/shaders");
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

    let mut compiler = Compiler::new()?;

    // OUT_DIR used by include_bytes! in Vulkan source
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);

    // 32-bit targets compile without Vulkan backends, so skip the shader pass there.
    if std::env::var("CARGO_CFG_TARGET_POINTER_WIDTH").as_deref() != Ok("32") {
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

fn configure_windows_stack() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }
    // Gameplay/notefield construction currently uses a large main-thread stack
    // frame in debug builds. Windows binaries default to a 1 MiB stack reserve,
    // which is smaller than the reserve we effectively get on Unix and has
    // started overflowing when entering gameplay.
    println!("cargo:rustc-link-arg-bins=/STACK:8388608");
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
    let mut paths: Vec<_> = glob::glob("src/core/gfx/shaders/vulkan_*.*")?
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
