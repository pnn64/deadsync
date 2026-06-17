//! In-app downloader for the `ffmpeg`/`ffprobe` runtime tools.
//!
//! We deliberately don't bundle these binaries in releases; this module
//! fetches a pinned, SHA256-verified build on demand into the runtime
//! `bin/` dir, where `deadlib-video` picks it up (falling back to `PATH`).
//!
//! ```text
//!   Idle ──(request_install)──► Confirm ──(request_confirm)──► Downloading ──► Extracting ──► Installed
//!                                  │                                                  │
//!                                  └──► (no source for host) ──► Unsupported          └──► Error
//! ```

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::action::ActionErrorKind;
use crate::download::{download_to_file, parse_hex32};
use crate::{ReleaseAsset, UpdaterError, download_agent, io_err_at, io_err_op};

/// Subdirectory of the cache dir where ffmpeg archives are staged.
pub const DOWNLOADS_SUBDIR: &str = "ffmpeg";

/// One downloadable archive for a host's ffmpeg source; may contain
/// `ffmpeg`, `ffprobe`, or both (extracted by basename).
#[derive(Clone, Copy, Debug)]
pub struct FfmpegArchive {
    /// Direct download URL of the zip archive.
    pub url: &'static str,
    /// Pinned lower-case hex SHA256 of the archive.
    pub sha256: &'static str,
    /// Archive size in bytes, or 0 when unknown (falls back to the
    /// server `Content-Length`).
    pub size: u64,
}

/// The pinned ffmpeg build for one host triplet.
#[derive(Clone, Copy, Debug)]
pub struct FfmpegSource {
    /// Version string shown in the overlay (e.g. `"7.0"`).
    pub version: &'static str,
    /// Where the build comes from, shown so users know what they're fetching.
    pub origin: &'static str,
    /// Archives that together provide `ffmpeg` + `ffprobe`.
    pub archives: &'static [FfmpegArchive],
}

impl FfmpegSource {
    /// Sum of all archive sizes, or `None` if any archive size is unknown.
    pub fn total_size(&self) -> Option<u64> {
        let mut total = 0u64;
        for archive in self.archives {
            if archive.size == 0 {
                return None;
            }
            total = total.saturating_add(archive.size);
        }
        Some(total)
    }
}

/* ---------- pinned per-host sources ---------- */

// Modern Windows x86_64 (win10+): gyan.dev's latest "essentials" build.
static GYAN_WIN_X64: FfmpegSource = FfmpegSource {
    version: "8.1.1",
    origin: "gyan.dev",
    archives: &[FfmpegArchive {
        url: "https://github.com/GyanD/codexffmpeg/releases/download/8.1.1/ffmpeg-8.1.1-essentials_build.zip",
        sha256: "6f58ce889f59c311410f7d2b18895b33c03456463486f3b1ebc93d97a0f54541",
        size: 109_282_242,
    }],
};

// Windows 7 x86_64: a Win7-compatible build we host, since modern
// gyan.dev releases drop Windows 7 support.
static DEADSYNC_WIN7_X64: FfmpegSource = FfmpegSource {
    version: "7.1.1",
    origin: "deadsync.dance",
    archives: &[FfmpegArchive {
        url: "https://deadsync.dance/ffmpeg-7.1.1-x86_64-win7.zip",
        sha256: "ff0d219295550e4faee20c69683e518de918343cfebef1710ba11bf975b0e91f",
        size: 197_446_383,
    }],
};

// Linux / macOS: Martin Riedl's static "release" builds, shipping
// `ffmpeg` and `ffprobe` as separate archives.
static MR_LINUX_X64: FfmpegSource = FfmpegSource {
    version: "8.1.1",
    origin: "ffmpeg.martin-riedl.de",
    archives: &[
        FfmpegArchive {
            url: "https://ffmpeg.martin-riedl.de/download/linux/amd64/1778762264_8.1.1/ffmpeg.zip",
            sha256: "50b9360d9f0de1555bb4dd354c708427027562624d553e93bb26060059bef16a",
            size: 0,
        },
        FfmpegArchive {
            url: "https://ffmpeg.martin-riedl.de/download/linux/amd64/1778762264_8.1.1/ffprobe.zip",
            sha256: "35595494d31fb0d15948199bc7242265929a6dcdb74d7a097bd3f373f840dbeb",
            size: 0,
        },
    ],
};

