use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(test)]
mod song_lua_runtime_tests;

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

pub fn gameplay_play_style_from_profile(
    play_style: deadsync_profile::PlayStyle,
) -> deadsync_gameplay::GameplayInputPlayStyle {
    match play_style {
        deadsync_profile::PlayStyle::Single => deadsync_gameplay::GameplayInputPlayStyle::Single,
        deadsync_profile::PlayStyle::Versus => deadsync_gameplay::GameplayInputPlayStyle::Versus,
        deadsync_profile::PlayStyle::Double => deadsync_gameplay::GameplayInputPlayStyle::Double,
    }
}

pub fn gameplay_player_side_from_profile(
    side: deadsync_profile::PlayerSide,
) -> deadsync_gameplay::GameplayInputPlayerSide {
    match side {
        deadsync_profile::PlayerSide::P1 => deadsync_gameplay::GameplayInputPlayerSide::P1,
        deadsync_profile::PlayerSide::P2 => deadsync_gameplay::GameplayInputPlayerSide::P2,
    }
}

pub fn profile_side_from_gameplay(
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

pub fn song_pack_group(song: &deadsync_chart::SongData) -> Arc<str> {
    Arc::from(
        song.simfile_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned(),
    )
}

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
        pack_banner_path = pack.banner_path.clone();
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

pub fn gameplay_runtime_profile_data(
    player_profiles: &[deadsync_profile::Profile; deadsync_core::input::MAX_PLAYERS],
    session: &deadsync_gameplay::GameplaySession,
) -> [deadsync_profile::Profile; deadsync_core::input::MAX_PLAYERS] {
    let mut runtime_profiles = (*player_profiles).clone();
    if session.p2_runtime_player() {
        runtime_profiles[0] = runtime_profiles[1].clone();
    }
    runtime_profiles
}

pub fn gameplay_tick_mode_from_profile(
    mode: deadsync_profile::TimingTickMode,
) -> deadsync_gameplay::GameplayTimingTickMode {
    match mode {
        deadsync_profile::TimingTickMode::Off => deadsync_gameplay::GameplayTimingTickMode::Off,
        deadsync_profile::TimingTickMode::Assist => {
            deadsync_gameplay::GameplayTimingTickMode::Assist
        }
        deadsync_profile::TimingTickMode::Hit => deadsync_gameplay::GameplayTimingTickMode::Hit,
    }
}

pub fn profile_tick_mode_from_gameplay(
    mode: deadsync_gameplay::GameplayTimingTickMode,
) -> deadsync_profile::TimingTickMode {
    match mode {
        deadsync_gameplay::GameplayTimingTickMode::Off => deadsync_profile::TimingTickMode::Off,
        deadsync_gameplay::GameplayTimingTickMode::Assist => {
            deadsync_profile::TimingTickMode::Assist
        }
        deadsync_gameplay::GameplayTimingTickMode::Hit => deadsync_profile::TimingTickMode::Hit,
    }
}

pub fn gameplay_fail_type_from_config(
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

pub fn gameplay_config_from_config(
    cfg: &deadsync_config::app_config::Config,
) -> deadsync_gameplay::GameplayConfig {
    deadsync_gameplay::GameplayConfig {
        mine_hit_sound: cfg.mine_hit_sound,
        default_fail_type: gameplay_fail_type_from_config(cfg.default_fail_type),
        global_offset_seconds: cfg.global_offset_seconds,
        visual_delay_seconds: cfg.visual_delay_seconds,
        machine_pack_ini_offsets: cfg.machine_pack_ini_offsets,
        machine_default_sync_pref: cfg.machine_default_sync_offset.sync_pref(),
        machine_allow_per_player_global_offsets: cfg.machine_allow_per_player_global_offsets,
        machine_enable_replays: cfg.machine_enable_replays,
        center_1player_notefield: cfg.center_1player_notefield,
        delayed_back: cfg.delayed_back,
    }
}

pub fn score_display_mode_from_profile(
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

fn gameplay_target_score_setting(
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
        deadsync_profile::TargetScoreSetting::MachineBest => {
            deadsync_gameplay::GameplayTargetScoreSetting::MachineBest
        }
        deadsync_profile::TargetScoreSetting::PersonalBest => {
            deadsync_gameplay::GameplayTargetScoreSetting::PersonalBest
        }
    }
}

pub fn gameplay_attack_mode(
    mode: deadsync_profile::AttackMode,
) -> deadsync_gameplay::GameplayAttackMode {
    match mode {
        deadsync_profile::AttackMode::Off => deadsync_gameplay::GameplayAttackMode::Off,
        deadsync_profile::AttackMode::On => deadsync_gameplay::GameplayAttackMode::On,
        deadsync_profile::AttackMode::Random => deadsync_gameplay::GameplayAttackMode::Random,
    }
}

pub fn chart_effects_from_profile(
    profile: &deadsync_profile::Profile,
) -> deadsync_gameplay::ChartAttackEffects {
    deadsync_gameplay::ChartAttackEffects {
        insert_mask: profile.insert_active_mask.bits(),
        remove_mask: profile.remove_active_mask.bits(),
        holds_mask: profile.holds_active_mask.bits(),
        turn_bits: 0,
    }
}

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

pub type SongLuaRuntimeOverlayStateDelta =
    deadsync_gameplay::SongLuaRuntimeOverlayStateDelta<deadsync_song_lua::SongLuaOverlayStateDelta>;

pub fn song_lua_difficulty_from_chart(difficulty: &str) -> deadsync_song_lua::SongLuaDifficulty {
    if difficulty.eq_ignore_ascii_case("beginner") {
        deadsync_song_lua::SongLuaDifficulty::Beginner
    } else if difficulty.eq_ignore_ascii_case("easy") || difficulty.eq_ignore_ascii_case("basic") {
        deadsync_song_lua::SongLuaDifficulty::Easy
    } else if difficulty.eq_ignore_ascii_case("medium")
        || difficulty.eq_ignore_ascii_case("standard")
    {
        deadsync_song_lua::SongLuaDifficulty::Medium
    } else if difficulty.eq_ignore_ascii_case("hard")
        || difficulty.eq_ignore_ascii_case("difficult")
    {
        deadsync_song_lua::SongLuaDifficulty::Hard
    } else if difficulty.eq_ignore_ascii_case("edit") {
        deadsync_song_lua::SongLuaDifficulty::Edit
    } else {
        deadsync_song_lua::SongLuaDifficulty::Challenge
    }
}

pub const fn song_lua_speedmod_from_setting(
    speed: deadsync_rules::scroll::ScrollSpeedSetting,
) -> deadsync_song_lua::SongLuaSpeedMod {
    match speed {
        deadsync_rules::scroll::ScrollSpeedSetting::XMod(value) => {
            deadsync_song_lua::SongLuaSpeedMod::X(value)
        }
        deadsync_rules::scroll::ScrollSpeedSetting::CMod(value) => {
            deadsync_song_lua::SongLuaSpeedMod::C(value)
        }
        deadsync_rules::scroll::ScrollSpeedSetting::MMod(value) => {
            deadsync_song_lua::SongLuaSpeedMod::M(value)
        }
    }
}

pub const fn song_lua_compile_play_style(
    play_style: deadsync_gameplay::GameplayInputPlayStyle,
) -> deadsync_gameplay::SongLuaCompilePlayStyle {
    match play_style {
        deadsync_gameplay::GameplayInputPlayStyle::Single => {
            deadsync_gameplay::SongLuaCompilePlayStyle::Single
        }
        deadsync_gameplay::GameplayInputPlayStyle::Versus => {
            deadsync_gameplay::SongLuaCompilePlayStyle::Versus
        }
        deadsync_gameplay::GameplayInputPlayStyle::Double => {
            deadsync_gameplay::SongLuaCompilePlayStyle::Double
        }
    }
}

pub const fn song_lua_runtime_time_unit(
    unit: deadsync_song_lua::SongLuaTimeUnit,
) -> deadsync_gameplay::SongLuaRuntimeTimeUnit {
    match unit {
        deadsync_song_lua::SongLuaTimeUnit::Beat => deadsync_gameplay::SongLuaRuntimeTimeUnit::Beat,
        deadsync_song_lua::SongLuaTimeUnit::Second => {
            deadsync_gameplay::SongLuaRuntimeTimeUnit::Second
        }
    }
}

pub const fn song_lua_runtime_span_mode(
    span_mode: deadsync_song_lua::SongLuaSpanMode,
) -> deadsync_gameplay::SongLuaRuntimeSpanMode {
    match span_mode {
        deadsync_song_lua::SongLuaSpanMode::Len => deadsync_gameplay::SongLuaRuntimeSpanMode::Len,
        deadsync_song_lua::SongLuaSpanMode::End => deadsync_gameplay::SongLuaRuntimeSpanMode::End,
    }
}

pub fn song_lua_runtime_ease_target(
    target: &deadsync_song_lua::SongLuaEaseTarget,
) -> deadsync_gameplay::SongLuaRuntimeEaseTargetOwned {
    match target {
        deadsync_song_lua::SongLuaEaseTarget::Mod(target_name) => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Mod(target_name.clone())
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerX,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerY,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerZ => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZ,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerRotationX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationX,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerRotationY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationY,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerRotationZ => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationZ,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerSkewX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerSkewX,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerSkewY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerSkewY,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerZoom => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoom,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerZoomX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomX,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerZoomY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomY,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerZoomZ => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomZ,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::Function => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Function
        }
    }
}

pub fn song_lua_runtime_mod_windows(
    windows: &[deadsync_song_lua::SongLuaModWindow],
) -> Vec<deadsync_gameplay::SongLuaRuntimeModWindow> {
    windows
        .iter()
        .map(|window| deadsync_gameplay::SongLuaRuntimeModWindow {
            player: window.player,
            unit: song_lua_runtime_time_unit(window.unit),
            start: window.start,
            limit: window.limit,
            span_mode: song_lua_runtime_span_mode(window.span_mode),
            mods: window.mods.clone(),
        })
        .collect()
}

pub fn song_lua_runtime_ease_windows(
    windows: &[deadsync_song_lua::SongLuaEaseWindow],
) -> Vec<deadsync_gameplay::SongLuaRuntimeEaseWindow> {
    windows
        .iter()
        .map(|window| deadsync_gameplay::SongLuaRuntimeEaseWindow {
            player: window.player,
            unit: song_lua_runtime_time_unit(window.unit),
            start: window.start,
            limit: window.limit,
            span_mode: song_lua_runtime_span_mode(window.span_mode),
            target: song_lua_runtime_ease_target(&window.target),
            from: window.from,
            to: window.to,
            easing: window.easing.clone(),
            sustain: window.sustain,
            opt1: window.opt1,
            opt2: window.opt2,
        })
        .collect()
}

pub fn song_lua_runtime_column_offset_windows(
    windows: &[deadsync_song_lua::SongLuaColumnOffsetWindow],
) -> Vec<deadsync_gameplay::SongLuaRuntimeColumnOffsetWindow> {
    windows
        .iter()
        .map(
            |window| deadsync_gameplay::SongLuaRuntimeColumnOffsetWindow {
                player: window.player,
                unit: song_lua_runtime_time_unit(window.unit),
                start: window.start,
                limit: window.limit,
                span_mode: song_lua_runtime_span_mode(window.span_mode),
                column: window.column,
                from_y: window.from_y,
                to_y: window.to_y,
                easing: window.easing.clone(),
                sustain: window.sustain,
                opt1: window.opt1,
                opt2: window.opt2,
            },
        )
        .collect()
}

pub fn song_lua_overlay_delta_mask(
    delta: &deadsync_song_lua::SongLuaOverlayStateDelta,
) -> deadsync_gameplay::SongLuaOverlayDeltaMask {
    let mut mask = 0u128;
    let mut bit = 0u32;
    macro_rules! field {
        ($field:ident) => {{
            if delta.$field.is_some() {
                mask |= 1u128 << bit;
            }
            bit += 1;
        }};
    }

    field!(x);
    field!(y);
    field!(z);
    field!(z_bias);
    field!(draw_order);
    field!(draw_by_z_position);
    field!(halign);
    field!(valign);
    field!(text_align);
    field!(uppercase);
    field!(shadow_len);
    field!(shadow_color);
    field!(glow);
    field!(fov);
    field!(vanishpoint);
    field!(diffuse);
    field!(vertex_colors);
    field!(visible);
    field!(cropleft);
    field!(cropright);
    field!(croptop);
    field!(cropbottom);
    field!(fadeleft);
    field!(faderight);
    field!(fadetop);
    field!(fadebottom);
    field!(mask_source);
    field!(mask_dest);
    field!(depth_test);
    field!(zoom);
    field!(zoom_x);
    field!(zoom_y);
    field!(zoom_z);
    field!(basezoom);
    field!(basezoom_x);
    field!(basezoom_y);
    field!(basezoom_z);
    field!(rot_x_deg);
    field!(rot_y_deg);
    field!(rot_z_deg);
    field!(skew_x);
    field!(skew_y);
    field!(blend);
    field!(vibrate);
    field!(effect_magnitude);
    field!(effect_clock);
    field!(effect_mode);
    field!(effect_color1);
    field!(effect_color2);
    field!(effect_period);
    field!(effect_offset);
    field!(effect_timing);
    field!(rainbow);
    field!(rainbow_scroll);
    field!(text_jitter);
    field!(text_distortion);
    field!(text_glow_mode);
    field!(mult_attrs_with_diffuse);
    field!(sprite_animate);
    field!(sprite_loop);
    field!(sprite_playback_rate);
    field!(sprite_state_delay);
    field!(sprite_state_index);
    field!(vert_spacing);
    field!(wrap_width_pixels);
    field!(max_width);
    field!(max_height);
    field!(max_w_pre_zoom);
    field!(max_h_pre_zoom);
    field!(max_dimension_uses_zoom);
    field!(texture_filtering);
    field!(texture_wrapping);
    field!(texcoord_offset);
    field!(custom_texture_rect);
    field!(texcoord_velocity);
    field!(size);
    field!(stretch_rect);
    field!(sound_play);

    let _ = bit;
    mask
}

pub fn song_lua_runtime_overlay_state_delta(
    delta: deadsync_song_lua::SongLuaOverlayStateDelta,
) -> SongLuaRuntimeOverlayStateDelta {
    SongLuaRuntimeOverlayStateDelta {
        overlap_mask: song_lua_overlay_delta_mask(&delta),
        delta,
    }
}

pub fn song_lua_runtime_overlay_ease_window(
    ease: &deadsync_song_lua::SongLuaOverlayEase,
) -> deadsync_gameplay::SongLuaRuntimeOverlayEaseWindow<SongLuaRuntimeOverlayStateDelta> {
    deadsync_gameplay::SongLuaRuntimeOverlayEaseWindow {
        overlay_index: ease.overlay_index,
        unit: song_lua_runtime_time_unit(ease.unit),
        start: ease.start,
        limit: ease.limit,
        span_mode: song_lua_runtime_span_mode(ease.span_mode),
        sustain: ease.sustain,
        from: song_lua_runtime_overlay_state_delta(ease.from),
        to: song_lua_runtime_overlay_state_delta(ease.to),
        easing: ease.easing.clone(),
        opt1: ease.opt1,
        opt2: ease.opt2,
    }
}

pub fn build_song_lua_constant_windows_for_player<OverlayActor>(
    compiled: &deadsync_song_lua::CompiledSongLua<OverlayActor>,
    timing_player: &deadsync_rules::timing::TimingData,
    player: usize,
    global_offset_seconds: f32,
) -> Vec<deadsync_gameplay::AttackMaskWindow> {
    let time_mods = song_lua_runtime_mod_windows(&compiled.time_mods);
    let beat_mods = song_lua_runtime_mod_windows(&compiled.beat_mods);
    deadsync_gameplay::build_song_lua_constant_windows_for_player(
        &time_mods,
        &beat_mods,
        timing_player,
        player,
        global_offset_seconds,
    )
}

pub fn build_song_lua_ease_windows_for_player<OverlayActor>(
    compiled: &deadsync_song_lua::CompiledSongLua<OverlayActor>,
    timing_player: &deadsync_rules::timing::TimingData,
    player: usize,
    global_offset_seconds: f32,
    constant_windows: &[deadsync_gameplay::AttackMaskWindow],
) -> (Vec<deadsync_gameplay::SongLuaEaseMaskWindow>, usize) {
    let eases = song_lua_runtime_ease_windows(&compiled.eases);
    deadsync_gameplay::build_song_lua_ease_windows_for_player(
        &eases,
        timing_player,
        player,
        global_offset_seconds,
        constant_windows,
        |_| {},
    )
}

pub fn build_song_lua_column_offset_windows_for_player<OverlayActor>(
    compiled: &deadsync_song_lua::CompiledSongLua<OverlayActor>,
    timing_player: &deadsync_rules::timing::TimingData,
    player: usize,
    global_offset_seconds: f32,
) -> Vec<deadsync_gameplay::SongLuaColumnOffsetWindowRuntime> {
    let column_offsets = song_lua_runtime_column_offset_windows(&compiled.column_offsets);
    deadsync_gameplay::build_song_lua_column_offset_windows_for_player(
        &column_offsets,
        timing_player,
        player,
        global_offset_seconds,
    )
}

pub fn build_song_lua_actor_message_events_for_commands(
    messages: &[deadsync_song_lua::SongLuaMessageEvent],
    message_seconds: &[Option<f32>],
    commands: &[deadsync_song_lua::SongLuaOverlayMessageCommand],
) -> Vec<deadsync_gameplay::SongLuaOverlayMessageRuntime> {
    deadsync_gameplay::build_song_lua_actor_message_events_with_seconds(
        messages
            .iter()
            .enumerate()
            .map(|(idx, message)| (idx, message.message.as_str())),
        message_seconds,
        commands
            .iter()
            .enumerate()
            .map(|(idx, command)| (idx, command.message.as_str())),
    )
}

pub fn build_song_lua_overlay_message_events_with_seconds<Kind>(
    compiled: &deadsync_song_lua::CompiledSongLua<deadsync_song_lua::SongLuaOverlayActor<Kind>>,
    message_seconds: &[Option<f32>],
) -> Vec<Vec<deadsync_gameplay::SongLuaOverlayMessageRuntime>> {
    compiled
        .overlays
        .iter()
        .map(|overlay| {
            build_song_lua_actor_message_events_for_commands(
                &compiled.messages,
                message_seconds,
                &overlay.message_commands,
            )
        })
        .collect()
}

fn song_lua_compiled_overlay_ease_cutoff_second<Kind>(
    compiled: &deadsync_song_lua::CompiledSongLua<deadsync_song_lua::SongLuaOverlayActor<Kind>>,
    ease: &deadsync_song_lua::SongLuaOverlayEase,
    overlay_events: &[Vec<deadsync_gameplay::SongLuaOverlayMessageRuntime>],
    start_second: f32,
) -> Option<f32> {
    let overlay = compiled.overlays.get(ease.overlay_index)?;
    let events = overlay_events.get(ease.overlay_index)?;
    let from_mask = song_lua_overlay_delta_mask(&ease.from);
    let to_mask = song_lua_overlay_delta_mask(&ease.to);
    let blocks = events
        .iter()
        .filter_map(|event| {
            let command = overlay.message_commands.get(event.command_index)?;
            Some((event.event_second, command))
        })
        .flat_map(|(event_second, command)| {
            command.blocks.iter().map(move |block| {
                (
                    event_second,
                    block.start,
                    song_lua_overlay_delta_mask(&block.delta),
                )
            })
        });
    deadsync_gameplay::song_lua_overlay_ease_cutoff_second(
        start_second,
        &from_mask,
        &to_mask,
        blocks,
    )
}

pub fn build_song_lua_overlay_ease_windows_with_events<Kind>(
    compiled: &deadsync_song_lua::CompiledSongLua<deadsync_song_lua::SongLuaOverlayActor<Kind>>,
    timing_player: &deadsync_rules::timing::TimingData,
    global_offset_seconds: f32,
    overlay_events: &[Vec<deadsync_gameplay::SongLuaOverlayMessageRuntime>],
) -> Vec<deadsync_gameplay::SongLuaOverlayEaseWindowRuntime<SongLuaRuntimeOverlayStateDelta>> {
    let mut out = Vec::new();
    for ease in &compiled.overlay_eases {
        let runtime_ease = song_lua_runtime_overlay_ease_window(ease);
        if let Some(window) = deadsync_gameplay::build_song_lua_overlay_ease_window_for(
            &runtime_ease,
            timing_player,
            global_offset_seconds,
            |start_second| {
                song_lua_compiled_overlay_ease_cutoff_second(
                    compiled,
                    ease,
                    overlay_events,
                    start_second,
                )
            },
        ) {
            out.push(window);
        }
    }
    out
}

pub fn build_song_lua_overlay_ease_windows<Kind>(
    compiled: &deadsync_song_lua::CompiledSongLua<deadsync_song_lua::SongLuaOverlayActor<Kind>>,
    timing_player: &deadsync_rules::timing::TimingData,
    global_offset_seconds: f32,
) -> Vec<deadsync_gameplay::SongLuaOverlayEaseWindowRuntime<SongLuaRuntimeOverlayStateDelta>> {
    let message_seconds = deadsync_gameplay::build_song_lua_message_seconds(
        compiled.messages.iter().map(|message| message.beat),
        timing_player,
        global_offset_seconds,
    );
    let overlay_events =
        build_song_lua_overlay_message_events_with_seconds(compiled, &message_seconds);
    build_song_lua_overlay_ease_windows_with_events(
        compiled,
        timing_player,
        global_offset_seconds,
        &overlay_events,
    )
}

fn song_lua_compile_player_screen_x(
    num_players: usize,
    player_index: usize,
    profile: &deadsync_profile::Profile,
    viewport: deadsync_gameplay::GameplayViewport,
    play_style: deadsync_gameplay::GameplayInputPlayStyle,
    player_side: deadsync_gameplay::GameplayInputPlayerSide,
    center_1player_notefield: bool,
) -> f32 {
    deadsync_gameplay::song_lua_compile_player_screen_x(
        num_players,
        player_index,
        viewport,
        song_lua_compile_play_style(play_style),
        deadsync_gameplay::gameplay_is_single_p2_side(play_style, player_side),
        profile.note_field_offset_x as f32,
        center_1player_notefield,
    )
}

pub fn song_lua_compile_context(
    song: &deadsync_chart::SongData,
    charts: &[Arc<deadsync_chart::ChartData>; deadsync_core::input::MAX_PLAYERS],
    num_players: usize,
    player_profiles: &[deadsync_profile::Profile; deadsync_core::input::MAX_PLAYERS],
    scroll_speed: &[deadsync_rules::scroll::ScrollSpeedSetting; deadsync_core::input::MAX_PLAYERS],
    music_rate: f32,
    machine_global_offset_seconds: f32,
    viewport: deadsync_gameplay::GameplayViewport,
    session: &deadsync_gameplay::GameplaySession,
    center_1player_notefield: bool,
) -> deadsync_song_lua::SongLuaCompileContext {
    let play_style = session.play_style;
    let player_side = session.player_side;
    let mut context = deadsync_song_lua::SongLuaCompileContext::new(
        song.simfile_path
            .parent()
            .map(|path| path.to_path_buf())
            .unwrap_or_default(),
        song.title.clone(),
    );
    context.song_display_bpms =
        song.display_bpm_pair_or(charts.first().map(|chart| chart.as_ref()), [60.0, 60.0]);
    context.song_music_rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    context.music_length_seconds = song.music_length_seconds.max(song.precise_last_second());
    context.style_name = match play_style {
        deadsync_gameplay::GameplayInputPlayStyle::Single => "single",
        deadsync_gameplay::GameplayInputPlayStyle::Versus => "versus",
        deadsync_gameplay::GameplayInputPlayStyle::Double => "double",
    }
    .to_string();
    context.global_offset_seconds = machine_global_offset_seconds;
    context.screen_width = viewport.width();
    context.screen_height = viewport.height();
    context.confusion_offset_available = true;
    context.confusion_available = true;
    context.amod_available = false;
    context.players = std::array::from_fn(|player| deadsync_song_lua::SongLuaPlayerContext {
        enabled: player < num_players,
        difficulty: if player < num_players {
            song_lua_difficulty_from_chart(&charts[player].difficulty)
        } else {
            deadsync_song_lua::SongLuaDifficulty::default_enabled()
        },
        display_bpms: if player < num_players {
            song.display_bpm_pair_or(Some(charts[player].as_ref()), [60.0, 60.0])
        } else {
            [60.0, 60.0]
        },
        speedmod: if player < num_players {
            song_lua_speedmod_from_setting(scroll_speed[player])
        } else {
            deadsync_song_lua::SongLuaSpeedMod::default()
        },
        noteskin_name: if player < num_players {
            player_profiles[player].noteskin.to_string()
        } else {
            deadsync_profile::NoteSkin::default().to_string()
        },
        screen_x: song_lua_compile_player_screen_x(
            num_players,
            player,
            &player_profiles[player],
            viewport,
            play_style,
            player_side,
            center_1player_notefield,
        ),
        screen_y: viewport.center_y(),
    });
    context
}

pub fn groovestats_eval_state_from_profile(
    chart: &deadsync_chart::ChartData,
    profile: &deadsync_profile::Profile,
    music_rate: f32,
    autoplay_used: bool,
    is_course_mode: bool,
    course_submit_allowed: bool,
    fail_type_ok: bool,
) -> deadsync_score::GrooveStatsEvalState {
    deadsync_score::groovestats_eval_state_from_parts(deadsync_score::GrooveStatsEvalInput {
        chart_type: chart.chart_type.as_str(),
        music_rate,
        remove_mask: profile.remove_active_mask.bits(),
        insert_mask: profile.insert_active_mask.bits(),
        holds_mask: profile.holds_active_mask.bits(),
        fail_type_ok,
        autoplay_used,
        is_course_mode,
        course_submit_allowed,
        custom_fantastic_window: profile.custom_fantastic_window,
        custom_fantastic_window_ms: profile.custom_fantastic_window_ms,
    })
}

pub fn groovestats_submit_invalid_reason_from_profile(
    chart: &deadsync_chart::ChartData,
    song_has_lua: bool,
    lua_submit_allowed: bool,
    profile: &deadsync_profile::Profile,
    music_rate: f32,
    fail_type_ok: bool,
) -> Option<String> {
    if song_has_lua && !lua_submit_allowed {
        return Some("simfile relies on lua".to_string());
    }
    groovestats_eval_state_from_profile(
        chart,
        profile,
        music_rate,
        false,
        false,
        false,
        fail_type_ok,
    )
    .reason_lines
    .into_iter()
    .next()
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
            lua_submit_allowed: deadsync_score::lua_chart_submit_allowed(chart.short_hash.as_str()),
            song_completed_naturally: gs.song_completed_naturally(),
            is_failing: gs.players()[player_idx].is_failing,
            life: gs.players()[player_idx].life,
            has_fail_time: gs.players()[player_idx].fail_time.is_some(),
            course_stage_life_submit_eligible: gs.course_stage_life_submit_eligible(player_idx),
        },
    );
    result.state
}

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

