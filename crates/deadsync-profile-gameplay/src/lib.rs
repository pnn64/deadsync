pub mod cabinet_records;
pub mod results;
pub mod stage_charts;
pub mod stage_options;
pub mod stage_replay;
pub mod stage_scores;

use deadsync_gameplay::{CrossoverRow, build_crossover_rows};
use deadsync_profile::groovestats::groovestats_eval_state_from_profile;
use deadsync_rules::{note::Note, timing::TimingSegments};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GameplayProfile(pub deadsync_profile::Profile);

impl Deref for GameplayProfile {
    type Target = deadsync_profile::Profile;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for GameplayProfile {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<deadsync_profile::Profile> for GameplayProfile {
    #[inline(always)]
    fn from(profile: deadsync_profile::Profile) -> Self {
        Self(profile)
    }
}

impl From<GameplayProfile> for deadsync_profile::Profile {
    #[inline(always)]
    fn from(profile: GameplayProfile) -> Self {
        profile.0
    }
}

#[must_use]
pub const fn gameplay_play_style_from_profile(
    play_style: deadsync_profile::PlayStyle,
) -> deadsync_gameplay::GameplayInputPlayStyle {
    match play_style {
        deadsync_profile::PlayStyle::Single => deadsync_gameplay::GameplayInputPlayStyle::Single,
        deadsync_profile::PlayStyle::Versus => deadsync_gameplay::GameplayInputPlayStyle::Versus,
        deadsync_profile::PlayStyle::Double => deadsync_gameplay::GameplayInputPlayStyle::Double,
        deadsync_profile::PlayStyle::PumpSingle => {
            deadsync_gameplay::GameplayInputPlayStyle::PumpSingle
        }
        deadsync_profile::PlayStyle::PumpVersus => {
            deadsync_gameplay::GameplayInputPlayStyle::PumpVersus
        }
        deadsync_profile::PlayStyle::PumpDouble => {
            deadsync_gameplay::GameplayInputPlayStyle::PumpDouble
        }
        deadsync_profile::PlayStyle::Couple => deadsync_gameplay::GameplayInputPlayStyle::Couple,
        deadsync_profile::PlayStyle::Routine => deadsync_gameplay::GameplayInputPlayStyle::Routine,
        deadsync_profile::PlayStyle::PumpCouple => {
            deadsync_gameplay::GameplayInputPlayStyle::PumpCouple
        }
        deadsync_profile::PlayStyle::PumpRoutine => {
            deadsync_gameplay::GameplayInputPlayStyle::PumpRoutine
        }
    }
}

#[must_use]
pub const fn gameplay_player_side_from_profile(
    side: deadsync_profile::PlayerSide,
) -> deadsync_gameplay::GameplayInputPlayerSide {
    match side {
        deadsync_profile::PlayerSide::P1 => deadsync_gameplay::GameplayInputPlayerSide::P1,
        deadsync_profile::PlayerSide::P2 => deadsync_gameplay::GameplayInputPlayerSide::P2,
    }
}

#[must_use]
pub const fn profile_side_from_gameplay(
    side: deadsync_gameplay::GameplayInputPlayerSide,
) -> deadsync_profile::PlayerSide {
    match side {
        deadsync_gameplay::GameplayInputPlayerSide::P1 => deadsync_profile::PlayerSide::P1,
        deadsync_gameplay::GameplayInputPlayerSide::P2 => deadsync_profile::PlayerSide::P2,
    }
}

#[derive(Clone, Debug)]
pub struct GameplayPackData {
    pub pack_group: Arc<str>,
    pub pack_banner_path: Option<PathBuf>,
    pub sync_pref: deadsync_chart::SyncPref,
}

#[must_use]
pub fn song_pack_group(song: &deadsync_chart::SongData) -> Arc<str> {
    Arc::from(
        song.simfile_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or(""),
    )
}

#[must_use]
pub fn gameplay_pack_data(
    song: &deadsync_chart::SongData,
    course_name: Option<&Arc<str>>,
    course_banner_path: Option<&PathBuf>,
) -> GameplayPackData {
    let pack_group = song_pack_group(song);
    let mut pack_banner_path = None;
    let mut sync_pref = deadsync_chart::SyncPref::Default;
    if !pack_group.is_empty()
        && let Some(pack) = deadsync_simfile::runtime_cache::get_song_cache()
            .iter()
            .find(|pack| pack.group_name == pack_group.as_ref())
    {
        pack_banner_path.clone_from(&pack.banner_path);
        sync_pref = pack.sync_pref;
    }
    if let Some(course_name) = course_name {
        pack_banner_path = course_banner_path.cloned();
        return GameplayPackData {
            pack_group: course_name.clone(),
            pack_banner_path,
            sync_pref,
        };
    }
    GameplayPackData {
        pack_group,
        pack_banner_path,
        sync_pref,
    }
}

#[must_use]
pub fn gameplay_runtime_profile_data(
    player_profiles: &[deadsync_profile::Profile; deadsync_core::input::MAX_PLAYERS],
    session: &deadsync_gameplay::GameplaySession,
) -> [deadsync_profile::Profile; deadsync_core::input::MAX_PLAYERS] {
    let first_player = usize::from(session.p2_runtime_player());
    [
        player_profiles[first_player].clone(),
        player_profiles[1].clone(),
    ]
}

/// Use simfile timing to classify the transformed chart's crossover rows.
pub fn gameplay_crossover_annotations_for_player(
    notes: &[Note],
    note_range: (usize, usize),
    timing_segments: &TimingSegments,
    cols_per_player: usize,
    col_start: usize,
) -> Vec<CrossoverRow> {
    let (start, end) = note_range;
    if start >= end {
        return Vec::new();
    }
    let annotations = match cols_per_player {
        4 => {
            let (rows, row_to_beat, _) = build_crossover_rows::<4>(notes, note_range, col_start);
            deadsync_simfile::timing::crossover_annotations::<4>(
                &rows,
                &row_to_beat,
                timing_segments,
            )
        }
        8 => {
            let (rows, row_to_beat, _) = build_crossover_rows::<8>(notes, note_range, col_start);
            deadsync_simfile::timing::crossover_annotations::<8>(
                &rows,
                &row_to_beat,
                timing_segments,
            )
        }
        _ => return Vec::new(),
    };
    annotations
        .iter()
        .map(|annotation| CrossoverRow {
            beat: annotation.beat,
            column_mask: annotation.column_mask,
            crossover: annotation.crossover,
            bracket: annotation.bracket,
        })
        .collect()
}

#[must_use]
pub const fn gameplay_tick_mode_from_profile(
    mode: deadsync_profile::TimingTickMode,
) -> deadsync_gameplay::GameplayTimingTickMode {
    match mode {
        deadsync_profile::TimingTickMode::Off => deadsync_gameplay::GameplayTimingTickMode::Off,
        deadsync_profile::TimingTickMode::Assist => {
            deadsync_gameplay::GameplayTimingTickMode::Assist
        }
        deadsync_profile::TimingTickMode::Hit => deadsync_gameplay::GameplayTimingTickMode::Hit,
        deadsync_profile::TimingTickMode::Beat => deadsync_gameplay::GameplayTimingTickMode::Beat,
    }
}

#[must_use]
pub const fn profile_tick_mode_from_gameplay(
    mode: deadsync_gameplay::GameplayTimingTickMode,
) -> deadsync_profile::TimingTickMode {
    match mode {
        deadsync_gameplay::GameplayTimingTickMode::Off => deadsync_profile::TimingTickMode::Off,
        deadsync_gameplay::GameplayTimingTickMode::Assist => {
            deadsync_profile::TimingTickMode::Assist
        }
        deadsync_gameplay::GameplayTimingTickMode::Hit => deadsync_profile::TimingTickMode::Hit,
        deadsync_gameplay::GameplayTimingTickMode::Beat => deadsync_profile::TimingTickMode::Beat,
    }
}

#[must_use]
pub const fn gameplay_fail_type_from_config(
    fail_type: deadsync_config::theme::DefaultFailType,
) -> deadsync_gameplay::GameplayFailType {
    match fail_type {
        deadsync_config::theme::DefaultFailType::Immediate => {
            deadsync_gameplay::GameplayFailType::Immediate
        }
        deadsync_config::theme::DefaultFailType::ImmediateContinue => {
            deadsync_gameplay::GameplayFailType::ImmediateContinue
        }
    }
}

#[must_use]
pub const fn gameplay_note_scroll_clock_from_config(
    clock: deadsync_config::audio::NoteScrollClock,
) -> deadsync_gameplay::GameplayNoteScrollClock {
    match clock {
        deadsync_config::audio::NoteScrollClock::RawAudio => {
            deadsync_gameplay::GameplayNoteScrollClock::RawAudio
        }
        deadsync_config::audio::NoteScrollClock::FrameStable => {
            deadsync_gameplay::GameplayNoteScrollClock::FrameStable
        }
    }
}

#[must_use]
pub const fn gameplay_config_from_config(
    cfg: &deadsync_config::app_config::Config,
) -> deadsync_gameplay::GameplayConfig {
    deadsync_gameplay::GameplayConfig {
        mine_hit_sound: cfg.mine_hit_sound,
        default_fail_type: gameplay_fail_type_from_config(cfg.default_fail_type),
        global_offset_seconds: cfg.global_offset_seconds,
        pad_stick_seconds: cfg.pad_stick_seconds,
        input_debounce_seconds: cfg.input_debounce_seconds,
        visual_delay_seconds: cfg.visual_delay_seconds,
        note_scroll_clock: gameplay_note_scroll_clock_from_config(cfg.note_scroll_clock),
        machine_pack_ini_offsets: cfg.machine_pack_ini_offsets,
        machine_default_sync_pref: cfg.machine_default_sync_offset.sync_pref(),
        machine_allow_per_player_global_offsets: cfg.machine_allow_per_player_global_offsets,
        machine_enable_replays: cfg.machine_enable_replays,
        center_1player_notefield: cfg.center_1player_notefield,
        delayed_back: cfg.delayed_back,
    }
}

#[must_use]
pub const fn score_display_mode_from_profile(
    mode: deadsync_profile::ScoreDisplayMode,
) -> deadsync_gameplay::GameplayScoreDisplayMode {
    match mode {
        deadsync_profile::ScoreDisplayMode::Normal => {
            deadsync_gameplay::GameplayScoreDisplayMode::Normal
        }
        deadsync_profile::ScoreDisplayMode::Predictive => {
            deadsync_gameplay::GameplayScoreDisplayMode::Predictive
        }
    }
}

const fn gameplay_target_score_setting(
    setting: deadsync_profile::TargetScoreSetting,
) -> deadsync_gameplay::GameplayTargetScoreSetting {
    match setting {
        deadsync_profile::TargetScoreSetting::CMinus => {
            deadsync_gameplay::GameplayTargetScoreSetting::CMinus
        }
        deadsync_profile::TargetScoreSetting::C => deadsync_gameplay::GameplayTargetScoreSetting::C,
        deadsync_profile::TargetScoreSetting::CPlus => {
            deadsync_gameplay::GameplayTargetScoreSetting::CPlus
        }
        deadsync_profile::TargetScoreSetting::BMinus => {
            deadsync_gameplay::GameplayTargetScoreSetting::BMinus
        }
        deadsync_profile::TargetScoreSetting::B => deadsync_gameplay::GameplayTargetScoreSetting::B,
        deadsync_profile::TargetScoreSetting::BPlus => {
            deadsync_gameplay::GameplayTargetScoreSetting::BPlus
        }
        deadsync_profile::TargetScoreSetting::AMinus => {
            deadsync_gameplay::GameplayTargetScoreSetting::AMinus
        }
        deadsync_profile::TargetScoreSetting::A => deadsync_gameplay::GameplayTargetScoreSetting::A,
        deadsync_profile::TargetScoreSetting::APlus => {
            deadsync_gameplay::GameplayTargetScoreSetting::APlus
        }
        deadsync_profile::TargetScoreSetting::SMinus => {
            deadsync_gameplay::GameplayTargetScoreSetting::SMinus
        }
        deadsync_profile::TargetScoreSetting::S => deadsync_gameplay::GameplayTargetScoreSetting::S,
        deadsync_profile::TargetScoreSetting::SPlus => {
            deadsync_gameplay::GameplayTargetScoreSetting::SPlus
        }
        deadsync_profile::TargetScoreSetting::Star1 => {
            deadsync_gameplay::GameplayTargetScoreSetting::Star1
        }
        deadsync_profile::TargetScoreSetting::Star2 => {
            deadsync_gameplay::GameplayTargetScoreSetting::Star2
        }
        deadsync_profile::TargetScoreSetting::Star3 => {
            deadsync_gameplay::GameplayTargetScoreSetting::Star3
        }
        deadsync_profile::TargetScoreSetting::Star4 => {
            deadsync_gameplay::GameplayTargetScoreSetting::Star4
        }
        deadsync_profile::TargetScoreSetting::SpecifiedValue => {
            deadsync_gameplay::GameplayTargetScoreSetting::SpecifiedValue
        }
        deadsync_profile::TargetScoreSetting::MachineBest => {
            deadsync_gameplay::GameplayTargetScoreSetting::MachineBest
        }
        deadsync_profile::TargetScoreSetting::GhostData => {
            deadsync_gameplay::GameplayTargetScoreSetting::GhostData
        }
        deadsync_profile::TargetScoreSetting::PersonalBest => {
            deadsync_gameplay::GameplayTargetScoreSetting::PersonalBest
        }
    }
}

#[must_use]
pub const fn gameplay_attack_mode(
    mode: deadsync_profile::AttackMode,
) -> deadsync_gameplay::GameplayAttackMode {
    match mode {
        deadsync_profile::AttackMode::Off => deadsync_gameplay::GameplayAttackMode::Off,
        deadsync_profile::AttackMode::On => deadsync_gameplay::GameplayAttackMode::On,
        deadsync_profile::AttackMode::Random => deadsync_gameplay::GameplayAttackMode::Random,
    }
}

#[must_use]
pub const fn chart_effects_from_profile(
    profile: &deadsync_profile::Profile,
) -> deadsync_gameplay::ChartAttackEffects {
    deadsync_gameplay::ChartAttackEffects {
        insert_mask: profile.insert_active_mask.bits(),
        remove_mask: profile.remove_active_mask.bits(),
        holds_mask: profile.holds_active_mask.bits(),
        turn_bits: 0,
    }
}

#[must_use]
pub fn score_invalid_reason_lines_for_profile(
    chart: &deadsync_chart::ChartData,
    profile: &deadsync_profile::Profile,
    music_rate: f32,
) -> Vec<&'static str> {
    deadsync_gameplay::score_invalid_reason_lines_for_options(
        chart,
        deadsync_gameplay::ScoreValidityOptions {
            chart_effects: chart_effects_from_profile(profile),
            attack_mode: gameplay_attack_mode(profile.attack_mode),
            music_rate,
        },
    )
}

#[must_use]
pub fn blue_fantastic_window_ms_for_profile(
    base_fa_plus_s: f32,
    profile: &deadsync_profile::Profile,
) -> f32 {
    deadsync_gameplay::blue_fantastic_window_ms(deadsync_gameplay::FantasticWindowOptions {
        base_fa_plus_s,
        custom_fantastic_window_s: profile.custom_fantastic_window.then_some(
            f32::from(deadsync_profile::clamp_custom_fantastic_window_ms(
                profile.custom_fantastic_window_ms,
            )) / 1000.0,
        ),
        fa_plus_10ms_blue_window: profile.fa_plus_10ms_blue_window,
    })
}

#[inline(always)]
fn groovestats_fail_type_ok_from_app_runtime() -> bool {
    matches!(
        deadsync_config::runtime::get().default_fail_type,
        deadsync_config::theme::DefaultFailType::Immediate
            | deadsync_config::theme::DefaultFailType::ImmediateContinue
    )
}

pub fn groovestats_eval_state_from_app_runtime<
    RuntimeProfile,
    OverlayActor,
    CapturedActor,
    StateDelta,
>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
) -> deadsync_score::GrooveStatsEvalState
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
{
    if player_idx >= gs.num_players().min(deadsync_core::input::MAX_PLAYERS) {
        return deadsync_score::GrooveStatsEvalState::default();
    }

    let chart = gs.charts()[player_idx].as_ref();
    let profile = gs.profiles()[player_idx].deref();
    let outcome = gs.individual_song_outcome(player_idx);
    let result = deadsync_score::groovestats_eval_state_from_gameplay_parts(
        groovestats_eval_state_from_profile(
            chart,
            profile,
            gs.music_rate(),
            gs.autoplay_used(),
            gs.course_display_is_course_stage(),
            deadsync_config::runtime::get().autosubmit_course_scores_individually,
            groovestats_fail_type_ok_from_app_runtime(),
        ),
        deadsync_score::GrooveStatsGameplayEvalInput {
            song_has_lua: gs.song().has_lua,
            lua_submit_allowed: gs.mods.song_lua_compile_succeeded
                || deadsync_score::lua_chart_submit_allowed(chart.short_hash.as_str()),
            song_completed_naturally: outcome.song_completed_naturally,
            is_failing: outcome.is_failing,
            life: outcome.life,
            has_fail_time: outcome.fail_time.is_some(),
            course_stage_life_submit_eligible: gs.course_stage_life_submit_eligible(player_idx),
        },
    );
    result.state
}

