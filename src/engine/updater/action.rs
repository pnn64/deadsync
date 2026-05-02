//! Action state machine for the in-app updater.
//!
//! `state.rs` holds the *passive* "is an update available?" snapshot
//! that the menu banner reads.  This file holds the *active* state for
//! the user-driven download flow:
//!
//! ```text
//!   Idle ──(request_check_now)──► Checking ──┬──► ConfirmDownload ──(request_download)──► Downloading ──► Ready
//!                                              ├──► UpToDate
//!                                              └──► Error
//!                                Downloading ─►(checksum mismatch)──► Error
//! ```
//!
//! All transitions go through pure functions on [`ActionPhase`] so the
//! state machine itself is unit-testable without touching the network or
//! disk.  The thin `request_*` helpers wrap a worker thread that calls
//! into the existing fetch / download primitives.
//!
//! The UI overlay (`screens::components::shared::update_overlay`)
//! renders [`ActionPhase`] and dispatches user input here; nothing in
//! this module touches winit, fonts, or the renderer.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use super::download::{
    cross_check_api_digest, download_to_file, fetch_checksum_sidecar, parse_checksum_sidecar,
    ApiDigestCheck,
};
use super::{
    apply_supported_for_host, classify, expected_asset_name, fetch_latest_release, host_target,
    pick_asset_for_host, FetchOutcome, ReleaseAsset, ReleaseInfo, UpdateState,
    UpdaterError,
};
use crate::config;

/// Subdirectory of `cache_dir` where downloaded archives land.
pub const DOWNLOADS_SUBDIR: &str = "updates";

/// Public phases of the download flow.  Cloned cheaply to hand to the
/// UI thread on every frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionPhase {
    /// Nothing in flight; the overlay should not be visible.
    Idle,
    /// A check-now request is running on the worker thread.
    Checking,
    /// A check completed and an update is available.  The overlay shows
    /// release notes / size / "Download" / "Later".
    ConfirmDownload {
        info: ReleaseInfo,
        asset: ReleaseAsset,
    },
    /// A check completed and reported the build is current.  Lets the
    /// overlay say "You're up to date" rather than vanishing silently.
    UpToDate { tag: String },
    /// A check completed and reported an update, but this build can't
    /// install it in-place on the current host (e.g. macOS).  We still
    /// surface the new version + the release-page URL so the user can
    /// download manually; the overlay never offers a Download button.
    AvailableNoInstall { info: ReleaseInfo },
    /// The download is in flight.  `total` is `Some` when the server
    /// reported a Content-Length and otherwise falls back to the asset
    /// metadata; it is also `None` while we're still computing it.
    /// `eta_secs` is the worker's best estimate of the remaining
    /// download time in whole seconds, or `None` when not enough
    /// samples have been collected yet (or the total size is unknown).
    Downloading {
        info: ReleaseInfo,
        asset: ReleaseAsset,
        written: u64,
        total: Option<u64>,
        eta_secs: Option<u64>,
    },
    /// The download finished and the file passed checksum verification.
    /// `path` is the absolute on-disk path to the verified archive.
    /// `sha256` is persisted with the snapshot so the apply step can
    /// re-verify the file immediately before extraction: a long
    /// gap between download and apply, on-disk corruption, or tampering
    /// would otherwise install a different archive than the one we
    /// verified at download time.
    Ready {
        info: ReleaseInfo,
        path: PathBuf,
        sha256: [u8; 32],
    },
    /// The user confirmed apply; the worker is extracting + swapping
    /// files in place.  The overlay shows a spinner.  On success the
    /// worker spawns the new process and `std::process::exit`s, so this
    /// phase is normally terminal-on-success; on failure the worker
    /// transitions to [`ActionPhase::Error`].
    Applying { info: ReleaseInfo },
    /// The on-disk install was successfully replaced with the new
    /// version, but the relaunch of the new binary failed (sandbox
    /// refusal, missing perms, ENOENT, etc.).  The current still-old
    /// process is now running against the new install tree, which is
    /// risky to keep using; the overlay tells the user to restart
    /// manually.  The journal is left in `Applied` so the next launch
    /// completes cleanup (backup deletion).
    AppliedRestartRequired { info: ReleaseInfo, detail: String },
    /// A failure surfaced by the worker.  `kind` lets the UI pick a
    /// localised summary; `detail` is a developer-facing string for logs
    /// and tooltips.
    Error { kind: ActionErrorKind, detail: String },
}

/// Coarse classification of failures so the UI can show a localised
/// summary without having to parse [`UpdaterError`] strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionErrorKind {
    Network,
    RateLimited,
    HttpStatus,
    Parse,
    NoAssetForHost,
    Checksum,
    Io,
}

