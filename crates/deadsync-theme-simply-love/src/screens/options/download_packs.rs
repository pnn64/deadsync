use super::*;
use deadlib_present::space::{screen_center_x, screen_center_y};
use deadsync_online::stepmaniaonline::{
    CatalogPhase, InstallPhase, InstallSnapshot, PackInfo, Snapshot, search_catalog,
};

const Z: i16 = 1490;
const PANEL_W: f32 = 620.0;
const PANEL_H: f32 = 452.0;
const HEADER_H: f32 = 48.0;
const SEARCH_H: f32 = 42.0;
const FILTER_H: f32 = 28.0;
const PANE_W: f32 = 286.0;
const ROW_H: f32 = 37.0;
const VIEW_ROWS: usize = 7;
const QUERY_MAX_CHARS: usize = 96;
const CURSOR_PERIOD: f32 = 0.8;
const BROWSER_NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(375);
const BROWSER_NAV_REPEAT_INTERVAL: Duration = Duration::from_millis(125);
const BROWSER_TAB_SCROLL_DIVISOR: u32 = 4;

#[derive(Clone, Debug)]
struct DownloadConfirm {
    pack_id: u64,
    name: String,
    size_bytes: u64,
    choice: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SubstyleKey {
    All,
    Named(String),
    Uncategorized,
}

#[derive(Clone, Debug)]
struct SubstyleButton {
    key: SubstyleKey,
    count: usize,
}

#[derive(Clone, Copy, Debug)]
struct BrowserNavHold {
    delta: isize,
    held_for: Duration,
    since_scroll: Duration,
}

#[derive(Clone, Debug)]
pub(super) struct DownloadPacksOverlayData {
    query: String,
    /// Options-logic-owned search index. It is single-threaded, lives only for
    /// this overlay, and is bounded by the fetched catalog. It is rebuilt on
    /// catalog/query changes, never on an ordinary frame; misses mean an empty
    /// result set and replacement is its only eviction. It is dropped on close,
    /// the header exposes its result count, and ordinary-frame lookup/render is
    /// bounded to `VIEW_ROWS` entries.
    results: Vec<usize>,
    selected: usize,
    /// Catalog-revision-owned facets. They are rebuilt only when the immutable
    /// catalog changes, capped by its distinct substyle count, and dropped with
    /// the overlay. Filtering scans only the already bounded search results.
    substyles: Vec<SubstyleButton>,
    selected_substyle: usize,
    catalog_revision: u64,
    blink_t: f32,
    nav_hold: Option<BrowserNavHold>,
    tab_held: bool,
    confirm: Option<DownloadConfirm>,
    local_message: Option<String>,
    /// Options-logic-owned installed-name index. It is single-threaded, lives
    /// for the overlay, and is capped at two names per scanned song pack. It is
    /// warmed on open and replaced after a song-cache generation change; a miss
    /// simply presents the pack as downloadable. It is dropped on close, needs
    /// no eviction or destruction work, and gives O(1) ordinary-frame checks.
    installed_names: HashSet<String>,
}

#[derive(Clone, Debug)]
pub(super) enum DownloadPacksOverlayState {
    Hidden,
    Visible(Box<DownloadPacksOverlayData>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputOutcome {
    None,
    Edited,
    Changed,
    Activated,
    Closed,
    EnsureCatalog,
    RefreshCatalog,
    Download(u64),
}

#[inline(always)]
pub(super) const fn overlay_visible(state: &DownloadPacksOverlayState) -> bool {
    matches!(state, DownloadPacksOverlayState::Visible(_))
}

pub(super) fn show_overlay(
    state: &mut DownloadPacksOverlayState,
    snapshot: &Snapshot,
    installed_packs: &[OptionsSongPackView],
) {
    let mut data = DownloadPacksOverlayData {
        query: String::new(),
        results: Vec::new(),
        selected: 0,
        substyles: Vec::new(),
        selected_substyle: 0,
        catalog_revision: u64::MAX,
        blink_t: 0.0,
        nav_hold: None,
        tab_held: false,
        confirm: None,
        local_message: None,
        installed_names: installed_pack_names(installed_packs),
    };
    rebuild_substyles(&mut data, snapshot, None);
    rebuild_results(&mut data, snapshot, None);
    *state = DownloadPacksOverlayState::Visible(Box::new(data));
}

fn hide_overlay(state: &mut DownloadPacksOverlayState) {
    *state = DownloadPacksOverlayState::Hidden;
}

pub(super) fn update_overlay(state: &mut DownloadPacksOverlayState, dt: f32) -> bool {
    let DownloadPacksOverlayState::Visible(data) = state else {
        return false;
    };
    data.blink_t = (data.blink_t + dt.max(0.0)) % CURSOR_PERIOD;
    true
}

pub fn sync_stepmaniaonline(
    state: &mut State,
    snapshot: Arc<Snapshot>,
    ready_song_dirs: Vec<PathBuf>,
) {
    let (selected_id, selected_substyle) = match &state.download_packs_overlay {
        DownloadPacksOverlayState::Visible(data) => (
            selected_pack(data, &state.stepmaniaonline_snapshot).map(|pack| pack.id),
            active_substyle(data).cloned(),
        ),
        DownloadPacksOverlayState::Hidden => (None, None),
    };
    state.stepmaniaonline_snapshot = snapshot;
    if let DownloadPacksOverlayState::Visible(data) = &mut state.download_packs_overlay
        && data.catalog_revision != state.stepmaniaonline_snapshot.revision
    {
        rebuild_substyles(
            data,
            &state.stepmaniaonline_snapshot,
            selected_substyle.as_ref(),
        );
        rebuild_results(data, &state.stepmaniaonline_snapshot, selected_id);
    }
    for dir in ready_song_dirs {
        if !state.pending_pack_reload_dirs.contains(&dir) {
            state.pending_pack_reload_dirs.push(dir);
        }
    }
}

pub(super) fn update_browser(state: &mut State, dt: f32) -> bool {
    if !update_overlay(&mut state.download_packs_overlay, dt) {
        return false;
    }
    if repeat_nav_hold(&mut state.download_packs_overlay, dt) == InputOutcome::Changed {
        queue_sfx(state, "assets/sounds/change.ogg");
    }
    true
}

pub(super) fn sync_installed_packs(
    state: &mut DownloadPacksOverlayState,
    installed_packs: &[OptionsSongPackView],
) {
    if let DownloadPacksOverlayState::Visible(data) = state {
        data.installed_names = installed_pack_names(installed_packs);
    }
}

pub(super) fn handle_browser_input(state: &mut State, event: &InputEvent) -> Option<ThemeEffect> {
    if !overlay_visible(&state.download_packs_overlay) {
        return None;
    }
    let nav_delta = browser_ud_delta(event.action);
    if let Some(delta) = nav_delta {
        if !event.pressed {
            release_nav_hold(&mut state.download_packs_overlay, delta);
            return Some(ThemeEffect::None);
        }
    } else if !event.pressed {
        return Some(ThemeEffect::None);
    } else {
        clear_browser_nav_hold(&mut state.download_packs_overlay);
    }

    let outcome = if screen_input::dedicated_three_key_nav_enabled()
        && let Some((_, action)) =
            screen_input::three_key_menu_action(&mut state.menu_lr_chord, event)
    {
        handle_three_key_input(
            &mut state.download_packs_overlay,
            action,
            &state.stepmaniaonline_snapshot,
        )
    } else {
        handle_virtual_input(
            &mut state.download_packs_overlay,
            event,
            &state.stepmaniaonline_snapshot,
        )
    };
    if outcome == InputOutcome::Changed
        && let Some(delta) = nav_delta
        && !overlay_confirming(&state.download_packs_overlay)
    {
        start_nav_hold(&mut state.download_packs_overlay, delta);
    }
    Some(apply_outcome(state, outcome))
}

#[inline(always)]
const fn browser_ud_delta(action: VirtualAction) -> Option<isize> {
    match action {
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => Some(-1),
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => Some(1),
        _ => None,
    }
}

fn overlay_confirming(state: &DownloadPacksOverlayState) -> bool {
    matches!(state, DownloadPacksOverlayState::Visible(data) if data.confirm.is_some())
}

fn start_nav_hold(state: &mut DownloadPacksOverlayState, delta: isize) {
    let DownloadPacksOverlayState::Visible(data) = state else {
        return;
    };
    data.nav_hold = Some(BrowserNavHold {
        delta,
        held_for: Duration::ZERO,
        since_scroll: Duration::ZERO,
    });
}

fn release_nav_hold(state: &mut DownloadPacksOverlayState, delta: isize) {
    let DownloadPacksOverlayState::Visible(data) = state else {
        return;
    };
    if data.nav_hold.is_some_and(|hold| hold.delta == delta) {
        data.nav_hold = None;
    }
}

fn clear_browser_nav_hold(state: &mut DownloadPacksOverlayState) {
    if let DownloadPacksOverlayState::Visible(data) = state {
        data.nav_hold = None;
    }
}

fn repeat_nav_hold(state: &mut DownloadPacksOverlayState, dt: f32) -> InputOutcome {
    let DownloadPacksOverlayState::Visible(data) = state else {
        return InputOutcome::None;
    };
    if data.confirm.is_some() || data.results.len() <= 1 || dt <= 0.0 || !dt.is_finite() {
        data.nav_hold = None;
        return InputOutcome::None;
    }
    let Some(hold) = data.nav_hold.as_mut() else {
        return InputOutcome::None;
    };
    let elapsed = Duration::from_secs_f32(dt);
    hold.held_for = hold.held_for.saturating_add(elapsed);
    hold.since_scroll = hold.since_scroll.saturating_add(elapsed);
    let divisor = if data.tab_held {
        BROWSER_TAB_SCROLL_DIVISOR
    } else {
        1
    };
    let delay = BROWSER_NAV_INITIAL_HOLD_DELAY / divisor;
    let interval = BROWSER_NAV_REPEAT_INTERVAL / divisor;
    if hold.held_for < delay || hold.since_scroll < interval {
        return InputOutcome::None;
    }
    let delta = hold.delta;
    let outcome = move_selection(data, delta);
    if outcome == InputOutcome::Changed
        && let Some(hold) = data.nav_hold.as_mut()
    {
        hold.since_scroll = Duration::ZERO;
    }
    outcome
}

fn handle_three_key_input(
    state: &mut DownloadPacksOverlayState,
    action: screen_input::ThreeKeyMenuAction,
    snapshot: &Snapshot,
) -> InputOutcome {
    let DownloadPacksOverlayState::Visible(data) = state else {
        return InputOutcome::None;
    };
    if data.confirm.is_some() {
        return match action {
            screen_input::ThreeKeyMenuAction::Prev | screen_input::ThreeKeyMenuAction::Next => {
                toggle_confirm_choice(data)
            }
            screen_input::ThreeKeyMenuAction::Confirm => confirm_selection(data),
            screen_input::ThreeKeyMenuAction::Cancel => cancel_confirmation(data),
        };
    }
    match action {
        screen_input::ThreeKeyMenuAction::Prev => move_selection(data, -1),
        screen_input::ThreeKeyMenuAction::Next => move_selection(data, 1),
        screen_input::ThreeKeyMenuAction::Confirm if snapshot.phase == CatalogPhase::Error => {
            InputOutcome::RefreshCatalog
        }
        screen_input::ThreeKeyMenuAction::Confirm if snapshot.phase == CatalogPhase::Idle => {
            InputOutcome::EnsureCatalog
        }
        screen_input::ThreeKeyMenuAction::Confirm => activate_selected(data, snapshot),
        screen_input::ThreeKeyMenuAction::Cancel => {
            hide_overlay(state);
            InputOutcome::Closed
        }
    }
}

pub fn handle_raw_key_event(
    state: &mut State,
    key: Option<&RawKeyboardEvent>,
    text: Option<&str>,
) -> ThemeEffect {
    if !overlay_visible(&state.download_packs_overlay) {
        return ThemeEffect::None;
    }
    if let Some(outcome) = handle_raw_shortcut(&mut state.download_packs_overlay, key) {
        let effect = apply_outcome(state, outcome);
        let effect = if matches!(effect, ThemeEffect::None) {
            ThemeEffect::ConsumeInput
        } else {
            effect
        };
        return prepend_pending_sfx(state, effect);
    }
    // Text entry owns keyboard keys so WASD-style pad mappings do not move the
    // list while typing. Arrows retain normal menu navigation, and the two
    // process-wide diagnostic shortcuts still fall through to the shell.
    let owns_input = text.is_some()
        || key.is_some_and(|key| {
            let confirming = overlay_confirming(&state.download_packs_overlay);
            confirming
                || !matches!(
                    key.code,
                    KeyCode::ArrowUp
                        | KeyCode::ArrowDown
                        | KeyCode::ArrowLeft
                        | KeyCode::ArrowRight
                        | KeyCode::F3
                        | KeyCode::F9
                )
        });
    if !owns_input {
        return ThemeEffect::None;
    }
    let outcome = handle_raw_input(
        &mut state.download_packs_overlay,
        key,
        text,
        &state.stepmaniaonline_snapshot,
    );
    let effect = apply_outcome(state, outcome);
    let effect = if matches!(effect, ThemeEffect::None) {
        ThemeEffect::ConsumeInput
    } else {
        effect
    };
    prepend_pending_sfx(state, effect)
}

fn handle_raw_shortcut(
    state: &mut DownloadPacksOverlayState,
    key: Option<&RawKeyboardEvent>,
) -> Option<InputOutcome> {
    let key = key?;
    let DownloadPacksOverlayState::Visible(data) = state else {
        return None;
    };
    match key.code {
        KeyCode::Tab => {
            data.tab_held = key.pressed;
            Some(InputOutcome::None)
        }
        KeyCode::PageUp | KeyCode::PageDown => {
            if key.pressed && !key.repeat && data.confirm.is_none() {
                data.nav_hold = None;
                let direction = if key.code == KeyCode::PageUp { -1 } else { 1 };
                Some(page_selection(data, direction))
            } else {
                Some(InputOutcome::None)
            }
        }
        _ => None,
    }
}

fn apply_outcome(state: &mut State, outcome: InputOutcome) -> ThemeEffect {
    match outcome {
        InputOutcome::None | InputOutcome::Edited => {}
        InputOutcome::Changed | InputOutcome::Closed => {
            queue_sfx(state, "assets/sounds/change.ogg");
        }
        InputOutcome::Activated => {
            queue_sfx(state, "assets/sounds/start.ogg");
        }
        InputOutcome::EnsureCatalog => {
            queue_sfx(state, "assets/sounds/start.ogg");
            queue_online(
                state,
                crate::SimplyLoveOnlineRequest::EnsureStepManiaOnlineCatalog,
            );
        }
        InputOutcome::RefreshCatalog => {
            queue_sfx(state, "assets/sounds/start.ogg");
            queue_online(
                state,
                crate::SimplyLoveOnlineRequest::RefreshStepManiaOnlineCatalog,
            );
        }
        InputOutcome::Download(pack_id) => {
            queue_sfx(state, "assets/sounds/start.ogg");
            queue_online(
                state,
                crate::SimplyLoveOnlineRequest::DownloadStepManiaOnlinePack { pack_id },
            );
        }
    }
    ThemeEffect::None
}

fn handle_virtual_input(
    state: &mut DownloadPacksOverlayState,
    event: &InputEvent,
    snapshot: &Snapshot,
) -> InputOutcome {
    if !event.pressed {
        return InputOutcome::None;
    }
    let DownloadPacksOverlayState::Visible(data) = state else {
        return InputOutcome::None;
    };
    if data.confirm.is_some() {
        return handle_confirm_action(data, event.action);
    }

    match event.action {
        VirtualAction::p1_back | VirtualAction::p2_back => {
            hide_overlay(state);
            InputOutcome::Closed
        }
        VirtualAction::p1_start | VirtualAction::p2_start
            if snapshot.phase == CatalogPhase::Error =>
        {
            InputOutcome::RefreshCatalog
        }
        VirtualAction::p1_start | VirtualAction::p2_start
            if snapshot.phase == CatalogPhase::Idle =>
        {
            InputOutcome::EnsureCatalog
        }
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => move_selection(data, -1),
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => move_selection(data, 1),
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => move_substyle(data, snapshot, -1),
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => move_substyle(data, snapshot, 1),
        VirtualAction::p1_start | VirtualAction::p2_start => activate_selected(data, snapshot),
        VirtualAction::p1_select | VirtualAction::p2_select => clear_query(data, snapshot),
        _ => InputOutcome::None,
    }
}

fn handle_raw_input(
    state: &mut DownloadPacksOverlayState,
    key: Option<&RawKeyboardEvent>,
    text: Option<&str>,
    snapshot: &Snapshot,
) -> InputOutcome {
    let DownloadPacksOverlayState::Visible(data) = state else {
        return InputOutcome::None;
    };
    if data.confirm.is_some() {
        let Some(key) = key.filter(|key| key.pressed) else {
            return InputOutcome::None;
        };
        return match key.code {
            KeyCode::ArrowLeft | KeyCode::ArrowRight => toggle_confirm_choice(data),
            KeyCode::Enter | KeyCode::NumpadEnter => confirm_selection(data),
            KeyCode::Escape => cancel_confirmation(data),
            _ => InputOutcome::None,
        };
    }
    if let Some(text) = text {
        return if add_query_text(data, snapshot, text) {
            InputOutcome::Edited
        } else {
            InputOutcome::None
        };
    }
    let Some(key) = key.filter(|key| key.pressed) else {
        return InputOutcome::None;
    };
    match key.code {
        KeyCode::Backspace => {
            if data.query.pop().is_some() {
                data.blink_t = 0.0;
                rebuild_results(data, snapshot, None);
                InputOutcome::Edited
            } else {
                InputOutcome::None
            }
        }
        KeyCode::Delete => clear_query(data, snapshot),
        KeyCode::Escape => {
            hide_overlay(state);
            InputOutcome::Closed
        }
        KeyCode::Enter | KeyCode::NumpadEnter if snapshot.phase == CatalogPhase::Error => {
            InputOutcome::RefreshCatalog
        }
        KeyCode::Enter | KeyCode::NumpadEnter if snapshot.phase == CatalogPhase::Idle => {
            InputOutcome::EnsureCatalog
        }
        KeyCode::Enter | KeyCode::NumpadEnter => activate_selected(data, snapshot),
        _ => InputOutcome::None,
    }
}

fn handle_confirm_action(
    data: &mut DownloadPacksOverlayData,
    action: VirtualAction,
) -> InputOutcome {
    match action {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => toggle_confirm_choice(data),
        VirtualAction::p1_start | VirtualAction::p2_start => confirm_selection(data),
        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => cancel_confirmation(data),
        _ => InputOutcome::None,
    }
}

fn toggle_confirm_choice(data: &mut DownloadPacksOverlayData) -> InputOutcome {
    let Some(confirm) = data.confirm.as_mut() else {
        return InputOutcome::None;
    };
    confirm.choice = 1 - confirm.choice.min(1);
    InputOutcome::Changed
}

fn confirm_selection(data: &mut DownloadPacksOverlayData) -> InputOutcome {
    let Some(confirm) = data.confirm.take() else {
        return InputOutcome::None;
    };
    if confirm.choice == 1 {
        data.local_message = None;
        return InputOutcome::Changed;
    }
    data.local_message = Some(
        tr_fmt(
            "OptionsDownloadPacks",
            "QueuedMessage",
            &[("name", &confirm.name)],
        )
        .to_string(),
    );
    InputOutcome::Download(confirm.pack_id)
}

fn cancel_confirmation(data: &mut DownloadPacksOverlayData) -> InputOutcome {
    data.confirm = None;
    InputOutcome::Changed
}

fn activate_selected(data: &mut DownloadPacksOverlayData, snapshot: &Snapshot) -> InputOutcome {
    if snapshot.phase != CatalogPhase::Ready {
        return InputOutcome::None;
    }
    let Some(pack) = selected_pack(data, snapshot) else {
        return InputOutcome::None;
    };
    if pack_is_installed(pack, snapshot, &data.installed_names) {
        data.local_message = Some(tr("OptionsDownloadPacks", "AlreadyInstalled").to_string());
        return InputOutcome::Activated;
    }
    if let Some(install) = install_for_pack(snapshot, pack.id)
        && matches!(
            install.phase,
            InstallPhase::Queued | InstallPhase::Downloading | InstallPhase::Extracting
        )
    {
        data.local_message = Some(install_status_text(install));
        return InputOutcome::Activated;
    }
    data.confirm = Some(DownloadConfirm {
        pack_id: pack.id,
        name: pack.name.clone(),
        size_bytes: pack.size_bytes,
        choice: 0,
    });
    data.local_message = None;
    InputOutcome::Activated
}

fn move_selection(data: &mut DownloadPacksOverlayData, delta: isize) -> InputOutcome {
    let len = data.results.len();
    if len <= 1 || delta == 0 {
        return InputOutcome::None;
    }
    data.selected = ((data.selected as isize + delta).rem_euclid(len as isize)) as usize;
    data.local_message = None;
    InputOutcome::Changed
}

fn move_substyle(
    data: &mut DownloadPacksOverlayData,
    snapshot: &Snapshot,
    delta: isize,
) -> InputOutcome {
    let len = data.substyles.len();
    if len <= 1 || delta == 0 {
        return InputOutcome::None;
    }
    data.selected_substyle =
        ((data.selected_substyle as isize + delta).rem_euclid(len as isize)) as usize;
    data.nav_hold = None;
    rebuild_results(data, snapshot, None);
    InputOutcome::Changed
}

fn page_selection(data: &mut DownloadPacksOverlayData, direction: isize) -> InputOutcome {
    let Some(last) = data.results.len().checked_sub(1) else {
        return InputOutcome::None;
    };
    let next = if direction < 0 {
        data.selected.saturating_sub(VIEW_ROWS)
    } else {
        data.selected.saturating_add(VIEW_ROWS).min(last)
    };
    if next == data.selected {
        return InputOutcome::None;
    }
    data.selected = next;
    data.local_message = None;
    InputOutcome::Changed
}

fn clear_query(data: &mut DownloadPacksOverlayData, snapshot: &Snapshot) -> InputOutcome {
    if data.query.is_empty() {
        return InputOutcome::None;
    }
    let selected_id = selected_pack(data, snapshot).map(|pack| pack.id);
    data.query.clear();
    data.blink_t = 0.0;
    rebuild_results(data, snapshot, selected_id);
    InputOutcome::Edited
}

fn add_query_text(data: &mut DownloadPacksOverlayData, snapshot: &Snapshot, text: &str) -> bool {
    let mut len = data.query.chars().count();
    let before = len;
    for ch in text.chars().filter(|ch| !ch.is_control()) {
        if len >= QUERY_MAX_CHARS {
            break;
        }
        data.query.push(ch);
        len += 1;
    }
    if len == before {
        return false;
    }
    data.blink_t = 0.0;
    rebuild_results(data, snapshot, None);
    true
}

fn active_substyle(data: &DownloadPacksOverlayData) -> Option<&SubstyleKey> {
    data.substyles
        .get(data.selected_substyle)
        .map(|button| &button.key)
}

fn rebuild_substyles(
    data: &mut DownloadPacksOverlayData,
    snapshot: &Snapshot,
    preserve: Option<&SubstyleKey>,
) {
    let mut named: Vec<(String, usize)> = Vec::new();
    let mut uncategorized = 0usize;
    for pack in snapshot.catalog.iter() {
        let Some(value) = pack
            .substyle
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            uncategorized += 1;
            continue;
        };
        let key = value.to_lowercase();
        if let Some((_, count)) = named.iter_mut().find(|(candidate, _)| candidate == &key) {
            *count += 1;
        } else {
            named.push((key, 1));
        }
    }
    named.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));