#[must_use]
pub fn scroll_effects_from_option(
    scroll: deadsync_profile::ScrollOption,
) -> deadsync_gameplay::ScrollEffects {
    deadsync_gameplay::scroll_effects_from_flags(
        scroll.contains(deadsync_profile::ScrollOption::Reverse),
        scroll.contains(deadsync_profile::ScrollOption::Split),
        scroll.contains(deadsync_profile::ScrollOption::Alternate),
        scroll.contains(deadsync_profile::ScrollOption::Cross),
        scroll.contains(deadsync_profile::ScrollOption::Centered),
    )
}

#[must_use]
pub const fn tap_explosion_options_from_profile(
    profile: &deadsync_profile::Profile,
) -> deadsync_gameplay::TapExplosionOptions {
    let mask = profile.tap_explosion_active_mask;
    deadsync_gameplay::TapExplosionOptions {
        fantastic: mask.contains(deadsync_profile::TapExplosionMask::FANTASTIC),
        excellent: mask.contains(deadsync_profile::TapExplosionMask::EXCELLENT),
        great: mask.contains(deadsync_profile::TapExplosionMask::GREAT),
        decent: mask.contains(deadsync_profile::TapExplosionMask::DECENT),
        way_off: mask.contains(deadsync_profile::TapExplosionMask::WAY_OFF),
        miss: mask.contains(deadsync_profile::TapExplosionMask::MISS),
        held: mask.contains(deadsync_profile::TapExplosionMask::HELD),
        holding: mask.contains(deadsync_profile::TapExplosionMask::HOLDING),
    }
}