static MR_LINUX_ARM64: FfmpegSource = FfmpegSource {
    version: "8.1.1",
    origin: "ffmpeg.martin-riedl.de",
    archives: &[
        FfmpegArchive {
            url: "https://ffmpeg.martin-riedl.de/download/linux/arm64/1778760876_8.1.1/ffmpeg.zip",
            sha256: "5499ff0fb22b051f21f1458ebfb461ab1994467f037b911f4188ddac6c189037",
            size: 0,
        },
        FfmpegArchive {
            url: "https://ffmpeg.martin-riedl.de/download/linux/arm64/1778760876_8.1.1/ffprobe.zip",
            sha256: "c795ddf7e49ffc3db9386c0c29f69cb1d87bf7537e11202c79d04244ba63ce60",
            size: 0,
        },
    ],
};

static MR_MACOS_X64: FfmpegSource = FfmpegSource {
    version: "8.1.1",
    origin: "ffmpeg.martin-riedl.de",
    archives: &[
        FfmpegArchive {
            url: "https://ffmpeg.martin-riedl.de/download/macos/amd64/1778768838_8.1.1/ffmpeg.zip",
            sha256: "8cb711bfa6f66033112d708dc275220419d0fdb49c5b752f8db25f11a92d321f",
            size: 0,
        },
        FfmpegArchive {
            url: "https://ffmpeg.martin-riedl.de/download/macos/amd64/1778768838_8.1.1/ffprobe.zip",
            sha256: "e9b9b83fef584c367b27c683a1172921b4f48fa8bd5df6712ef54e63b915ea50",
            size: 0,
        },
    ],
};

static MR_MACOS_ARM64: FfmpegSource = FfmpegSource {
    version: "8.1.1",
    origin: "ffmpeg.martin-riedl.de",
    archives: &[
        FfmpegArchive {
            url: "https://ffmpeg.martin-riedl.de/download/macos/arm64/1778761665_8.1.1/ffmpeg.zip",
            sha256: "a05b1a47bb3ac89a95a55eec713f8bbb347051bb07015f3b7d08fb62ed81a21e",
            size: 0,
        },
        FfmpegArchive {
            url: "https://ffmpeg.martin-riedl.de/download/macos/arm64/1778761665_8.1.1/ffprobe.zip",
            sha256: "135e70d2518beeb568183952dbc4bdeca1628dd49a7376d57e6b27dbc57d209f",
            size: 0,
        },
    ],
};

/// The pinned ffmpeg source for this host, or `None` for hosts we have
/// no maintained static build for (Win7 32-bit, FreeBSD, etc.), which
/// keep relying on an `ffmpeg`/`ffprobe` already on `PATH`.
pub fn host_ffmpeg_source() -> Option<&'static FfmpegSource> {
    if cfg!(all(
        target_os = "windows",
        target_arch = "x86_64",
        target_vendor = "win7"
    )) {
        Some(&DEADSYNC_WIN7_X64)
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some(&GYAN_WIN_X64)
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(&MR_LINUX_X64)
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some(&MR_LINUX_ARM64)
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some(&MR_MACOS_X64)
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(&MR_MACOS_ARM64)
    } else {
        None
    }
}

/// True when the in-app downloader can install ffmpeg on this host.
/// The Options entry is hidden when this is false.
pub fn install_supported_for_host() -> bool {
    host_ffmpeg_source().is_some()
}

/* ---------- phases ---------- */

