use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Fixture {
    pub path: PathBuf,
    base: PathBuf,
}

impl Fixture {
    pub fn new(label: &str, files: usize, depth: usize) -> Self {
        let base =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/asset-discovery-fixtures");
        std::fs::create_dir_all(&base).unwrap();
        let base = base.canonicalize().unwrap();
        let path = base.join(format!(
            "{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        for level in 0..=depth {
            let dir = (0..level).fold(path.clone(), |path, index| {
                path.join(format!("Layer{index}"))
            });
            std::fs::create_dir_all(&dir).unwrap();
            for index in 0..files {
                let name = format!(
                    "{:04}-{}_{}.{}",
                    index,
                    if index % 7 == 0 { "日本" } else { "Asset" },
                    level,
                    if index % 3 == 0 { "png" } else { "lua" }
                );
                std::fs::write(dir.join(name), [level as u8, index as u8, 19]).unwrap();
            }
        }
        Self { path, base }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only remove this task's uniquely named directory under the canonical
        // workspace fixture root; remove_dir_all does not follow symlinks.
        assert!(self.path.starts_with(&self.base) && self.path != self.base);
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}

pub fn link(target: &Path, path: &Path, directory: bool) -> bool {
    #[cfg(windows)]
    let result = if directory {
        std::os::windows::fs::symlink_dir(target, path)
    } else {
        std::os::windows::fs::symlink_file(target, path)
    };
    #[cfg(unix)]
    let result = {
        let _ = directory;
        std::os::unix::fs::symlink(target, path)
    };
    #[cfg(windows)]
    if result
        .as_ref()
        .is_err_and(|error| error.raw_os_error() == Some(1314))
    {
        if !directory {
            eprintln!(
                "File-symlink fixture unavailable: Windows privilege 1314; directory junctions are tested."
            );
            return false;
        }
        // Junctions need no symlink privilege. Pass paths through environment
        // variables, never through interpolated PowerShell source.
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "New-Item -ItemType Junction -Path $env:DEADSYNC_LINK_PATH -Target $env:DEADSYNC_LINK_TARGET -ErrorAction Stop | Out-Null"])
            .env("DEADSYNC_LINK_PATH", path.to_string_lossy().trim_start_matches(r"\\?\"))
            .env("DEADSYNC_LINK_TARGET", target.to_string_lossy().trim_start_matches(r"\\?\"))
            .output().unwrap();
        assert!(
            output.status.success(),
            "junction creation: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            std::fs::symlink_metadata(path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(std::fs::metadata(path).unwrap().is_dir());
        return true;
    }
    result.expect("create fixture symlink");
    true
}

pub fn pair<T>(
    name: &str,
    iterations: usize,
    units: usize,
    old: impl FnMut() -> T,
    new: impl FnMut() -> T,
) {
    use crate::perf::measure_sampled;
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        measure_sampled(&format!("{name}_new"), iterations, units, new);
        measure_sampled(&format!("{name}_old"), iterations, units, old);
    } else {
        measure_sampled(&format!("{name}_old"), iterations, units, old);
        measure_sampled(&format!("{name}_new"), iterations, units, new);
    }
}