    let mut substyles = Vec::with_capacity(named.len() + 2);
    substyles.push(SubstyleButton {
        key: SubstyleKey::All,
        count: snapshot.catalog.len(),
    });
    substyles.extend(named.into_iter().map(|(name, count)| SubstyleButton {
        key: SubstyleKey::Named(name),
        count,
    }));
    if uncategorized > 0 {
        substyles.push(SubstyleButton {
            key: SubstyleKey::Uncategorized,
            count: uncategorized,
        });
    }
    data.selected_substyle = preserve
        .and_then(|key| substyles.iter().position(|button| &button.key == key))
        .unwrap_or(0);
    data.substyles = substyles;
}

fn pack_matches_substyle(pack: &PackInfo, key: &SubstyleKey) -> bool {
    match key {
        SubstyleKey::All => true,
        SubstyleKey::Named(expected) => pack
            .substyle
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| value.eq_ignore_ascii_case(expected)),
        SubstyleKey::Uncategorized => pack
            .substyle
            .as_deref()
            .is_none_or(|value| value.trim().is_empty()),
    }
}

fn rebuild_results(
    data: &mut DownloadPacksOverlayData,
    snapshot: &Snapshot,
    preserve_pack_id: Option<u64>,
) {
    let active = active_substyle(data).cloned().unwrap_or(SubstyleKey::All);
    data.results = search_catalog(&snapshot.catalog, &data.query)
        .into_iter()
        .filter(|&index| {
            snapshot
                .catalog
                .get(index)
                .is_some_and(|pack| pack_matches_substyle(pack, &active))
        })
        .collect();
    data.catalog_revision = snapshot.revision;
    data.selected = preserve_pack_id
        .and_then(|id| {
            data.results.iter().position(|&index| {
                snapshot
                    .catalog
                    .get(index)
                    .is_some_and(|pack| pack.id == id)
            })
        })
        .unwrap_or(0)
        .min(data.results.len().saturating_sub(1));
    data.confirm = None;
    data.local_message = None;
    data.nav_hold = None;
}