/// Public phases of the ffmpeg install flow.  Cloned cheaply to hand to
/// the UI thread every frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FfmpegPhase {
    /// Nothing in flight; the overlay should not be visible.
    Idle,
    /// Probing whether `ffmpeg`/`ffprobe` already resolve. Shown briefly
    /// so the UI thread never blocks on the subprocess-spawning check.
    Checking,
    /// Asking the user to confirm the download. `already_available` is set
    /// when the tools already resolve, so the overlay notes the download
    /// is optional.
    Confirm {
        version: String,
        origin: String,
        total: Option<u64>,
        already_available: bool,
    },
    /// Archives are downloading. `written`/`total` are cumulative across
    /// all of the source's archives.
    Downloading {
        version: String,
        written: u64,
        total: Option<u64>,
        eta_secs: Option<u64>,
        /// Estimated transfer rate (bytes/sec), or `None` until enough
        /// samples accumulate.
        speed_bps: Option<u64>,
    },
    /// Download finished; binaries are being extracted and installed.
    Extracting { version: String },
    /// Tools installed. Playback works immediately, so no restart needed.
    Installed { version: String },
    /// No maintained build for this host; the user must install
    /// `ffmpeg`/`ffprobe` manually.
    Unsupported,
    /// `ffmpeg`/`ffprobe` already resolve and there's nothing to offer, so
    /// the overlay just confirms the user is good to go.
    AlreadyAvailable,
    /// A failure surfaced by the worker.
    Error {
        kind: ActionErrorKind,
        detail: String,
    },
}

static PHASE: LazyLock<RwLock<FfmpegPhase>> = LazyLock::new(|| RwLock::new(FfmpegPhase::Idle));
static WORKER_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static OP_GENERATION: AtomicU64 = AtomicU64::new(0);

fn begin_operation() -> u64 {
    OP_GENERATION.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
}

fn worker_should_stop(generation: u64) -> bool {
    OP_GENERATION.load(Ordering::SeqCst) != generation
}

/// Snapshot of the current phase.  Cheap; clones strings.
pub fn current() -> FfmpegPhase {
    PHASE
        .read()
        .map(|guard| guard.clone())
        .unwrap_or(FfmpegPhase::Idle)
}

fn set_phase(next: FfmpegPhase) {
    if let Ok(mut guard) = PHASE.write() {
        *guard = next;
    }
}

fn set_phase_if_current(generation: u64, next: FfmpegPhase) -> bool {
    if let Ok(mut guard) = PHASE.write()
        && OP_GENERATION.load(Ordering::SeqCst) == generation
    {
        *guard = next;
        return true;
    }
    false
}

/// Reset the overlay to [`FfmpegPhase::Idle`].  Safe from any state.
pub fn dismiss() {
    set_phase(FfmpegPhase::Idle);
}

/// Cancel an in-flight download and return to `Idle`. The worker's
/// result is discarded at its next polling point. No-op outside
/// `Downloading`.
pub fn request_cancel() {
    if matches!(current(), FfmpegPhase::Downloading { .. }) {
        let _ = begin_operation();
        set_phase(FfmpegPhase::Idle);
    }
}

/// Open the install flow from the menu.  Moves to [`FfmpegPhase::Confirm`]
/// when this host has a source, or [`FfmpegPhase::Unsupported`] otherwise.
pub fn request_install() {
    let _guard = match WORKER_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    match host_ffmpeg_source() {
        Some(source) => set_phase(FfmpegPhase::Confirm {
            version: source.version.to_string(),
            origin: source.origin.to_string(),
            total: source.total_size(),
            already_available: false,
        }),
        None => set_phase(FfmpegPhase::Unsupported),
    }
}

/// Open the overlay in a transient [`FfmpegPhase::Checking`] state and
/// return the operation generation for [`resolve_availability_check`].
/// `None` if another ffmpeg operation is already in flight.
///
/// The caller runs the probe and reports back: it spawns subprocesses
/// (which would stutter the UI thread), and this crate can't depend on
/// `deadlib-video`.
pub fn begin_availability_check() -> Option<u64> {
    let _guard = WORKER_LOCK.try_lock().ok()?;
    let generation = begin_operation();
    set_phase(FfmpegPhase::Checking);
    Some(generation)
}