pub const fn gameplay_turn_option(
    turn: deadsync_profile::TurnOption,
) -> deadsync_gameplay::GameplayTurnOption {
    match turn {
        deadsync_profile::TurnOption::None => deadsync_gameplay::GameplayTurnOption::None,
        deadsync_profile::TurnOption::Mirror => deadsync_gameplay::GameplayTurnOption::Mirror,
        deadsync_profile::TurnOption::LRMirror => deadsync_gameplay::GameplayTurnOption::LRMirror,
        deadsync_profile::TurnOption::UDMirror => deadsync_gameplay::GameplayTurnOption::UDMirror,
        deadsync_profile::TurnOption::Left => deadsync_gameplay::GameplayTurnOption::Left,
        deadsync_profile::TurnOption::Right => deadsync_gameplay::GameplayTurnOption::Right,
        deadsync_profile::TurnOption::Shuffle => deadsync_gameplay::GameplayTurnOption::Shuffle,
        deadsync_profile::TurnOption::Blender => deadsync_gameplay::GameplayTurnOption::Blender,
        deadsync_profile::TurnOption::Random => deadsync_gameplay::GameplayTurnOption::Random,
    }
}

const fn mini_indicator_mode(
    mode: deadsync_profile::MiniIndicator,
) -> deadsync_gameplay::GameplayMiniIndicatorMode {
    match mode {
        deadsync_profile::MiniIndicator::None => deadsync_gameplay::GameplayMiniIndicatorMode::None,
        deadsync_profile::MiniIndicator::SubtractiveScoring => {
            deadsync_gameplay::GameplayMiniIndicatorMode::SubtractiveScoring
        }
        deadsync_profile::MiniIndicator::PredictiveScoring => {
            deadsync_gameplay::GameplayMiniIndicatorMode::PredictiveScoring
        }
        deadsync_profile::MiniIndicator::PaceScoring => {
            deadsync_gameplay::GameplayMiniIndicatorMode::PaceScoring
        }
        deadsync_profile::MiniIndicator::RivalScoring => {
            deadsync_gameplay::GameplayMiniIndicatorMode::RivalScoring
        }
        deadsync_profile::MiniIndicator::Pacemaker => {
            deadsync_gameplay::GameplayMiniIndicatorMode::Pacemaker
        }
        deadsync_profile::MiniIndicator::StreamProg => {
            deadsync_gameplay::GameplayMiniIndicatorMode::StreamProg
        }
    }
}

