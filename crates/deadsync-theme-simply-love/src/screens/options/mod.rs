use crate::act;
use crate::assets::{AssetManager, visual_styles};
use crate::assets::{FontRole, machine_font_key};
use crate::config::{
    self, arrowcloud_qr_login_when_choice_index, breakdown_style_choice_index,
    breakdown_style_from_choice, default_fail_type_choice_index, default_fail_type_from_choice,
    default_sync_offset_choice_index, default_sync_offset_from_choice,
    groovestats_qr_login_when_choice_index, log_level_choice_index, log_level_from_choice,
    machine_bar_color_choice_index, machine_bar_color_from_choice,
    machine_evaluation_style_choice_index, machine_evaluation_style_from_choice,
    machine_font_choice_index, machine_font_from_choice, machine_preferred_play_mode_choice_index,
    machine_preferred_play_mode_from_choice, machine_preferred_play_style_choice_index,
    machine_preferred_play_style_from_choice, null_or_die_graph_orientation_choice_index,
    null_or_die_graph_origin_choice_index, null_or_die_kernel_target_choice_index,
    null_or_die_kernel_type_choice_index, random_background_mode_choice_index,
    random_background_mode_from_choice, select_music_default_sort_choice_index,
    select_music_default_sort_from_choice, select_music_difficulty_color_scheme_choice_index,
    select_music_difficulty_color_scheme_from_choice, select_music_itl_rank_mode_choice_index,
    select_music_itl_rank_mode_from_choice, select_music_itl_wheel_mode_choice_index,
    select_music_itl_wheel_mode_from_choice, select_music_new_pack_mode_choice_index,
    select_music_new_pack_mode_from_choice, select_music_pattern_info_mode_choice_index,
    select_music_pattern_info_mode_from_choice, select_music_scorebox_placement_choice_index,
    select_music_scorebox_placement_from_choice, select_music_series_source_choice_index,
    select_music_series_source_from_choice, select_music_song_select_bg_mode_choice_index,
    select_music_song_select_bg_mode_from_choice, select_music_step_artist_box_mode_choice_index,
    select_music_step_artist_box_mode_from_choice, select_music_wheel_style_choice_index,
    select_music_wheel_style_from_choice, srpg_shop_folder_choice_index, srpg_variant_choice_index,
    srpg_variant_from_choice, sync_graph_mode_choice_index, version_overlay_side_choice_index,
    version_overlay_side_from_choice, visual_style_choice_index, visual_style_from_choice,
};
#[cfg(target_os = "windows")]
use crate::config::{
    windows_pad_backend_choice_index as windows_backend_choice_index,
    windows_pad_backend_from_choice as windows_backend_from_choice,
};
use crate::screens::input as screen_input;
use crate::screens::pack_sync as shared_pack_sync;
use crate::screens::select_music;
use crate::screens::{Screen, ThemeEffect};
use crate::views::{
    OptionsInitView, OptionsPackSyncView, OptionsSongPackView, SimplyLoveUpdaterCapabilities,
    SimplyLoveUpdaterView,
};
use deadlib_present::space::{is_wide, screen_height, screen_width, widescale};
use deadsync_input::{InputEvent, KeyCode, RawKeyboardEvent, VirtualAction};
use deadsync_score as score_data;
use deadsync_theme::views::{
    AppPathKind, AppPathsView, AudioOptionsView, GraphicsMonitorView, SmxAssignmentView,
};
use deadsync_theme::{
    AudioOutputModeChoice, AudioRequest, AudioVolumeTarget, DisplayModeChoice, FullscreenChoice,
    PresentPolicyChoice, RendererChoice, thread_choice_index, thread_count_from_choice,
};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::assets::i18n::{LookupKey, lookup_key, tr, tr_fmt};
use crate::screens::components::shared::screen_bar::{ScreenBarPosition, ScreenBarTitlePlacement};
use crate::screens::components::shared::{screen_bar, visual_style_bg};
use deadlib_present::actors;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::font;

// Submodules — wildcard re-exports let sibling modules reach every item via
// `use super::*`.
mod submenus;
use submenus::*;
mod constants;
use constants::*;
mod format;
use format::*;
mod row;
use row::*;
mod item;
use item::*;
mod state;
use state::*;
mod visibility;
use visibility::*;
mod reload;
pub use reload::sync_reload_events;
use reload::*;
mod score_import;
#[cfg(any(test, feature = "bench-support"))]
pub use score_import::ScoreImportPickerBenchmark;
use score_import::*;
mod apply_replaygain;
use apply_replaygain::*;
mod pack_sync;
pub(crate) mod qr_login;
use pack_sync::*;
#[cfg(any(test, feature = "bench-support"))]
pub use qr_login::QrOverlayBenchmark;
mod download_packs;
use download_packs::*;
mod judgment_palettes;
use judgment_palettes::*;
mod layout;
mod transitions;
use layout::*;
mod update;
use update::*;
mod input;
use input::*;
mod render;
use render::*;

