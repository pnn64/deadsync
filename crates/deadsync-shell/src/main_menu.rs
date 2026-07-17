use deadsync_online::arrowcloud::{
    ConnectionError as ArrowCloudError, ConnectionStatus as ArrowCloudStatus,
};
use deadsync_online::groovestats::{
    ConnectionError as GrooveError, ConnectionStatus as GrooveStatus,
};
use deadsync_theme_simply_love::views::{
    MainMenuArrowCloudError, MainMenuArrowCloudStatus, MainMenuGrooveError, MainMenuGrooveStatus,
    MainMenuRuntimeView, MainMenuSmxConflictView,
};

fn groove_status(boogie: bool, status: GrooveStatus) -> MainMenuGrooveStatus {
    match status {
        GrooveStatus::Pending => MainMenuGrooveStatus::Pending { boogie },
        GrooveStatus::Error(kind) => MainMenuGrooveStatus::Error {
            boogie,
            kind: match kind {
                GrooveError::Disabled => MainMenuGrooveError::Disabled,
                GrooveError::MachineOffline => MainMenuGrooveError::MachineOffline,
                GrooveError::CannotConnect => MainMenuGrooveError::CannotConnect,
                GrooveError::TimedOut => MainMenuGrooveError::TimedOut,
                GrooveError::InvalidResponse => MainMenuGrooveError::InvalidResponse,
            },
        },
        GrooveStatus::Connected(services) => MainMenuGrooveStatus::Connected {
            boogie,
            get_scores: services.get_scores,
            leaderboard: services.leaderboard,
            auto_submit: services.auto_submit,
        },
    }
}

fn arrowcloud_status(status: ArrowCloudStatus) -> MainMenuArrowCloudStatus {
    match status {
        ArrowCloudStatus::Pending => MainMenuArrowCloudStatus::Pending,
        ArrowCloudStatus::Connected => MainMenuArrowCloudStatus::Connected,
        ArrowCloudStatus::Error(kind) => MainMenuArrowCloudStatus::Error(match kind {
            ArrowCloudError::Disabled => MainMenuArrowCloudError::Disabled,
            ArrowCloudError::TimedOut => MainMenuArrowCloudError::TimedOut,
            ArrowCloudError::HostBlocked => MainMenuArrowCloudError::HostBlocked,
            ArrowCloudError::CannotConnect => MainMenuArrowCloudError::CannotConnect,
        }),
    }
}

pub(crate) fn runtime_view() -> MainMenuRuntimeView {
    let (allow_shutdown_host, dedicated_three_key_nav, smx_input) = {
        let config = deadsync_config::prelude::get();
        (
            config.allow_shutdown_host,
            config.three_key_navigation && config.only_dedicated_menu_buttons,
            config.smx_input,
        )
    };
    let (pack_count, song_count) = {
        let song_cache = deadsync_simfile::runtime_cache::get_song_cache();
        (
            song_cache.len(),
            song_cache.iter().map(|pack| pack.songs.len()).sum(),
        )
    };
    let course_count = deadsync_simfile::runtime_cache::get_course_cache().len();
    let boogie = deadsync_online::runtime::is_boogiestats_active();
    let smx_conflict =
        (smx_input && deadsync_smx::conflict_warning_active()).then_some(MainMenuSmxConflictView {
            color_rgb: deadsync_smx::CONFLICT_WARNING_RGB,
        });

    MainMenuRuntimeView {
        allow_shutdown_host,
        dedicated_three_key_nav,
        song_count,
        pack_count,
        course_count,
        groovestats: groove_status(boogie, deadsync_online::groovestats::runtime_get_status()),
        arrowcloud: arrowcloud_status(deadsync_online::arrowcloud::runtime_get_status()),
        smx_conflict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_online::groovestats::Services;

    #[test]
    fn groove_status_preserves_service_and_capabilities() {
        assert_eq!(
            groove_status(
                true,
                GrooveStatus::Connected(Services {
                    get_scores: true,
                    leaderboard: false,
                    auto_submit: true,
                }),
            ),
            MainMenuGrooveStatus::Connected {
                boogie: true,
                get_scores: true,
                leaderboard: false,
                auto_submit: true,
            }
        );
        assert_eq!(
            groove_status(false, GrooveStatus::Error(GrooveError::TimedOut)),
            MainMenuGrooveStatus::Error {
                boogie: false,
                kind: MainMenuGrooveError::TimedOut,
            }
        );
    }

    #[test]
    fn arrowcloud_status_preserves_error_kind() {
        assert_eq!(
            arrowcloud_status(ArrowCloudStatus::Error(ArrowCloudError::HostBlocked)),
            MainMenuArrowCloudStatus::Error(MainMenuArrowCloudError::HostBlocked)
        );
    }
}