fn selected_pack<'a>(
    data: &DownloadPacksOverlayData,
    snapshot: &'a Snapshot,
) -> Option<&'a PackInfo> {
    let catalog_index = *data.results.get(data.selected)?;
    snapshot.catalog.get(catalog_index)
}

fn install_for_pack(snapshot: &Snapshot, pack_id: u64) -> Option<&InstallSnapshot> {
    snapshot
        .installs
        .iter()
        .find(|install| install.pack_id == pack_id)
}

fn canonical_pack_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn installed_pack_names(installed_packs: &[OptionsSongPackView]) -> HashSet<String> {
    let mut names = HashSet::with_capacity(installed_packs.len().saturating_mul(2));
    for pack in installed_packs {
        names.insert(canonical_pack_name(&pack.group_name));
        names.insert(canonical_pack_name(&pack.display_name));
    }
    names.remove("");
    names
}

fn pack_is_installed(
    pack: &PackInfo,
    snapshot: &Snapshot,
    installed_names: &HashSet<String>,
) -> bool {
    if install_for_pack(snapshot, pack.id)
        .is_some_and(|install| install.phase == InstallPhase::Installed)
    {
        return true;
    }
    let canonical = canonical_pack_name(&pack.name);
    if !canonical.is_empty() && installed_names.contains(&canonical) {
        return true;
    }
    let sanitized = deadsync_online::stepmaniaonline::sanitize_pack_name(&pack.name, pack.id);
    installed_names.contains(&canonical_pack_name(&sanitized))
}

