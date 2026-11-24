use super::*;
use crate::views::{GameplayPolicyView, SimplyLoveVisualPolicyView};
use deadsync_assets::gameplay_presentation::{
    GameplayBackgroundMedia, GameplayPresentationInit, GameplayScreen, GameplayTextPrewarm,
    PracticeGameplayHud,
};
use deadsync_config::app_config::Config;
use deadsync_theme::views::{GameplayChatLayout, GameplayLivePresentation, GameplayStageView};

struct SimplyLoveGameplay {
    presentation: GameplayPresentation,
    visual_policy: SimplyLoveVisualPolicyView,
}

impl crate::SimplyLoveTheme {
    pub fn mount_gameplay(
        stage: &ActiveGameplayStage,
        assets: GameplayStageAssets,
        view: &GameplayStageView,
        config: &Config,
        init: Option<GameplayPresentationInit>,
        scores: crate::views::GameplayScoreInitView,
    ) -> impl GameplayScreen<GameplayLobbySession, GameplayHud<Actor>>
    + GameplayLivePresentation<GameplayCoreState>
    + use<> {
        let init = init.map_or_else(GameplayInitView::default, |init| GameplayInitView {
            runtime: GameplayRuntimeView {
                policy: GameplayPolicyView::from_config(config, init.scorebox_pane_filter),
                play_style: init.hud.play_style,
                player_side: init.hud.player_side,
                joined: [init.hud.p1.joined, init.hud.p2.joined],
            },
            hud: init.hud,
            judgment_palettes: init.judgment_palettes,
            scores: Default::default(),
        });
        SimplyLoveGameplay {
            presentation: Self::build_gameplay_presentation(stage, assets, view, init, scores),
            visual_policy: SimplyLoveVisualPolicyView::from_config(config),
        }
    }
}

impl GameplayStagePresenter<GameplayLobbySession> for SimplyLoveGameplay {
    type View = ActorViewOverride;
    type Policy = ();

    fn compose_stage_frame(
        &mut self,
        stage: &ActiveGameplayStage,
        lobby: &GameplayLobbySession,
        song: &mut deadsync_song_lua::playback::FrameScratch,
        actors: &mut Vec<Actor>,
        context: FramePresentationContext<'_, Self::View, Self::Policy>,
        content: SongFrameContent,
    ) -> deadsync_song_lua::playback::GameplayActorSegments {
        self.presentation.compose_frame(
            stage,
            lobby,
            song,
            actors,
            FramePresentationContext {
                assets: context.assets,
                view: context.view,
                uptime_seconds: context.uptime_seconds,
                policy: self.visual_policy,
            },
            content,
        )
    }

    fn append_song_lua_exit(&self, actors: &mut Vec<Actor>, fade_progress: f32) {
        self.presentation
            .append_song_lua_exit(actors, fade_progress);
    }
}

impl GameplayLivePresentation<GameplayCoreState> for SimplyLoveGameplay {
    fn update_gameplay_visuals(&mut self, game: &GameplayCoreState, delta_time: f32) {
        self.presentation.update_gameplay_visuals(game, delta_time);
    }

    fn sync_leaderboards(
        &mut self,
        updates: [Option<score_data::CachedPlayerLeaderboardData>; MAX_PLAYERS],
    ) {
        self.presentation.sync_leaderboards(updates);
    }

    fn sync_itl_cmod_warning(&mut self, warnings: [bool; MAX_PLAYERS]) {
        self.presentation.sync_itl_cmod_warning(warnings);
    }

    fn toggle_lobby_hud(&mut self) {
        self.presentation.toggle_lobby_hud();
    }

    fn sync_smx_sensors(&mut self, views: [Option<SmxSensorPadView>; MAX_PLAYERS]) {
        self.presentation.sync_smx_sensors(views);
    }

    fn sync_heart_rate(&mut self, generation: (u64, u64), view: HeartRateView) {
        self.presentation.sync_heart_rate(generation, view);
    }
}

