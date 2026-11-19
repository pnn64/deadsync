use crate::ui_layout::UILayoutType;

pub(crate) const DIFFICULTY_METER_SIZE: f32 = 30.0;
const MAX_HUD_SCORE_WIDTH: f32 = 854.0;
const SQUAREISH_HUD_REFERENCE_WIDTH: f32 = 640.0;
const SQUAREISH_DIFFICULTY_OFFSET: f32 = 292.5;
const WIDE_DIFFICULTY_OFFSET: f32 = 342.5;
const P1_SCORE_OFFSET_DIVISOR: f32 = 4.3;
const P2_SCORE_OFFSET_DIVISOR: f32 = 2.75;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GameplayHudXPositions {
    pub score_p1: f32,
    pub score_p2: f32,
    pub difficulty_p1: f32,
    pub difficulty_p2: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GameplayHudXOffsets {
    reference_width: f32,
    difficulty_meter_offset: f32,
}

impl GameplayHudXOffsets {
    const SQUAREISH: Self = Self {
        reference_width: SQUAREISH_HUD_REFERENCE_WIDTH,
        difficulty_meter_offset: SQUAREISH_DIFFICULTY_OFFSET,
    };

    const WIDE: Self = Self {
        reference_width: MAX_HUD_SCORE_WIDTH,
        difficulty_meter_offset: WIDE_DIFFICULTY_OFFSET,
    };

    const fn for_layout_type(layout_type: UILayoutType) -> Self {
        match layout_type {
            UILayoutType::Squareish => Self::SQUAREISH,
            UILayoutType::Wide | UILayoutType::UltraWide => Self::WIDE,
        }
    }
}

pub(crate) fn gameplay_hud_x_positions(screen_w: f32, screen_h: f32) -> GameplayHudXPositions {
    let aspect_ratio = screen_w / screen_h.max(1.0);
    let offsets =
        GameplayHudXOffsets::for_layout_type(UILayoutType::from_aspect_ratio(aspect_ratio));
    let screen_center_x = screen_w * 0.5;
    let score_anchor_width = screen_w.min(MAX_HUD_SCORE_WIDTH);
    let difficulty_offset =
        offsets.difficulty_meter_offset * (screen_w / offsets.reference_width).clamp(0.0, 1.0);
    let difficulty_half_width = (DIFFICULTY_METER_SIZE * 0.5).min(screen_w * 0.5);

    GameplayHudXPositions {
        score_p1: screen_center_x - score_anchor_width / P1_SCORE_OFFSET_DIVISOR,
        score_p2: screen_center_x + score_anchor_width / P2_SCORE_OFFSET_DIVISOR,
        difficulty_p1: (screen_center_x - difficulty_offset)
            .clamp(difficulty_half_width, screen_w - difficulty_half_width),
        difficulty_p2: (screen_center_x + difficulty_offset)
            .clamp(difficulty_half_width, screen_w - difficulty_half_width),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GameplayTopRowLayout {
    pub scale: f32,
    pub field_scale: f32,
    pub song_meter_width: f32,
    pub health_meter_offset: f32,
    pub health_meter_width: f32,
    pub vertical_health_meter_offset: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GameplayTopRowLayoutOffsets {
    song_meter_width: f32,
    health_meter_offset: f32,
    vertical_health_meter_offset: f32,
}

impl GameplayTopRowLayoutOffsets {
    const SQUAREISH: Self = Self {
        song_meter_width: 310.0,
        health_meter_offset: 238.0,
        vertical_health_meter_offset: 302.0,
    };

    const WIDE: Self = Self {
        song_meter_width: 417.0,
        health_meter_offset: 288.0,
        vertical_health_meter_offset: 400.0,
    };

    const fn for_layout_type(layout_type: UILayoutType) -> Self {
        match layout_type {
            UILayoutType::Squareish => Self::SQUAREISH,
            UILayoutType::Wide | UILayoutType::UltraWide => Self::WIDE,
        }
    }
}

pub(crate) fn gameplay_top_row_layout(screen_w: f32, screen_h: f32) -> GameplayTopRowLayout {
    let aspect_ratio = screen_w / screen_h.max(1.0);
    let layout_type = UILayoutType::from_aspect_ratio(aspect_ratio);
    let offsets = GameplayTopRowLayoutOffsets::for_layout_type(layout_type);
    let scale = if layout_type == UILayoutType::Squareish {
        (aspect_ratio / (4.0 / 3.0)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let field_scale = if layout_type == UILayoutType::Squareish {
        scale.clamp(0.5, 1.0)
    } else {
        1.0
    };
    let health_meter_width = 136.0 * scale;
    let vertical_health_meter_width = 16.0 * scale;
    let vertical_health_meter_offset = (offsets.vertical_health_meter_offset * scale)
        .min((screen_w * 0.5 - (vertical_health_meter_width + 2.0) * 0.5).max(0.0));

    GameplayTopRowLayout {
        scale,
        field_scale,
        song_meter_width: offsets.song_meter_width * scale,
        health_meter_offset: offsets.health_meter_offset * scale,
        health_meter_width,
        vertical_health_meter_offset,
    }
}

#[cfg(test)]
mod tests {
    use super::{DIFFICULTY_METER_SIZE, gameplay_hud_x_positions, gameplay_top_row_layout};

    #[test]
    fn hud_anchors_scale_to_narrow_viewports() {
        let four_three = gameplay_hud_x_positions(640.0, 480.0);
        assert_eq!(four_three.difficulty_p1, 27.5);
        assert_eq!(four_three.difficulty_p2, 612.5);

        for (preset_width, preset_height) in [
            (640.0, 480.0),
            (800.0, 600.0),
            (1024.0, 768.0),
            (1280.0, 1024.0),
            (1280.0, 720.0),
            (640.0, 640.0),
            (1280.0, 800.0),
            (1280.0, 540.0),
            (1720.0, 720.0),
            (540.0, 960.0),
        ] {
            let aspect = preset_width / preset_height;
            let width = 480.0 * aspect;
            let height = 480.0;
            let anchors = gameplay_hud_x_positions(width, height);
            assert!(anchors.score_p1 > 0.0 && anchors.score_p1 < width);
            assert!(anchors.score_p2 > 0.0 && anchors.score_p2 < width);
            assert!(anchors.difficulty_p1 >= DIFFICULTY_METER_SIZE * 0.5);
            assert!(anchors.difficulty_p2 <= width - DIFFICULTY_METER_SIZE * 0.5);
        }
    }

    #[test]
    fn wide_and_ultrawide_share_difficulty_offsets() {
        let wide = gameplay_hud_x_positions(1280.0, 720.0);
        let ultrawide = gameplay_hud_x_positions(1720.0, 720.0);

        assert_eq!(640.0 - wide.difficulty_p1, 860.0 - ultrawide.difficulty_p1);
        assert_eq!(wide.difficulty_p2 - 640.0, ultrawide.difficulty_p2 - 860.0);
    }

    #[test]
    fn top_row_fits_all_aspect_presets() {
        for (preset_width, preset_height) in [
            (640.0, 480.0),
            (800.0, 600.0),
            (1024.0, 768.0),
            (1280.0, 1024.0),
            (1280.0, 720.0),
            (640.0, 640.0),
            (1280.0, 800.0),
            (1280.0, 540.0),
            (1720.0, 720.0),
            (540.0, 960.0),
        ] {
            let aspect_ratio = preset_width / preset_height;
            let screen_h = 480.0;
            let screen_w = screen_h * aspect_ratio;
            let layout = gameplay_top_row_layout(screen_w, screen_h);
            let health_half_width = layout.health_meter_width * 0.5;
            let health_outer_edge =
                layout.health_meter_offset + health_half_width + 2.0 * layout.scale;
            let health_inner_edge = layout.health_meter_offset - health_half_width;
            let vertical_health_outer_edge =
                layout.vertical_health_meter_offset + (16.0 * layout.scale + 2.0) * 0.5;

            assert!(
                health_outer_edge <= screen_w * 0.5,
                "{aspect_ratio}: healthbar edge"
            );
            assert!(
                vertical_health_outer_edge <= screen_w * 0.5,
                "{aspect_ratio}: vertical healthbar edge"
            );
            assert!(
                health_inner_edge >= layout.song_meter_width * 0.5,
                "{aspect_ratio}: row overlap"
            );
        }
    }

    #[test]
    fn wide_and_ultrawide_share_top_row_offsets() {
        let wide = gameplay_top_row_layout(1280.0, 720.0);
        let ultrawide = gameplay_top_row_layout(1720.0, 720.0);

        assert_eq!(wide.song_meter_width, ultrawide.song_meter_width);
        assert_eq!(wide.health_meter_offset, ultrawide.health_meter_offset);
        assert_eq!(
            wide.vertical_health_meter_offset,
            ultrawide.vertical_health_meter_offset
        );
    }
}