const fn error_bar_trim(
    trim: deadsync_profile::ErrorBarTrim,
) -> deadsync_gameplay::GameplayErrorBarTrim {
    match trim {
        deadsync_profile::ErrorBarTrim::Off => deadsync_gameplay::GameplayErrorBarTrim::Off,
        deadsync_profile::ErrorBarTrim::Fantastic => {
            deadsync_gameplay::GameplayErrorBarTrim::Fantastic
        }
        deadsync_profile::ErrorBarTrim::Excellent => {
            deadsync_gameplay::GameplayErrorBarTrim::Excellent
        }
        deadsync_profile::ErrorBarTrim::Great => deadsync_gameplay::GameplayErrorBarTrim::Great,
    }
}

impl deadsync_gameplay::GameplayProfileData for GameplayProfile {
    fn insert_mask_bits(&self) -> u8 {
        self.insert_active_mask.bits()
    }

    fn remove_mask_bits(&self) -> u8 {
        self.remove_active_mask.bits()
    }

    fn holds_mask_bits(&self) -> u8 {
        self.holds_active_mask.bits()
    }

    fn appearance_mask_bits(&self) -> u8 {
        self.appearance_effects_active_mask.bits()
    }

    fn accel_mask_bits(&self) -> u8 {
        self.accel_effects_active_mask.bits()
    }

    fn visual_mask_bits(&self) -> u16 {
        self.visual_effects_active_mask.bits()
    }

    fn turn_option(&self) -> deadsync_gameplay::GameplayTurnOption {
        gameplay_turn_option(self.turn_option)
    }

    fn attack_mode(&self) -> deadsync_gameplay::GameplayAttackMode {
        gameplay_attack_mode(self.attack_mode)
    }

    fn perspective_effects(&self) -> deadsync_gameplay::PerspectiveEffects {
        let (tilt, skew) = self.perspective.tilt_skew();
        deadsync_gameplay::PerspectiveEffects { tilt, skew }
    }

    fn scroll_effects(&self) -> deadsync_gameplay::ScrollEffects {
        deadsync_gameplay::scroll_effects_from_flags(
            self.scroll_option
                .contains(deadsync_profile::ScrollOption::Reverse),
            self.scroll_option
                .contains(deadsync_profile::ScrollOption::Split),
            self.scroll_option
                .contains(deadsync_profile::ScrollOption::Alternate),
            self.scroll_option
                .contains(deadsync_profile::ScrollOption::Cross),
            self.scroll_option
                .contains(deadsync_profile::ScrollOption::Centered),
        )
    }

    fn mini_indicator_options(&self) -> deadsync_gameplay::GameplayMiniIndicatorOptions {
        deadsync_gameplay::GameplayMiniIndicatorOptions {
            requested_mode: mini_indicator_mode(self.mini_indicator),
            measure_counter_enabled: self.measure_counter != deadsync_profile::MeasureCounter::None,
            subtractive_scoring: self.subtractive_scoring,
            pacemaker: self.pacemaker,
        }
    }

    fn target_score(&self) -> deadsync_gameplay::GameplayTargetScoreSetting {
        gameplay_target_score_setting(self.target_score)
    }

    fn target_score_kind(&self) -> deadsync_gameplay::GameplayTargetScoreKind {
        match self.mini_indicator_score_type {
            deadsync_profile::MiniIndicatorScoreType::Itg => {
                deadsync_gameplay::GameplayTargetScoreKind::Itg
            }
            deadsync_profile::MiniIndicatorScoreType::Ex => {
                deadsync_gameplay::GameplayTargetScoreKind::Ex
            }
            deadsync_profile::MiniIndicatorScoreType::HardEx => {
                deadsync_gameplay::GameplayTargetScoreKind::HardEx
            }
        }
    }

    fn target_score_miss_policy(&self) -> deadsync_gameplay::GameplayTargetScoreMissPolicy {
        match self.target_score_miss_policy {
            deadsync_profile::TargetScoreMissPolicy::Nothing => {
                deadsync_gameplay::GameplayTargetScoreMissPolicy::Nothing
            }
            deadsync_profile::TargetScoreMissPolicy::DimMiniIndicator => {
                deadsync_gameplay::GameplayTargetScoreMissPolicy::DimMiniIndicator
            }
            deadsync_profile::TargetScoreMissPolicy::Fail => {
                deadsync_gameplay::GameplayTargetScoreMissPolicy::Fail
            }
            deadsync_profile::TargetScoreMissPolicy::RestartSong => {
                deadsync_gameplay::GameplayTargetScoreMissPolicy::RestartSong
            }
        }
    }

    fn timing_disabled_windows(&self) -> [bool; 5] {
        self.timing_windows.disabled_windows()
    }

