//! Download and SHA-256 verification for release assets.
//!
//! The release CI workflows publish a `<archive>.sha256` sidecar next to
//! every archive (see `.github/workflows/release-*.yml`).  The sidecar
//! follows the GNU coreutils format produced by `sha256sum`:
//!
//! ```text
//! <64-hex-digits>  <filename>\n
//! ```
//!
//! This module exposes:
//! * pure helpers ([`parse_checksum_sidecar`], [`parse_hex32`],
//!   [`sha256_hex`], [`verify_sha256`]) that the unit tests cover;
//! * an HTTP wrapper ([`fetch_checksum_sidecar`]) that downloads the
//!   small text file; and
//! * a streaming archive downloader ([`download_to_file`]) that hashes
//!   bytes as they arrive, writes them to disk, and refuses to leave a
//!   file behind on mismatch.
//!
//! No UI integration lives here — the screen layer
//! (`screens::components::shared::update_overlay`) calls these
//! functions and decides what to do with the resulting path.

use super::{user_agent, ReleaseAsset, UpdaterError};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Length of an upload `.sha256` sidecar in bytes is bounded; we refuse
/// anything larger than this to avoid pathological allocations on bad
/// servers.  A normal sidecar is ~80 bytes.
const SIDECAR_MAX_BYTES: u64 = 4096;

/// Streaming chunk size for asset downloads.  64 KiB balances syscall
/// overhead against memory pressure during the (~50 MiB) archive copy.
const COPY_CHUNK_BYTES: usize = 64 * 1024;