pub(super) fn build_overlay(state: &State, active_color_index: i32) -> Option<Vec<Actor>> {
    let DownloadPacksOverlayState::Visible(data) = &state.download_packs_overlay else {
        return None;
    };
    let snapshot = &state.stepmaniaonline_snapshot;
    let accent = color::simply_love_rgba(active_color_index);
    let cx = screen_center_x();
    let cy = screen_center_y();
    let mut out = Vec::with_capacity(80);

    out.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0): zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.90): z(Z)
    ));
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(PANEL_W + 4.0, PANEL_H + 4.0):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z + 1)
    ));
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(PANEL_W, PANEL_H):
        diffuse(0.025, 0.025, 0.035, 0.99): z(Z + 2)
    ));
    out.push(act!(quad:
        align(0.0, 0.0): xy(cx - PANEL_W * 0.5, cy - PANEL_H * 0.5):
        zoomto(7.0, PANEL_H): diffuse(accent[0], accent[1], accent[2], 0.86): z(Z + 3)
    ));
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy - PANEL_H * 0.5 + HEADER_H * 0.5):
        zoomto(PANEL_W, HEADER_H): diffuse(0.0, 0.0, 0.0, 0.92): z(Z + 4)
    ));
    out.push(act!(text:
        font(current_machine_font_key(FontRole::Header)):
        settext(tr("OptionsDownloadPacks", "Title")):
        align(0.0, 0.5): xy(cx - PANEL_W * 0.5 + 18.0, cy - 188.0):
        zoom(0.42): diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
    ));
    out.push(act!(text:
        font(current_machine_font_key(FontRole::Bold)):
        settext(tr("OptionsDownloadPacks", "Source")):
        align(1.0, 0.5): xy(cx + PANEL_W * 0.5 - 16.0, cy - 194.0):
        zoom(0.24): diffuse(accent[0], accent[1], accent[2], 1.0): z(Z + 6): horizalign(right)
    ));
    let shown = data.results.len().to_string();
    let total = snapshot.catalog.len().to_string();
    out.push(act!(text:
        font("miso"):
        settext(tr_fmt(
            "OptionsDownloadPacks",
            "CatalogCount",
            &[("shown", &shown), ("total", &total)],
        )):
        align(1.0, 0.5): xy(cx + PANEL_W * 0.5 - 16.0, cy - 180.0):
        zoom(0.66): diffuse(0.72, 0.72, 0.76, 1.0): z(Z + 6): horizalign(right)
    ));

    push_search(&mut out, data, accent, cx, cy);
    push_substyle_buttons(&mut out, data, accent, cx, cy);
    match snapshot.phase {
        CatalogPhase::Idle | CatalogPhase::Loading => {
            let message = snapshot
                .message
                .clone()
                .unwrap_or_else(|| tr("OptionsDownloadPacks", "Loading").to_string());
            push_status(&mut out, &message, [1.0, 1.0, 1.0, 1.0], cx, cy);
        }
        CatalogPhase::Error => {
            let message = snapshot
                .message
                .clone()
                .unwrap_or_else(|| tr("OptionsDownloadPacks", "LoadError").to_string());
            push_status(&mut out, &message, [1.0, 0.43, 0.34, 1.0], cx, cy);
        }
        CatalogPhase::Ready if data.results.is_empty() => push_status(
            &mut out,
            tr("OptionsDownloadPacks", "Empty").as_ref(),
            [0.85, 0.85, 0.88, 1.0],
            cx,
            cy,
        ),
        CatalogPhase::Ready => {
            push_catalog(&mut out, data, snapshot, accent, cx, cy);
        }
    }
    push_footer(&mut out, snapshot, accent, cx, cy);
    if let Some(confirm) = data.confirm.as_ref() {
        push_confirmation(&mut out, confirm, accent, cx, cy);
    }
    Some(out)
}

