const HOLD_BODY_LEGACY_SEGMENT_LIMIT: usize = 512;
const HOLD_BODY_SEGMENT_SAFETY_MAX: usize = 65_536;

#[inline(always)]
pub fn hold_tail_cap_bounds(
    body_tail_y: f32,
    cap_height: f32,
    rendered_body_top: Option<f32>,
    rendered_body_bottom: Option<f32>,
) -> Option<(f32, f32)> {
    let default_bounds = (body_tail_y, body_tail_y + cap_height);
    let rb = match (rendered_body_top, rendered_body_bottom) {
        (Some(t), Some(b)) if b > t + 0.5 => b,
        _ => return Some(default_bounds),
    };

    let dist = body_tail_y - rb;
    if dist < -2.0 || dist > cap_height + 2.0 {
        return Some(default_bounds);
    }

    Some((rb, rb + cap_height))
}

#[inline(always)]
pub fn clipped_hold_body_bounds(
    body_top: f32,
    body_bottom: f32,
    natural_top: f32,
    natural_bottom: f32,
) -> Option<(f32, f32)> {
    let clipped_top = body_top.max(natural_top);
    let clipped_bottom = body_bottom.min(natural_bottom);
    (clipped_bottom > clipped_top).then_some((clipped_top, clipped_bottom))
}

#[inline(always)]
pub fn hold_draw_span(y_head: f32, y_tail: f32, screen_height: f32) -> Option<(f32, f32)> {
    let mut top = y_head.min(y_tail);
    let mut bottom = y_head.max(y_tail);
    if bottom < -200.0 || top > screen_height + 200.0 {
        return None;
    }
    top = top.max(-400.0);
    bottom = bottom.min(screen_height + 400.0);
    (bottom >= top).then_some((top, bottom))
}

#[inline(always)]
pub fn hold_body_segment_budget(visible_span: f32, segment_height: f32) -> (usize, bool) {
    let estimated = if visible_span <= f32::EPSILON || segment_height <= f32::EPSILON {
        1
    } else {
        (visible_span / segment_height).ceil() as usize
    };
    let max_segments = estimated
        .saturating_add(2)
        .clamp(2048, HOLD_BODY_SEGMENT_SAFETY_MAX);
    (max_segments, estimated <= HOLD_BODY_LEGACY_SEGMENT_LIMIT)
}

#[inline(always)]
pub fn bottom_cap_uv_window(
    v_base0: f32,
    v_base1: f32,
    draw_height: f32,
    cap_span: f32,
    anchor_to_top: bool,
) -> Option<(f32, f32)> {
    if cap_span <= f32::EPSILON || draw_height <= f32::EPSILON {
        return None;
    }
    let tex_add = if anchor_to_top {
        0.0
    } else {
        (1.0 - draw_height / cap_span).clamp(0.0, 1.0)
    };
    let v_span = v_base1 - v_base0;
    let t0 = tex_add;
    let t1 = (draw_height / cap_span) + tex_add;
    Some((v_base0 + v_span * t0, v_base0 + v_span * t1))
}

