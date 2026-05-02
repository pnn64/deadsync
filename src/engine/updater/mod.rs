//! GitHub-release based update checker.
//!
//! This module is intentionally split into pure parsing/data logic and a
//! single thin HTTP wrapper so that everything except the actual network
//! call is unit-testable against the checked-in `fixtures/` JSON.
//!
//! The HTTP wrapper supports `If-None-Match` so we can re-check on launch
//! without re-downloading the (~14 KB) JSON payload, and it returns a typed
//! [`FetchOutcome`] that distinguishes a fresh response from a 304.

use crate::engine::version;
use semver::Version;
use serde::Deserialize;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::time::Duration;

pub mod download;
pub mod state;

#[cfg(windows)]
pub mod apply_windows;

pub mod apply_journal;

pub mod apply_unix;

/// Owner/repo of the upstream release feed.  Centralised so test fixtures
/// and CI artifacts use the same string.
pub const RELEASES_REPO: &str = "pnn64/deadsync";

/// Endpoint for the most recent non-prerelease, non-draft release.
pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/pnn64/deadsync/releases/latest";

/// Environment variable that, when set, replaces [`LATEST_RELEASE_URL`]
/// for the duration of the process.  Intended for local end-to-end
/// tests served by `python -m http.server` against a fixture
/// directory; never set this in production.
pub const ENV_RELEASE_URL_OVERRIDE: &str = "DEADSYNC_UPDATER_RELEASE_URL";

/// Resolves the URL the update check should hit.  By default this is
/// [`LATEST_RELEASE_URL`], but [`ENV_RELEASE_URL_OVERRIDE`] can
/// override it for local end-to-end tests.
pub fn release_url() -> String {
    std::env::var(ENV_RELEASE_URL_OVERRIDE)
        .unwrap_or_else(|_| LATEST_RELEASE_URL.to_string())
}

/// User-Agent header value sent with every request.  GitHub rejects API
/// calls that omit a UA.  Includes the build version so server-side logs
/// can correlate stale clients.
#[inline]
pub fn user_agent() -> String {
    format!("deadsync/{} (+https://github.com/pnn64/deadsync)", env!("CARGO_PKG_VERSION"))
}

/// Networking timeouts for the updater's HTTP traffic.  Two distinct
/// agents are exposed:
///
/// * [`check_agent`] — short global timeout, used for the small
///   release JSON / sidecar / etag-poke requests.  These should be
///   snappy so a slow network can't block startup indefinitely.
///
/// * [`download_agent`] — no global timeout (a 50 MiB archive on a
///   1 Mbps link genuinely takes minutes), but generous connect and
///   resolve timeouts so unreachable hosts still fail fast.
///
/// Both agents are kept process-wide via [`LazyLock`] so the
/// underlying connection pool is reused across calls.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);

static CHECK_AGENT: std::sync::LazyLock<ureq::Agent> = std::sync::LazyLock::new(|| {
    crate::engine::network::build_agent(crate::engine::network::AgentConfig::with_global(
        CHECK_TIMEOUT,
    ))
});

static DOWNLOAD_AGENT: std::sync::LazyLock<ureq::Agent> = std::sync::LazyLock::new(|| {
    crate::engine::network::build_agent(crate::engine::network::AgentConfig {
        timeout: None,
        connect_timeout: Some(DOWNLOAD_CONNECT_TIMEOUT),
        resolve_timeout: Some(DOWNLOAD_RESOLVE_TIMEOUT),
    })
});

/// Returns the shared agent used for the small update-check HTTP
/// calls (release JSON, ETag polls, checksum sidecar).
pub fn check_agent() -> ureq::Agent {
    CHECK_AGENT.clone()
}

