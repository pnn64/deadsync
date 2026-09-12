//! Download and transactionally install HURG-IIDX's Cel and Metal Workshop.

use crate::{ReleaseAsset, UpdaterError, download};
use deadsync_noteskin::{
    pack::InstalledPack,
    workshop::{self, PACK_ID},
};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{
    LazyLock, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

// Verified GitHub source archive for workshop::COMMIT; never follow a moving branch.
const URL: &str = "https://codeload.github.com/HURG-IIDX/Noteskin-Workshop-Cel-and-Metal/zip/5ba831ae039e7319b7ae5f223b3532c05e8b4072";
const SHA256: &str = "614e6946cbd6dd9af993dabbd08cacf7be32ed4bba028249e314b2bb3d3327ea";
const ARCHIVE_BYTES: u64 = 445_744_044;
// Bounds accommodate this revision's 1,939 files and 454 MiB unpacked source.
const MAX_ENTRIES: usize = 4_000;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;

/// Worker progress presented by the Downloads menu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Downloading {
        written: u64,
        total: u64,
    },
    Preparing {
        done: usize,
        total: usize,
    },
    Publishing,
    Cancelling,
    Installed,
    Error {
        detail: String,
    },
}

// One menu-started worker owns disk/network work. The mutex retains one phase;
// an atomic revision lets stable frames skip locking and cloning. Updates publish
// at most 10 Hz, plus phase transitions. No work runs on the gameplay/audio thread.
static PHASE: LazyLock<Mutex<Phase>> = LazyLock::new(|| Mutex::new(Phase::Idle));
static REVISION: AtomicU64 = AtomicU64::new(0);
static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn current() -> Phase {
    PHASE.lock().expect("workshop phase lock poisoned").clone()
}
pub fn phase_revision() -> u64 {
    REVISION.load(Ordering::Acquire)
}

fn publish(phase: Phase) {
    let mut state = PHASE.lock().expect("workshop phase lock poisoned");
    if matches!(*state, Phase::Cancelling)
        && matches!(phase, Phase::Downloading { .. } | Phase::Preparing { .. })
    {
        return;
    }
    *state = phase;
    REVISION.fetch_add(1, Ordering::Release);
}

/// Start an install into the game's `assets/noteskins/hurg` directory.
///
/// `activate` runs on the worker after publication, to load the menu previews.
/// Repeated requests while busy are ignored. Failures remain visible for retry.
pub fn request_install(
    target: PathBuf,
    activate: impl FnOnce(&Path) -> Result<(), String> + Send + 'static,
) {
    {
        let mut state = PHASE.lock().expect("workshop phase lock poisoned");
        if !matches!(*state, Phase::Idle | Phase::Installed | Phase::Error { .. }) {
            return;
        }
        CANCEL.store(false, Ordering::Release);
        *state = Phase::Downloading {
            written: 0,
            total: ARCHIVE_BYTES,
        };
        REVISION.fetch_add(1, Ordering::Release);
    }
    let spawn = std::thread::Builder::new()
        .name("deadsync-workshop".into())
        .spawn(move || {
            let result =
                install(&target).and_then(|()| activate(&target).map_err(UpdaterError::Io));
            publish(match result {
                Ok(()) => Phase::Installed,
                Err(UpdaterError::Cancelled) => Phase::Idle,
                Err(error) => {
                    log::warn!("Workshop install failed: {error}");
                    Phase::Error {
                        detail: error.to_string(),
                    }
                }
            });
        });
    if let Err(error) = spawn {
        publish(Phase::Error {
            detail: error.to_string(),
        });
    }
}

/// Cancel before publication, or dismiss a completed operation.
pub fn dismiss() {
    let mut state = PHASE.lock().expect("workshop phase lock poisoned");
    match *state {
        Phase::Downloading { .. } | Phase::Preparing { .. } => {
            CANCEL.store(true, Ordering::Release);
            *state = Phase::Cancelling;
        }
        Phase::Publishing | Phase::Cancelling => return,
        _ => *state = Phase::Idle,
    }
    REVISION.fetch_add(1, Ordering::Release);
}

