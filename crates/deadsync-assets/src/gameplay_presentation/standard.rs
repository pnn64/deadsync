//! Asset-backed stage composition for themes that use the standard gameplay HUD.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{
    GameplayBackgroundMedia, GameplayPresentationInit, GameplayRenderView, GameplayScreen,
    GameplayTextPrewarm, PracticeGameplayHud,
    background::{GameplayBackground, push_background},
    display_mods::{self, DisplayModsFrame},
    field::{self, GameplayNotefieldPlan, ResolvedJudgmentAssets},
    field_style::{DEFAULT_GAMEPLAY_HUD_STYLE, notefield_style_with_palette},
    life::{LifeMeterVisual, visible_life},
    text_slots::FRAME_TEXT_COUNTDOWN_BASE,
};
use crate::gameplay_stage::{
    ActiveGameplayStage, GameplayCoreState, GameplayStageAssets, GameplayStagePresenter,
};
use crate::noteskin::SpriteSlot;
use deadlib_assets::AssetView;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::color::Color;
use deadlib_present::dsl::SpriteBuilder;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_config::{app_config::Config, theme::MachineFont};
use deadsync_core::color::{JudgmentColorRole, JudgmentPalette};
use deadsync_core::hud_style::{
    GameplayBackgroundWarningView, GameplayChatLayout, GameplayDifficultyView, GameplayHud,
    GameplayHudMotion, GameplayLifeView, GameplayProfileFooterView, GameplayScoreView,
    GameplaySongMeterView, GameplayStageBadgeView, GameplayWarningScope,
};
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
use deadsync_profile::{GameplayHudSnapshot, Profile};
use deadsync_profile_gameplay::{GameplayProfile, score_display_mode_from_profile};
use deadsync_song_lua::gameplay_presentation::{
    FieldHudBuffers, FieldPresentation, FramePresentationContext, GameplayFrameBuffers,
    GameplayFramePresenter, SongFrameContent, compose_stage_frame,
};
use deadsync_song_lua::playback::{
    FieldFrame, FieldLayout, FrameScratch, GameplayActorSegments, ScreenLayer, song_meter_progress,
};

