//! Scroll speed and shared options for a prepared gameplay stage.

use deadsync_chart::{ChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{
    NoCmodAlternative, PlayStyle, PlayerOptionsData, Profile, migrate_noteskin_parts,
};
use deadsync_rules::scroll::{ScrollSpeedMode, ScrollSpeedSetting};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageScrollMode {
    Live { enforce_tournament_mmod: bool },
    Replay,
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
