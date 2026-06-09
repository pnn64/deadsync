//! Title menu screen.
//!
//! Split for hot-reload: host-owned [`State`] + boundary types in [`state`], the
//! pure render path in [`render`] (the hot-reload target), and the host-side
//! glue here — `init`, input handling (host-owned), screen transitions, and
//! [`build_host_context`] which resolves every process-global the render path
//! needs into a [`HostContext`] each frame.

// The pure render path is the hot-reload target. Under `feature = "hot"` it is
// **excluded from the engine rlib** so that editing `render.rs` recompiles only
// the tiny `deadsync-screens` cdylib (subsecond) instead of dirtying the whole
// engine rlib; the host then dispatches through the hot-loaded cdylib and uses
// `render_hot_stub` only as the pre-load / quarantine fallback. Without the
// feature (normal + release builds) the real renderer is compiled in directly.
#[cfg(not(feature = "hot"))]
pub mod render;
#[cfg(feature = "hot")]
#[path = "render_hot_stub.rs"]
pub mod render;
pub mod state;

pub use render::get_actors;
pub use state::{HostContext, State};

use deadsync::assets::i18n;
use deadsync::assets::{FontRole, current_machine_font_key};
use deadsync::engine::input::RawKeyboardEvent;
use deadsync::engine::present::actors::Actor;
use deadsync::engine::present::color;
use deadsync::engine::space::screen_center_x;
use deadsync::game::course::get_course_cache;
use deadsync::game::online as network;
use deadsync::game::song::get_song_cache;
use deadsync::screens::components::menu::menu_splash;
use deadsync::screens::components::shared::{transitions, visual_style_bg};
use deadsync::screens::input as screen_input;
use deadsync::screens::{Screen, ScreenAction};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_online::arrowcloud::ConnectionError as ArrowCloudError;
use deadsync_online::arrowcloud::ConnectionStatus as ArrowCloudConnectionStatus;
use deadsync_online::groovestats::ConnectionError as GrooveStatsError;
use deadsync_online::groovestats::ConnectionStatus;
use state::{ArrowCloudStatusKey, GrooveStatusKey, StatusTextCache};
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use winit::keyboard::KeyCode;

const TRANSITION_IN_DURATION: f32 = 0.5;
const TRANSITION_OUT_DURATION: f32 = 1.0;

pub const OPTION_COUNT: usize = 3;

pub fn init() -> State {
    State {
        selected_index: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        rainbow_mode: false,
        started_by_p2: false,
        bg: visual_style_bg::State::new(),
        i18n_revision: Cell::new(i18n::revision()),
        info_text_cache: RefCell::new(None),
        groovestats_text_cache: RefCell::new(None),
        arrowcloud_text_cache: RefCell::new(None),
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        menu_lr_undo: [0; 2],
    }
}

/// Resolve every process-global the pure render path needs into a snapshot.
///
/// Runs host-side each frame (the live engine is statically linked here). All
/// localized text is resolved and cached here so `render::get_actors` never
/// reads an engine global and never receives ownership of host heap.
pub fn build_host_context(state: &State) -> HostContext {
    sync_i18n_cache(state, i18n::revision());

    let version: Arc<str> = Arc::from(deadsync::engine::version::current().to_string());
    let banner_tag = update_banner_tag();
    let song_cache = get_song_cache();
    let pack_count = song_cache.len();
    let song_count: usize = song_cache.iter().map(|pack| pack.songs.len()).sum();
    let course_count = get_course_cache().len();

    HostContext {
        info_text: menu_info_text(
            state,
            &version,
            banner_tag.as_deref(),
            song_count,
            pack_count,
            course_count,
        ),
        menu_labels: [
            i18n::tr("Menu", "Gameplay"),
            i18n::tr("Menu", "Options"),
            i18n::tr("Menu", "Exit"),
        ],
        footer_title: i18n::tr("Common", "EventMode"),
        footer_side: i18n::tr("Common", "PressStart"),
        gs: groovestats_text(state, groove_status_key()),
        ac: arrowcloud_text(state, arrowcloud_status_key()),
        screen_center_x: screen_center_x(),
        bg_elapsed_s: visual_style_bg::elapsed_seconds(),
        menu_font: current_machine_font_key(FontRole::Bold),
        // StepManiaX pad-conflict warning: shown when two pads share a P1/P2
        // jumper and no assignment resolves them. Resolved host-side (engine
        // globals) so the pure render path only consumes the prepared strings.
        smx_warning: if crate::config::get().smx_input
            && crate::engine::smx::conflict_warning_active()
        {
            Some([
                i18n::tr("Menu", "SmxAssignWarning1"),
                i18n::tr("Menu", "SmxAssignWarning2"),
            ])
        } else {
            None
        },
    }
}

