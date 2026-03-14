use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

#[cfg(not(windows))]
use std::ffi::OsStr;

const LOG_FILE_PATH: &str = "deadsync.log";
static FILE_LOGGING_ENABLED: AtomicBool = AtomicBool::new(true);
static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

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

fn open_log_file() -> Option<File> {
    match OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(LOG_FILE_PATH)
    {
        Ok(file) => Some(file),
        Err(err) => {
            eprintln!("Failed to open '{LOG_FILE_PATH}' for logging: {err}");
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

pub fn write_startup_report(lines: &[String]) {
    emit_info_line(&format!(
        "deadsync {} (build {}, {})",
        env!("CARGO_PKG_VERSION"),
        option_env!("DEADSYNC_BUILD_HASH").unwrap_or("unknown"),
        option_env!("DEADSYNC_BUILD_STAMP").unwrap_or("unknown")
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

pub fn init(file_logging_enabled: bool) {
    FILE_LOGGING_ENABLED.store(file_logging_enabled, Ordering::Relaxed);
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
