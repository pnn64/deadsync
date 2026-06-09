//! Host-owned state and boundary types for the title menu screen.
//!
//! `State` lives here (host-owned, survives a hot reload) together with the small
//! Copy status-key enums and the render-time cache. `HostContext` is the boundary
//! value the host resolves each frame and hands to the pure `render::get_actors`;
//! every string it carries is already resolved and cached host-side before the
//! boundary is crossed.

use deadsync::screens::components::shared::visual_style_bg;
use deadsync::screens::input as screen_input;
use deadsync_online::arrowcloud::ConnectionError as ArrowCloudError;
use deadsync_online::groovestats::ConnectionError as GrooveStatsError;
use std::cell::{Cell, RefCell};
use std::sync::Arc;

/// Resolved status text + extra lines, cached on `State` keyed by a Copy status key.
#[derive(Clone)]
pub struct StatusTextCache<K, const N: usize> {
    pub key: K,
    pub main: Arc<str>,
    pub lines: [Option<Arc<str>>; N],
    pub line_count: usize,
}

/// Fully captures every input that affects the GrooveStats/BoogieStats status text.
/// Derived host-side from the network globals; the render path only formats from it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GrooveStatusKey {
    Pending {
        boogie: bool,
    },
    Error {
        boogie: bool,
        kind: GrooveStatsError,
    },
    Connected {
        boogie: bool,
        disabled_mask: u8,
    },
}

/// Fully captures every input that affects the ArrowCloud status text.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ArrowCloudStatusKey {
    Pending,
    Connected,
    Error(ArrowCloudError),
}

pub struct State {
    pub selected_index: usize,
    pub active_color_index: i32,
    pub rainbow_mode: bool,
    pub started_by_p2: bool,
    // The following fields are read/written by the hot render path, so they must
    // be `pub` for the reloadable cdylib to reach them.
    #[doc(hidden)]
    pub bg: visual_style_bg::State,
    #[doc(hidden)]
    pub i18n_revision: Cell<u64>,
    #[doc(hidden)]
    pub info_text_cache: RefCell<Option<(Option<String>, Arc<str>)>>,
    #[doc(hidden)]
    pub groovestats_text_cache: RefCell<Option<StatusTextCache<GrooveStatusKey, 3>>>,
    #[doc(hidden)]
    pub arrowcloud_text_cache: RefCell<Option<StatusTextCache<ArrowCloudStatusKey, 1>>>,
    // Input-path only (stays host-owned, never touched by the hot render unit).
    pub(crate) menu_lr_chord: screen_input::MenuLrChordTracker,
    pub(crate) menu_lr_undo: [i8; 2],
}

/// Everything the pure render path needs that would otherwise be a process-global
/// read, fully pre-resolved host-side (see `super::build_host_context`).
pub struct HostContext {
    /// Pre-resolved menu info line (version + optional update tag + the
    /// song/pack/course summary), cached on `State`.
    pub info_text: Arc<str>,
    /// Pre-resolved menu option labels (Gameplay / Options / Exit).
    pub menu_labels: [Arc<str>; 3],
    /// Pre-resolved footer title (`Common/EventMode`).
    pub footer_title: Arc<str>,
    /// Pre-resolved footer side text (`Common/PressStart`), shown left and right.
    pub footer_side: Arc<str>,
    /// Pre-resolved GrooveStats/BoogieStats status block, cached on `State`.
    pub gs: StatusTextCache<GrooveStatusKey, 3>,
    /// Pre-resolved ArrowCloud status block, cached on `State`.
    pub ac: StatusTextCache<ArrowCloudStatusKey, 1>,
    pub screen_center_x: f32,
    /// Shared UI background elapsed clock, resolved host-side so the render path
    /// animates off the host's ticked clock rather than the cdylib's own copy.
    pub bg_elapsed_s: f32,
    /// Lib-owned font key for the menu list (`current_machine_font_key`).
    pub menu_font: &'static str,
    /// Pre-resolved StepManiaX pad-conflict warning lines; `Some` only when the
    /// warning is active (SMX input on and an unresolved P1/P2 jumper).
    pub smx_warning: Option<[Arc<str>; 2]>,
}
