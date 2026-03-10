use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

const LOG_FILE_PATH: &str = "deadsync.log";
static FILE_LOGGING_ENABLED: AtomicBool = AtomicBool::new(true);

struct TeeWriter {
    stderr: io::Stderr,
    file: Option<File>,
}

impl TeeWriter {
    fn new() -> Self {
        Self {
            stderr: io::stderr(),
            file: open_log_file(),
        }
    }

    fn write_file(&mut self, buf: &[u8]) {
        if !FILE_LOGGING_ENABLED.load(Ordering::Relaxed) {
            return;
        }
        if let Some(file) = &mut self.file {
            let _ = file.write_all(buf);
        }
    }

    fn flush_file(&mut self) {
        if !FILE_LOGGING_ENABLED.load(Ordering::Relaxed) {
            return;
        }
        if let Some(file) = &mut self.file {
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

pub fn init(file_logging_enabled: bool) {
    FILE_LOGGING_ENABLED.store(file_logging_enabled, Ordering::Relaxed);
    let mut builder = env_logger::builder();
    builder
        .filter_level(log::LevelFilter::Trace)
        // Keep GPU stack internals quiet even when the app log level is Trace.
        .filter_module("wgpu", log::LevelFilter::Warn)
        .filter_module("wgpu_core", log::LevelFilter::Warn)
        .filter_module("wgpu_hal", log::LevelFilter::Warn)
        .filter_module("wgpu_types", log::LevelFilter::Warn)
        .filter_module("naga", log::LevelFilter::Warn)
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