fn push_search(
    out: &mut Vec<Actor>,
    data: &DownloadPacksOverlayData,
    accent: [f32; 4],
    cx: f32,
    cy: f32,
) {
    let y = cy - 145.0;
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, y): zoomto(PANEL_W - 28.0, SEARCH_H):
        diffuse(0.0, 0.0, 0.0, 0.84): z(Z + 4)
    ));
    out.push(act!(quad:
        align(0.0, 0.5): xy(cx - PANEL_W * 0.5 + 14.0, y): zoomto(4.0, SEARCH_H):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z + 5)
    ));
    out.push(act!(text:
        font(current_machine_font_key(FontRole::Bold)):
        settext(tr("OptionsDownloadPacks", "SearchLabel")):
        align(0.0, 0.5): xy(cx - PANEL_W * 0.5 + 28.0, y): zoom(0.25):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z + 6): horizalign(left)
    ));
    let cursor = if data.blink_t < CURSOR_PERIOD * 0.5 {
        "_"
    } else {
        " "
    };
    let value = if data.query.is_empty() {
        format!(
            "{} {cursor}",
            tr("OptionsDownloadPacks", "SearchPlaceholder")
        )
    } else {
        format!("> {}{cursor}", data.query)
    };
    out.push(act!(text:
        font("miso"): settext(value): align(0.0, 0.5):
        xy(cx - PANEL_W * 0.5 + 105.0, y): zoom(0.82): maxwidth(PANEL_W - 138.0):
        diffuse(
            if data.query.is_empty() { 0.60 } else { 1.0 },
            if data.query.is_empty() { 0.60 } else { 1.0 },
            if data.query.is_empty() { 0.64 } else { 1.0 },
            1.0
        ): z(Z + 6): horizalign(left)
    ));
}

fn push_substyle_buttons(
    out: &mut Vec<Actor>,
    data: &DownloadPacksOverlayData,
    accent: [f32; 4],
    cx: f32,
    cy: f32,
) {
    let count = data.substyles.len().max(1);
    let total_width = PANEL_W - 28.0;
    let gap = 3.0;
    let button_width = (total_width - gap * count.saturating_sub(1) as f32) / count as f32;
    let left = cx - total_width * 0.5;
    let y = cy - 108.0;
    for (index, button) in data.substyles.iter().enumerate() {
        let x = left + button_width * 0.5 + index as f32 * (button_width + gap);
        let active = index == data.selected_substyle;
        out.push(act!(quad:
            align(0.5, 0.5): xy(x, y): zoomto(button_width, FILTER_H):
            diffuse(accent[0], accent[1], accent[2], if active { 0.88 } else { 0.13 }):
            z(Z + 5)
        ));
        out.push(act!(text:
            font(current_machine_font_key(FontRole::Bold)):
            settext(substyle_label(&button.key)):
            align(0.5, 0.5): xy(x, y - 4.0): zoom(0.19): maxwidth(button_width - 8.0):
            diffuse(1.0, 1.0, 1.0, if active { 1.0 } else { 0.67 }):
            z(Z + 6): horizalign(center)
        ));
        out.push(act!(text:
            font("miso"): settext(button.count.to_string()):
            align(0.5, 0.5): xy(x, y + 7.0): zoom(0.52): maxwidth(button_width - 8.0):
            diffuse(0.90, 0.90, 0.93, if active { 1.0 } else { 0.55 }):
            z(Z + 6): horizalign(center)
        ));
    }
}

fn substyle_label(key: &SubstyleKey) -> String {
    match key {
        SubstyleKey::All => tr("OptionsDownloadPacks", "AllSubstyles").to_string(),
        SubstyleKey::Named(value) => value.to_uppercase(),
        SubstyleKey::Uncategorized => {
            tr("OptionsDownloadPacks", "UncategorizedSubstyle").to_string()
        }
    }
}

fn push_catalog(
    out: &mut Vec<Actor>,
    data: &DownloadPacksOverlayData,
    snapshot: &Snapshot,
    accent: [f32; 4],
    cx: f32,
    cy: f32,
) {
    let selected = data.selected.min(data.results.len().saturating_sub(1));
    let start = selected
        .saturating_sub(VIEW_ROWS / 2)
        .min(data.results.len().saturating_sub(VIEW_ROWS));
    let list_x = cx + 157.0;
    let list_top = cy - 74.0;
    out.push(act!(quad:
        align(0.5, 0.5): xy(list_x, cy + 47.0): zoomto(PANE_W, 274.0):
        diffuse(0.0, 0.0, 0.0, 0.76): z(Z + 4)
    ));
    for (slot, &catalog_index) in data.results.iter().skip(start).take(VIEW_ROWS).enumerate() {
        let Some(pack) = snapshot.catalog.get(catalog_index) else {
            continue;
        };
        let row_index = start + slot;
        let active = row_index == selected;
        let y = list_top + slot as f32 * ROW_H;
        out.push(act!(quad:
            align(0.5, 0.5): xy(list_x, y): zoomto(PANE_W - 8.0, ROW_H - 3.0):
            diffuse(accent[0], accent[1], accent[2], if active { 0.82 } else { 0.12 }):
            z(Z + 5)
        ));
        out.push(act!(text:
            font(current_machine_font_key(FontRole::Bold)): settext(pack.name.clone()):
            align(0.0, 0.5): xy(list_x - PANE_W * 0.5 + 9.0, y - 5.0):
            zoom(0.25): maxwidth(258.0):
            diffuse(1.0, 1.0, 1.0, if active { 1.0 } else { 0.75 }):
            z(Z + 6): horizalign(left)
        ));
        out.push(act!(text:
            font("miso"): settext(pack_row_detail(pack, snapshot, &data.installed_names)):
            align(0.0, 0.5): xy(list_x - PANE_W * 0.5 + 9.0, y + 8.0):
            zoom(0.67): maxwidth(258.0):
            diffuse(0.88, 0.88, 0.90, if active { 1.0 } else { 0.60 }):
            z(Z + 6): horizalign(left)
        ));
    }
    if let Some(pack) = selected_pack(data, snapshot) {
        push_pack_detail(out, data, pack, snapshot, accent, cx, cy);
    }
}

