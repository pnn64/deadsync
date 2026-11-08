use super::*;
use crate::screens::components::shared::update_overlay::perf_original::PanelContent;
pub(crate) fn prepare(phase: &FfmpegPhase) -> Option<PanelContent> {
    if matches!(phase, FfmpegPhase::Idle) {
        return None;
    }
    Some(panel_content(phase))
}

pub(crate) fn panel_content(phase: &FfmpegPhase) -> PanelContent {
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
