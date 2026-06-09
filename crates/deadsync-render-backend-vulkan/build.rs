use shaderc::{Compiler, ShaderKind};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=src/shaders");

    let mut compiler = Compiler::new()?;
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    compile_vulkan_shaders(&mut compiler, &out_dir)
}

fn compile_vulkan_shaders(compiler: &mut Compiler, out_dir: &Path) -> Result<(), Box<dyn Error>> {
    use std::{fmt::Write as _, time::SystemTime};

    let mut opts = shaderc::CompileOptions::new()?;
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    if profile == "release" {
        opts.set_optimization_level(shaderc::OptimizationLevel::Performance);
    } else {
        opts.set_optimization_level(shaderc::OptimizationLevel::Zero);
        opts.set_generate_debug_info();
    }

    let mut paths: Vec<_> = fs::read_dir("src/shaders")?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("vulkan_"))
        })
        .collect();
    paths.sort();

    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());

        let kind = match path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
        {
            "vert" => ShaderKind::Vertex,
            "frag" => ShaderKind::Fragment,
            "comp" => ShaderKind::Compute,
            "geom" => ShaderKind::Geometry,
            "tesc" => ShaderKind::TessControl,
            "tese" => ShaderKind::TessEvaluation,
            _ => continue,
        };

        let src_mtime = fs::metadata(&path)?
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let dest_path = out_dir.join(format!("{file_name}.spv"));

        if let Ok(dest_meta) = fs::metadata(&dest_path)
            && let Ok(dest_mtime) = dest_meta.modified()
            && dest_mtime >= src_mtime
        {
            continue;
        }

        let source = fs::read_to_string(&path)?;
        let src_name = path.to_string_lossy();
        let spirv = match compiler.compile_into_spirv(&source, kind, &src_name, "main", Some(&opts))
        {
            Ok(ok) => ok,
            Err(e) => {
                let mut msg = String::new();
                writeln!(&mut msg, "Shader compile failed: {src_name}")?;
                for (i, line) in source.lines().enumerate() {
                    writeln!(&mut msg, "{:4} | {}", i + 1, line)?;
                }
                writeln!(&mut msg, "\nError: {e}")?;
                return Err(msg.into());
            }
        };

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
