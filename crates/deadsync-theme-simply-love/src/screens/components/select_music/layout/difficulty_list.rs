use super::WidgetBounds;

const DIFFICULTY_SLOT_SIZE: f32 = 28.0;
pub(crate) const DIFFICULTY_SLOT_BORDER: f32 = 2.0;
pub(crate) const DIFFICULTY_SLOT_COUNT: usize = 5;
pub(crate) const DIFFICULTY_LIST_WIDTH: f32 = DIFFICULTY_SLOT_SIZE + 2.0 * DIFFICULTY_SLOT_BORDER;
pub(crate) const DIFFICULTY_LIST_HEIGHT: f32 = DIFFICULTY_SLOT_SIZE * DIFFICULTY_SLOT_COUNT as f32
    + DIFFICULTY_SLOT_BORDER * (DIFFICULTY_SLOT_COUNT as f32 + 1.0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DifficultyListLayout {
    pub bounds: WidgetBounds,
    pub slot_centers_y: [f32; DIFFICULTY_SLOT_COUNT],
    pub slot_size: f32,
}

impl DifficultyListLayout {
    pub(crate) fn anchored_bottom_right(chart_panel: WidgetBounds) -> Self {
        let bounds = WidgetBounds::new(
            chart_panel.right() - DIFFICULTY_LIST_WIDTH,
            chart_panel.bottom() - DIFFICULTY_LIST_HEIGHT,
            DIFFICULTY_LIST_WIDTH,
            DIFFICULTY_LIST_HEIGHT,
        );
        const BORDER: f32 = 2.0;
        let slot_height = (bounds.height - BORDER * 2.0) / DIFFICULTY_SLOT_COUNT as f32;
        let slot_centers_y =
            std::array::from_fn(|slot| bounds.y + BORDER + (slot as f32 + 0.5) * slot_height);

        Self {
            bounds,
            slot_centers_y,
            slot_size: DIFFICULTY_SLOT_SIZE,
        }
    }
}
