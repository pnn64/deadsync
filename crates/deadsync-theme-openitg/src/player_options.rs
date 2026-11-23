//! Player Options layout and controls over the shared stage settings.

use crate::presentation::{ACCENT, HEIGHT, INK, Layout, MUTED, WIDTH, quad};
use crate::{Effect, OpenItgRuntimeRequest, OpenItgScreen, OpenItgTheme};
use deadlib_assets::AssetManager;
use deadlib_platform::input::{KeyCode, RawKeyboardEvent};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_chart::SongData;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile::requests::ProfileRequest;
use deadsync_profile::{MAX_HEART_RATE_MAX, MAX_HEART_RATE_MIN, migrate_noteskin_parts};
use deadsync_profile_gameplay::stage_options::{
    PlayerOptionsView, StageOptions, adjust_music_rate, adjust_scroll_speed,
    player_options_active_sides, player_options_persist_side, reference_bpm_for_song,
};
use deadsync_rules::scroll::{ScrollSpeedMode, ScrollSpeedSetting};
use deadsync_theme::player_options::{
    PlayerOptionsEntry, PlayerOptionsSelection, ThemePlayerOptions,
};
use deadsync_theme::{AudioRequest, UiCue};
use std::cell::OnceCell;
use std::sync::Arc;

pub(super) const TRANSITION_SECONDS: f32 = 0.3;
const FIRST_ROW_Y: f32 = 127.0;
const ROW_PITCH: f32 = 28.0;
const EXIT_Y: f32 = 353.0;
const EXIT: usize = ROWS.len() - 1;

#[derive(Clone, Copy)]
enum Row {
    SpeedType,
    Speed,
    Rate,
    Chart,
    Exit,
}

const ROWS: [Row; 5] = [Row::SpeedType, Row::Speed, Row::Rate, Row::Chart, Row::Exit];

impl Row {
    fn label(self) -> &'static str {
        match self {
            Self::SpeedType => "Speed Type",
            Self::Speed => "Scroll Speed",
            Self::Rate => "Music Rate",
            Self::Chart => "Stepchart",
            Self::Exit => "Exit",
        }
    }

    fn y(self, index: usize) -> f32 {
        if matches!(self, Self::Exit) {
            EXIT_Y
        } else {
            FIRST_ROW_Y + index as f32 * ROW_PITCH
        }
    }
}

/// Retains the editable settings and each active player's focus.
pub struct PlayerOptionsState {
    song: Arc<SongData>,
    steps: [usize; MAX_PLAYERS],
    preferred: [usize; MAX_PLAYERS],
    color_index: i32,
    return_screen: OpenItgScreen,
    fixed_stepchart_label: Option<Arc<str>>,
    chart_labels: OnceCell<[TextContent; MAX_PLAYERS]>,
    view: PlayerOptionsView,
    scroll_speeds: [ScrollSpeedSetting; MAX_PLAYERS],
    active: [bool; MAX_PLAYERS],
    focus: [usize; MAX_PLAYERS],
}

impl PlayerOptionsState {
    fn ready_to_play(&self) -> bool {
        self.active
            .iter()
            .zip(self.focus)
            .all(|(active, focus)| !*active || focus == EXIT)
    }