fn push_pack_detail(
    out: &mut Vec<Actor>,
    data: &DownloadPacksOverlayData,
    pack: &PackInfo,
    snapshot: &Snapshot,
    accent: [f32; 4],
    cx: f32,
    cy: f32,
) {
    let x = cx - PANEL_W * 0.5 + 15.0;
    let install = install_for_pack(snapshot, pack.id);
    let installed = pack_is_installed(pack, snapshot, &data.installed_names);
    out.push(act!(quad:
        align(0.0, 0.0): xy(x, cy - 88.0): zoomto(PANE_W, 274.0):
        diffuse(0.0, 0.0, 0.0, 0.76): z(Z + 4)
    ));
    out.push(act!(text:
        font(current_machine_font_key(FontRole::Bold)): settext(pack.name.clone()):
        align(0.0, 0.5): xy(x + 9.0, cy - 69.0): zoom(0.34): maxwidth(262.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 6): horizalign(left)
    ));
    out.push(act!(text:
        font("miso"): settext(format!("SMO #{}", pack.id)):
        align(1.0, 0.5): xy(x + PANE_W - 9.0, cy - 50.0): zoom(0.64):
        diffuse(0.62, 0.62, 0.66, 1.0): z(Z + 6): horizalign(right)
    ));
    let (status, status_color) = pack_status(install, installed);
    out.push(act!(text:
        font(current_machine_font_key(FontRole::Bold)): settext(status):
        align(0.0, 0.5): xy(x + 9.0, cy - 49.0): zoom(0.25): maxwidth(205.0):
        diffuse(status_color[0], status_color[1], status_color[2], status_color[3]):
        z(Z + 6): horizalign(left)
    ));

    let songs_label = tr("OptionsDownloadPacks", "Songs");
    let size_label = tr("OptionsDownloadPacks", "Size");
    let pack_type_label = tr("OptionsDownloadPacks", "PackType");
    let substyle_label = tr("OptionsDownloadPacks", "Substyle");
    let sync_label = tr("OptionsDownloadPacks", "Sync");
    let min_version_label = tr("OptionsDownloadPacks", "MinVersion");
    let pack_type = optional_meta(pack.pack_type.as_deref());
    let substyle = optional_meta(pack.substyle.as_deref());
    let sync = optional_meta(pack.sync.as_deref());
    let min_version = optional_meta(pack.min_version.as_deref());
    push_meta_pair(
        out,
        x + 9.0,
        cy - 22.0,
        &songs_label,
        &pack.song_count.to_string(),
        &size_label,
        &format_bytes(pack.size_bytes),
        accent,
    );
    push_meta_pair(
        out,
        x + 9.0,
        cy + 27.0,
        &pack_type_label,
        pack_type.as_ref(),
        &substyle_label,
        substyle.as_ref(),
        accent,
    );
    push_meta_pair(
        out,
        x + 9.0,
        cy + 76.0,
        &sync_label,
        sync.as_ref(),
        &min_version_label,
        min_version.as_ref(),
        accent,
    );

    if let Some(install) = install
        && matches!(install.phase, InstallPhase::Downloading)
    {
        push_progress(out, install, accent, x + 9.0, cy + 132.0);
    }
    let message = data.local_message.clone().unwrap_or_else(|| {
        if installed {
            tr("OptionsDownloadPacks", "AlreadyInstalled").to_string()
        } else if let Some(install) = install {
            install_status_text(install)
        } else {
            tr("OptionsDownloadPacks", "Ready").to_string()
        }
    });
    out.push(act!(text:
        font(current_machine_font_key(FontRole::Bold)): settext(message):
        align(0.0, 1.0): xy(x + 9.0, cy + 177.0): zoom(0.24): maxwidth(264.0):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z + 6): horizalign(left)
    ));
}

#[allow(clippy::too_many_arguments)]
fn push_meta_pair(
    out: &mut Vec<Actor>,
    x: f32,
    y: f32,
    left_label: &str,
    left_value: &str,
    right_label: &str,
    right_value: &str,
    accent: [f32; 4],
) {
    out.push(act!(text:
        font(current_machine_font_key(FontRole::Bold)): settext(left_label.to_owned()):
        align(0.0, 0.5): xy(x, y): zoom(0.21): maxwidth(124.0):
        diffuse(accent[0], accent[1], accent[2], 0.92): z(Z + 6): horizalign(left)
    ));
    out.push(act!(text:
        font("miso"): settext(left_value.to_owned()): align(0.0, 0.5):
        xy(x, y + 15.0): zoom(0.72): maxwidth(124.0):
        diffuse(0.94, 0.94, 0.96, 1.0): z(Z + 6): horizalign(left)
    ));
    out.push(act!(text:
        font(current_machine_font_key(FontRole::Bold)): settext(right_label.to_owned()):
        align(0.0, 0.5): xy(x + 137.0, y): zoom(0.21): maxwidth(124.0):
        diffuse(accent[0], accent[1], accent[2], 0.92): z(Z + 6): horizalign(left)
    ));
    out.push(act!(text:
        font("miso"): settext(right_value.to_owned()): align(0.0, 0.5):
        xy(x + 137.0, y + 15.0): zoom(0.72): maxwidth(124.0):
        diffuse(0.94, 0.94, 0.96, 1.0): z(Z + 6): horizalign(left)
    ));
}

fn push_progress(
    out: &mut Vec<Actor>,
    install: &InstallSnapshot,
    accent: [f32; 4],
    x: f32,
    y: f32,
) {
    let progress = install_progress(install);
    let width = 268.0;
    out.push(act!(quad:
        align(0.0, 0.5): xy(x, y): zoomto(width, 8.0):
        diffuse(0.22, 0.22, 0.24, 1.0): z(Z + 6)
    ));
    if progress > 0.0 {
        out.push(act!(quad:
            align(0.0, 0.5): xy(x, y): zoomto(width * progress, 8.0):
            diffuse(accent[0], accent[1], accent[2], 1.0): z(Z + 7)
        ));
    }
}

fn push_status(out: &mut Vec<Actor>, message: &str, rgba: [f32; 4], cx: f32, cy: f32) {
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy + 22.0): zoomto(540.0, 128.0):
        diffuse(0.0, 0.0, 0.0, 0.82): z(Z + 5)
    ));
    out.push(act!(text:
        font(current_machine_font_key(FontRole::Bold)): settext(message.to_owned()):
        align(0.5, 0.5): xy(cx, cy + 10.0): zoom(0.34):
        wrapwidthpixels(1050.0): maxwidth(510.0):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]): z(Z + 6): horizalign(center)
    ));
}

fn push_footer(out: &mut Vec<Actor>, snapshot: &Snapshot, accent: [f32; 4], cx: f32, cy: f32) {
    let active_download = snapshot.installs.iter().any(|install| {
        matches!(
            install.phase,
            InstallPhase::Queued | InstallPhase::Downloading | InstallPhase::Extracting
        )
    });
    let hint = if snapshot.phase == CatalogPhase::Error {
        tr("OptionsDownloadPacks", "RefreshHint")
    } else if active_download {
        tr("OptionsDownloadPacks", "FooterBusy")
    } else {
        tr("OptionsDownloadPacks", "Footer")
    };
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy + PANEL_H * 0.5 - 18.0): zoomto(PANEL_W, 36.0):
        diffuse(0.0, 0.0, 0.0, 0.94): z(Z + 5)
    ));
    out.push(act!(quad:
        align(0.0, 0.5): xy(cx - PANEL_W * 0.5, cy + PANEL_H * 0.5 - 18.0):
        zoomto(7.0, 36.0): diffuse(accent[0], accent[1], accent[2], 0.86): z(Z + 6)
    ));
    out.push(act!(text:
        font("miso"): settext(hint): align(0.5, 0.5):
        xy(cx, cy + PANEL_H * 0.5 - 18.0): zoom(0.68): maxwidth(PANEL_W - 28.0):
        diffuse(1.0, 1.0, 1.0, 0.86): z(Z + 6): horizalign(center)
    ));
}

