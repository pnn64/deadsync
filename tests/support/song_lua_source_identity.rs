//! Check replay inputs against the source archive attached to a capture.
//! This belongs to the harness: ordinary gameplay accepts edited songs.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{BufReader, Read},
    path::{Component, Path, PathBuf},
};

fn digest(path: &Path) -> Result<(u64, String), String> {
    let mut file =
        BufReader::new(File::open(path).map_err(|e| format!("{}: {e}", path.display()))?);
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        hash.update(&buffer[..count]);
    }
    let hex = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((bytes, hex))
}

fn check_digest(path: &Path, bytes: Option<u64>, expected: &str) -> Result<String, String> {
    let (actual_bytes, actual) = digest(path)?;
    if actual != expected || bytes.is_some_and(|bytes| actual_bytes != bytes) {
        return Err(format!(
            "replay source identity mismatch: {} (expected SHA-256 {expected}, got {actual}). \
             Use the capture's original sources or capture the edited song again; \
             this is an input mismatch, not an engine parity result.",
            path.display(),
        ));
    }
    Ok(actual)
}

fn song_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if !path
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(format!("invalid source archive member: {}", path.display()));
    }
    path.strip_prefix("song")
        .map(Path::to_path_buf)
        .map_err(|_| {
            format!(
                "source is outside archive song directory: {}",
                path.display()
            )
        })
}

/// Older standalone traces have no source manifest. Report that explicitly.
/// When a capture manifest does record an archive, enforce every recorded song
/// file, including the simfile, Lua dependencies and media, before preparation.
pub fn verify(trace: &Path, simfile: &Path) -> Result<Value, String> {
    let parent = trace.parent().unwrap_or_else(|| Path::new("."));
    let manifest_path = parent.join("manifest.json");
    let unverified = || -> Result<Value, String> {
        Ok(json!({
            "status": "not-recorded",
            "simfile": simfile,
            "simfile_sha256": digest(simfile)?.1,
        }))
    };
    if !manifest_path.exists() {
        return unverified();
    }
    let manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let Some(captures) = manifest.get("captures") else {
        return unverified();
    };
    let captures = captures
        .as_array()
        .ok_or("capture manifest captures must be an array")?;
    let name = trace.file_name().ok_or("capture has no filename")?;
    let capture = captures
        .iter()
        .find(|capture| {
            capture["trace"]
                .as_str()
                .is_some_and(|path| Path::new(path) == Path::new(name))
        })
        .ok_or("trace is not listed in its capture manifest")?;
    let trace_sha256 = check_digest(
        trace,
        None,
        capture["compressed_sha256"]
            .as_str()
            .ok_or("capture hash is missing")?,
    )?;
    let archive = parent.join(
        manifest["source_archive"]
            .as_str()
            .ok_or("source archive is missing")?,
    );
    let archive_hash = archive
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".tar.zst"))
        .filter(|hash| hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or("source archive must have a SHA-256 content-addressed filename")?;
    check_digest(&archive, None, archive_hash)?;
    let reader = zstd::stream::read::Decoder::new(File::open(&archive).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(reader);
    let mut source_manifest = None;
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        if entry.path().map_err(|e| e.to_string())?.as_ref() == Path::new("manifest.json") {
            source_manifest =
                Some(serde_json::from_reader::<_, Value>(&mut entry).map_err(|e| e.to_string())?);
            break;
        }
    }
    let source_manifest = source_manifest.ok_or("source archive has no manifest")?;
    let recorded_simfile = source_manifest["chart"]["simfile"]
        .as_str()
        .ok_or("archive simfile is missing")?;
    let simfile_relative = song_path(recorded_simfile)?;
    // Support nested simfiles and relocated source directories.
    let mut song_root = simfile;
    for _ in simfile_relative.components() {
        song_root = song_root.parent().ok_or("simfile has no song root")?;
    }
    let files = source_manifest["files"]
        .as_array()
        .ok_or("archive source files are missing")?;
    let mut checked = BTreeSet::new();
    let mut simfile_sha256 = None;
    for file in files {
        let name = file["path"]
            .as_str()
            .ok_or("archive member path is missing")?;
        if !name.starts_with("song/") {
            continue;
        }
        let relative = song_path(name)?;
        if !checked.insert(relative.clone()) {
            return Err(format!("duplicate source member: {name}"));
        }
        let path = if name == recorded_simfile {
            simfile.to_path_buf()
        } else {
            song_root.join(relative)
        };
        let hash = check_digest(
            &path,
            Some(
                file["bytes"]
                    .as_u64()
                    .ok_or("source byte count is missing")?,
            ),
            file["sha256"].as_str().ok_or("source hash is missing")?,
        )?;
        if name == recorded_simfile {
            simfile_sha256 = Some(hash);
        }
    }
    let simfile_sha256 = simfile_sha256.ok_or("archive does not pin its simfile")?;
    Ok(json!({
        "status": "verified",
        "manifest": manifest_path,
        "trace_sha256": trace_sha256,
        "source_archive": archive,
        "source_archive_sha256": archive_hash,
        "simfile": simfile,
        "simfile_sha256": simfile_sha256,
        "verified_song_files": checked.len(),
    }))
}
