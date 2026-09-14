use crate::perf;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct Tree {
    pub path: PathBuf,
    root: PathBuf,
}

impl Tree {
    pub fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join(format!(
            "asset-discovery-{:010}-{:010}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path, root }
    }

    pub fn dir(&self, relative: &str) -> PathBuf {
        let path = self.path.join(relative);
        fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        assert_eq!(self.path.parent(), Some(self.root.as_path()));
        assert!(
            self.path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("asset-discovery-")
        );
        fs::remove_dir_all(&self.path).unwrap();
    }
}

pub fn write(path: impl AsRef<Path>) {
    fs::write(path, b"fixture").unwrap();
}

pub fn populate(dir: &Path, accepted: usize, rejected: usize, extension: &str) {
    for i in 0..accepted {
        write(dir.join(format!("asset-{i:04} 2x7.{extension}")));
    }
    for i in 0..rejected {
        write(dir.join(format!("unrelated-{i:04}.txt")));
    }
}

pub fn pair<T>(name: &str, units: usize, old: impl FnMut() -> T, new: impl FnMut() -> T) {
    let run = |variant: &str, work| {
        perf::measure_sampled(&format!("{name}/{variant}"), 16, units.max(1), work);
    };
    // Use trait objects only at the outer operation boundary, equally on both sides.
    let mut old = old;
    let mut new = new;
    let old: &mut dyn FnMut() -> T = &mut old;
    let new: &mut dyn FnMut() -> T = &mut new;
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        run("new", new);
        run("old", old);
    } else {
        run("old", old);
        run("new", new);
    }
}

#[cfg(any(windows, unix))]
pub fn lossy_name(suffix: &str) -> std::ffi::OsString {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let mut units = vec![b'N' as u16, 0xd800];
        units.extend(suffix.encode_utf16());
        std::ffi::OsString::from_wide(&units)
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let mut bytes = vec![b'N', 0xff];
        bytes.extend(suffix.bytes());
        std::ffi::OsString::from_vec(bytes)
    }
}

#[cfg(windows)]
pub fn directory_link(target: &Path, link: &Path) {
    use std::os::windows::process::CommandExt;
    // PowerShell's junction target parser requires ordinary drive paths.
    let plain = |path: &Path| {
        let text = path.to_string_lossy();
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    };
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command",
            "New-Item -ItemType Junction -Path $env:DEADSYNC_TEST_LINK -Target $env:DEADSYNC_TEST_TARGET -ErrorAction Stop | Out-Null"])
        .env("DEADSYNC_TEST_LINK", plain(link))
        .env("DEADSYNC_TEST_TARGET", plain(target))
        .creation_flags(0x0800_0000)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        link.is_dir(),
        "junction must initially resolve to its target"
    );
}

#[cfg(unix)]
pub fn directory_link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(any(windows, unix))]
pub fn file_link(target: &Path, link: &Path) -> bool {
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_file(target, link);
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(target, link);
    if cfg!(windows)
        && result
            .as_ref()
            .is_err_and(|e| e.raw_os_error() == Some(1314))
    {
        eprintln!("SKIPPED file symlink cases: Windows privilege 1314");
        return false;
    }
    result.unwrap();
    true
}
