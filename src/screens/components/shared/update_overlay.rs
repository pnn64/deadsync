//! Modal overlay that visualises [`crate::engine::updater::action::ActionPhase`].
//!
//! The overlay renders only when the action state is non-Idle.  It owns
//! no state of its own — every frame the screen passes the current
//! [`ActionPhase`] in, [`build`] returns the actor list, and
//! [`handle_input`] decides whether the screen should consume the input
//! or pass it through to the underlying menu.
//!
//! Layout (centred):
//!
//! * full-screen dim quad           — z 1500
//! * panel (≈ 600 × 360)             — z 1501
//! * title / body / footer text     — z 1502
//! * progress bar (Downloading)     — child of panel
//!
//! No animation: a static panel keeps the modal's geometry deterministic
//! (so the unit tests can assert actor counts) and avoids new tween work
//! while the underlying flow is still being built out.

use crate::act;
use crate::assets::i18n::{tr, tr_fmt};
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::present::actors::{Actor, TextAlign};
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::engine::updater::action::{
    self, ActionErrorKind, ActionPhase,
};


use super::loading_bar;

const PANEL_W: f32 = 720.0;
const PANEL_H: f32 = 420.0;
const PANEL_BG_HEX: &str = "#000000";
const PANEL_BORDER_HEX: &str = "#ffffff";
const TITLE_PX: f32 = 48.0;
const BODY_PX: f32 = 28.0;
const FOOTER_PX: f32 = 30.0;

const Z_BACKDROP: i16 = 1500;
const Z_PANEL_BORDER: i16 = 1501;
const Z_PANEL_BG: i16 = 1502;
const Z_PANEL_TEXT: i16 = 1503;

/// Pixel size for the prominent version tag (e.g. "v0.3.875") shown
/// above the body in download / apply phases.
const VERSION_PX: f32 = 60.0;

/// 30-frame spritesheet shared with the evaluation submit-status footer.
const SPINNER_TEXTURE: &str = "submit/LoadingSpinner_10x3.png";
const SPINNER_FRAMES: u32 = 30;
const SPINNER_FPS: f32 = 30.0;
const SPINNER_PX: f32 = 64.0;

/// Controls how [`handle_input`] reports back to its caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputOutcome {
    /// The overlay was Idle; the caller should run its own input.
    Passthrough,
    /// The overlay handled the event; the caller should treat it as
    /// consumed (do not navigate, do not exit).
    Consumed,
}

/// Build the actor list for the overlay.  Returns an empty `Vec` when
/// the phase is [`ActionPhase::Idle`], so callers can unconditionally
/// `.extend(update_overlay::build(&action::current()))`.
pub fn build(phase: &ActionPhase) -> Vec<Actor> {
    if matches!(phase, ActionPhase::Idle) {
        return Vec::new();
    }

    let mut actors = Vec::with_capacity(8);

    // 1) full-screen dim
    let mut dim = color::rgba_hex("#000000");
    dim[3] = 0.7;
    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(dim[0], dim[1], dim[2], dim[3]):
        z(Z_BACKDROP)
    ));

    // 2) panel background + border (drawn as two centred quads).
    let cx = screen_center_x();
    let cy = screen_center_y();
    let bg = color::rgba_hex(PANEL_BG_HEX);
    let border = color::rgba_hex(PANEL_BORDER_HEX);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(PANEL_W + 4.0, PANEL_H + 4.0):
        diffuse(border[0], border[1], border[2], 1.0):
        z(Z_PANEL_BORDER)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(PANEL_W, PANEL_H):
        diffuse(bg[0], bg[1], bg[2], 1.0):
        z(Z_PANEL_BG)
    ));

    let title_y = cy - PANEL_H * 0.5 + 50.0;
    let footer_y = cy + PANEL_H * 0.5 - 40.0;

    let (title, body_lines, footer, progress) = phase_strings(phase);
    let (title_rgba, body_rgba) = phase_palette(phase);

    actors.push(panel_text_tinted(
        &title,
        cx,
        title_y,
        TITLE_PX,
        TextAlign::Center,
        title_rgba,
    ));

    // If the phase carries a version tag, render it BIG below the title
    // as the visual focal point of the modal.
    let mut next_y = title_y + TITLE_PX * 0.55 + 24.0;
    if let Some(tag) = phase_version_tag(phase) {
        next_y += VERSION_PX * 0.5;
        actors.push(panel_text_tinted(
            &tag,
            cx,
            next_y,
            VERSION_PX,
            TextAlign::Center,
            [1.0, 1.0, 1.0, 1.0],
        ));
        next_y += VERSION_PX * 0.5 + 28.0;
    }

    let line_gap = 36.0;
    for (i, line) in body_lines.iter().enumerate() {
        let y = next_y + (i as f32) * line_gap;
        actors.push(panel_text_tinted(line, cx, y, BODY_PX, TextAlign::Center, body_rgba));
    }

    if let Some(progress) = progress {
        let bar_w = PANEL_W - 100.0;
        let bar_h = 32.0;
        let bar_x = cx - bar_w * 0.5;
        let bar_y = footer_y - 56.0;
        actors.push(loading_bar::build(loading_bar::LoadingBarParams {
            align: [0.0, 1.0],
            offset: [bar_x, bar_y],
            width: bar_w,
            height: bar_h,
            progress,
            label: progress_label(progress).into(),
            fill_rgba: color::rgba_hex("#3399ff"),
            bg_rgba: color::rgba_hex("#202020"),
            border_rgba: color::rgba_hex("#606060"),
            text_rgba: [1.0, 1.0, 1.0, 1.0],
            text_zoom: 0.8,
            z: Z_PANEL_TEXT,
        }));
    }

    let footer_display = animated_footer(&footer);
    actors.push(panel_text(&footer_display, cx, footer_y, FOOTER_PX, TextAlign::Center));

    if matches!(phase, ActionPhase::Checking | ActionPhase::Applying { .. }) {
        actors.push(spinner_actor(cx, cy + 60.0));
    }

    actors
}

