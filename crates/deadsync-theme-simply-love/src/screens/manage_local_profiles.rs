use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::{FontRole, machine_font_key};
use crate::screens::components::shared::loading_bar;
use crate::screens::components::shared::screen_bar::{
    self, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::shared::transitions;
use crate::screens::components::shared::visual_style_bg;
use crate::screens::input as screen_input;
use crate::screens::{Screen, ThemeEffect};
use crate::views::{LocalProfileView, ManageLocalProfilesView};
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_height, screen_width};
use deadsync_input::KeyCode;
use deadsync_input::RawKeyboardEvent;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile::PlayerSide;
use deadsync_profile::favorites_view::unicode_case_insensitive_cmp;
use std::sync::Arc;
use std::time::{Duration, Instant};

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* -------------------------- hold-to-scroll timing ------------------------- */
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);
const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);

/* --------------------------------- layout -------------------------------- */
/// Bars in `screen_bar.rs` use 32.0 px height.
const BAR_H: f32 = 32.0;
/// Screen-space margins (pixels, not scaled)
const LEFT_MARGIN_PX: f32 = 33.0;
const RIGHT_MARGIN_PX: f32 = 25.0;
const FIRST_ROW_TOP_MARGIN_PX: f32 = 18.0;
const BOTTOM_MARGIN_PX: f32 = 0.0;

const VISIBLE_ROWS: usize = 10;
const ROW_H: f32 = 33.0;
const ROW_GAP: f32 = 2.5;
const LIST_W: f32 = 509.0;
const SEP_W: f32 = 2.5;
const DESC_W: f32 = 292.0;
const DESC_H: f32 = (VISIBLE_ROWS as f32) * ROW_H + ((VISIBLE_ROWS - 1) as f32) * ROW_GAP;

const HEART_LEFT_PAD: f32 = 13.0;
const TEXT_LEFT_PAD: f32 = 40.66;
const ITEM_TEXT_ZOOM: f32 = 0.88;
const HEART_ZOOM: f32 = 0.026;

const DESC_TITLE_TOP_PAD_PX: f32 = 9.75;
const DESC_TITLE_SIDE_PAD_PX: f32 = 7.5;
const DESC_BULLET_TOP_PAD_PX: f32 = 23.25;
const DESC_BULLET_SIDE_PAD_PX: f32 = 7.5;
const DESC_BULLET_INDENT_PX: f32 = 10.0;
const DESC_TITLE_ZOOM: f32 = 1.0;
const DESC_BODY_ZOOM: f32 = 1.0;

const NAME_MAX_LEN: usize = 32;
const PROFILE_MENU_W: f32 = 450.0;
const PROFILE_MENU_HEADER_H: f32 = 56.0;
const PROFILE_MENU_ITEM_H: f32 = 44.0;
const PROFILE_MENU_BORDER: f32 = 3.0;

#[derive(Clone, Debug)]
enum RowKind {
    CreateNew,
    ImportItg,
    Profile { id: String, display_name: String },
    Exit,
}

#[derive(Clone, Debug)]
struct Row {
    kind: RowKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavWrap {
    Wrap,
    Clamp,
}

#[derive(Clone, Debug)]
struct NameEntryState {
    mode: NameEntryMode,
    value: String,
    error: Option<Arc<str>>,
    blink_t: f32,
}

#[derive(Clone, Debug)]
enum NameEntryMode {
    Create,
    Rename { id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileMenuAction {
    SetP1,
    SetP2,
    LinkArrowCloud,
    LinkGrooveStats,
    Rename,
    Delete,
}

fn profile_menu_action_label(action: ProfileMenuAction) -> Arc<str> {
    match action {
        ProfileMenuAction::SetP1 => tr("Profiles", "SetP1"),
        ProfileMenuAction::SetP2 => tr("Profiles", "SetP2"),
        ProfileMenuAction::LinkArrowCloud => tr("Profiles", "LinkArrowCloud"),
        ProfileMenuAction::LinkGrooveStats => tr("Profiles", "LinkGrooveStats"),
        ProfileMenuAction::Rename => tr("Profiles", "Rename"),
        ProfileMenuAction::Delete => tr("Profiles", "Delete"),
    }
}

const PROFILE_MENU_ACTIONS: [ProfileMenuAction; 6] = [
    ProfileMenuAction::SetP1,
    ProfileMenuAction::SetP2,
    ProfileMenuAction::LinkArrowCloud,
    ProfileMenuAction::LinkGrooveStats,
    ProfileMenuAction::Rename,
    ProfileMenuAction::Delete,
];

#[derive(Clone, Debug)]
struct ProfileMenuState {
    id: String,
    display_name: String,
    selected_action: usize,
}

#[derive(Clone, Debug)]
struct DeleteConfirmState {
    id: String,
    display_name: String,
    error: Option<Arc<str>>,
}

/// Modal listing ITGmania profiles found on disk, for the user to pick one to
/// import. A trailing synthetic "Browse for game directory…" row (selectable
/// index `candidates.len()`) lets the user point at a portable install.
struct ImportPickerState {
    candidates: Vec<crate::SimplyLoveItgProfileCandidate>,
    selected: usize,
    /// Transient notice shown under the list (e.g. after a browse found nothing).
    info: Option<Arc<str>>,
}

impl ImportPickerState {
    /// The index of the synthetic "Browse…" row.
    fn browse_index(&self) -> usize {
        self.candidates.len()
    }

    /// `true` when the "Browse…" row is currently selected.
    fn browse_selected(&self) -> bool {
        self.selected == self.browse_index()
    }

    /// The existing-profile name if the candidate at `idx` is already imported.
    fn imported_as_at(&self, idx: usize) -> Option<&str> {
        self.candidates
            .get(idx)
            .and_then(|candidate| candidate.imported_as.as_deref())
    }
}

/// Theme-owned presentation state for a shell-owned import worker.
struct ImportJob {
    /// Latest `(done, total, song label)` reported by the worker, if any.
    progress: Option<(usize, usize, Arc<str>)>,
    /// `(instant, done)` captured at the first determinate progress tick, used
    /// as the baseline for the score-write rate and ETA estimate.
    save_anchor: Option<(Instant, usize)>,
    cancel_requested: bool,
}

/// Outcome of one import section, mapped to a status icon and color in the
/// two-column summary ledger.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SectionStatus {
    /// Brought over in full.
    Imported,
    /// Imported, but some items were skipped (e.g. scores for charts not in the
    /// library, or favorites whose song is missing).
    Partial,
    /// Nothing to import — not an error (e.g. no ArrowCloud key, no avatar).
    Skipped,
}

impl SectionStatus {
    /// The status glyph (proven to render in the menu font — see the evaluation
    /// submit footer).
    fn icon(self) -> &'static str {
        match self {
            Self::Imported | Self::Partial => "✔",
            Self::Skipped => "⊘",
        }
    }

    /// Icon / status-text color: green when imported, amber when partial, gray
    /// when skipped.
    fn rgba(self) -> [f32; 4] {
        match self {
            Self::Imported => [0.55, 0.92, 0.55, 1.0],
            Self::Partial => [0.96, 0.78, 0.36, 1.0],
            Self::Skipped => [0.62, 0.62, 0.62, 1.0],
        }
    }
}

/// One line in the import summary / message modal.
enum MessageLine {
    /// A centered line (sub-heading, notes, error/canceled body) with its color.
    Center { text: String, rgba: [f32; 4] },
    /// A two-column ledger row: `label` on the left, a status icon + `status`
    /// text on the right.
    Row {
        label: String,
        status: String,
        kind: SectionStatus,
    },
}

impl MessageLine {
    /// A centered, full-white line (error/canceled bodies, sub-heading).
    fn plain(text: String) -> Self {
        Self::Center {
            text,
            rgba: [1.0, 1.0, 1.0, 1.0],
        }
    }

    /// A centered, dimmed line (secondary notes / caveats).
    fn note(text: String) -> Self {
        Self::Center {
            text,
            rgba: [0.78, 0.78, 0.78, 1.0],
        }
    }

    /// A two-column ledger row.
    fn row(label: String, status: String, kind: SectionStatus) -> Self {
        Self::Row {
            label,
            status,
            kind,
        }
    }
}

/// A simple centered message modal: an import summary, "none found", or an
/// error. Dismissed with Start/Back.
struct ImportMessageState {
    title: Arc<str>,
    lines: Vec<MessageLine>,
}

pub struct State {
    pub selected: usize,
    prev_selected: usize,
    pub active_color_index: i32,
    bg: visual_style_bg::State,
    rows: Vec<Row>,
    default_profile_ids: [Option<String>; 2],
    dedicated_three_key_nav: bool,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    nav_key_last_scrolled_at: Option<Instant>,
    name_entry: Option<NameEntryState>,
    profile_menu: Option<ProfileMenuState>,
    delete_confirm: Option<DeleteConfirmState>,
    import_picker: Option<ImportPickerState>,
    import_job: Option<ImportJob>,
    import_browse_pending: bool,
    import_message: Option<ImportMessageState>,
    pending_effects: Vec<ThemeEffect>,
    menu_lr_chord: screen_input::MenuLrChordTracker,
    menu_lr_undo: i8,
}

pub fn init(view: ManageLocalProfilesView) -> State {
    let rows = build_rows(view.profiles);
    State {
        selected: 0,
        prev_selected: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
        rows,
        default_profile_ids: view.default_profile_ids,
        dedicated_three_key_nav: view.dedicated_three_key_nav,
        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
        name_entry: None,
        profile_menu: None,
        delete_confirm: None,
        import_picker: None,
        import_job: None,
        import_browse_pending: false,
        import_message: None,
        pending_effects: Vec::new(),
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        menu_lr_undo: 0,
    }
}

fn build_rows(profiles: Vec<LocalProfileView>) -> Vec<Row> {
    let mut out = Vec::with_capacity(profiles.len() + 2);
    out.push(Row {
        kind: RowKind::CreateNew,
    });
    out.push(Row {
        kind: RowKind::ImportItg,
    });
    for p in profiles {
        out.push(Row {
            kind: RowKind::Profile {
                id: p.id,
                display_name: p.display_name,
            },
        });
    }
    out.push(Row {
        kind: RowKind::Exit,
    });
    out
}

pub fn sync_runtime_view(state: &mut State, view: ManageLocalProfilesView) {
    state.rows = build_rows(view.profiles);
    state.default_profile_ids = view.default_profile_ids;
    state.dedicated_three_key_nav = view.dedicated_three_key_nav;
    if state.rows.is_empty() {
        state.selected = 0;
        state.prev_selected = 0;
        return;
    }
    state.selected = state.selected.min(state.rows.len() - 1);
    state.prev_selected = state.prev_selected.min(state.rows.len() - 1);
}

fn move_selected(state: &mut State, dir: NavDirection, wrap: NavWrap) {
    let total = state.rows.len();
    if total == 0 {
        state.selected = 0;
        return;
    }
    let last = total - 1;
    state.prev_selected = state.selected;
    state.selected = match dir {
        NavDirection::Up => {
            if state.selected == 0 {
                match wrap {
                    NavWrap::Wrap => last,
                    NavWrap::Clamp => 0,
                }
            } else {
                state.selected - 1
            }
        }
        NavDirection::Down => {
            if state.selected >= last {
                match wrap {
                    NavWrap::Wrap => 0,
                    NavWrap::Clamp => last,
                }
            } else {
                state.selected + 1
            }
        }
    };
}

fn on_nav_press(state: &mut State, dir: NavDirection) {
    let now = Instant::now();
    state.nav_key_held_direction = Some(dir);
    state.nav_key_held_since = Some(now);
    state.nav_key_last_scrolled_at = Some(now);
}

fn on_nav_release(state: &mut State, dir: NavDirection) {
    if state.nav_key_held_direction == Some(dir) {
        state.nav_key_held_direction = None;
        state.nav_key_held_since = None;
        state.nav_key_last_scrolled_at = None;
    }
}

fn reset_nav_hold(state: &mut State) {
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.nav_key_last_scrolled_at = None;
}

fn scroll_offset(selected: usize, total_rows: usize) -> usize {
    let anchor_row: usize = 4;
    let max_offset = total_rows.saturating_sub(VISIBLE_ROWS);
    if total_rows <= VISIBLE_ROWS {
        0
    } else {
        selected.saturating_sub(anchor_row).min(max_offset)
    }
}

fn update_hold_scroll(state: &mut State) {
    if state.name_entry.is_some() || state.profile_menu.is_some() || state.delete_confirm.is_some()
    {
        return;
    }
    let Some(dir) = state.nav_key_held_direction else {
        return;
    };
    let Some(held_since) = state.nav_key_held_since else {
        return;
    };
    let Some(last_at) = state.nav_key_last_scrolled_at else {
        return;
    };

    let now = Instant::now();
    if now.duration_since(held_since) < NAV_INITIAL_HOLD_DELAY {
        return;
    }
    if now.duration_since(last_at) < NAV_REPEAT_SCROLL_INTERVAL {
        return;
    }

    move_selected(state, dir, NavWrap::Clamp);
    state.nav_key_last_scrolled_at = Some(now);
}

fn update_name_entry_blink(state: &mut State, dt: f32) {
    let Some(entry) = state.name_entry.as_mut() else {
        return;
    };
    entry.blink_t = (entry.blink_t + dt) % 1.0;
}

pub fn update(state: &mut State, dt: f32) -> Option<ThemeEffect> {
    update_hold_scroll(state);
    update_name_entry_blink(state, dt);
    if state.selected != state.prev_selected {
        state.prev_selected = state.selected;
        state
            .pending_effects
            .push(crate::effects::sfx("assets/sounds/change.ogg"));
    }
    let effects = std::mem::take(&mut state.pending_effects);
    match effects.len() {
        0 => None,
        1 => effects.into_iter().next(),
        _ => Some(ThemeEffect::Batch(effects)),
    }
}

fn name_conflicts(state: &State, name: &str, skip_profile_id: Option<&str>) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return false;
    }
    for row in &state.rows {
        let RowKind::Profile { id, display_name } = &row.kind else {
            continue;
        };
        if skip_profile_id.is_some_and(|skip| skip == id) {
            continue;
        }
        if display_name.trim() == trimmed {
            return true;
        }
    }
    false
}