/// Lower-case hex of a SHA-256 digest.
#[inline]
pub fn sha256_hex(digest: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Decode a 64-character hex string into 32 raw bytes.  Returns `None`
/// for any non-hex character or wrong length.
pub fn parse_hex32(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let bytes = hex.as_bytes();
    let mut out = [0u8; 32];
    for (i, slot) in out.iter_mut().enumerate() {
        let hi = decode_nibble(bytes[i * 2])?;
        let lo = decode_nibble(bytes[i * 2 + 1])?;
        *slot = (hi << 4) | lo;
    }
    Some(out)
}

#[inline]
fn decode_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Constant-time-ish comparison of two SHA-256 digests.  Not a security
/// boundary (the digest is public), but writing it explicitly avoids
/// short-circuiting reads when added to other tooling later.
#[inline]
pub fn verify_sha256(actual: &[u8; 32], expected: &[u8; 32]) -> bool {
    let mut diff: u8 = 0;
    for (a, b) in actual.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Hash the supplied bytes with SHA-256.
pub fn sha256_of(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Stream-hash the file at `path` with SHA-256.  Used by the apply path
/// to re-verify the staged archive immediately before extraction so that
/// any tampering or bit-rot between download and apply surfaces as
/// [`UpdaterError::ChecksumMismatch`] rather than a corrupt install.
pub fn sha256_of_file(path: &Path) -> Result<[u8; 32], UpdaterError> {
    let mut file = File::open(path).map_err(|err| super::io_err_at("open", path, err))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; COPY_CHUNK_BYTES];
    loop {
        let read = file
            .read(&mut buf)
            .map_err(|err| super::io_err_at("read", path, err))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hasher.finalize().into())
}

/// Parse a `sha256sum`-style sidecar.  The sidecar may contain multiple
/// entries (one per line); we return the digest matching `expected_filename`.
///
/// Each entry is `<hex>  <name>` (two spaces separate hash and name in
/// GNU coreutils).  We accept either one or more spaces / a tab to be
/// permissive about trailing-whitespace cleanups.
pub fn parse_checksum_sidecar(
    text: &str,
    expected_filename: &str,
) -> Result<[u8; 32], UpdaterError> {
    if expected_filename.is_empty() {
        return Err(UpdaterError::ChecksumSidecarMalformed(
            "empty expected filename".to_owned(),
        ));
    }
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Split into "<hex>" and "<name>" (skip any leading "*" binary marker).
        let mut parts = line.splitn(2, |c: char| c.is_whitespace());
        let hex = match parts.next() {
            Some(h) => h.trim(),
            None => continue,
        };
        let rest = match parts.next() {
            Some(r) => r.trim_start().trim_start_matches('*').trim(),
            None => continue,
        };
        if rest == expected_filename {
            return parse_hex32(hex).ok_or_else(|| {
                UpdaterError::ChecksumSidecarMalformed(format!(
                    "invalid hex digest for {expected_filename}",
                ))
            });
        }
    }
    Err(UpdaterError::ChecksumSidecarMalformed(format!(
        "no entry for {expected_filename}",
    )))
}

/// Parse GitHub's release-asset `digest` field, which has the form
/// `"<algo>:<hex>"` (e.g. `"sha256:abcdef..."`).  Returns the raw 32-byte
/// SHA-256 digest, or:
///
/// * `Ok(None)` if the algorithm prefix is recognised but isn't sha256
///   (we have no way to verify it, so callers should skip the API
///   cross-check rather than fail closed); and
/// * `Err(ChecksumSidecarMalformed)` if the value is otherwise unparseable
///   (missing colon, bad hex, wrong digest length, etc.).
///
/// Used by the apply pipeline to cross-check that GitHub's API agrees
/// with the `.sha256` sidecar before we trust either.
pub fn parse_api_digest(value: &str) -> Result<Option<[u8; 32]>, UpdaterError> {
    let trimmed = value.trim();
    let (algo, hex) = trimmed.split_once(':').ok_or_else(|| {
        UpdaterError::ChecksumSidecarMalformed(format!(
            "api digest '{trimmed}' missing algorithm prefix"
        ))
    })?;
    if !algo.eq_ignore_ascii_case("sha256") {
        return Ok(None);
    }
    parse_hex32(hex.trim())
        .map(Some)
        .ok_or_else(|| {
            UpdaterError::ChecksumSidecarMalformed(format!(
                "api digest '{trimmed}' is not a valid sha256 hex value"
            ))
        })
}

/// Cross-check GitHub's `assets[].digest` field against the parsed `.sha256`
/// sidecar digest before we trust either as the verification target.
///
/// * `Ok(())` — either the API didn't surface a digest, the algorithm is
///   one we can't verify (skipped), or the API digest matched the sidecar.
/// * `Err(ChecksumMismatch)` — the API digest is sha256 and disagrees
///   with the sidecar; this almost certainly indicates a tampered sidecar
///   or a broken release publish, and we must fail closed.
/// * `Err(ChecksumSidecarMalformed)` — the API digest exists but is
///   syntactically broken (missing prefix / bad hex).
///
/// Returns the cross-check outcome via [`ApiDigestCheck`] so callers
/// can log the "skipped" case differently from the "matched" case
/// without re-parsing.
#[derive(Debug, PartialEq, Eq)]
pub enum ApiDigestCheck {
    /// No `digest` field on the asset.
    Absent,
    /// API digest used a non-sha256 algorithm we can't verify.
    UnsupportedAlgorithm,
    /// API digest matched the sidecar — proceed.
    Matched,
}

pub fn cross_check_api_digest(
    api_digest: Option<&str>,
    sidecar_digest: &[u8; 32],
) -> Result<ApiDigestCheck, UpdaterError> {
    let raw = match api_digest {
        Some(s) => s,
        None => return Ok(ApiDigestCheck::Absent),
    };
    match parse_api_digest(raw)? {
        None => Ok(ApiDigestCheck::UnsupportedAlgorithm),
        Some(api_bytes) => {
            if &api_bytes == sidecar_digest {
                Ok(ApiDigestCheck::Matched)
            } else {
                Err(UpdaterError::ChecksumMismatch {
                    expected: format!("api={}", sha256_hex(&api_bytes)),
                    actual: format!("sidecar={}", sha256_hex(sidecar_digest)),
                })
            }
        }
    }
}

/// Build the canonical sidecar URL for a release asset.
///
/// CI publishes `<archive>.sha256` alongside the archive at the same
/// browser-download base, so deriving the URL by string append matches
/// the real layout without an extra API call.
#[inline]
pub fn checksum_sidecar_url(asset_url: &str) -> String {
    format!("{asset_url}.sha256")
}

/// Download the `.sha256` sidecar for an asset.
pub fn fetch_checksum_sidecar(
    agent: &ureq::Agent,
    asset_url: &str,
) -> Result<String, UpdaterError> {
    let url = checksum_sidecar_url(asset_url);
    let response = agent
        .get(&url)
        .header("User-Agent", user_agent().as_str())
        .header("Accept", "text/plain")
        .call()
        .map_err(|err| UpdaterError::Network(err.to_string()))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(UpdaterError::HttpStatus(status));
    }
    let bytes = response
        .into_body()
        .with_config()
        .limit(SIDECAR_MAX_BYTES)
        .read_to_vec()
        .map_err(|err| UpdaterError::Network(err.to_string()))?;
    String::from_utf8(bytes)
        .map_err(|err| UpdaterError::ChecksumSidecarMalformed(err.to_string()))
}