/// Clear the host-side render text caches. Called on screen (re)entry and
/// whenever the i18n language changes.
pub fn clear_render_cache(state: &State) {
    *state.info_text_cache.borrow_mut() = None;
    *state.groovestats_text_cache.borrow_mut() = None;
    *state.arrowcloud_text_cache.borrow_mut() = None;
}

fn sync_i18n_cache(state: &State, revision: u64) {
    if state.i18n_revision.get() == revision {
        return;
    }
    clear_render_cache(state);
    state.i18n_revision.set(revision);
}

fn groove_error_text(kind: GrooveStatsError) -> Arc<str> {
    match kind {
        GrooveStatsError::Disabled => i18n::tr("Menu", "Disabled"),
        GrooveStatsError::MachineOffline => i18n::tr("Menu", "MachineOffline"),
        GrooveStatsError::CannotConnect => i18n::tr("Menu", "CannotConnect"),
        GrooveStatsError::TimedOut => i18n::tr("Menu", "TimedOut"),
        GrooveStatsError::InvalidResponse => i18n::tr("Menu", "FailedToLoad"),
    }
}

fn arrowcloud_error_text(kind: ArrowCloudError) -> Arc<str> {
    match kind {
        ArrowCloudError::Disabled => i18n::tr("Menu", "Disabled"),
        ArrowCloudError::TimedOut => i18n::tr("Menu", "TimedOut"),
        ArrowCloudError::HostBlocked => i18n::tr("Menu", "HostBlocked"),
        ArrowCloudError::CannotConnect => i18n::tr("Menu", "CannotConnect"),
    }
}

fn menu_info_text(
    state: &State,
    version: &str,
    banner_tag: Option<&str>,
    song_count: usize,
    pack_count: usize,
    course_count: usize,
) -> Arc<str> {
    if let Some((cached_tag, text)) = state.info_text_cache.borrow().as_ref()
        && cached_tag.as_deref() == banner_tag
    {
        return text.clone();
    }

    let mut version_line = i18n::tr_fmt("Menu", "VersionLine", &[("version", version)]).to_string();
    if let Some(tag) = banner_tag {
        let suffix = i18n::tr_fmt("Menu", "UpdateAvailableSuffix", &[("version", tag)]);
        version_line.push(' ');
        version_line.push_str(&suffix);
    }
    let songs = song_count.to_string();
    let packs = pack_count.to_string();
    let courses = course_count.to_string();
    let summary = i18n::tr_fmt(
        "Menu",
        "SongSummary",
        &[("songs", &songs), ("packs", &packs), ("courses", &courses)],
    );
    let text = Arc::<str>::from(format!("{version_line}\n{summary}"));
    *state.info_text_cache.borrow_mut() = Some((banner_tag.map(str::to_owned), text.clone()));
    text
}

#[inline(always)]
fn groove_service_name(boogie: bool) -> Arc<str> {
    if boogie {
        i18n::tr("Menu", "BoogieStatsName")
    } else {
        i18n::tr("Menu", "GrooveStatsName")
    }
}