/// Returns the shared agent used for streaming archive downloads.
/// No global timeout; relies on connect / resolve timeouts to fail
/// fast on unreachable hosts.
pub fn download_agent() -> ureq::Agent {
    DOWNLOAD_AGENT.clone()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    /// Optional GitHub-supplied digest (e.g. `"sha256:abcdef..."`) captured
    /// from the release API.  Surfaced in the confirm overlay so users can
    /// see what the binary will be verified against.
    pub digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag: String,
    pub version: Version,
    pub html_url: String,
    pub body: String,
    pub published_at: Option<String>,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    /// The build is on or ahead of the latest published release.
    UpToDate,
    /// A newer release is available.
    Available(ReleaseInfo),
    /// The latest tag could not be parsed as semver (e.g. someone pushed a
    /// tag like `nightly-…`).  Surfaced so the UI can decline to display
    /// stale "update available" banners on garbage tags.
    UnknownLatest,
}

/// Outcome of an HTTP poll against the releases endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchOutcome {
    /// Server responded `304 Not Modified` for the supplied ETag.
    NotModified,
    /// Server returned a fresh payload.
    Fresh {
        info: ReleaseInfo,
        etag: Option<String>,
    },
}

#[derive(Debug)]
pub enum UpdaterError {
    Network(String),
    HttpStatus(u16),
    RateLimited,
    Parse(String),
    Io(String),
    ChecksumMismatch { expected: String, actual: String },
    ChecksumSidecarMalformed(String),
    AssetNotFound(String),
    /// The user cancelled an in-flight check or download via the
    /// overlay; surfaced so the caller can route to `Idle` rather than
    /// the error phase.
    Cancelled,
}

impl Display for UpdaterError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "network error: {msg}"),
            Self::HttpStatus(code) => write!(f, "unexpected HTTP status {code}"),
            Self::RateLimited => f.write_str("github API rate limit exceeded"),
            Self::Parse(msg) => write!(f, "failed to parse release JSON: {msg}"),
            Self::Io(msg) => write!(f, "i/o error: {msg}"),
            Self::ChecksumMismatch { expected, actual } => write!(
                f,
                "sha256 mismatch: expected {expected}, downloaded {actual}",
            ),
            Self::ChecksumSidecarMalformed(msg) => {
                write!(f, "checksum sidecar malformed: {msg}")
            }
            Self::AssetNotFound(name) => write!(f, "release asset not found: {name}"),
            Self::Cancelled => f.write_str("cancelled by user"),
        }
    }
}

impl Error for UpdaterError {}

/// Wraps an `io::Error` with the operation name and path that produced
/// it, so user-facing messages identify *what* failed instead of
/// surfacing bare OS strings like `"Access is denied. (os error 5)"`.
///
/// `op` is a short verb (e.g. `"create_dir_all"`, `"rename"`,
/// `"open"`); `path` is the affected filesystem path.
pub fn io_err_at(op: &str, path: &std::path::Path, err: std::io::Error) -> UpdaterError {
    UpdaterError::Io(format!("{op} '{}': {err}", path.display()))
}

/// Like [`io_err_at`] but for operations without a meaningful single
/// path (zip header read, archive entry by-index, current_exe).
pub fn io_err_op(op: &str, err: impl Display) -> UpdaterError {
    UpdaterError::Io(format!("{op}: {err}"))
}