fn default_new_profile_name(state: &State) -> String {
    for i in 1..1000 {
        let candidate = format!("New{i:04}");
        if !name_conflicts(state, &candidate, None) {
            return candidate;
        }
    }
    "New0001".to_string()
}

fn validate_profile_name(state: &State, mode: &NameEntryMode, name: &str) -> Result<(), Arc<str>> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(tr("Profiles", "NameCannotBeBlank"));
    }

    let skip_id = match mode {
        NameEntryMode::Create => None,
        NameEntryMode::Rename { id } => Some(id.as_str()),
    };
    if name_conflicts(state, trimmed, skip_id) {
        return Err(tr("Profiles", "NameConflict"));
    }
    Ok(())
}

fn confirm_name_entry(state: &mut State) -> ThemeEffect {
    let Some(entry) = state.name_entry.as_ref() else {
        return ThemeEffect::None;
    };

    if let Err(error) = validate_profile_name(state, &entry.mode, &entry.value) {
        if let Some(entry) = state.name_entry.as_mut() {
            entry.error = Some(error);
        }
        return ThemeEffect::None;
    }

    let display_name = entry.value.trim().to_owned();
    let request = match &entry.mode {
        NameEntryMode::Create => {
            crate::SimplyLoveProfileRequest::CreateLocalProfile { display_name }
        }
        NameEntryMode::Rename { id } => crate::SimplyLoveProfileRequest::RenameLocalProfile {
            profile_id: id.clone(),
            display_name,
        },
    };
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(request))
}

fn cancel_name_entry(state: &mut State) {
    state.name_entry = None;
    reset_nav_hold(state);
}

fn begin_name_entry_create(state: &mut State) {
    reset_nav_hold(state);
    state.name_entry = Some(NameEntryState {
        mode: NameEntryMode::Create,
        value: default_new_profile_name(state),
        error: None,
        blink_t: 0.0,
    });
}

fn begin_name_entry_rename(state: &mut State, id: &str, display_name: &str) {
    reset_nav_hold(state);
    state.name_entry = Some(NameEntryState {
        mode: NameEntryMode::Rename { id: id.to_string() },
        value: display_name.to_string(),
        error: None,
        blink_t: 0.0,
    });
}

fn begin_profile_menu(state: &mut State, id: &str, display_name: &str) {
    reset_nav_hold(state);
    state.profile_menu = Some(ProfileMenuState {
        id: id.to_string(),
        display_name: display_name.to_string(),
        selected_action: 0,
    });
}

fn cancel_profile_menu(state: &mut State) {
    state.profile_menu = None;
    reset_nav_hold(state);
}

fn move_profile_menu_selected(state: &mut State, dir: NavDirection) {
    let Some(menu) = state.profile_menu.as_mut() else {
        return;
    };
    let len = PROFILE_MENU_ACTIONS.len();
    if len == 0 {
        menu.selected_action = 0;
        return;
    }
    menu.selected_action = match dir {
        NavDirection::Up => {
            if menu.selected_action == 0 {
                len - 1
            } else {
                menu.selected_action - 1
            }
        }
        NavDirection::Down => (menu.selected_action + 1) % len,
    };
}

fn confirm_profile_menu(state: &mut State) -> ThemeEffect {
    let Some(menu) = state.profile_menu.clone() else {
        return ThemeEffect::None;
    };
    let Some(action) = PROFILE_MENU_ACTIONS.get(menu.selected_action).copied() else {
        return ThemeEffect::None;
    };

    match action {
        ProfileMenuAction::SetP1 => ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
            crate::SimplyLoveProfileRequest::SetDefaultLocalProfile {
                side: PlayerSide::P1,
                profile_id: menu.id,
            },
        )),
        ProfileMenuAction::SetP2 => ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
            crate::SimplyLoveProfileRequest::SetDefaultLocalProfile {
                side: PlayerSide::P2,
                profile_id: menu.id,
            },
        )),
        ProfileMenuAction::LinkArrowCloud => {
            cancel_profile_menu(state);
            crate::effects::sfx_then(
                "assets/sounds/start.ogg",
                ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Online(
                    crate::SimplyLoveOnlineRequest::LinkArrowCloud {
                        profile_id: menu.id.clone(),
                        display_name: menu.display_name.clone(),
                    },
                )),
            )
        }
        ProfileMenuAction::LinkGrooveStats => {
            cancel_profile_menu(state);
            crate::effects::sfx_then(
                "assets/sounds/start.ogg",
                ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Online(
                    crate::SimplyLoveOnlineRequest::LinkGrooveStats {
                        profile_id: menu.id.clone(),
                        display_name: menu.display_name.clone(),
                    },
                )),
            )
        }
        ProfileMenuAction::Rename => {
            state.profile_menu = None;
            begin_name_entry_rename(state, &menu.id, &menu.display_name);
            crate::effects::sfx("assets/sounds/start.ogg")
        }
        ProfileMenuAction::Delete => {
            state.profile_menu = None;
            begin_delete_confirm(state, &menu.id, &menu.display_name);
            crate::effects::sfx("assets/sounds/start.ogg")
        }
    }
}

fn begin_delete_confirm(state: &mut State, id: &str, display_name: &str) {
    reset_nav_hold(state);
    state.profile_menu = None;
    state.delete_confirm = Some(DeleteConfirmState {
        id: id.to_string(),
        display_name: display_name.to_string(),
        error: None,
    });
}

#[inline(always)]
fn selected_after_delete(selected_before: usize, total_after: usize) -> usize {
    if total_after == 0 {
        return 0;
    }
    let mut selected = selected_before.min(total_after - 1);
    if selected + 1 == total_after && selected > 0 {
        selected -= 1;
    }
    selected
}

fn confirm_delete(state: &mut State) -> ThemeEffect {
    let Some(confirm) = state.delete_confirm.as_ref() else {
        return ThemeEffect::None;
    };
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
        crate::SimplyLoveProfileRequest::DeleteLocalProfile {
            profile_id: confirm.id.clone(),
        },
    ))
}

pub fn apply_local_profile_event(state: &mut State, event: crate::SimplyLoveLocalProfileEvent) {
    match event {
        crate::SimplyLoveLocalProfileEvent::Created { result, view } => match result {
            Ok(profile_id) => {
                state.name_entry = None;
                sync_runtime_view(state, view);
                select_profile_row(state, &profile_id);
                finish_local_profile_change(state);
            }
            Err(()) => set_name_entry_error(state, "CreateFailed"),
        },
        crate::SimplyLoveLocalProfileEvent::Renamed {
            profile_id,
            result,
            view,
        } => match result {
            Ok(()) => {
                state.name_entry = None;
                sync_runtime_view(state, view);
                select_profile_row(state, &profile_id);
                finish_local_profile_change(state);
            }
            Err(()) => set_name_entry_error(state, "RenameFailed"),
        },
        crate::SimplyLoveLocalProfileEvent::DefaultSet { view } => {
            sync_runtime_view(state, view);
            cancel_profile_menu(state);
            finish_local_profile_change(state);
        }
        crate::SimplyLoveLocalProfileEvent::Deleted { result, view } => match result {
            Ok(()) => {
                let selected_before = state.selected;
                state.delete_confirm = None;
                sync_runtime_view(state, view);
                let selected = selected_after_delete(selected_before, state.rows.len());
                state.selected = selected;
                state.prev_selected = selected;
                finish_local_profile_change(state);
            }
            Err(()) => {
                if let Some(confirm) = state.delete_confirm.as_mut() {
                    confirm.error = Some(tr("Profiles", "DeleteFailed"));
                }
            }
        },
    }
}

fn set_name_entry_error(state: &mut State, key: &str) {
    if let Some(entry) = state.name_entry.as_mut() {
        entry.error = Some(tr("Profiles", key));
    }
}

fn finish_local_profile_change(state: &mut State) {
    reset_nav_hold(state);
    state
        .pending_effects
        .push(crate::effects::sfx("assets/sounds/start.ogg"));
}

fn cancel_delete_confirm(state: &mut State) {
    state.delete_confirm = None;
    reset_nav_hold(state);
}

/* ----------------------------- ITGmania import ---------------------------- */

fn begin_import_picker(state: &mut State) -> ThemeEffect {
    reset_nav_hold(state);
    let candidates = Vec::new();
    // Always open the picker — even with nothing auto-detected — so the
    // "Browse for game directory…" row is available (no dead end).
    let info = None;
    state.import_picker = Some(ImportPickerState {
        candidates,
        selected: 0,
        info,
    });
    crate::effects::sfx_then(
        "assets/sounds/start.ogg",
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
            crate::SimplyLoveProfileRequest::DiscoverItgProfiles,
        )),
    )
}