pub enum GameplayHudLabel<'a> {
    ExitPrompt(HoldToExitKey),
    Replay(&'a Arc<str>),
    Profile { index: usize, name: &'a str },
    LobbyCode(&'a str),
}

pub struct StandardGameplaySettings {
    pub translated_titles: bool,
    pub center_single_notefield: bool,
    pub background_brightness: f32,
    pub background_color: Color,
    pub machine_font: MachineFont,
}

pub struct StandardGameplayStyle<L> {
    pub hud: GameplayHud<Actor>,
    pub badge: Arc<str>,
    pub round: Option<usize>,
    pub replay_status: Option<Arc<str>>,
    pub push_label: fn(&mut Vec<Actor>, GameplayHudLabel<'_>),
    pub lobby_code: fn(&L) -> Option<&str>,
}

struct PlayerField {
    judgment: ResolvedJudgmentAssets,
    plan: GameplayNotefieldPlan,
    broken_runs: BrokenRunLookup,
    streams: StreamProgressLookup,
    width: f32,
    display_mods: Arc<str>,
}

pub struct StandardGameplay<L> {
    style: StandardGameplayStyle<L>,
    background: GameplayBackground,
    hide_intro: bool,
    title: Arc<str>,
    rate: Arc<str>,
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

impl<L> StandardGameplay<L> {
    pub fn new(
        stage: &ActiveGameplayStage,
        assets: GameplayStageAssets,
        settings: StandardGameplaySettings,
        init: Option<GameplayPresentationInit>,
        style: StandardGameplayStyle<L>,
    ) -> Self {
        let palettes = init.as_ref().map(|init| init.judgment_palettes);
        let center_single = init.is_some() && settings.center_single_notefield;
        let profiles = std::array::from_fn(|player| &stage.profiles()[player].0);
        let widths = field::gameplay_notefield_widths(&stage.gameplay, &stage.noteskin_assets);
        let fields = std::array::from_fn(|player| {
            let profile = profiles[player];
            let judgment = ResolvedJudgmentAssets::from_profile(profile);
            judgment.prewarm();
            let field_style = palettes.map_or(DEFAULT_GAMEPLAY_HUD_STYLE, |palettes| {
                notefield_style_with_palette(palettes[player])
            });
            let mut plan = field::gameplay_notefield_plan(
                profile,
                &judgment,
                stage.player_blue_window_ms(player) / 1000.0,
                field_style,
            );
            plan.install_feedback(
                style.hud.field_feedback,
                style.hud.default_tap_atlas,
                style.hud.default_hold_atlas,
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
        let mut screen = Self {
            style,
            background: GameplayBackground::new(stage),
            hide_intro: assets.hide_song_intro,
            title: Arc::from(stage.song().display_full_title(settings.translated_titles)),
            rate: Arc::from(""),
            pack_banner: assets.pack_banner_path,
            profiles: init.map(|init| init.hud),
            center_single,
            background_brightness: settings.background_brightness,
            background_color: settings.background_color,
            machine_font: settings.machine_font,
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

impl<L> GameplayStagePresenter<L> for StandardGameplay<L> {
    type View = GameplayRenderView;
    type Policy = ();

    fn compose_stage_frame(
        &mut self,
        stage: &ActiveGameplayStage,
        lobby: &L,
        song: &mut FrameScratch,
        actors: &mut Vec<Actor>,
        context: FramePresentationContext<'_, Self::View, Self::Policy>,
        content: SongFrameContent,
    ) -> GameplayActorSegments {
        compose_stage_frame(
            actors,
            stage,
            &StandardFrame {
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

impl<L> StandardGameplay<L> {
    pub fn update_visuals(&mut self, game: &GameplayCoreState, delta_time: f32) {
        for (index, clock) in self
            .warning_seconds
            .iter_mut()
            .enumerate()
            .take(game.num_players())
        {
            let (profile, player) = (&game.profiles()[index].0, &game.players()[index]);
            let failed = player.is_failing || player.life <= 0.0;
            self.life[index].update(player.life, failed, delta_time);
            if !failed && warning_visible(profile, player.life, self.style.hud.danger_threshold) {
                *clock += delta_time.max(0.0);
            }
        }
    }

    pub fn toggle_lobby(&mut self) {
        self.lobby_hud_visible = !self.lobby_hud_visible;
    }

    pub fn set_cmod_warning(&mut self, warnings: [bool; MAX_PLAYERS]) {
        self.cmod_warning = warnings;
    }
}

impl<L> GameplayScreen<L, GameplayHud<Actor>> for StandardGameplay<L> {
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
        self.style.replay_status.as_deref()
    }

    fn chat_layout(&self, stage: &ActiveGameplayStage, _: &L) -> Option<GameplayChatLayout> {
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
        self.style.hud = hud;
    }

    fn prewarm_text(&self, stage: &ActiveGameplayStage, _: &L, prewarm: GameplayTextPrewarm<'_>) {
        let fonts = prewarm.assets.fonts();
        deadsync_song_lua::playback::prewarm_text_layout(prewarm.cache, fonts, &stage.gameplay);
        super::prewarm_compose_storage(prewarm.scratch, stage.num_players());
        field::prewarm_text(
            prewarm,
            &stage.gameplay,
            self.fields.each_ref().map(|field| &field.plan),
        );
    }

    fn entry(
        &self,
        _: &ActiveGameplayStage,
        _: &L,
        _: &dyn AssetView,
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

impl<L> PracticeGameplayHud for StandardGameplay<L> {
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

struct StandardFrame<'a, L> {
    screen: &'a StandardGameplay<L>,
    stage: &'a ActiveGameplayStage,
    lobby: &'a L,
}

impl<L> GameplayFramePresenter for StandardFrame<'_, L> {
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

impl<L> StandardFrame<'_, L> {
    fn draw(
        &self,
        layer: ScreenLayer,
        actors: &mut Vec<Actor>,
        field: FieldLayout,
        motion: GameplayHudMotion,
    ) {
        let screen = self.screen;
        let hud = &screen.style.hud;
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
                self.push_warning(actors);
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
            ScreenLayer::Stage if !screen.style.badge.is_empty() => {
                (hud.stage_badge)(
                    actors,
                    GameplayStageBadgeView {
                        text: &screen.style.badge,
                        round: screen.style.round,
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
            ScreenLayer::ExitPrompt => {
                if let Some(key) = game.exit_prompt_state().hold_to_exit_key {
                    (screen.style.push_label)(actors, GameplayHudLabel::ExitPrompt(key));
                }
            }
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
                if let Some(status) = &screen.style.replay_status {
                    (screen.style.push_label)(actors, GameplayHudLabel::Replay(status));
                } else if game.autoplay_enabled() {
                    (hud.autoplay_label)(actors);
                }
                self.push_profile_footer(actors);
            }
            ScreenLayer::Lobby if screen.lobby_hud_visible => {
                if let Some(code) = (screen.style.lobby_code)(self.lobby) {
                    (screen.style.push_label)(actors, GameplayHudLabel::LobbyCode(code));
                }
            }
            _ => {}
        }
    }

    fn player_is_p2(&self, index: usize) -> bool {
        self.stage.gameplay.setup.session.runtime_player_side(index) == GameplayInputPlayerSide::P2
    }

    fn push_warning(&self, actors: &mut Vec<Actor>) {
        let screen = self.screen;
        let hud = &screen.style.hud;
        let game = &self.stage.gameplay;
        let play_style = game.setup.session.play_style;
        let count = if play_style.is_routine() {
            1
        } else {
            game.num_players()
        };
        let joined = &game.players()[..count];
        let is_low = |index: usize| {
            warning_visible(
                &game.profiles()[index].0,
                joined[index].life,
                hud.danger_threshold,
            )
        };
        let failed = |index: usize| joined[index].is_failing || joined[index].life <= 0.0;
        let brightness = screen.background_brightness;
        if hud.warning_scope == GameplayWarningScope::AllPlayers {
            if (0..count).all(is_low) && (0..count).any(|index| !failed(index)) {
                (hud.background_warning)(
                    actors,
                    GameplayBackgroundWarningView {
                        p2: false,
                        both_sides: true,
                        failed: false,
                        danger_seconds: screen.warning_seconds[..count]
                            .iter()
                            .copied()
                            .fold(0.0, f32::max),
                        brightness,
                    },
                );
            }
            return;
        }
        for index in 0..count {
            let failed = failed(index);
            if !game.profiles()[index].hide_lifebar && (failed || is_low(index)) {
                (hud.background_warning)(
                    actors,
                    GameplayBackgroundWarningView {
                        p2: self.player_is_p2(index),
                        both_sides: play_style.is_double() || play_style.is_routine(),
                        failed,
                        danger_seconds: screen.warning_seconds[index],
                        brightness,
                    },
                );
            }
        }
    }

    fn push_profile_footer(&self, actors: &mut Vec<Actor>) {
        let Some(profiles) = &self.screen.profiles else {
            return;
        };
        let players = [&profiles.p1, &profiles.p2];
        let player_names = players.map(|profile| {
            (profile.joined && !profile.guest && !profile.hide_username)
                .then_some(profile.display_name.as_str())
        });
        if let Some(draw) = self.screen.style.hud.profile_footer {
            draw(actors, GameplayProfileFooterView { player_names });
        } else {
            for (index, name) in player_names.into_iter().enumerate() {
                if let Some(name) = name {
                    (self.screen.style.push_label)(
                        actors,
                        GameplayHudLabel::Profile { index, name },
                    );
                }
            }
        }
    }

    fn push_player_hud(&self, actors: &mut Vec<Actor>, index: usize, motion: GameplayHudMotion) {
        let profile = &self.stage.profiles()[index];
        let chart = &self.stage.charts()[index];
        let p2 = self.player_is_p2(index);
        let hud = &self.screen.style.hud;
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
        (self.screen.style.hud.percentage_score)(
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
}

fn black_cover(alpha: f32, depth: i16) -> SpriteBuilder {
    let mut cover = SpriteBuilder::solid();
    cover.xy(screen_center_x(), screen_center_y());
    cover.size(screen_width(), screen_height());
    cover.diffuse([0.0, 0.0, 0.0, alpha]);
    cover.z(depth);
    cover
}
