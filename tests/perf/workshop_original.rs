use super::*;
use crate::screens::components::shared::update_overlay::perf_original::PanelContent;
pub(crate) fn prepare_workshop(phase: &Phase) -> Option<PanelContent> {
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