/// Finish the probe started by [`begin_availability_check`]. With a host
/// source, shows [`FfmpegPhase::Confirm`] (flagging `already_available`
/// so the download reads as optional); otherwise [`AlreadyAvailable`] or
/// [`Unsupported`]. No-op if the overlay has since moved on.
pub fn resolve_availability_check(generation: u64, available: bool) {
    let next = match host_ffmpeg_source() {
        Some(source) => FfmpegPhase::Confirm {
            version: source.version.to_string(),
            origin: source.origin.to_string(),
            total: source.total_size(),
            already_available: available,
        },
        None if available => FfmpegPhase::AlreadyAvailable,
        None => FfmpegPhase::Unsupported,
    };
    set_phase_if_current(generation, next);
}

/// Cancel an in-flight availability check and return to `Idle`, bumping
/// the generation so a late [`resolve_availability_check`] is discarded.
pub fn cancel_check() {
    if matches!(current(), FfmpegPhase::Checking) {
        let _ = begin_operation();
        set_phase(FfmpegPhase::Idle);
    }
}

/// Confirm the download from the [`FfmpegPhase::Confirm`] overlay and
/// spawn the worker.  No-op in any other phase.
pub fn request_confirm() {
    let _guard = match WORKER_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if !matches!(current(), FfmpegPhase::Confirm { .. }) {
        return;
    }
    let Some(source) = host_ffmpeg_source() else {
        set_phase(FfmpegPhase::Unsupported);
        return;
    };
    let generation = begin_operation();
    set_phase(FfmpegPhase::Downloading {
        version: source.version.to_string(),
        written: 0,
        total: source.total_size(),
        eta_secs: None,
        speed_bps: None,
    });
    if let Err(err) = thread::Builder::new()
        .name("deadsync-ffmpeg-install".to_owned())
        .spawn(move || run_install(source, generation))
    {
        set_phase_if_current(
            generation,
            error_phase(&UpdaterError::Io(format!(
                "spawn ffmpeg-install worker: {err}"
            ))),
        );
    }
}

fn error_phase(err: &UpdaterError) -> FfmpegPhase {
    FfmpegPhase::Error {
        kind: ActionErrorKind::classify(err),
        detail: err.to_string(),
    }
}

/* ---------- worker ---------- */

