use deadsync_theme_simply_love::SimplyLoveUpdaterRequest;
use deadsync_theme_simply_love::views::{
    SimplyLoveFfmpegPhase, SimplyLoveReleaseAssetView, SimplyLoveReleaseView,
    SimplyLoveUpdateErrorKind, SimplyLoveUpdatePhase, SimplyLoveUpdaterCapabilities,
    SimplyLoveUpdaterView,
};
use deadsync_updater::action::{self, ActionErrorKind, ActionPhase};
use deadsync_updater::ffmpeg::{self, FfmpegPhase};

pub(super) fn capabilities() -> SimplyLoveUpdaterCapabilities {
    SimplyLoveUpdaterCapabilities {
        app_update: deadsync_updater::apply_supported_for_host(),
        ffmpeg_install: ffmpeg::install_supported_for_host(),
    }
}

pub(super) fn available_update_tag() -> Option<String> {
    match deadsync_updater::state::snapshot()? {
        deadsync_updater::UpdateState::Available(info) => Some(info.tag),
        deadsync_updater::UpdateState::UpToDate | deadsync_updater::UpdateState::UnknownLatest => {
            None
        }
    }
}

pub(super) fn view() -> SimplyLoveUpdaterView {
    SimplyLoveUpdaterView {
        update: update_phase(action::current()),
        ffmpeg: ffmpeg_phase(ffmpeg::current()),
    }
}

pub(super) fn execute(request: SimplyLoveUpdaterRequest) {
    match request {
        SimplyLoveUpdaterRequest::CheckForUpdates => action::request_check_now(),
        SimplyLoveUpdaterRequest::CheckForRollback => action::request_rollback_check(),
        SimplyLoveUpdaterRequest::DownloadUpdate => action::request_download(),
        SimplyLoveUpdaterRequest::ApplyUpdate => action::request_apply(),
        SimplyLoveUpdaterRequest::DismissUpdate => action::dismiss(),
        SimplyLoveUpdaterRequest::CancelUpdate => action::request_cancel(),
        SimplyLoveUpdaterRequest::MoveRollback(delta) => action::rollback_move(delta),
        SimplyLoveUpdaterRequest::ConfirmRollback => action::request_rollback_confirm(),
        SimplyLoveUpdaterRequest::CheckFfmpegAvailability => check_ffmpeg_availability(),
        SimplyLoveUpdaterRequest::ConfirmFfmpegInstall => ffmpeg::request_confirm(),
        SimplyLoveUpdaterRequest::DismissFfmpeg => ffmpeg::dismiss(),
        SimplyLoveUpdaterRequest::CancelFfmpegCheck => ffmpeg::cancel_check(),
        SimplyLoveUpdaterRequest::CancelFfmpegDownload => ffmpeg::request_cancel(),
    }
}

fn check_ffmpeg_availability() {
    let Some(generation) = ffmpeg::begin_availability_check() else {
        return;
    };
    std::thread::spawn(move || {
        let available = deadlib_video::ffmpeg_available();
        ffmpeg::resolve_availability_check(generation, available);
    });
}

fn update_phase(phase: ActionPhase) -> SimplyLoveUpdatePhase {
    match phase {
        ActionPhase::Idle => SimplyLoveUpdatePhase::Idle,
        ActionPhase::Checking => SimplyLoveUpdatePhase::Checking,
        ActionPhase::ConfirmDownload { info, asset } => SimplyLoveUpdatePhase::ConfirmDownload {
            info: release_view(info),
            asset: asset_view(asset),
        },
        ActionPhase::UpToDate { tag } => SimplyLoveUpdatePhase::UpToDate { tag },
        ActionPhase::RollbackChecking => SimplyLoveUpdatePhase::RollbackChecking,
        ActionPhase::RollbackPick {
            candidates,
            selected,
        } => SimplyLoveUpdatePhase::RollbackPick {
            candidates: candidates
                .into_iter()
                .map(|(info, _)| release_view(info))
                .collect(),
            selected,
        },
        ActionPhase::RollbackEmpty => SimplyLoveUpdatePhase::RollbackEmpty,
        ActionPhase::AvailableNoInstall { info } => SimplyLoveUpdatePhase::AvailableNoInstall {
            info: release_view(info),
        },
        ActionPhase::Downloading {
            info,
            written,
            total,
            eta_secs,
            ..
        } => SimplyLoveUpdatePhase::Downloading {
            info: release_view(info),
            written,
            total,
            eta_secs,
        },
        ActionPhase::Ready { info, .. } => SimplyLoveUpdatePhase::Ready {
            info: release_view(info),
        },
        ActionPhase::Applying { info } => SimplyLoveUpdatePhase::Applying {
            info: release_view(info),
        },
        ActionPhase::AppliedRestartRequired { info, detail } => {
            SimplyLoveUpdatePhase::AppliedRestartRequired {
                info: release_view(info),
                detail,
            }
        }
        ActionPhase::Error { kind, detail } => SimplyLoveUpdatePhase::Error {
            kind: error_kind(kind),
            detail,
        },
    }
}

fn ffmpeg_phase(phase: FfmpegPhase) -> SimplyLoveFfmpegPhase {
    match phase {
        FfmpegPhase::Idle => SimplyLoveFfmpegPhase::Idle,
        FfmpegPhase::Checking => SimplyLoveFfmpegPhase::Checking,
        FfmpegPhase::Confirm {
            version,
            origin,
            total,
            already_available,
        } => SimplyLoveFfmpegPhase::Confirm {
            version,
            origin,
            total,
            already_available,
        },
        FfmpegPhase::Downloading {
            version,
            written,
            total,
            eta_secs,
            speed_bps,
        } => SimplyLoveFfmpegPhase::Downloading {
            version,
            written,
            total,
            eta_secs,
            speed_bps,
        },
        FfmpegPhase::Extracting { version } => SimplyLoveFfmpegPhase::Extracting { version },
        FfmpegPhase::Installed { version } => SimplyLoveFfmpegPhase::Installed { version },
        FfmpegPhase::Unsupported => SimplyLoveFfmpegPhase::Unsupported,
        FfmpegPhase::AlreadyAvailable => SimplyLoveFfmpegPhase::AlreadyAvailable,
        FfmpegPhase::Error { kind, detail } => SimplyLoveFfmpegPhase::Error {
            kind: error_kind(kind),
            detail,
        },
    }
}

fn release_view(info: deadsync_updater::ReleaseInfo) -> SimplyLoveReleaseView {
    SimplyLoveReleaseView {
        tag: info.tag,
        html_url: info.html_url,
        published_at: info.published_at,
    }
}

fn asset_view(asset: deadsync_updater::ReleaseAsset) -> SimplyLoveReleaseAssetView {
    SimplyLoveReleaseAssetView {
        size: asset.size,
        digest: asset.digest,
    }
}

const fn error_kind(kind: ActionErrorKind) -> SimplyLoveUpdateErrorKind {
    match kind {
        ActionErrorKind::Network => SimplyLoveUpdateErrorKind::Network,
        ActionErrorKind::RateLimited => SimplyLoveUpdateErrorKind::RateLimited,
        ActionErrorKind::HttpStatus => SimplyLoveUpdateErrorKind::HttpStatus,
        ActionErrorKind::Parse => SimplyLoveUpdateErrorKind::Parse,
        ActionErrorKind::NoAssetForHost => SimplyLoveUpdateErrorKind::NoAssetForHost,
        ActionErrorKind::Checksum => SimplyLoveUpdateErrorKind::Checksum,
        ActionErrorKind::Io => SimplyLoveUpdateErrorKind::Io,
    }
}
