// Frozen from f686ea82f94ef92967f791f8dede7494f492c020 for behavioral and paired performance comparisons.
use super::*;
pub(crate) struct PanelContent {
    /// Large heading at the top of the panel.
    title: TextContent,
    /// Optional focal tag rendered BIG below the title (e.g. a version).
    version_tag: Option<TextContent>,
    /// Centre-aligned body lines.
    body_lines: Vec<TextContent>,
    /// Four prepared footer frames; a trailing "…" animates as a dot cycle.
    footer_frames: [TextContent; 4],
    footer_animated: bool,
    /// Determinate progress and its prepared label, if any.
    progress: Option<PanelProgress>,
    /// Whether to render the animated spinner sprite.
    show_spinner: bool,
}

struct PanelProgress {
    fraction: f32,
    label: TextContent,
}

impl PanelContent {
    pub(crate) fn new(
        title: String,
        version_tag: Option<String>,
        body_lines: Vec<String>,
        footer: String,
        progress: Option<f32>,
        show_spinner: bool,
    ) -> Self {
        let (footer_frames, footer_animated) = footer_frames(footer);
        Self {
            title: retained_string(title),
            version_tag: version_tag.map(retained_string),
            body_lines: body_lines.into_iter().map(retained_string).collect(),
            footer_frames,
            footer_animated,
            progress: progress.map(|fraction| PanelProgress {
                fraction,
                label: retained_string(progress_label(fraction)),
            }),
            show_spinner,
        }
    }
}

pub(crate) fn prepare(phase: &ActionPhase) -> Option<PanelContent> {
    if matches!(phase, ActionPhase::Idle) {
        return None;
    }
    let (title, body_lines, footer, progress) = phase_strings(phase);
    Some(PanelContent::new(
        title,
        phase_version_tag(phase),
        body_lines,
        footer,
        progress,
        matches!(
            phase,
            ActionPhase::Checking | ActionPhase::Applying { .. } | ActionPhase::RollbackChecking
        ),
    ))
}

