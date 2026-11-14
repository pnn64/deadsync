use crate::ui_layout::UILayoutType;
use deadlib_present::space::{screen_height, screen_width};

pub(crate) const BANNER_NATIVE_WIDTH: f32 = 418.0;
pub(crate) const BANNER_NATIVE_HEIGHT: f32 = 164.0;
pub(crate) const TRACKLIST_RATING_BOX_WIDTH: f32 = 32.0;
pub(crate) const TRACKLIST_RATING_BOX_HEIGHT: f32 = 152.0;

const RATING_VISIBLE_SLOTS: usize = 5;
const RATING_SLOT_SPACING: f32 = 30.0;
const RATING_SLOT_SIZE: f32 = 28.0;
const TRACKLIST_ROW_SPACING: f32 = 23.0;
const TRACKLIST_TARGET_VISIBLE_ROWS: usize = 6;
const TRACKLIST_TEXT_Y_OFFSET: f32 = 0.0;
const TRACKLIST_TEXT_HEIGHT: f32 = 15.0;

const LEFT_PANE_REFERENCE_WIDTH: f32 = 310.0;
const SUMMARY_REFERENCE_WIDTH: f32 = 320.0;
const LEFT_PANE_INSET: f32 = 10.0;
const RATING_BOX_GAP: f32 = 2.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CourseStatsLayout {
    Compact,
    Standard,
}

#[derive(Clone, Copy)]
struct LayoutConfig {
    scale_from_pane: f32,
    scale_offset: f32,
    content_center_screen_scale: f32,
    content_center_pane_scale: f32,
    content_center_offset: f32,
    summary_width_pane_scale: f32,
    summary_width_offset: f32,
    banner_zoom_pane_scale: f32,
    banner_zoom_offset: f32,
    rating_center_screen_scale: f32,
    rating_center_pane_scale: f32,
    rating_center_box_width_scale: f32,
    rating_center_offset: f32,
    panel_width: f32,
    panel_left_rating_center_scale: f32,
    panel_left_scale_offset: f32,
    stats_layout: CourseStatsLayout,
}

const SQUAREISH_LAYOUT: LayoutConfig = LayoutConfig {
    scale_from_pane: 1.0 / LEFT_PANE_REFERENCE_WIDTH,
    scale_offset: 0.0,
    content_center_screen_scale: 0.0,
    content_center_pane_scale: 0.5,
    content_center_offset: 0.0,
    summary_width_pane_scale: 1.0,
    summary_width_offset: 0.0,
    banner_zoom_pane_scale: 1.0 / BANNER_NATIVE_WIDTH,
    banner_zoom_offset: 0.0,
    rating_center_screen_scale: 0.0,
    rating_center_pane_scale: 1.0,
    rating_center_box_width_scale: -0.5,
    rating_center_offset: 0.0,
    panel_width: 276.0,
    panel_left_rating_center_scale: 0.0,
    panel_left_scale_offset: 0.0,
    stats_layout: CourseStatsLayout::Compact,
};

const WIDE_LAYOUT: LayoutConfig = LayoutConfig {
    scale_from_pane: 0.0,
    scale_offset: 1.0,
    content_center_screen_scale: 0.5,
    content_center_pane_scale: 0.0,
    content_center_offset: -170.0,
    summary_width_pane_scale: 0.0,
    summary_width_offset: SUMMARY_REFERENCE_WIDTH,
    banner_zoom_pane_scale: 0.0,
    banner_zoom_offset: 0.7655,
    rating_center_screen_scale: 0.5,
    rating_center_pane_scale: 0.0,
    rating_center_box_width_scale: 0.0,
    rating_center_offset: -26.0,
    panel_width: 286.0,
    panel_left_rating_center_scale: 1.0,
    panel_left_scale_offset: -(TRACKLIST_RATING_BOX_WIDTH * 0.5 + RATING_BOX_GAP + 286.0),
    stats_layout: CourseStatsLayout::Standard,
};

const ULTRA_WIDE_LAYOUT: LayoutConfig = LayoutConfig {
    scale_from_pane: 0.0,
    scale_offset: 1.0,
    content_center_screen_scale: 0.5,
    content_center_pane_scale: 0.0,
    content_center_offset: -170.0,
    summary_width_pane_scale: 0.0,
    summary_width_offset: SUMMARY_REFERENCE_WIDTH,
    banner_zoom_pane_scale: 0.0,
    banner_zoom_offset: 0.7655,
    rating_center_screen_scale: 0.5,
    rating_center_pane_scale: 0.0,
    rating_center_box_width_scale: 0.0,
    rating_center_offset: -26.0,
    panel_width: 286.0,
    panel_left_rating_center_scale: 1.0,
    panel_left_scale_offset: -(TRACKLIST_RATING_BOX_WIDTH * 0.5 + RATING_BOX_GAP + 286.0),
    stats_layout: CourseStatsLayout::Standard,
};

