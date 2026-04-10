use std::collections::HashSet;

use crate::engine::input::InputEvent;
use crate::engine::present::actors::Actor;

use super::{Action, Item};

pub const FOCUS_TWEEN_SECONDS: f32 = 0.15;

// --- Types ---

/// Identifies a category that can be expanded/collapsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Sorts,
    Profile,
    Advanced,
    Styles,
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
        let mut expanded = HashSet::new();
        expanded.insert(Category::Sorts);
        Self { expanded }
    }
}

impl CategoryState {
    pub fn is_expanded(&self, category: Category) -> bool {
        self.expanded.contains(&category)
    }
}

/// Visible state for the category-based select music menu.
#[derive(Clone, Debug)]
pub struct VisibleState {
    pub selected_index: usize,
    pub prev_selected_index: usize,
    pub focus_anim_elapsed: f32,
    pub categories: CategoryState,
}

pub fn open() -> VisibleState {
    VisibleState {
        selected_index: 0,
        prev_selected_index: 0,
        focus_anim_elapsed: FOCUS_TWEEN_SECONDS,
        categories: CategoryState::default(),
    }
}

// --- Entry building ---

/// Build the flattened entry list based on current category expansion state.
pub fn build_entries(
    _items_standalone: &[Item],
    _items_sorts: &[Item],
    _items_profile: Option<&[Item]>,
    _items_advanced: &[Item],
    _items_styles: Option<&[Item]>,
    _categories: &CategoryState,
) -> Vec<Entry> {
    // TODO: implement collapsible category entry building
    Vec::new()
}

// --- Input handling ---

pub enum InputOutcome {
    None,
    Moved,
    ToggleCategory(Category),
    ActivateAction(Action),
    Close,
}

pub fn handle_input(
    _state: &mut VisibleState,
    _entries: &[Entry],
    _ev: &InputEvent,
) -> InputOutcome {
    // TODO: implement category menu input handling
    InputOutcome::None
}

// --- Rendering ---

pub struct RenderParams<'a> {
    pub entries: &'a [Entry],
    pub selected_index: usize,
    pub prev_selected_index: usize,
    pub focus_anim_elapsed: f32,
    pub selected_color: [f32; 4],
    pub categories: &'a CategoryState,
}

pub fn build_overlay(_p: RenderParams<'_>) -> Vec<Actor> {
    // TODO: implement category menu rendering
    Vec::new()
}
