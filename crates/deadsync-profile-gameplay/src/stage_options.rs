//! Scroll speed and shared options for a prepared gameplay stage.

use deadsync_chart::{ChartData, SongData};
use deadsync_config::{
    app_config::Config,
    theme::{MachineFont, TournamentModeOptions, TournamentScoringSystem},
};
use deadsync_core::color::JudgmentPalette;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{
    MAX_HEART_RATE_DEFAULT, NoCmodAlternative, PlayStyle, PlayerOptionsData, PlayerSide, Profile, ScorePosition,
    StepStatisticsMask, migrate_noteskin_parts, player_side_for_index, player_side_index,
};
use deadsync_rules::scroll::{ScrollSpeedMode, ScrollSpeedSetting};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageScrollMode {
    Live { enforce_tournament_mmod: bool },
    Replay,
}

/// Machine settings and service availability for the options editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerOptionsPolicyView {
    pub allow_per_player_global_offsets: bool,
    pub heart_rate_monitors: bool,
    pub arcade_navigation: bool,
    pub machine_font: MachineFont,
    pub smx_input: bool,
    pub smx_panel_lights: bool,
    pub scorebox_available: bool,
    pub golf_enabled: bool,
    pub keyboard_features: bool,
    pub tournament_mode: bool,
}

impl PlayerOptionsPolicyView {
    #[must_use]
    pub const fn from_config(config: &Config, scorebox_available: bool) -> Self {
        Self {
            allow_per_player_global_offsets: config.machine_allow_per_player_global_offsets,
            heart_rate_monitors: config.machine_enable_heart_rate_monitors,
            arcade_navigation: config.arcade_options_navigation,
            machine_font: config.machine_font,
            smx_input: config.smx_input,
            smx_panel_lights: config.smx_panel_lights,
            scorebox_available,
            golf_enabled: config.enable_golf,
            keyboard_features: config.keyboard_features,
            tournament_mode: config.tournament.enabled,
        }
    }
}

impl Default for PlayerOptionsPolicyView {
    fn default() -> Self {
        Self::from_config(&Config::default(), false)
    }
}

/// Editable profile settings without identity, stats, or credentials.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerOptionsPlayerView {
    pub options: PlayerOptionsData,
    pub judgment_palette_id: Option<String>,
    pub heart_rate_device_id: Option<String>,
    pub max_heart_rate: u16,
}

