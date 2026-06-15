//! Modal overlay that visualises [`deadsync_updater::ffmpeg::FfmpegPhase`].
//!
//! Mirrors [`super::update_overlay`] but for the ffmpeg install flow. It
//! owns no state: each frame the screen passes in the current
//! [`FfmpegPhase`], [`build`] returns the actors, and [`handle_input`]
//! decides whether to consume the input or pass it to the menu.

use crate::assets::i18n::{tr, tr_fmt};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_present::actors::Actor;
use deadsync_updater::action::ActionErrorKind;
use deadsync_updater::ffmpeg::{self, FfmpegPhase};

use super::update_overlay::{
    InputOutcome, PanelContent, format_eta, format_size, format_speed, render_panel,
};

/// Build the actor list for the overlay, or an empty `Vec` when
/// [`FfmpegPhase::Idle`] so callers can unconditionally extend with it.
pub fn build(phase: &FfmpegPhase, active_color_index: i32) -> Vec<Actor> {
    if matches!(phase, FfmpegPhase::Idle) {
        return Vec::new();
    }
    let content = panel_content(phase);
    render_panel(&content, active_color_index)
}

/// Map a [`FfmpegPhase`] to renderable [`PanelContent`].
fn panel_content(phase: &FfmpegPhase) -> PanelContent {
    match phase {
        FfmpegPhase::Idle => PanelContent {
            title: String::new(),
            version_tag: None,
            body_lines: Vec::new(),
            footer: String::new(),
            progress: None,
            show_spinner: false,
        },
        FfmpegPhase::Checking => PanelContent {
            title: tr("FfmpegInstall", "TitleChecking").to_string(),
            version_tag: None,
            body_lines: vec![tr("FfmpegInstall", "BodyChecking").to_string()],
            footer: tr("FfmpegInstall", "FooterPleaseWait").to_string(),
            progress: None,
            show_spinner: true,
        },
        FfmpegPhase::Confirm {
            version,
            origin,
            total,
            already_available,
        } => {
            let mut body = Vec::new();
            if *already_available {
                body.push(tr("FfmpegInstall", "BodyAlreadyOptional").to_string());
            }
            body.push(tr("FfmpegInstall", "BodyConfirm").to_string());
            body.push(tr_fmt("FfmpegInstall", "BodySource", &[("origin", origin)]).to_string());
            if let Some(t) = total.filter(|t| *t > 0) {
                body.push(
                    tr_fmt("FfmpegInstall", "BodySize", &[("size", &format_size(t))]).to_string(),
                );
            }
            let title = if *already_available {
                tr("FfmpegInstall", "TitleAlready")
            } else {
                tr("FfmpegInstall", "TitleConfirm")
            };
            PanelContent {
                title: title.to_string(),
                version_tag: version_tag(version),
                body_lines: body,
                footer: tr("FfmpegInstall", "FooterConfirm").to_string(),
                progress: None,
                show_spinner: false,
            }
        }
        FfmpegPhase::Downloading {
            version,
            written,
            total,
            eta_secs,
            speed_bps,
        } => {
            let mut body = match total {
                Some(t) if *t > 0 => {
                    vec![format!("{} / {}", format_size(*written), format_size(*t))]
                }
                _ => vec![format_size(*written)],
            };
            if let Some(secs) = eta_secs {
                body.push(
                    tr("FfmpegInstall", "BodyEtaShort").replace("{time}", &format_eta(*secs)),
                );
            }
            if let Some(bps) = speed_bps {
                body.push(tr("FfmpegInstall", "BodySpeed").replace("{speed}", &format_speed(*bps)));
            }
            let progress = total.and_then(|t| (t > 0).then_some(*written as f32 / t as f32));
            PanelContent {
                title: tr("FfmpegInstall", "TitleDownloading").to_string(),
                version_tag: version_tag(version),
                body_lines: body,
                footer: tr("FfmpegInstall", "FooterPleaseWait").to_string(),
                progress: progress.or(Some(0.0)),
                show_spinner: false,
            }
        }
        FfmpegPhase::Extracting { version } => PanelContent {
            title: tr("FfmpegInstall", "TitleExtracting").to_string(),
            version_tag: version_tag(version),
            body_lines: vec![tr("FfmpegInstall", "BodyExtracting").to_string()],
            footer: tr("FfmpegInstall", "FooterPleaseWait").to_string(),
            progress: None,
            show_spinner: true,
        },
        FfmpegPhase::Installed { version } => PanelContent {
            title: tr("FfmpegInstall", "TitleInstalled").to_string(),
            version_tag: version_tag(version),
            body_lines: vec![tr("FfmpegInstall", "BodyInstalled").to_string()],
            footer: tr("FfmpegInstall", "FooterDismiss").to_string(),
            progress: None,
            show_spinner: false,
        },
        FfmpegPhase::Unsupported => PanelContent {
            title: tr("FfmpegInstall", "TitleUnsupported").to_string(),
            version_tag: None,
            body_lines: vec![
                tr("FfmpegInstall", "BodyUnsupported").to_string(),
                tr("FfmpegInstall", "BodyUnsupportedHint").to_string(),
            ],
            footer: tr("FfmpegInstall", "FooterDismiss").to_string(),
            progress: None,
            show_spinner: false,
        },
        FfmpegPhase::AlreadyAvailable => PanelContent {
            title: tr("FfmpegInstall", "TitleAlready").to_string(),
            version_tag: None,
            body_lines: vec![tr("FfmpegInstall", "BodyAlready").to_string()],
            footer: tr("FfmpegInstall", "FooterDismiss").to_string(),
            progress: None,
            show_spinner: false,
        },
        FfmpegPhase::Error { kind, detail } => PanelContent {
            title: tr("FfmpegInstall", "TitleError").to_string(),
            version_tag: None,
            body_lines: vec![
                tr("FfmpegInstall", error_kind_key(*kind)).to_string(),
                truncate(detail, 80),
            ],
            footer: tr("FfmpegInstall", "FooterDismiss").to_string(),
            progress: None,
            show_spinner: false,
        },
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
/// Mutates the global ffmpeg state via [`ffmpeg::request_confirm`] /
/// [`ffmpeg::request_cancel`] / [`ffmpeg::dismiss`] as appropriate.
pub fn handle_input(phase: &FfmpegPhase, ev: &InputEvent) -> InputOutcome {
    if matches!(phase, FfmpegPhase::Idle) {
        return InputOutcome::Passthrough;
    }
    if !ev.pressed {
        return InputOutcome::Consumed;
    }
    match phase {
        // While probing, Back aborts and returns to the menu; other input
        // is swallowed.  The probe resolves to Confirm/AlreadyAvailable/
        // Unsupported on its own when it finishes.
        FfmpegPhase::Checking => match ev.action {
            VirtualAction::p1_back | VirtualAction::p2_back => {
                ffmpeg::cancel_check();
                InputOutcome::Consumed
            }
            _ => InputOutcome::Consumed,
        },
        FfmpegPhase::Confirm { .. } => match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start => {
                ffmpeg::request_confirm();
                InputOutcome::Consumed
            }
            VirtualAction::p1_back | VirtualAction::p2_back => {
                ffmpeg::dismiss();
                InputOutcome::Consumed
            }
            _ => InputOutcome::Consumed,
        },
        // Back cancels the download; the worker exits to Idle without
        // committing partial state.
        FfmpegPhase::Downloading { .. } => match ev.action {
            VirtualAction::p1_back | VirtualAction::p2_back => {
                ffmpeg::request_cancel();
                InputOutcome::Consumed
            }
            _ => InputOutcome::Consumed,
        },
        // Extracting can't be safely aborted: swallow all input.
        FfmpegPhase::Extracting { .. } => InputOutcome::Consumed,
        FfmpegPhase::Installed { .. }
        | FfmpegPhase::Unsupported
        | FfmpegPhase::AlreadyAvailable
        | FfmpegPhase::Error { .. } => match ev.action {
            VirtualAction::p1_start
            | VirtualAction::p2_start
            | VirtualAction::p1_back
            | VirtualAction::p2_back => {
                ffmpeg::dismiss();
                InputOutcome::Consumed
            }
            _ => InputOutcome::Consumed,
        },
        FfmpegPhase::Idle => InputOutcome::Passthrough,
    }
}

/// Prefix an ffmpeg version with `v` for the focal tag (e.g. `8.1.1` →
/// `v8.1.1`); idempotent if already prefixed.
fn version_tag(version: &str) -> Option<String> {
    let trimmed = version.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('v') || trimmed.starts_with('V') {
        Some(trimmed.to_owned())
    } else {
        Some(format!("v{trimmed}"))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn press(action: VirtualAction) -> InputEvent {
        use deadsync_core::input::InputSource;
        use std::time::Instant;
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed: true,
            source: InputSource::Keyboard,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    #[test]
    fn build_idle_returns_no_actors() {
        assert!(build(&FfmpegPhase::Idle, 0).is_empty());
    }

    #[test]
    fn build_confirm_returns_actors() {
        let phase = FfmpegPhase::Confirm {
            version: "7.0".to_owned(),
            origin: "gyan.dev".to_owned(),
            total: Some(90_000_000),
            already_available: false,
        };
        assert!(!build(&phase, 0).is_empty());
    }

    #[test]
    fn handle_input_passes_through_when_idle() {
        let ev = press(VirtualAction::p1_start);
        assert_eq!(
            handle_input(&FfmpegPhase::Idle, &ev),
            InputOutcome::Passthrough
        );
    }

    #[test]
    fn handle_input_consumes_when_visible() {
        let ev = press(VirtualAction::p1_start);
        assert_eq!(
            handle_input(&FfmpegPhase::Unsupported, &ev),
            InputOutcome::Consumed
        );
    }

    #[test]
    fn already_available_builds_and_dismisses() {
        assert!(!build(&FfmpegPhase::AlreadyAvailable, 0).is_empty());
        let ev = press(VirtualAction::p1_start);
        assert_eq!(
            handle_input(&FfmpegPhase::AlreadyAvailable, &ev),
            InputOutcome::Consumed
        );
    }

    #[test]
    fn version_tag_adds_v_prefix() {
        assert_eq!(version_tag("8.1.1").as_deref(), Some("v8.1.1"));
        assert_eq!(version_tag("v7.1.1").as_deref(), Some("v7.1.1"));
        assert_eq!(version_tag("  7.0  ").as_deref(), Some("v7.0"));
        assert_eq!(version_tag(""), None);
    }
}