fn run_install(source: &'static FfmpegSource, generation: u64) {
    let staging_dir = match downloads_dir() {
        Some(dir) => dir,
        None => {
            set_phase_if_current(
                generation,
                error_phase(&UpdaterError::Io(
                    "cannot determine ffmpeg cache directory".to_string(),
                )),
            );
            return;
        }
    };
    let install_dir = match install_dir() {
        Some(dir) => dir,
        None => {
            set_phase_if_current(
                generation,
                error_phase(&UpdaterError::Io(
                    "cannot determine runtime bin directory".to_string(),
                )),
            );
            return;
        }
    };

    let total = source.total_size();
    let mut written_base = 0u64;
    let mut downloaded: Vec<PathBuf> = Vec::with_capacity(source.archives.len());

    for archive in source.archives {
        let expected = match parse_hex32(archive.sha256) {
            Some(d) => d,
            None => {
                set_phase_if_current(
                    generation,
                    error_phase(&UpdaterError::Io(format!(
                        "pinned ffmpeg checksum for '{}' is malformed",
                        archive.url
                    ))),
                );
                return;
            }
        };
        if worker_should_stop(generation) {
            log::info!("ffmpeg install cancelled before archive fetch");
            return;
        }
        let file_name = archive_file_name(archive.url);
        let dest = staging_dir.join(&file_name);
        let asset = ReleaseAsset {
            name: file_name.clone(),
            browser_download_url: archive.url.to_string(),
            size: archive.size,
            digest: None,
        };
        let version = source.version.to_string();
        let mut throttle = ProgressThrottle::default();
        let mut first_sample: Option<(Instant, u64)> = None;
        let progress = |archive_written: u64, _archive_total: Option<u64>| {
            let written = written_base.saturating_add(archive_written);
            let now = Instant::now();
            let (start_t, start_w) = *first_sample.get_or_insert((now, written));
            let elapsed = now.duration_since(start_t).as_secs_f64();
            let bytes = written.saturating_sub(start_w) as f64;
            let speed = (elapsed >= 0.5 && bytes > 0.0)
                .then(|| bytes / elapsed)
                .filter(|s| *s > 0.0);
            let eta_secs = match (total, speed) {
                (Some(t), Some(speed)) if t > written => {
                    Some(((t - written) as f64 / speed).ceil() as u64)
                }
                _ => None,
            };
            let speed_bps = speed.map(|s| s as u64);
            if !throttle.should_publish(now, written, total, eta_secs) {
                return;
            }
            set_phase_if_current(
                generation,
                FfmpegPhase::Downloading {
                    version: version.clone(),
                    written,
                    total,
                    eta_secs,
                    speed_bps,
                },
            );
        };

        match download_to_file(
            &download_agent(),
            &asset,
            &expected,
            &dest,
            progress,
            || worker_should_stop(generation),
        ) {
            Ok(()) => {
                written_base = written_base.saturating_add(archive.size);
                downloaded.push(dest);
            }
            Err(UpdaterError::Cancelled) => {
                log::info!("ffmpeg download cancelled by user");
                return;
            }
            Err(err) => {
                log::warn!("ffmpeg download failed: {err}");
                set_phase_if_current(generation, error_phase(&err));
                return;
            }
        }
    }

    if worker_should_stop(generation) {
        return;
    }
    set_phase_if_current(
        generation,
        FfmpegPhase::Extracting {
            version: source.version.to_string(),
        },
    );

    match install_tools(&downloaded, &install_dir) {
        Ok(()) => {
            // Drop the staged archives now that the binaries are installed.
            for archive in &downloaded {
                let _ = fs::remove_file(archive);
            }
            log::info!(
                "ffmpeg {} installed into {}",
                source.version,
                install_dir.display()
            );
            set_phase_if_current(
                generation,
                FfmpegPhase::Installed {
                    version: source.version.to_string(),
                },
            );
        }
        Err(err) => {
            log::warn!("ffmpeg install failed: {err}");
            set_phase_if_current(generation, error_phase(&err));
        }
    }
}

/// Extract `ffmpeg`/`ffprobe` from each downloaded archive into
/// `install_dir`, then require that both tools ended up installed.
fn install_tools(archives: &[PathBuf], install_dir: &Path) -> Result<(), UpdaterError> {
    fs::create_dir_all(install_dir).map_err(|err| io_err_at("create_dir_all", install_dir, err))?;

    let mut installed: Vec<&'static str> = Vec::new();
    for archive in archives {
        for tool in extract_tools_from_zip(archive, install_dir)? {
            if !installed.contains(&tool) {
                installed.push(tool);
            }
        }
    }

    for required in TOOLS {
        if !installed.contains(required) {
            return Err(UpdaterError::Io(format!(
                "downloaded archives did not contain '{required}'"
            )));
        }
    }
    Ok(())
}

/// The tools we extract and install.
const TOOLS: &[&str] = &["ffmpeg", "ffprobe"];