fn cancel_import_picker(state: &mut State) {
    state.import_picker = None;
    reset_nav_hold(state);
}

fn move_import_picker_selected(state: &mut State, dir: NavDirection) {
    let Some(picker) = state.import_picker.as_mut() else {
        return;
    };
    // Selectable rows = candidates + the trailing "Browse…" row.
    let len = picker.candidates.len() + 1;
    picker.selected = match dir {
        NavDirection::Up => {
            if picker.selected == 0 {
                len - 1
            } else {
                picker.selected - 1
            }
        }
        NavDirection::Down => (picker.selected + 1) % len,
    };
}

fn confirm_import_picker(state: &mut State) -> ThemeEffect {
    // The "Browse…" row opens a native folder picker; keep the picker open.
    if state
        .import_picker
        .as_ref()
        .is_some_and(ImportPickerState::browse_selected)
    {
        reset_nav_hold(state);
        state.import_browse_pending = true;
        return crate::effects::sfx_then(
            "assets/sounds/start.ogg",
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
                crate::SimplyLoveProfileRequest::BrowseItgProfiles {
                    title: tr("Profiles", "ImportBrowsePrompt").to_string(),
                },
            )),
        );
    }

    // Refuse already-imported profiles: keep the picker open and explain why.
    if let Some(picker) = state.import_picker.as_mut() {
        let sel = picker.selected;
        if let Some(name) = picker.imported_as_at(sel).map(str::to_owned) {
            picker.info = Some(tr_fmt("Profiles", "ImportAlreadyInfo", &[("name", &name)]));
            return crate::effects::sfx("assets/sounds/boom.ogg");
        }
    }

    let Some(picker) = state.import_picker.take() else {
        return ThemeEffect::None;
    };
    let Some(candidate) = picker.candidates.get(picker.selected) else {
        return ThemeEffect::None;
    };
    let dir = candidate.dir.clone();
    reset_nav_hold(state);

    state.import_job = Some(ImportJob {
        progress: None,
        save_anchor: None,
        cancel_requested: false,
    });
    crate::effects::sfx_then(
        "assets/sounds/start.ogg",
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
            crate::SimplyLoveProfileRequest::StartItgProfileImport { dir },
        )),
    )
}

/// Merges newly-found candidates into the picker, de-duplicating by canonical
/// path and re-sorting by display name. Returns how many *new* profiles were
/// added.
fn merge_import_candidates(
    picker: &mut ImportPickerState,
    found: Vec<crate::SimplyLoveItgProfileCandidate>,
) -> usize {
    let mut seen: std::collections::HashSet<_> = picker
        .candidates
        .iter()
        .map(|candidate| candidate.dir.clone())
        .collect();
    let mut added = 0;
    for cand in found {
        if seen.insert(cand.dir.clone()) {
            picker.candidates.push(cand);
            added += 1;
        }
    }
    picker
        .candidates
        .sort_by(|a, b| unicode_case_insensitive_cmp(&a.display_name, &b.display_name));
    added
}

pub fn apply_import_events(state: &mut State, events: Vec<crate::SimplyLoveProfileImportEvent>) {
    for event in events {
        match event {
            crate::SimplyLoveProfileImportEvent::Candidates {
                candidates,
                browsed_dir,
            } => apply_import_candidates(state, candidates, browsed_dir.as_deref()),
            crate::SimplyLoveProfileImportEvent::BrowseCanceled => {
                state.import_browse_pending = false;
            }
            crate::SimplyLoveProfileImportEvent::Progress { done, total, label } => {
                let Some(job) = state.import_job.as_mut() else {
                    continue;
                };
                if job.save_anchor.is_none() {
                    job.save_anchor = Some((Instant::now(), done.saturating_sub(1)));
                }
                job.progress = Some((done, total, Arc::from(label)));
            }
            crate::SimplyLoveProfileImportEvent::Finished(outcome) => {
                finish_import(state, outcome);
            }
        }
    }
}

fn apply_import_candidates(
    state: &mut State,
    candidates: Vec<crate::SimplyLoveItgProfileCandidate>,
    browsed_dir: Option<&std::path::Path>,
) {
    if browsed_dir.is_some() {
        state.import_browse_pending = false;
    }
    let Some(picker) = state.import_picker.as_mut() else {
        return;
    };
    if candidates.is_empty() {
        picker.info = Some(match browsed_dir {
            Some(dir) => tr_fmt(
                "Profiles",
                "ImportBrowseNoneFoundBody",
                &[("dir", &dir.display().to_string())],
            ),
            None => tr("Profiles", "ImportNoneFoundBody"),
        });
        return;
    }
    let added = merge_import_candidates(picker, candidates);
    if browsed_dir.is_some() {
        picker.info = Some(tr_fmt(
            "Profiles",
            "ImportBrowseFoundBody",
            &[("count", &added.to_string())],
        ));
        picker.selected = 0;
        state
            .pending_effects
            .push(crate::effects::sfx("assets/sounds/change.ogg"));
    }
}

fn finish_import(state: &mut State, outcome: Result<crate::SimplyLoveItgImportSummary, String>) {
    state.import_job = None;
    match outcome {
        Ok(summary) => {
            if let Some(existing) = &summary.already_imported_as {
                state.import_message = Some(import_already_message(existing));
                state
                    .pending_effects
                    .push(crate::effects::sfx("assets/sounds/boom.ogg"));
            } else if summary.canceled {
                state.import_message = Some(import_canceled_message());
                state
                    .pending_effects
                    .push(crate::effects::sfx("assets/sounds/change.ogg"));
            } else {
                select_profile_row(state, &summary.profile_id);
                state.import_message = Some(import_summary_message(&summary));
                state
                    .pending_effects
                    .push(crate::effects::sfx("assets/sounds/start.ogg"));
            }
        }
        Err(error) => {
            state.import_message = Some(ImportMessageState {
                title: tr("Profiles", "ImportFailedTitle"),
                lines: vec![MessageLine::plain(error)],
            });
        }
    }
}

fn select_profile_row(state: &mut State, profile_id: &str) {
    if let Some(pos) = state
        .rows
        .iter()
        .position(|r| matches!(&r.kind, RowKind::Profile { id, .. } if id == profile_id))
    {
        state.selected = pos;
        state.prev_selected = pos;
    }
}

fn import_summary_message(summary: &crate::SimplyLoveItgImportSummary) -> ImportMessageState {
    let mut lines = Vec::new();
    lines.push(MessageLine::plain(
        tr_fmt(
            "Profiles",
            "ImportSummaryName",
            &[("name", &summary.display_name)],
        )
        .to_string(),
    ));

    // Scores: imported/total, amber when some were skipped.
    let scores_skipped =
        summary.charts_song_not_found + summary.charts_chart_not_found + summary.scores_unmapped;
    lines.push(if summary.scores_total == 0 {
        section_row(
            "ImportRowScores",
            tr("Profiles", "ImportStatNoneFound").to_string(),
            SectionStatus::Skipped,
        )
    } else {
        let status = ratio_status(&[
            ("done", &fmt_count(summary.scores_imported)),
            ("total", &fmt_count(summary.scores_total)),
        ]);
        let kind = if scores_skipped > 0 {
            SectionStatus::Partial
        } else {
            SectionStatus::Imported
        };
        section_row("ImportRowScores", status, kind)
    });

    // Favorites: matched/total, amber when some songs weren't found.
    lines.push(if summary.favorites_total == 0 {
        section_row(
            "ImportRowFavorites",
            tr("Profiles", "ImportStatNoneFound").to_string(),
            SectionStatus::Skipped,
        )
    } else {
        let status = ratio_status(&[
            ("done", &fmt_count(summary.favorites_imported)),
            ("total", &fmt_count(summary.favorites_total)),
        ]);
        let kind = if summary.favorites_imported < summary.favorites_total {
            SectionStatus::Partial
        } else {
            SectionStatus::Imported
        };
        section_row("ImportRowFavorites", status, kind)
    });

    // Player options (from Simply Love).
    lines.push(bool_row(
        "ImportRowPlayerOptions",
        summary.simply_love_options_imported,
        "ImportStatFromSimplyLove",
        "ImportStatDefaults",
    ));

    // GrooveStats / ArrowCloud credentials.
    lines.push(bool_row(
        "ImportRowGrooveStats",
        summary.groovestats_imported,
        "ImportStatLinked",
        "ImportStatNotSetUp",
    ));
    lines.push(bool_row(
        "ImportRowArrowCloud",
        summary.arrowcloud_imported,
        "ImportStatLinked",
        "ImportStatNotSetUp",
    ));

    // ITL event data.
    lines.push(if summary.itl_entries_imported > 0 {
        let status = tr_fmt(
            "Profiles",
            "ImportStatItlScores",
            &[("count", &fmt_count(summary.itl_entries_imported))],
        )
        .to_string();
        section_row("ImportRowItl", status, SectionStatus::Imported)
    } else {
        section_row(
            "ImportRowItl",
            tr("Profiles", "ImportStatNoneFound").to_string(),
            SectionStatus::Skipped,
        )
    });

    // Avatar.
    lines.push(bool_row(
        "ImportRowAvatar",
        summary.avatar_imported,
        "ImportStatImported",
        "ImportStatNoneFound",
    ));

    if summary.online_keys_imported() {
        lines.push(MessageLine::note(
            tr("Profiles", "ImportSummaryOnlineNudge").to_string(),
        ));
    }
    lines.push(MessageLine::note(
        tr("Profiles", "ImportSummaryExNote").to_string(),
    ));
    ImportMessageState {
        title: tr("Profiles", "ImportSummaryTitle"),
        lines,
    }
}

/// Builds a ledger row from a label key and an already-resolved status string.
fn section_row(label_key: &str, status: impl Into<String>, kind: SectionStatus) -> MessageLine {
    MessageLine::row(tr("Profiles", label_key).to_string(), status.into(), kind)
}

/// Builds a boolean ledger row: imported (green, `done_key` status) when `on`,
/// otherwise skipped (gray, `none_key` status).
fn bool_row(label_key: &str, on: bool, done_key: &str, none_key: &str) -> MessageLine {
    if on {
        section_row(
            label_key,
            tr("Profiles", done_key).to_string(),
            SectionStatus::Imported,
        )
    } else {
        section_row(
            label_key,
            tr("Profiles", none_key).to_string(),
            SectionStatus::Skipped,
        )
    }
}

/// Formats the shared `{done} / {total}` status string.
fn ratio_status(args: &[(&str, &str)]) -> String {
    tr_fmt("Profiles", "ImportStatRatio", args).to_string()
}

