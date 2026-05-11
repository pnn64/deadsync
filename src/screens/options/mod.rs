use crate::act;
use crate::assets::{self, AssetManager, visual_styles};
use crate::assets::{FontRole, current_machine_font_key};
use crate::config::{
    self, BreakdownStyle, DefaultFailType, DisplayMode, FullscreenType, LogLevel, MachineFont,
    MachinePreferredPlayMode, MachinePreferredPlayStyle, NewPackMode, SelectMusicItlRankMode,
    SelectMusicItlWheelMode, SelectMusicPatternInfoMode, SelectMusicScoreboxPlacement,
    SelectMusicWheelStyle, SimpleIni, SyncGraphMode, VisualStyle, dirs,
};
use crate::engine::audio;
use crate::engine::display::{self, MonitorSpec};
use crate::engine::gfx::{BackendType, PresentModePolicy};
#[cfg(target_os = "windows")]
use crate::engine::input::WindowsPadBackend;
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::lights::DriverKind as LightsDriverKind;
use crate::engine::space::{is_wide, screen_height, screen_width, widescale};
use crate::game::parsing::{noteskin as noteskin_parser, simfile as song_loading};
use crate::game::{course, profile, scores};
use crate::screens::input as screen_input;
use crate::screens::pack_sync as shared_pack_sync;
use crate::screens::select_music;
use crate::screens::{Screen, ScreenAction};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::assets::i18n::{LookupKey, lookup_key, tr, tr_fmt};
use crate::engine::present::actors;
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::present::font;
use crate::screens::components::shared::screen_bar::{ScreenBarPosition, ScreenBarTitlePlacement};
use crate::screens::components::shared::{screen_bar, visual_style_bg};

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
pub use render::{clear_description_layout_cache, clear_render_cache, get_actors};
pub use state::{State, init};
pub use submenus::update_monitor_specs;
pub use transitions::{in_transition, out_transition};
pub use update::{
    open_input_submenu, sync_display_mode, sync_display_resolution, sync_high_dpi, sync_max_fps,
    sync_present_mode_policy, sync_show_stats_mode, sync_translated_titles, sync_video_renderer,
    sync_vsync, update,
};

#[cfg(test)]
mod tests;