/// Open `archive` and extract any entry whose basename matches one of
/// [`TOOLS`] into `install_dir` (atomically; +x on unix).  Returns the
/// canonical tool names that were installed from this archive.
fn extract_tools_from_zip(
    archive: &Path,
    install_dir: &Path,
) -> Result<Vec<&'static str>, UpdaterError> {
    let file = fs::File::open(archive).map_err(|err| io_err_at("open", archive, err))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|err| io_err_op(&format!("open zip '{}'", archive.display()), err))?;

    let mut installed = Vec::new();
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| io_err_op("read zip entry", err))?;
        if !entry.is_file() {
            continue;
        }
        let Some(tool) = match_tool(entry.name()) else {
            continue;
        };
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|err| io_err_op(&format!("extract '{}'", entry.name()), err))?;
        let dest = install_dir.join(installed_tool_name(tool));
        write_executable(&dest, &bytes)?;
        if !installed.contains(&tool) {
            installed.push(tool);
        }
    }
    Ok(installed)
}

/// Match a zip-entry path against [`TOOLS`] by basename, tolerating a
/// trailing `.exe` and any directory prefix.
fn match_tool(entry_path: &str) -> Option<&'static str> {
    let base = entry_path.rsplit(['/', '\\']).next().unwrap_or(entry_path);
    let stem = if base.len() >= 4 && base[base.len() - 4..].eq_ignore_ascii_case(".exe") {
        &base[..base.len() - 4]
    } else {
        base
    };
    TOOLS.iter().copied().find(|t| stem.eq_ignore_ascii_case(t))
}

/// Canonical on-disk filename for a tool on this host.
fn installed_tool_name(tool: &str) -> String {
    if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_string()
    }
}

/// Write `bytes` to `dest` atomically, executable on unix. Replaces any
/// pre-existing tool in place.
fn write_executable(dest: &Path, bytes: &[u8]) -> Result<(), UpdaterError> {
    let mut staging = dest.as_os_str().to_owned();
    staging.push(".part");
    let staging = PathBuf::from(staging);
    let _ = fs::remove_file(&staging);

    fs::write(&staging, bytes).map_err(|err| io_err_at("write", &staging, err))?;
    set_executable(&staging)?;

    fs::rename(&staging, dest).map_err(|err| {
        let _ = fs::remove_file(&staging);
        UpdaterError::Io(format!(
            "rename '{}' -> '{}': {err}",
            staging.display(),
            dest.display()
        ))
    })
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), UpdaterError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|err| io_err_at("set_permissions", path, err))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), UpdaterError> {
    Ok(())
}

/// Derive a staging file name from an archive URL's last path segment.
fn archive_file_name(url: &str) -> String {
    url.rsplit('/')
        .find(|seg| !seg.is_empty())
        .unwrap_or("ffmpeg-archive.zip")
        .to_string()
}

/// Absolute path of the directory archives are staged into.
pub fn downloads_dir() -> Option<PathBuf> {
    Some(
        deadlib_platform::dirs::app_dirs()
            .cache_dir
            .join(DOWNLOADS_SUBDIR),
    )
}

/// Runtime `bin/` directory the tools are installed into
/// (`<current_dir>/bin`), matching `deadlib-video`'s resolution path.
pub fn install_dir() -> Option<PathBuf> {
    std::env::current_dir().ok().map(|dir| dir.join("bin"))
}

/* ---------- progress throttle (local copy of action.rs's) ---------- */

#[derive(Default)]
struct ProgressThrottle {
    last_published: Option<Instant>,
    last_pct: Option<u32>,
    last_eta: Option<u64>,
}