    fn move_focus(&mut self, player: usize, delta: isize, effects: &mut Vec<Effect>) {
        self.focus[player] =
            (self.focus[player] as isize + delta).rem_euclid(ROWS.len() as isize) as usize;
        effects.push(Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Change)));
    }

    fn start(&mut self, player: usize, effects: &mut Vec<Effect>) {
        if self.focus[player] != EXIT {
            if self.view.policy.arcade_navigation {
                self.move_focus(player, 1, effects);
            } else {
                self.focus[player] = EXIT;
                effects.push(Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Change)));
            }
            return;
        }

        if self.ready_to_play() {
            let play = if self.return_screen == OpenItgScreen::Practice {
                OpenItgScreen::Practice
            } else {
                OpenItgScreen::Gameplay
            };
            effects.push(Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Confirm)));
            effects.push(Effect::Navigate(play));
        }
    }

    fn adjust(&mut self, player: usize, delta: isize, effects: &mut Vec<Effect>) {
        let previous_speed = self.scroll_speeds[player];
        let speed = match ROWS[self.focus[player]] {
            Row::SpeedType => {
                let modes = [
                    ScrollSpeedMode::XMod,
                    ScrollSpeedMode::CMod,
                    ScrollSpeedMode::MMod,
                ];
                let current = match previous_speed {
                    ScrollSpeedSetting::XMod(_) => 0,
                    ScrollSpeedSetting::CMod(_) => 1,
                    ScrollSpeedSetting::MMod(_) => 2,
                };
                let mode = modes[(current + delta).rem_euclid(modes.len() as isize) as usize];
                let chart = self
                    .song
                    .chart_for_steps_index(self.view.play_style.chart_type(), self.steps[0]);
                let bpm = reference_bpm_for_song(&self.song, chart);
                let rate = if self.view.music_rate.is_finite() && self.view.music_rate > 0.0 {
                    self.view.music_rate
                } else {
                    1.0
                };
                previous_speed.converted_to(mode, bpm, rate)
            }
            Row::Speed => adjust_scroll_speed(previous_speed, delta),
            Row::Rate => {
                let rate = adjust_music_rate(self.view.music_rate, delta);
                if rate.to_bits() != self.view.music_rate.to_bits() {
                    self.view.music_rate = rate;
                    effects.push(Effect::Runtime(OpenItgRuntimeRequest::Profile(
                        ProfileRequest::SetMusicRate(rate),
                    )));
                    effects.push(Effect::Runtime(OpenItgRuntimeRequest::Audio(
                        AudioRequest::SetMusicRate(rate),
                    )));
                    effects.push(Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Change)));
                }
                return;
            }
            Row::Chart | Row::Exit => return,
        };
        if speed == previous_speed {
            return;
        }

        self.scroll_speeds[player] = speed;
        let edited = &mut self.view.players[player];
        edited.options.scroll_speed = speed;
        if let Some(side) =
            player_options_persist_side(self.view.play_style, self.view.player_side, player)
        {
            effects.push(Effect::Runtime(OpenItgRuntimeRequest::Profile(
                ProfileRequest::UpdatePlayerOptions {
                    side,
                    options: Box::new(edited.options.clone()),
                    judgment_palette_id: edited.judgment_palette_id.clone(),
                    heart_rate_device_id: edited.heart_rate_device_id.clone(),
                    max_heart_rate: edited.max_heart_rate,
                },
            )));
        }
        effects.push(Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Change)));
    }

    fn value(&self, row: Row, player: usize) -> TextContent {
        let speed = self.scroll_speeds[player];
        match row {
            Row::SpeedType => match speed {
                ScrollSpeedSetting::XMod(_) => "X-Mod",
                ScrollSpeedSetting::CMod(_) => "C-Mod",
                ScrollSpeedSetting::MMod(_) => "M-Mod",
            }
            .into(),
            Row::Speed => {
                TextContent::inline_format(format_args!("{speed}")).unwrap_or_else(|| "-".into())
            }
            Row::Rate => TextContent::inline_format(format_args!("{:.2}x", self.view.music_rate))
                .unwrap_or_else(|| "-".into()),
            Row::Chart => self
                .chart_labels
                .get_or_init(|| std::array::from_fn(|index| self.chart_label(index)))[player]
                .clone(),
            Row::Exit => "Play".into(),
        }
    }

    fn chart_label(&self, player: usize) -> TextContent {
        if let Some(label) = &self.fixed_stepchart_label {
            return TextContent::Shared(Arc::clone(label));
        }
        let Some(chart) = self
            .song
            .chart_for_steps_index(self.view.play_style.chart_type(), self.steps[player])
        else {
            return "-".into();
        };
        TextContent::inline_format(format_args!("{} {}", chart.difficulty, chart.meter))
            .unwrap_or_else(|| {
                TextContent::Shared(Arc::from(format!("{} {}", chart.difficulty, chart.meter)))
            })
    }
}