fn push_confirmation(
    out: &mut Vec<Actor>,
    confirm: &DownloadConfirm,
    accent: [f32; 4],
    cx: f32,
    cy: f32,
) {
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(PANEL_W, PANEL_H):
        diffuse(0.0, 0.0, 0.0, 0.72): z(Z + 18)
    ));
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(460.0, 210.0):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z + 20)
    ));
    out.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy): zoomto(456.0, 206.0):
        diffuse(0.02, 0.02, 0.025, 0.99): z(Z + 21)
    ));
    out.push(act!(text:
        font(current_machine_font_key(FontRole::Header)):
        settext(tr("OptionsDownloadPacks", "ConfirmTitle")):
        align(0.5, 0.5): xy(cx, cy - 78.0): zoom(0.36):
        diffuse(accent[0], accent[1], accent[2], 1.0): z(Z + 22): horizalign(center)
    ));
    let size = format_bytes(confirm.size_bytes);
    out.push(act!(text:
        font(current_machine_font_key(FontRole::Bold)):
        settext(tr_fmt(
            "OptionsDownloadPacks",
            "ConfirmBody",
            &[("name", &confirm.name), ("size", &size)],
        )):
        align(0.5, 0.5): xy(cx, cy - 33.0): zoom(0.29): maxwidth(420.0):
        diffuse(1.0, 1.0, 1.0, 1.0): z(Z + 22): horizalign(center)
    ));
    out.push(act!(text:
        font("miso"): settext(tr("OptionsDownloadPacks", "ConfirmWarning")):
        align(0.5, 0.5): xy(cx, cy + 6.0): zoom(0.67): maxwidth(420.0):
        diffuse(0.74, 0.74, 0.78, 1.0): z(Z + 22): horizalign(center)
    ));
    for (index, key) in ["Yes", "No"].into_iter().enumerate() {
        let x = cx + (index as f32 - 0.5) * 156.0;
        let active = confirm.choice == index;
        out.push(act!(quad:
            align(0.5, 0.5): xy(x, cy + 66.0): zoomto(138.0, 38.0):
            diffuse(accent[0], accent[1], accent[2], if active { 0.88 } else { 0.16 }):
            z(Z + 22)
        ));
        out.push(act!(text:
            font(current_machine_font_key(FontRole::Bold)):
            settext(tr("OptionsDownloadPacks", key)):
            align(0.5, 0.5): xy(x, cy + 66.0): zoom(0.25):
            diffuse(1.0, 1.0, 1.0, if active { 1.0 } else { 0.65 }):
            z(Z + 23): horizalign(center)
        ));
    }
}

fn optional_meta(value: Option<&str>) -> Cow<'_, str> {
    value.filter(|value| !value.trim().is_empty()).map_or_else(
        || Cow::Owned(tr("OptionsDownloadPacks", "Unknown").to_string()),
        Cow::Borrowed,
    )
}

fn pack_row_detail(
    pack: &PackInfo,
    snapshot: &Snapshot,
    installed_names: &HashSet<String>,
) -> String {
    if pack_is_installed(pack, snapshot, installed_names) {
        return tr("OptionsDownloadPacks", "Installed").to_string();
    }
    if let Some(install) = install_for_pack(snapshot, pack.id) {
        if install.phase == InstallPhase::Error {
            return tr("OptionsDownloadPacks", "FailedShort").to_string();
        }
        return install_status_text(install);
    }
    format!(
        "{} SONGS  •  {}",
        pack.song_count,
        format_bytes(pack.size_bytes)
    )
}

fn pack_status(install: Option<&InstallSnapshot>, installed: bool) -> (String, [f32; 4]) {
    if installed {
        return (
            tr("OptionsDownloadPacks", "Installed").to_string(),
            [0.36, 1.0, 0.54, 1.0],
        );
    }
    if let Some(install) = install {
        let color = if install.phase == InstallPhase::Error {
            [1.0, 0.40, 0.32, 1.0]
        } else {
            [1.0, 0.83, 0.28, 1.0]
        };
        return (install_status_text(install), color);
    }
    (
        tr("OptionsDownloadPacks", "Ready").to_string(),
        [0.82, 0.82, 0.86, 1.0],
    )
}

fn install_status_text(install: &InstallSnapshot) -> String {
    match install.phase {
        InstallPhase::Queued => tr("OptionsDownloadPacks", "Queued").to_string(),
        InstallPhase::Downloading => {
            let progress = format!("{:.0}", install_progress(install) * 100.0);
            tr_fmt(
                "OptionsDownloadPacks",
                "Downloading",
                &[("progress", &progress)],
            )
            .to_string()
        }
        InstallPhase::Extracting => tr("OptionsDownloadPacks", "Extracting").to_string(),
        InstallPhase::Installed => tr("OptionsDownloadPacks", "Installed").to_string(),
        InstallPhase::Error => {
            let error = install
                .message
                .clone()
                .unwrap_or_else(|| tr("OptionsDownloadPacks", "LoadError").to_string());
            tr_fmt("OptionsDownloadPacks", "Failed", &[("error", &error)]).to_string()
        }
    }
}