/// Animated spinner sprite, frame derived from wall-clock time so the
/// renderer doesn't need to thread per-overlay state through every
/// build call.  Reuses the 10×3 spritesheet from the evaluation
/// submit-status footer.
fn spinner_actor(cx: f32, cy: f32) -> Actor {
    use std::sync::LazyLock;
    use std::time::Instant;
    static SPIN_START: LazyLock<Instant> = LazyLock::new(Instant::now);
    let elapsed = SPIN_START.elapsed().as_secs_f32();
    let frame = ((elapsed * SPINNER_FPS) as u32) % SPINNER_FRAMES;
    act!(sprite(SPINNER_TEXTURE):
        align(0.5, 0.5):
        xy(cx, cy):
        setsize(SPINNER_PX, SPINNER_PX):
        setstate(frame):
        z(Z_PANEL_TEXT):
        diffuse(1.0, 1.0, 1.0, 1.0)
    )
}

/// Replace any trailing horizontal-ellipsis ("…") on the footer with
/// an animated dot cycle.  The tail always reserves three character
/// slots so the (center-aligned) label doesn't shift left/right as
/// dots come and go.  Cycles a hair under 1 s end-to-end (~200 ms per
/// step) — fast enough to read as "alive" without strobing.
fn animated_footer(footer: &str) -> String {
    const DOT_PERIOD_S: f32 = 0.20;
    let Some(stripped) = footer.strip_suffix('…') else {
        return footer.to_owned();
    };
    use std::sync::LazyLock;
    use std::time::Instant;
    static DOT_START: LazyLock<Instant> = LazyLock::new(Instant::now);
    let elapsed = DOT_START.elapsed().as_secs_f32();
    let phase = ((elapsed / DOT_PERIOD_S) as u32) % 4;
    let dots: &str = match phase {
        0 => "   ",
        1 => ".  ",
        2 => ".. ",
        _ => "...",
    };
    format!("{stripped}{dots}")
}

#[inline]
fn panel_text(text: &str, x: f32, y: f32, px: f32, align: TextAlign) -> Actor {
    panel_text_tinted(text, x, y, px, align, [1.0, 1.0, 1.0, 1.0])
}

#[inline]
fn panel_text_tinted(
    text: &str,
    x: f32,
    y: f32,
    px: f32,
    align: TextAlign,
    rgba: [f32; 4],
) -> Actor {
    let mut actor = act!(text:
        font("miso"):
        settext(text.to_owned()):
        align(0.5, 0.5):
        xy(x, y):
        zoom(px / 28.0):
        z(Z_PANEL_TEXT):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3])
    );
    if let Actor::Text { align_text, .. } = &mut actor {
        *align_text = align;
    }
    actor
}

