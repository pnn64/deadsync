//! Lightweight axis-aligned hit-testing for mouse-driven UI.
//!
//! Each screen that opts in to mouse input rebuilds a small `Vec<HitRect>`
//! each frame (alongside its actor list) describing where its interactive
//! regions are in logical (top-left origin) coordinates. The pointer
//! plumbing in `app/mod.rs` queries `hit_test` to map pointer positions to
//! a screen-local id.
//!
//! Rectangles are simple AABBs. If a screen needs rotated or non-rectangular
//! buttons later it can layer that on top, but every interactive element in
//! the current codebase is rectangular.

use crate::engine::space::LogicalPos;

/// A screen-local interactive region. `id` is opaque to this module — screens
/// pick whatever discriminator is convenient (often a menu index or an enum
/// cast to u32).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitRect {
    pub min: LogicalPos,
    pub max: LogicalPos,
    pub id: u32,
}

impl HitRect {
    /// Build a `HitRect` from a center point and full width/height. Useful
    /// for menu items laid out by `menu_list::build_vertical_menu`, where
    /// each row is described by its center.
    #[inline]
    pub fn from_center(cx: f32, cy: f32, w: f32, h: f32, id: u32) -> Self {
        let hw = 0.5 * w;
        let hh = 0.5 * h;
        Self {
            min: LogicalPos::new(cx - hw, cy - hh),
            max: LogicalPos::new(cx + hw, cy + hh),
            id,
        }
    }

    #[inline]
    pub fn contains(&self, p: LogicalPos) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
}

/// Find the topmost (last-pushed) `HitRect` that contains `point` and return
/// its `id`. Iteration is reversed so later entries — which by convention
/// are drawn last and so appear "on top" — win ties.
#[inline]
pub fn hit_test(rects: &[HitRect], point: LogicalPos) -> Option<u32> {
    for r in rects.iter().rev() {
        if r.contains(point) {
            return Some(r.id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{HitRect, hit_test};
    use crate::engine::space::LogicalPos;

    #[test]
    fn contains_includes_borders() {
        let r = HitRect {
            min: LogicalPos::new(10.0, 20.0),
            max: LogicalPos::new(30.0, 40.0),
            id: 0,
        };
        assert!(r.contains(LogicalPos::new(10.0, 20.0)));
        assert!(r.contains(LogicalPos::new(30.0, 40.0)));
        assert!(r.contains(LogicalPos::new(20.0, 30.0)));
        assert!(!r.contains(LogicalPos::new(9.9, 30.0)));
        assert!(!r.contains(LogicalPos::new(30.1, 30.0)));
        assert!(!r.contains(LogicalPos::new(20.0, 19.9)));
        assert!(!r.contains(LogicalPos::new(20.0, 40.1)));
    }

    #[test]
    fn from_center_round_trips() {
        let r = HitRect::from_center(100.0, 200.0, 50.0, 20.0, 7);
        assert_eq!(r.min, LogicalPos::new(75.0, 190.0));
        assert_eq!(r.max, LogicalPos::new(125.0, 210.0));
        assert!(r.contains(LogicalPos::new(100.0, 200.0)));
        assert!(!r.contains(LogicalPos::new(74.9, 200.0)));
    }

    #[test]
    fn hit_test_picks_topmost_overlap() {
        let rects = vec![
            HitRect::from_center(50.0, 50.0, 40.0, 40.0, 1),
            HitRect::from_center(55.0, 55.0, 40.0, 40.0, 2),
        ];
        // Inside only the first rect.
        assert_eq!(hit_test(&rects, LogicalPos::new(32.0, 32.0)), Some(1));
        // Inside both — topmost (id=2) wins.
        assert_eq!(hit_test(&rects, LogicalPos::new(50.0, 50.0)), Some(2));
        // Outside both.
        assert_eq!(hit_test(&rects, LogicalPos::new(100.0, 100.0)), None);
    }

    #[test]
    fn hit_test_empty_is_none() {
        assert_eq!(hit_test(&[], LogicalPos::new(0.0, 0.0)), None);
    }
}

