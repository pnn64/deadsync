pub const NUM_QUANTIZATIONS: usize = 9;
pub const ITG_DANCE_COL_SPACING: i32 = 64;

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub num_cols: usize,
    pub num_players: usize,
}

pub fn itg_column_xs(num_cols: usize) -> Vec<i32> {
    if num_cols == 0 {
        return Vec::new();
    }
    let half_spacing = ITG_DANCE_COL_SPACING / 2;
    (0..num_cols)
        .map(|i| (i as i32 * ITG_DANCE_COL_SPACING) - ((num_cols - 1) as i32 * half_spacing))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum Quantization {
    Q4th = 0,
    Q8th,
    Q12th,
    Q16th,
    Q24th,
    Q32nd,
    Q48th,
    Q64th,
    Q192nd,
}

pub const NOTE_ANIM_PART_COUNT: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum NoteAnimPart {
    Tap = 0,
    Mine,
    Lift,
    Fake,
    HoldHead,
    HoldTopCap,
    HoldBody,
    HoldBottomCap,
    HoldTail,
    RollHead,
    RollTopCap,
    RollBody,
    RollBottomCap,
    RollTail,
}

impl NoteAnimPart {
    pub const ALL: [Self; NOTE_ANIM_PART_COUNT] = [
        Self::Tap,
        Self::Mine,
        Self::Lift,
        Self::Fake,
        Self::HoldHead,
        Self::HoldTopCap,
        Self::HoldBody,
        Self::HoldBottomCap,
        Self::HoldTail,
        Self::RollHead,
        Self::RollTopCap,
        Self::RollBody,
        Self::RollBottomCap,
        Self::RollTail,
    ];

    pub const fn metric_prefix(self) -> &'static str {
        match self {
            Self::Tap => "TapNote",
            Self::Mine => "TapMine",
            Self::Lift => "TapLift",
            Self::Fake => "TapFake",
            Self::HoldHead => "HoldHead",
            Self::HoldTopCap => "HoldTopCap",
            Self::HoldBody => "HoldBody",
            Self::HoldBottomCap => "HoldBottomCap",
            Self::HoldTail => "HoldTail",
            Self::RollHead => "RollHead",
            Self::RollTopCap => "RollTopCap",
            Self::RollBody => "RollBody",
            Self::RollBottomCap => "RollBottomCap",
            Self::RollTail => "RollTail",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NoteDisplayMetrics {
    pub draw_hold_head_for_taps_on_same_row: bool,
    pub draw_roll_head_for_taps_on_same_row: bool,
    pub tap_hold_roll_on_row_means_hold: bool,
    pub hold_head_is_above_wavy_parts: bool,
    pub hold_tail_is_above_wavy_parts: bool,
    pub start_drawing_hold_body_offset_from_head: f32,
    pub stop_drawing_hold_body_offset_from_tail: f32,
    pub hold_let_go_gray_percent: f32,
    pub flip_head_and_tail_when_reverse: bool,
    pub flip_hold_body_when_reverse: bool,
    pub top_hold_anchor_when_reverse: bool,
    pub hold_active_is_add_layer: bool,
    pub part_animation: [NotePartAnimation; NOTE_ANIM_PART_COUNT],
    pub part_texture_translate: [NotePartTextureTranslate; NOTE_ANIM_PART_COUNT],
}

#[derive(Debug, Clone, Copy)]
pub struct NotePartAnimation {
    pub length: f32,
    pub vivid: bool,
}

impl Default for NotePartAnimation {
    fn default() -> Self {
        Self {
            length: 1.0,
            vivid: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteColorType {
    Denominator,
    Progress,
    ProgressAlternate,
}

impl NoteColorType {
    pub fn from_metric(value: &str) -> Option<Self> {
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if value.eq_ignore_ascii_case("Denominator") {
            Some(Self::Denominator)
        } else if value.eq_ignore_ascii_case("Progress") {
            Some(Self::Progress)
        } else if value.eq_ignore_ascii_case("ProgressAlternate") {
            Some(Self::ProgressAlternate)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NotePartTextureTranslate {
    pub addition_offset: [f32; 2],
    pub note_color_spacing: [f32; 2],
    pub note_color_count: i32,
    pub note_color_type: NoteColorType,
}

impl Default for NotePartTextureTranslate {
    fn default() -> Self {
        Self {
            addition_offset: [0.0, 0.0],
            note_color_spacing: [0.0, 0.0],
            note_color_count: 8,
            note_color_type: NoteColorType::Denominator,
        }
    }
}

impl Default for NoteDisplayMetrics {
    fn default() -> Self {
        Self {
            draw_hold_head_for_taps_on_same_row: true,
            draw_roll_head_for_taps_on_same_row: true,
            tap_hold_roll_on_row_means_hold: true,
            hold_head_is_above_wavy_parts: true,
            hold_tail_is_above_wavy_parts: true,
            start_drawing_hold_body_offset_from_head: 0.0,
            stop_drawing_hold_body_offset_from_tail: 0.0,
            hold_let_go_gray_percent: 0.25,
            flip_head_and_tail_when_reverse: false,
            flip_hold_body_when_reverse: false,
            top_hold_anchor_when_reverse: false,
            hold_active_is_add_layer: false,
            part_animation: [NotePartAnimation::default(); NOTE_ANIM_PART_COUNT],
            part_texture_translate: [NotePartTextureTranslate::default(); NOTE_ANIM_PART_COUNT],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn itg_column_xs_center_columns_on_64px_spacing() {
        assert_eq!(itg_column_xs(0), Vec::<i32>::new());
        assert_eq!(itg_column_xs(4), vec![-96, -32, 32, 96]);
        assert_eq!(
            itg_column_xs(8),
            vec![-224, -160, -96, -32, 32, 96, 160, 224]
        );
    }
}
