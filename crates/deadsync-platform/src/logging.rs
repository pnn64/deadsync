use chrono::{DateTime, Local};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

#[cfg(not(windows))]
use std::ffi::OsStr;

const LOG_FILE_PATH_FALLBACK: &str = "deadsync.log";
/// Total number of log files to retain: the live `deadsync.log` plus the most
/// recent `MAX_LOG_FILES - 1` timestamped backups of previous runs.
const MAX_LOG_FILES: usize = 3;
static FILE_LOGGING_ENABLED: AtomicBool = AtomicBool::new(true);
static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static LOG_FILE_PATH: OnceLock<PathBuf> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub struct StartupBuildInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub build_hash: &'static str,
    pub build_stamp: &'static str,
}

struct TeeWriter {
    stderr: io::Stderr,
}

impl TeeWriter {
    fn new() -> Self {
        Self {
            stderr: io::stderr(),
        }
    }

    fn write_file(&mut self, buf: &[u8]) {
        if !FILE_LOGGING_ENABLED.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(mut slot) = log_file_slot().lock()
            && let Some(file) = slot.as_mut()
        {
            let _ = file.write_all(buf);
        }
    }

    fn flush_file(&mut self) {
        if !FILE_LOGGING_ENABLED.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(mut slot) = log_file_slot().lock()
            && let Some(file) = slot.as_mut()
        {
            let _ = file.flush();
        }
    }
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stderr.write_all(buf)?;
        self.write_file(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stderr.flush()?;
        self.flush_file();
        Ok(())
    }
}

fn log_file_slot() -> &'static Mutex<Option<File>> {
    LOG_FILE.get_or_init(|| Mutex::new(None))
}

fn reset_log_file() {
    if let Ok(mut slot) = log_file_slot().lock() {
        *slot = open_log_file();
    }
}

fn log_file_path() -> PathBuf {
    LOG_FILE_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from(LOG_FILE_PATH_FALLBACK))
}

/// Rotates logs so the previous run is preserved under a timestamped name.
///
/// `deadsync.log` always points at the current run. Before it is truncated the
/// existing file is archived as `deadsync-YYYY-MM-DD_HHMMSS.log` (stamped from
/// its last-modified time), then older backups are pruned so at most
/// `keep` files total remain on disk (the live log plus `keep - 1` backups).
fn rotate_log_files(path: &Path, keep: usize) {
    let backups = match keep.checked_sub(1) {
        Some(n) if n > 0 => n,
        // Only the live log is retained; truncation alone handles that.
        _ => return,
    };
    if path.exists()
        && let Some(dest) = timestamped_backup_path(path)
    {
        let _ = std::fs::rename(path, dest);
    }
    prune_old_logs(path, backups);
}

/// Builds a unique timestamped backup path next to `path`, e.g.
/// `deadsync.log` -> `deadsync-2026-06-15_175005.log`. A numeric suffix is
/// appended if a backup from the same second already exists.
fn timestamped_backup_path(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    let modified = std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .unwrap_or_else(|_| SystemTime::now());
    let stamp = DateTime::<Local>::from(modified)
        .format("%Y-%m-%d_%H%M%S")
        .to_string();
    let name = |suffix: &str| match &ext {
        Some(ext) => format!("{stem}-{stamp}{suffix}.{ext}"),
        None => format!("{stem}-{stamp}{suffix}"),
    };
    let mut candidate = dir.join(name(""));
    let mut counter = 1;
    while candidate.exists() {
        candidate = dir.join(name(&format!("-{counter}")));
        counter += 1;
    }
    Some(candidate)
}

/// Deletes the oldest timestamped backups, keeping the newest `keep`.
fn prune_old_logs(path: &Path, keep: usize) {
    let dir = path.parent().unwrap_or_else(|| Path::new(""));
    let scan_dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
        return;
    };
    let prefix = format!("{stem}-");
    let suffix = path.extension().map(|e| format!(".{}", e.to_string_lossy()));
    let Ok(entries) = std::fs::read_dir(scan_dir) else {
        return;
    };
    let mut backups: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with(&prefix) {
                return None;
            }
            if let Some(suffix) = &suffix
                && !name.ends_with(suffix)
            {
                return None;
            }
            let path = entry.path();
            path.is_file().then_some(path)
        })
        .collect();
    if backups.len() <= keep {
        return;
    }
    // Timestamped names sort chronologically, so the oldest sort first.
    backups.sort();
    let remove_count = backups.len() - keep;
    for old in backups.into_iter().take(remove_count) {
        let _ = std::fs::remove_file(old);
    }
}

fn open_log_file() -> Option<File> {
    let path = log_file_path();
    if FILE_LOGGING_ENABLED.load(Ordering::Relaxed) {
        rotate_log_files(&path, MAX_LOG_FILES);
    }
    match OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
    {
        Ok(file) => Some(file),
        Err(err) => {
            eprintln!("Failed to open '{}' for logging: {err}", path.display());
            None
        }
    }
}

fn emit_info_line(message: &str) {
    let args = format_args!("{message}");
    let record = log::Record::builder()
        .args(args)
        .level(log::Level::Info)
        .target("startup")
        .build();
    log::logger().log(&record);
}