fn check_cancel() -> Result<(), UpdaterError> {
    if CANCEL.load(Ordering::Acquire) {
        Err(UpdaterError::Cancelled)
    } else {
        Ok(())
    }
}

fn install(target: &Path) -> Result<(), UpdaterError> {
    let root = target
        .parent()
        .ok_or_else(|| UpdaterError::Io("workshop destination has no parent".into()))?;
    fs::create_dir_all(root).map_err(io_error)?;
    if target.exists() {
        validate_target(&target)?;
        if InstalledPack::load(&target)
            .is_ok_and(|p| p.manifest.version == format!("{}-rust1", workshop::COMMIT))
        {
            return begin_publish();
        }
    }
    let stage = create_stage(root)?;
    let archive = stage.0.join("source.zip");
    let asset = ReleaseAsset {
        name: "workshop.zip".into(),
        browser_download_url: URL.into(),
        size: ARCHIVE_BYTES,
        digest: None,
    };
    let expected = download::parse_hex32(SHA256).expect("pinned workshop SHA-256 is valid");
    let mut last = Instant::now();
    let oversized = AtomicBool::new(false);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(crate::user_agent())
        .timeout_connect(Some(Duration::from_secs(15)))
        // This is a deadline for the entire 426 MiB body, not an idle-read timeout.
        // One hour accommodates roughly 1 Mbps connections; cancellation is polled per chunk.
        .timeout_recv_body(Some(Duration::from_secs(3600)))
        .build()
        .into();
    download::download_to_file(
        &agent,
        &asset,
        &expected,
        &archive,
        |written, _| {
            oversized.store(written > ARCHIVE_BYTES, Ordering::Relaxed);
            if last.elapsed() >= Duration::from_millis(100) || written == ARCHIVE_BYTES {
                publish(Phase::Downloading {
                    written,
                    total: ARCHIVE_BYTES,
                });
                last = Instant::now();
            }
        },
        || CANCEL.load(Ordering::Acquire) || oversized.load(Ordering::Relaxed),
    )?;
    let pack = stage.0.join("pack");
    fs::create_dir(&pack).map_err(io_error)?;
    publish(Phase::Preparing { done: 0, total: 0 });
    let prepare_started = Instant::now();
    extract_with_progress(&archive, &pack, |done, total| {
        if last.elapsed() >= Duration::from_millis(100) || done == total {
            publish(Phase::Preparing {
                done: done * 500 / total.max(1),
                total: 1000,
            });
            last = Instant::now();
        }
        !CANCEL.load(Ordering::Acquire)
    })?;
    log::info!(
        "Workshop extraction completed in {:.3}s",
        prepare_started.elapsed().as_secs_f64()
    );
    let previews_started = Instant::now();
    workshop::compile(&pack, |done, total| {
        if last.elapsed() >= Duration::from_millis(100) || done == total {
            publish(Phase::Preparing {
                done: 500 + done * 500 / total.max(1),
                total: 1000,
            });
            last = Instant::now();
        }
        !CANCEL.load(Ordering::Acquire)
    })
    .map_err(|error| match error {
        deadsync_noteskin::pack::Error::Io(e) if e.kind() == io::ErrorKind::Interrupted => {
            UpdaterError::Cancelled
        }
        e => UpdaterError::Io(e.to_string()),
    })?;
    log::info!(
        "Workshop preview compilation completed in {:.3}s",
        previews_started.elapsed().as_secs_f64()
    );
    begin_publish()?;
    commit_pack(&pack, &target)?;
    Ok(())
}

/// Relocate an existing Workshop before asset discovery without replacing a destination.
///
/// # Errors
/// Returns an error if the legacy pack is invalid or cannot be moved. Its files
/// remain at the original location if relocation fails.
pub fn migrate(target: &Path, legacy_roots: &[PathBuf]) -> Result<(), UpdaterError> {
    if target.exists() {
        return Ok(());
    }
    for root in legacy_roots {
        let source = root.join(PACK_ID);
        if !source.exists() {
            continue;
        }
        validate_target(&source)?;
        let parent = target
            .parent()
            .ok_or_else(|| UpdaterError::Io("workshop destination has no parent".into()))?;
        fs::create_dir_all(parent).map_err(io_error)?;
        match fs::rename(&source, target) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
                let stage = create_stage(parent)?;
                let copied = stage.0.join("pack");
                copy_legacy_pack(&source, &copied)?;
                validate_target(&copied)?;
                commit_pack(&copied, target)?;
                if let Err(error) = fs::remove_dir_all(&source) {
                    log::warn!("Workshop moved; old copy could not be removed: {error}");
                }
            }
            Err(error) => return Err(io_error(error)),
        }
        log::info!("Moved HURG-IIDX's Workshop to {}", target.display());
        return Ok(());
    }
    Ok(())
}

