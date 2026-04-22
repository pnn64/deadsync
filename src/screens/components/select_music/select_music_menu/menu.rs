use std::collections::HashSet;

use crate::act;
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};

use super::{Action, Item, scroll_anim_dir, set_text_clip_rect};

// --- Layout constants (matching Simply Love SortMenu) ---
const WIDTH: f32 = 210.0;
const HEIGHT: f32 = 204.0;
const ITEM_SPACING: f32 = 36.0;
const DIM_ALPHA: f32 = 0.8;
const HINT_Y_OFFSET: f32 = 120.0;
const HINT_TEXT: &str = "PRESS &SELECT; TO CANCEL";
const WHEEL_SLOTS: usize = 9;
const FONT_TOP: &str = "miso";
const FONT_BOTTOM: &str = "wendy";
const CATEGORY_INDENT: f32 = 8.0;

pub const FOCUS_TWEEN_SECONDS: f32 = 0.15;

const UNFOCUSED_ROW_BG: [f32; 4] = [0.2, 0.2, 0.2, 1.0];
const FOCUSED_ROW_BG: [f32; 4] = [0.35, 0.35, 0.35, 1.0];
const GO_BACK_COLOR_UNFOCUSED: [f32; 3] = [0.494, 0.055, 0.075]; // #7E0E13
const GO_BACK_COLOR_FOCUSED: [f32; 3] = [1.0, 0.6, 0.6]; // Simply Love GainFocus
const TEXT_UNFOCUSED_GRAY: f32 = 0.6; // #999999
const TEXT_FOCUSED_WHITE: f32 = 1.0;

// --- Types ---

/// Identifies a category that can be expanded/collapsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Sorts,
    Profile,
    Advanced,
    Styles,
    Playlists,
}

/// A visible entry in the select music menu wheel.
#[derive(Clone, Debug)]
pub enum Entry {
    /// A collapsible category header. Shows folder icon.
    CategoryHeader {
        category: Category,
        label: &'static str,
    },
    /// A regular action item nested under a category (indented).
    CategoryItem(Item),
    /// A standalone action item (not inside any category).
    StandaloneItem(Item),
}

/// Tracks which categories are currently expanded.
#[derive(Clone, Debug)]
pub struct CategoryState {
    pub expanded: HashSet<Category>,
}

impl Default for CategoryState {
    fn default() -> Self {
        Self {
            expanded: HashSet::new(),
        }
    }
}

impl CategoryState {
    pub fn toggle(&mut self, category: Category) {
        if self.expanded.contains(&category) {
            self.expanded.remove(&category);
        } else {
            self.expanded.insert(category);
        }
    }

    pub fn is_expanded(&self, category: Category) -> bool {
        self.expanded.contains(&category)
    }
}

/// Visible state for the category-based select music menu.
#[derive(Clone, Debug)]
pub struct VisibleState {
    pub selected_index: usize,
    pub prev_selected_index: usize,
    pub last_move_dir: isize,
    pub focus_anim_elapsed: f32,
    pub categories: CategoryState,
    /// Cached flattened entries, rebuilt on category toggle.
    pub cached_entries: Vec<Entry>,
}

impl VisibleState {
    /// Rebuild the cached entry list from the current item lists and expansion state.
    pub fn rebuild_entries(&mut self, lists: &CategoryItemLists) {
        self.cached_entries = build_entries(lists, &self.categories);
    }
}

pub fn open() -> VisibleState {
    VisibleState {
        selected_index: 0,
        prev_selected_index: 0,
        last_move_dir: 0,
        focus_anim_elapsed: FOCUS_TWEEN_SECONDS,
        categories: CategoryState::default(),
        cached_entries: Vec::new(),
    }
}

// --- Item lists ---

/// Named struct for the category item lists, replacing the 5-element tuple.
pub struct CategoryItemLists {
    pub standalone: Vec<Item>,
    pub sorts: Vec<Item>,
    pub profile: Option<Vec<Item>>,
    pub advanced: Vec<Item>,
    pub styles: Option<Vec<Item>>,
    pub playlists: Option<Vec<Item>>,
}

// --- Entry building ---

/// Build the flattened entry list based on current category expansion state.
pub fn build_entries(lists: &CategoryItemLists, categories: &CategoryState) -> Vec<Entry> {
    build_entries_from_slices(
        &lists.standalone,
        &lists.sorts,
        lists.profile.as_deref(),
        &lists.advanced,
        lists.styles.as_deref(),
        lists.playlists.as_deref(),
        categories,
    )
}

