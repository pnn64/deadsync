//! OpenITG's gameplay screen, backed by the shared stage compositor.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use deadlib_assets::AssetManager;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::color::Color;
use deadlib_present::dsl::SpriteBuilder;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_assets::gameplay_presentation::{
    GameplayBackgroundMedia, GameplayPresentationInit, GameplayRenderView, GameplayScreen,
    GameplayTextPrewarm, PracticeGameplayHud,
    background::{GameplayBackground, push_background},
    display_mods::{self, DisplayModsFrame},
    field::{self, GameplayNotefieldPlan, ResolvedJudgmentAssets},
    field_style::{DEFAULT_GAMEPLAY_HUD_STYLE, notefield_style_with_palette},
    life::{LifeMeterVisual, visible_life},
    text_slots::FRAME_TEXT_COUNTDOWN_BASE,
};
use deadsync_assets::gameplay_stage::{
    ActiveGameplayStage, GameplayCoreState, GameplayStageAssets, GameplayStagePresenter,
};
use deadsync_assets::noteskin::SpriteSlot;
use deadsync_config::{app_config::Config, theme::MachineFont};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    GAMEPLAY_ENTRY_TRANSITION_SECONDS, GAMEPLAY_RESTART_TRANSITION_SECONDS,
    GameplayInputPlayerSide, HoldToExitKey, display_ex_score_percent_for_mode,
    exit_transition_alpha,
};
use deadsync_notefield::{
    BrokenRunLookup, NotefieldComposeRequest, NotefieldHudComposeResult, PreparedNotefield,
    StreamProgressLookup,
};
use deadsync_online::gameplay_lobby::GameplayLobbySession;
use deadsync_profile::{GameplayHudSnapshot, Profile};
use deadsync_profile_gameplay::{
    GameplayProfile, score_display_mode_from_profile, stage_scores::GameplayScoreSnapshots,
};
use deadsync_song_lua::gameplay_presentation::{
    FieldHudBuffers, FieldPresentation, FramePresentationContext, GameplayFrameBuffers,
    GameplayFramePresenter, SongFrameContent, compose_stage_frame,
};
use deadsync_song_lua::playback::{
    FieldFrame, FieldLayout, FrameScratch, GameplayActorSegments, ScreenLayer, song_meter_progress,
};
use deadsync_theme::color::{JudgmentColorRole, JudgmentPalette};
use deadsync_theme::views::{
    GameplayBackgroundWarningView, GameplayChatLayout, GameplayDifficultyView, GameplayHud,
    GameplayHudMotion, GameplayLifeView, GameplayLivePresentation, GameplayScoreView,
    GameplaySongMeterView, GameplayStageBadgeView, GameplayStageIntro, GameplayStageView,
};

use crate::gameplay::GAMEPLAY_HUD;
use crate::presentation::Layout;
use crate::resources::MENU_NORMAL_FONT;

struct PlayerField {
    judgment: ResolvedJudgmentAssets,
    plan: GameplayNotefieldPlan,
    broken_runs: BrokenRunLookup,
    streams: StreamProgressLookup,
    width: f32,
    display_mods: Arc<str>,
}

struct OpenItgGameplay {
    hud: GameplayHud<Actor>,
    background: GameplayBackground,
    hide_intro: bool,
    title: Arc<str>,
    round: Option<usize>,
    badge: Arc<str>,
    rate: Arc<str>,
    replay_status: Option<Arc<str>>,
    pack_banner: Option<PathBuf>,
    profiles: Option<GameplayHudSnapshot>,
    center_single: bool,
    background_brightness: f32,
    background_color: Color,
    machine_font: MachineFont,
    palettes: Option<[JudgmentPalette; MAX_PLAYERS]>,
    fields: [PlayerField; MAX_PLAYERS],
    life: [LifeMeterVisual; MAX_PLAYERS],
    warning_seconds: [f32; MAX_PLAYERS],
    cmod_warning: [bool; MAX_PLAYERS],
    lobby_hud_visible: bool,
}