/// fsyncs a directory so that newly-created or renamed entries inside
/// it are durable across power loss. POSIX requires this in addition
/// to fsync on the file itself; Windows commits directory metadata as
/// part of the rename, so this is a no-op there.
#[cfg(unix)]
pub fn sync_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
pub fn sync_dir(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

/* ---------- raw JSON shape ---------- */

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

/// Parse a GitHub Releases JSON payload into [`ReleaseInfo`].
///
/// Unknown tags (those that don't parse as semver) cause this to return
/// `Err(UpdaterError::Parse(..))` rather than silently dropping the result,
/// because the alternative (treating an unparseable tag as "up to date")
/// would mask CI mistakes.
pub fn parse_release_json(bytes: &[u8]) -> Result<ReleaseInfo, UpdaterError> {
    let raw: RawRelease = serde_json::from_slice(bytes)
        .map_err(|err| UpdaterError::Parse(err.to_string()))?;
    let version = version::parse_release_tag(&raw.tag_name).ok_or_else(|| {
        UpdaterError::Parse(format!("tag '{}' is not valid semver", raw.tag_name))
    })?;
    let assets = raw
        .assets
        .into_iter()
        .map(|a| ReleaseAsset {
            name: a.name,
            browser_download_url: a.browser_download_url,
            size: a.size,
            digest: a.digest,
        })
        .collect();
    Ok(ReleaseInfo {
        tag: raw.tag_name,
        version,
        html_url: raw.html_url,
        body: raw.body,
        published_at: raw.published_at,
        assets,
    })
}

/// Compare a release against the current build and decide what to surface.
#[inline]
pub fn classify(latest: ReleaseInfo) -> UpdateState {
    let current = version::current();
    if version::is_newer(&latest.version, &current) {
        UpdateState::Available(latest)
    } else {
        UpdateState::UpToDate
    }
}

/* ---------- host asset selection ---------- */

/// The triplet of release-asset attributes that uniquely identifies a build
/// for a particular host.  The strings match the segments used in
/// `deadsync-{tag}-{arch}-{os}.{ext}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostTarget {
    pub arch: &'static str,
    pub os: &'static str,
    pub ext: &'static str,
}

/// Returns the [`HostTarget`] for the build doing the asking.  `None` when
/// the host is a combination we don't ship binaries for (which means we
/// also can't pick an asset for it).
#[inline]
pub const fn host_target() -> Option<HostTarget> {
    // Mapping from Rust target_arch -> the substring used in our asset
    // names.  Anything not listed is treated as unsupported.
    const ARCH: Option<&str> = if cfg!(target_arch = "x86_64") {
        Some("x86_64")
    } else if cfg!(target_arch = "aarch64") {
        Some("arm64")
    } else {
        None
    };

    // Mapping from target_os -> (asset-name-os, asset-extension).  Windows
    // is the only platform that ships a zip; everything else uses tar.gz.
    const OS_AND_EXT: Option<(&str, &str)> = if cfg!(target_os = "windows") {
        Some(("windows", "zip"))
    } else if cfg!(target_os = "linux") {
        Some(("linux", "tar.gz"))
    } else if cfg!(target_os = "macos") {
        Some(("macos", "tar.gz"))
    } else if cfg!(target_os = "freebsd") {
        Some(("freebsd", "tar.gz"))
    } else {
        None
    };

    match (ARCH, OS_AND_EXT) {
        (Some(arch), Some((os, ext))) => Some(HostTarget { arch, os, ext }),
        _ => None,
    }
}

/// Build the canonical asset filename for a release tag and host triplet.
/// Public so unit tests and the picker share the same string.
#[inline]
pub fn expected_asset_name(version_tag: &str, target: HostTarget) -> String {
    let HostTarget { arch, os, ext } = target;
    let tag = if version_tag.starts_with('v') {
        version_tag.to_string()
    } else {
        format!("v{version_tag}")
    };
    format!("deadsync-{tag}-{arch}-{os}.{ext}")
}

/// Pick the release asset matching the supplied host triplet, if any.
///
/// `version_tag` is taken from the [`ReleaseInfo`] (e.g. `v0.3.871`); the
/// matching is a strict equality check on `name` so we never accidentally
/// pull a sibling asset (e.g. a SHA256SUMS file or a different-arch
/// build).
#[inline]
pub fn pick_asset_for_host<'a>(
    assets: &'a [ReleaseAsset],
    version_tag: &str,
    target: HostTarget,
) -> Option<&'a ReleaseAsset> {
    let expected = expected_asset_name(version_tag, target);
    assets.iter().find(|a| a.name == expected)
}