    fn column_flash_options(&self) -> deadsync_gameplay::ColumnFlashOptions {
        let mask = self.column_flash_mask;
        deadsync_gameplay::ColumnFlashOptions {
            enabled: self.column_flash_on_miss,
            blue_fantastic: mask.contains(deadsync_profile::ColumnFlashMask::BLUE_FANTASTIC),
            white_fantastic: mask.contains(deadsync_profile::ColumnFlashMask::WHITE_FANTASTIC),
            excellent: mask.contains(deadsync_profile::ColumnFlashMask::EXCELLENT),
            great: mask.contains(deadsync_profile::ColumnFlashMask::GREAT),
            decent: mask.contains(deadsync_profile::ColumnFlashMask::DECENT),
            way_off: mask.contains(deadsync_profile::ColumnFlashMask::WAY_OFF),
            miss: mask.contains(deadsync_profile::ColumnFlashMask::MISS),
        }
    }

    fn tap_explosion_options(&self) -> deadsync_gameplay::TapExplosionOptions {
        tap_explosion_options_from_profile(self)
    }

    fn fantastic_options(&self, base_fa_plus_s: f32) -> deadsync_gameplay::FantasticWindowOptions {
        deadsync_gameplay::FantasticWindowOptions {
            base_fa_plus_s,
            custom_fantastic_window_s: self.custom_fantastic_window.then_some(
                f32::from(deadsync_profile::clamp_custom_fantastic_window_ms(
                    self.custom_fantastic_window_ms,
                )) / 1000.0,
            ),
            fa_plus_10ms_blue_window: self.fa_plus_10ms_blue_window,
        }
    }

    fn fantastic_feedback_options(&self) -> deadsync_gameplay::FantasticFeedbackOptions {
        deadsync_gameplay::FantasticFeedbackOptions {
            show_fa_plus_window: self.show_fa_plus_window,
            fa_plus_10ms_blue_window: self.fa_plus_10ms_blue_window,
            split_15_10ms: self.split_15_10ms,
            custom_fantastic_window: self.custom_fantastic_window,
        }
    }

    fn error_bar_options(&self) -> deadsync_gameplay::GameplayErrorBarOptions {
        let mut mask = self.error_bar_active_mask;
        if mask.is_empty() {
            mask = deadsync_profile::error_bar_mask_from_style(self.error_bar, self.error_bar_text);
        }
        deadsync_gameplay::GameplayErrorBarOptions {
            mask_bits: mask.bits(),
            text_scalable: self.text_error_bar_scalable,
            text_threshold_ms: deadsync_profile::clamp_text_error_bar_threshold_ms(
                self.text_error_bar_threshold_ms,
            ),
            show_fa_plus_window: self.show_fa_plus_window,
            trim: error_bar_trim(self.error_bar_trim),
            multi_tick: self.error_bar_multi_tick,
            error_ms_display: self.error_ms_display,
            short_average_enabled: self.short_average_error_bar_enabled,
            short_average_intensity: deadsync_profile::clamp_average_error_bar_intensity(
                self.average_error_bar_intensity,
            ),
            long_average_enabled: self.long_error_bar_enabled,
            long_average_threshold_ms: deadsync_profile::clamp_long_error_bar_threshold_ms(
                self.long_error_bar_threshold_ms,
            ),
            long_average_intensity: deadsync_profile::clamp_long_error_bar_intensity(
                self.long_error_bar_intensity,
            ),
            long_average_min_samples: deadsync_profile::clamp_long_error_bar_min_samples(
                self.long_error_bar_min_samples,
            ),
            average_interval_ms: deadsync_profile::clamp_average_error_bar_interval_ms(
                self.average_error_bar_interval_ms,
            ),
        }
    }

    fn measure_counter_threshold(&self) -> Option<usize> {
        self.measure_counter.notes_threshold()
    }

    fn note_field_offset_x(&self) -> f32 {
        self.note_field_offset_x as f32
    }

    fn noteskin_name(&self) -> String {
        self.noteskin.to_string()
    }

    fn mini_percent(&self) -> f32 {
        self.mini_percent as f32
    }

    fn spacing_percent(&self) -> i32 {
        self.spacing_percent
    }

    fn global_offset_shift_ms(&self) -> i32 {
        self.global_offset_shift_ms
    }

    fn visual_delay_ms(&self) -> i32 {
        self.visual_delay_ms
    }

    fn reverse_scroll(&self) -> bool {
        self.reverse_scroll
    }

    fn column_cues(&self) -> bool {
        self.column_cues
    }

    fn crossover_cues(&self) -> bool {
        self.crossover_cues
    }

    fn crossover_cue_duration_ms(&self) -> u16 {
        self.crossover_cue_duration_ms
    }

    fn crossover_cue_quantization(&self) -> u8 {
        self.crossover_cue_quantization
    }

    fn crossover_cue_brackets(&self) -> bool {
        self.crossover_cue_brackets
    }

    fn golf_scoring_enabled(&self) -> bool {
        self.show_golf_score || self.show_golf_results
    }

    fn carry_combo_between_songs(&self) -> bool {
        self.carry_combo_between_songs
    }

    fn calculated_weight_pounds(&self) -> i32 {
        self.0.calculated_weight_pounds()
    }

    fn hide_lifebar(&self) -> bool {
        self.hide_lifebar
    }

    fn hide_danger(&self) -> bool {
        self.hide_danger
    }

    fn hide_song_background(&self) -> bool {
        self.hide_song_bg
    }

    fn rescore_early_hits(&self) -> bool {
        self.rescore_early_hits
    }

    fn hide_early_dw_judgments(&self) -> bool {
        self.hide_early_dw_judgments
    }

    fn hide_early_dw_flash(&self) -> bool {
        self.hide_early_dw_flash
    }

    fn hide_early_dw_column_flash(&self) -> bool {
        self.hide_early_dw_column_flash
    }

    fn combo_milestones_enabled(&self) -> bool {
        !self.hide_combo && !self.hide_combo_explosions
    }
}

#[cfg(test)]
mod gameplay_profile_tests {
    use super::*;
    use deadsync_gameplay::GameplayProfileData;

    #[test]
    fn hidden_combo_art_disables_milestone_runtime_work() {
        let mut profile = GameplayProfile(deadsync_profile::Profile::default());
        profile.hide_combo = false;
        profile.hide_combo_explosions = false;
        assert!(profile.combo_milestones_enabled());

        profile.hide_combo_explosions = true;
        assert!(!profile.combo_milestones_enabled());

        profile.hide_combo_explosions = false;
        profile.hide_combo = true;
        assert!(!profile.combo_milestones_enabled());
    }
}