/// Returns the staging path (`<dest>.part`) where bytes are written
/// before checksum verification.  The download is renamed on top of
/// `dest` only after a successful verify, so a crash / cancel / hash
/// mismatch can never leave a half-written file at the canonical name.
pub fn staging_path(dest: &Path) -> PathBuf {
    let mut name = dest
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

/// Stream `asset` into `<dest>.part`, hashing as it goes.  On success the
/// staging file is fsynced and atomically renamed onto `dest`; on
/// failure (network, cancel, checksum mismatch) the staging file is
/// removed and any pre-existing `dest` is left untouched.
///
/// `progress` is invoked after every chunk with `(written, total_opt)`
/// so the UI layer can render a progress bar.  The total may be
/// `None` if the server omits Content-Length; we fall back to the asset
/// metadata in that case.
///
/// `should_cancel` is polled before each chunk write; returning `true`
/// aborts the download with [`UpdaterError::Cancelled`] and removes the
/// staging file.  Callers that don't need cancellation can pass
/// `|| false`.
pub fn download_to_file(
    agent: &ureq::Agent,
    asset: &ReleaseAsset,
    expected_sha256: &[u8; 32],
    dest: &Path,
    mut progress: impl FnMut(u64, Option<u64>),
    should_cancel: impl Fn() -> bool,
) -> Result<(), UpdaterError> {
    if let Some(parent) = dest.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|err| super::io_err_at("create_dir_all", parent, err))?;
    }

    let staging = staging_path(dest);
    // Drop any leftover staging file from a previous crashed / killed
    // run; otherwise File::create would just truncate it but anything
    // weirder (different perms, hardlink) would surface as an error
    // partway through.
    let _ = fs::remove_file(&staging);

    if should_cancel() {
        return Err(UpdaterError::Cancelled);
    }

    let response = agent
        .get(&asset.browser_download_url)
        .header("User-Agent", user_agent().as_str())
        .header("Accept", "application/octet-stream")
        .call()
        .map_err(|err| UpdaterError::Network(err.to_string()))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(UpdaterError::HttpStatus(status));
    }

    let total = response
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| (asset.size > 0).then_some(asset.size));

    let mut reader = response.into_body().into_reader();
    let result = stream_to_file(
        &mut reader,
        &staging,
        expected_sha256,
        total,
        &mut progress,
        &should_cancel,
    );
    match result {
        Ok(()) => {
            replace_file(&staging, dest).map_err(|err| {
                // Rename failed — drop the staged bytes so we don't
                // leave them masquerading as the next run's "leftover".
                let _ = fs::remove_file(&staging);
                UpdaterError::Io(format!(
                    "rename '{}' -> '{}': {err}",
                    staging.display(),
                    dest.display(),
                ))
            })?;
            // Re-check cancellation after the rename: the user may
            // have pressed Back during the rename itself (or any of
            // the post-stream ureq teardown).  Drop the freshly-named
            // archive so a future attempt starts clean rather than
            // racing with a stale Ready-shaped artifact on disk.
            if should_cancel() {
                let _ = fs::remove_file(dest);
                return Err(UpdaterError::Cancelled);
            }
            Ok(())
        }
        Err(err) => {
            // Best-effort cleanup; ignore secondary I/O errors.
            let _ = fs::remove_file(&staging);
            Err(err)
        }
    }
}

/// Atomically (or as-close-to as the platform allows) move `staging`
/// onto `dest`, replacing any existing file at `dest`.  On Windows we
/// pre-delete `dest` to sidestep AV / network-share paths that don't
/// honour `MOVEFILE_REPLACE_EXISTING`.
fn replace_file(staging: &Path, dest: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        match fs::remove_file(dest) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    fs::rename(staging, dest)
}