// Public API re-exports
pub use download_packs::sync_stepmaniaonline;
pub use input::handle_input;
pub use layout::clear_submenu_row_layout_cache;
pub use render::{
    clear_description_layout_cache, clear_render_cache, get_actors, push_actors,
    sync_updater_panels,
};
pub use state::{State, init};
pub use submenus::update_monitor_specs;
pub use transitions::{in_transition, out_transition};
pub use update::{
    is_smx_config_view, open_graphics_submenu, open_input_submenu, open_lights_submenu,
    open_smx_config_submenu, sync_display_aspect_ratio, sync_display_mode, sync_display_resolution,
    sync_hide_mouse_cursor, sync_high_dpi, sync_max_fps, sync_present_mode_policy,
    sync_show_stats_mode, sync_song_packs, sync_translated_titles, sync_video_renderer, sync_vsync,
    update,
};

#[inline(always)]
fn queue_sfx(state: &mut State, path: &'static str) {
    state.pending_sfx.push(path);
}

const NEXT_ROW_SFX: &str = "assets/sounds/next_row.ogg";
const PREV_ROW_SFX: &str = "assets/sounds/prev_row.ogg";

const MAX_PENDING_AUDIO_REQUESTS: usize = 2;

fn queue_audio(state: &mut State, request: AudioRequest) {
    state.pending_audio.push(request);
    debug_assert!(state.pending_audio.len() <= MAX_PENDING_AUDIO_REQUESTS);
}

fn queue_volume_change(state: &mut State, target: AudioVolumeTarget, percent: u8) {
    queue_audio(state, AudioRequest::SetVolume { target, percent });
    queue_audio(
        state,
        AudioRequest::PlaySfx("assets/sounds/change_value.ogg"),
    );
}

fn select_music_config_effect(request: crate::SimplyLoveSelectMusicConfigRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::SelectMusic(request),
    ))
}

fn machine_config_effect(request: crate::SimplyLoveMachineConfigRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::Machine(request),
    ))
}

fn coin_config_effect(request: crate::SimplyLoveCoinConfigRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::Coin(request),
    ))
}

fn advanced_config_effect(request: crate::SimplyLoveAdvancedConfigRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::Advanced(request),
    ))
}

fn course_config_effect(request: crate::SimplyLoveCourseConfigRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::Course(request),
    ))
}

fn gameplay_config_effect(request: crate::SimplyLoveGameplayConfigRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::Gameplay(request),
    ))
}

fn tournament_config_effect(request: crate::SimplyLoveTournamentConfigRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::Tournament(request),
    ))
}

fn lights_config_effect(request: crate::SimplyLoveLightsConfigRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::Lights(request),
    ))
}

fn null_or_die_config_effect(request: crate::SimplyLoveNullOrDieConfigRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::NullOrDie(request),
    ))
}

fn online_config_effect(request: crate::SimplyLoveOnlineConfigRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::Online(request),
    ))
}

fn options_config_effect(request: crate::SimplyLoveOptionsConfigRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Config(
        crate::SimplyLoveConfigRequest::Options(request),
    ))
}

fn queue_sync(state: &mut State, request: crate::SimplyLoveSyncRequest) {
    state.pending_sync.push(request);
}

fn queue_online(state: &mut State, request: crate::SimplyLoveOnlineRequest) {
    state.pending_online.push(request);
}

fn queue_online_reinitialize(state: &mut State) {
    debug_assert!(!state.online_reinit_pending);
    state.online_reinit_pending = true;
}

fn append_pending_effects(state: &mut State, effect: ThemeEffect, effects: &mut Vec<ThemeEffect>) {
    effects.extend(state.pending_sfx.drain(..).map(crate::effects::sfx));
    effects.extend(
        state
            .pending_sync
            .drain(..)
            .map(|request| ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Sync(request))),
    );
    effects.extend(
        state
            .pending_online
            .drain(..)
            .map(|request| ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Online(request))),
    );
    effect.append_to(effects);
    if std::mem::take(&mut state.online_reinit_pending) {
        effects.push(ThemeEffect::Runtime(
            crate::SimplyLoveRuntimeRequest::Online(crate::SimplyLoveOnlineRequest::Reinitialize),
        ));
    }
    effects.extend(
        state
            .pending_audio
            .drain(..)
            .map(|request| ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(request))),
    );
}

pub fn apply_sync_analysis_events(state: &mut State, events: Vec<crate::SimplyLoveSyncEvent>) {
    for event in events {
        shared_pack_sync::apply_event(&mut state.pack_sync_overlay, event);
    }
}

pub fn apply_score_import_events(
    state: &mut State,
    events: Vec<crate::SimplyLoveScoreImportEvent>,
) {
    for event in events {
        apply_score_import_event(state, event);
    }
}

pub fn handle_raw_key_event(
    state: &mut State,
    key: Option<&RawKeyboardEvent>,
    text: Option<&str>,
    effects: &mut Vec<ThemeEffect>,
) -> bool {
    if judgment_palettes::handle_raw_key_event(state, key, text, effects) {
        return true;
    }
    download_packs::handle_raw_key_event(state, key, text, effects)
}

pub fn apply_apply_replaygain_events(
    state: &mut State,
    events: impl IntoIterator<Item = crate::views::SimplyLoveApplyReplayGainEvent>,
) {
    for event in events {
        apply_apply_replaygain_event(state, event);
    }
}

#[cfg(test)]
mod tests;