pub fn itl_score_calc_input_from_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
) -> deadsync_score::ItlScoreCalcInput<'_>
where
    RuntimeProfile: deadsync_gameplay::GameplayProfileData,
{
    let (start, end) = gs.note_range_for_player(player_idx);
    let totals = gs.stage_totals_for_player(player_idx);
    let outcome = gs.individual_song_outcome(player_idx);
    deadsync_score::ItlScoreCalcInput {
        notes: &gs.notes()[start..end],
        note_times: &gs.note_time_cache_ns()[start..end],
        hold_end_times: &gs.hold_end_time_cache_ns()[start..end],
        total_steps: totals.total_steps,
        holds_total: totals.holds_total,
        rolls_total: totals.rolls_total,
        mines_total: totals.mines_total,
        fail_time: outcome
            .fail_time
            .map(deadsync_core::song_time::song_time_ns_from_seconds),
    }
}

pub fn itl_current_score_hundredths_from_runtime<
    RuntimeProfile,
    OverlayActor,
    CapturedActor,
    StateDelta,
>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
) -> Option<u32>
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
{
    let disabled_windows = gs.profiles()[player_idx].timing_windows.disabled_windows();
    deadsync_score::itl_current_score_hundredths_for_submit(
        itl_score_calc_input_from_runtime(gs, player_idx),
        disabled_windows.as_slice(),
    )
}

pub fn itl_judgments_from_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
) -> deadsync_score::ItlJudgments
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
{
    let player = &gs.players()[player_idx];
    let totals = gs.stage_totals_for_player(player_idx);
    let windows = gs.live_window_counts(player_idx);
    let disabled = gs.profiles()[player_idx].timing_windows.disabled_windows();
    deadsync_score::itl_judgments_from_counts(deadsync_score::ItlJudgmentCountsInput {
        fantastic_plus: windows.w0,
        fantastic: windows.w1,
        excellent: windows.w2,
        great: windows.w3,
        decent: if disabled[3] { 0 } else { windows.w4 },
        way_off: if disabled[4] { 0 } else { windows.w5 },
        miss: windows.miss,
        total_steps: totals.total_steps,
        holds_held: player.holds_held,
        total_holds: totals.holds_total,
        mines_hit: player.mines_hit,
        total_mines: totals.mines_total,
        rolls_held: player.rolls_held,
        total_rolls: totals.rolls_total,
    })
}

pub fn itl_eval_state_from_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
    data: &deadsync_score::ItlFileData,
    song_dir: Option<&str>,
    group_name: Option<&str>,
    subtitle: &str,
    groovestats_state: &deadsync_score::GrooveStatsEvalState,
) -> deadsync_score::ItlEvalState
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
{
    let profile = gs.profiles()[player_idx].deref();
    let disabled_windows = profile.timing_windows.disabled_windows();
    let outcome = gs.individual_song_outcome(player_idx);
    let passed = deadsync_score::gameplay_run_passed(
        outcome.song_completed_naturally,
        outcome.is_failing,
        outcome.life,
        outcome.fail_time.is_some(),
    );
    deadsync_score::itl_eval_state_from_gameplay_context(deadsync_score::ItlGameplayEvalInput {
        song_dir,
        group_name,
        data,
        chart_hash: gs.charts()[player_idx].short_hash.as_str(),
        subtitle,
        used_cmod: deadsync_score::groovestats_used_cmod(profile.scroll_speed),
        groovestats_valid: groovestats_state.valid,
        groovestats_reason_lines: groovestats_state.reason_lines.as_slice(),
        music_rate: gs.music_rate(),
        remove_mask: profile.remove_active_mask.bits(),
        disabled_windows: disabled_windows.as_slice(),
        passed,
    })
}

pub fn itl_eval_state_for_runtime_player<
    RuntimeProfile,
    OverlayActor,
    CapturedActor,
    StateDelta,
    S,
    A,
    R,
    G,
>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
    song_dir: Option<&str>,
    group_name: Option<&str>,
    subtitle: &str,
    mut side_for_player: S,
    active_profile_id_for_side: A,
    read_itl_file: R,
    groovestats_state_for_player: G,
) -> deadsync_score::ItlEvalState
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
    S: FnMut(usize, usize) -> deadsync_profile::PlayerSide,
    A: FnOnce(deadsync_profile::PlayerSide) -> Option<String>,
    R: FnOnce(&str) -> deadsync_score::ItlFileData,
    G: FnOnce(usize) -> deadsync_score::GrooveStatsEvalState,
{
    if player_idx >= gs.num_players().min(deadsync_core::input::MAX_PLAYERS) {
        return deadsync_score::ItlEvalState::default();
    }
    let side = side_for_player(gs.num_players(), player_idx);
    let Some(profile_id) = active_profile_id_for_side(side) else {
        return deadsync_score::ItlEvalState::default();
    };
    let data = read_itl_file(profile_id.as_str());
    let groovestats_state = groovestats_state_for_player(player_idx);
    itl_eval_state_from_runtime(
        gs,
        player_idx,
        &data,
        song_dir,
        group_name,
        subtitle,
        &groovestats_state,
    )
}

pub fn itl_eval_state_from_app_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
) -> deadsync_score::ItlEvalState
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
{
    let song = gs.song();
    let song_dir = deadsync_score::itl_song_dir(song);
    let group_name = deadsync_simfile::runtime_cache::song_pack_group_for_song(song);
    itl_eval_state_for_runtime_player(
        gs,
        player_idx,
        song_dir.as_deref(),
        group_name.as_deref(),
        song.display_subtitle(false),
        deadsync_profile::app_runtime::gameplay_side_for_player,
        deadsync_profile::runtime_active_local_profile_id_for_side,
        deadsync_profile::app_runtime::read_itl_file_for_id,
        |idx| groovestats_eval_state_from_app_runtime(gs, idx),
    )
}

pub fn should_warn_itl_cmod_from_runtime<
    RuntimeProfile,
    OverlayActor,
    CapturedActor,
    StateDelta,
    S,
    A,
    W,
>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
    song_dir: Option<&str>,
    group_name: Option<&str>,
    subtitle: &str,
    mut side_for_player: S,
    active_profile_id_for_side: A,
    should_warn: W,
) -> bool
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
    S: FnMut(usize, usize) -> deadsync_profile::PlayerSide,
    A: FnOnce(deadsync_profile::PlayerSide) -> Option<String>,
    W: FnOnce(Option<&str>, Option<&str>, Option<&str>, &str, &str) -> bool,
{
    if player_idx >= gs.num_players().min(deadsync_core::input::MAX_PLAYERS)
        || gs.course_display_is_course_stage()
        || !deadsync_score::groovestats_used_cmod(gs.profiles()[player_idx].scroll_speed)
    {
        return false;
    }

    let side = side_for_player(gs.num_players(), player_idx);
    let profile_id = active_profile_id_for_side(side);
    should_warn(
        profile_id.as_deref(),
        song_dir,
        group_name,
        gs.charts()[player_idx].short_hash.as_str(),
        subtitle,
    )
}