impl ActionErrorKind {
    fn classify(err: &UpdaterError) -> Self {
        match err {
            UpdaterError::Network(_) => Self::Network,
            UpdaterError::RateLimited => Self::RateLimited,
            UpdaterError::HttpStatus(_) => Self::HttpStatus,
            UpdaterError::Parse(_) => Self::Parse,
            UpdaterError::AssetNotFound(_) => Self::NoAssetForHost,
            UpdaterError::ChecksumMismatch { .. }
            | UpdaterError::ChecksumSidecarMalformed(_) => Self::Checksum,
            UpdaterError::Io(_) => Self::Io,
            // Cancelled flows return early in the worker before
            // surfacing here, but classify defensively as Io so the
            // overlay degrades gracefully if it ever leaks through.
            UpdaterError::Cancelled => Self::Io,
        }
    }
}

/// Decides when a download progress tick is worth publishing to the
/// global PHASE.  Cloning `ReleaseInfo`/`ReleaseAsset` and taking the
/// PHASE write lock per ~64 KiB chunk burns thousands of allocations
/// and lock acquisitions per second on a fast link; throttling keeps
/// the overlay smooth without flooding.
#[derive(Default)]
struct ProgressThrottle {
    last_published: Option<Instant>,
    last_pct: Option<u32>,
    last_eta: Option<u64>,
}