/// Formats a count with thousands separators (e.g. `1,234`) for readability in
/// the summary, since imported histories can be large.
fn fmt_count(n: usize) -> String {
    let digits = n.to_string();
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

/// The modal shown after a canceled import (the partial profile was deleted).
fn import_canceled_message() -> ImportMessageState {
    ImportMessageState {
        title: tr("Profiles", "ImportCanceledTitle"),
        lines: vec![MessageLine::plain(
            tr("Profiles", "ImportCanceledBody").to_string(),
        )],
    }
}

/// The modal shown when an import was refused because the ITGmania profile was
/// already imported (matched by derived GUID); `existing` is its DeadSync name.
fn import_already_message(existing: &str) -> ImportMessageState {
    ImportMessageState {
        title: tr("Profiles", "ImportAlreadyTitle"),
        lines: vec![MessageLine::plain(
            tr_fmt("Profiles", "ImportAlreadyBody", &[("name", existing)]).to_string(),
        )],
    }
}

fn dismiss_import_message(state: &mut State) -> ThemeEffect {
    state.import_message = None;
    reset_nav_hold(state);
    crate::effects::sfx("assets/sounds/start.ogg")
}

/// Requests a clean cancel of the running import. The worker polls this flag,
/// stops writing scores, deletes the partially-created profile, and reports a
/// canceled summary. Idempotent — repeated presses are harmless.
fn request_import_cancel(state: &mut State) -> ThemeEffect {
    let Some(job) = state.import_job.as_mut() else {
        return ThemeEffect::None;
    };
    if !job.cancel_requested {
        job.cancel_requested = true;
        log::warn!("ITGmania import cancel requested by user.");
        return crate::effects::sfx_then(
            "assets/sounds/change.ogg",
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
                crate::SimplyLoveProfileRequest::CancelItgProfileImport,
            )),
        );
    }
    ThemeEffect::None
}

fn handle_import_picker_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    if !ev.pressed {
        return ThemeEffect::None;
    }
    match ev.action {
        VirtualAction::p1_back | VirtualAction::p2_back => {
            cancel_import_picker(state);
            ThemeEffect::None
        }
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => {
            move_import_picker_selected(state, NavDirection::Up);
            crate::effects::sfx("assets/sounds/change.ogg")
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => {
            move_import_picker_selected(state, NavDirection::Down);
            crate::effects::sfx("assets/sounds/change.ogg")
        }
        VirtualAction::p1_start | VirtualAction::p2_start => confirm_import_picker(state),
        _ => ThemeEffect::None,
    }
}

#[inline(always)]
fn activate_selected_row(state: &mut State) -> ThemeEffect {
    let total = state.rows.len();
    if total == 0 {
        return ThemeEffect::None;
    }
    let sel = state.selected.min(total - 1);
    let start_row = state.rows[sel].kind.clone();
    match start_row {
        RowKind::CreateNew => {
            begin_name_entry_create(state);
            ThemeEffect::None
        }
        RowKind::ImportItg => begin_import_picker(state),
        RowKind::Exit => crate::effects::sfx_then(
            "assets/sounds/start.ogg",
            ThemeEffect::Navigate(Screen::Options),
        ),
        RowKind::Profile { id, display_name } => {
            begin_profile_menu(state, &id, &display_name);
            crate::effects::sfx("assets/sounds/start.ogg")
        }
    }
}

#[inline(always)]
fn undo_nav_move(state: &mut State, undo: i8) {
    match undo {
        1 => move_selected(state, NavDirection::Down, NavWrap::Wrap),
        -1 => move_selected(state, NavDirection::Up, NavWrap::Wrap),
        _ => {}
    }
}

#[inline(always)]
fn undo_profile_menu_move(state: &mut State, undo: i8) {
    match undo {
        1 => move_profile_menu_selected(state, NavDirection::Down),
        -1 => move_profile_menu_selected(state, NavDirection::Up),
        _ => {}
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    if state.import_job.is_some() {
        // While an import runs, the only interaction is Back to request a clean
        // cancel; everything else is swallowed so the modal stays put.
        if ev.pressed && matches!(ev.action, VirtualAction::p1_back | VirtualAction::p2_back) {
            return request_import_cancel(state);
        }
        return ThemeEffect::None;
    }
    if state.import_browse_pending {
        return ThemeEffect::None;
    }
    if state.import_message.is_some() {
        if ev.pressed
            && matches!(
                ev.action,
                VirtualAction::p1_start
                    | VirtualAction::p2_start
                    | VirtualAction::p1_back
                    | VirtualAction::p2_back
            )
        {
            return dismiss_import_message(state);
        }
        return ThemeEffect::None;
    }

    let three_key_action = screen_input::three_key_menu_action(
        &mut state.menu_lr_chord,
        ev,
        state.dedicated_three_key_nav,
    );
    if state.dedicated_three_key_nav {
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left
                if !ev.pressed =>
            {
                state.menu_lr_undo = 0;
                on_nav_release(state, NavDirection::Up);
                return ThemeEffect::None;
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right
                if !ev.pressed =>
            {
                state.menu_lr_undo = 0;
                on_nav_release(state, NavDirection::Down);
                return ThemeEffect::None;
            }
            _ => {}
        }
        if let Some((_, nav)) = three_key_action {
            if state.import_picker.is_some() {
                return match nav {
                    screen_input::ThreeKeyMenuAction::Prev => {
                        move_import_picker_selected(state, NavDirection::Up);
                        crate::effects::sfx("assets/sounds/change.ogg")
                    }
                    screen_input::ThreeKeyMenuAction::Next => {
                        move_import_picker_selected(state, NavDirection::Down);
                        crate::effects::sfx("assets/sounds/change.ogg")
                    }
                    screen_input::ThreeKeyMenuAction::Confirm => confirm_import_picker(state),
                    screen_input::ThreeKeyMenuAction::Cancel => {
                        cancel_import_picker(state);
                        ThemeEffect::None
                    }
                };
            }
            if state.name_entry.is_some() {
                return match nav {
                    screen_input::ThreeKeyMenuAction::Confirm => confirm_name_entry(state),
                    screen_input::ThreeKeyMenuAction::Cancel => {
                        cancel_name_entry(state);
                        ThemeEffect::None
                    }
                    _ => ThemeEffect::None,
                };
            }
            if state.delete_confirm.is_some() {
                return match nav {
                    screen_input::ThreeKeyMenuAction::Confirm => confirm_delete(state),
                    screen_input::ThreeKeyMenuAction::Cancel => {
                        cancel_delete_confirm(state);
                        ThemeEffect::None
                    }
                    _ => ThemeEffect::None,
                };
            }
            if state.profile_menu.is_some() {
                return match nav {
                    screen_input::ThreeKeyMenuAction::Prev => {
                        move_profile_menu_selected(state, NavDirection::Up);
                        on_nav_press(state, NavDirection::Up);
                        state.menu_lr_undo = 1;
                        crate::effects::sfx("assets/sounds/change.ogg")
                    }
                    screen_input::ThreeKeyMenuAction::Next => {
                        move_profile_menu_selected(state, NavDirection::Down);
                        on_nav_press(state, NavDirection::Down);
                        state.menu_lr_undo = -1;
                        crate::effects::sfx("assets/sounds/change.ogg")
                    }
                    screen_input::ThreeKeyMenuAction::Confirm => {
                        state.menu_lr_undo = 0;
                        confirm_profile_menu(state)
                    }
                    screen_input::ThreeKeyMenuAction::Cancel => {
                        undo_profile_menu_move(state, state.menu_lr_undo);
                        state.menu_lr_undo = 0;
                        cancel_profile_menu(state);
                        ThemeEffect::None
                    }
                };
            }
            return match nav {
                screen_input::ThreeKeyMenuAction::Prev => {
                    move_selected(state, NavDirection::Up, NavWrap::Wrap);
                    on_nav_press(state, NavDirection::Up);
                    state.menu_lr_undo = 1;
                    ThemeEffect::None
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    move_selected(state, NavDirection::Down, NavWrap::Wrap);
                    on_nav_press(state, NavDirection::Down);
                    state.menu_lr_undo = -1;
                    ThemeEffect::None
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    state.menu_lr_undo = 0;
                    activate_selected_row(state)
                }
                screen_input::ThreeKeyMenuAction::Cancel => {
                    undo_nav_move(state, state.menu_lr_undo);
                    state.menu_lr_undo = 0;
                    ThemeEffect::Navigate(Screen::Options)
                }
            };
        }
    }
    if state.import_picker.is_some() {
        return handle_import_picker_input(state, ev);
    }
    if state.name_entry.is_some() {
        return match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
                confirm_name_entry(state)
            }
            VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
                cancel_name_entry(state);
                ThemeEffect::None
            }
            _ => ThemeEffect::None,
        };
    }

    if state.delete_confirm.is_some() {
        return match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
                confirm_delete(state)
            }
            VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
                cancel_delete_confirm(state);
                ThemeEffect::None
            }
            _ => ThemeEffect::None,
        };
    }

    if state.profile_menu.is_some() {
        return match ev.action {
            VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
                cancel_profile_menu(state);
                ThemeEffect::None
            }
            VirtualAction::p1_up
            | VirtualAction::p1_menu_up
            | VirtualAction::p2_up
            | VirtualAction::p2_menu_up
                if ev.pressed =>
            {
                move_profile_menu_selected(state, NavDirection::Up);
                crate::effects::sfx("assets/sounds/change.ogg")
            }
            VirtualAction::p1_down
            | VirtualAction::p1_menu_down
            | VirtualAction::p2_down
            | VirtualAction::p2_menu_down
                if ev.pressed =>
            {
                move_profile_menu_selected(state, NavDirection::Down);
                crate::effects::sfx("assets/sounds/change.ogg")
            }
            VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
                confirm_profile_menu(state)
            }
            _ => ThemeEffect::None,
        };
    }

    match ev.action {
        VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
            return ThemeEffect::Navigate(Screen::Options);
        }
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => {
            if ev.pressed {
                move_selected(state, NavDirection::Up, NavWrap::Wrap);
                on_nav_press(state, NavDirection::Up);
            } else {
                on_nav_release(state, NavDirection::Up);
            }
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => {
            if ev.pressed {
                move_selected(state, NavDirection::Down, NavWrap::Wrap);
                on_nav_press(state, NavDirection::Down);
            } else {
                on_nav_release(state, NavDirection::Down);
            }
        }
        VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
            return activate_selected_row(state);
        }
        _ => {}
    }

    ThemeEffect::None
}

pub fn handle_raw_key_event(
    state: &mut State,
    key_event: Option<&RawKeyboardEvent>,
    text: Option<&str>,
) -> ThemeEffect {
    let Some(entry) = state.name_entry.as_mut() else {
        return ThemeEffect::None;
    };
    if let Some(key_event) = key_event {
        if !key_event.pressed {
            return ThemeEffect::None;
        }
        let code = key_event.code;
        match code {
            KeyCode::Backspace => {
                let _ = entry.value.pop();
                entry.error = None;
                return ThemeEffect::None;
            }
            KeyCode::Escape => return ThemeEffect::None,
            _ => {}
        }
    }

    let Some(text) = text else {
        return ThemeEffect::None;
    };

    let mut len = entry.value.chars().count();
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        if len >= NAME_MAX_LEN {
            break;
        }
        entry.value.push(ch);
        len += 1;
    }
    entry.error = None;
    ThemeEffect::None
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

