use crate::act;
use crate::assets::{self, AssetManager, visual_styles};
use crate::assets::{FontRole, current_machine_font_key};
use crate::config::{
    self, SimpleIni, arrowcloud_qr_login_when_choice_index, breakdown_style_choice_index,
    breakdown_style_from_choice, default_fail_type_choice_index, default_fail_type_from_choice,
    default_sync_offset_choice_index, default_sync_offset_from_choice,
    groovestats_qr_login_when_choice_index, log_level_choice_index, log_level_from_choice,
    machine_bar_color_choice_index, machine_bar_color_from_choice,
    machine_evaluation_style_choice_index, machine_evaluation_style_from_choice,
    machine_font_choice_index, machine_font_from_choice, machine_preferred_play_mode_choice_index,
    machine_preferred_play_mode_from_choice, machine_preferred_play_style_choice_index,
    machine_preferred_play_style_from_choice, null_or_die_kernel_target_choice_index,
    null_or_die_kernel_type_choice_index, random_background_mode_choice_index,
    random_background_mode_from_choice, select_music_itl_rank_mode_choice_index,
    select_music_itl_rank_mode_from_choice, select_music_itl_wheel_mode_choice_index,
    select_music_itl_wheel_mode_from_choice, select_music_new_pack_mode_choice_index,
    select_music_new_pack_mode_from_choice, select_music_pattern_info_mode_choice_index,
    select_music_pattern_info_mode_from_choice, select_music_scorebox_placement_choice_index,
    select_music_scorebox_placement_from_choice, select_music_song_select_bg_mode_choice_index,
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
use crate::views::{OptionsSongPackView, SimplyLoveUpdaterCapabilities, SimplyLoveUpdaterView};
use deadlib_present::space::{is_wide, screen_height, screen_width, widescale};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile::compat as profile;
use deadsync_score as score_data;
use deadsync_simfile::app_runtime as song_loading;
use deadsync_theme::views::{
    AppPathKind, AppPathsView, AudioOptionsView, GraphicsMonitorView, GraphicsOptionsView,
    NoteskinCatalogView, SmxAssignmentView, SmxGifCatalogView,
};
use deadsync_theme::{
    AudioOutputModeChoice, AudioRequest, AudioVolumeTarget, DisplayModeChoice, FullscreenChoice,
    PresentPolicyChoice, RendererChoice, thread_choice_index, thread_count_from_choice,
};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
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
use reload::*;
mod score_import;
use score_import::*;
mod pack_sync;
pub(crate) mod qr_login;
use pack_sync::*;
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
pub use input::handle_input;
pub use layout::clear_submenu_row_layout_cache;
pub use render::{clear_description_layout_cache, clear_render_cache, get_actors, push_actors};
pub use state::{State, init};
pub use submenus::update_monitor_specs;
pub use transitions::{in_transition, out_transition};
pub use update::{
    is_smx_config_view, open_graphics_submenu, open_input_submenu, open_lights_submenu,
    open_smx_config_submenu, sync_display_mode, sync_display_resolution, sync_hide_mouse_cursor,
    sync_high_dpi, sync_max_fps, sync_present_mode_policy, sync_show_stats_mode, sync_song_packs,
    sync_translated_titles, sync_video_renderer, sync_vsync, update,
};

#[inline(always)]
fn queue_sfx(state: &mut State, path: &'static str) {
    state.pending_sfx.push(path);
}

fn volume_change_effect(target: AudioVolumeTarget, percent: u8) -> ThemeEffect {
    ThemeEffect::Batch(vec![
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            AudioRequest::SetVolume { target, percent },
        )),
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
            AudioRequest::PlaySfx("assets/sounds/change_value.ogg".to_owned()),
        )),
    ])
}

fn audio_requests_effect(requests: Vec<AudioRequest>) -> ThemeEffect {
    let mut effects = requests
        .into_iter()
        .map(|request| ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(request)));
    let Some(first) = effects.next() else {
        return ThemeEffect::None;
    };
    let rest: Vec<_> = effects.collect();
    if rest.is_empty() {
        first
    } else {
        let mut batch = Vec::with_capacity(rest.len() + 1);
        batch.push(first);
        batch.extend(rest);
        ThemeEffect::Batch(batch)
    }
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

fn queue_sync(state: &mut State, request: crate::SimplyLoveSyncRequest) {
    state.pending_sync.push(request);
}

fn queue_online(state: &mut State, request: crate::SimplyLoveOnlineRequest) {
    state.pending_online.push(request);
}

fn prepend_pending_sfx(state: &mut State, effect: ThemeEffect) -> ThemeEffect {
    let request_count =
        state.pending_sfx.len() + state.pending_sync.len() + state.pending_online.len();
    if request_count == 0 {
        return effect;
    }

    let has_effect = !matches!(effect, ThemeEffect::None);
    let mut effects = Vec::with_capacity(request_count + usize::from(has_effect));
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
    if has_effect {
        effects.push(effect);
    }
    if effects.len() == 1 {
        effects.pop().expect("one queued Options effect")
    } else {
        ThemeEffect::Batch(effects)
    }
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

fn prepend_pending_sfx_opt(state: &mut State, effect: Option<ThemeEffect>) -> Option<ThemeEffect> {
    if state.pending_sfx.is_empty()
        && state.pending_sync.is_empty()
        && state.pending_online.is_empty()
    {
        return effect;
    }
    Some(prepend_pending_sfx(
        state,
        effect.unwrap_or(ThemeEffect::None),
    ))
}

#[cfg(test)]
mod tests;