impl crate::OpenItgTheme {
    pub fn mount_gameplay(
        stage: &ActiveGameplayStage,
        assets: GameplayStageAssets,
        view: &GameplayStageView,
        config: &Config,
        init: Option<GameplayPresentationInit>,
        _scores: GameplayScoreSnapshots,
    ) -> impl GameplayScreen<GameplayLobbySession, GameplayHud<Actor>>
    + GameplayLivePresentation<GameplayCoreState>
    + use<> {
        let palettes = init.as_ref().map(|init| init.judgment_palettes);
        let center_single = init.is_some() && config.center_1player_notefield;
        let profiles = std::array::from_fn(|player| &stage.profiles()[player].0);
        let widths = field::gameplay_notefield_widths(&stage.gameplay, &stage.noteskin_assets);
        let fields = std::array::from_fn(|player| {
            let profile = profiles[player];
            let judgment = ResolvedJudgmentAssets::from_profile(profile);
            judgment.prewarm();
            let style = palettes.map_or(DEFAULT_GAMEPLAY_HUD_STYLE, |palettes| {
                notefield_style_with_palette(palettes[player])
            });
            let mut plan = field::gameplay_notefield_plan(
                profile,
                &judgment,
                stage.player_blue_window_ms(player) / 1000.0,
                style,
            );
            plan.install_feedback(
                GAMEPLAY_HUD.field_feedback,
                GAMEPLAY_HUD.default_tap_atlas,
                GAMEPLAY_HUD.default_hold_atlas,
            );
            PlayerField {
                judgment,
                plan,
                broken_runs: BrokenRunLookup::new(stage.measure_counter_segments(player)),
                streams: StreamProgressLookup::new(stage.mini_indicator_stream_segments(player)),
                width: widths[player],
                display_mods: field::preferred_mods_text(&stage.gameplay, player),
            }
        });
        field::prewarm_hud_actor_resources(stage.actor_resources(), profiles, stage.num_players());
        let (round, badge) = match view.intro {
            GameplayStageIntro::Song { round } => (round, Arc::from("EVENT")),
            GameplayStageIntro::Course { number, .. } => {
                (Some(number), Arc::from(format!("STAGE {number}")))
            }
            GameplayStageIntro::Restart(count) => (None, Arc::from(format!("RESTART {count}"))),
            GameplayStageIntro::Demonstration | GameplayStageIntro::Practice => {
                (None, Arc::from(""))
            }
        };
        let replay_status = view.replay.as_ref().map(|replay| {
            Arc::from(format!(
                "Autoplay - {} {:.2}%",
                replay.name,
                replay.score_10000 / 100.0
            ))
        });
        let mut screen = OpenItgGameplay {
            hud: GAMEPLAY_HUD,
            background: GameplayBackground::new(stage),
            hide_intro: assets.hide_song_intro,
            title: Arc::from(stage.song().display_full_title(config.translated_titles)),
            round,
            badge,
            rate: Arc::from(""),
            replay_status,
            pack_banner: view
                .course
                .as_ref()
                .map_or(assets.pack_banner_path, |course| course.banner_path.clone()),
            profiles: init.map(|init| init.hud),
            center_single,
            background_brightness: config.bg_brightness,
            background_color: config.gameplay_bg_color,
            machine_font: config.machine_font,
            palettes,
            fields,
            life: stage.players().each_ref().map(|player| {
                LifeMeterVisual::new(player.life, player.is_failing || player.life <= 0.0)
            }),
            warning_seconds: [0.0; MAX_PLAYERS],
            cmod_warning: [false; MAX_PLAYERS],
            lobby_hud_visible: true,
        };
        screen.music_rate_changed(stage);
        screen
    }
}

impl GameplayStagePresenter<GameplayLobbySession> for OpenItgGameplay {
    type View = GameplayRenderView;
    type Policy = ();

    fn compose_stage_frame(
        &mut self,
        stage: &ActiveGameplayStage,
        lobby: &GameplayLobbySession,
        song: &mut FrameScratch,
        actors: &mut Vec<Actor>,
        context: FramePresentationContext<'_, Self::View, Self::Policy>,
        content: SongFrameContent,
    ) -> GameplayActorSegments {
        compose_stage_frame(
            actors,
            stage,
            &OpenItgFrame {
                screen: self,
                stage,
                lobby,
            },
            GameplayFrameBuffers {
                song,
                presentation: &mut (),
            },
            context,
            content,
        )
    }