pub fn should_warn_itl_cmod_from_app_runtime<
    RuntimeProfile,
    OverlayActor,
    CapturedActor,
    StateDelta,
>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
) -> bool
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
{
    let song = gs.song();
    let song_dir = deadsync_score::itl_song_dir(song);
    // Pack identity is encoded by the catalog layout and Gameplay derives it
    // the same way during setup. Borrow it from the path instead of scanning
    // every loaded simfile to rediscover the current song's pack.
    let group_name = deadsync_simfile::event_intro::song_pack_group(song);
    should_warn_itl_cmod_from_runtime(
        gs,
        player_idx,
        song_dir.as_deref(),
        group_name,
        song.display_subtitle(false),
        deadsync_profile::app_runtime::gameplay_side_for_player,
        deadsync_profile::runtime_active_local_profile_id_for_side,
        deadsync_profile::app_runtime::should_warn_itl_cmod,
    )
}

pub fn itl_save_player_from_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
    profile_id: String,
    song_dir: Option<String>,
    group_name: Option<String>,
    subtitle: String,
    date: String,
    groovestats_state: deadsync_score::GrooveStatsEvalState,
) -> deadsync_score::ItlGameplaySavePlayer
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
{
    let profile = gs.profiles()[player_idx].deref();
    let chart = gs.charts()[player_idx].as_ref();
    let disabled_windows = profile.timing_windows.disabled_windows();
    let outcome = gs.individual_song_outcome(player_idx);
    let passed = deadsync_score::gameplay_run_passed(
        outcome.song_completed_naturally,
        outcome.is_failing,
        outcome.life,
        outcome.fail_time.is_some(),
    );
    deadsync_score::ItlGameplaySavePlayer {
        player_idx,
        profile_id,
        song_dir,
        event_name: group_name,
        chart_hash: chart.short_hash.clone(),
        chart_name: chart.chart_name.clone(),
        chart_type: chart.chart_type.clone(),
        subtitle,
        used_cmod: deadsync_score::groovestats_used_cmod(profile.scroll_speed),
        groovestats_valid: groovestats_state.valid,
        groovestats_reason_lines: groovestats_state.reason_lines,
        music_rate: gs.music_rate(),
        remove_mask: profile.remove_active_mask.bits(),
        disabled_windows,
        passed,
        judgments: itl_judgments_from_runtime(gs, player_idx),
        ex_percent: deadsync_score::itl_ex_score_percent(itl_score_calc_input_from_runtime(
            gs, player_idx,
        )),
        date,
    }
}

pub fn save_itl_data_from_runtime<
    RuntimeProfile,
    OverlayActor,
    CapturedActor,
    StateDelta,
    A,
    G,
    S,
    L,
    B,
>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    song_dir: Option<String>,
    group_name: Option<String>,
    subtitle: String,
    date: String,
    mut active_profile_id_for_player: A,
    mut groovestats_state_for_player: G,
    save_players: S,
    log_skip: L,
    log_autoplay_skip: B,
) -> [Option<deadsync_score::ItlEventProgress>; deadsync_core::input::MAX_PLAYERS]
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
    A: FnMut(usize, usize) -> Option<String>,
    G: FnMut(usize) -> deadsync_score::GrooveStatsEvalState,
    S: FnOnce(
        Vec<deadsync_score::ItlGameplaySavePlayer>,
        L,
    ) -> Vec<deadsync_score::ItlGameplaySaveProgress>,
    L: for<'a> FnMut(deadsync_score::ItlGameplaySaveSkip<'a>),
    B: FnOnce(),
{
    let mut progress: [Option<deadsync_score::ItlEventProgress>;
        deadsync_core::input::MAX_PLAYERS] = std::array::from_fn(|_| None);
    if gs.autoplay_used() {
        log_autoplay_skip();
        return progress;
    }

    let players = (0..gs.num_players().min(deadsync_core::input::MAX_PLAYERS))
        .filter_map(|player_idx| {
            let profile_id = active_profile_id_for_player(gs.num_players(), player_idx)?;
            Some(itl_save_player_from_runtime(
                gs,
                player_idx,
                profile_id,
                song_dir.clone(),
                group_name.clone(),
                subtitle.clone(),
                date.clone(),
                groovestats_state_for_player(player_idx),
            ))
        })
        .collect::<Vec<_>>();

    for result in save_players(players, log_skip) {
        progress[result.player_idx] = Some(result.progress);
    }

    progress
}

pub fn save_itl_data_from_app_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
) -> [Option<deadsync_score::ItlEventProgress>; deadsync_core::input::MAX_PLAYERS]
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
{
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let song = gs.song();
    let song_dir = deadsync_score::itl_song_dir(song);
    let group_name = deadsync_simfile::runtime_cache::song_pack_group_for_song(song);
    let subtitle = song.display_subtitle(false).to_string();
    save_itl_data_from_runtime(
        gs,
        song_dir,
        group_name,
        subtitle,
        date,
        |num_players, player_idx| {
            deadsync_profile::app_runtime::active_local_profile_id_for_gameplay_player(
                num_players,
                player_idx,
            )
            .map(|(_, profile_id)| profile_id)
        },
        |player_idx| groovestats_eval_state_from_app_runtime(gs, player_idx),
        deadsync_profile::app_runtime::save_itl_gameplay_players,
        |skip| {
            let side = deadsync_profile::app_runtime::gameplay_side_for_player(
                gs.num_players(),
                skip.player_idx,
            );
            log::debug!(
                "Skipping ITL save for {:?} ({}): {}",
                side,
                skip.chart_hash,
                skip.reason_lines.join("; ")
            );
        },
        || log::debug!("Skipping ITL save: autoplay or replay was used during this stage."),
    )
}