fn layout_config(layout_type: UILayoutType) -> &'static LayoutConfig {
    match layout_type {
        UILayoutType::Squareish => &SQUAREISH_LAYOUT,
        UILayoutType::Wide => &WIDE_LAYOUT,
        UILayoutType::UltraWide => &ULTRA_WIDE_LAYOUT,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CourseSelectLayout {
    pub scale: f32,
    pub left_pane_width: f32,
    pub content_center_x: f32,
    pub banner_center_y: f32,
    pub banner_zoom: f32,
    pub summary_width: f32,
    pub summary_center_y: f32,
    pub rating_box_center_x: f32,
    pub rating_box_center_y: f32,
    pub rating_box_width: f32,
    pub rating_box_height: f32,
    pub panel_left: f32,
    pub panel_top: f32,
    pub panel_width: f32,
    pub panel_height: f32,
    pub stats_layout: CourseStatsLayout,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CourseRatingListLayout {
    pub panel_left: f32,
    pub panel_top: f32,
    pub panel_width: f32,
    pub panel_height: f32,
    pub panel_bottom: f32,
    pub scale: f32,
    pub list_left_x: f32,
    pub list_title_x: f32,
    pub list_start_y: f32,
    pub list_right_pad: f32,
    pub list_clip: [f32; 4],
    pub visible_rows: usize,
    pub row_spacing: f32,
    pub text_height: f32,
    pub rating_box_center_x: f32,
    pub rating_box_center_y: f32,
    pub rating_box_width: f32,
    pub rating_box_height: f32,
    pub rating_visible_slots: usize,
    pub rating_slot_spacing: f32,
    pub rating_slot_size: f32,
    pub rating_meter_zoom: f32,
    pub rating_arrow_y_offset: f32,
    pub rating_arrow_p1_x_offset: f32,
    pub rating_arrow_p2_x_offset: f32,
    pub rating_arrow_bounce_distance: f32,
    pub rating_arrow_zoom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CourseSummaryLayout {
    pub scale: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
}

impl CourseRatingListLayout {
    pub(crate) fn rating_slot_y(self, slot: usize) -> f32 {
        let center_slot = (self.rating_visible_slots - 1) as f32 * 0.5;
        (slot as f32 - center_slot).mul_add(self.rating_slot_spacing, self.rating_box_center_y)
    }

    pub(crate) fn rating_arrow_y(self, slot: usize) -> f32 {
        self.rating_slot_y(slot) + self.rating_arrow_y_offset
    }
}

impl CourseSelectLayout {
    pub(crate) fn current() -> Self {
        Self::for_dimensions(screen_width(), screen_height(), UILayoutType::current())
    }

    fn for_dimensions(screen_width: f32, screen_height: f32, layout_type: UILayoutType) -> Self {
        let constants = layout_config(layout_type);
        let left_pane_width = (screen_width * 0.5 - LEFT_PANE_INSET).max(0.0);
        let scale = left_pane_width
            .mul_add(constants.scale_from_pane, constants.scale_offset)
            .clamp(0.0, 1.0);
        let content_center_x = screen_width.mul_add(
            constants.content_center_screen_scale,
            left_pane_width * constants.content_center_pane_scale,
        ) + constants.content_center_offset;
        let summary_width = left_pane_width.mul_add(
            constants.summary_width_pane_scale,
            constants.summary_width_offset,
        );
        let banner_zoom = left_pane_width.mul_add(
            constants.banner_zoom_pane_scale,
            constants.banner_zoom_offset,
        );
        let rating_box_width = TRACKLIST_RATING_BOX_WIDTH * scale;
        let rating_box_height = TRACKLIST_RATING_BOX_HEIGHT * scale;
        let rating_box_center_x = screen_width.mul_add(
            constants.rating_center_screen_scale,
            left_pane_width.mul_add(
                constants.rating_center_pane_scale,
                rating_box_width * constants.rating_center_box_width_scale,
            ) + constants.rating_center_offset,
        );
        let rating_box_center_y = screen_height * 0.5 + 67.0;
        let panel_width = constants.panel_width * scale;
        let panel_left = rating_box_center_x * constants.panel_left_rating_center_scale
            + constants.panel_left_scale_offset * scale;
        let panel_height = rating_box_height;
        let panel_top = rating_box_center_y - panel_height * 0.5;

        Self {
            scale,
            left_pane_width,
            content_center_x,
            banner_center_y: 96.0,
            banner_zoom,
            summary_width,
            summary_center_y: screen_height * 0.5 - 55.0,
            rating_box_center_x,
            rating_box_center_y,
            rating_box_width,
            rating_box_height,
            panel_left,
            panel_top,
            panel_width,
            panel_height,
            stats_layout: constants.stats_layout,
        }
    }

    pub(crate) fn rating_list(self) -> CourseRatingListLayout {
        let scale = self.scale;
        let list_left_x = self.panel_left + 10.0 * scale;
        CourseRatingListLayout {
            panel_left: self.panel_left,
            panel_top: self.panel_top,
            panel_width: self.panel_width,
            panel_height: self.panel_height,
            panel_bottom: self.panel_top + self.panel_height,
            scale,
            list_left_x,
            list_title_x: list_left_x + 38.0 * scale,
            list_start_y: self.panel_top + (8.0 + TRACKLIST_TEXT_Y_OFFSET) * scale,
            list_right_pad: 14.0 * scale,
            list_clip: [
                self.panel_left,
                self.panel_top,
                self.panel_width,
                self.panel_height,
            ],
            visible_rows: TRACKLIST_TARGET_VISIBLE_ROWS,
            row_spacing: TRACKLIST_ROW_SPACING * scale,
            text_height: TRACKLIST_TEXT_HEIGHT * scale,
            rating_box_center_x: self.rating_box_center_x,
            rating_box_center_y: self.rating_box_center_y,
            rating_box_width: self.rating_box_width,
            rating_box_height: self.rating_box_height,
            rating_visible_slots: RATING_VISIBLE_SLOTS,
            rating_slot_spacing: RATING_SLOT_SPACING * scale,
            rating_slot_size: RATING_SLOT_SIZE * scale,
            rating_meter_zoom: 0.45 * scale,
            rating_arrow_y_offset: scale,
            rating_arrow_p1_x_offset: -27.0 * scale,
            rating_arrow_p2_x_offset: 8.0 * scale,
            rating_arrow_bounce_distance: 3.0 * scale,
            rating_arrow_zoom: 0.575 * scale,
        }
    }

    pub(crate) fn course_summary(self) -> CourseSummaryLayout {
        CourseSummaryLayout {
            scale: self.scale,
            center_x: self.content_center_x,
            center_y: self.summary_center_y,
            width: self.summary_width,
            height: 50.0 * self.scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CourseSelectLayout, CourseStatsLayout};
    use crate::ui_layout::UILayoutType;

    #[test]
    fn squareish_course_widgets_fit_inside_the_left_pane() {
        for width in [480.0, 600.0, 640.0] {
            let layout = CourseSelectLayout::for_dimensions(width, 480.0, UILayoutType::Squareish);
            let pane_right = layout.left_pane_width;
            let panel_right = layout.panel_left + layout.panel_width;
            let rating_right = layout.rating_box_center_x + layout.rating_box_width * 0.5;
            let summary_left = layout.content_center_x - layout.summary_width * 0.5;
            let summary_right = layout.content_center_x + layout.summary_width * 0.5;

            assert!(layout.panel_left >= -f32::EPSILON);
            assert!(panel_right <= pane_right + f32::EPSILON);
            assert!(rating_right <= pane_right + f32::EPSILON);
            assert!(summary_left >= -f32::EPSILON);
            assert!(summary_right <= pane_right + f32::EPSILON);
        }
    }

    #[test]
    fn wide_course_layout_keeps_its_existing_tracklist_geometry() {
        let layout = CourseSelectLayout::for_dimensions(853.0, 480.0, UILayoutType::Wide);

        assert_eq!(layout.scale, 1.0);
        assert_eq!(layout.content_center_x, 256.5);
        assert_eq!(layout.panel_left, 96.5);
        assert_eq!(layout.panel_width, 286.0);
        assert_eq!(layout.stats_layout, CourseStatsLayout::Standard);
    }

    #[test]
    fn ultra_wide_course_layout_preserves_standard_geometry() {
        let layout = CourseSelectLayout::for_dimensions(1138.0, 480.0, UILayoutType::UltraWide);

        assert_eq!(layout.scale, 1.0);
        assert_eq!(layout.content_center_x, 399.0);
        assert_eq!(layout.stats_layout, CourseStatsLayout::Standard);
    }

    #[test]
    fn rating_list_layout_anchors_rows_and_selector_to_the_panel() {
        let layout = CourseSelectLayout::for_dimensions(640.0, 640.0, UILayoutType::Squareish);
        let rating_list = layout.rating_list();

        assert_eq!(
            rating_list.list_left_x,
            rating_list.panel_left + 10.0 * layout.scale
        );
        assert_eq!(
            rating_list.list_title_x,
            rating_list.list_left_x + 38.0 * layout.scale
        );
        assert_eq!(
            rating_list.list_clip,
            [
                rating_list.panel_left,
                rating_list.panel_top,
                rating_list.panel_width,
                rating_list.panel_height,
            ]
        );
        assert_eq!(rating_list.rating_box_center_x, layout.rating_box_center_x);
        assert_eq!(rating_list.rating_box_center_y, layout.rating_box_center_y);
        assert_eq!(rating_list.visible_rows, 6);
        assert_eq!(rating_list.rating_visible_slots, 5);
    }
}
