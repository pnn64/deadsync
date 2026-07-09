use crate::act;
use crate::assets::{self, AssetManager, visual_styles};
use crate::assets::{FontRole, current_machine_font_key};
use crate::config::{
    self, DisplayMode, FullscreenType, SimpleIni, arrowcloud_qr_login_when_choice_index,
    arrowcloud_qr_login_when_from_choice, breakdown_style_choice_index,
    breakdown_style_from_choice, default_fail_type_choice_index, default_fail_type_from_choice,
    default_sync_offset_choice_index, default_sync_offset_from_choice,
    groovestats_qr_login_when_choice_index, groovestats_qr_login_when_from_choice,
    log_level_choice_index, log_level_from_choice, machine_bar_color_choice_index,
    machine_bar_color_from_choice, machine_evaluation_style_choice_index,
    machine_evaluation_style_from_choice, machine_font_choice_index, machine_font_from_choice,
    machine_preferred_play_mode_choice_index, machine_preferred_play_mode_from_choice,
    machine_preferred_play_style_choice_index, machine_preferred_play_style_from_choice,
    null_or_die_kernel_target_choice_index, null_or_die_kernel_target_from_choice,
    null_or_die_kernel_type_choice_index, null_or_die_kernel_type_from_choice,
    random_background_mode_choice_index, random_background_mode_from_choice,
    select_music_itl_rank_mode_choice_index, select_music_itl_rank_mode_from_choice,
    select_music_itl_wheel_mode_choice_index, select_music_itl_wheel_mode_from_choice,
    select_music_new_pack_mode_choice_index, select_music_new_pack_mode_from_choice,
    select_music_pattern_info_mode_choice_index, select_music_pattern_info_mode_from_choice,
    select_music_scorebox_placement_choice_index, select_music_scorebox_placement_from_choice,
    select_music_song_select_bg_mode_choice_index, select_music_song_select_bg_mode_from_choice,
    select_music_step_artist_box_mode_choice_index, select_music_step_artist_box_mode_from_choice,
    select_music_wheel_style_choice_index, select_music_wheel_style_from_choice,
    srpg_variant_choice_index, srpg_variant_from_choice, sync_graph_mode_choice_index,
    sync_graph_mode_from_choice, version_overlay_side_choice_index,
    version_overlay_side_from_choice, visual_style_choice_index, visual_style_from_choice,
};
use crate::screens::input as screen_input;
use crate::screens::pack_sync as shared_pack_sync;
use crate::screens::select_music;
use crate::screens::{Screen, ScreenAction};
use deadlib_platform::display::{
    self, MonitorSpec, fullscreen_type_choice_index, fullscreen_type_from_choice,
};
use deadlib_present::space::{is_wide, screen_height, screen_width, widescale};
use deadlib_render::{
    BackendType, PresentModePolicy, backend_type_choice_index as backend_to_renderer_choice_index,
    backend_type_from_choice as renderer_choice_index_to_backend, build_software_thread_choices,
    present_mode_policy_choice_index, present_mode_policy_from_choice,
    software_thread_choice_index, software_thread_from_choice,
};
use deadsync_audio_stream as audio;
use deadsync_input::{InputEvent, VirtualAction};
#[cfg(target_os = "windows")]
use deadsync_input_native::{
    windows_pad_backend_choice_index as windows_backend_choice_index,
    windows_pad_backend_from_choice as windows_backend_from_choice,
};
use deadsync_online::score_compat as scores;
use deadsync_profile::compat as profile;
use deadsync_score as score_data;
use deadsync_simfile::app_runtime as song_loading;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    sync_high_dpi, sync_max_fps, sync_present_mode_policy, sync_show_stats_mode,
    sync_translated_titles, sync_video_renderer, sync_vsync, update,
};

#[cfg(test)]
mod tests;
