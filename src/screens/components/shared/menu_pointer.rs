//! Reusable hover/click state for vertical list menus.
//!
//! Screens that present a simple list (main menu, select mode, select
//! color, etc.) all need the same three behaviours when mouse input is
//! enabled:
//!
//! * pointer Move sets the highlighted row to whichever item the cursor
//!   is over (silently — no change sfx on every mouse pixel),
//! * pointer Leave clears the highlight,
//! * pointer Down(Left) returns the index of the clicked row so the
//!   screen can launch its confirm action.
//!
//! `MenuPointer` is the bookkeeping shared by those screens. The screen
//! owns its `selected_index`, builds its `HitRect`s each frame via
//! `menu_list::item_rects`, and calls into `MenuPointer` from
//! `handle_pointer`. It also queries `hovered()` to decide whether the
//! mouse cursor should switch to its hover variant.

use crate::engine::space::LogicalPos;
use crate::screens::components::shared::hitbox::{HitRect, hit_test};

#[derive(Clone, Copy, Debug, Default)]
pub struct MenuPointer {
    /// Index of the row currently under the pointer, or `None` if the
    /// cursor is outside every row (or has left the window entirely).
    hovered: Option<usize>,
}

impl MenuPointer {
    pub const fn new() -> Self {
        Self { hovered: None }
    }

    /// True if the pointer is currently over an interactive row. Used by
    /// the cursor-swap logic in `app::dispatch_pointer_event`.
    #[inline]
    pub const fn hovers_interactive(&self) -> bool {
        self.hovered.is_some()
    }

    #[inline]
    pub const fn hovered(&self) -> Option<usize> {
        self.hovered
    }

    /// Update the hovered row from a pointer Move event. Returns the new
    /// index if the cursor is over one (so the caller can set its own
    /// `selected_index`), or `None` if the cursor left the menu.
    pub fn on_move(&mut self, pos: Option<LogicalPos>, rects: &[HitRect]) -> Option<usize> {
        let new_hover = pos.and_then(|p| hit_test(rects, p)).map(|id| id as usize);
        self.hovered = new_hover;
        new_hover
    }

    /// Clear the hover state, called from `PointerKind::Leave`.
    #[inline]
    pub fn on_leave(&mut self) {
        self.hovered = None;
    }

    /// Resolve a left-click into a row index. Returns the clicked row's
    /// index, or `None` if the click missed every row.
    pub fn on_click(&self, pos: Option<LogicalPos>, rects: &[HitRect]) -> Option<usize> {
        pos.and_then(|p| hit_test(rects, p)).map(|id| id as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::MenuPointer;
    use crate::engine::space::LogicalPos;
    use crate::screens::components::shared::hitbox::HitRect;

    fn rects() -> Vec<HitRect> {
        vec![
            HitRect::from_center(100.0, 100.0, 80.0, 20.0, 0),
            HitRect::from_center(100.0, 130.0, 80.0, 20.0, 1),
            HitRect::from_center(100.0, 160.0, 80.0, 20.0, 2),
        ]
    }

    #[test]
    fn on_move_inside_a_row_returns_its_index() {
        let mut mp = MenuPointer::new();
        let r = rects();
        assert_eq!(mp.on_move(Some(LogicalPos::new(100.0, 130.0)), &r), Some(1));
        assert!(mp.hovers_interactive());
        assert_eq!(mp.hovered(), Some(1));
    }

    #[test]
    fn on_move_outside_returns_none_and_clears_hover() {
        let mut mp = MenuPointer::new();
        let r = rects();
        // Hover row 0, then move off-row.
        assert_eq!(mp.on_move(Some(LogicalPos::new(100.0, 100.0)), &r), Some(0));
        assert_eq!(mp.on_move(Some(LogicalPos::new(0.0, 0.0)), &r), None);
        assert!(!mp.hovers_interactive());
        assert_eq!(mp.hovered(), None);
    }

    #[test]
    fn on_move_with_no_position_clears_hover() {
        let mut mp = MenuPointer::new();
        let r = rects();
        assert_eq!(mp.on_move(Some(LogicalPos::new(100.0, 100.0)), &r), Some(0));
        assert_eq!(mp.on_move(None, &r), None);
        assert!(!mp.hovers_interactive());
    }

    #[test]
    fn on_leave_clears_hover() {
        let mut mp = MenuPointer::new();
        let r = rects();
        mp.on_move(Some(LogicalPos::new(100.0, 100.0)), &r);
        mp.on_leave();
        assert!(!mp.hovers_interactive());
    }

    #[test]
    fn on_click_returns_clicked_row_index() {
        let mp = MenuPointer::new();
        let r = rects();
        assert_eq!(mp.on_click(Some(LogicalPos::new(100.0, 160.0)), &r), Some(2));
        assert_eq!(mp.on_click(Some(LogicalPos::new(0.0, 0.0)), &r), None);
        assert_eq!(mp.on_click(None, &r), None);
    }
}