fn copy_legacy_pack(source: &Path, target: &Path) -> Result<(), UpdaterError> {
    let mut pending = vec![(source.to_path_buf(), target.to_path_buf())];
    let mut entries = 0;
    let mut bytes = 0u64;
    while let Some((source, target)) = pending.pop() {
        fs::create_dir(&target).map_err(io_error)?;
        for entry in fs::read_dir(source).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let kind = entry.file_type().map_err(io_error)?;
            entries += 1;
            if entries > MAX_ENTRIES || kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
                return Err(UpdaterError::Io(
                    "legacy Workshop contains unsupported entries".into(),
                ));
            }
            let destination = target.join(entry.file_name());
            if kind.is_dir() {
                pending.push((entry.path(), destination));
            } else {
                bytes = bytes.saturating_add(entry.metadata().map_err(io_error)?.len());
                if bytes > MAX_EXPANDED_BYTES {
                    return Err(UpdaterError::Io(
                        "legacy Workshop exceeds the size limit".into(),
                    ));
                }
                fs::copy(entry.path(), destination).map_err(io_error)?;
            }
        }
    }
    Ok(())
}

fn begin_publish() -> Result<(), UpdaterError> {
    let mut state = PHASE.lock().expect("workshop phase lock poisoned");
    check_cancel()?;
    *state = Phase::Publishing;
    REVISION.fetch_add(1, Ordering::Release);
    Ok(())
}

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            log::warn!("Workshop staging cleanup failed: {error}");
        }
    }
}

fn create_stage(root: &Path) -> Result<Stage, UpdaterError> {
    let path = root.join(format!(".workshop-{:016x}", rand::random::<u64>()));
    fs::create_dir(&path).map_err(io_error)?;
    Ok(Stage(path))
}

fn validate_target(target: &Path) -> Result<(), UpdaterError> {
    if fs::symlink_metadata(target)
        .map_err(io_error)?
        .file_type()
        .is_symlink()
    {
        return Err(UpdaterError::Io("workshop destination is a symlink".into()));
    }
    let pack = InstalledPack::load(target).map_err(|e| UpdaterError::Io(e.to_string()))?;
    if pack.manifest.id != PACK_ID {
        return Err(UpdaterError::Io(
            "workshop destination belongs to another pack".into(),
        ));
    }
    Ok(())
}

fn commit_pack(pack: &Path, target: &Path) -> Result<(), UpdaterError> {
    // Keep the backup outside the staging guard: a failed rollback must retain it.
    let backup = target.with_file_name(format!(".workshop-backup-{:016x}", rand::random::<u64>()));
    let replacing = target.exists();
    if replacing {
        validate_target(target)?;
        fs::rename(target, &backup).map_err(io_error)?;
    }
    if let Err(error) = fs::rename(pack, target) {
        if replacing && let Err(restore) = fs::rename(&backup, target) {
            return Err(UpdaterError::Io(format!(
                "install failed: {error}; restore failed: {restore}; previous pack retained at {}",
                backup.display()
            )));
        }
        return Err(io_error(error));
    }
    if replacing && let Err(error) = fs::remove_dir_all(&backup) {
        log::warn!("Workshop backup cleanup failed: {error}");
    }
    Ok(())
}