impl ThemePlayerOptions for OpenItgTheme {
    type PlayerOptionsState = PlayerOptionsState;
    type PlayerOptionsView = PlayerOptionsView;
    type PlayerOptionsStage = StageOptions;
    type PlayerOptionsInput = InputEvent;
    type PlayerOptionsRawKey = RawKeyboardEvent;
    type PlayerOptionsVisualPolicy = ();
    type PlayerOptionsNoteskin = ();

    fn init_player_options(
        entry: PlayerOptionsEntry<OpenItgScreen, PlayerOptionsView>,
    ) -> PlayerOptionsState {
        let mut view = entry.view;
        for player in &mut view.players {
            migrate_noteskin_parts(&mut player.options);
            player.max_heart_rate = player
                .max_heart_rate
                .clamp(MAX_HEART_RATE_MIN, MAX_HEART_RATE_MAX);
        }
        let active = player_options_active_sides(view.play_style, view.player_side, view.joined);
        let scroll_speeds = entry.scroll_speeds.unwrap_or_else(|| {
            view.players
                .each_ref()
                .map(|player| player.options.scroll_speed)
        });
        PlayerOptionsState {
            song: entry.song,
            steps: entry.steps,
            preferred: entry.preferred,
            color_index: entry.color_index,
            return_screen: entry.return_screen,
            fixed_stepchart_label: entry.fixed_stepchart_label.map(Arc::from),
            chart_labels: OnceCell::new(),
            view,
            scroll_speeds,
            active,
            focus: [0; MAX_PLAYERS],
        }
    }

    fn player_options_selection(
        state: &PlayerOptionsState,
    ) -> PlayerOptionsSelection<'_, OpenItgScreen> {
        PlayerOptionsSelection {
            song: &state.song,
            steps: state.steps,
            preferred: state.preferred,
            color_index: state.color_index,
            return_screen: state.return_screen,
            fixed_stepchart_label: state.fixed_stepchart_label.as_deref(),
            scroll_speeds: state.scroll_speeds,
            music_rate: state.view.music_rate,
        }
    }

    fn player_options_stage(state: &PlayerOptionsState) -> StageOptions {
        StageOptions {
            play_style: state.view.play_style,
            chart_steps_index: state.steps,
            music_rate: state.view.music_rate,
            player_options: state
                .view
                .players
                .each_ref()
                .map(|player| player.options.clone()),
            scroll_speeds: state.scroll_speeds,
            judgment_palette_ids: state
                .view
                .players
                .each_ref()
                .map(|player| player.judgment_palette_id.clone()),
            heart_rate_device_ids: state
                .view
                .players
                .each_ref()
                .map(|player| player.heart_rate_device_id.clone()),
        }
    }

    fn player_options_song_mut(state: &mut PlayerOptionsState) -> &mut Arc<SongData> {
        state.chart_labels.take();
        &mut state.song
    }

    fn prepare_player_options(_state: &mut PlayerOptionsState, _assets: &AssetManager) {}

    fn update_player_options(
        _state: &mut PlayerOptionsState,
        _dt: f32,
        _assets: &AssetManager,
        _effects: &mut Vec<Effect>,
    ) {
    }

    fn handle_player_options_input(
        state: &mut PlayerOptionsState,
        _assets: &AssetManager,
        input: &InputEvent,
        effects: &mut Vec<Effect>,
    ) {
        if input.pressed {
            handle_action(state, input.action, effects);
        }
    }

    fn handle_player_options_raw_key(
        state: &mut PlayerOptionsState,
        key: Option<&RawKeyboardEvent>,
        _text: Option<&str>,
        _ctrl_held: bool,
        effects: &mut Vec<Effect>,
    ) -> bool {
        if key.is_some_and(|key| key.code == KeyCode::Escape && key.pressed && !key.repeat) {
            effects.push(Effect::Navigate(state.return_screen));
            true
        } else {
            false
        }
    }

    fn push_player_options_actors(
        actors: &mut Vec<Actor>,
        state: &PlayerOptionsState,
        _assets: &AssetManager,
        (): (),
    ) {
        push_options(actors, state);
    }
}

