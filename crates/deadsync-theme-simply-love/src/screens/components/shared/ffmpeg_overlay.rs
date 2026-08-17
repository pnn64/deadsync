//! Modal overlay that visualises a shell-prepared FFmpeg install phase.
//!
//! Mirrors [`super::update_overlay`] for the FFmpeg install flow. Options
//! retains prepared panel content between source revisions, while input
//! continues to inspect the current [`FfmpegPhase`].

use crate::assets::i18n::{tr, tr_fmt};
use crate::effects::SimplyLoveUpdaterRequest;
use crate::views::{
    SimplyLoveFfmpegPhase as FfmpegPhase, SimplyLoveUpdateErrorKind as ActionErrorKind,
};
use deadlib_present::actors::Actor;
use deadsync_input::{InputEvent, VirtualAction};

use super::update_overlay::{
    InputOutcome, PanelContent, format_eta, format_size, format_speed, render_panel,
};

/// Compile actor-ready panel content when the FFmpeg phase changes.
pub(crate) fn prepare(phase: &FfmpegPhase) -> Option<PanelContent> {
    if matches!(phase, FfmpegPhase::Idle) {
        return None;
    }
    Some(panel_content(phase))
}

/// Build the actor list from retained content, or no actors when idle.
pub(crate) fn build(content: Option<&PanelContent>, active_color_index: i32) -> Vec<Actor> {
    content.map_or_else(Vec::new, |content| {
        render_panel(content, active_color_index)
    })
}

/// Map a [`FfmpegPhase`] to renderable [`PanelContent`].
fn panel_content(phase: &FfmpegPhase) -> PanelContent {
    match phase {
        FfmpegPhase::Idle => {
            PanelContent::new(String::new(), None, Vec::new(), String::new(), None, false)
        }
        FfmpegPhase::Checking => PanelContent::new(
            tr("FfmpegInstall", "TitleChecking").to_string(),
            None,
            vec![tr("FfmpegInstall", "BodyChecking").to_string()],
            tr("FfmpegInstall", "FooterPleaseWait").to_string(),
            None,
            true,
        ),
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
            PanelContent::new(
                title.to_string(),
                version_tag(version),
                body,
                tr("FfmpegInstall", "FooterConfirm").to_string(),
                None,
                false,
            )
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
            PanelContent::new(
                tr("FfmpegInstall", "TitleDownloading").to_string(),
                version_tag(version),
                body,
                tr("FfmpegInstall", "FooterPleaseWait").to_string(),
                progress.or(Some(0.0)),
                false,
            )
        }
        FfmpegPhase::Extracting { version } => PanelContent::new(
            tr("FfmpegInstall", "TitleExtracting").to_string(),
            version_tag(version),
            vec![tr("FfmpegInstall", "BodyExtracting").to_string()],
            tr("FfmpegInstall", "FooterPleaseWait").to_string(),
            None,
            true,
        ),
        FfmpegPhase::Installed { version } => PanelContent::new(
            tr("FfmpegInstall", "TitleInstalled").to_string(),
            version_tag(version),
            vec![tr("FfmpegInstall", "BodyInstalled").to_string()],
            tr("FfmpegInstall", "FooterDismiss").to_string(),
            None,
            false,
        ),
        FfmpegPhase::Unsupported => PanelContent::new(
            tr("FfmpegInstall", "TitleUnsupported").to_string(),
            None,
            vec![
                tr("FfmpegInstall", "BodyUnsupported").to_string(),
                tr("FfmpegInstall", "BodyUnsupportedHint").to_string(),
            ],
            tr("FfmpegInstall", "FooterDismiss").to_string(),
            None,
            false,
        ),
        FfmpegPhase::AlreadyAvailable => PanelContent::new(
            tr("FfmpegInstall", "TitleAlready").to_string(),
            None,
            vec![tr("FfmpegInstall", "BodyAlready").to_string()],
            tr("FfmpegInstall", "FooterDismiss").to_string(),
            None,
            false,
        ),
        FfmpegPhase::Error { kind, detail } => PanelContent::new(
            tr("FfmpegInstall", "TitleError").to_string(),
            None,
            vec![
                tr("FfmpegInstall", error_kind_key(*kind)).to_string(),
                truncate(detail, 80),
            ],
            tr("FfmpegInstall", "FooterDismiss").to_string(),
            None,
            false,
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
                InputOutcome::Request(SimplyLoveUpdaterRequest::CancelFfmpegCheck)
            }
            _ => InputOutcome::Consumed,
        },
        FfmpegPhase::Confirm { .. } => match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start => {
                InputOutcome::Request(SimplyLoveUpdaterRequest::ConfirmFfmpegInstall)
            }
            VirtualAction::p1_back | VirtualAction::p2_back => {
                InputOutcome::Request(SimplyLoveUpdaterRequest::DismissFfmpeg)
            }
            _ => InputOutcome::Consumed,
        },
        // Back cancels the download; the worker exits to Idle without
        // committing partial state.
        FfmpegPhase::Downloading { .. } => match ev.action {
            VirtualAction::p1_back | VirtualAction::p2_back => {
                InputOutcome::Request(SimplyLoveUpdaterRequest::CancelFfmpegDownload)
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
                InputOutcome::Request(SimplyLoveUpdaterRequest::DismissFfmpeg)
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

    fn build_phase(phase: &FfmpegPhase, active_color_index: i32) -> Vec<Actor> {
        let content = prepare(phase);
        build(content.as_ref(), active_color_index)
    }

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
        assert!(build_phase(&FfmpegPhase::Idle, 0).is_empty());
    }

    #[test]
    fn build_confirm_returns_actors() {
        let phase = FfmpegPhase::Confirm {
            version: "7.0".to_owned(),
            origin: "gyan.dev".to_owned(),
            total: Some(90_000_000),
            already_available: false,
        };
        assert!(!build_phase(&phase, 0).is_empty());
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
            InputOutcome::Request(SimplyLoveUpdaterRequest::DismissFfmpeg)
        );
    }

    #[test]
    fn already_available_builds_and_dismisses() {
        assert!(!build_phase(&FfmpegPhase::AlreadyAvailable, 0).is_empty());
        let ev = press(VirtualAction::p1_start);
        assert_eq!(
            handle_input(&FfmpegPhase::AlreadyAvailable, &ev),
            InputOutcome::Request(SimplyLoveUpdaterRequest::DismissFfmpeg)
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