impl Default for PlayerOptionsPlayerView {
    fn default() -> Self {
        Self {
            options: PlayerOptionsData::default(),
            judgment_palette_id: None,
            heart_rate_device_id: None,
            max_heart_rate: MAX_HEART_RATE_DEFAULT,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JudgmentPaletteChoiceView {
    pub id: String,
    pub name: String,
    pub palette: JudgmentPalette,
}

/// Session, settings, and the selected theme's palette choices for Player Options.
#[derive(Clone, Debug)]
pub struct PlayerOptionsView {
    pub policy: PlayerOptionsPolicyView,
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
    pub joined: [bool; MAX_PLAYERS],
    pub music_rate: f32,
    pub players: [PlayerOptionsPlayerView; MAX_PLAYERS],
    pub judgment_palettes: Vec<JudgmentPaletteChoiceView>,
}

/// Physical sides that can edit this stage, including solo play on P2.
pub const fn player_options_active_sides(
    play_style: PlayStyle,
    side: PlayerSide,
    joined: [bool; MAX_PLAYERS],
) -> [bool; MAX_PLAYERS] {
    if play_style.player_count() > 1 {
        if joined[0] || joined[1] {
            joined
        } else {
            [true; MAX_PLAYERS]
        }
    } else if joined[0] != joined[1] {
        joined
    } else {
        match side {
            PlayerSide::P1 => [true, false],
            PlayerSide::P2 => [false, true],
        }
    }
}

/// The owner of the stored settings in styles with one persistent profile.
pub const fn player_options_primary_slot(play_style: PlayStyle, player_side: PlayerSide) -> usize {
    if play_style.player_count() > 1 {
        0
    } else {
        player_side_index(player_side)
    }
}

/// Returns the profile to save for an edit on the given physical slot.
pub const fn player_options_persist_side(
    play_style: PlayStyle,
    player_side: PlayerSide,
    slot: usize,
) -> Option<PlayerSide> {
    if slot < MAX_PLAYERS
        && (play_style.is_versus() || slot == player_options_primary_slot(play_style, player_side))
    {
        Some(player_side_for_index(slot))
    } else {
        None
    }
}

fn adjust_quantized(value: f32, delta: isize, step: f32, min: f32, max: f32) -> f32 {
    let adjusted = (delta as f32).mul_add(step, value);
    ((adjusted / step).round() * step).clamp(min, max)
}

/// Adjusts the current scroll modifier on the canonical stage options grid.
pub fn adjust_scroll_speed(setting: ScrollSpeedSetting, delta: isize) -> ScrollSpeedSetting {
    match setting {
        ScrollSpeedSetting::XMod(value) => {
            ScrollSpeedSetting::XMod(adjust_quantized(value, delta, 0.05, 0.05, 20.0))
        }
        ScrollSpeedSetting::CMod(value) => {
            ScrollSpeedSetting::CMod(adjust_quantized(value, delta, 5.0, 5.0, 2000.0))
        }
        ScrollSpeedSetting::MMod(value) => {
            ScrollSpeedSetting::MMod(adjust_quantized(value, delta, 5.0, 5.0, 2000.0))
        }
    }
}

/// Adjusts the draft stage music rate in hundredths.
pub fn adjust_music_rate(rate: f32, delta: isize) -> f32 {
    adjust_quantized(rate, delta, 0.01, 0.05, 3.0)
}

/// The settings submitted to start a stage, including options edited for this play.
#[derive(Debug, PartialEq)]
pub struct StageOptions {
    pub play_style: PlayStyle,
    pub chart_steps_index: [usize; MAX_PLAYERS],
    pub music_rate: f32,
    pub player_options: [PlayerOptionsData; MAX_PLAYERS],
    pub scroll_speeds: [ScrollSpeedSetting; MAX_PLAYERS],
    pub judgment_palette_ids: [Option<String>; MAX_PLAYERS],
    pub heart_rate_device_ids: [Option<String>; MAX_PLAYERS],
}

impl StageOptions {
    #[must_use]
    pub fn from_profiles(
        play_style: PlayStyle,
        chart_steps_index: [usize; MAX_PLAYERS],
        music_rate: f32,
        profiles: &[Profile; MAX_PLAYERS],
    ) -> Self {
        let player_options = profiles.each_ref().map(|profile| {
            let mut options = profile.current_player_options();
            migrate_noteskin_parts(&mut options);
            options
        });
        Self {
            play_style,
            chart_steps_index,
            music_rate,
            scroll_speeds: player_options
                .each_ref()
                .map(|options| options.scroll_speed),
            player_options,
            judgment_palette_ids: profiles.each_ref().map(|p| p.judgment_palette_id.clone()),
            heart_rate_device_ids: profiles.each_ref().map(|p| p.heart_rate_device_id.clone()),
        }
    }

    /// Use the original selection to look up display BPM, even when the
    /// stage enters a different style (for example, during a Lua reload).
    pub fn resolve_scroll(
        &mut self,
        song: &SongData,
        play_style: PlayStyle,
        mode: StageScrollMode,
    ) {
        let reference_chart =
            song.chart_for_steps_index(self.play_style.chart_type(), self.chart_steps_index[0]);
        self.scroll_speeds = resolve_stage_scroll(
            song,
            reference_chart,
            self.music_rate,
            play_style,
            &mut self.player_options,
            self.scroll_speeds,
            mode,
        );
    }
}

/// Apply stage-only modifier policies to owned profile copies for simulation.
pub fn gameplay_profiles(
    mut profiles: [Profile; MAX_PLAYERS],
    song: &SongData,
    mut stage: StageOptions,
    tournament: TournamentModeOptions,
    play_style: PlayStyle,
    is_replay: bool,
    golf_enabled: bool,
) -> (
    [Profile; MAX_PLAYERS],
    [ScrollSpeedSetting; MAX_PLAYERS],
) {
    let scroll_mode = if is_replay {
        StageScrollMode::Replay
    } else {
        StageScrollMode::Live {
            enforce_tournament_mmod: tournament.enabled && tournament.enforce_no_cmod,
        }
    };
    stage.resolve_scroll(song, play_style, scroll_mode);
    for (idx, mut options) in stage.player_options.into_iter().enumerate() {
        apply_tournament_policy(&mut options, tournament, play_style);
        apply_golf_policy(&mut options, golf_enabled);
        gameplay_profile(
            &mut profiles[idx],
            options,
            stage.judgment_palette_ids[idx].take(),
            stage.heart_rate_device_ids[idx].take(),
        );
    }
    (profiles, stage.scroll_speeds)
}

fn apply_golf_policy(options: &mut PlayerOptionsData, enabled: bool) {
    if !enabled {
        options.show_golf_score = false;
        options.show_golf_results = false;
    }
}

fn apply_tournament_policy(
    options: &mut PlayerOptionsData,
    tournament: TournamentModeOptions,
    play_style: PlayStyle,
) {
    if !tournament.enabled {
        return;
    }

    options.show_ex_score =
        tournament.scoring_system == TournamentScoringSystem::Ex;
    options.show_hard_ex_score = false;
    options.show_fa_plus_pane = true;
    if tournament.show_step_stats {
        options.step_statistics = StepStatisticsMask::all_widgets();
        if play_style.is_versus() {
            options.score_position = ScorePosition::StepStatistics;
        }
    } else {
        options.step_statistics = StepStatisticsMask::empty();
        options.score_position = ScorePosition::Normal;
    }
}

fn gameplay_profile(
    profile: &mut Profile,
    options: PlayerOptionsData,
    judgment_palette_id: Option<String>,
    heart_rate_device_id: Option<String>,
) {
    profile.set_current_player_options(options);
    profile.judgment_palette_id = judgment_palette_id;
    profile.heart_rate_device_id = heart_rate_device_id;
}

#[must_use]
pub fn reference_bpm_for_song(song: &SongData, chart: Option<&ChartData>) -> f32 {
    let bpm = song
        .chart_display_bpm_range(chart)
        .map(|(_, hi)| hi as f32)
        .unwrap_or(song.max_bpm as f32);
    if bpm.is_finite() && bpm > 0.0 {
        bpm
    } else {
        120.0
    }
}

#[must_use]
pub fn effective_scroll_speed_with_alt(
    base: ScrollSpeedSetting,
    alternative: NoCmodAlternative,
    is_no_cmod: bool,
    reference_bpm: f32,
    rate: f32,
) -> ScrollSpeedSetting {
    if !is_no_cmod || !matches!(base, ScrollSpeedSetting::CMod(_)) {
        return base;
    }
    let mode = match alternative {
        NoCmodAlternative::None => return base,
        NoCmodAlternative::XMod => ScrollSpeedMode::XMod,
        NoCmodAlternative::MMod => ScrollSpeedMode::MMod,
    };
    base.converted_to(mode, reference_bpm, rate)
}

/// Keep the stage's scroll settings and score eligibility options in sync.
pub fn apply_no_cmod_alternative(
    song: &SongData,
    reference_chart: Option<&ChartData>,
    music_rate: f32,
    options: &mut [PlayerOptionsData; MAX_PLAYERS],
    speeds: [ScrollSpeedSetting; MAX_PLAYERS],
    enforce_tournament_mmod: bool,
) -> [ScrollSpeedSetting; MAX_PLAYERS] {
    let is_no_cmod = song.is_no_cmod();
    let reference_bpm = reference_bpm_for_song(song, reference_chart);
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    std::array::from_fn(|player| {
        let alternative = if enforce_tournament_mmod {
            NoCmodAlternative::MMod
        } else {
            options[player].no_cmod_alternative
        };
        let effective = effective_scroll_speed_with_alt(
            speeds[player],
            alternative,
            is_no_cmod,
            reference_bpm,
            rate,
        );
        options[player].scroll_speed = effective;
        effective
    })
}

/// Resolve the stage-local option copies and retain each replay participant's
/// recorded speed and Mini. Live Routine shares the master's effective values.
pub fn resolve_stage_scroll(
    song: &SongData,
    reference_chart: Option<&ChartData>,
    music_rate: f32,
    play_style: PlayStyle,
    options: &mut [PlayerOptionsData; MAX_PLAYERS],
    speeds: [ScrollSpeedSetting; MAX_PLAYERS],
    mode: StageScrollMode,
) -> [ScrollSpeedSetting; MAX_PLAYERS] {
    let StageScrollMode::Live {
        enforce_tournament_mmod,
    } = mode
    else {
        return speeds;
    };
    let mut speeds = apply_no_cmod_alternative(
        song,
        reference_chart,
        music_rate,
        options,
        speeds,
        enforce_tournament_mmod,
    );
    if play_style.is_routine() {
        speeds[1] = speeds[0];
        options[1].scroll_speed = options[0].scroll_speed;
        options[1].mini_percent = options[0].mini_percent;
    }
    speeds
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_simfile::cache::build_song_meta;
    use deadsync_simfile::song::{ParseSongOptions, parse_song_bytes};
    use std::path::Path;

    fn no_cmod_song() -> SongData {
        let parsed = parse_song_bytes(
            b"#TITLE:Speed options;#SUBTITLE:No CMod;#BPMS:0=150;\n\
                #NOTES:dance-single::Hard:9:0,0,0,0,0:\n1000\n0000\n0000\n0000\n;",
            "sm",
            Path::new("tests/stage-scroll.sm"),
            &ParseSongOptions::new(vec![], vec![], vec![]),
            |_| 2.0,
        )
        .unwrap();
        build_song_meta(parsed, 0.0)
    }

    #[test]
    fn stage_scroll_resolves_no_cmod_and_keeps_score_options_in_sync() {
        let song = no_cmod_song();
        let mut options = [
            PlayerOptionsData {
                no_cmod_alternative: NoCmodAlternative::XMod,
                ..Default::default()
            },
            PlayerOptionsData::default(),
        ];
        let base = [
            ScrollSpeedSetting::CMod(600.0),
            ScrollSpeedSetting::CMod(627.0),
        ];
        for (enforce_tournament_mmod, expected) in [
            (false, [ScrollSpeedSetting::XMod(3.2), base[1]]),
            (
                true,
                [
                    ScrollSpeedSetting::MMod(600.0),
                    ScrollSpeedSetting::MMod(625.0),
                ],
            ),
        ] {
            let speeds = resolve_stage_scroll(
                &song,
                song.charts.first(),
                1.25,
                PlayStyle::Versus,
                &mut options,
                base,
                StageScrollMode::Live {
                    enforce_tournament_mmod,
                },
            );
            assert_eq!(speeds, expected);
            assert_eq!(
                speeds,
                options.each_ref().map(|options| options.scroll_speed)
            );
        }
    }

    #[test]
    fn stage_scroll_shares_effective_routine_options_only_for_live_play() {
        let song = no_cmod_song();
        let original = [
            PlayerOptionsData {
                scroll_speed: ScrollSpeedSetting::CMod(600.0),
                no_cmod_alternative: NoCmodAlternative::MMod,
                mini_percent: 20,
                reverse_scroll: true,
                ..Default::default()
            },
            PlayerOptionsData {
                scroll_speed: ScrollSpeedSetting::XMod(2.5),
                mini_percent: -40,
                ..Default::default()
            },
        ];
        let base = original.each_ref().map(|options| options.scroll_speed);
        let resolve = |play_style, mode| {
            let mut options = original.clone();
            let speeds =
                resolve_stage_scroll(&song, None, 1.0, play_style, &mut options, base, mode);
            assert_eq!(
                speeds,
                options.each_ref().map(|options| options.scroll_speed)
            );
            (options, speeds)
        };
        let live = StageScrollMode::Live {
            enforce_tournament_mmod: false,
        };
        for play_style in [PlayStyle::Routine, PlayStyle::PumpRoutine] {
            let (options, speeds) = resolve(play_style, live);
            assert_eq!(speeds, [ScrollSpeedSetting::MMod(600.0); 2]);
            assert_eq!(options[1].mini_percent, 20);
            assert!(options[0].reverse_scroll && !options[1].reverse_scroll);
            assert_eq!(
                resolve(play_style, StageScrollMode::Replay),
                (original.clone(), base)
            );
        }
        let (versus, speeds) = resolve(PlayStyle::Versus, live);
        assert_eq!(versus[1], original[1]);
        assert_eq!(speeds[1], base[1]);
    }
}

#[cfg(test)]
mod gameplay_profiles_tests {
    use super::*;

    #[test]
    fn gameplay_profile_preserves_identity_while_applying_stage_options() {
        let mut profile = Profile {
            display_name: "Alice".to_owned(),
            current_combo: 42,
            ..Default::default()
        };
        profile.mini_percent = 5;
        let options = PlayerOptionsData {
            mini_percent: 37,
            ..Default::default()
        };

        gameplay_profile(
            &mut profile,
            options.clone(),
            Some("palette-1".to_owned()),
            Some("hrm-1".to_owned()),
        );

        assert_eq!(profile.display_name, "Alice");
        assert_eq!(profile.current_combo, 42);
        assert_eq!(profile.current_player_options(), options);
        assert_eq!(profile.judgment_palette_id.as_deref(), Some("palette-1"));
        assert_eq!(profile.heart_rate_device_id.as_deref(), Some("hrm-1"));
    }

    #[test]
    fn golf_master_switch_disables_only_the_gameplay_copy() {
        let original = PlayerOptionsData {
            show_golf_score: true,
            show_golf_results: true,
            golf_all_charts: true,
            golf_stroke_feedback: true,
            ..Default::default()
        };
        let mut effective = original.clone();
        apply_golf_policy(&mut effective, false);
        assert!(!effective.show_golf_score && !effective.show_golf_results);
        assert!(effective.golf_all_charts && effective.golf_stroke_feedback);
        assert!(original.show_golf_score && original.show_golf_results);

        let mut effective = original.clone();
        apply_golf_policy(&mut effective, true);
        assert_eq!(effective, original);
    }

    #[test]
    fn tournament_policy_overrides_only_the_gameplay_copy() {
        let original = PlayerOptionsData {
            show_ex_score: false,
            show_hard_ex_score: true,
            show_fa_plus_pane: false,
            score_position: ScorePosition::Normal,
            ..Default::default()
        };
        let mut effective = original.clone();

        apply_tournament_policy(
            &mut effective,
            TournamentModeOptions {
                enabled: true,
                scoring_system: TournamentScoringSystem::Ex,
                show_step_stats: true,
                enforce_no_cmod: true,
            },
            PlayStyle::Versus,
        );

        assert!(effective.show_ex_score);
        assert!(!effective.show_hard_ex_score);
        assert!(effective.show_fa_plus_pane);
        assert_eq!(
            effective.step_statistics,
            StepStatisticsMask::all_widgets()
        );
        assert_eq!(
            effective.score_position,
            ScorePosition::StepStatistics
        );
        assert!(!original.show_ex_score);
        assert!(original.show_hard_ex_score);
        assert!(!original.show_fa_plus_pane);
    }

    #[test]
    fn tournament_hide_stats_uses_itg_score_and_normal_position() {
        let mut effective = PlayerOptionsData {
            show_ex_score: true,
            show_hard_ex_score: true,
            step_statistics: StepStatisticsMask::all_widgets(),
            score_position: ScorePosition::StepStatistics,
            ..Default::default()
        };

        apply_tournament_policy(
            &mut effective,
            TournamentModeOptions {
                enabled: true,
                scoring_system: TournamentScoringSystem::Itg,
                show_step_stats: false,
                enforce_no_cmod: false,
            },
            PlayStyle::Single,
        );

        assert!(!effective.show_ex_score);
        assert!(!effective.show_hard_ex_score);
        assert!(effective.step_statistics.is_empty());
        assert_eq!(
            effective.score_position,
            ScorePosition::Normal
        );
    }
}