pub(crate) fn phase_strings(phase: &ActionPhase) -> (String, Vec<String>, String, Option<f32>) {
    match phase {
        ActionPhase::Idle => (String::new(), Vec::new(), String::new(), None),
        ActionPhase::Checking => (
            tr("Updater", "TitleChecking").to_string(),
            vec![tr("Updater", "BodyChecking").to_string()],
            tr("Updater", "FooterPleaseWait").to_string(),
            None,
        ),
        ActionPhase::RollbackChecking => (
            tr("Updater", "TitleRollback").to_string(),
            vec![tr("Updater", "BodyRollbackChecking").to_string()],
            tr("Updater", "FooterPleaseWait").to_string(),
            None,
        ),
        ActionPhase::RollbackPick {
            candidates,
            selected,
        } => {
            let mut body = Vec::with_capacity(candidates.len() + 1);
            body.push(
                tr_fmt(
                    "Updater",
                    "BodyRollbackCurrent",
                    &[("version", &deadsync_version::current_tag())],
                )
                .to_string(),
            );
            for (i, info) in candidates.iter().enumerate() {
                let key = if i == *selected {
                    "BodyRollbackRowSelected"
                } else {
                    "BodyRollbackRow"
                };
                let mut row = tr_fmt("Updater", key, &[("version", &info.tag)]).to_string();
                if let Some(date) = format_published_at(info.published_at.as_deref()) {
                    row.push(' ');
                    row.push_str(&tr_fmt(
                        "Updater",
                        "BodyRollbackRowDate",
                        &[("date", &date)],
                    ));
                }
                body.push(row);
            }
            (
                tr("Updater", "TitleRollback").to_string(),
                body,
                tr("Updater", "FooterRollbackPick").to_string(),
                None,
            )
        }
        ActionPhase::RollbackEmpty => (
            tr("Updater", "TitleRollback").to_string(),
            vec![tr("Updater", "BodyRollbackEmpty").to_string()],
            tr("Updater", "FooterDismiss").to_string(),
            None,
        ),
        ActionPhase::ConfirmDownload { info, asset } => {
            let mut body = Vec::with_capacity(5);
            body.push(
                tr_fmt(
                    "Updater",
                    "BodyCurrent",
                    &[("version", &deadsync_version::current_tag())],
                )
                .to_string(),
            );
            body.push(tr_fmt("Updater", "BodyLatest", &[("version", &info.tag)]).to_string());
            body.push(
                tr_fmt("Updater", "BodySize", &[("size", &format_size(asset.size))]).to_string(),
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
                Some(t) if *t > 0 => {
                    vec![format!("{} / {}", format_size(*written), format_size(*t))]
                }
                _ => vec![format_size(*written)],
            };
            if let Some(secs) = eta_secs {
                body.push(tr("Updater", "BodyEtaShort").replace("{time}", &format_eta(*secs)));
            }
            let progress = total.and_then(|t| (t > 0).then_some(*written as f32 / t as f32));
            (
                tr("Updater", "TitleDownloading").to_string(),
                body,
                tr("Updater", "FooterPleaseWait").to_string(),
                progress.or(Some(0.0)),
            )
        }
        ActionPhase::Ready { info: _info } => (
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
        ActionPhase::AppliedRestartRequired {
            info: _info,
            detail,
        } => (
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

pub(crate) fn phase_version_tag(phase: &ActionPhase) -> Option<String> {
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

pub(crate) fn footer_frames(footer: String) -> ([TextContent; 4], bool) {
    let Some(stripped) = footer.strip_suffix('…') else {
        let text = retained_string(footer);
        return (std::array::from_fn(|_| text.clone()), false);
    };
    const DOTS: [&str; 4] = ["   ", ".  ", ".. ", "..."];
    (
        std::array::from_fn(|index| retained_string(format!("{stripped}{}", DOTS[index]))),
        true,
    )
}

pub(crate) fn retained_string(value: String) -> TextContent {
    TextContent::inline_str(&value).unwrap_or_else(|| TextContent::Shared(Arc::from(value)))
}

impl PanelContent {
    fn into_current(self) -> super::PanelContent {
        super::PanelContent {
            title: self.title,
            version_tag: self.version_tag,
            body_lines: self.body_lines,
            footer_frames: self.footer_frames,
            footer_animated: self.footer_animated,
            progress: self.progress.map(|p| super::PanelProgress {
                fraction: p.fraction,
                label: p.label,
            }),
            show_spinner: self.show_spinner,
        }
    }
}

pub(crate) fn assert_same(original: Option<PanelContent>, current: Option<super::PanelContent>) {
    let (mut original, mut current) = match (original, current) {
        (None, None) => return,
        (Some(original), Some(current)) => (original.into_current(), current),
        _ => panic!("panel visibility changed"),
    };
    assert_eq!(original.title.as_str(), current.title.as_str());
    assert_eq!(
        original.version_tag.as_ref().map(TextContent::as_str),
        current.version_tag.as_ref().map(TextContent::as_str)
    );
    assert_eq!(
        original
            .body_lines
            .iter()
            .map(TextContent::as_str)
            .collect::<Vec<_>>(),
        current
            .body_lines
            .iter()
            .map(TextContent::as_str)
            .collect::<Vec<_>>()
    );
    assert_eq!(original.footer_animated, current.footer_animated);
    assert_eq!(original.show_spinner, current.show_spinner);
    assert_eq!(
        original
            .progress
            .as_ref()
            .map(|p| (p.fraction.to_bits(), p.label.as_str())),
        current
            .progress
            .as_ref()
            .map(|p| (p.fraction.to_bits(), p.label.as_str()))
    );
    // Freeze only wall-clock selections. Compare every prepared footer frame
    // and every deterministic actor field; the spinner-enable flag is checked above.
    original.footer_animated = false;
    current.footer_animated = false;
    original.show_spinner = false;
    current.show_spinner = false;
    for frame in 0..4 {
        assert_eq!(
            original.footer_frames[frame].as_str(),
            current.footer_frames[frame].as_str()
        );
        original.footer_frames[0] = original.footer_frames[frame].clone();
        current.footer_frames[0] = current.footer_frames[frame].clone();
        for color in [0, 5] {
            let mut before = Vec::new();
            let mut after = Vec::new();
            super::push_panel(&mut before, &original, color);
            super::push_panel(&mut after, &current, color);
            crate::overlay_support::assert_actors_equal(before, after);
        }
    }
}