/// Returns true when this build can perform the in-app extract + swap
/// for the host it's running on: today that's Windows and Linux/FreeBSD.
/// Notably **macOS is false**: we publish macOS download artifacts so
/// users can install manually, but the apply path isn't implemented
/// for `.app` bundle layout / code-signing concerns, so the in-app
/// overlay must not pretend it can install.
#[inline]
pub const fn apply_supported_for_host() -> bool {
    cfg!(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))
}

/// Fetch the latest release from GitHub.
///
/// `agent` is taken by reference so callers can plug in a configured ureq
/// agent (we use the shared one from `engine::network` in production but
/// tests can construct a no-network agent if needed).
///
/// Pass `etag = Some(prev)` to enable conditional requests; the server
/// returns 304 when the release hasn't changed and we avoid re-parsing.
pub fn fetch_latest_release(
    agent: &ureq::Agent,
    etag: Option<&str>,
) -> Result<FetchOutcome, UpdaterError> {
    let url = release_url();
    let mut request = agent
        .get(&url)
        .header("User-Agent", user_agent().as_str())
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(prev) = etag {
        request = request.header("If-None-Match", prev);
    }

    let response = match request.call() {
        Ok(resp) => resp,
        Err(err) => return Err(UpdaterError::Network(err.to_string())),
    };

    let status = response.status().as_u16();
    if status == 304 {
        return Ok(FetchOutcome::NotModified);
    }
    if status == 403 {
        // GitHub returns 403 for rate-limit exhaustion; distinguish so the
        // UI can show a friendlier message.
        if let Some(remaining) = response
            .headers()
            .get("X-RateLimit-Remaining")
            .and_then(|v| v.to_str().ok())
            && remaining == "0"
        {
            return Err(UpdaterError::RateLimited);
        }
        return Err(UpdaterError::HttpStatus(status));
    }
    if !(200..300).contains(&status) {
        return Err(UpdaterError::HttpStatus(status));
    }

    let etag = response
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    let bytes = response
        .into_body()
        .read_to_vec()
        .map_err(|err| UpdaterError::Network(err.to_string()))?;
    let info = parse_release_json(&bytes)?;
    Ok(FetchOutcome::Fresh { info, etag })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("fixtures/latest_release.json");

    #[test]
    fn parses_real_fixture() {
        let info = parse_release_json(FIXTURE).expect("fixture parses");
        assert_eq!(info.tag, "v0.3.871");
        assert_eq!(info.version, Version::new(0, 3, 871));
        assert!(info.html_url.contains("v0.3.871"));
        assert_eq!(info.assets.len(), 6, "fixture should expose 6 assets");
        let win = info
            .assets
            .iter()
            .find(|a| a.name == "deadsync-v0.3.871-x86_64-windows.zip")
            .expect("windows asset present");
        assert!(win.size > 1_000_000, "size should be the real archive size");
        assert!(win.browser_download_url.starts_with("https://github.com/"));
    }

    #[test]
    fn classify_up_to_date_when_versions_match() {
        let mut info = parse_release_json(FIXTURE).unwrap();
        info.version = version::current();
        info.tag = format!("v{}", version::current());
        assert_eq!(classify(info), UpdateState::UpToDate);
    }

    #[test]
    fn apply_supported_matches_cfg_targets() {
        // Mirrors the cfg gate in cli::apply_for_host so the two stay in
        // sync.  Updating one without the other would let the overlay
        // walk users into a "disabled in this build" dead-end.
        let expected = cfg!(any(
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd",
        ));
        assert_eq!(apply_supported_for_host(), expected);
    }

    #[test]
    fn classify_available_when_remote_newer() {
        let mut info = parse_release_json(FIXTURE).unwrap();
        let cur = version::current();
        info.version = Version::new(cur.major, cur.minor, cur.patch + 1);
        info.tag = format!("v{}", info.version);
        assert!(matches!(classify(info), UpdateState::Available(_)));
    }

    #[test]
    fn classify_up_to_date_when_remote_older() {
        let mut info = parse_release_json(FIXTURE).unwrap();
        info.version = Version::new(0, 0, 1);
        info.tag = "v0.0.1".to_string();
        assert_eq!(classify(info), UpdateState::UpToDate);
    }

    #[test]
    fn rejects_invalid_tag() {
        let bad = br#"{"tag_name":"nightly","html_url":"x","assets":[]}"#;
        assert!(matches!(
            parse_release_json(bad),
            Err(UpdaterError::Parse(_))
        ));
    }

    #[test]
    fn rejects_garbage_payload() {
        assert!(matches!(
            parse_release_json(b"not json"),
            Err(UpdaterError::Parse(_))
        ));
    }

    #[test]
    fn user_agent_includes_version() {
        let ua = user_agent();
        assert!(ua.starts_with("deadsync/"));
        assert!(ua.contains(env!("CARGO_PKG_VERSION")));
    }

    fn fixture_assets() -> Vec<ReleaseAsset> {
        parse_release_json(FIXTURE).unwrap().assets
    }

    fn target(arch: &'static str, os: &'static str, ext: &'static str) -> HostTarget {
        HostTarget { arch, os, ext }
    }

    #[test]
    fn expected_asset_name_handles_v_prefix() {
        let t = target("x86_64", "windows", "zip");
        assert_eq!(
            expected_asset_name("v0.3.871", t),
            "deadsync-v0.3.871-x86_64-windows.zip"
        );
        assert_eq!(
            expected_asset_name("0.3.871", t),
            "deadsync-v0.3.871-x86_64-windows.zip"
        );
    }

    #[test]
    fn picks_each_published_combo_from_fixture() {
        let assets = fixture_assets();
        let cases = [
            ("x86_64", "windows", "zip"),
            ("x86_64", "linux", "tar.gz"),
            ("arm64", "linux", "tar.gz"),
            ("x86_64", "macos", "tar.gz"),
            ("arm64", "macos", "tar.gz"),
            ("x86_64", "freebsd", "tar.gz"),
        ];
        for (arch, os, ext) in cases {
            let chosen = pick_asset_for_host(&assets, "v0.3.871", target(arch, os, ext))
                .unwrap_or_else(|| panic!("missing asset for {arch}-{os}.{ext}"));
            assert!(chosen.name.contains(arch));
            assert!(chosen.name.contains(os));
            assert!(chosen.name.ends_with(ext));
        }
    }

    #[test]
    fn returns_none_for_unknown_host_combo() {
        let assets = fixture_assets();
        assert!(
            pick_asset_for_host(&assets, "v0.3.871", target("riscv64", "linux", "tar.gz"))
                .is_none()
        );
    }

    #[test]
    fn returns_none_for_wrong_extension() {
        let assets = fixture_assets();
        // Windows asset is .zip, not .tar.gz; mismatched extension must
        // refuse to fall back to a different archive.
        assert!(
            pick_asset_for_host(&assets, "v0.3.871", target("x86_64", "windows", "tar.gz"))
                .is_none()
        );
    }

    #[test]
    fn host_target_is_some_on_supported_platforms() {
        // The compiler picks the cfg branch; just assert the function
        // returns Some for any host CI runs on (we ship binaries for all
        // of them).
        if cfg!(any(
            all(target_arch = "x86_64", target_os = "windows"),
            all(target_arch = "x86_64", target_os = "linux"),
            all(target_arch = "x86_64", target_os = "macos"),
            all(target_arch = "x86_64", target_os = "freebsd"),
            all(target_arch = "aarch64", target_os = "linux"),
            all(target_arch = "aarch64", target_os = "macos"),
        )) {
            let t = host_target().expect("supported host should resolve");
            assert!(!t.arch.is_empty() && !t.os.is_empty() && !t.ext.is_empty());
        }
    }
}