impl ProgressThrottle {
    fn should_publish(
        &mut self,
        now: Instant,
        written: u64,
        total: Option<u64>,
        eta_secs: Option<u64>,
    ) -> bool {
        let pct = total
            .and_then(|t| (t != 0).then(|| ((written.min(t) as u128 * 100) / t as u128) as u32));
        let is_final = matches!(total, Some(t) if written >= t);
        let publish = is_final
            || self.last_published.is_none()
            || pct != self.last_pct
            || eta_secs != self.last_eta
            || self
                .last_published
                .map(|t| now.duration_since(t) >= Duration::from_millis(100))
                .unwrap_or(true);
        if publish {
            self.last_published = Some(now);
            self.last_pct = pct;
            self.last_eta = eta_secs;
        }
        publish
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_source_has_archives_with_valid_checksums() {
        if let Some(source) = host_ffmpeg_source() {
            assert!(!source.archives.is_empty());
            for archive in source.archives {
                assert!(
                    parse_hex32(archive.sha256).is_some(),
                    "pinned checksum for {} must be 64 hex chars",
                    archive.url
                );
                assert!(archive.url.starts_with("https://"));
            }
        }
    }

    #[test]
    fn all_pinned_sources_have_valid_checksums() {
        for source in [
            &GYAN_WIN_X64,
            &DEADSYNC_WIN7_X64,
            &MR_LINUX_X64,
            &MR_LINUX_ARM64,
            &MR_MACOS_X64,
            &MR_MACOS_ARM64,
        ] {
            for archive in source.archives {
                assert!(
                    parse_hex32(archive.sha256).is_some(),
                    "checksum for {} should decode",
                    archive.url
                );
            }
        }
    }

    #[test]
    fn match_tool_handles_prefixes_and_exe() {
        assert_eq!(
            match_tool("ffmpeg-7.0-essentials_build/bin/ffmpeg.exe"),
            Some("ffmpeg")
        );
        assert_eq!(
            match_tool("ffmpeg-7.0-essentials_build/bin/ffprobe.exe"),
            Some("ffprobe")
        );
        assert_eq!(match_tool("ffmpeg"), Some("ffmpeg"));
        assert_eq!(match_tool("ffprobe"), Some("ffprobe"));
        assert_eq!(match_tool("bin/FFMPEG.EXE"), Some("ffmpeg"));
        assert_eq!(match_tool("bin/ffplay.exe"), None);
        assert_eq!(match_tool("doc/ffmpeg.html"), None);
    }

    #[test]
    fn archive_file_name_takes_last_segment() {
        assert_eq!(
            archive_file_name("https://host/a/b/ffmpeg.zip"),
            "ffmpeg.zip"
        );
        assert_eq!(
            archive_file_name("https://host/x/ffmpeg-7.0-essentials_build.zip"),
            "ffmpeg-7.0-essentials_build.zip"
        );
    }

    #[test]
    fn total_size_unknown_when_any_archive_unknown() {
        assert_eq!(GYAN_WIN_X64.total_size(), Some(109_282_242));
        assert_eq!(DEADSYNC_WIN7_X64.total_size(), Some(197_446_383));
        assert_eq!(MR_LINUX_X64.total_size(), None);
    }

    #[test]
    fn install_tools_extracts_and_requires_both() {
        use std::io::Write;

        let tmp = std::env::temp_dir().join(format!(
            "deadsync-ffmpeg-test-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();

        let archive_path = tmp.join("tools.zip");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            zip.start_file("build/bin/ffmpeg.exe", opts).unwrap();
            zip.write_all(b"fake-ffmpeg").unwrap();
            zip.start_file("build/bin/ffprobe.exe", opts).unwrap();
            zip.write_all(b"fake-ffprobe").unwrap();
            zip.finish().unwrap();
        }

        let install = tmp.join("bin");
        install_tools(&[archive_path], &install).expect("both tools install");
        assert!(install.join(installed_tool_name("ffmpeg")).is_file());
        assert!(install.join(installed_tool_name("ffprobe")).is_file());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn install_tools_errors_when_tool_missing() {
        use std::io::Write;

        let tmp = std::env::temp_dir().join(format!(
            "deadsync-ffmpeg-test-miss-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&tmp).unwrap();

        let archive_path = tmp.join("partial.zip");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            zip.start_file("ffmpeg", opts).unwrap();
            zip.write_all(b"only-ffmpeg").unwrap();
            zip.finish().unwrap();
        }

        let install = tmp.join("bin");
        let err = install_tools(&[archive_path], &install).unwrap_err();
        assert!(matches!(err, UpdaterError::Io(_)));

        let _ = fs::remove_dir_all(&tmp);
    }
}