pub fn tap_explosion_options_from_profile(
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

fn gameplay_turn_option(
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

fn mini_indicator_mode(
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

fn error_bar_trim(trim: deadsync_profile::ErrorBarTrim) -> deadsync_gameplay::GameplayErrorBarTrim {
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

    fn step_statistics_density_graph(&self) -> bool {
        self.step_statistics
            .contains(deadsync_profile::StepStatisticsMask::DENSITY_GRAPH)
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

    fn nps_graph_at_top(&self) -> bool {
        self.nps_graph_at_top
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
    let totals = gs.display_totals_for_player(player_idx);
    deadsync_score::ItlScoreCalcInput {
        notes: &gs.notes()[start..end],
        note_times: &gs.note_time_cache_ns()[start..end],
        hold_end_times: &gs.hold_end_time_cache_ns()[start..end],
        total_steps: totals.total_steps,
        holds_total: totals.holds_total,
        rolls_total: totals.rolls_total,
        mines_total: totals.mines_total,
        fail_time: gs.players()[player_idx]
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
    let totals = gs.display_totals_for_player(player_idx);
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
    let passed = deadsync_score::gameplay_run_passed(
        gs.song_completed_naturally(),
        gs.players()[player_idx].is_failing,
        gs.players()[player_idx].life,
        gs.players()[player_idx].fail_time.is_some(),
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
    let group_name = deadsync_simfile::runtime_cache::song_pack_group_for_song(song);
    should_warn_itl_cmod_from_runtime(
        gs,
        player_idx,
        song_dir.as_deref(),
        group_name.as_deref(),
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
    let passed = deadsync_score::gameplay_run_passed(
        gs.song_completed_naturally(),
        gs.players()[player_idx].is_failing,
        gs.players()[player_idx].life,
        gs.players()[player_idx].fail_time.is_some(),
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
    let totals = gs.display_totals_for_player(player_idx);
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
        song_completed_naturally: gs.song_completed_naturally(),
        is_failing: player.is_failing,
        life: player.life,
        fail_time: player.fail_time,
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
    let players = (0..gs.num_players()).filter_map(|player_idx| {
        let profile_id = active_profile_id_for_player(gs.num_players(), player_idx)?;
        Some(local_score_player_from_runtime(gs, player_idx, profile_id))
    });
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