/// `(title_rgba, body_rgba)` for the phase.  Lets us tint the title green
/// for "good news" phases (update available, ready to install), white for
/// neutral status (checking, up to date), and red for errors.
/// All overlay text renders white; the panel background carries the
/// state, not the text colour.
fn phase_palette(_phase: &ActionPhase) -> ([f32; 4], [f32; 4]) {
    let white = [1.0, 1.0, 1.0, 1.0];
    (white, white)
}

/// The release tag (e.g. `"v0.3.875"`) the modal is talking about, when
/// the phase carries one.  Used so the build can render the tag as a
/// prominent focal point above the body text.
fn phase_version_tag(phase: &ActionPhase) -> Option<String> {
    match phase {
        // ConfirmDownload renders "Current: ..." / "Latest: ..." in the body,
        // so the big focal tag would just duplicate the latest version.
        ActionPhase::Downloading { info, .. }
        | ActionPhase::Ready { info, .. }
        | ActionPhase::Applying { info }
        | ActionPhase::AppliedRestartRequired { info, .. }
        | ActionPhase::AvailableNoInstall { info } => Some(info.tag.clone()),
        ActionPhase::UpToDate { tag } => Some(tag.clone()),
        _ => None,
    }
}

/// Return `(title, body_lines, footer_hint, progress_fraction_opt)` for
/// the supplied phase.  Pure so the unit tests can assert on the strings
/// without invoking the renderer.
pub fn phase_strings(phase: &ActionPhase) -> (String, Vec<String>, String, Option<f32>) {
    match phase {
        ActionPhase::Idle => (String::new(), Vec::new(), String::new(), None),
        ActionPhase::Checking => (
            tr("Updater", "TitleChecking").to_string(),
            vec![tr("Updater", "BodyChecking").to_string()],
            tr("Updater", "FooterPleaseWait").to_string(),
            None,
        ),
        ActionPhase::ConfirmDownload { info, asset } => {
            let mut body = Vec::with_capacity(5);
            body.push(
                tr_fmt(
                    "Updater",
                    "BodyCurrent",
                    &[("version", &crate::engine::version::current_tag())],
                )
                .to_string(),
            );
            body.push(
                tr_fmt("Updater", "BodyLatest", &[("version", &info.tag)]).to_string(),
            );
            body.push(
                tr_fmt(
                    "Updater",
                    "BodySize",
                    &[("size", &format_size(asset.size))],
                )
                .to_string(),
            );
            if let Some(date) = format_published_at(info.published_at.as_deref()) {
                body.push(tr_fmt("Updater", "BodyPublished", &[("date", &date)]).to_string());
            }
            if let Some(sha) = format_sha256_short(asset.digest.as_deref()) {
                body.push(tr_fmt("Updater", "BodySha256", &[("sha", &sha)]).to_string());
            }
            (
                tr("Updater", "TitleConfirm").to_string(),
                body,
                tr("Updater", "FooterConfirm").to_string(),
                None,
            )
        }
        ActionPhase::UpToDate { tag: _tag } => (
            tr("Updater", "TitleUpToDate").to_string(),
            // Tag is rendered above as the focal point, so the body
            // can stay empty (the title alone reads cleanly).
            Vec::new(),
            tr("Updater", "FooterDismiss").to_string(),
            None,
        ),
        ActionPhase::AvailableNoInstall { info } => (
            tr("Updater", "TitleConfirm").to_string(),
            vec![
                tr("Updater", "BodyManualDownload").to_string(),
                truncate(&info.html_url, 80),
            ],
            tr("Updater", "FooterDismiss").to_string(),
            None,
        ),
        ActionPhase::Downloading {
            info: _info,
            written,
            total,
            eta_secs,
            ..
        } => {
            let mut body = match total {
                Some(t) if *t > 0 => vec![format!("{} / {}", format_size(*written), format_size(*t))],
                _ => vec![format_size(*written)],
            };
            if let Some(secs) = eta_secs {
                body.push(
                    tr("Updater", "BodyEtaShort")
                        .replace("{time}", &format_eta(*secs)),
                );
            }
            let progress = total.and_then(|t| (t > 0).then_some(*written as f32 / t as f32));
            (
                tr("Updater", "TitleDownloading").to_string(),
                body,
                tr("Updater", "FooterPleaseWait").to_string(),
                progress.or(Some(0.0)),
            )
        }
        ActionPhase::Ready { info: _info, path: _path, .. } => (
            tr("Updater", "TitleReady").to_string(),
            vec![tr("Updater", "BodyReadyShort").to_string()],
            tr("Updater", "FooterInstall").to_string(),
            None,
        ),
        ActionPhase::Applying { info: _info } => (
            tr("Updater", "TitleApplying").to_string(),
            vec![tr("Updater", "BodyApplyingWarning").to_string()],
            tr("Updater", "FooterPleaseWait").to_string(),
            None,
        ),
        ActionPhase::AppliedRestartRequired { info: _info, detail } => (
            tr("Updater", "TitleAppliedRestartRequired").to_string(),
            vec![
                tr("Updater", "BodyAppliedRestartRequired").to_string(),
                truncate(detail, 80),
            ],
            tr("Updater", "FooterDismiss").to_string(),
            None,
        ),
        ActionPhase::Error { kind, detail } => (
            tr("Updater", "TitleError").to_string(),
            vec![
                tr("Updater", error_kind_key(*kind)).to_string(),
                truncate(detail, 80),
            ],
            tr("Updater", "FooterDismiss").to_string(),
            None,
        ),
    }
}