impl ProgressThrottle {
    /// Returns `true` if this tick should be published.  Always
    /// publishes the first tick, the final byte (when `total` is
    /// known), and any change in integer percent or ETA bucket;
    /// otherwise rate-limits to one publication per 100 ms.
    ///
    /// For indeterminate streams (`total = None`) `pct` is always
    /// `None`, so the change-detector never fires and the byte
    /// counter advances on the 100 ms timer alone.  That's the
    /// intended behaviour: the overlay shows a byte counter (not a
    /// percent bar) for unknown-length downloads, and 10 Hz is
    /// plenty for a numeric readout.
    fn should_publish(
        &mut self,
        now: Instant,
        written: u64,
        total: Option<u64>,
        eta_secs: Option<u64>,
    ) -> bool {
        let pct = total.and_then(|t| {
            if t == 0 {
                None
            } else {
                Some(((written.min(t) as u128 * 100) / t as u128) as u32)
            }
        });
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

static PHASE: LazyLock<RwLock<ActionPhase>> =
    LazyLock::new(|| RwLock::new(ActionPhase::Idle));

/// Worker-thread mutex.  Held only by `request_*` helpers so that two
/// rapid-fire button presses never spawn two workers; the second call
/// becomes a no-op.
static WORKER_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Monotonic operation-generation counter.  Bumped by every
/// `request_check_now` / `request_download` / `request_cancel` so each
/// worker can capture its generation at spawn and refuse to publish
/// results once a newer operation has started.  Workers compare their
/// captured generation to the current one, and `set_phase_if_current`
/// refuses to clobber the global phase from a stale worker.
static OP_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Bump the generation and return the new value.  Called by every
/// worker-spawning request and by `request_cancel`.
fn begin_operation() -> u64 {
    OP_GENERATION.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
}

/// True if `generation` is no longer the active operation.  Workers
/// poll this at the same checkpoints the old `CANCEL` flag was polled
/// at, and treat a stale generation as a cancellation request.
fn worker_should_stop(generation: u64) -> bool {
    OP_GENERATION.load(Ordering::SeqCst) != generation
}

/// Returns `true` if the in-flight check / download has been
/// cancelled or superseded.  Retained as a thin wrapper over
/// [`worker_should_stop`] for callers that don't track a generation.
#[cfg(test)]
fn cancel_requested_for(generation: u64) -> bool {
    worker_should_stop(generation)
}

/// Mark the in-flight check / download as cancelled and immediately
/// flip the overlay to [`ActionPhase::Idle`].  The worker thread keeps
/// running until it reaches the next polling point but its result is
/// discarded.
///
/// No-op if the current phase isn't cancellable (i.e. only `Checking`
/// and `Downloading` consult the flag); calling it from `Applying` is
/// deliberately ignored because a partial apply can't safely be
/// abandoned.
pub fn request_cancel() {
    match current() {
        ActionPhase::Checking | ActionPhase::Downloading { .. } => {
            // Bump the generation: any in-flight worker's eventual
            // result will be discarded by `set_phase_if_current`.
            let _ = begin_operation();
            set_phase(ActionPhase::Idle);
        }
        _ => {}
    }
}

/// Snapshot of the current phase.  Cheap; clones strings.
pub fn current() -> ActionPhase {
    PHASE
        .read()
        .map(|guard| guard.clone())
        .unwrap_or(ActionPhase::Idle)
}

/// Reset the overlay to [`ActionPhase::Idle`].  Called on Cancel /
/// Escape from the UI; safe to call from any state.
pub fn dismiss() {
    set_phase(ActionPhase::Idle);
}

fn set_phase(next: ActionPhase) {
    if let Ok(mut guard) = PHASE.write() {
        *guard = next;
    }
}

/// Conditionally publish `next` to the global phase: only if
/// `generation` is still the active operation.  Returns `true` when
/// the phase was published.  Workers must use this (rather than
/// [`set_phase`]) for any state they want to *show the user*: if a
/// newer operation has started, the worker's result is stale and
/// silently dropped.
///
/// The generation is re-checked under the write lock so a request
/// that bumps the generation between our check and the store can't
/// slip in undetected.
fn set_phase_if_current(generation: u64, next: ActionPhase) -> bool {
    if let Ok(mut guard) = PHASE.write() {
        if OP_GENERATION.load(Ordering::SeqCst) == generation {
            *guard = next;
            return true;
        }
    }
    false
}

/// Pure transition: "check completed, here is the result, what should
/// the overlay show?".  Lifted out of the worker so we can unit-test it.
///
/// Reads `[Options] UpdaterInstallEnabled` from the live config; for
/// pure unit tests, prefer [`classify_check_result_with`] to inject the
/// flag directly without touching global state.
pub fn classify_check_result(state: UpdateState) -> ActionPhase {
    classify_check_result_with(state, config::get().updater_install_enabled)
}

/// Same as [`classify_check_result`] but takes the install-enabled flag
/// explicitly so tests can exercise both branches without mutating the
/// global config.
pub fn classify_check_result_with(state: UpdateState, install_enabled: bool) -> ActionPhase {
    match state {
        UpdateState::UpToDate => ActionPhase::UpToDate {
            tag: crate::engine::version::current_tag(),
        },
        UpdateState::UnknownLatest => ActionPhase::Error {
            kind: ActionErrorKind::Parse,
            detail: "latest release tag is not valid semver".to_owned(),
        },
        UpdateState::Available(info) => match host_target() {
            None => ActionPhase::Error {
                kind: ActionErrorKind::NoAssetForHost,
                detail: "this host platform is not in the release matrix".to_owned(),
            },
            Some(target) => match pick_asset_for_host(&info.assets, &info.tag, target) {
                Some(asset) => {
                    if !apply_supported_for_host() || !install_enabled {
                        // We ship a download for this host but either
                        // can't run the apply step in-place (notably
                        // macOS) or the operator has opted out via
                        // `[Options] UpdaterInstallEnabled = 0` (Steam
                        // / package-manager builds).  Show the release
                        // info without a Download button.
                        ActionPhase::AvailableNoInstall { info }
                    } else {
                        let asset = asset.clone();
                        ActionPhase::ConfirmDownload { info, asset }
                    }
                }
                None => ActionPhase::Error {
                    kind: ActionErrorKind::NoAssetForHost,
                    detail: format!(
                        "release {} did not include {}",
                        info.tag,
                        expected_asset_name(&info.tag, target),
                    ),
                },
            },
        },
    }
}

/// Pure transition: convert an [`UpdaterError`] into an [`ActionPhase::Error`].
pub fn classify_error(err: &UpdaterError) -> ActionPhase {
    ActionPhase::Error {
        kind: ActionErrorKind::classify(err),
        detail: err.to_string(),
    }
}

/// Spawn a worker that runs a "check now" against the GitHub releases
/// endpoint.  No-op if a worker is already in flight.  Always returns
/// after spawning (or deciding not to spawn); the caller polls
/// [`current`] for progress.
pub fn request_check_now() {
    let _guard = match WORKER_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if matches!(current(), ActionPhase::Checking | ActionPhase::Downloading { .. }) {
        return;
    }
    // Bump the generation *before* setting the phase so any prior
    // in-flight worker's late result is already stale by the time
    // they observe the new Checking phase.
    let generation = begin_operation();
    set_phase(ActionPhase::Checking);
    let _ = thread::Builder::new()
        .name("deadsync-updater-check".to_owned())
        .spawn(move || run_check_now(generation));
}

fn run_check_now(generation: u64) {
    let agent = super::check_agent();
    // We deliberately ignore the persisted ETag so a manual "Check now"
    // always returns Fresh; otherwise the user would be stuck staring at
    // a Checking spinner with nothing happening on a 304.
    let outcome = match fetch_latest_release(&agent, None) {
        Ok(o) => o,
        Err(err) => {
            if worker_should_stop(generation) {
                return;
            }
            log::warn!("Manual update check failed: {err}");
            set_phase_if_current(generation, classify_error(&err));
            return;
        }
    };
    if worker_should_stop(generation) {
        log::info!("Manual update check cancelled by user; discarding result");
        return;
    }
    let info = match outcome {
        FetchOutcome::NotModified => {
            // We forced etag=None so this branch is unreachable, but
            // surface it as up-to-date if the server returns 304 anyway.
            set_phase_if_current(
                generation,
                ActionPhase::UpToDate {
                    tag: crate::engine::version::current_tag(),
                },
            );
            return;
        }
        FetchOutcome::Fresh { info, .. } => info,
    };
    let state = classify(info);
    // Mirror the passive snapshot used by the menu banner so a manual
    // check refreshes that too.  This is unconditional: even a
    // superseded worker's freshly-fetched release info is useful for
    // the banner, and `replace_snapshot` has its own staleness check.
    super::state::replace_snapshot(state.clone());
    set_phase_if_current(generation, classify_check_result(state));
}

/// Spawn a worker that downloads + verifies the asset associated with
/// the current [`ActionPhase::ConfirmDownload`].  No-op if the phase
/// isn't `ConfirmDownload`.
pub fn request_download() {
    let _guard = match WORKER_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let (info, asset) = match current() {
        ActionPhase::ConfirmDownload { info, asset } => (info, asset),
        _ => return,
    };
    if !config::get().updater_install_enabled {
        // Operator opted out via `[Options] UpdaterInstallEnabled = 0`
        // after the ConfirmDownload phase was published.  Re-route to
        // the no-install variant so the overlay drops the Download
        // button on the next frame.
        set_phase(ActionPhase::AvailableNoInstall { info });
        return;
    }
    let generation = begin_operation();
    set_phase(ActionPhase::Downloading {
        info: info.clone(),
        asset: asset.clone(),
        written: 0,
        total: Some(asset.size).filter(|s| *s > 0),
        eta_secs: None,
    });
    let _ = thread::Builder::new()
        .name("deadsync-updater-download".to_owned())
        .spawn(move || run_download(info, asset, generation));
}

fn run_download(info: ReleaseInfo, asset: ReleaseAsset, generation: u64) {
    let check = super::check_agent();
    let sidecar = match fetch_checksum_sidecar(&check, &asset.browser_download_url) {
        Ok(t) => t,
        Err(err) => {
            if worker_should_stop(generation) {
                return;
            }
            log::warn!("Failed to fetch checksum sidecar: {err}");
            set_phase_if_current(generation, classify_error(&err));
            return;
        }
    };
    if worker_should_stop(generation) {
        log::info!("Update download cancelled before sidecar parse");
        return;
    }
    let expected = match parse_checksum_sidecar(&sidecar, &asset.name) {
        Ok(d) => d,
        Err(err) => {
            log::warn!("Failed to parse checksum sidecar: {err}");
            set_phase_if_current(generation, classify_error(&err));
            return;
        }
    };
    if let Some(api_digest) = asset.digest.as_deref() {
        match cross_check_api_digest(Some(api_digest), &expected) {
            Ok(ApiDigestCheck::Matched) => {}
            Ok(ApiDigestCheck::UnsupportedAlgorithm) => {
                log::info!(
                    "Skipping API digest cross-check for {}: unsupported algorithm in '{}'",
                    asset.name,
                    api_digest
                );
            }
            Ok(ApiDigestCheck::Absent) => unreachable!("guarded by Some(_)"),
            Err(err) => {
                log::warn!(
                    "GitHub API digest cross-check failed for {} (api='{}'): {err}",
                    asset.name,
                    api_digest
                );
                set_phase_if_current(generation, classify_error(&err));
                return;
            }
        }
    }
    let dest = downloads_dir().join(&asset.name);

    let info_for_progress = info.clone();
    let asset_for_progress = asset.clone();
    // Track the first observed (instant, written) sample so we can
    // estimate remaining time as a running average over the live
    // download.  Anchoring at the first chunk (rather than the
    // download start) discards TLS handshake / TTFB latency, which
    // would otherwise drag the early estimate way too high.
    let mut first_sample: Option<(Instant, u64)> = None;
    let mut throttle = ProgressThrottle::default();
    let progress = move |written: u64, total: Option<u64>| {
        let now = Instant::now();
        let (start_t, start_w) = *first_sample.get_or_insert((now, written));
        let eta_secs = match total {
            Some(t) if t > written => {
                let elapsed = now.duration_since(start_t).as_secs_f64();
                let bytes = written.saturating_sub(start_w) as f64;
                // Wait until we have at least ~half a second of samples
                // and a positive byte delta before publishing an ETA;
                // otherwise the first few estimates are wildly wrong.
                if elapsed >= 0.5 && bytes > 0.0 {
                    let speed = bytes / elapsed;
                    if speed > 0.0 {
                        Some(((t - written) as f64 / speed).ceil() as u64)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if !throttle.should_publish(now, written, total, eta_secs) {
            return;
        }
        // Progress updates use the gen-aware setter so a superseded
        // download's residual progress ticks can't overwrite a fresh
        // ConfirmDownload / Idle phase.
        set_phase_if_current(
            generation,
            ActionPhase::Downloading {
                info: info_for_progress.clone(),
                asset: asset_for_progress.clone(),
                written,
                total,
                eta_secs,
            },
        );
    };

    match download_to_file(
        &super::download_agent(),
        &asset,
        &expected,
        &dest,
        progress,
        move || worker_should_stop(generation),
    ) {
        Ok(()) => {
            log::info!("Update {} downloaded to {}", info.tag, dest.display());
            set_phase_if_current(
                generation,
                ActionPhase::Ready {
                    info,
                    path: dest,
                    sha256: expected,
                },
            );
        }
        Err(UpdaterError::Cancelled) => {
            log::info!("Update download cancelled by user");
            // request_cancel (or a superseding op) already flipped the
            // phase; nothing to publish.
        }
        Err(err) => {
            log::warn!("Update download failed: {err}");
            set_phase_if_current(generation, classify_error(&err));
        }
    }
}

/// Absolute path of the directory archives are downloaded into.
pub fn downloads_dir() -> PathBuf {
    config::dirs::app_dirs().cache_dir.join(DOWNLOADS_SUBDIR)
}

/// Spawn a worker that runs the platform apply + relaunch.  No-op if
/// the current phase isn't [`ActionPhase::Ready`].  On success the
/// worker spawns the new process and calls `std::process::exit(0)`, so
/// the caller never observes any phase past [`ActionPhase::Applying`]
/// in the success path.
pub fn request_apply() {
    let _guard = match WORKER_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let (info, path, sha256) = match current() {
        ActionPhase::Ready { info, path, sha256 } => (info, path, sha256),
        _ => return,
    };
    set_phase(ActionPhase::Applying { info: info.clone() });
    let _ = thread::Builder::new()
        .name("deadsync-updater-apply".to_owned())
        .spawn(move || run_apply(info, path, sha256));
}

fn run_apply(info: super::ReleaseInfo, archive_path: PathBuf, expected_sha256: [u8; 32]) {
    match super::cli::apply_archive_and_relaunch(&archive_path, &expected_sha256) {
        Ok(super::cli::ApplyOutcome::Relaunched) => {
            log::info!("Self-update applied; exiting to let new process take over");
            std::process::exit(0);
        }
        Ok(super::cli::ApplyOutcome::AppliedNoRelaunch { detail }) => {
            // Apply committed but spawn failed.  The on-disk install
            // is on the new version and the journal is `Applied`;
            // staying in this old process against a mutated install
            // tree is risky, but auto-exiting would also be hostile
            // (the user may not realise what happened).  Surface a
            // dedicated phase so the overlay can ask for a manual
            // restart, and log loudly for triage.  We use the `info`
            // captured at spawn time -- not `current()` -- so a
            // dismissal or error transition that races with the apply
            // worker can't strand the user on the old binary without
            // a restart prompt.
            log::warn!(
                "Self-update applied but relaunch failed: {detail}; manual restart required",
            );
            set_phase(ActionPhase::AppliedRestartRequired { info, detail });
        }
        Err(err) => {
            log::error!("Self-update apply failed: {err}");
            set_phase(classify_error(&err));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::version;
    use semver::Version;

    #[test]
    fn progress_throttle_publishes_first_tick_then_rate_limits() {
        let mut t = ProgressThrottle::default();
        let t0 = Instant::now();
        // First call always publishes, even with no total / no pct.
        assert!(t.should_publish(t0, 0, None, None));
        // Second call with no change in pct/eta and only a few millis
        // elapsed must be suppressed.
        assert!(!t.should_publish(t0 + Duration::from_millis(5), 1024, None, None));
    }

    #[test]
    fn progress_throttle_publishes_on_pct_change() {
        let mut t = ProgressThrottle::default();
        let t0 = Instant::now();
        let total = Some(1000u64);
        assert!(t.should_publish(t0, 0, total, None));
        // Same percent (0), <100ms elapsed -> suppressed.
        assert!(!t.should_publish(t0 + Duration::from_millis(1), 5, total, None));
        // Percent ticked from 0 -> 1 -> publish even though only a
        // millisecond passed.
        assert!(t.should_publish(t0 + Duration::from_millis(2), 11, total, None));
    }

    #[test]
    fn progress_throttle_always_publishes_final_byte() {
        let mut t = ProgressThrottle::default();
        let t0 = Instant::now();
        let total = Some(100u64);
        assert!(t.should_publish(t0, 0, total, None));
        assert!(!t.should_publish(t0 + Duration::from_millis(1), 0, total, None));
        // Final byte must publish so the UI never sticks at 99%.
        assert!(t.should_publish(t0 + Duration::from_millis(2), 100, total, None));
    }

    #[test]
    fn progress_throttle_caps_publication_count_for_streaming_download() {
        // Simulate a 10 MiB download streamed in 64 KiB chunks at
        // ~1 GiB/s.  Without throttling that's ~160 publications;
        // with throttling we expect roughly one per integer percent
        // (~100) plus a few elapsed-time ticks, well under 160.
        let mut t = ProgressThrottle::default();
        let total: u64 = 10 * 1024 * 1024;
        let chunk: u64 = 64 * 1024;
        let mut now = Instant::now();
        let mut written = 0u64;
        let mut publishes = 0usize;
        while written < total {
            written = (written + chunk).min(total);
            now += Duration::from_micros(64); // ~1 GiB/s
            if t.should_publish(now, written, Some(total), None) {
                publishes += 1;
            }
        }
        assert!(
            publishes <= 110,
            "expected throttled count <=110, got {publishes}",
        );
        assert!(publishes >= 1);
    }

    #[test]
    fn progress_throttle_advances_at_10hz_for_indeterminate_stream() {
        // For total=None the change-detector can't fire (pct is
        // always None and we pass eta=None), so publication is
        // driven entirely by the 100 ms timer.  Assert the byte
        // counter still advances roughly once per 100 ms over a
        // simulated one-second stream and never goes silent.
        let mut t = ProgressThrottle::default();
        let mut now = Instant::now();
        let mut publishes = 0usize;
        let mut written = 0u64;
        for _ in 0..1000 {
            written += 64 * 1024;
            now += Duration::from_millis(1);
            if t.should_publish(now, written, None, None) {
                publishes += 1;
            }
        }
        // 1000 ms / 100 ms cadence -> roughly 10 publications, plus
        // the initial first-tick.  Allow some scheduling slack.
        assert!(
            (8..=12).contains(&publishes),
            "expected ~10 publications at the 100ms floor, got {publishes}",
        );
    }

    fn release_with_tag(tag: &str, asset_name: &str) -> ReleaseInfo {
        ReleaseInfo {
            tag: tag.to_owned(),
            version: Version::parse(tag.trim_start_matches('v')).unwrap(),
            html_url: format!("https://example/{tag}"),
            body: String::new(),
            published_at: None,
            assets: vec![ReleaseAsset {
                name: asset_name.to_owned(),
                browser_download_url: format!("https://example/{tag}/{asset_name}"),
                size: 1024,
                digest: None,
            }],
        }
    }

    #[test]
    fn classify_check_result_up_to_date_yields_up_to_date_phase() {
        let phase = classify_check_result(UpdateState::UpToDate);
        assert!(matches!(phase, ActionPhase::UpToDate { .. }));
    }

    #[test]
    fn classify_check_result_unknown_latest_yields_error() {
        let phase = classify_check_result(UpdateState::UnknownLatest);
        match phase {
            ActionPhase::Error { kind, .. } => assert_eq!(kind, ActionErrorKind::Parse),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn classify_check_result_available_with_matching_asset_yields_confirm() {
        // Build an asset whose name matches whatever the host target is,
        // so the test passes on every platform we run CI on.
        let target = match host_target() {
            Some(t) => t,
            None => return, // unsupported host; nothing to assert.
        };
        let bumped = format!(
            "v{}.{}.{}",
            version::current().major,
            version::current().minor,
            version::current().patch + 1,
        );
        let name = expected_asset_name(&bumped, target);
        let info = release_with_tag(&bumped, &name);
        let phase = classify_check_result(UpdateState::Available(info.clone()));
        if apply_supported_for_host() {
            match phase {
                ActionPhase::ConfirmDownload { asset, .. } => assert_eq!(asset.name, name),
                other => panic!("expected ConfirmDownload, got {other:?}"),
            }
        } else {
            match phase {
                ActionPhase::AvailableNoInstall { info: got } => {
                    assert_eq!(got.tag, bumped);
                }
                other => panic!("expected AvailableNoInstall, got {other:?}"),
            }
        }
    }

    #[test]
    fn classify_check_result_available_on_unsupported_host_skips_download() {
        // Synthesize an Available state regardless of host so we exercise
        // the gating logic on every CI runner.  We can only assert when
        // running on a host that actually picks an asset; otherwise the
        // NoAssetForHost branch fires before the gate.
        let target = match host_target() {
            Some(t) => t,
            None => return,
        };
        let bumped = format!(
            "v{}.{}.{}",
            version::current().major,
            version::current().minor,
            version::current().patch + 1,
        );
        let name = expected_asset_name(&bumped, target);
        let info = release_with_tag(&bumped, &name);
        let phase = classify_check_result(UpdateState::Available(info));
        if apply_supported_for_host() {
            assert!(matches!(phase, ActionPhase::ConfirmDownload { .. }));
        } else {
            assert!(matches!(phase, ActionPhase::AvailableNoInstall { .. }));
        }
    }

    #[test]
    fn classify_check_result_with_install_disabled_skips_download() {
        // Operator-driven equivalent of the unsupported-host branch:
        // install_enabled=false should send Available -> AvailableNoInstall
        // even on hosts where apply_supported_for_host() is true.
        let target = match host_target() {
            Some(t) => t,
            None => return,
        };
        let bumped = format!(
            "v{}.{}.{}",
            version::current().major,
            version::current().minor,
            version::current().patch + 1,
        );
        let name = expected_asset_name(&bumped, target);
        let info = release_with_tag(&bumped, &name);

        let disabled = classify_check_result_with(
            UpdateState::Available(info.clone()),
            false,
        );
        match disabled {
            ActionPhase::AvailableNoInstall { info: got } => assert_eq!(got.tag, bumped),
            other => panic!("expected AvailableNoInstall when install disabled, got {other:?}"),
        }

        // Confirm the enabled branch still works on supported hosts so a
        // future regression in the gate doesn't silently disable installs.
        let enabled = classify_check_result_with(UpdateState::Available(info), true);
        if apply_supported_for_host() {
            assert!(matches!(enabled, ActionPhase::ConfirmDownload { .. }));
        } else {
            assert!(matches!(enabled, ActionPhase::AvailableNoInstall { .. }));
        }
    }

    #[test]
    fn classify_check_result_available_without_matching_asset_yields_error() {
        if host_target().is_none() {
            return;
        }
        let bumped = format!(
            "v{}.{}.{}",
            version::current().major,
            version::current().minor,
            version::current().patch + 1,
        );
        let info = release_with_tag(&bumped, "deadsync-x99-totally-fake.bin");
        let phase = classify_check_result(UpdateState::Available(info));
        match phase {
            ActionPhase::Error { kind, .. } => {
                assert_eq!(kind, ActionErrorKind::NoAssetForHost);
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn classify_error_maps_each_variant() {
        let cases = [
            (
                UpdaterError::Network("boom".into()),
                ActionErrorKind::Network,
            ),
            (UpdaterError::RateLimited, ActionErrorKind::RateLimited),
            (UpdaterError::HttpStatus(503), ActionErrorKind::HttpStatus),
            (UpdaterError::Parse("x".into()), ActionErrorKind::Parse),
            (
                UpdaterError::AssetNotFound("a".into()),
                ActionErrorKind::NoAssetForHost,
            ),
            (
                UpdaterError::ChecksumMismatch {
                    expected: "00".into(),
                    actual: "ff".into(),
                },
                ActionErrorKind::Checksum,
            ),
            (
                UpdaterError::ChecksumSidecarMalformed("bad".into()),
                ActionErrorKind::Checksum,
            ),
            (UpdaterError::Io("nope".into()), ActionErrorKind::Io),
        ];
        for (err, expected_kind) in cases {
            let phase = classify_error(&err);
            match phase {
                ActionPhase::Error { kind, .. } => assert_eq!(kind, expected_kind),
                other => panic!("expected Error, got {other:?}"),
            }
        }
    }

    #[test]
    fn request_cancel_flips_checking_to_idle_and_bumps_generation() {
        // Serialise behind WORKER_LOCK so we don't trample concurrent tests.
        let _g = WORKER_LOCK.lock().unwrap();
        let before = OP_GENERATION.load(Ordering::SeqCst);
        set_phase(ActionPhase::Checking);
        request_cancel();
        assert_eq!(current(), ActionPhase::Idle);
        assert_ne!(
            OP_GENERATION.load(Ordering::SeqCst),
            before,
            "request_cancel must bump the generation so the in-flight worker is invalidated",
        );
        // Any worker that captured `before` must now be considered stale.
        assert!(cancel_requested_for(before));
        set_phase(ActionPhase::Idle);
    }

    #[test]
    fn request_cancel_flips_downloading_to_idle_and_bumps_generation() {
        let _g = WORKER_LOCK.lock().unwrap();
        let before = OP_GENERATION.load(Ordering::SeqCst);
        let info = release_with_tag("v9.9.9", "x");
        let asset = info.assets[0].clone();
        set_phase(ActionPhase::Downloading {
            info,
            asset,
            written: 0,
            total: None,
            eta_secs: None,
        });
        request_cancel();
        assert_eq!(current(), ActionPhase::Idle);
        assert_ne!(OP_GENERATION.load(Ordering::SeqCst), before);
        assert!(cancel_requested_for(before));
        set_phase(ActionPhase::Idle);
    }

    #[test]
    fn request_cancel_is_noop_outside_check_or_download() {
        let _g = WORKER_LOCK.lock().unwrap();
        // Applying must not be cancellable: a partial extract / swap
        // would corrupt the install.
        set_phase(ActionPhase::Applying {
            info: release_with_tag("v9.9.9", "x"),
        });
        let before = OP_GENERATION.load(Ordering::SeqCst);
        request_cancel();
        assert!(matches!(current(), ActionPhase::Applying { .. }));
        assert_eq!(
            OP_GENERATION.load(Ordering::SeqCst),
            before,
            "non-cancellable phase must not bump the generation",
        );
        set_phase(ActionPhase::Idle);

        // Idle is also a no-op (nothing to cancel).
        let before = OP_GENERATION.load(Ordering::SeqCst);
        set_phase(ActionPhase::Idle);
        request_cancel();
        assert_eq!(current(), ActionPhase::Idle);
        assert_eq!(OP_GENERATION.load(Ordering::SeqCst), before);
    }

    #[test]
    fn set_phase_if_current_drops_stale_worker_writes() {
        // A slow worker captures a generation, the user cancels
        // (bumping the generation), and the worker belatedly tries to
        // publish a Ready / Error.  The stale result must be silently
        // dropped, not clobber the fresh Idle / Checking state the
        // cancel left behind.
        let _g = WORKER_LOCK.lock().unwrap();
        let stale = OP_GENERATION.load(Ordering::SeqCst);
        // Simulate the cancel + new op: bump the generation twice.
        let _ = begin_operation();
        set_phase(ActionPhase::Checking);
        let fresh = OP_GENERATION.load(Ordering::SeqCst);
        assert_ne!(stale, fresh);

        // Stale worker tries to publish Ready: must be dropped.
        let info = release_with_tag("v9.9.9", "x");
        let published = set_phase_if_current(
            stale,
            ActionPhase::Ready {
                info: info.clone(),
                path: PathBuf::from("ignored"),
                sha256: [0u8; 32],
            },
        );
        assert!(!published, "stale generation must not publish");
        assert!(
            matches!(current(), ActionPhase::Checking),
            "fresh phase must be preserved, got {:?}",
            current(),
        );

        // Fresh worker can publish.
        let published = set_phase_if_current(
            fresh,
            ActionPhase::UpToDate {
                tag: "v1.2.3".into(),
            },
        );
        assert!(published);
        assert!(matches!(current(), ActionPhase::UpToDate { .. }));
        set_phase(ActionPhase::Idle);
    }

    #[test]
    fn worker_should_stop_returns_true_when_generation_advances() {
        let _g = WORKER_LOCK.lock().unwrap();
        let mine = OP_GENERATION.load(Ordering::SeqCst);
        assert!(!worker_should_stop(mine));
        let _ = begin_operation();
        assert!(worker_should_stop(mine), "advancing generation must stop a stale worker");
    }

    #[test]
    fn dismiss_returns_to_idle_from_any_state() {
        // Drive the global state machine; serialise the test behind the
        // worker lock so other tests can't observe transient phases.
        let _g = WORKER_LOCK.lock().unwrap();
        set_phase(ActionPhase::Checking);
        dismiss();
        assert_eq!(current(), ActionPhase::Idle);

        set_phase(ActionPhase::UpToDate { tag: "v1.2.3".into() });
        dismiss();
        assert_eq!(current(), ActionPhase::Idle);

        set_phase(ActionPhase::Error {
            kind: ActionErrorKind::Network,
            detail: String::new(),
        });
        dismiss();
        assert_eq!(current(), ActionPhase::Idle);
    }

    #[test]
    fn downloads_dir_is_under_cache_dir() {
        let dir = downloads_dir();
        assert!(
            dir.ends_with(DOWNLOADS_SUBDIR),
            "downloads dir was {}",
            dir.display(),
        );
    }
}