fn build_groovestats_text(key: GrooveStatusKey) -> StatusTextCache<GrooveStatusKey, 3> {
    let mut lines = [None, None, None];
    let (main, line_count) = match key {
        GrooveStatusKey::Pending { boogie } => {
            let service = groove_service_name(boogie);
            (
                i18n::tr_fmt("Menu", "ServicePending", &[("service", service.as_ref())]),
                0,
            )
        }
        GrooveStatusKey::Error { boogie, kind } => {
            lines[0] = Some(groove_error_text(kind));
            if kind == GrooveStatsError::Disabled {
                (i18n::tr("Menu", "GrooveStatsDisabled"), 1)
            } else {
                let service = groove_service_name(boogie);
                (
                    i18n::tr_fmt(
                        "Menu",
                        "ServiceNotConnected",
                        &[("service", service.as_ref())],
                    ),
                    1,
                )
            }
        }
        GrooveStatusKey::Connected {
            boogie,
            disabled_mask,
        } => {
            if disabled_mask == 0 {
                let service = groove_service_name(boogie);
                (
                    i18n::tr_fmt("Menu", "ServiceConnected", &[("service", service.as_ref())]),
                    0,
                )
            } else if disabled_mask == 0b111 {
                (i18n::tr("Menu", "GrooveStatsDisabled"), 0)
            } else {
                let mut line_count = 0;
                if disabled_mask & 0b001 != 0 {
                    lines[line_count] = Some(i18n::tr("Menu", "GetScoresDisabled"));
                    line_count += 1;
                }
                if disabled_mask & 0b010 != 0 {
                    lines[line_count] = Some(i18n::tr("Menu", "LeaderboardDisabled"));
                    line_count += 1;
                }
                if disabled_mask & 0b100 != 0 {
                    lines[line_count] = Some(i18n::tr("Menu", "AutoSubmitDisabled"));
                    line_count += 1;
                }
                (i18n::tr("Menu", "GrooveStatsWarn"), line_count)
            }
        }
    };
    StatusTextCache {
        key,
        main,
        lines,
        line_count,
    }
}

fn groovestats_text(state: &State, key: GrooveStatusKey) -> StatusTextCache<GrooveStatusKey, 3> {
    if let Some(cache) = state.groovestats_text_cache.borrow().as_ref()
        && cache.key == key
    {
        return cache.clone();
    }
    let cache = build_groovestats_text(key);
    *state.groovestats_text_cache.borrow_mut() = Some(cache.clone());
    cache
}

fn build_arrowcloud_text(key: ArrowCloudStatusKey) -> StatusTextCache<ArrowCloudStatusKey, 1> {
    let mut lines = [None];
    let (main, line_count) = match key {
        ArrowCloudStatusKey::Pending => (i18n::tr("Menu", "ArrowCloudPending"), 0),
        ArrowCloudStatusKey::Connected => (i18n::tr("Menu", "ArrowCloudConnected"), 0),
        ArrowCloudStatusKey::Error(kind) => {
            lines[0] = Some(arrowcloud_error_text(kind));
            (i18n::tr("Menu", "ArrowCloudDisabled"), 1)
        }
    };
    StatusTextCache {
        key,
        main,
        lines,
        line_count,
    }
}

fn arrowcloud_text(state: &State, key: ArrowCloudStatusKey) -> StatusTextCache<ArrowCloudStatusKey, 1> {
    if let Some(cache) = state.arrowcloud_text_cache.borrow().as_ref()
        && cache.key == key
    {
        return cache.clone();
    }
    let cache = build_arrowcloud_text(key);
    *state.arrowcloud_text_cache.borrow_mut() = Some(cache.clone());
    cache
}

fn update_banner_tag() -> Option<String> {
    match deadsync::engine::updater::state::snapshot()? {
        deadsync::engine::updater::UpdateState::Available(info) => Some(info.tag),
        _ => None,
    }
}

#[inline(always)]
fn groove_status_key() -> GrooveStatusKey {
    let boogie = network::is_boogiestats_active();
    match network::get_status() {
        ConnectionStatus::Pending => GrooveStatusKey::Pending { boogie },
        ConnectionStatus::Error(kind) => GrooveStatusKey::Error { boogie, kind },
        ConnectionStatus::Connected(services) => GrooveStatusKey::Connected {
            boogie,
            disabled_mask: (!services.get_scores) as u8
                | (((!services.leaderboard) as u8) << 1)
                | (((!services.auto_submit) as u8) << 2),
        },
    }
}

#[inline(always)]
fn arrowcloud_status_key() -> ArrowCloudStatusKey {
    match network::get_arrowcloud_status() {
        ArrowCloudConnectionStatus::Pending => ArrowCloudStatusKey::Pending,
        ArrowCloudConnectionStatus::Connected => ArrowCloudStatusKey::Connected,
        ArrowCloudConnectionStatus::Error(kind) => ArrowCloudStatusKey::Error(kind),
    }
}

