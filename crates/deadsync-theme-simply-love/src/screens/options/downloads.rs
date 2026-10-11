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
            Some(format!("{} / {}", format_size(*written), format_size(*total)).into()),
            "Cancel",
            Some(*written as f32 / (*total).max(1) as f32),
            false,
        ),
        Phase::Preparing { done, total } => (
            "Preparing",
            Some(tr("Downloads", "PreparingBody").into()),
            "Cancel",
            (*total > 0).then(|| *done as f32 / *total as f32),
            *total == 0,
        ),
        Phase::Publishing => (
            "Preparing",
            Some(tr("Downloads", "PublishingBody").into()),
            "Wait",
            None,
            true,
        ),
        Phase::Cancelling => ("Cancelling", None, "Wait", None, true),
        Phase::Installed => (
            "Installed",
            Some(tr("Downloads", "InstalledBody").into()),
            "Dismiss",
            None,
            false,
        ),
        Phase::Error { detail } => (
            "Error",
            Some(detail.chars().take(100).collect::<String>().into()),
            "Retry",
            None,
            false,
        ),
    };
    let mut lines = Vec::with_capacity(usize::from(body.is_some()) + 1);
    lines.push(tr("Downloads", status).into());
    lines.extend(body);
    Some(PanelContent::new(
        tr("Downloads", "Workshop").into(),
        None,
        lines,
        tr("Downloads", footer).into(),
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

#[cfg(test)]
#[path = "../../../../../tests/perf/workshop_original.rs"]
pub(crate) mod perf_original;

#[cfg(test)]
#[path = "../../../../../tests/perf/workshop_ownership.rs"]
pub(crate) mod perf_tests;