fn build_entries_from_slices(
    items_standalone: &[Item],
    items_sorts: &[Item],
    items_profile: Option<&[Item]>,
    items_advanced: &[Item],
    items_styles: Option<&[Item]>,
    items_playlists: Option<&[Item]>,
    categories: &CategoryState,
) -> Vec<Entry> {
    // If a category is expanded, show ONLY that category header + its items
    // (wrapping/repeating in the wheel). This matches Simply Love's behavior.
    if categories.is_expanded(Category::Sorts) {
        let mut entries = vec![Entry::CategoryHeader {
            category: Category::Sorts,
            label: "Sorts...",
        }];
        for item in items_sorts {
            entries.push(Entry::CategoryItem(item.clone()));
        }
        return entries;
    }
    if categories.is_expanded(Category::Profile) {
        if let Some(profile_items) = items_profile {
            let mut entries = vec![Entry::CategoryHeader {
                category: Category::Profile,
                label: "Profile...",
            }];
            for item in profile_items {
                entries.push(Entry::CategoryItem(item.clone()));
            }
            return entries;
        }
    }
    if categories.is_expanded(Category::Advanced) {
        let mut entries = vec![Entry::CategoryHeader {
            category: Category::Advanced,
            label: "Advanced...",
        }];
        for item in items_advanced {
            entries.push(Entry::CategoryItem(item.clone()));
        }
        return entries;
    }
    if categories.is_expanded(Category::Styles) {
        if let Some(style_items) = items_styles {
            let mut entries = vec![Entry::CategoryHeader {
                category: Category::Styles,
                label: "Styles...",
            }];
            for item in style_items {
                entries.push(Entry::CategoryItem(item.clone()));
            }
            return entries;
        }
    }
    if categories.is_expanded(Category::Playlists) {
        if let Some(playlist_items) = items_playlists {
            let mut entries = vec![Entry::CategoryHeader {
                category: Category::Playlists,
                label: "Playlists...",
            }];
            for item in playlist_items {
                entries.push(Entry::CategoryItem(item.clone()));
            }
            return entries;
        }
    }

    // No category expanded — show all standalone items + collapsed category headers
    let mut entries = Vec::new();

    for item in items_standalone {
        entries.push(Entry::StandaloneItem(item.clone()));
    }

    entries.push(Entry::CategoryHeader {
        category: Category::Sorts,
        label: "Sorts...",
    });

    if items_profile.is_some() {
        entries.push(Entry::CategoryHeader {
            category: Category::Profile,
            label: "Profile...",
        });
    }

    entries.push(Entry::CategoryHeader {
        category: Category::Advanced,
        label: "Advanced Options",
    });

    if items_styles.is_some() {
        entries.push(Entry::CategoryHeader {
            category: Category::Styles,
            label: "Styles...",
        });
    }
    if items_playlists.is_some() {
        entries.push(Entry::CategoryHeader {
            category: Category::Playlists,
            label: "Playlists...",
        });
    }

    entries
}

// --- Input handling ---

pub enum InputOutcome {
    None,
    Moved,
    ToggleCategory(Category),
    ActivateAction(Action),
    Close,
}

pub fn handle_input(state: &mut VisibleState, entries: &[Entry], ev: &InputEvent) -> InputOutcome {
    if !ev.pressed {
        return InputOutcome::None;
    }

    match ev.action {
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => {
            if move_selection(state, entries.len(), -1) {
                InputOutcome::Moved
            } else {
                InputOutcome::None
            }
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
            if move_selection(state, entries.len(), 1) {
                InputOutcome::Moved
            } else {
                InputOutcome::None
            }
        }
        VirtualAction::p1_start | VirtualAction::p2_start => activate(state, entries),
        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => InputOutcome::Close,
        _ => InputOutcome::None,
    }
}

pub fn move_selection(state: &mut VisibleState, len: usize, delta: isize) -> bool {
    if len <= 1 {
        return false;
    }
    let old = state.selected_index.min(len - 1);
    let next = ((old as isize + delta).rem_euclid(len as isize)) as usize;
    if next == old {
        return false;
    }
    state.prev_selected_index = old;
    state.last_move_dir = delta.signum();
    state.selected_index = next;
    state.focus_anim_elapsed = 0.0;
    true
}

fn activate(state: &mut VisibleState, entries: &[Entry]) -> InputOutcome {
    if entries.is_empty() {
        return InputOutcome::Close;
    }
    let idx = state.selected_index.min(entries.len() - 1);
    match &entries[idx] {
        Entry::CategoryHeader { category, .. } => {
            state.categories.toggle(*category);
            InputOutcome::ToggleCategory(*category)
        }
        Entry::CategoryItem(item) | Entry::StandaloneItem(item) => {
            InputOutcome::ActivateAction(item.action.clone())
        }
    }
}