fn scaled_block_origin_with_margins() -> (f32, f32, f32) {
    let total_w = LIST_W + SEP_W + DESC_W;
    let total_h = DESC_H;

    let sw = screen_width();
    let sh = screen_height();

    let content_top = BAR_H;
    let content_bottom = sh - BAR_H;
    let content_h = (content_bottom - content_top).max(0.0);

    let avail_w = (sw - LEFT_MARGIN_PX - RIGHT_MARGIN_PX).max(0.0);
    let avail_h = (content_h - FIRST_ROW_TOP_MARGIN_PX - BOTTOM_MARGIN_PX).max(0.0);

    let s_w = if total_w > 0.0 {
        avail_w / total_w
    } else {
        1.0
    };
    let s_h = if total_h > 0.0 {
        avail_h / total_h
    } else {
        1.0
    };
    let s = s_w.min(s_h).max(0.0);

    let ox = LEFT_MARGIN_PX + total_w.mul_add(-s, avail_w).max(0.0);
    let oy = content_top + FIRST_ROW_TOP_MARGIN_PX;
    (s, ox, oy)
}

fn indicator_text(id: &str, p1_id: Option<&str>, p2_id: Option<&str>) -> Option<Arc<str>> {
    let is_p1 = p1_id.is_some_and(|p1| p1 == id);
    let is_p2 = p2_id.is_some_and(|p2| p2 == id);
    match (is_p1, is_p2) {
        (true, true) => Some(tr("Profiles", "P1P2Assigned")),
        (true, false) => Some(tr("Profiles", "P1Assigned")),
        (false, true) => Some(tr("Profiles", "P2Assigned")),
        (false, false) => None,
    }
}

fn help_for_selected(state: &State, p1_id: Option<&str>, p2_id: Option<&str>) -> (String, String) {
    let Some(row) = state.rows.get(state.selected) else {
        return (String::new(), String::new());
    };

    match &row.kind {
        RowKind::CreateNew => {
            let title = tr("Profiles", "CreateProfileTitle");
            let b1 = tr("Profiles", "EnterProfileNamePrompt");
            let b2 = tr("Profiles", "PressStartConfirm");
            let b3 = tr("Profiles", "PressBackCancel");
            let bullets = make_bullets(&[&b1, &b2, &b3]);
            (title.to_string(), bullets)
        }
        RowKind::Exit => (tr("Profiles", "ReturnToOptions").to_string(), String::new()),
        RowKind::ImportItg => {
            let title = tr("Profiles", "ImportItgTitle");
            let b1 = tr("Profiles", "ImportItgHelp1");
            let b2 = tr("Profiles", "ImportItgHelp2");
            let bullets = make_bullets(&[&b1, &b2]);
            (title.to_string(), bullets)
        }
        RowKind::Profile { id, display_name } => {
            let title =
                tr_fmt("Profiles", "LocalProfileFormat", &[("name", display_name)]).to_string();

            let assigned = match indicator_text(id, p1_id, p2_id) {
                Some(tag) => tr_fmt("Profiles", "AssignedFormat", &[("tag", &tag)]).to_string(),
                None => tr("Profiles", "AssignedNone").to_string(),
            };
            let b1 = tr_fmt("Profiles", "IdFormat", &[("id", id)]).to_string();
            let b3 = tr("Profiles", "OpenActionsPrompt");
            let bullets = make_bullets(&[&b1, &assigned, &b3]);
            (title, bullets)
        }
    }
}

fn make_bullets(lines: &[&str]) -> String {
    let mut out = String::new();
    let mut first = true;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !first {
            out.push('\n');
        }
        out.push('•');
        out.push(' ');
        out.push_str(trimmed);
        first = false;
    }
    out
}

fn push_desc(ui: &mut Vec<Actor>, state: &State, s: f32, desc_x: f32, list_y: f32) {
    let (title, bullets) = help_for_selected(
        state,
        state.default_profile_ids[0].as_deref(),
        state.default_profile_ids[1].as_deref(),
    );

    let mut cursor_y = DESC_TITLE_TOP_PAD_PX.mul_add(s, list_y);
    let title_x = desc_x + DESC_TITLE_SIDE_PAD_PX * s;
    let max_title_w = (DESC_W - 2.0 * DESC_TITLE_SIDE_PAD_PX)
        .mul_add(s, 0.0)
        .max(0.0);
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(title_x, cursor_y):
        zoom(DESC_TITLE_ZOOM):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("miso"):
        maxwidth(max_title_w):
        settext(title):
        horizalign(left)
    ));

    cursor_y += DESC_BULLET_TOP_PAD_PX * s;
    if bullets.is_empty() {
        return;
    }

    let bullet_side_pad = DESC_BULLET_SIDE_PAD_PX * s;
    let bullet_x = DESC_BULLET_INDENT_PX.mul_add(s, desc_x + bullet_side_pad);
    let max_bullet_w = (DESC_W - 2.0 * DESC_BULLET_SIDE_PAD_PX)
        .mul_add(s, 0.0)
        .max(0.0);
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(bullet_x, cursor_y):
        zoom(DESC_BODY_ZOOM):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("miso"):
        maxwidth(max_bullet_w):
        settext(bullets):
        horizalign(left)
    ));
}

fn push_name_entry_overlay(ui: &mut Vec<Actor>, state: &State) {
    let Some(entry) = &state.name_entry else {
        return;
    };

    let w = screen_width();
    let h = screen_height();
    let accent = color::simply_love_rgba(state.active_color_index);
    let border = 4.0;
    let box_w = (w * 0.75).clamp(560.0, 1200.0);
    let top_h = 210.0;
    let bottom_h = 72.0;
    let box_h = top_h + bottom_h + 2.0 * border;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let top_cy = cy - box_h * 0.5 + border + top_h * 0.5;
    let bottom_cy = cy + box_h * 0.5 - border - bottom_h * 0.5;

    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(box_w, box_h):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1001)
    ));
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, top_cy):
        zoomto(box_w - 2.0 * border, top_h):
        diffuse(accent[0], accent[1], accent[2], 1.0):
        z(1002)
    ));
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, bottom_cy):
        zoomto(box_w - 2.0 * border, bottom_h):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1002)
    ));

    let name_prompt = tr("Profiles", "EnterProfileNamePrompt");
    ui.push(act!(text:
        align(0.5, 0.5):
        xy(cx, top_cy):
        font("miso"):
        zoom(1.0):
        settext(name_prompt):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1003):
        horizalign(center)
    ));

    let cursor = if entry.blink_t < 0.5 { "_" } else { " " };
    let mut value = entry.value.clone();
    if value.chars().count() < NAME_MAX_LEN {
        value.push_str(cursor);
    }
    ui.push(act!(text:
        align(0.5, 0.5):
        xy(cx, bottom_cy):
        font("miso"):
        zoom(1.55):
        maxwidth(box_w - 40.0):
        settext(value):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1003):
        horizalign(center)
    ));

    let Some(err) = &entry.error else {
        return;
    };
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy + box_h * 0.5 + 8.0):
        font("miso"):
        zoom(0.9):
        maxwidth(box_w - 40.0):
        settext(err.clone()):
        diffuse(1.0, 0.2, 0.2, 1.0):
        z(1003):
        horizalign(center)
    ));
}

fn push_delete_confirm_overlay(ui: &mut Vec<Actor>, state: &State) {
    let Some(confirm) = &state.delete_confirm else {
        return;
    };

    let w = screen_width();
    let h = screen_height();
    let box_w = 700.0_f32.min(w * 0.92);
    let box_h = 190.0_f32;
    let cx = w * 0.5;
    let cy = h * 0.5;

    push_overlay_backdrop(ui, w, h);
    push_overlay_box(ui, cx, cy, box_w, box_h);

    let prompt = tr_fmt(
        "Profiles",
        "DeleteConfirmFormat",
        &[("name", &confirm.display_name)],
    );
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy - box_h * 0.5 + 16.0):
        font("miso"):
        zoom(1.0):
        maxwidth(box_w - 40.0):
        settext(prompt):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));
    let cannot_be_undone = tr("Profiles", "CannotBeUndone");
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy - box_h * 0.5 + 58.0):
        font("miso"):
        zoom(0.9):
        settext(cannot_be_undone):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));
    let yes_no = tr("Profiles", "YesNoPrompt");
    ui.push(act!(text:
        align(0.5, 1.0):
        xy(cx, cy + box_h * 0.5 - 10.0):
        font("miso"):
        zoom(0.9):
        settext(yes_no):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));

    push_overlay_error(ui, confirm.error.as_ref(), cx, cy, box_w, box_h);
}

fn push_overlay_backdrop(ui: &mut Vec<Actor>, w: f32, h: f32) {
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(1000)
    ));
}

fn push_overlay_box(ui: &mut Vec<Actor>, cx: f32, cy: f32, w: f32, h: f32) {
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(w, h):
        diffuse(0.2, 0.2, 0.2, 1.0):
        z(1001)
    ));
}

/// Backdrop + box matching the app's standard popup look (e.g. the Score Import
/// dialog): a darker full-screen dim and a near-black panel, rather than the
/// lighter gray of the legacy in-screen modals. Used by the import flow popups.
fn push_popup_backdrop(ui: &mut Vec<Actor>, w: f32, h: f32) {
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.82):
        z(1000)
    ));
}

fn push_popup_box(ui: &mut Vec<Actor>, cx: f32, cy: f32, w: f32, h: f32) {
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(w, h):
        diffuse(0.02, 0.02, 0.02, 0.96):
        z(1001)
    ));
}

/// A popup title rendered in the machine Header font, matching the app's
/// standard popup heading. `top` is the popup box's top edge.
fn push_popup_title(
    ui: &mut Vec<Actor>,
    text: impl Into<String>,
    cx: f32,
    top: f32,
    max_w: f32,
    header_font: &'static str,
) {
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, top + 14.0):
        font(header_font):
        zoom(0.72):
        maxwidth(max_w):
        settext(text.into()):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));
}

/// A popup footer hint (e.g. "Press Start to continue.") in the standard dimmed
/// white style used by the app's popups.
fn push_popup_footer(ui: &mut Vec<Actor>, text: impl Into<String>, cx: f32, baseline_y: f32) {
    ui.push(act!(text:
        align(0.5, 1.0):
        xy(cx, baseline_y):
        font("miso"):
        zoom(0.78):
        settext(text.into()):
        diffuse(1.0, 1.0, 1.0, 0.7):
        z(1003):
        horizalign(center)
    ));
}

fn push_overlay_error(
    ui: &mut Vec<Actor>,
    err: Option<&Arc<str>>,
    cx: f32,
    cy: f32,
    box_w: f32,
    box_h: f32,
) {
    let Some(err) = err else {
        return;
    };
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy + box_h * 0.5 - 46.0):
        font("miso"):
        zoom(0.9):
        maxwidth(box_w - 40.0):
        settext(err.clone()):
        diffuse(1.0, 0.2, 0.2, 1.0):
        z(1002):
        horizalign(center)
    ));
}

const IMPORT_PICK_MAX_VISIBLE: usize = 8;