pub fn local_score_player_from_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    player_idx: usize,
    profile_id: String,
) -> deadsync_score::LocalScoreGameplayPlayer<'_>
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
{
    let player = &gs.players()[player_idx];
    let profile = gs.profiles()[player_idx].deref();
    let chart = gs.charts()[player_idx].as_ref();
    let totals = gs.stage_totals_for_player(player_idx);
    let outcome = gs.individual_song_outcome(player_idx);
    let invalid_reasons = if gs.score_valid_for_player(player_idx) {
        Vec::new()
    } else {
        score_invalid_reason_lines_for_profile(chart, profile, gs.music_rate())
    };
    let (start, end) = gs.note_range_for_player(player_idx);
    let replay = deadsync_score::local_replay_edges_for_player(
        gs.recorded_replay_edges()
            .iter()
            .map(|edge| deadsync_score::LocalReplayEdgeInput {
                event_music_time_ns: edge.event_music_time_ns,
                lane_index: edge.lane_index,
                pressed: edge.pressed,
                source: edge.source,
            }),
        player_idx,
        gs.num_players(),
        gs.num_cols(),
        gs.cols_per_player(),
    );

    deadsync_score::LocalScoreGameplayPlayer {
        player_idx,
        profile_id,
        profile_initials: profile.player_initials.as_str(),
        chart_hash: chart.short_hash.as_str(),
        invalid_reasons,
        score_valid: gs.score_valid_for_player(player_idx),
        scoring_counts: &player.scoring_counts,
        holds_held_for_score: player.holds_held_for_score,
        rolls_held_for_score: player.rolls_held_for_score,
        mines_hit_for_score: player.mines_hit_for_score,
        possible_grade_points: totals.possible_grade_points,
        song_completed_naturally: outcome.song_completed_naturally,
        is_failing: outcome.is_failing,
        life: outcome.life,
        fail_time: outcome.fail_time,
        notes: &gs.notes()[start..end],
        note_times: &gs.note_time_cache_ns()[start..end],
        hold_end_times: &gs.hold_end_time_cache_ns()[start..end],
        total_steps: totals.total_steps,
        holds_total: totals.holds_total,
        rolls_total: totals.rolls_total,
        mines_total: totals.mines_total,
        counts: player.judgment_counts,
        white_fantastics: Some(gs.live_window_counts(player_idx).w1),
        holds_held: player.holds_held,
        rolls_held: player.rolls_held,
        mines_avoided: player.mines_avoided,
        hands_achieved: player.hands_achieved,
        beat0_time_ns: gs
            .timing_for_player(player_idx)
            .map(|timing| timing.get_time_for_beat_ns(0.0))
            .unwrap_or(0),
        replay,
    }
}

/// FillInRoutineHighScore: the Routine score that both participants share,
/// saved once for the master player.
fn routine_local_score_player_from_runtime<'a, RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &'a deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    profile_id: String,
    scoring_counts: &'a deadsync_rules::judgment::JudgeCounts,
) -> deadsync_score::LocalScoreGameplayPlayer<'a>
where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
{
    // Init appends the second participant's notes right after the first's.
    let (start, first_end) = gs.note_range_for_player(0);
    let (second_start, end) = gs.note_range_for_player(1);
    debug_assert_eq!(first_end, second_start);
    deadsync_score::routine_shared_local_score_player(
        local_score_player_from_runtime(gs, 0, profile_id),
        local_score_player_from_runtime(gs, 1, String::new()),
        scoring_counts,
        &gs.notes()[start..end],
        &gs.note_time_cache_ns()[start..end],
        &gs.hold_end_time_cache_ns()[start..end],
    )
}

pub fn save_local_scores_from_runtime<
    RuntimeProfile,
    OverlayActor,
    CapturedActor,
    StateDelta,
    A,
    W,
    L,
>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
    mut active_profile_id_for_player: A,
    write_score: W,
    log_skip: L,
) where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
    A: FnMut(usize, usize) -> Option<String>,
    W: FnMut(&str, &str, &str, &mut deadsync_score::LocalScoreEntry) -> bool,
    L: FnMut(deadsync_score::LocalScoreGameplaySaveSkip),
{
    let played_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    // StageStats::CommitScores fills only the master player's high score in
    // Routine, from the shared stats both participants earned together.
    let routine_counts = gs.setup.session.play_style.is_routine().then(|| {
        let [first, second] = [&gs.players()[0].scoring_counts, &gs.players()[1].scoring_counts];
        std::array::from_fn(|ix| first[ix].saturating_add(second[ix]))
    });
    let players: Vec<_> = if let Some(counts) = routine_counts.as_ref() {
        active_profile_id_for_player(gs.num_players(), 0)
            .map(|profile_id| routine_local_score_player_from_runtime(gs, profile_id, counts))
            .into_iter()
            .collect()
    } else {
        (0..gs.num_players())
            .filter_map(|player_idx| {
                let profile_id = active_profile_id_for_player(gs.num_players(), player_idx)?;
                Some(local_score_player_from_runtime(gs, player_idx, profile_id))
            })
            .collect()
    };
    deadsync_score::save_local_gameplay_scores(
        played_at_ms,
        gs.music_rate(),
        gs.autoplay_used(),
        players,
        write_score,
        log_skip,
    );
}

pub fn save_local_scores_from_app_runtime<RuntimeProfile, OverlayActor, CapturedActor, StateDelta>(
    gs: &deadsync_gameplay::GameplayRuntimeState<
        RuntimeProfile,
        OverlayActor,
        CapturedActor,
        StateDelta,
    >,
) where
    RuntimeProfile:
        Deref<Target = deadsync_profile::Profile> + deadsync_gameplay::GameplayProfileData,
{
    for player in 0..gs.num_players() {
        let Some(histories) = gs.ghost_history_for_save(player) else {
            continue;
        };
        let Some((_, id)) =
            deadsync_profile::app_runtime::active_local_profile_id_for_gameplay_player(
                gs.num_players(),
                player,
            )
        else {
            continue;
        };
        let Some(chart) = gs.chart(player) else {
            continue;
        };
        let dir = deadsync_profile::app_runtime::local_profile_dir(&id);
        if let Err(error) = deadsync_score::ghost::save_best(&dir, &chart.short_hash, histories) {
            log::warn!(
                "Could not save ghost history for {}: {error}",
                chart.short_hash
            );
        }
    }
    save_local_scores_from_runtime(
        gs,
        |num_players, player_idx| {
            deadsync_profile::app_runtime::active_local_profile_id_for_gameplay_player(
                num_players,
                player_idx,
            )
            .map(|(_, profile_id)| profile_id)
        },
        deadsync_profile::app_runtime::append_local_score_for_id,
        |skip| match skip {
            deadsync_score::LocalScoreGameplaySaveSkip::Autoplay => {
                log::debug!("Skipping local score save: autoplay was used during this stage.");
            }
            deadsync_score::LocalScoreGameplaySaveSkip::Invalid { player_idx, detail } => {
                log::debug!(
                    "Skipping local score save for player {}: {}.",
                    player_idx + 1,
                    detail
                );
            }
        },
    );
}