#[derive(Clone, Copy)]
enum Control {
    Row(isize),
    Value(isize),
    Start,
    Back,
    PreviousArcadeRow,
}

fn handle_action(state: &mut PlayerOptionsState, action: VirtualAction, effects: &mut Vec<Effect>) {
    use VirtualAction::*;
    let (player, control) = match action {
        p1_up | p1_menu_up => (0, Control::Row(-1)),
        p2_up | p2_menu_up => (1, Control::Row(-1)),
        p1_down | p1_menu_down => (0, Control::Row(1)),
        p2_down | p2_menu_down => (1, Control::Row(1)),
        p1_left | p1_menu_left => (0, Control::Value(-1)),
        p2_left | p2_menu_left => (1, Control::Value(-1)),
        p1_right | p1_menu_right => (0, Control::Value(1)),
        p2_right | p2_menu_right => (1, Control::Value(1)),
        p1_start => (0, Control::Start),
        p2_start => (1, Control::Start),
        p1_back => (0, Control::Back),
        p2_back => (1, Control::Back),
        p1_select => (0, Control::PreviousArcadeRow),
        p2_select => (1, Control::PreviousArcadeRow),
        _ => return,
    };
    if !state.active[player] {
        return;
    }
    match control {
        Control::Row(delta) => state.move_focus(player, delta, effects),
        Control::Value(delta) => state.adjust(player, delta, effects),
        Control::Start => state.start(player, effects),
        Control::Back => effects.push(Effect::Navigate(state.return_screen)),
        Control::PreviousArcadeRow if state.view.policy.arcade_navigation => {
            state.move_focus(player, -1, effects)
        }
        Control::PreviousArcadeRow => {}
    }
}