fn archive_path(name: &str) -> Result<Option<PathBuf>, UpdaterError> {
    let prefix = format!("Noteskin-Workshop-Cel-and-Metal-{}/", workshop::COMMIT);
    let relative = name
        .strip_prefix(&prefix)
        .ok_or_else(|| UpdaterError::Io("unexpected workshop archive root".into()))?;
    if relative.is_empty() {
        return Ok(None);
    }
    let trimmed = relative.trim_end_matches('/');
    if trimmed.contains(['\\', ':'])
        || trimmed
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == "..")
        || Path::new(trimmed)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(UpdaterError::Io("invalid workshop archive path".into()));
    }
    let first = trimmed.split('/').next().unwrap_or_default();
    if matches!(
        first,
        "Cel - Workshop" | "Metal - Workshop" | "Customizations" | "LICENSE" | "README.md"
    ) {
        Ok(Some(PathBuf::from(trimmed)))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
fn extract(
    archive: &Path,
    target: &Path,
    cancelled: impl Fn() -> bool,
) -> Result<(), UpdaterError> {
    extract_with_progress(archive, target, |_, _| !cancelled())
}

fn extract_with_progress(
    archive: &Path,
    target: &Path,
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<(), UpdaterError> {
    let reader = BufReader::with_capacity(128 * 1024, File::open(archive).map_err(io_error)?);
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| UpdaterError::Io(e.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(UpdaterError::Io(
            "workshop archive has too many entries".into(),
        ));
    }
    let mut total = 0_u64;
    let mut names = HashSet::with_capacity(zip.len());
    let mut directories = HashSet::new();
    for index in 0..zip.len() {
        if !progress(index, zip.len()) {
            return Err(UpdaterError::Cancelled);
        }
        let mut entry = zip
            .by_index(index)
            .map_err(|e| UpdaterError::Io(e.to_string()))?;
        let Some(relative) = archive_path(entry.name())? else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| !matches!(mode & 0o170000, 0 | 0o100000))
            || !names.insert(relative.to_string_lossy().to_lowercase())
        {
            return Err(UpdaterError::Io(
                "duplicate or non-regular workshop file".into(),
            ));
        }
        total = total.saturating_add(entry.size());
        if entry.size() > MAX_FILE_BYTES || total > MAX_EXPANDED_BYTES {
            return Err(UpdaterError::Io(
                "workshop archive exceeds size limit".into(),
            ));
        }
        let path = target.join(relative);
        let parent = path.parent().expect("entry is beneath staging root");
        if !directories.contains(parent) {
            fs::create_dir_all(parent).map_err(io_error)?;
            directories.insert(parent.to_owned());
        }
        let file = File::options()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(io_error)?;
        let mut file = BufWriter::with_capacity(128 * 1024, file);
        let size = entry.size();
        let copied = io::copy(&mut (&mut entry).take(size + 1), &mut file).map_err(io_error)?;
        if copied != size {
            return Err(UpdaterError::Io("workshop entry size mismatch".into()));
        }
        file.flush().map_err(io_error)?;
    }
    if !progress(zip.len(), zip.len()) {
        return Err(UpdaterError::Cancelled);
    }
    Ok(())
}

fn io_error(error: io::Error) -> UpdaterError {
    UpdaterError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn archive(stage: &Stage, entries: &[(&str, &[u8])]) -> PathBuf {
        let path = stage.0.join("test.zip");
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, bytes) in entries {
            let name = format!(
                "Noteskin-Workshop-Cel-and-Metal-{}/{name}",
                workshop::COMMIT
            );
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer.start_file(name, opts).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    #[test]
    fn extraction_keeps_assets_and_omits_gallery() {
        let stage = create_stage(&std::env::temp_dir()).unwrap();
        let zip = archive(
            &stage,
            &[
                ("Cel - Workshop/NoteSkin.lua", b"return {}"),
                ("Preview Gallery/one.png", b"gallery"),
                ("LICENSE", b"license"),
            ],
        );
        let target = stage.0.join("pack");
        extract(&zip, &target, || false).unwrap();
        assert_eq!(
            fs::read(target.join("Cel - Workshop/NoteSkin.lua")).unwrap(),
            b"return {}"
        );
        assert!(target.join("LICENSE").is_file());
        assert!(!target.join("Preview Gallery").exists());
    }

    #[test]
    fn extraction_rejects_escaping_and_duplicate_paths() {
        for name in [
            "../outside",
            "Customizations/../../outside",
            "Customizations\\outside",
            "Customizations/file:stream",
        ] {
            let stage = create_stage(&std::env::temp_dir()).unwrap();
            let zip = archive(&stage, &[(name, b"bad")]);
            assert!(
                extract(&zip, &stage.0.join("pack"), || false).is_err(),
                "{name}"
            );
            assert!(!stage.0.join("outside").exists());
        }
        let stage = create_stage(&std::env::temp_dir()).unwrap();
        let zip = archive(
            &stage,
            &[
                ("Customizations/Test.png", b"a"),
                ("Customizations/test.png", b"b"),
            ],
        );
        assert!(extract(&zip, &stage.0.join("pack"), || false).is_err());
    }

    #[test]
    fn cancellation_and_invalid_installs_preserve_existing_files() {
        let stage = create_stage(&std::env::temp_dir()).unwrap();
        let zip = archive(&stage, &[("LICENSE", b"license")]);
        let target = stage.0.join("pack");
        assert!(matches!(
            extract(&zip, &target, || true),
            Err(UpdaterError::Cancelled)
        ));
        assert!(!target.exists());
        fs::create_dir(&target).unwrap();
        fs::write(target.join("custom.txt"), b"keep").unwrap();
        assert!(commit_pack(&stage.0.join("missing"), &target).is_err());
        assert_eq!(fs::read(target.join("custom.txt")).unwrap(), b"keep");
    }

    #[test]
    fn migration_preserves_existing_destinations_and_invalid_sources() {
        let stage = create_stage(&std::env::temp_dir()).unwrap();
        let old_root = stage.0.join("noteskin-packs");
        let old = old_root.join(PACK_ID);
        let target = stage.0.join("assets/noteskins/hurg");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("custom.txt"), b"keep").unwrap();
        assert!(migrate(&target, &[old_root.clone()]).is_err());
        assert!(!target.exists());
        assert_eq!(fs::read(old.join("custom.txt")).unwrap(), b"keep");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("custom.txt"), b"new").unwrap();
        migrate(&target, &[old_root]).unwrap();
        assert_eq!(fs::read(target.join("custom.txt")).unwrap(), b"new");
        assert_eq!(fs::read(old.join("custom.txt")).unwrap(), b"keep");
        // Cross-volume staging retains nested files and never consumes the source.
        fs::create_dir(old.join("nested")).unwrap();
        fs::write(old.join("nested/file"), b"nested").unwrap();
        let copy = stage.0.join("copy");
        copy_legacy_pack(&old, &copy).unwrap();
        assert_eq!(fs::read(copy.join("nested/file")).unwrap(), b"nested");
        assert!(old.join("nested/file").exists());
    }

    #[test]
    #[ignore = "downloads the pinned 426 MiB upstream archive over HTTPS"]
    fn live_download_installs_and_reinstalls_workshop() {
        let stage = create_stage(&std::env::temp_dir()).unwrap();
        let root = stage.0.clone();
        request_install(root.join("hurg"), |path| {
            InstalledPack::load(path)
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
        let start = Instant::now();
        loop {
            match current() {
                Phase::Installed => break,
                Phase::Error { detail } => panic!("{detail}"),
                _ => assert!(
                    start.elapsed() < Duration::from_secs(900),
                    "download timed out"
                ),
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let target = root.join("hurg");
        let pack = InstalledPack::load(&target).unwrap();
        assert_eq!(
            pack.manifest
                .skins
                .iter()
                .map(|s| s.options.len())
                .sum::<usize>(),
            1049
        );
        assert!(target.join("LICENSE").is_file());
        assert!(target.join("README.md").is_file());
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            1,
            "staging archive must be removed"
        );
        // An interrupted replacement restores the valid previous installation.
        assert!(commit_pack(&root.join("missing"), &target).is_err());
        assert!(InstalledPack::load(&target).is_ok());
        // Current revisions activate without touching the network or rewriting assets.
        let before = fs::metadata(target.join("pack.json"))
            .unwrap()
            .modified()
            .unwrap();
        install(&target).unwrap();
        assert_eq!(
            fs::metadata(target.join("pack.json"))
                .unwrap()
                .modified()
                .unwrap(),
            before
        );
        dismiss();
    }
}