fn push_import_picker_overlay(
    ui: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    header_font: &'static str,
) {
    let Some(picker) = &state.import_picker else {
        return;
    };

    let w = screen_width();
    let h = screen_height();
    let header_h = 56.0_f32;
    let item_h = 34.0_f32;
    let footer_h = 44.0_f32;
    let total = picker.candidates.len();
    let visible = total.min(IMPORT_PICK_MAX_VISIBLE);
    // The list area always shows at least one row (an empty-state hint), plus a
    // trailing "Browse…" row, plus an optional info line.
    let list_rows = visible.max(1);
    let info_h = if picker.info.is_some() { 26.0 } else { 0.0 };
    let box_h = header_h + (list_rows as f32) * item_h + item_h + info_h + footer_h;

    // Lay the list out as a centered block: a name column whose width is the
    // widest label (profile names or the "Browse…" row) and, when any profile is
    // already imported, a tag column that begins just past it so every tag lines
    // up at the same x. The whole block is centered in the popup.
    const ROW_ZOOM: f32 = 0.95;
    const TAG_ZOOM: f32 = 0.82;
    const ROW_TAG_GAP: f32 = 24.0;
    const SIDE_PAD: f32 = 56.0;
    let tag_text = format!("✔ {}", tr("Profiles", "ImportTagImported"));
    let tag_w = measure_label_width(asset_manager, &tag_text, TAG_ZOOM);
    // Fixed name-column width: size for the longest name a profile can have
    // (NAME_MAX_LEN), so the container stays the same width regardless of which
    // profiles are in the list. "M" is the widest glyph, guaranteeing any name fits.
    let max_name_ref: String = std::iter::repeat('M').take(NAME_MAX_LEN).collect();
    let widest_label =
        measure_label_width(asset_manager, &max_name_ref, ROW_ZOOM).max(measure_label_width(
            asset_manager,
            &tr("Profiles", "ImportBrowseButton"),
            ROW_ZOOM,
        ));
    // Always reserve the tag column so the container is the same width whether or
    // not any profile is currently flagged as imported.
    let block_w = widest_label + ROW_TAG_GAP + tag_w;
    let title_w = measure_text_width(
        asset_manager,
        header_font,
        &tr("Profiles", "ImportPickTitle"),
        0.72,
    );
    let footer_w = measure_label_width(asset_manager, &tr("Profiles", "ImportPickPrompt"), 0.78);
    let box_w = (block_w + SIDE_PAD)
        .max(title_w + 40.0)
        .max(footer_w + 40.0)
        .clamp(360.0, w * 0.92);
    let cx = w * 0.5;
    let cy = h * 0.5;
    let top = cy - box_h * 0.5;

    push_popup_backdrop(ui, w, h);
    push_popup_box(ui, cx, cy, box_w, box_h);
    push_popup_title(
        ui,
        tr("Profiles", "ImportPickTitle").to_string(),
        cx,
        top,
        box_w - 40.0,
        header_font,
    );

    // Window the candidate list around the selection (clamped so the synthetic
    // "Browse…" row selection still shows the tail of the list).
    let list_sel = picker.selected.min(total.saturating_sub(1));
    let offset = if total <= IMPORT_PICK_MAX_VISIBLE {
        0
    } else {
        list_sel
            .saturating_sub(IMPORT_PICK_MAX_VISIBLE - 1)
            .min(total - IMPORT_PICK_MAX_VISIBLE)
    };
    let accent = color::simply_love_rgba(state.active_color_index);
    // Centered block geometry.
    let block_left = cx - block_w * 0.5;
    let tag_x = block_left + widest_label + ROW_TAG_GAP;
    let hl_left = cx - block_w * 0.5 - 12.0;
    let hl_w = block_w + 24.0;

    if total == 0 {
        // Empty-state hint, centered where the list would be.
        ui.push(act!(text:
            align(0.5, 0.5):
            xy(cx, top + header_h + item_h * 0.5):
            font("miso"):
            zoom(0.9):
            maxwidth(box_w - 40.0):
            settext(tr("Profiles", "ImportPickEmpty")):
            diffuse(0.7, 0.7, 0.7, 1.0):
            z(1003):
            horizalign(center)
        ));
    } else {
        for i in 0..visible {
            let idx = offset + i;
            let Some(cand) = picker.candidates.get(idx) else {
                break;
            };
            let row_y = (i as f32).mul_add(item_h, top + header_h);
            let selected = idx == picker.selected;
            let imported_as = picker.imported_as_at(idx);
            if selected {
                ui.push(act!(quad:
                    align(0.0, 0.0):
                    xy(hl_left, row_y):
                    zoomto(hl_w, item_h):
                    diffuse(0.17, 0.23, 0.28, 0.95):
                    z(1002)
                ));
            }
            // Already-imported rows are dimmed (disabled); otherwise white, or the
            // accent color when selected.
            let text_col = if imported_as.is_some() {
                [0.45, 0.45, 0.45, 1.0]
            } else if selected {
                [accent[0], accent[1], accent[2], 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };
            // Name in the left column (left-aligned at the block edge).
            ui.push(act!(text:
                align(0.0, 0.5):
                xy(block_left, row_y + item_h * 0.5):
                font("miso"):
                zoom(ROW_ZOOM):
                maxwidth(widest_label):
                settext(cand.display_name.clone()):
                diffuse(text_col[0], text_col[1], text_col[2], text_col[3]):
                z(1003):
                horizalign(left)
            ));
            // "✔ Imported" tag in the shared tag column (aligned past the widest
            // name / the Browse row, so every tag lines up).
            if imported_as.is_some() {
                ui.push(act!(text:
                    align(0.0, 0.5):
                    xy(tag_x, row_y + item_h * 0.5):
                    font("miso"):
                    zoom(TAG_ZOOM):
                    settext(format!("✔ {}", tr("Profiles", "ImportTagImported"))):
                    diffuse(0.55, 0.92, 0.55, 0.85):
                    z(1003):
                    horizalign(left)
                ));
            }
        }
    }

    // Trailing "Browse for game directory…" row.
    let browse_y = (list_rows as f32).mul_add(item_h, top + header_h);
    let browse_selected = picker.browse_selected();
    if browse_selected {
        ui.push(act!(quad:
            align(0.0, 0.0):
            xy(hl_left, browse_y):
            zoomto(hl_w, item_h):
            diffuse(0.17, 0.23, 0.28, 0.95):
            z(1002)
        ));
    }
    let browse_col = if browse_selected {
        [accent[0], accent[1], accent[2], 1.0]
    } else {
        [0.85, 0.85, 0.85, 1.0]
    };
    ui.push(act!(text:
        align(0.0, 0.5):
        xy(block_left, browse_y + item_h * 0.5):
        font("miso"):
        zoom(ROW_ZOOM):
        maxwidth(widest_label):
        settext(tr("Profiles", "ImportBrowseButton")):
        diffuse(browse_col[0], browse_col[1], browse_col[2], browse_col[3]):
        z(1003):
        horizalign(left)
    ));

    // Optional info notice (e.g. result of a browse, or "already imported"),
    // centered just below the browse row so it sits close to the list.
    if let Some(info) = &picker.info {
        let info_y = (list_rows as f32 + 1.0).mul_add(item_h, top + header_h) + 6.0;
        ui.push(act!(text:
            align(0.5, 0.0):
            xy(cx, info_y):
            font("miso"):
            zoom(0.8):
            maxwidth(box_w - 40.0):
            settext(info.to_string()):
            diffuse(accent[0], accent[1], accent[2], 1.0):
            z(1003):
            horizalign(center)
        ));
    }

    push_popup_footer(
        ui,
        tr("Profiles", "ImportPickPrompt").to_string(),
        cx,
        cy + box_h * 0.5 - 12.0,
    );
}

fn push_import_progress_overlay(ui: &mut Vec<Actor>, state: &State, header_font: &'static str) {
    let Some(job) = state.import_job.as_ref() else {
        return;
    };
    let canceling = job.cancel_requested;
    let w = screen_width();
    let h = screen_height();
    let box_w = 560.0_f32.min(w * 0.92);
    let box_h = 176.0_f32;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let top = cy - box_h * 0.5;

    push_popup_backdrop(ui, w, h);
    push_popup_box(ui, cx, cy, box_w, box_h);

    // Heading switches to a "canceling" acknowledgement once Back is pressed.
    let heading = if canceling {
        tr("Profiles", "ImportCanceling")
    } else {
        tr("Profiles", "ImportInProgress")
    };
    push_popup_title(ui, heading.to_string(), cx, top, box_w - 40.0, header_font);

    // Until scores start being written there's no determinate progress yet
    // (file read, resolver build and in-memory score matching are quick) — leave
    // just the heading. Once the disk-write phase begins, show the progress bar
    // and a "saving" sub-label.
    if let Some((done, total, label)) = &job.progress {
        let progress = if *total > 0 {
            (*done as f32 / *total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let accent = color::simply_love_rgba(state.active_color_index);
        let bar_w = (box_w - 64.0).max(0.0);
        ui.push(loading_bar::build(loading_bar::LoadingBarParams {
            align: [0.5, 0.5],
            offset: [cx, cy],
            width: bar_w,
            height: 22.0,
            progress,
            label: crate::screens::progress_count_text(*done, *total).into(),
            fill_rgba: [accent[0], accent[1], accent[2], 1.0],
            bg_rgba: [0.0, 0.0, 0.0, 1.0],
            border_rgba: [1.0, 1.0, 1.0, 1.0],
            text_rgba: [1.0, 1.0, 1.0, 1.0],
            text_zoom: 0.8,
            z: 1002,
        }));

        let sublabel = if label.is_empty() {
            saving_status_text(state, *done, *total)
        } else {
            label.to_string()
        };
        ui.push(act!(text:
            align(0.5, 0.5):
            xy(cx, cy + 34.0):
            font("miso"):
            zoom(0.78):
            maxwidth(box_w - 40.0):
        settext(sublabel):
            diffuse(0.8, 0.8, 0.8, 1.0):
            z(1003):
            horizalign(center)
        ));
    }

    // Cancel hint at the bottom (hidden once cancellation is already underway).
    if !canceling {
        push_popup_footer(
            ui,
            tr("Profiles", "ImportCancelHint").to_string(),
            cx,
            cy + box_h * 0.5 - 12.0,
        );
    }
}

/// Builds the "Saving scores…" sub-label, appending a writes-per-second rate and
/// an ETA once enough of the disk-write phase has elapsed to estimate them.
fn saving_status_text(state: &State, done: usize, total: usize) -> String {
    let base = tr("Profiles", "ImportSavingScores").to_string();
    let Some(job) = state.import_job.as_ref() else {
        return base;
    };
    let Some((started, done0)) = job.save_anchor else {
        return base;
    };
    let elapsed = started.elapsed().as_secs_f64();
    let processed = done.saturating_sub(done0);
    // Wait for a short warm-up so the first few file writes don't produce a wild
    // rate/ETA. Also guard against the final tick where nothing remains.
    if elapsed < 0.4 || processed == 0 || done >= total {
        return base;
    }
    let rate = processed as f64 / elapsed;
    if rate <= 0.0 {
        return base;
    }
    let remaining = total.saturating_sub(done);
    let eta_secs = (remaining as f64 / rate).ceil() as u64;
    tr_fmt(
        "Profiles",
        "ImportSavingDetail",
        &[
            ("rate", &format!("{}", rate.round() as u64)),
            ("eta", &format_eta(eta_secs)),
        ],
    )
    .to_string()
}

/// Formats a remaining-seconds estimate as a short `"45s"` or `"1m 05s"` string.
fn format_eta(secs: u64) -> String {
    if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

fn push_import_message_overlay(
    ui: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    header_font: &'static str,
) {
    let Some(message) = &state.import_message else {
        return;
    };

    let w = screen_width();
    let h = screen_height();
    let box_w = 680.0_f32.min(w * 0.92);
    let line_h = 28.0_f32;
    let header_h = 58.0_f32;
    let footer_h = 44.0_f32;
    let box_h = header_h + (message.lines.len().max(1) as f32) * line_h + footer_h;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let top = cy - box_h * 0.5;

    push_popup_backdrop(ui, w, h);
    push_popup_box(ui, cx, cy, box_w, box_h);
    push_popup_title(
        ui,
        message.title.to_string(),
        cx,
        top,
        box_w - 40.0,
        header_font,
    );

    // Two-column ledger, centered as a block: measure the widest label and the
    // widest status so the label/icon/status columns line up, then center the
    // whole table horizontally within the popup.
    const ROW_ZOOM: f32 = 0.9;
    const GAP_LABEL_ICON: f32 = 66.0;
    const GAP_ICON_STATUS: f32 = 12.0;
    let mut widest_label = 0.0_f32;
    let mut widest_status = 0.0_f32;
    let mut icon_w = 0.0_f32;
    for line in &message.lines {
        if let MessageLine::Row {
            label,
            status,
            kind,
        } = line
        {
            widest_label = widest_label.max(measure_label_width(asset_manager, label, ROW_ZOOM));
            widest_status = widest_status.max(measure_label_width(asset_manager, status, ROW_ZOOM));
            icon_w = icon_w.max(measure_label_width(asset_manager, kind.icon(), ROW_ZOOM));
        }
    }
    let block_w = widest_label + GAP_LABEL_ICON + icon_w + GAP_ICON_STATUS + widest_status;
    let block_left = cx - block_w * 0.5;
    let label_x = block_left;
    let icon_x = block_left + widest_label + GAP_LABEL_ICON;
    let status_x = icon_x + icon_w + GAP_ICON_STATUS;

    for (i, line) in message.lines.iter().enumerate() {
        let line_y = (i as f32).mul_add(line_h, top + header_h);
        match line {
            MessageLine::Center { text, rgba } => {
                ui.push(act!(text:
                    align(0.5, 0.0):
                    xy(cx, line_y):
                    font("miso"):
                            zoom(ROW_ZOOM):
                    maxwidth(box_w - 48.0):
                            settext(text.clone()):
                            diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
                    z(1002):
                    horizalign(center)
                ));
            }
            MessageLine::Row {
                label,
                status,
                kind,
            } => {
                let c = kind.rgba();
                // Label (left column, near-white for scannability).
                ui.push(act!(text:
                                align(0.0, 0.0):
                                xy(label_x, line_y):
                    font("miso"):
                                zoom(ROW_ZOOM):
                                maxwidth(widest_label + 1.0):
                                settext(label.clone()):
                                diffuse(0.96, 0.96, 0.96, 1.0):
                    z(1002):
                                horizalign(left)
                ));
                // Status icon (colored by outcome).
                ui.push(act!(text:
                    align(0.0, 0.0):
                    xy(icon_x, line_y):
                    font("miso"):
                    zoom(ROW_ZOOM):
                    settext(kind.icon().to_string()):
                    diffuse(c[0], c[1], c[2], c[3]):
                    z(1002):
                    horizalign(left)
                ));
                // Status text (same outcome color).
                ui.push(act!(text:
                    align(0.0, 0.0):
                    xy(status_x, line_y):
                    font("miso"):
                    zoom(ROW_ZOOM):
                    maxwidth(widest_status + 1.0):
                    settext(status.clone()):
                    diffuse(c[0], c[1], c[2], c[3]):
                    z(1002):
                    horizalign(left)
                ));
            }
        }
    }

    push_popup_footer(
        ui,
        tr("Profiles", "ImportMessageDismiss").to_string(),
        cx,
        cy + box_h * 0.5 - 12.0,
    );
}

/// Measures the rendered width (logical px) of `text` in `font_key` at `zoom`.
fn measure_text_width(asset_manager: &AssetManager, font_key: &str, text: &str, zoom: f32) -> f32 {
    let mut out = 0.0_f32;
    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font(font_key, |measure_font| {
            let w = deadlib_present::font::measure_line_width_logical(measure_font, text, all_fonts)
                as f32;
            if w.is_finite() && w > 0.0 {
                out = w * zoom;
            }
        });
    });
    out
}

/// Measures the rendered width (logical px) of a `miso` label at `zoom`.
fn measure_label_width(asset_manager: &AssetManager, text: &str, zoom: f32) -> f32 {
    measure_text_width(asset_manager, "miso", text, zoom)
}

fn push_list_chrome(
    ui: &mut Vec<Actor>,
    col_active_bg: [f32; 4],
    s: f32,
    list_x: f32,
    list_y: f32,
) {
    let list_w = LIST_W * s;
    let sep_w = SEP_W * s;
    let desc_h = DESC_H * s;

    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(list_x + list_w, list_y):
        zoomto(sep_w, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3])
    ));

    let desc_x = list_x + list_w + sep_w;
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(desc_x, list_y):
        zoomto(DESC_W * s, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3])
    ));
}

