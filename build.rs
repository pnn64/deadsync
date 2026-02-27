use fs_extra::dir::{CopyOptions, copy};
use shaderc::{Compiler, ShaderKind};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn Error>> {
    // Rerun on shader or asset changes
    println!("cargo:rerun-if-changed=src/core/gfx/shaders");
    println!("cargo:rerun-if-changed=assets");

    embed_windows_icon()?;

    let mut compiler = Compiler::new()?;

    // OUT_DIR used by include_bytes! in Vulkan source
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);

    // Compile Vulkan shaders with optimization based on the build profile.
    compile_vulkan_shaders(&mut compiler, &out_dir)?;

    // Copy assets into target/<profile>
    let target_dir = compute_target_dir()?;
    copy_assets(&target_dir)?;

    Ok(())
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