fn install_progress(install: &InstallSnapshot) -> f32 {
    if install.total_bytes == 0 {
        0.0
    } else {
        (install.downloaded_bytes as f64 / install.total_bytes as f64).clamp(0.0, 1.0) as f32
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pack(id: u64, name: &str, substyle: Option<&str>) -> PackInfo {
        PackInfo::new(
            id,
            name.to_string(),
            10,
            1_000,
            None,
            None,
            substyle.map(str::to_string),
            None,
        )
    }

    fn test_snapshot(packs: Vec<PackInfo>) -> Snapshot {
        Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from(packs),
            revision: 1,
            message: None,
            installs: Vec::new(),
        }
    }

    fn raw_key(code: KeyCode, pressed: bool, repeat: bool) -> RawKeyboardEvent {
        RawKeyboardEvent {
            code,
            pressed,
            repeat,
            timestamp: Instant::now(),
            host_nanos: 0,
        }
    }

    fn test_overlay_data() -> DownloadPacksOverlayData {
        DownloadPacksOverlayData {
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            substyles: vec![SubstyleButton {
                key: SubstyleKey::All,
                count: 0,
            }],
            selected_substyle: 0,
            catalog_revision: 0,
            blink_t: 0.0,
            nav_hold: None,
            tab_held: false,
            confirm: None,
            local_message: None,
            installed_names: HashSet::new(),
        }
    }

    #[test]
    fn byte_sizes_stay_readable_across_pack_scales() {
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1_048_576), "1.0 MiB");
        assert_eq!(format_bytes(3_221_225_472), "3.00 GiB");
    }

    #[test]
    fn installed_name_comparison_ignores_punctuation_and_case() {
        assert_eq!(
            canonical_pack_name("DDRMAX 2: Oni Edits"),
            canonical_pack_name("ddrmax-2 oni edits")
        );
    }

    #[test]
    fn installed_name_comparison_recognizes_sanitized_destinations() {
        let pack = PackInfo::new(
            42,
            "CON: Arcade Pack".to_string(),
            1,
            1024,
            None,
            None,
            None,
            None,
        );
        let installed = deadsync_online::stepmaniaonline::sanitize_pack_name(&pack.name, pack.id);
        let installed_names = HashSet::from([canonical_pack_name(&installed)]);
        assert!(pack_is_installed(
            &pack,
            &Snapshot::default(),
            &installed_names
        ));
    }

    #[test]
    fn selection_pages_without_running_past_results() {
        let mut data = test_overlay_data();
        data.results = (0..20).collect();
        data.selected = 18;
        assert_eq!(page_selection(&mut data, 1), InputOutcome::Changed);
        assert_eq!(data.selected, 19);
        assert_eq!(page_selection(&mut data, 1), InputOutcome::None);
        assert_eq!(page_selection(&mut data, -1), InputOutcome::Changed);
        assert_eq!(data.selected, 12);
    }

    #[test]
    fn substyle_buttons_are_stable_and_include_uncategorized() {
        let snapshot = test_snapshot(vec![
            test_pack(1, "Technical", Some("technical")),
            test_pack(2, "Stamina A", Some("stamina")),
            test_pack(3, "Unsorted", None),
            test_pack(4, "Stamina B", Some(" STAMINA ")),
        ]);
        let mut data = test_overlay_data();
        rebuild_substyles(&mut data, &snapshot, None);
        let keys: Vec<_> = data
            .substyles
            .iter()
            .map(|button| button.key.clone())
            .collect();
        assert_eq!(
            keys,
            vec![
                SubstyleKey::All,
                SubstyleKey::Named("stamina".to_string()),
                SubstyleKey::Named("technical".to_string()),
                SubstyleKey::Uncategorized,
            ]
        );
        assert_eq!(
            data.substyles
                .iter()
                .map(|button| button.count)
                .collect::<Vec<_>>(),
            vec![4, 2, 1, 1]
        );
    }

    #[test]
    fn search_and_substyle_filters_intersect() {
        let snapshot = test_snapshot(vec![
            test_pack(1, "Alpha Stamina", Some("stamina")),
            test_pack(2, "Alpha Technical", Some("technical")),
            test_pack(3, "Alpha Classics", None),
        ]);
        let mut data = test_overlay_data();
        data.query = "alpha".to_string();
        rebuild_substyles(&mut data, &snapshot, None);
        data.selected_substyle = data
            .substyles
            .iter()
            .position(|button| button.key == SubstyleKey::Named("stamina".to_string()))
            .expect("stamina filter exists");
        rebuild_results(&mut data, &snapshot, None);
        assert_eq!(data.results, vec![0]);
    }

    #[test]
    fn substyle_navigation_wraps() {
        let snapshot = test_snapshot(vec![
            test_pack(1, "Technical", Some("technical")),
            test_pack(2, "Unsorted", None),
        ]);
        let mut data = test_overlay_data();
        rebuild_substyles(&mut data, &snapshot, None);
        rebuild_results(&mut data, &snapshot, None);
        assert_eq!(
            move_substyle(&mut data, &snapshot, -1),
            InputOutcome::Changed
        );
        assert_eq!(active_substyle(&data), Some(&SubstyleKey::Uncategorized));
    }

    #[test]
    fn held_navigation_matches_srpg_timing_and_tab_turbo() {
        let mut state = DownloadPacksOverlayState::Visible(Box::new(test_overlay_data()));
        let DownloadPacksOverlayState::Visible(data) = &mut state else {
            unreachable!();
        };
        data.results = (0..4).collect();
        start_nav_hold(&mut state, 1);
        assert_eq!(repeat_nav_hold(&mut state, 0.374), InputOutcome::None);
        assert_eq!(repeat_nav_hold(&mut state, 0.002), InputOutcome::Changed);
        assert_eq!(repeat_nav_hold(&mut state, 0.124), InputOutcome::None);
        assert_eq!(repeat_nav_hold(&mut state, 0.002), InputOutcome::Changed);

        start_nav_hold(&mut state, 1);
        let DownloadPacksOverlayState::Visible(data) = &mut state else {
            unreachable!();
        };
        data.tab_held = true;
        assert_eq!(repeat_nav_hold(&mut state, 0.094), InputOutcome::Changed);
        release_nav_hold(&mut state, 1);
        assert_eq!(repeat_nav_hold(&mut state, 1.0), InputOutcome::None);
    }

    #[test]
    fn tab_acceleration_tracks_press_and_release() {
        let mut state = DownloadPacksOverlayState::Visible(Box::new(test_overlay_data()));
        assert_eq!(
            handle_raw_shortcut(&mut state, Some(&raw_key(KeyCode::Tab, true, false))),
            Some(InputOutcome::None)
        );
        let DownloadPacksOverlayState::Visible(data) = &state else {
            unreachable!();
        };
        assert!(data.tab_held);
        assert_eq!(
            handle_raw_shortcut(&mut state, Some(&raw_key(KeyCode::Tab, false, false))),
            Some(InputOutcome::None)
        );
        let DownloadPacksOverlayState::Visible(data) = state else {
            unreachable!();
        };
        assert!(!data.tab_held);
    }

    #[test]
    fn three_key_navigation_moves_rows_and_toggles_confirmation() {
        let mut data = test_overlay_data();
        data.results = (0..3).collect();
        let mut state = DownloadPacksOverlayState::Visible(Box::new(data));
        assert_eq!(
            handle_three_key_input(
                &mut state,
                screen_input::ThreeKeyMenuAction::Next,
                &Snapshot::default(),
            ),
            InputOutcome::Changed
        );
        let DownloadPacksOverlayState::Visible(data) = &mut state else {
            unreachable!();
        };
        assert_eq!(data.selected, 1);
        data.confirm = Some(DownloadConfirm {
            pack_id: 7,
            name: "Test Pack".to_string(),
            size_bytes: 1024,
            choice: 0,
        });
        assert_eq!(
            handle_three_key_input(
                &mut state,
                screen_input::ThreeKeyMenuAction::Prev,
                &Snapshot::default(),
            ),
            InputOutcome::Changed
        );
        let DownloadPacksOverlayState::Visible(data) = state else {
            unreachable!();
        };
        assert_eq!(data.confirm.expect("confirmation remains open").choice, 1);
    }

    #[test]
    fn query_input_is_bounded_and_ignores_controls() {
        let mut data = test_overlay_data();
        let text = format!("\n{}", "x".repeat(QUERY_MAX_CHARS + 20));
        assert!(add_query_text(&mut data, &Snapshot::default(), &text));
        assert_eq!(data.query.chars().count(), QUERY_MAX_CHARS);
        assert!(!data.query.contains('\n'));
    }

    #[test]
    fn text_does_not_dismiss_download_confirmation() {
        let mut state = DownloadPacksOverlayState::Visible(Box::new(test_overlay_data()));
        let DownloadPacksOverlayState::Visible(data) = &mut state else {
            unreachable!();
        };
        data.confirm = Some(DownloadConfirm {
            pack_id: 7,
            name: "Test Pack".to_string(),
            size_bytes: 1024,
            choice: 0,
        });
        assert_eq!(
            handle_raw_input(&mut state, None, Some("x"), &Snapshot::default()),
            InputOutcome::None
        );
        let DownloadPacksOverlayState::Visible(data) = state else {
            unreachable!();
        };
        assert!(data.confirm.is_some());
        assert!(data.query.is_empty());
    }
}