struct RowColors {
    active_bg: [f32; 4],
    inactive_bg: [f32; 4],
    brand_bg: [f32; 4],
    active_text: [f32; 4],
    white: [f32; 4],
    black: [f32; 4],
}

fn row_label(kind: &RowKind) -> Arc<str> {
    match kind {
        RowKind::CreateNew => tr("Profiles", "CreateProfileButton"),
        RowKind::ImportItg => tr("Profiles", "ImportItgButton"),
        RowKind::Exit => tr("Common", "Exit"),
        RowKind::Profile { display_name, .. } => Arc::from(display_name.as_str()),
    }
}

fn row_is_exit(kind: &RowKind) -> bool {
    matches!(kind, RowKind::Exit)
}

fn row_width(list_w: f32, sep_w: f32, is_active: bool, is_exit: bool) -> f32 {
    if is_exit {
        list_w - sep_w
    } else if is_active {
        list_w
    } else {
        list_w - sep_w
    }
}

fn row_bg_color(colors: &RowColors, is_active: bool, is_exit: bool) -> [f32; 4] {
    if is_active {
        if is_exit {
            colors.brand_bg
        } else {
            colors.active_bg
        }
    } else {
        colors.inactive_bg
    }
}

fn row_text_color(colors: &RowColors, is_active: bool, is_exit: bool) -> [f32; 4] {
    if is_exit {
        if is_active {
            colors.black
        } else {
            colors.white
        }
    } else if is_active {
        colors.active_text
    } else {
        colors.white
    }
}

fn push_row(
    ui: &mut Vec<Actor>,
    kind: &RowKind,
    is_active: bool,
    row_y: f32,
    list_x: f32,
    list_w: f32,
    sep_w: f32,
    s: f32,
    colors: &RowColors,
    p1_id: Option<&str>,
    p2_id: Option<&str>,
    assets: &'static crate::visual_styles::Assets,
) {
    let is_exit = row_is_exit(kind);
    let row_mid_y = (0.5 * ROW_H).mul_add(s, row_y);
    let row_w = row_width(list_w, sep_w, is_active, is_exit);
    let bg = row_bg_color(colors, is_active, is_exit);
    let text_col = row_text_color(colors, is_active, is_exit);

    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(list_x, row_y):
        zoomto(row_w, ROW_H * s):
        diffuse(bg[0], bg[1], bg[2], bg[3])
    ));

    if !is_exit {
        let heart_x = HEART_LEFT_PAD.mul_add(s, list_x);
        let heart_tint = if is_active {
            colors.active_text
        } else {
            colors.white
        };
        let texture = assets.select_color;
        let zoom = HEART_ZOOM * (566.0 / assets.select_color_size[1].max(1) as f32);
        ui.push(act!(sprite(texture):
            align(0.0, 0.5):
            xy(heart_x, row_mid_y):
            zoom(zoom):
            diffuse(heart_tint[0], heart_tint[1], heart_tint[2], heart_tint[3])
        ));
    }

    let text_x = TEXT_LEFT_PAD.mul_add(s, list_x);
    ui.push(act!(text:
        align(0.0, 0.5):
        xy(text_x, row_mid_y):
        zoom(ITEM_TEXT_ZOOM):
        diffuse(text_col[0], text_col[1], text_col[2], text_col[3]):
        font("miso"):
        settext(row_label(kind)):
        horizalign(left)
    ));

    if let RowKind::Profile { id, .. } = kind
        && let Some(tag) = indicator_text(id, p1_id, p2_id)
    {
        ui.push(act!(text:
            align(1.0, 0.5):
            xy(list_x + list_w - 12.0 * s, row_mid_y):
            zoom(0.75):
            diffuse(text_col[0], text_col[1], text_col[2], text_col[3]):
            font("miso"):
            settext(tag):
            horizalign(right)
        ));
    }
}

fn push_rows(
    ui: &mut Vec<Actor>,
    state: &State,
    s: f32,
    list_x: f32,
    list_y: f32,
    col_active_bg: [f32; 4],
    col_inactive_bg: [f32; 4],
    assets: &'static crate::visual_styles::Assets,
) {
    let list_w = LIST_W * s;
    let sep_w = SEP_W * s;
    let total_rows = state.rows.len();
    let offset = scroll_offset(state.selected, total_rows);
    let colors = RowColors {
        active_bg: col_active_bg,
        inactive_bg: col_inactive_bg,
        brand_bg: color::simply_love_rgba(state.active_color_index),
        active_text: color::simply_love_rgba(state.active_color_index + state.selected as i32),
        white: [1.0, 1.0, 1.0, 1.0],
        black: [0.0, 0.0, 0.0, 1.0],
    };

    let p1_id = state.default_profile_ids[0].as_deref();
    let p2_id = state.default_profile_ids[1].as_deref();

    for i_vis in 0..VISIBLE_ROWS {
        let row_idx = offset + i_vis;
        if row_idx >= total_rows {
            break;
        }
        let row_y = ((i_vis as f32) * (ROW_H + ROW_GAP)).mul_add(s, list_y);
        let is_active = row_idx == state.selected;
        push_row(
            ui,
            &state.rows[row_idx].kind,
            is_active,
            row_y,
            list_x,
            list_w,
            sep_w,
            s,
            &colors,
            p1_id,
            p2_id,
            assets,
        );
    }
}

fn selected_row_top_y(state: &State, s: f32, list_y: f32) -> f32 {
    if state.rows.is_empty() {
        return list_y;
    }
    let offset = scroll_offset(state.selected, state.rows.len());
    let vis = state.selected.saturating_sub(offset).min(VISIBLE_ROWS - 1);
    ((vis as f32) * (ROW_H + ROW_GAP)).mul_add(s, list_y)
}