    fn append_song_lua_exit(&self, actors: &mut Vec<Actor>, fade_progress: f32) {
        actors.push(black_cover(fade_progress, 1200).build(0));
    }
}

impl GameplayLivePresentation<GameplayCoreState> for OpenItgGameplay {
    fn update_gameplay_visuals(&mut self, game: &GameplayCoreState, delta_time: f32) {
        for (index, clock) in self
            .warning_seconds
            .iter_mut()
            .enumerate()
            .take(game.num_players())
        {
            let (profile, player) = (&game.profiles()[index].0, &game.players()[index]);
            let failed = player.is_failing || player.life <= 0.0;
            self.life[index].update(player.life, failed, delta_time);
            if !failed && warning_visible(profile, player.life, self.hud.danger_threshold) {
                *clock += delta_time.max(0.0);
            }
        }
    }

    fn toggle_lobby_hud(&mut self) {
        self.lobby_hud_visible = !self.lobby_hud_visible;
    }

    fn sync_itl_cmod_warning(&mut self, warnings: [bool; MAX_PLAYERS]) {
        self.cmod_warning = warnings;
    }
}

impl GameplayScreen<GameplayLobbySession, GameplayHud<Actor>> for OpenItgGameplay {
    fn background(&self) -> &GameplayBackgroundMedia {
        &self.background.media
    }
    fn background_mut(&mut self) -> &mut GameplayBackgroundMedia {
        &mut self.background.media
    }
    fn hide_song_intro(&self) -> bool {
        self.hide_intro
    }
    fn set_hide_song_intro(&mut self, hide: bool) {
        self.hide_intro = hide;
    }
    fn pack_banner_path(&self) -> Option<&Path> {
        self.pack_banner.as_deref()
    }
    fn replay_status_text(&self) -> Option<&str> {
        self.replay_status.as_deref()
    }

    fn chat_layout(
        &self,
        stage: &ActiveGameplayStage,
        _: &GameplayLobbySession,
    ) -> Option<GameplayChatLayout> {
        let style = stage.gameplay.setup.session.play_style;
        let p2 = stage.gameplay.setup.session.runtime_player_side(0) == GameplayInputPlayerSide::P2;
        let centered = style.is_double() || self.center_single;
        let direction = if p2 { 1.0 } else { -1.0 };
        let offset = if centered {
            0.0
        } else {
            screen_width().clamp(640.0, 854.0) * 0.25
        };
        let center = (offset + stage.profiles()[0].note_field_offset_x.clamp(0, 50) as f32)
            .mul_add(direction, screen_center_x());
        let half_width = self.fields[0].width * stage.field_zoom_for_player(0) * 0.5;
        Some(GameplayChatLayout {
            two_players: stage.num_players() > 1,
            p2,
            centered,
            field_left: center - half_width,
            field_right: center + half_width,
            statistics: false,
        })
    }

    fn begin_background_transition(
        &mut self,
        previous: Arc<str>,
        transition: &str,
        start_time: f32,
    ) {
        self.background
            .begin_transition(previous, transition, start_time);
    }

    fn install_hud(&mut self, hud: GameplayHud<Actor>) {
        for field in &mut self.fields {
            field.plan.install_feedback(
                hud.field_feedback,
                hud.default_tap_atlas,
                hud.default_hold_atlas,
            );
        }
        self.hud = hud;
    }

    fn prewarm_text(
        &self,
        stage: &ActiveGameplayStage,
        _: &GameplayLobbySession,
        prewarm: GameplayTextPrewarm<'_>,
    ) {
        let fonts = prewarm.assets.fonts();
        deadsync_song_lua::playback::prewarm_text_layout(prewarm.cache, fonts, &stage.gameplay);
        deadsync_assets::gameplay_presentation::prewarm_compose_storage(
            prewarm.scratch,
            stage.num_players(),
        );
        field::prewarm_text(
            prewarm,
            &stage.gameplay,
            self.fields.each_ref().map(|field| &field.plan),
        );
    }