#[cfg(target_os = "linux")]
fn os_summary() -> String {
    let pretty = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|content| {
            content.lines().find_map(|line| {
                let value = line.strip_prefix("PRETTY_NAME=")?;
                Some(value.trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| "Linux".to_string());
    format!(
        "{pretty} [{} {}]",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

#[cfg(not(target_os = "linux"))]
fn os_summary() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

#[inline(always)]
fn logical_cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(NonZeroUsize::get)
        .unwrap_or(1)
}

#[cfg(target_os = "linux")]
fn cpu_name() -> Option<String> {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()?
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            matches!(key.trim(), "model name" | "Processor" | "Hardware")
                .then(|| value.trim().to_string())
        })
        .filter(|name| !name.is_empty())
}

#[cfg(windows)]
fn cpu_name() -> Option<String> {
    std::env::var("PROCESSOR_IDENTIFIER")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn cpu_name() -> Option<String> {
    None
}

fn cpu_summary() -> String {
    let logical = logical_cpu_count();
    cpu_name().map_or_else(
        || format!("{logical} logical cores"),
        |name| format!("{name} ({logical} logical cores)"),
    )
}

#[cfg(target_os = "linux")]
fn memory_summary() -> Option<String> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let total = meminfo_kib(&meminfo, "MemTotal")?;
    let swap_total = meminfo_kib(&meminfo, "SwapTotal").unwrap_or(0);
    let swap_free = meminfo_kib(&meminfo, "SwapFree").unwrap_or(0);
    Some(if swap_total > 0 {
        format!(
            "{} total, {} swap ({} swap avail)",
            kib_to_mb(total),
            kib_to_mb(swap_total),
            kib_to_mb(swap_free)
        )
    } else {
        format!("{} total", kib_to_mb(total))
    })
}

#[cfg(not(target_os = "linux"))]
fn memory_summary() -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
fn meminfo_kib(meminfo: &str, key: &str) -> Option<u64> {
    meminfo.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim() != key {
            return None;
        }
        value
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u64>().ok())
    })
}

#[cfg(target_os = "linux")]
#[inline(always)]
fn kib_to_mb(kib: u64) -> u64 {
    kib / 1024
}

#[cfg(windows)]
fn path_volume(path: &Path) -> String {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => prefix.as_os_str().to_string_lossy().to_string(),
        _ => "<unknown>".to_string(),
    }
}

#[cfg(not(windows))]
fn path_volume(path: &Path) -> String {
    let mut parts = path.components();
    match (parts.next(), parts.next(), parts.next()) {
        (
            Some(Component::RootDir),
            Some(Component::Normal(mnt)),
            Some(Component::Normal(drive)),
        ) if mnt == OsStr::new("mnt") => {
            format!("/mnt/{}", drive.to_string_lossy())
        }
        (Some(Component::RootDir), ..) => "/".to_string(),
        _ => "<unknown>".to_string(),
    }
}

pub fn write_startup_report(build: StartupBuildInfo, lines: &[String]) {
    emit_info_line(&format!(
        "{} {} (build {}, {})",
        build.name, build.version, build.build_hash, build.build_stamp
    ));
    emit_info_line("--------------------------------------");
    emit_info_line(&format!(
        "Log starting {}",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    emit_info_line(&format!("OS: {}", os_summary()));
    emit_info_line(&format!("CPU: {}", cpu_summary()));
    if let Some(memory) = memory_summary() {
        emit_info_line(&format!("Memory: {memory}"));
    }
    if let Ok(exe) = std::env::current_exe() {
        emit_info_line(&format!("Executable: {}", exe.display()));
    }
    if let Ok(cwd) = std::env::current_dir() {
        emit_info_line(&format!("Runtime dir: {}", cwd.display()));
        emit_info_line(&format!("Drive: {}", path_volume(&cwd)));
    }
    for line in lines {
        emit_info_line(line);
    }
}

pub fn write_report_block(title: &str, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    emit_info_line(&format!("{title}:"));
    for line in lines {
        emit_info_line(line);
    }
}

pub fn init(file_logging_enabled: bool, log_file_path: PathBuf) {
    FILE_LOGGING_ENABLED.store(file_logging_enabled, Ordering::Relaxed);
    let _ = LOG_FILE_PATH.set(log_file_path);
    reset_log_file();
    let mut builder = env_logger::builder();
    builder
        .filter_level(log::LevelFilter::Trace)
        // Keep GPU stack internals quiet even when the app log level is Trace.
        .filter_module("wgpu", log::LevelFilter::Warn)
        .filter_module("wgpu_core", log::LevelFilter::Warn)
        .filter_module("wgpu_hal", log::LevelFilter::Warn)
        .filter_module("wgpu_types", log::LevelFilter::Warn)
        .filter_module("naga", log::LevelFilter::Warn)
        // Keep window loop internals quiet; calloop trace logs can flood files.
        .filter_module("calloop", log::LevelFilter::Warn)
        // Never emit raw ureq proto dumps; they can include sensitive request headers.
        .filter_module("ureq_proto::util", log::LevelFilter::Off)
        // Keep HTTP client internals quiet unless warning/error.
        .filter_module("ureq_proto", log::LevelFilter::Debug)
        .filter_module("ureq", log::LevelFilter::Debug)
        .target(env_logger::Target::Pipe(Box::new(TeeWriter::new())));
    let _ = builder.try_init();
}

#[inline(always)]
pub fn set_file_logging_enabled(enabled: bool) {
    FILE_LOGGING_ENABLED.store(enabled, Ordering::Relaxed);
}
