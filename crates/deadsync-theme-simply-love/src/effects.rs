use deadsync_theme::AudioRequest;

pub use deadsync_theme::app_requests::{
    AdvancedConfigRequest as SimplyLoveAdvancedConfigRequest,
    CoinConfigRequest as SimplyLoveCoinConfigRequest,
    ConfigRequest as SimplyLoveConfigRequest,
    ContentRequest as SimplyLoveContentRequest,
    CourseConfigRequest as SimplyLoveCourseConfigRequest,
    DebugRequest as SimplyLoveDebugRequest,
    AppEffect as SimplyLoveEffect,
    EffectRouteContext as SimplyLoveEffectRouteContext,
    EffectRoutePlan as SimplyLoveEffectRoutePlan,
    GameplayConfigRequest as SimplyLoveGameplayConfigRequest,
    GameplayPadLights as SimplyLoveGameplayPadLights,
    GraphOrientation as SimplyLoveGraphOrientation,
    GraphOrigin as SimplyLoveGraphOrigin,
    HardwareRequest as SimplyLoveHardwareRequest,
    AppInputResult as SimplyLoveInputResult,
    ItgImportSummary as SimplyLoveItgImportSummary,
    ItgProfileCandidate as SimplyLoveItgProfileCandidate,
    LightsConfigRequest as SimplyLoveLightsConfigRequest,
    LightsDriver as SimplyLoveLightsDriver,
    LobbyRequest as SimplyLoveLobbyRequest,
    LocalProfileEvent as SimplyLoveLocalProfileEvent,
    MachineConfigRequest as SimplyLoveMachineConfigRequest,
    MappingsConfigRequest as SimplyLoveMappingsConfigRequest,
    MediaRequest as SimplyLoveMediaRequest,
    NullOrDieConfigRequest as SimplyLoveNullOrDieConfigRequest,
    NullOrDieGraph as SimplyLoveNullOrDieGraph,
    OnlineConfigRequest as SimplyLoveOnlineConfigRequest,
    OnlineRequest as SimplyLoveOnlineRequest,
    OptionsConfigRequest as SimplyLoveOptionsConfigRequest,
    ProfileImportEvent as SimplyLoveProfileImportEvent,
    ProfileRequest as SimplyLoveProfileRequest,
    QrLoginEvent as SimplyLoveQrLoginEvent,
    QrLoginPolicy as SimplyLoveQrLoginPolicy,
    QrLoginRequest as SimplyLoveQrLoginRequest,
    QrLoginService as SimplyLoveQrLoginService,
    QrLoginSlot as SimplyLoveQrLoginSlot,
    QrLoginSlotAvailability as SimplyLoveQrLoginSlotAvailability,
    RuntimeRequest as SimplyLoveRuntimeRequest,
    ScoreImportEvent as SimplyLoveScoreImportEvent,
    ScoreImportProfile as SimplyLoveScoreImportProfile,
    ScoreImportProgress as SimplyLoveScoreImportProgress,
    ScoreImportRequest as SimplyLoveScoreImportRequest,
    ScoreImportSummary as SimplyLoveScoreImportSummary,
    SelectMusicConfigRequest as SimplyLoveSelectMusicConfigRequest,
    SongSearchRequest as SimplyLoveSongSearchRequest,
    SongSearchResult as SimplyLoveSongSearchResult,
    SrpgShopFolder as SimplyLoveSrpgShopFolder,
    SyncEvent as SimplyLoveSyncEvent,
    SyncKernel as SimplyLoveSyncKernel,
    SyncKernelTarget as SimplyLoveSyncKernelTarget,
    SyncOwner as SimplyLoveSyncOwner,
    SyncPlotView as SimplyLoveSyncPlotView,
    SyncRequest as SimplyLoveSyncRequest,
    SyncResult as SimplyLoveSyncResult,
    SyncSongResult as SimplyLoveSyncSongResult,
    SyncStreamEvent as SimplyLoveSyncStreamEvent,
    SyncTarget as SimplyLoveSyncTarget,
    TournamentConfigRequest as SimplyLoveTournamentConfigRequest,
    UpdaterRequest as SimplyLoveUpdaterRequest,
    effect_for_gameplay_exit, gameplay_action_for_effect, resolve_effect_route,
};

pub(crate) const fn sfx(path: &'static str) -> SimplyLoveEffect {
    SimplyLoveEffect::Runtime(SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(path)))
}

pub(crate) fn sfx_then(path: &'static str, effect: SimplyLoveEffect) -> SimplyLoveEffect {
    SimplyLoveEffect::sequence(sfx(path), effect)
}

pub(crate) const fn lobby(request: SimplyLoveLobbyRequest) -> SimplyLoveEffect {
    SimplyLoveEffect::Runtime(SimplyLoveRuntimeRequest::Online(
        SimplyLoveOnlineRequest::Lobby(request),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_theme::Screen;

    #[test]
    fn sfx_then_preserves_audio_before_follow_up_effect() {
        let effect = sfx_then(
            "assets/sounds/start.ogg",
            SimplyLoveEffect::Navigate(Screen::SelectStyle),
        );
        let SimplyLoveEffect::Batch(effects) = effect else {
            panic!("expected batch effect");
        };
        assert_eq!(effects.len(), 2);
        assert!(matches!(
            &effects[0],
            SimplyLoveEffect::Runtime(SimplyLoveRuntimeRequest::Audio(
                AudioRequest::PlaySfx(path)
            )) if *path == "assets/sounds/start.ogg"
        ));
        assert!(matches!(
            effects[1],
            SimplyLoveEffect::Navigate(Screen::SelectStyle)
        ));
    }

    #[test]
    fn single_lobby_request_stays_unbatched() {
        let effect = SimplyLoveEffect::sequence(
            SimplyLoveEffect::None,
            lobby(SimplyLoveLobbyRequest::UpdateMachineStats {
                screen_name: "ScreenGameplay",
                p1_ready: true,
                p2_ready: false,
                p1_stats: None,
                p2_stats: None,
            }),
        );
        assert!(matches!(
            effect,
            SimplyLoveEffect::Runtime(SimplyLoveRuntimeRequest::Online(
                SimplyLoveOnlineRequest::Lobby(SimplyLoveLobbyRequest::UpdateMachineStats {
                    screen_name: "ScreenGameplay",
                    p1_ready: true,
                    p2_ready: false,
                    ..
                })
            ))
        ));
    }
}