fn error_kind_key(kind: ActionErrorKind) -> &'static str {
    match kind {
        ActionErrorKind::Network => "ErrorNetwork",
        ActionErrorKind::RateLimited => "ErrorRateLimited",
        ActionErrorKind::HttpStatus => "ErrorHttpStatus",
        ActionErrorKind::Parse => "ErrorParse",
        ActionErrorKind::NoAssetForHost => "ErrorNoAsset",
        ActionErrorKind::Checksum => "ErrorChecksum",
        ActionErrorKind::Io => "ErrorIo",
    }
}

/// Dispatch a virtual input event against the current overlay state.
/// Mutates the global action state via [`action::request_download`] /
/// [`action::dismiss`] as appropriate.
pub fn handle_input(phase: &ActionPhase, ev: &InputEvent) -> InputOutcome {
    if matches!(phase, ActionPhase::Idle) {
        return InputOutcome::Passthrough;
    }
    if !ev.pressed {
        return InputOutcome::Consumed;
    }
    match phase {
        ActionPhase::ConfirmDownload { .. } => match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start => {
                action::request_download();
                InputOutcome::Consumed
            }
            VirtualAction::p1_back | VirtualAction::p2_back => {
                action::dismiss();
                InputOutcome::Consumed
            }
            _ => InputOutcome::Consumed,
        },
        ActionPhase::Ready { .. } => match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start => {
                action::request_apply();
                InputOutcome::Consumed
            }
            VirtualAction::p1_back | VirtualAction::p2_back => {
                action::dismiss();
                InputOutcome::Consumed
            }
            _ => InputOutcome::Consumed,
        },
        ActionPhase::UpToDate { .. }
        | ActionPhase::AvailableNoInstall { .. }
        | ActionPhase::AppliedRestartRequired { .. }
        | ActionPhase::Error { .. } => match ev.action {
            VirtualAction::p1_start
            | VirtualAction::p2_start
            | VirtualAction::p1_back
            | VirtualAction::p2_back => {
                action::dismiss();
                InputOutcome::Consumed
            }
            _ => InputOutcome::Consumed,
        },
        // Checking / Downloading: Back / Start cancel the in-flight
        // worker (the worker polls the cancel flag at safe points and
        // exits to Idle without committing partial state).  All other
        // input is swallowed so the user can't open menus underneath.
        ActionPhase::Checking | ActionPhase::Downloading { .. } => match ev.action {
            VirtualAction::p1_back | VirtualAction::p2_back => {
                action::request_cancel();
                InputOutcome::Consumed
            }
            _ => InputOutcome::Consumed,
        },
        // Applying: cannot safely abort a partial extract / swap, so
        // every input is swallowed.  Listed explicitly (rather than
        // falling through to a `_` arm) so adding a new `ActionPhase`
        // variant in the future is a compile error here instead of
        // silently inheriting the swallow-all behaviour.
        ActionPhase::Applying { .. } => InputOutcome::Consumed,
        ActionPhase::Idle => InputOutcome::Passthrough,
    }
}