// --- Rendering ---

pub struct RenderParams<'a> {
    pub entries: &'a [Entry],
    pub selected_index: usize,
    pub prev_selected_index: usize,
    pub last_move_dir: isize,
    pub focus_anim_elapsed: f32,
    pub selected_color: [f32; 4],
}

pub fn build_overlay(p: RenderParams<'_>) -> Vec<Actor> {
    let mut actors = Vec::new();
    let cx = screen_center_x();
    let cy = screen_center_y();
    let clip_rect = [cx - WIDTH * 0.5, cy - HEIGHT * 0.5, WIDTH, HEIGHT];
    let selected_index = p.selected_index.min(p.entries.len().saturating_sub(1));

    // Background dim
    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, DIM_ALPHA):
        z(1450)
    ));

    // White border around the menu area (2px)
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(WIDTH + 4.0, HEIGHT + 4.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1451)
    ));
    // Black fill inside the border
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(WIDTH, HEIGHT):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1452)
    ));

    // Hint text below the menu
    actors.push(act!(text:
        font(FONT_BOTTOM):
        settext(HINT_TEXT):
        align(0.5, 0.5):
        xy(cx, cy + HINT_Y_OFFSET):
        zoom(0.29):
        diffuse(1.0, 1.0, 1.0, 0.7):
        z(1451):
        horizalign(center)
    ));

    if !p.entries.is_empty() {
        let focus_t = (p.focus_anim_elapsed / FOCUS_TWEEN_SECONDS.max(1e-6)).clamp(0.0, 1.0);
        let scroll_dir_val = scroll_anim_dir(
            p.entries.len(),
            p.prev_selected_index.min(p.entries.len() - 1),
            selected_index,
            p.last_move_dir,
        ) as f32;
        let scroll_shift = scroll_dir_val * (1.0 - focus_t);

        let half_slots = (WHEEL_SLOTS / 2) as f32;
        for slot in 0..WHEEL_SLOTS {
            let slot_pos = slot as f32 - half_slots + scroll_shift;
            let entry_offset = slot as isize - (WHEEL_SLOTS / 2) as isize;
            let entry_idx = ((selected_index as isize + entry_offset)
                .rem_euclid(p.entries.len() as isize)) as usize;
            let entry = &p.entries[entry_idx];

            render_row(&mut actors, entry, slot_pos, cx, cy, &clip_rect);
        }
    }

    actors
}

