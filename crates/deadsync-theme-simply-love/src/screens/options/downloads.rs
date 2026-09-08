//! Update checks, optional downloads, and the Workshop installation panel.

use super::*;
use crate::SimplyLoveUpdaterRequest as Request;
use crate::screens::components::shared::update_overlay::{InputOutcome, PanelContent, format_size};
use crate::views::SimplyLoveWorkshopPhase as Phase;

pub(super) const DOWNLOADS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::CheckForUpdates,
        label: lookup_key("Options", "CheckForUpdates"),
        choices: &[],
        inline: false,
    },
    SubRow {
        id: SubRowId::DownloadVideoSupport,
        label: lookup_key("Downloads", "VideoSupport"),
        choices: &[],
        inline: false,
    },
    SubRow {
        id: SubRowId::DownloadWorkshop,
        label: lookup_key("Downloads", "Workshop"),
        choices: &[],
        inline: false,
    },
];

pub(super) const DOWNLOADS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::CheckForUpdates,
        name: lookup_key("Options", "CheckForUpdates"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "CheckForUpdatesHelp",
        ))],
    },
    Item {
        id: ItemId::DownloadVideoSupport,
        name: lookup_key("Downloads", "VideoSupport"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "DownloadVideoSupportHelp",
        ))],
    },
    Item {
        id: ItemId::DownloadWorkshop,
        name: lookup_key("Downloads", "Workshop"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "Downloads",
            "WorkshopHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub(super) fn prepare_workshop(phase: &Phase) -> Option<PanelContent> {
    let (status, body, footer, progress, spinner) = match phase {
        Phase::Idle => return None,
        Phase::Downloading { written, total } => (
            "Downloading",
            vec![format!(
                "{} / {}",
                format_size(*written),
                format_size(*total)
            )],
            "Cancel",
            Some(*written as f32 / (*total).max(1) as f32),
            false,
        ),
        Phase::Preparing { done, total } => (
            "Preparing",
            vec![tr("Downloads", "PreparingBody").to_string()],
            "Cancel",
            (*total > 0).then(|| *done as f32 / *total as f32),
            *total == 0,
        ),
        Phase::Publishing => (
            "Preparing",
            vec![tr("Downloads", "PublishingBody").to_string()],
            "Wait",
            None,
            true,
        ),
        Phase::Cancelling => ("Cancelling", Vec::new(), "Wait", None, true),
        Phase::Installed => (
            "Installed",
            vec![tr("Downloads", "InstalledBody").to_string()],
            "Dismiss",
            None,
            false,
        ),
        Phase::Error { detail } => (
            "Error",
            vec![detail.chars().take(100).collect()],
            "Retry",
            None,
            false,
        ),
    };
    let mut lines = Vec::with_capacity(body.len() + 1);
    lines.push(tr("Downloads", status).to_string());
    lines.extend(body);
    Some(PanelContent::new(
        tr("Downloads", "Workshop").to_string(),
        None,
        lines,
        tr("Downloads", footer).to_string(),
        progress,
        spinner,
    ))
}

pub(super) fn workshop_input(phase: &Phase, ev: &InputEvent) -> InputOutcome {
    if matches!(phase, Phase::Idle) {
        return InputOutcome::Passthrough;
    }
    if !ev.pressed {
        return InputOutcome::Consumed;
    }
    let back = matches!(ev.action, VirtualAction::p1_back | VirtualAction::p2_back);
    let start = matches!(ev.action, VirtualAction::p1_start | VirtualAction::p2_start);
    match phase {
        Phase::Downloading { .. } | Phase::Preparing { .. } if back => {
            InputOutcome::Request(Request::DismissWorkshop)
        }
        Phase::Installed if start || back => InputOutcome::Request(Request::DismissWorkshop),
        Phase::Error { .. } if start => InputOutcome::Request(Request::InstallWorkshop),
        Phase::Error { .. } if back => InputOutcome::Request(Request::DismissWorkshop),
        _ => InputOutcome::Consumed,
    }
}