// Keyboard input is handled centrally via the virtual dispatcher in app
// Screen-specific raw keyboard handling for Menu (e.g., F4 to Sandbox)
pub fn handle_raw_key_event(_state: &mut State, key: &RawKeyboardEvent) -> ScreenAction {
    if !key.pressed {
        return ScreenAction::None;
    }
    match key.code {
        KeyCode::F4 => return ScreenAction::Navigate(Screen::Sandbox),
        KeyCode::Escape => return ScreenAction::Exit,
        _ => {}
    }
    ScreenAction::None
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition(active_color_index: i32) -> (Vec<Actor>, f32) {
    let mut actors: Vec<Actor> = Vec::new();

    // Visual-style splash, matching Simply Love's ScreenTitleMenu out.lua look.
    actors.extend(menu_splash::build(active_color_index));

    // Full-screen fade to black behind the hearts.
    let fade = transitions::fade_out_black_actor(TRANSITION_OUT_DURATION, 1200);
    actors.push(fade);

    (actors, TRANSITION_OUT_DURATION)
}

#[inline(always)]
fn move_selection(state: &mut State, delta: isize) {
    let n = OPTION_COUNT as isize;
    let cur = state.selected_index as isize;
    state.selected_index = (cur + delta).rem_euclid(n) as usize;
    deadsync::engine::audio::play_sfx("assets/sounds/change.ogg");
}

#[inline(always)]
fn start_selected(state: &mut State, started_by_p2: bool) -> ScreenAction {
    deadsync::engine::audio::play_sfx("assets/sounds/start.ogg");
    state.started_by_p2 = started_by_p2;
    match state.selected_index {
        0 => ScreenAction::Navigate(Screen::SelectProfile),
        1 => ScreenAction::Navigate(Screen::Options),
        2 => ScreenAction::Exit,
        _ => ScreenAction::None,
    }
}

#[inline(always)]
const fn menu_nav_delta(action: VirtualAction) -> Option<isize> {
    match action {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => Some(-1),
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => Some(1),
        _ => None,
    }
}

// Event-driven virtual input handler
pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if let Some(side) = screen_input::menu_lr_side(ev.action)
        && !ev.pressed
    {
        state.menu_lr_undo[screen_input::player_side_ix(side)] = 0;
    }
    if let Some((side, nav)) = screen_input::three_key_menu_action(&mut state.menu_lr_chord, ev) {
        let side_ix = screen_input::player_side_ix(side);
        return match nav {
            screen_input::ThreeKeyMenuAction::Prev => {
                move_selection(state, -1);
                state.menu_lr_undo[side_ix] = 1;
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Next => {
                move_selection(state, 1);
                state.menu_lr_undo[side_ix] = -1;
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Confirm => {
                state.menu_lr_undo[side_ix] = 0;
                start_selected(state, side_ix == 1)
            }
            screen_input::ThreeKeyMenuAction::Cancel => {
                let undo = state.menu_lr_undo[side_ix];
                if undo != 0 {
                    move_selection(state, undo as isize);
                    state.menu_lr_undo[side_ix] = 0;
                }
                ScreenAction::Exit
            }
        };
    }
    if !ev.pressed {
        return ScreenAction::None;
    }
    if let Some(delta) = menu_nav_delta(ev.action) {
        move_selection(state, delta);
        return ScreenAction::None;
    }
    match ev.action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            start_selected(state, matches!(ev.action, VirtualAction::p2_start))
        }
        VirtualAction::p1_back | VirtualAction::p2_back => ScreenAction::Exit,
        _ => ScreenAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::menu_nav_delta;
    use deadsync_input::VirtualAction;

    #[test]
    fn title_menu_left_and_up_move_previous() {
        assert_eq!(menu_nav_delta(VirtualAction::p1_left), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_menu_left), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_up), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_menu_up), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_left), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_menu_left), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_up), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_menu_up), Some(-1));
    }

    #[test]
    fn title_menu_right_and_down_move_next() {
        assert_eq!(menu_nav_delta(VirtualAction::p1_right), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_menu_right), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_down), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_menu_down), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_right), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_menu_right), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_down), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_menu_down), Some(1));
    }
}