fn push_profile_menu_overlay(ui: &mut Vec<Actor>, state: &State, s: f32, list_x: f32, list_y: f32) {
    let Some(menu) = &state.profile_menu else {
        return;
    };

    let row_top = selected_row_top_y(state, s, list_y);
    let menu_w = PROFILE_MENU_W * s;
    let header_h = PROFILE_MENU_HEADER_H * s;
    let item_h = PROFILE_MENU_ITEM_H * s;
    let border = PROFILE_MENU_BORDER * s;
    let body_h = item_h * PROFILE_MENU_ACTIONS.len() as f32;
    let menu_h = header_h + body_h + 2.0 * border;
    let mut menu_x = (LIST_W * 0.52).mul_add(s, list_x);
    let mut menu_y = row_top;

    menu_x = menu_x.clamp(10.0, (screen_width() - menu_w - 10.0).max(10.0));
    menu_y = menu_y.clamp(
        BAR_H + 4.0,
        (screen_height() - BAR_H - menu_h - 4.0).max(BAR_H + 4.0),
    );

    let inner_x = menu_x + border;
    let inner_y = menu_y + border;
    let inner_w = (menu_w - 2.0 * border).max(0.0);
    let accent = color::simply_love_rgba(state.active_color_index);

    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(menu_x, menu_y):
        zoomto(menu_w, menu_h):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1004)
    ));
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(inner_x, inner_y):
        zoomto(inner_w, header_h):
        diffuse(0.92, 0.92, 0.92, 1.0):
        z(1005)
    ));
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(inner_x, inner_y + header_h):
        zoomto(inner_w, body_h):
        diffuse(0.0, 0.06, 0.10, 0.96):
        z(1005)
    ));
    ui.push(act!(text:
        align(0.0, 0.5):
        xy(14.0_f32.mul_add(s, inner_x), inner_y + header_h * 0.5):
        font("miso"):
        zoom(1.20):
        settext(menu.display_name.clone()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1006):
        horizalign(left)
    ));

    for (i, action) in PROFILE_MENU_ACTIONS.iter().enumerate() {
        let row_y = (i as f32).mul_add(item_h, inner_y + header_h);
        let selected = i == menu.selected_action;
        if selected {
            ui.push(act!(quad:
                align(0.0, 0.0):
                xy(inner_x, row_y):
                zoomto(inner_w, item_h):
                diffuse(0.17, 0.23, 0.28, 0.95):
                z(1005)
            ));
        }
        let text_col = if selected {
            [accent[0], accent[1], accent[2], 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        ui.push(act!(text:
            align(0.0, 0.5):
            xy(14.0_f32.mul_add(s, inner_x), row_y + item_h * 0.5):
            font("miso"):
            zoom(1.0):
            settext(profile_menu_action_label(*action)):
            diffuse(text_col[0], text_col[1], text_col[2], text_col[3]):
            z(1006):
            horizalign(left)
        ));
    }
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(220);

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
            visual_policy,
        },
    );

    if alpha_multiplier <= 0.0 {
        return;
    }

    let ui_start = actors.len();
    let header_font = machine_font_key(visual_policy.machine_font, FontRole::Header);
    let title = tr("ScreenTitles", "ManageProfiles");
    actors.push(screen_bar::build(screen_bar::ScreenBarParams {
        visual_policy,
        title: &title,
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
        fg_color: [1.0, 1.0, 1.0, 1.0],
    }));

    let col_active_bg = color::rgba_hex("#333333");
    let base_inactive = color::rgba_hex("#071016");
    let col_inactive_bg: [f32; 4] = [base_inactive[0], base_inactive[1], base_inactive[2], 0.8];

    let (s, list_x, list_y) = scaled_block_origin_with_margins();
    push_list_chrome(actors, col_active_bg, s, list_x, list_y);
    push_rows(
        actors,
        state,
        s,
        list_x,
        list_y,
        col_active_bg,
        col_inactive_bg,
        visual_policy.assets,
    );

    let list_w = LIST_W * s;
    let sep_w = SEP_W * s;
    let desc_x = list_x + list_w + sep_w;
    push_desc(actors, state, s, desc_x, list_y);
    push_profile_menu_overlay(actors, state, s, list_x, list_y);
    push_name_entry_overlay(actors, state);
    push_delete_confirm_overlay(actors, state);
    push_import_picker_overlay(actors, state, asset_manager, header_font);
    push_import_progress_overlay(actors, state, header_font);
    push_import_message_overlay(actors, state, asset_manager, header_font);

    for actor in &mut actors[ui_start..] {
        actor.mul_alpha(alpha_multiplier);
    }
}

pub fn get_actors(
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(220);
    push_actors(
        &mut actors,
        state,
        asset_manager,
        alpha_multiplier,
        Default::default(),
    );
    actors
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::InputSource;

    #[test]
    fn format_eta_uses_seconds_then_minutes() {
        assert_eq!(format_eta(0), "0s");
        assert_eq!(format_eta(9), "9s");
        assert_eq!(format_eta(59), "59s");
        assert_eq!(format_eta(60), "1m 00s");
        assert_eq!(format_eta(75), "1m 15s");
        assert_eq!(format_eta(605), "10m 05s");
    }

    #[test]
    fn fmt_count_groups_thousands() {
        assert_eq!(fmt_count(0), "0");
        assert_eq!(fmt_count(7), "7");
        assert_eq!(fmt_count(42), "42");
        assert_eq!(fmt_count(999), "999");
        assert_eq!(fmt_count(1000), "1,000");
        assert_eq!(fmt_count(12345), "12,345");
        assert_eq!(fmt_count(1234567), "1,234,567");
    }

    fn input_event(action: VirtualAction, pressed: bool) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed,
            source: InputSource::Keyboard,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    fn press(state: &mut State, action: VirtualAction) -> ThemeEffect {
        handle_input(state, &input_event(action, true))
    }

    fn state_with_profile_row() -> State {
        let mut state = init(ManageLocalProfilesView::default());
        state.rows = vec![
            Row {
                kind: RowKind::CreateNew,
            },
            Row {
                kind: RowKind::Profile {
                    id: "test-profile".to_string(),
                    display_name: "Test Profile".to_string(),
                },
            },
            Row {
                kind: RowKind::Exit,
            },
        ];
        state.selected = 0;
        state.prev_selected = 0;
        state
    }

    fn profile_view(id: &str, display_name: &str) -> LocalProfileView {
        LocalProfileView {
            id: id.to_owned(),
            display_name: display_name.to_owned(),
        }
    }

    #[test]
    fn init_uses_shell_prepared_catalog_and_defaults() {
        let state = init(ManageLocalProfilesView {
            profiles: vec![profile_view("alice", "Alice")],
            default_profile_ids: [Some("alice".to_owned()), None],
            dedicated_three_key_nav: false,
        });

        assert!(matches!(state.rows[0].kind, RowKind::CreateNew));
        assert!(matches!(state.rows[1].kind, RowKind::ImportItg));
        assert!(matches!(
            &state.rows[2].kind,
            RowKind::Profile { id, display_name } if id == "alice" && display_name == "Alice"
        ));
        assert!(matches!(state.rows[3].kind, RowKind::Exit));
        assert_eq!(state.default_profile_ids, [Some("alice".to_owned()), None]);
    }

    #[test]
    fn create_request_waits_for_shell_result_then_selects_profile() {
        let mut state = init(ManageLocalProfilesView::default());
        state.name_entry = Some(NameEntryState {
            mode: NameEntryMode::Create,
            value: "Alice".to_owned(),
            error: None,
            blink_t: 0.0,
        });

        assert!(matches!(
            confirm_name_entry(&mut state),
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
                crate::SimplyLoveProfileRequest::CreateLocalProfile { display_name }
            )) if display_name == "Alice"
        ));
        assert!(state.name_entry.is_some());

        apply_local_profile_event(
            &mut state,
            crate::SimplyLoveLocalProfileEvent::Created {
                result: Ok("alice".to_owned()),
                view: ManageLocalProfilesView {
                    profiles: vec![profile_view("alice", "Alice")],
                    default_profile_ids: [Some("alice".to_owned()), None],
                    dedicated_three_key_nav: false,
                },
            },
        );

        assert!(state.name_entry.is_none());
        assert_eq!(state.selected, 2);
        assert!(matches!(
            update(&mut state, 0.0),
            Some(ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                deadsync_theme::AudioRequest::PlaySfx(path)
            ))) if path == "assets/sounds/start.ogg"
        ));
    }

    #[test]
    fn failed_delete_keeps_confirmation_and_sets_error() {
        let mut state = state_with_profile_row();
        begin_delete_confirm(&mut state, "test-profile", "Test Profile");

        apply_local_profile_event(
            &mut state,
            crate::SimplyLoveLocalProfileEvent::Deleted {
                result: Err(()),
                view: ManageLocalProfilesView::default(),
            },
        );

        assert!(
            state
                .delete_confirm
                .as_ref()
                .is_some_and(|confirm| confirm.error.is_some())
        );
        assert!(matches!(
            state.rows[1].kind,
            RowKind::Profile { ref id, .. } if id == "test-profile"
        ));
    }

    #[test]
    fn p2_can_navigate_profile_list() {
        let mut state = state_with_profile_row();

        press(&mut state, VirtualAction::p2_down);
        assert_eq!(state.selected, 1);

        press(&mut state, VirtualAction::p2_down);
        assert_eq!(state.selected, 2);

        let ThemeEffect::Batch(effects) = press(&mut state, VirtualAction::p2_start) else {
            panic!("expected audio and navigation batch");
        };
        assert!(matches!(
            &effects[0],
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                deadsync_theme::AudioRequest::PlaySfx(path)
            )) if path == "assets/sounds/start.ogg"
        ));
        assert!(matches!(effects[1], ThemeEffect::Navigate(Screen::Options)));
    }

    #[test]
    fn profile_list_up_and_down_emit_one_change_sfx() {
        let mut state = state_with_profile_row();

        for (action, selected) in [(VirtualAction::p1_down, 1), (VirtualAction::p1_up, 0)] {
            press(&mut state, action);
            assert_eq!(state.selected, selected);
            let Some(effect) = update(&mut state, 0.0) else {
                panic!("expected profile-list navigation SFX");
            };
            assert!(matches!(
                effect,
                ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                    deadsync_theme::AudioRequest::PlaySfx(path)
                )) if path == "assets/sounds/change.ogg"
            ));
            assert!(update(&mut state, 0.0).is_none());
        }
    }

    #[test]
    fn p2_can_navigate_profile_action_menu() {
        let mut state = state_with_profile_row();

        press(&mut state, VirtualAction::p2_down);
        press(&mut state, VirtualAction::p2_start);
        assert_eq!(
            state.profile_menu.as_ref().map(|m| m.selected_action),
            Some(0)
        );

        press(&mut state, VirtualAction::p2_down);
        assert_eq!(
            state.profile_menu.as_ref().map(|m| m.selected_action),
            Some(1)
        );

        press(&mut state, VirtualAction::p2_back);
        assert!(state.profile_menu.is_none());
    }

    #[test]
    fn browse_requests_shell_picker_and_keeps_modal_open() {
        let mut state = init(ManageLocalProfilesView::default());
        state.import_picker = Some(ImportPickerState {
            candidates: Vec::new(),
            selected: 0,
            info: None,
        });

        let effect = confirm_import_picker(&mut state);
        let ThemeEffect::Batch(effects) = effect else {
            panic!("expected batched picker effect");
        };
        assert_eq!(effects.len(), 2);
        assert!(matches!(
            &effects[0],
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                deadsync_theme::AudioRequest::PlaySfx(path)
            )) if path == "assets/sounds/start.ogg"
        ));
        assert!(matches!(
            &effects[1],
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
                crate::SimplyLoveProfileRequest::BrowseItgProfiles { title }
            )) if !title.is_empty()
        ));
        assert!(state.import_picker.is_some());
        assert!(state.import_browse_pending);
        assert!(matches!(
            press(&mut state, VirtualAction::p1_back),
            ThemeEffect::None
        ));
        assert!(state.import_picker.is_some());

        apply_import_events(
            &mut state,
            vec![crate::SimplyLoveProfileImportEvent::BrowseCanceled],
        );
        assert!(!state.import_browse_pending);
    }
}