fn push_options(actors: &mut Vec<Actor>, state: &PlayerOptionsState) {
    let (width, height) = (screen_width(), screen_height());
    let layout = Layout::centered(width, height);
    let ink = [INK[0], INK[1], INK[2], 1.0];
    let muted = [MUTED[0], MUTED[1], MUTED[2], 1.0];
    let accent = [ACCENT[0], ACCENT[1], ACCENT[2], 1.0];

    actors.push(quad(
        [0.0, 0.0],
        [width, height],
        [0.035, 0.048, 0.072, 1.0],
        -20,
    ));
    actors.push(layout.panel(0.0, 0.0, [WIDTH, HEIGHT], [0.063, 0.087, 0.126, 1.0], -19));
    actors.push(layout.label([36.0, 38.0], 0.0, "PLAYER OPTIONS".into(), 1.8, ink));
    actors.push(layout.panel(36.0, 70.0, [568.0, 1.0], [0.32, 0.40, 0.48, 1.0], -17));

    let positions = if state.active == [true, true] {
        [337.0, 515.0]
    } else {
        [395.0, 395.0]
    };
    for (player, &x) in positions.iter().enumerate() {
        if state.active[player] {
            let label = if player == 0 { "P1" } else { "P2" };
            actors.push(layout.label([x, 92.0], 0.5, label.into(), 1.1, accent));
        }
    }
    for (index, row) in ROWS.iter().copied().enumerate() {
        let y = row.y(index);
        actors.push(layout.panel(36.0, y - 13.0, [568.0, 26.0], [0.09, 0.13, 0.19, 1.0], -18));
        actors.push(layout.label([53.0, y], 0.0, row.label().into(), 1.0, muted));
        for (player, &x) in positions.iter().enumerate() {
            if state.active[player] {
                let focused = state.focus[player] == index;
                if focused {
                    actors.push(layout.panel(
                        x - 79.0,
                        y - 12.0,
                        [158.0, 24.0],
                        [0.24, 0.30, 0.38, 1.0],
                        -10,
                    ));
                    actors.push(layout.panel(x - 79.0, y - 12.0, [2.0, 24.0], accent, -9));
                }
                actors.push(layout.column_text(
                    [x, y],
                    152.0,
                    state.value(row, player),
                    if focused { ink } else { muted },
                ));
            }
        }
    }

    actors.push(layout.panel(36.0, 426.0, [568.0, 1.0], [0.32, 0.40, 0.48, 1.0], -17));
    actors.push(layout.label([36.0, 447.0], 0.0, "BACK".into(), 1.0, muted));
    let start_help = if state.ready_to_play() {
        "START: PLAY"
    } else if state.view.policy.arcade_navigation {
        "START: NEXT ROW"
    } else {
        "START: EXIT ROW"
    };
    actors.push(layout.label([604.0, 447.0], 1.0, start_help.into(), 0.9, accent));
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_profile::{PlayStyle, PlayerSide};
    use deadsync_profile_gameplay::stage_options::{
        PlayerOptionsPlayerView, PlayerOptionsPolicyView,
    };

    fn song() -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: "test.sm".into(),
            title: "Options fixture".into(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            translit_artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
            normalized_bpms: String::new(),
            song_timing: None,
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            precise_note_first_second_seconds: None,
            precise_note_last_second_seconds: None,
            last_second_hint: 0.0,
            charts: Vec::new(),
        })
    }

    fn entry(
        play_style: PlayStyle,
        player_side: PlayerSide,
        joined: [bool; MAX_PLAYERS],
    ) -> PlayerOptionsEntry<OpenItgScreen, PlayerOptionsView> {
        PlayerOptionsEntry {
            song: song(),
            steps: [3, 3],
            preferred: [3, 3],
            color_index: 2,
            return_screen: OpenItgScreen::CourseSelection,
            fixed_stepchart_label: Some("Hard 9".into()),
            noteskin_catalog: Default::default(),
            smx_gif_catalog: Default::default(),
            heart_rate_devices: Default::default(),
            view: PlayerOptionsView {
                policy: PlayerOptionsPolicyView {
                    arcade_navigation: true,
                    ..Default::default()
                },
                play_style,
                player_side,
                joined,
                music_rate: 1.0,
                players: std::array::from_fn(|_| PlayerOptionsPlayerView::default()),
                judgment_palettes: Vec::new(),
            },
            scroll_speeds: None,
        }
    }

    #[test]
    fn restored_effective_speed_and_p2_profile_save_stay_on_their_original_side() {
        let mut entry = entry(PlayStyle::Single, PlayerSide::P2, [false, true]);
        entry.scroll_speeds = Some([
            ScrollSpeedSetting::CMod(400.0),
            ScrollSpeedSetting::MMod(805.0),
        ]);
        entry.view.players[1].options.scroll_speed = ScrollSpeedSetting::CMod(400.0);
        entry.view.players[1].judgment_palette_id = Some("p2-palette".into());
        let mut state = OpenItgTheme::init_player_options(entry);
        let restored = OpenItgTheme::player_options_stage(&state);
        assert_eq!(restored.scroll_speeds[1], ScrollSpeedSetting::MMod(805.0));
        assert_eq!(
            restored.player_options[1].scroll_speed,
            ScrollSpeedSetting::CMod(400.0)
        );
        let mut effects = Vec::new();
        handle_action(&mut state, VirtualAction::p1_menu_right, &mut effects);
        assert!(effects.is_empty());
        handle_action(&mut state, VirtualAction::p2_menu_down, &mut effects);
        effects.clear();
        handle_action(&mut state, VirtualAction::p2_menu_right, &mut effects);
        let [
            Effect::Runtime(OpenItgRuntimeRequest::Profile(ProfileRequest::UpdatePlayerOptions {
                side,
                options,
                judgment_palette_id,
                ..
            })),
            Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Change)),
        ] = effects.as_slice()
        else {
            panic!("expected a profile change and selection cue");
        };
        assert_eq!(*side, PlayerSide::P2);
        assert_eq!(options.scroll_speed, ScrollSpeedSetting::MMod(810.0));
        assert_eq!(judgment_palette_id.as_deref(), Some("p2-palette"));
        assert_eq!(
            OpenItgTheme::player_options_stage(&state).scroll_speeds,
            [
                ScrollSpeedSetting::CMod(400.0),
                ScrollSpeedSetting::MMod(810.0)
            ]
        );
    }

    #[test]
    fn versus_arcade_start_waits_for_both_exit_rows() {
        let mut state = OpenItgTheme::init_player_options(entry(
            PlayStyle::Versus,
            PlayerSide::P1,
            [true, true],
        ));
        let mut effects = Vec::new();
        for _ in 0..ROWS.len() - 1 {
            handle_action(&mut state, VirtualAction::p1_start, &mut effects);
        }
        assert_eq!(state.focus, [EXIT, 0]);
        effects.clear();
        handle_action(&mut state, VirtualAction::p1_start, &mut effects);
        assert!(effects.is_empty());
        handle_action(&mut state, VirtualAction::p2_menu_up, &mut effects);
        effects.clear();
        handle_action(&mut state, VirtualAction::p2_start, &mut effects);
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Confirm)),
                Effect::Navigate(OpenItgScreen::Gameplay)
            ]
        ));
    }

    #[test]
    fn player_options_preserves_reference_row_anchors_at_native_and_wide_sizes() {
        use deadlib_present::space::{Metrics, with_current_metrics};

        let state = OpenItgTheme::init_player_options(entry(
            PlayStyle::Versus,
            PlayerSide::P1,
            [true, true],
        ));
        for (width, shift) in [(640.0, 0.0), (854.0, 107.0)] {
            with_current_metrics(Metrics::centered(width, HEIGHT), || {
                let mut actors = Vec::new();
                push_options(&mut actors, &state);
                for (label, expected) in [
                    ("Speed Type", [53.0 + shift, 127.0]),
                    ("Scroll Speed", [53.0 + shift, 155.0]),
                    ("Exit", [53.0 + shift, 353.0]),
                    ("P1", [337.0 + shift, 92.0]),
                    ("P2", [515.0 + shift, 92.0]),
                ] {
                    assert!(
                        actors.iter().any(|actor| matches!(actor,
                            Actor::Text { content: TextContent::Static(text), offset, .. }
                                if *text == label && *offset == expected
                        )),
                        "missing {label} at {expected:?}"
                    );
                }
            });
        }
    }

    #[test]
    fn rate_edits_send_the_same_value_to_the_session_and_audio() {
        let mut state = OpenItgTheme::init_player_options(entry(
            PlayStyle::Single,
            PlayerSide::P1,
            [true, false],
        ));
        let mut effects = Vec::new();
        handle_action(&mut state, VirtualAction::p1_menu_down, &mut effects);
        handle_action(&mut state, VirtualAction::p1_menu_down, &mut effects);
        effects.clear();
        handle_action(&mut state, VirtualAction::p1_menu_right, &mut effects);
        let [
            Effect::Runtime(OpenItgRuntimeRequest::Profile(ProfileRequest::SetMusicRate(session))),
            Effect::Runtime(OpenItgRuntimeRequest::Audio(AudioRequest::SetMusicRate(audio))),
            Effect::Runtime(OpenItgRuntimeRequest::Cue(UiCue::Change)),
        ] = effects.as_slice()
        else {
            panic!("expected paired session and audio changes");
        };
        assert_eq!(session, audio);
        assert!((*session - 1.01).abs() < 0.001);
        assert_eq!(
            *session,
            OpenItgTheme::player_options_stage(&state).music_rate
        );
        effects.clear();
        handle_action(&mut state, VirtualAction::p1_back, &mut effects);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Navigate(OpenItgScreen::CourseSelection)]
        ));
    }
}