    fn entry(
        &self,
        _: &ActiveGameplayStage,
        _: &GameplayLobbySession,
        _: &AssetManager,
        restart: bool,
        _: &Config,
    ) -> (Vec<Actor>, f32) {
        let duration = if restart {
            GAMEPLAY_RESTART_TRANSITION_SECONDS
        } else {
            GAMEPLAY_ENTRY_TRANSITION_SECONDS
        };
        if self.hide_intro || !restart {
            return (Vec::new(), duration);
        }
        let fade = deadlib_present::__act_from_builder!(
            (linear(duration): alpha(0.0)) black_cover(1.0, 1200)
        );
        (vec![fade], duration)
    }
}

impl PracticeGameplayHud for OpenItgGameplay {
    fn machine_font(&self) -> MachineFont {
        self.machine_font
    }

    fn music_rate_changed(&mut self, stage: &ActiveGameplayStage) {
        let rate = stage.music_rate();
        self.rate = if (rate - 1.0).abs() < 0.001 {
            Arc::from("")
        } else {
            let rate = format!("{rate:.2}");
            Arc::from(format!(
                "{}x",
                rate.trim_end_matches('0').trim_end_matches('.')
            ))
        };
    }
}

struct OpenItgFrame<'a> {
    screen: &'a OpenItgGameplay,
    stage: &'a ActiveGameplayStage,
    lobby: &'a GameplayLobbySession,
}

impl GameplayFramePresenter for OpenItgFrame<'_> {
    type Profile = GameplayProfile;
    type Slot = SpriteSlot;
    type View = GameplayRenderView;
    type Policy = ();
    type Scratch = ();

    fn draw_layers<'a>(
        &'a self,
        _: &'a mut (),
        context: FramePresentationContext<'a, Self::View, ()>,
    ) -> impl FnMut(ScreenLayer, &mut Vec<Actor>, FieldLayout) + 'a {
        move |layer, actors, field| self.draw(layer, actors, field, context.view.hud_motion)
    }

    fn field_presentation(
        &self,
        _: FramePresentationContext<'_, Self::View, ()>,
        player: usize,
    ) -> FieldPresentation<'_> {
        let plan = &self.screen.fields[player].plan;
        FieldPresentation {
            hud_style: plan.hud_style,
            hud_options: plan.options,
            center_one_player: self.screen.center_single,
            countdown_font: plan.small_combo_font,
            countdown_text_slot: FRAME_TEXT_COUNTDOWN_BASE
                + player as u8 * deadsync_notefield::COLUMN_COUNTDOWN_SLOTS_PER_PLAYER,
            ..Default::default()
        }
    }

    fn draw_field_hud(
        &self,
        context: FramePresentationContext<'_, Self::View, ()>,
        game: &GameplayCoreState,
        frame: &FieldFrame<'_>,
        request: &NotefieldComposeRequest<'_, SpriteSlot>,
        prepared: &PreparedNotefield<'_, SpriteSlot>,
        buffers: FieldHudBuffers<'_>,
    ) -> NotefieldHudComposeResult {
        let field = &self.screen.fields[frame.player];
        if let Some(alpha) = display_mods::active_alpha(
            request.view.hide_display_mods,
            request.visual.elapsed_screen_s,
        ) {
            display_mods::compose(
                buffers.actors,
                DisplayModsFrame {
                    mods_text: Arc::clone(&field.display_mods),
                    alpha,
                    playfield_center_x: prepared.field.playfield_center_x,
                    notefield_offset_y: prepared.field.notefield_offset_y,
                },
                self.screen.cmod_warning[frame.player].then_some(TextContent::Static("C-MOD ON")),
            );
        }
        field::compose_hud(
            game,
            &field.judgment,
            None,
            &field.plan,
            frame,
            request,
            prepared,
            [1.0; 4],
            context.assets.texture_context(),
            &field.broken_runs,
            &field.streams,
            buffers,
        )
    }
}

fn warning_visible(profile: &Profile, life: f32, threshold: f32) -> bool {
    life < threshold && !profile.hide_danger && !profile.hide_lifebar
}