impl GameplayScreen<GameplayLobbySession, GameplayHud<Actor>> for SimplyLoveGameplay {
    fn background(&self) -> &GameplayBackgroundMedia {
        &self.presentation.background.media
    }
    fn background_mut(&mut self) -> &mut GameplayBackgroundMedia {
        &mut self.presentation.background.media
    }
    fn hide_song_intro(&self) -> bool {
        self.presentation.hide_song_intro
    }
    fn set_hide_song_intro(&mut self, hide: bool) {
        self.presentation.hide_song_intro = hide;
    }
    fn pack_banner_path(&self) -> Option<&Path> {
        self.presentation.pack_banner_path.as_deref()
    }
    fn replay_status_text(&self) -> Option<&str> {
        self.presentation.replay_status_text.as_deref()
    }

    fn visible_banner_paths<'a>(&'a self, stage: &'a ActiveGameplayStage) -> [Option<&'a Path>; 2] {
        visible_banner_paths_for(stage, &self.presentation)
    }

    fn chat_layout(
        &self,
        stage: &ActiveGameplayStage,
        lobby: &GameplayLobbySession,
    ) -> Option<GameplayChatLayout> {
        Some(twitch_chat_layout(&GameplayPresenter {
            stage,
            lobby,
            presentation: &self.presentation,
        }))
    }

    fn begin_background_transition(
        &mut self,
        previous: Arc<str>,
        transition: &str,
        start_time: f32,
    ) {
        self.presentation
            .background
            .begin_transition(previous, transition, start_time);
    }

    fn install_hud(&mut self, hud: GameplayHud<Actor>) {
        self.presentation.set_hud(hud);
    }
    fn has_hud(&self) -> bool {
        self.presentation.has_hud()
    }

    fn refresh_frame(&mut self, stage: &mut ActiveGameplayStage, config: &Config) {
        self.visual_policy = SimplyLoveVisualPolicyView::from_config(config);
        gameplay_stats::refresh_density_graph_meshes(
            &mut stage.gameplay,
            &mut self.presentation.density_graph,
        );
    }

    fn prewarm_assets(&mut self) {
        self.presentation
            .prewarm_combo_milestones(self.visual_policy);
    }

    fn prewarm_text(
        &self,
        stage: &ActiveGameplayStage,
        lobby: &GameplayLobbySession,
        prewarm: GameplayTextPrewarm<'_>,
    ) {
        let state = GameplayPresenter {
            stage,
            lobby,
            presentation: &self.presentation,
        };
        let fonts = prewarm.assets.fonts();
        prewarm_text_layout(prewarm.cache, prewarm.scratch, fonts, &state);
        gameplay_stats::prewarm_text_layout(prewarm.cache, fonts, prewarm.assets, &state);
        gameplay_stats::prewarm_frame_text_scratch(prewarm.cache, prewarm.scratch, fonts, &state);
        prewarm_compose_storage(prewarm.scratch, &state);
        deadsync_assets::gameplay_presentation::field::prewarm_text(
            prewarm,
            &stage.gameplay,
            self.presentation.notefield_plans.each_ref(),
        );
    }

    fn entry(
        &self,
        stage: &ActiveGameplayStage,
        lobby: &GameplayLobbySession,
        assets: &AssetManager,
        restarting: bool,
        config: &Config,
    ) -> (Vec<Actor>, f32) {
        in_transition_for(
            Some(&GameplayPresenter {
                stage,
                lobby,
                presentation: &self.presentation,
            }),
            assets,
            restarting,
            SimplyLoveVisualPolicyView::from_config(config),
        )
    }
}

impl PracticeGameplayHud for SimplyLoveGameplay {
    fn machine_font(&self) -> deadsync_config::theme::MachineFont {
        self.presentation.machine_font()
    }

    fn set_practice_timing_labels(
        &mut self,
        labels: [Box<[deadsync_notefield::TimingSegmentLabel]>; MAX_PLAYERS],
    ) {
        self.presentation.set_practice_timing_labels(labels);
    }

    fn music_rate_changed(&mut self, stage: &ActiveGameplayStage) {
        sync_music_rate_text(stage, &mut self.presentation);
    }
}