fn render_row(
    actors: &mut Vec<Actor>,
    entry: &Entry,
    slot_pos: f32,
    cx: f32,
    cy: f32,
    clip_rect: &[f32; 4],
) {
    let focus_lerp = (1.0 - slot_pos.abs()).clamp(0.0, 1.0);
    let row_alpha = 1.0_f32;
    let half_height = HEIGHT * 0.5;
    let y = slot_pos.mul_add(ITEM_SPACING, cy);
    let row_half = (ITEM_SPACING - 2.0) * 0.5;
    let row_top = y - row_half;
    let row_bot = y + row_half;
    let box_top = cy - half_height;
    let box_bot = cy + half_height;
    // Skip rows entirely outside the box
    if row_top >= box_bot || row_bot <= box_top {
        return;
    }
    // Compute visible portion for partially clipped rows
    let vis_top = row_top.max(box_top);
    let vis_bot = row_bot.min(box_bot);
    let vis_h = vis_bot - vis_top;
    let vis_cy = (vis_top + vis_bot) * 0.5;
    let left_x = cx - WIDTH * 0.5 + 12.0;

    // Row background: category headers get gray bg, others get black
    let is_category_header = matches!(entry, Entry::CategoryHeader { .. });
    if is_category_header {
        let bg = lerp_color(UNFOCUSED_ROW_BG, FOCUSED_ROW_BG, focus_lerp);
        actors.push(act!(quad:
            align(0.5, 0.5): xy(cx, vis_cy):
            zoomto(WIDTH, vis_h):
            diffuse(bg[0], bg[1], bg[2], row_alpha):
            z(1453)
        ));
    } else {
        actors.push(act!(quad:
            align(0.5, 0.5): xy(cx, vis_cy):
            zoomto(WIDTH, vis_h):
            diffuse(0.0, 0.0, 0.0, row_alpha):
            z(1453)
        ));
    }

    // Render text/icons at the row's original center position (may extend beyond box)
    let icon_size = 128.0 * 0.20; // folder icon rendered size
    let icon_top = y - icon_size * 0.5;
    let icon_bot = y + icon_size * 0.5;
    let box_top = cy - half_height;
    let box_bot = cy + half_height;
    let crop_top = if icon_top < box_top {
        (box_top - icon_top) / icon_size
    } else {
        0.0
    };
    let crop_bottom = if icon_bot > box_bot {
        (icon_bot - box_bot) / icon_size
    } else {
        0.0
    };

    match entry {
        Entry::CategoryHeader { label, .. } => {
            let tint = lerp_scalar(TEXT_UNFOCUSED_GRAY, TEXT_FOCUSED_WHITE, focus_lerp);
            // Folder icon — clipped to box boundary
            actors.push(act!(sprite("folder-solid.png"):
                align(0.0, 0.5):
                xy(left_x - 6.0, y):
                zoom(0.20):
                croptop(crop_top):
                cropbottom(crop_bottom):
                diffuse(tint, tint, tint, row_alpha):
                z(1454)
            ));
            // Category label
            let mut label_actor = act!(text:
                font(FONT_BOTTOM):
                settext(*label):
                align(0.0, 0.5):
                xy(left_x + 27.0, y):
                zoom(0.4):
                maxwidth(WIDTH - 50.0):
                diffuse(tint, tint, tint, row_alpha):
                z(1454):
                horizalign(left)
            );
            set_text_clip_rect(&mut label_actor, *clip_rect);
            actors.push(label_actor);
        }
        Entry::CategoryItem(item) => {
            let indent_x = left_x + CATEGORY_INDENT;
            let tint = item_tint(item, focus_lerp);
            render_item_text(actors, item, indent_x, y, row_alpha, &tint, clip_rect, true);
        }
        Entry::StandaloneItem(item) => {
            let tint = item_tint(item, focus_lerp);
            render_item_text(actors, item, left_x, y, row_alpha, &tint, clip_rect, false);
        }
    }
}

fn render_item_text(
    actors: &mut Vec<Actor>,
    item: &Item,
    x: f32,
    y: f32,
    row_alpha: f32,
    tint: &[f32; 3],
    clip_rect: &[f32; 4],
    indented: bool,
) {
    let max_w = if indented {
        WIDTH - CATEGORY_INDENT - 28.0
    } else {
        WIDTH - 28.0
    };
    if !item.top_label.is_empty() {
        let mut top = act!(text:
            font(FONT_TOP):
            settext(item.top_label.to_string()):
            align(0.0, 1.0):
            xy(x, y - 5.0):
            zoom(0.58):
            maxwidth(max_w):
            diffuse(tint[0], tint[1], tint[2], row_alpha * 0.85):
            z(1454):
            horizalign(left)
        );
        set_text_clip_rect(&mut top, *clip_rect);
        actors.push(top);
    }
    let mut bottom = act!(text:
        font(FONT_BOTTOM):
        settext(item.bottom_label.to_string()):
        align(0.0, 0.5):
        xy(x, y + 4.0):
        zoom(0.36):
        maxwidth(max_w):
        diffuse(tint[0], tint[1], tint[2], row_alpha):
        z(1454):
        horizalign(left)
    );
    set_text_clip_rect(&mut bottom, *clip_rect);
    actors.push(bottom);
}

#[inline(always)]
fn item_tint(item: &Item, focus_lerp: f32) -> [f32; 3] {
    if matches!(&item.action, Action::BackToMain) {
        [
            lerp_scalar(
                GO_BACK_COLOR_UNFOCUSED[0],
                GO_BACK_COLOR_FOCUSED[0],
                focus_lerp,
            ),
            lerp_scalar(
                GO_BACK_COLOR_UNFOCUSED[1],
                GO_BACK_COLOR_FOCUSED[1],
                focus_lerp,
            ),
            lerp_scalar(
                GO_BACK_COLOR_UNFOCUSED[2],
                GO_BACK_COLOR_FOCUSED[2],
                focus_lerp,
            ),
        ]
    } else {
        let v = lerp_scalar(TEXT_UNFOCUSED_GRAY, TEXT_FOCUSED_WHITE, focus_lerp);
        [v, v, v]
    }
}

#[inline(always)]
fn lerp_scalar(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline(always)]
fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        lerp_scalar(a[0], b[0], t),
        lerp_scalar(a[1], b[1], t),
        lerp_scalar(a[2], b[2], t),
        lerp_scalar(a[3], b[3], t),
    ]
}