impl OpenItgFrame<'_> {
    fn draw(
        &self,
        layer: ScreenLayer,
        actors: &mut Vec<Actor>,
        field: FieldLayout,
        motion: GameplayHudMotion,
    ) {
        let screen = self.screen;
        let hud = &screen.hud;
        let game = &self.stage.gameplay;
        let session = &game.setup.session;
        match layer {
            ScreenLayer::Background => push_background(
                actors,
                self.stage,
                &screen.background,
                Some(hud.fallback_background),
                screen.background_brightness,
                screen.background_color,
            ),
            ScreenLayer::Danger if screen.background_brightness > 0.0 => {
                let count = if session.play_style.is_routine() {
                    1
                } else {
                    game.num_players()
                };
                for index in 0..count {
                    let player = &game.players()[index];
                    let profile = &game.profiles()[index].0;
                    let failed = player.is_failing || player.life <= 0.0;
                    if !profile.hide_lifebar
                        && (failed || warning_visible(profile, player.life, hud.danger_threshold))
                    {
                        (hud.background_warning)(
                            actors,
                            GameplayBackgroundWarningView {
                                p2: self.player_is_p2(index),
                                both_sides: session.play_style.is_double()
                                    || session.play_style.is_routine(),
                                failed,
                                danger_seconds: screen.warning_seconds[index],
                                brightness: screen.background_brightness,
                            },
                        );
                    }
                }
            }
            ScreenLayer::Filter => {
                for (index, x) in field.per_player_fields {
                    if index >= game.num_players() || (session.play_style.is_routine() && index > 0)
                    {
                        continue;
                    }
                    let alpha = game.profiles()[index].background_filter.alpha();
                    if alpha > 0.0 {
                        let mut filter = SpriteBuilder::solid();
                        filter.xy(x, screen_center_y());
                        filter.size(screen.fields[index].width, screen_height());
                        filter.diffuse([0.0, 0.0, 0.0, alpha]);
                        filter.z(-99);
                        actors.push(filter.build(0));
                    }
                }
            }
            ScreenLayer::Stage if !screen.badge.is_empty() => {
                (hud.stage_badge)(
                    actors,
                    GameplayStageBadgeView {
                        text: &screen.badge,
                        round: screen.round,
                        motion,
                    },
                );
            }
            ScreenLayer::Hud => {
                for index in 0..game.num_players() {
                    self.push_player_hud(actors, index, motion);
                }
                (hud.song_meter)(
                    actors,
                    GameplaySongMeterView {
                        title: &screen.title,
                        rate: &screen.rate,
                        motion,
                        progress: song_meter_progress(
                            deadsync_core::song_time::song_time_ns_to_seconds(
                                game.current_music_time_ns(),
                            ),
                            game.song().precise_first_second(),
                            game.song().precise_last_second(),
                        ),
                    },
                );
            }
            ScreenLayer::Score(index) if index < game.num_players() => {
                self.push_score(actors, index, motion);
            }
            ScreenLayer::ExitPrompt => self.push_exit_prompt(actors),
            ScreenLayer::ExitFade => {
                if let Some(exit) = &game.exit_prompt_state().exit_transition {
                    let alpha = exit_transition_alpha(exit);
                    if alpha > 0.0 {
                        actors.push(black_cover(alpha, 1500).build(0));
                    }
                }
            }
            ScreenLayer::System => {
                if session.play_style.is_single() && game.num_players() == 1 {
                    (hud.unjoined_seat)(actors, !self.player_is_p2(0));
                }
                if let Some(status) = &screen.replay_status {
                    self.push_label(
                        actors,
                        [320.0, 447.0],
                        TextContent::Shared(Arc::clone(status)),
                    );
                } else if game.autoplay_enabled() {
                    (hud.autoplay_label)(actors);
                }
                if let Some(profiles) = &screen.profiles {
                    for (index, profile) in [&profiles.p1, &profiles.p2].into_iter().enumerate() {
                        if profile.joined && !profile.guest && !profile.hide_username {
                            if let Some(name) =
                                TextContent::inline_format(format_args!("{}", profile.display_name))
                            {
                                self.push_label(
                                    actors,
                                    [160.0 + 320.0 * index as f32, 465.0],
                                    name,
                                );
                            }
                        }
                    }
                }
            }
            ScreenLayer::Lobby if screen.lobby_hud_visible => {
                if let Some(lobby) = &self.lobby.runtime_view().snapshot.joined_lobby
                    && let Some(text) = TextContent::inline_format(format_args!("{}", lobby.code))
                {
                    self.push_label(actors, [320.0, 50.0], text);
                }
            }
            _ => {}
        }
    }

    fn player_is_p2(&self, index: usize) -> bool {
        self.stage.gameplay.setup.session.runtime_player_side(index) == GameplayInputPlayerSide::P2
    }

    fn push_player_hud(&self, actors: &mut Vec<Actor>, index: usize, motion: GameplayHudMotion) {
        let profile = &self.stage.profiles()[index];
        let chart = &self.stage.charts()[index];
        let p2 = self.player_is_p2(index);
        let hud = &self.screen.hud;
        (hud.difficulty)(
            actors,
            GameplayDifficultyView {
                p2,
                reverse: profile.reverse_scroll,
                difficulty: &chart.difficulty,
                description: &chart.description,
                meter: chart.meter,
                motion,
            },
        );
        if !profile.hide_score {
            self.push_score(actors, index, motion);
        }
        if !profile.hide_lifebar {
            let player = &self.stage.players()[index];
            let failed = player.is_failing || player.life <= 0.0;
            let life = visible_life(player.life, failed);
            let visual = self.screen.life[index];
            (hud.standard_life)(
                actors,
                GameplayLifeView {
                    p2,
                    level: visual.rendered_life(profile.lifemeter_type),
                    failed,
                    hot: visual.is_hot,
                    song_beat: self.stage.current_beat(),
                    motion,
                    percent_label: (profile.show_life_percent
                        && screen_width() >= screen_height() * (16.0 / 9.0))
                        .then_some(life * 100.0),
                },
            );
        }
    }

    fn push_score(&self, actors: &mut Vec<Actor>, index: usize, motion: GameplayHudMotion) {
        let profile = &self.stage.profiles()[index];
        let mode = score_display_mode_from_profile(profile.score_display_mode);
        let (percent, rgba) = if profile.show_ex_score {
            let score = self
                .stage
                .display_scored_ex_score_data(index, self.stage.player_blue_window_ms(index));
            (
                display_ex_score_percent_for_mode(&score, mode).max(0.0),
                self.screen
                    .palettes
                    .map_or([0.129, 0.8, 0.91, 1.0], |palettes| {
                        palettes[index].color(JudgmentColorRole::FantasticBlue)
                    }),
            )
        } else {
            (
                self.stage.display_gameplay_itg_score_percent(index, mode),
                [1.0; 4],
            )
        };
        (self.screen.hud.percentage_score)(
            actors,
            GameplayScoreView {
                p2: self.player_is_p2(index),
                doubles: self.stage.gameplay.setup.session.play_style.is_double(),
                percent,
                rgba,
                ex: profile.show_ex_score,
                motion,
                numeric_score: self.stage.players()[index].stage_score.score,
            },
        );
    }

    fn push_exit_prompt(&self, actors: &mut Vec<Actor>) {
        let prompt = self.stage.exit_prompt_state();
        let Some(key) = prompt.hold_to_exit_key else {
            return;
        };
        let text = match key {
            HoldToExitKey::Start => "Continue holding START to give up",
            HoldToExitKey::Back => "Continue holding BACK to give up",
        };
        self.push_label(actors, [320.0, 364.0], TextContent::Static(text));
    }

    fn push_label(&self, actors: &mut Vec<Actor>, at: [f32; 2], text: TextContent) {
        let layout = Layout::centered(screen_width(), screen_height());
        let mut label = layout.text(at, 0.5, text, 0.7, [1.0; 4]);
        label.font(MENU_NORMAL_FONT);
        label.z(1000);
        actors.push(label.build(0));
    }
}

fn black_cover(alpha: f32, depth: i16) -> SpriteBuilder {
    let mut cover = SpriteBuilder::solid();
    cover.xy(screen_center_x(), screen_center_y());
    cover.size(screen_width(), screen_height());
    cover.diffuse([0.0, 0.0, 0.0, alpha]);
    cover.z(depth);
    cover
}