/// Format a byte count as `"12.3 MiB"` / `"948 KiB"` / `"12 B"`.  Pure;
/// covered by unit tests.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut idx = 0;
    while value >= 1024.0 && idx + 1 < UNITS.len() {
        value /= 1024.0;
        idx += 1;
    }
    format!("{value:.1} {}", UNITS[idx])
}

/// Render a download-time-remaining estimate.  Buckets:
/// `<10s` for the noisy tail of large transfers, `M:SS` up to an
/// hour, and `Hh MMm` past that.  The output is short enough to fit
/// alongside the byte counter on a single overlay line.
pub fn format_eta(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    if m == 0 {
        format!("{s}s")
    } else {
        format!("{m}m{s}s")
    }
}

fn progress_label(progress: f32) -> String {
    let pct = (progress.clamp(0.0, 1.0) * 100.0).round() as i32;
    format!("{pct}%")
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Format the GitHub `published_at` ISO-8601 timestamp as a friendly
/// `YYYY-MM-DD` date.  Returns `None` when missing or unparseable so the
/// caller can simply skip the line.
fn format_published_at(raw: Option<&str>) -> Option<String> {
    let s = raw?.trim();
    if s.len() < 10 {
        return None;
    }
    let date = &s[..10];
    if date.as_bytes().get(4) == Some(&b'-') && date.as_bytes().get(7) == Some(&b'-') {
        Some(date.to_owned())
    } else {
        None
    }
}

/// Trim GitHub's `"sha256:..."` digest prefix and lowercase the hex so
/// the user can copy-paste the full hash for verification.  Returns
/// `None` when the upstream API didn't supply a digest.
fn format_sha256_short(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim().strip_prefix("sha256:").unwrap_or(raw?.trim());
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::updater::{ReleaseAsset, ReleaseInfo};
    use semver::Version;
    use std::path::PathBuf;

    fn sample_release() -> ReleaseInfo {
        ReleaseInfo {
            tag: "v9.9.9".to_owned(),
            version: Version::new(9, 9, 9),
            html_url: "https://example/v9.9.9".to_owned(),
            body: "first release note line\nsecond line\n".to_owned(),
            published_at: None,
            assets: vec![ReleaseAsset {
                name: "deadsync-v9.9.9-x86_64-linux.tar.gz".to_owned(),
                browser_download_url: "https://example/asset".to_owned(),
                size: 12 * 1024 * 1024,
                digest: None,
            }],
        }
    }

    #[test]
    fn build_idle_returns_no_actors() {
        assert!(build(&ActionPhase::Idle).is_empty());
    }

    #[test]
    fn build_checking_returns_actors() {
        let actors = build(&ActionPhase::Checking);
        assert!(!actors.is_empty(), "checking phase should render actors");
    }

    #[test]
    fn phase_strings_idle_is_empty() {
        let (t, b, f, p) = phase_strings(&ActionPhase::Idle);
        assert!(t.is_empty());
        assert!(b.is_empty());
        assert!(f.is_empty());
        assert!(p.is_none());
    }

    #[test]
    fn phase_strings_confirm_includes_version_and_size() {
        let mut r = sample_release();
        r.published_at = Some("2026-04-30T04:17:40Z".to_owned());
        r.assets[0].digest =
            Some("sha256:c154351dd3874a4a4630b16dbe673eb81b549342ac374ebf547d6fc3ac2e2b68".to_owned());
        let asset = r.assets[0].clone();
        let phase = ActionPhase::ConfirmDownload { info: r, asset };
        let (_t, body, _f, progress) = phase_strings(&phase);
        assert!(progress.is_none());
        let joined = body.join("\n");
        // ConfirmDownload no longer renders a focal version tag — current/latest
        // are surfaced in the body instead.
        assert!(phase_version_tag(&phase).is_none());
        assert!(joined.contains("Latest"), "latest label missing from {joined:?}");
        assert!(joined.contains("v9.9.9"), "latest version missing from {joined:?}");
        assert!(joined.contains("Current"), "current label missing from {joined:?}");
        assert!(joined.contains("MiB"), "size missing from {joined:?}");
        assert!(joined.contains("2026-04-30"), "date missing from {joined:?}");
        assert!(joined.contains("c154351dd3874a4a4630b16dbe673eb81b549342ac374ebf547d6fc3ac2e2b68"), "sha missing from {joined:?}");
        assert!(
            !joined.contains("first release note"),
            "release notes should be stripped from {joined:?}",
        );
    }

    #[test]
    fn phase_strings_confirm_omits_optional_lines_when_missing() {
        let mut r = sample_release();
        r.published_at = None;
        r.assets[0].digest = None;
        let asset = r.assets[0].clone();
        let phase = ActionPhase::ConfirmDownload { info: r, asset };
        let (_t, body, _f, _p) = phase_strings(&phase);
        // Current + Latest + Size remain even when date/digest are absent.
        assert_eq!(body.len(), 3, "body had {} lines: {body:?}", body.len());
        assert!(body[0].contains("Current"));
        assert!(body[1].contains("Latest"));
        assert!(body[2].contains("MiB"));
    }

    #[test]
    fn format_helpers() {
        assert_eq!(
            format_published_at(Some("2026-04-30T04:17:40Z")).as_deref(),
            Some("2026-04-30")
        );
        assert!(format_published_at(Some("garbage")).is_none());
        assert!(format_published_at(None).is_none());
        assert_eq!(
            format_sha256_short(Some("sha256:C154351DD3874A4A4630B16DBE673EB81B549342AC374EBF547D6FC3AC2E2B68")).as_deref(),
            Some("c154351dd3874a4a4630b16dbe673eb81b549342ac374ebf547d6fc3ac2e2b68"),
        );
        assert!(format_sha256_short(None).is_none());
    }

    #[test]
    fn phase_strings_downloading_reports_progress_fraction() {
        let r = sample_release();
        let asset = r.assets[0].clone();
        let phase = ActionPhase::Downloading {
            info: r,
            asset,
            written: 6 * 1024 * 1024,
            total: Some(12 * 1024 * 1024),
            eta_secs: None,
        };
        let (_t, _b, _f, p) = phase_strings(&phase);
        assert!(p.unwrap() > 0.49 && p.unwrap() < 0.51);
    }

    #[test]
    fn phase_strings_downloading_without_total_still_renders_bar() {
        let r = sample_release();
        let asset = r.assets[0].clone();
        let phase = ActionPhase::Downloading {
            info: r,
            asset,
            written: 1024,
            total: None,
            eta_secs: None,
        };
        let (_t, _b, _f, p) = phase_strings(&phase);
        // Always Some so the bar is visible; falls back to 0 when unknown.
        assert_eq!(p, Some(0.0));
    }

    #[test]
    fn phase_strings_downloading_appends_eta_line_when_known() {
        let r = sample_release();
        let asset = r.assets[0].clone();
        let phase = ActionPhase::Downloading {
            info: r,
            asset,
            written: 6 * 1024 * 1024,
            total: Some(12 * 1024 * 1024),
            eta_secs: Some(75),
        };
        let (_t, body, _f, _p) = phase_strings(&phase);
        assert_eq!(body.len(), 2);
        assert!(body[1].contains("1m15s"), "expected eta line, got {body:?}");
    }

    #[test]
    fn phase_strings_ready_shows_install_hint() {
        let r = sample_release();
        let phase = ActionPhase::Ready {
            info: r,
            path: PathBuf::from("/tmp/deadsync-v9.9.9-x86_64-linux.tar.gz"),
            sha256: [0u8; 32],
        };
        let (_t, body, footer, _p) = phase_strings(&phase);
        assert_eq!(phase_version_tag(&phase).as_deref(), Some("v9.9.9"));
        let joined = body.join("\n");
        assert!(joined.contains("verified"), "body was {joined:?}");
        assert!(footer.contains("Install"), "footer was {footer:?}");
        // The install/restart hint belongs in the footer only — no duplicated
        // body line.
        assert!(!joined.contains("install"), "body should not duplicate footer hint: {joined:?}");
        assert!(
            !joined.contains("/tmp/"),
            "raw download path should not be shown: {joined:?}",
        );
    }

    #[test]
    fn phase_strings_error_includes_detail() {
        let phase = ActionPhase::Error {
            kind: ActionErrorKind::Network,
            detail: "connection reset".to_owned(),
        };
        let (_t, body, _f, _p) = phase_strings(&phase);
        assert!(body.iter().any(|l| l.contains("connection reset")));
    }

    #[test]
    fn phase_strings_applied_restart_required_shows_restart_hint() {
        let r = sample_release();
        let phase = ActionPhase::AppliedRestartRequired {
            info: r,
            detail: "spawn new exe: permission denied".to_owned(),
        };
        let (title, body, footer, progress) = phase_strings(&phase);
        assert_eq!(phase_version_tag(&phase).as_deref(), Some("v9.9.9"));
        let joined = body.join("\n");
        assert!(
            title.to_lowercase().contains("install")
                || title.to_lowercase().contains("update"),
            "title was {title:?}",
        );
        assert!(
            joined.to_lowercase().contains("restart"),
            "body was {joined:?}",
        );
        assert!(
            joined.contains("permission denied"),
            "body should surface relaunch failure detail: {joined:?}",
        );
        assert!(footer.contains("OK"), "footer was {footer:?}");
        assert!(progress.is_none());
    }

    #[test]
    fn handle_input_dismisses_applied_restart_required() {
        let phase = ActionPhase::AppliedRestartRequired {
            info: sample_release(),
            detail: String::new(),
        };
        let ev = press(VirtualAction::p1_start);
        assert_eq!(handle_input(&phase, &ev), InputOutcome::Consumed);
    }

    #[test]
    fn handle_input_passes_through_when_idle() {
        let ev = press(VirtualAction::p1_start);
        assert_eq!(
            handle_input(&ActionPhase::Idle, &ev),
            InputOutcome::Passthrough
        );
    }

    #[test]
    fn handle_input_swallows_release_events_when_visible() {
        let ev = release(VirtualAction::p1_start);
        assert_eq!(
            handle_input(&ActionPhase::Checking, &ev),
            InputOutcome::Consumed
        );
    }

    #[test]
    fn handle_input_swallows_input_during_check_and_download() {
        let phases = [
            ActionPhase::Checking,
            ActionPhase::Downloading {
                info: sample_release(),
                asset: sample_release().assets[0].clone(),
                written: 0,
                total: Some(100),
                eta_secs: None,
            },
        ];
        for phase in phases {
            for action in [
                VirtualAction::p1_start,
                VirtualAction::p1_back,
                VirtualAction::p1_up,
            ] {
                let ev = press(action);
                assert_eq!(
                    handle_input(&phase, &ev),
                    InputOutcome::Consumed,
                    "phase {phase:?} action {action:?}",
                );
            }
        }
    }

    #[test]
    fn format_size_renders_human_readable() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1024 * 1024), "1.0 MiB");
        let s = format_size(12 * 1024 * 1024 + 512 * 1024);
        assert!(s.starts_with("12.5"), "got {s}");
        assert!(s.ends_with("MiB"));
    }

    #[test]
    fn format_eta_renders_minutes_optionally() {
        assert_eq!(format_eta(0), "0s");
        assert_eq!(format_eta(9), "9s");
        assert_eq!(format_eta(59), "59s");
        assert_eq!(format_eta(60), "1m0s");
        assert_eq!(format_eta(65), "1m5s");
        assert_eq!(format_eta(599), "9m59s");
        assert_eq!(format_eta(3600), "60m0s");
        assert_eq!(format_eta(3725), "62m5s");
    }

    #[test]
    fn truncate_uses_ellipsis_only_when_needed() {
        assert_eq!(truncate("hello", 10), "hello");
        let t = truncate("0123456789abcdef", 8);
        assert_eq!(t.chars().count(), 8);
        assert!(t.ends_with('…'));
    }

    fn press(action: VirtualAction) -> InputEvent {
        make_event(action, true)
    }

    fn release(action: VirtualAction) -> InputEvent {
        make_event(action, false)
    }

    fn make_event(action: VirtualAction, pressed: bool) -> InputEvent {
        use crate::engine::input::InputSource;
        use std::time::Instant;
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed,
            source: InputSource::Keyboard,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }
}