#[inline(always)]
pub fn hold_segment_pose(top: [f32; 2], bottom: [f32; 2]) -> ([f32; 2], f32, f32) {
    let dx = bottom[0] - top[0];
    let dy = bottom[1] - top[1];
    let length = dx.hypot(dy);
    let rotation_deg = if length <= f32::EPSILON {
        0.0
    } else {
        dx.atan2(dy).to_degrees()
    };
    (
        [(top[0] + bottom[0]) * 0.5, (top[1] + bottom[1]) * 0.5],
        length,
        rotation_deg,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        bottom_cap_uv_window, clipped_hold_body_bounds, hold_body_segment_budget, hold_draw_span,
        hold_segment_pose, hold_tail_cap_bounds,
    };

    #[test]
    fn hold_tail_cap_bounds_join_at_body_bottom_for_normal_scroll() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        let (top, bottom) = hold_tail_cap_bounds(body_tail_y, cap_height, Some(20.0), Some(96.0))
            .expect("cap should draw");
        assert_eq!((top, bottom), (96.0, 120.0));
    }

    #[test]
    fn hold_tail_cap_bounds_falls_back_when_body_is_below_tail_anchor() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(104.0), Some(160.0)),
            Some((100.0, 124.0))
        );
    }

    #[test]
    fn hold_tail_cap_bounds_skip_when_body_does_not_reach_tail() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(20.0), Some(70.0)),
            Some((100.0, 124.0))
        );
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(140.0), Some(200.0)),
            Some((100.0, 124.0))
        );
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, None, Some(95.0)),
            Some((100.0, 124.0))
        );
    }

    #[test]
    fn collapsed_hold_body_uses_tail_cap_fallback_bounds() {
        let body_top = 120.0;
        let body_bottom = 120.0;
        let natural_top = 100.0;
        let natural_bottom = 100.0;
        assert_eq!(
            clipped_hold_body_bounds(body_top, body_bottom, natural_top, natural_bottom),
            None
        );
        assert_eq!(
            hold_tail_cap_bounds(natural_bottom, 24.0, None, None),
            Some((100.0, 124.0))
        );
    }

    #[test]
    fn collapsed_hold_draw_span_still_draws_caps() {
        assert_eq!(hold_draw_span(120.0, 120.0, 480.0), Some((120.0, 120.0)));
    }

    #[test]
    fn tiny_hold_body_repeat_uses_mesh_budget() {
        let (budget, allow_legacy) = hold_body_segment_budget(900.0, 0.25);
        assert!(budget >= 3602);
        assert!(!allow_legacy);
    }

    #[test]
    fn normal_hold_body_repeat_keeps_legacy_budget() {
        let (budget, allow_legacy) = hold_body_segment_budget(900.0, 64.0);
        assert_eq!(budget, 2048);
        assert!(allow_legacy);
    }

    #[test]
    fn bottom_cap_uv_window_matches_itg_add_to_tex_coord_progression() {
        let (v0, v1) = bottom_cap_uv_window(0.0, 1.0, 12.0, 24.0, false)
            .expect("partial cap should produce UVs");
        assert!((v0 - 0.5).abs() <= 1e-6);
        assert!((v1 - 1.0).abs() <= 1e-6);

        let (full_v0, full_v1) =
            bottom_cap_uv_window(0.0, 1.0, 24.0, 24.0, false).expect("full cap should produce UVs");
        assert!((full_v0 - 0.0).abs() <= 1e-6);
        assert!((full_v1 - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn bottom_cap_uv_window_honors_top_anchor_when_reverse() {
        let (v0, v1) = bottom_cap_uv_window(0.2, 0.8, 12.0, 24.0, true)
            .expect("top-anchored reverse path should produce UVs");
        assert!((v0 - 0.2).abs() <= 1e-6);
        assert!((v1 - 0.5).abs() <= 1e-6);
    }

    #[test]
    fn bottom_cap_uv_window_rejects_degenerate_inputs() {
        assert_eq!(bottom_cap_uv_window(0.0, 1.0, 0.0, 24.0, false), None);
        assert_eq!(bottom_cap_uv_window(0.0, 1.0, 24.0, 0.0, false), None);
    }

    #[test]
    fn hold_segment_pose_keeps_vertical_segments_unrotated() {
        let (center, length, rotation) = hold_segment_pose([32.0, 100.0], [32.0, 180.0]);
        assert_eq!(center, [32.0, 140.0]);
        assert!((length - 80.0).abs() <= 1e-6);
        assert!(rotation.abs() <= 1e-6);
    }

    #[test]
    fn hold_segment_pose_uses_diagonal_length_and_rotation() {
        let (center, length, rotation) = hold_segment_pose([0.0, 0.0], [30.0, 40.0]);
        assert_eq!(center, [15.0, 20.0]);
        assert!((length - 50.0).abs() <= 1e-6);
        assert!((rotation - 36.869_896).abs() <= 1e-5);
    }
}