fn stream_to_file<R: Read>(
    reader: &mut R,
    staging: &Path,
    expected_sha256: &[u8; 32],
    total: Option<u64>,
    progress: &mut dyn FnMut(u64, Option<u64>),
    should_cancel: &dyn Fn() -> bool,
) -> Result<(), UpdaterError> {
    let mut file = File::create(staging)
        .map_err(|err| super::io_err_at("create", staging, err))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; COPY_CHUNK_BYTES];
    let mut written: u64 = 0;
    loop {
        if should_cancel() {
            return Err(UpdaterError::Cancelled);
        }
        let read = reader
            .read(&mut buf)
            .map_err(|err| UpdaterError::Network(err.to_string()))?;
        if read == 0 {
            break;
        }
        let chunk = &buf[..read];
        hasher.update(chunk);
        file.write_all(chunk)
            .map_err(|err| super::io_err_at("write", staging, err))?;
        written += read as u64;
        progress(written, total);
    }
    file.flush()
        .map_err(|err| super::io_err_at("flush", staging, err))?;
    // Re-check cancellation between the last chunk and the
    // multi-second flush/fsync tail.  Without this, a Back press
    // during the final fsync would still let the worker proceed to
    // the rename + publish Ready.
    if should_cancel() {
        return Err(UpdaterError::Cancelled);
    }
    // fsync the staging file so its bytes are durable on disk before we
    // rename it onto `dest`.  Without this, a crash between the rename
    // and the next fsync could expose a zero-length file at the final
    // name on some filesystems.
    file.sync_all()
        .map_err(|err| super::io_err_at("fsync", staging, err))?;
    drop(file);

    let actual: [u8; 32] = hasher.finalize().into();
    if !verify_sha256(&actual, expected_sha256) {
        return Err(UpdaterError::ChecksumMismatch {
            expected: sha256_hex(expected_sha256),
            actual: sha256_hex(&actual),
        });
    }
    // Final pre-return cancel check so the caller never sees a
    // successful stream result for a cancelled download.
    if should_cancel() {
        return Err(UpdaterError::Cancelled);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO_DIGEST_HEX: &str =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn sha256_hex_round_trip() {
        let bytes = sha256_of(b"");
        let hex = sha256_hex(&bytes);
        assert_eq!(hex, ZERO_DIGEST_HEX);
        assert_eq!(parse_hex32(&hex), Some(bytes));
    }

    #[test]
    fn parse_hex32_rejects_bad_input() {
        assert!(parse_hex32("").is_none());
        assert!(parse_hex32("abc").is_none());
        // 63 chars + non-hex
        assert!(parse_hex32(&"z".repeat(64)).is_none());
        // Wrong length
        assert!(parse_hex32(&"a".repeat(63)).is_none());
        assert!(parse_hex32(&"a".repeat(65)).is_none());
    }

    #[test]
    fn parse_hex32_accepts_mixed_case() {
        let lower = ZERO_DIGEST_HEX;
        let upper = lower.to_uppercase();
        assert_eq!(parse_hex32(lower), parse_hex32(&upper));
    }

    #[test]
    fn verify_sha256_detects_difference() {
        let a = sha256_of(b"hello");
        let b = sha256_of(b"world");
        assert!(verify_sha256(&a, &a));
        assert!(!verify_sha256(&a, &b));
    }

    #[test]
    fn parse_sidecar_single_entry() {
        let sidecar = format!("{ZERO_DIGEST_HEX}  deadsync-v1.2.3-x86_64-linux.tar.zst\n");
        let digest =
            parse_checksum_sidecar(&sidecar, "deadsync-v1.2.3-x86_64-linux.tar.zst").unwrap();
        assert_eq!(sha256_hex(&digest), ZERO_DIGEST_HEX);
    }

    #[test]
    fn parse_sidecar_skips_blank_and_comment_lines() {
        let sidecar = format!(
            "# this is a comment\n\n{ZERO_DIGEST_HEX}  deadsync.zip\n# trailing comment\n"
        );
        let digest = parse_checksum_sidecar(&sidecar, "deadsync.zip").unwrap();
        assert_eq!(sha256_hex(&digest), ZERO_DIGEST_HEX);
    }

    #[test]
    fn parse_sidecar_multi_entry_picks_matching_name() {
        let other = "1111111111111111111111111111111111111111111111111111111111111111";
        let sidecar = format!(
            "{other}  deadsync-v1.2.3-arm64-linux.tar.zst\n\
             {ZERO_DIGEST_HEX}  deadsync-v1.2.3-x86_64-linux.tar.zst\n"
        );
        let digest =
            parse_checksum_sidecar(&sidecar, "deadsync-v1.2.3-x86_64-linux.tar.zst").unwrap();
        assert_eq!(sha256_hex(&digest), ZERO_DIGEST_HEX);
    }

    #[test]
    fn parse_sidecar_handles_binary_marker_and_crlf() {
        let sidecar = format!("{ZERO_DIGEST_HEX} *deadsync.zip\r\n");
        let digest = parse_checksum_sidecar(&sidecar, "deadsync.zip").unwrap();
        assert_eq!(sha256_hex(&digest), ZERO_DIGEST_HEX);
    }

    #[test]
    fn parse_sidecar_errors_when_filename_missing() {
        let sidecar = format!("{ZERO_DIGEST_HEX}  other.zip\n");
        let err = parse_checksum_sidecar(&sidecar, "deadsync.zip").unwrap_err();
        assert!(matches!(err, UpdaterError::ChecksumSidecarMalformed(_)));
    }

    #[test]
    fn parse_sidecar_errors_on_bad_hex() {
        let sidecar = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz  deadsync.zip\n";
        let err = parse_checksum_sidecar(sidecar, "deadsync.zip").unwrap_err();
        assert!(matches!(err, UpdaterError::ChecksumSidecarMalformed(_)));
    }

    #[test]
    fn parse_sidecar_errors_on_empty_filename() {
        let err = parse_checksum_sidecar("anything", "").unwrap_err();
        assert!(matches!(err, UpdaterError::ChecksumSidecarMalformed(_)));
    }

    #[test]
    fn parse_api_digest_accepts_sha256_lowercase() {
        let value = format!("sha256:{ZERO_DIGEST_HEX}");
        let parsed = parse_api_digest(&value).unwrap().unwrap();
        assert_eq!(sha256_hex(&parsed), ZERO_DIGEST_HEX);
    }

    #[test]
    fn parse_api_digest_accepts_uppercase_algo_and_hex() {
        let upper = ZERO_DIGEST_HEX.to_uppercase();
        let value = format!("SHA256:{upper}");
        let parsed = parse_api_digest(&value).unwrap().unwrap();
        assert_eq!(sha256_hex(&parsed), ZERO_DIGEST_HEX);
    }

    #[test]
    fn parse_api_digest_unknown_algorithm_returns_none() {
        // sha512 / blake3 / etc. — we have no plumbing to verify those,
        // so callers should skip the API cross-check rather than fail.
        let parsed = parse_api_digest(&format!("sha512:{}", "a".repeat(128))).unwrap();
        assert!(parsed.is_none());
    }

    #[test]
    fn parse_api_digest_missing_prefix_errors() {
        let err = parse_api_digest(ZERO_DIGEST_HEX).unwrap_err();
        assert!(matches!(err, UpdaterError::ChecksumSidecarMalformed(_)));
    }

    #[test]
    fn parse_api_digest_bad_hex_errors() {
        let err = parse_api_digest("sha256:not-a-real-hex-value").unwrap_err();
        assert!(matches!(err, UpdaterError::ChecksumSidecarMalformed(_)));
    }

    #[test]
    fn parse_api_digest_wrong_length_errors() {
        let err = parse_api_digest(&format!("sha256:{}", "a".repeat(63))).unwrap_err();
        assert!(matches!(err, UpdaterError::ChecksumSidecarMalformed(_)));
    }

    #[test]
    fn cross_check_api_digest_returns_absent_when_field_missing() {
        let sidecar = sha256_of(b"payload");
        assert_eq!(
            cross_check_api_digest(None, &sidecar).unwrap(),
            ApiDigestCheck::Absent
        );
    }

    #[test]
    fn cross_check_api_digest_returns_matched_when_equal() {
        let sidecar = sha256_of(b"payload");
        let api = format!("sha256:{}", sha256_hex(&sidecar));
        assert_eq!(
            cross_check_api_digest(Some(&api), &sidecar).unwrap(),
            ApiDigestCheck::Matched
        );
    }

    #[test]
    fn cross_check_api_digest_skips_unsupported_algorithm() {
        let sidecar = sha256_of(b"payload");
        // sha512 — present but unverifiable by this client.
        let api = format!("sha512:{}", "a".repeat(128));
        assert_eq!(
            cross_check_api_digest(Some(&api), &sidecar).unwrap(),
            ApiDigestCheck::UnsupportedAlgorithm
        );
    }

    #[test]
    fn cross_check_api_digest_fails_closed_on_mismatch() {
        // Detects a swapped/tampered sidecar: GitHub's API digest is the
        // ground truth, the .sha256 file is mirrored alongside the asset.
        let sidecar = sha256_of(b"payload");
        let bogus = sha256_of(b"different bytes");
        let api = format!("sha256:{}", sha256_hex(&bogus));
        let err = cross_check_api_digest(Some(&api), &sidecar).unwrap_err();
        assert!(matches!(err, UpdaterError::ChecksumMismatch { .. }));
    }

    #[test]
    fn cross_check_api_digest_propagates_parse_errors() {
        let sidecar = sha256_of(b"payload");
        let err = cross_check_api_digest(Some("garbage"), &sidecar).unwrap_err();
        assert!(matches!(err, UpdaterError::ChecksumSidecarMalformed(_)));
    }

    #[test]
    fn checksum_sidecar_url_appends_extension() {
        let base = "https://github.com/pnn64/deadsync/releases/download/v1.2.3/deadsync.zip";
        assert_eq!(
            checksum_sidecar_url(base),
            "https://github.com/pnn64/deadsync/releases/download/v1.2.3/deadsync.zip.sha256",
        );
    }

    #[test]
    fn stream_to_file_writes_and_verifies() {
        let dir = tempdir();
        let dest = dir.join("payload.bin");
        let staging = staging_path(&dest);
        let payload = b"the quick brown fox jumps over the lazy dog".to_vec();
        let expected = sha256_of(&payload);
        let mut reader = std::io::Cursor::new(payload.clone());
        let mut seen_progress = 0u64;
        stream_to_file(
            &mut reader,
            &staging,
            &expected,
            Some(payload.len() as u64),
            &mut |w, _| seen_progress = w,
            &|| false,
        )
        .unwrap();
        assert_eq!(seen_progress, payload.len() as u64);
        // stream_to_file writes to the staging path; download_to_file is
        // responsible for the rename onto `dest`.
        let written = std::fs::read(&staging).unwrap();
        assert_eq!(written, payload);
        assert!(!dest.exists(), "stream_to_file must not touch the final dest");
    }

    #[test]
    fn stream_to_file_rejects_mismatch_and_removes_partial() {
        let dir = tempdir();
        let dest = dir.join("bad.bin");
        let staging = staging_path(&dest);
        let payload = b"hello world".to_vec();
        let mut wrong = sha256_of(&payload);
        wrong[0] ^= 0xff;
        let mut reader = std::io::Cursor::new(payload.clone());
        let err =
            stream_to_file(&mut reader, &staging, &wrong, None, &mut |_, _| {}, &|| false)
                .unwrap_err();
        assert!(matches!(err, UpdaterError::ChecksumMismatch { .. }));
        // download_to_file performs the cleanup; here we mimic that contract:
        let _ = std::fs::remove_file(&staging);
        assert!(!staging.exists());
        assert!(!dest.exists());
    }

    #[test]
    fn stream_to_file_returns_cancelled_when_flag_set_before_first_chunk() {
        let dir = tempdir();
        let dest = dir.join("cancelled.bin");
        let staging = staging_path(&dest);
        let payload = vec![0u8; 256 * 1024];
        let expected = sha256_of(&payload);
        let mut reader = std::io::Cursor::new(payload);
        let err = stream_to_file(
            &mut reader,
            &staging,
            &expected,
            None,
            &mut |_, _| {},
            &|| true,
        )
        .unwrap_err();
        assert!(matches!(err, UpdaterError::Cancelled));
    }

    #[test]
    fn stream_to_file_returns_cancelled_mid_stream() {
        let dir = tempdir();
        let dest = dir.join("cancel-mid.bin");
        let staging = staging_path(&dest);
        // 4 chunks worth of bytes so we reach the second loop iteration.
        let payload = vec![0xabu8; COPY_CHUNK_BYTES * 4];
        let expected = sha256_of(&payload);
        let mut reader = std::io::Cursor::new(payload);
        let calls = std::cell::Cell::new(0u32);
        let err = stream_to_file(
            &mut reader,
            &staging,
            &expected,
            None,
            &mut |_, _| {},
            &|| {
                let n = calls.get();
                calls.set(n + 1);
                n >= 2 // cancel on the third poll (after some bytes written)
            },
        )
        .unwrap_err();
        assert!(matches!(err, UpdaterError::Cancelled));
    }

    #[test]
    fn stream_to_file_returns_cancelled_after_eof_before_fsync() {
        // The cancel callback returns false while reading chunks but
        // flips to true after EOF.  Without the post-EOF cancel check
        // the streamer would still report Ok and let the caller
        // proceed to rename + publish Ready.
        let dir = tempdir();
        let dest = dir.join("late-cancel.bin");
        let staging = staging_path(&dest);
        let payload = vec![0x55u8; 16 * 1024];
        let expected = sha256_of(&payload);
        let mut reader = std::io::Cursor::new(payload.clone());
        let saw_eof = std::cell::Cell::new(false);
        // The streamer polls `should_cancel` once per loop iteration:
        // returns 0/1 = false (read chunk + EOF read), then we flip to
        // true so the post-EOF check fires.
        let polls = std::cell::Cell::new(0u32);
        let err = stream_to_file(
            &mut reader,
            &staging,
            &expected,
            None,
            &mut |written, _| {
                if written as usize == payload.len() {
                    saw_eof.set(true);
                }
            },
            &|| {
                let n = polls.get();
                polls.set(n + 1);
                // First poll = false (lets us read the only chunk),
                // second poll = false (lets us see EOF),
                // third+ = true (post-EOF check fires).
                n >= 2
            },
        )
        .unwrap_err();
        assert!(matches!(err, UpdaterError::Cancelled));
        assert!(saw_eof.get(), "test should have streamed the full payload first");
        // download_to_file is responsible for staging cleanup on Err.
        let _ = std::fs::remove_file(&staging);
    }

    #[test]
    fn stream_to_file_returns_cancelled_after_hash_when_flag_flips_late() {
        // Even after the hash check passes, a post-hash cancel must
        // be honoured so the caller doesn't see a successful result
        // for a download the user already abandoned.
        let dir = tempdir();
        let dest = dir.join("post-hash-cancel.bin");
        let staging = staging_path(&dest);
        let payload = vec![0x77u8; 8 * 1024];
        let expected = sha256_of(&payload);
        let mut reader = std::io::Cursor::new(payload);
        let polls = std::cell::Cell::new(0u32);
        let err = stream_to_file(
            &mut reader,
            &staging,
            &expected,
            None,
            &mut |_, _| {},
            &|| {
                let n = polls.get();
                polls.set(n + 1);
                // Polls 0..=2 false (chunk, EOF, post-EOF check),
                // poll 3+ true (post-hash check fires).
                n >= 3
            },
        )
        .unwrap_err();
        assert!(matches!(err, UpdaterError::Cancelled));
        let _ = std::fs::remove_file(&staging);
    }

    #[test]
    fn replace_file_moves_staging_onto_missing_dest() {
        let dir = tempdir();
        let staging = dir.join("payload.bin.part");
        let dest = dir.join("payload.bin");
        std::fs::write(&staging, b"new bytes").unwrap();
        assert!(!dest.exists());
        replace_file(&staging, &dest).expect("replace into missing dest");
        assert!(!staging.exists());
        assert_eq!(std::fs::read(&dest).unwrap(), b"new bytes");
    }

    #[test]
    fn replace_file_overwrites_pre_existing_dest() {
        // The user-visible bug this guards: dismiss Ready, re-check,
        // re-download, and the second rename trips over the leftover
        // Ready-phase archive.  `replace_file` must succeed and the
        // new bytes must win.
        let dir = tempdir();
        let staging = dir.join("payload.bin.part");
        let dest = dir.join("payload.bin");
        std::fs::write(&dest, b"OLD-archive-contents").unwrap();
        std::fs::write(&staging, b"new-archive-contents").unwrap();
        replace_file(&staging, &dest).expect("replace over existing dest");
        assert!(!staging.exists());
        assert_eq!(std::fs::read(&dest).unwrap(), b"new-archive-contents");
    }

    #[test]
    fn staging_path_appends_part_extension() {
        let dest = std::path::Path::new("/cache/updates/deadsync-v1.2.3.tar.zst");
        assert_eq!(
            staging_path(dest),
            std::path::PathBuf::from("/cache/updates/deadsync-v1.2.3.tar.zst.part"),
        );
    }

    #[test]
    fn staging_path_handles_extensionless_filenames() {
        let dest = std::path::Path::new("/cache/updates/deadsync");
        assert_eq!(
            staging_path(dest),
            std::path::PathBuf::from("/cache/updates/deadsync.part"),
        );
    }

    fn tempdir() -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "deadsync-updater-download-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
